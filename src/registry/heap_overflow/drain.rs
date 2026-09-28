use core::sync::atomic::Ordering;

use super::{HeapOverflow, DRAIN_OWNER_FREE, DRAIN_OWNER_HELD, ENTRY_EMPTY_BASE, INLINE_CAP};

/// R2-13: RAII guard for one exclusive [`HeapOverflow`] drain pass — the
/// `HeapOverflow` analogue of `RemoteFreeRing`'s `DrainHeadPublish` guard
/// (F-7, R34-17/task #536, `src/alloc_core/segment/remote_free_ring/`),
/// reused in shape and spirit rather than reinvented. It is the SOLE writer
/// of the ring's `head` on the drain path (the pre-R2-13 explicit
/// `head.store(h, Release)` after the drain loop is replaced by this
/// `Drop`, so there is exactly one publish whether the drain completes
/// normally or unwinds), and it additionally releases the ring's
/// `drain_owner` exclusive-consumer token that `HeapOverflow::begin_drain`
/// acquired.
///
/// The two `Drop` stores are ORDERED: `head` is published (Release) BEFORE
/// the token is released (Release), so the next consumer's Acquire success
/// on the token CAS happens-after the head publication even when the two
/// drains run on different threads — a cross-thread token hand-off is
/// ordered exactly like a same-thread re-drain.
///
/// `h` starts at the `head` value observed at token-acquisition time and is
/// updated inside the drain loop after each successful
/// reclaim → clear → advance, so on the unwind path only offsets FULLY
/// processed are committed — the panicking iteration's offset is NOT
/// advanced past, matching the sibling guard's invariant that `head` only
/// ever marks fully-drained slots. The ONE entry whose `reclaim` panicked
/// is therefore left queued at the published `head` and re-passed to
/// `reclaim` by the next drain (an explicit at-least-once choice — see
/// `try_drain`'s doc comment for the full contract).
pub(super) struct DrainGuard<'a> {
    ring: &'a HeapOverflow,
    pub(super) h: usize,
}

impl Drop for DrainGuard<'_> {
    fn drop(&mut self) {
        // Publish the new head so producers' full-check sees the freed
        // space. Release: pairs with their Acquire head load in
        // `push_impl`'s full-check (and with the next consumer's token
        // Acquire — see the store below).
        self.ring.head.store(self.h, Ordering::Release);
        // THEN release the token. Release: pairs with the next
        // `begin_drain`'s Acquire CAS, handing it the head publication
        // above.
        self.ring
            .drain_owner
            .store(DRAIN_OWNER_FREE, Ordering::Release);
    }
}

impl HeapOverflow {
    /// PERF-PASS-4 (G9/C2)-style pre-drain empty-guard: a `Relaxed`
    /// load of `tail`, compared against a CALLER-cached `usize`, plus a
    /// `Relaxed` check of the intrusive spill head (see
    /// [`HeapCore::overflow_tail_cache`](crate::registry::heap_core::HeapCore) — the
    /// analogue of `RemoteFreeRing::is_likely_empty`'s documented "caller
    /// already holds its own owner-private cached copy of `head`" shape,
    /// adapted here to cache `tail` instead since `HeapOverflow::try_drain` (like
    /// `RemoteFreeRing::drain`) is the sole writer of `head` — so the OWNER
    /// is also the only party who can usefully cache `head`'s progress, while
    /// `tail` is the field a REMOTE push moves and the one whose value
    /// changing makes a ring drain necessary; a non-null spill head also
    /// requires a drain pass).
    ///
    /// **Why `Relaxed` is sound (mirrors `RemoteFreeRing::is_likely_empty`'s
    /// own argument, restated for `tail` here):** `tail` is monotonic (only
    /// ever `wrapping_add(1)`-ed by a winning producer CAS), so a `Relaxed`
    /// read of it can only be STALE (an older value than the true current
    /// one) or exact — never a value that HIDES a genuine push. If the
    /// observed `tail` equals the cache, no push has landed since the cache
    /// was taken (the cache came from a real prior `tail` read, and `tail`
    /// cannot un-advance), so skipping the full drain is safe — a push that
    /// races concurrently with this check is caught by the NEXT
    /// opportunistic drain call, the same "later drain picks it up" liveness
    /// contract every lazy-drain path in this allocator already relies on.
    /// A spill swap races this guard in the same way: an older observed null
    /// can defer the note once, never permanently hide it after a joined
    /// producer or a subsequent owner acquire.
    #[inline(always)]
    pub(crate) fn is_likely_empty(&self, cached_tail: usize) -> bool {
        self.tail.load(Ordering::Relaxed) == cached_tail
            && self.spill_head.load(Ordering::Relaxed).is_null()
    }

    #[cfg(feature = "internals")]
    pub(crate) fn cursors_for_test(&self) -> (usize, usize) {
        (
            self.head.load(Ordering::Acquire),
            self.tail.load(Ordering::Acquire),
        )
    }

    /// R6-REGRESSION-2 (progress-detection stop condition in
    /// `HeapCore::push_with_overflow_retry`): this ring's current DRAIN
    /// cursor (`head`) as a single `Relaxed` load — the `HeapOverflow`
    /// analogue of `RemoteFreeRing::head_relaxed` (see that method's doc
    /// comment for the full rationale; restated briefly here for this ring's
    /// own field conventions).
    ///
    /// `head` is advanced ONLY by the exclusive consumer's
    /// [`try_drain`](Self::try_drain)
    /// (one `Release` store per drain pass, issued by the guard's `Drop` in
    /// `try_drain`) — producers never
    /// write it — so a producer stuck in the bounded retry loop can compare
    /// two loads of it taken one probe round apart as an exact "did the owner
    /// drain anything from this ring in that window" signal. `Relaxed` is
    /// sound by the same monotonicity argument [`is_likely_empty`](
    /// Self::is_likely_empty) documents for `tail`: a monotonic cursor read
    /// `Relaxed` can be stale (under-reporting progress by at most one probe
    /// round — benign) but can never fabricate an advance that did not
    /// happen; and no payload is ever read through this value (the retry
    /// loop's actual push attempt, [`push_uncounted`](Self::push_uncounted),
    /// re-establishes ordering via its own `Acquire` head load).
    #[inline(always)]
    pub(crate) fn head_relaxed(&self) -> usize {
        self.head.load(Ordering::Relaxed)
    }

    /// R2-13: acquire the exclusive-consumer token for one drain pass — a
    /// `drain_owner` CAS `DRAIN_OWNER_FREE → DRAIN_OWNER_HELD`. The
    /// success ordering is Acquire: it pairs with the previous holder's
    /// Release stores in `DrainGuard`'s `Drop` (head publication, then
    /// token release), so the winner is ordered after the previous pass's
    /// head publication even across a cross-thread hand-off. The failure
    /// ordering is Relaxed: a loser reads nothing the holder owns. Returns
    /// the RAII `DrainGuard` on success, `None` if the token is already
    /// held.
    pub(super) fn begin_drain(&self) -> Option<DrainGuard<'_>> {
        match self.drain_owner.compare_exchange(
            DRAIN_OWNER_FREE,
            DRAIN_OWNER_HELD,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => Some(DrainGuard { ring: self, h: 0 }),
            Err(_) => None,
        }
    }

    /// Drain every published entry, invoking `reclaim(base, packed)` for
    /// each, and return the ACTUAL drain stop position (the final `head`
    /// value published by this call — the cursor the next drain resumes
    /// from), or `None` if another consumer already holds this ring's
    /// exclusive-drain token.
    ///
    /// ## R2-13 — this is the ONLY drain entry point, and it is exclusive
    ///
    /// Before R2-13 this method was a generic `drain(&self, ...)` whose
    /// single-consumer requirement (the same discipline
    /// `RemoteFreeRing::drain` documents) was enforced by NOTHING: any
    /// number of concurrent or reentrant callers compiled and ran, and a
    /// second caller racing the first would re-process entries whose slots
    /// the first had already cleared but not yet published (`head` is only
    /// committed at the end of a pass), or observe a torn cursor.
    /// `try_drain` closes that off: it first acquires the ring's
    /// `drain_owner` token, and on a busy token returns `None` WITHOUT
    /// touching either cursor, any slot, or running any callback. The
    /// winner drains exclusively; every loser sees a benign "busy" it must
    /// treat as "nothing was drained, retry later" (the production caller
    /// leaves its `overflow_tail_cache` untouched on `None`, so the next
    /// opportunistic call re-drains whatever landed meanwhile).
    ///
    /// The token is held by an RAII guard whose `Drop` (a) publishes `head`
    /// and (b) releases the token — in that order, so a consumer that
    /// acquires the token later (possibly on another thread) is ordered
    /// after the head publication by the guard's Release stores pairing
    /// with the token CAS's Acquire. This also STRENGTHENS the soundness of
    /// this method's `Relaxed` `head` load below: unlike a bare
    /// ownership-transfer argument, EVERY drain's first act is the token
    /// CAS, so the previous consumer's head publication
    /// happens-before every subsequent consumer's reads, regardless of
    /// which thread each ran on.
    ///
    /// Called ONLY by the owning thread in production (single consumer —
    /// now enforced by the token, not assumed), on the SAME schedule the
    /// owner already drains its segments' own rings (see
    /// `HeapCore::drain_heap_overflow`'s call sites). Stops at the first
    /// reserved-but-not-yet-published slot (a producer won the tail CAS but
    /// has not stored `base` yet) — order is preserved by the cursors, a
    /// later drain picks it up, mirroring `RemoteFreeRing::drain` exactly.
    ///
    /// ## Unwind contract if `reclaim` panics (mirrors `RemoteFreeRing`'s
    /// `DrainHeadPublish`, restated for this ring)
    ///
    /// `reclaim` has NO no-panic contract, and the guard's publish-on-drop
    /// makes an unwind non-catastrophic:
    ///
    /// - Every entry whose `reclaim` returned normally, whose slot was
    ///   cleared, and whose cursor advance happened BEFORE the panic is
    ///   PUBLISHED by the guard's unwind drop: the next drain does not
    ///   re-deliver it, and producers' full-check sees the freed space.
    ///   (The pre-R2-13 code published `head` only after the whole loop, so
    ///   a later `reclaim` panic left those cleared-but-unpublished slots
    ///   permanently in front of the stale `head` — the next drain saw
    ///   `ENTRY_EMPTY_BASE` there and stopped immediately, jamming the ring
    ///   and stranding every remaining reclaim note.)
    /// - The ONE entry whose `reclaim` panicked is RETRIED, not skipped or
    ///   dropped: the loop body calls `reclaim` BEFORE clearing the slot
    ///   and BEFORE advancing the guard's cursor, so on unwind that slot
    ///   still holds its `(base, packed)` pair at the published `head`, and
    ///   the next drain re-passes it to `reclaim` — an explicit
    ///   AT-LEAST-ONCE choice for that element. A caller that unwinds
    ///   through (and then catches) a drain accepts that `reclaim` may run
    ///   twice for the in-flight entry, so its action must tolerate a
    ///   repeat. The production consumers
    ///   (`AllocCore::reclaim_offset`/`reclaim_offset_checked`) are
    ///   defensively re-entrant against exactly that: a block whose free
    ///   already landed is rejected by the reclaim primitives' own
    ///   is_free/magic/bounds guards on the retry. Achieving true
    ///   exactly-once-under-unwind would need a two-phase/idempotent
    ///   reclaim protocol, which no ring in this codebase has (see
    ///   `RemoteFreeRing`'s `DrainHeadPublish` doc comment for the same
    ///   residual, tracked as `docs/CORRECTNESS_OPEN_ITEMS.md` item 22).
    /// - The token is released by the same unwind drop, so the ring stays
    ///   usable after a caught panic (a leaked token would jam it forever).
    ///
    /// Transparently spans both tiers: `slot(h)` resolves each index to the
    /// inline array or the sidecar exactly as `push` does. A `h` in the
    /// sidecar range is only ever reached here after some producer
    /// successfully published into it (which, by `push`'s wedge-hazard fix,
    /// only happens after that producer's `ensure_sidecar` call already
    /// materialised it) — so the sidecar is always ready by the time a
    /// drain needs to read from it; no `ensure_sidecar` call is needed on
    /// this side.
    ///
    /// The returned `Some(h)` is the ACTUAL drain stop position — the final
    /// `head` value written by this call (the guard publishes it in its
    /// `Drop`), NOT the entry `tail` snapshot. This is load-bearing when
    /// the drain stopped early at a reserved-but-not-yet-published slot
    /// (`h < t` at the break): returning the entry-time `tail` there (the
    /// R2-4 bug) would make the caller's
    /// [`is_likely_empty`](Self::is_likely_empty) cache equal the
    /// still-current `tail`, so every subsequent guard check would WRONGLY
    /// skip the re-drain that must observe the slot once its producer
    /// finishes publishing — a pending cross-heap free gets stuck until an
    /// unrelated later push incidentally moves `tail`. Returning `h` keeps
    /// the cache strictly below `tail` while any reservation remains
    /// unpublished, so the guard keeps re-draining until the publish lands
    /// — mirroring `RemoteFreeRing::drain`'s own return-the-final-`head`
    /// contract (PERF-PASS-4 G9/C2). A busy `None` returns without caching
    /// anything, leaving the caller's cache exactly as it was.
    ///
    /// `pub` (doc-hidden, not stable API) — see [`push`](Self::push)'s doc
    /// comment for why.
    #[doc(hidden)]
    pub fn try_drain<F: FnMut(*mut u8, u32)>(&self, mut reclaim: F) -> Option<usize> {
        // R2-13: acquire the exclusive-consumer token FIRST — before either
        // cursor is read. `None` = another consumer holds it; nothing is
        // loaded, cleared, or invoked on this path.
        let mut guard = self.begin_drain()?;
        // Acquire: see every producer's Release reservation (tail CAS) and
        // their Release publish (base store).
        let t = self.tail.load(Ordering::Acquire);
        // Relaxed is sound here: `head` has exactly one writer at a time —
        // the current token holder — and this thread IS the holder (the
        // CAS above). The previous holder's Release publication of `head`
        // in `DrainGuard`'s `Drop` happens-before this load through the
        // token's own Release store + this CAS's Acquire, so even a
        // cross-thread token hand-off observes the published value;
        // producers never write `head`.
        let mut h = self.head.load(Ordering::Relaxed);
        guard.h = h;
        while h != t {
            let (base_slot, packed_slot) = self.slot(h);
            // Acquire: pairs with the producer's Release store of `base` —
            // seeing a non-empty `base` here also makes the producer's
            // Relaxed `packed` store (issued-before, program-order, on the
            // SAME producer thread, and Released by the `base` store that
            // follows it) visible per the Release sequence rule.
            let base_addr = base_slot.load(Ordering::Acquire);
            if base_addr == ENTRY_EMPTY_BASE {
                // Reserved but not yet published — stop; a later drain will
                // see it (identical reasoning to `RemoteFreeRing::drain`).
                break;
            }
            let packed = packed_slot.load(Ordering::Relaxed);
            reclaim(base_addr, packed);
            // Clear for the next wrap. Relaxed: the next producer to reserve
            // this slot will Release-store `base` again; our drain reads
            // Acquire.
            base_slot.store(ENTRY_EMPTY_BASE, Ordering::Relaxed);
            h = h.wrapping_add(1);
            // R2-13: the guard, not this function's tail, is the sole
            // publisher of `head`. Track the most-recently-fully-processed
            // position (reclaimed + cleared + advanced past) so an unwind
            // out of `reclaim` publishes ONLY real progress — the panicking
            // iteration's slot is neither cleared nor advanced past, and is
            // re-delivered by the next drain (see this method's unwind
            // contract above).
            guard.h = h;
        }
        // R2-4: return the ACTUAL stop position `h` (the value the guard is
        // about to publish to `self.head` in its `Drop`), NOT the
        // entry-time `tail` snapshot `t`. Returning `t` here — as the
        // pre-R2-4 code did — caches a value equal to the still-current
        // `tail` when the drain stopped at an unpublished slot, and
        // `is_likely_empty` then skips every subsequent re-drain, sticking
        // the pending free. See the method doc above for the full argument.
        Some(h)
    }

    /// **Test surface** (`#[doc(hidden)] pub`): advance `tail` by exactly one
    /// reservation WITHOUT publishing the slot's `(base, packed)` pair —
    /// faithfully reproducing the window between a winning producer's tail CAS
    /// and its subsequent `base` publish store, during which a concurrent
    /// `drain` observes the slot as reserved-but-not-yet-published and must
    /// stop. Lets `tests/heap_overflow_drain_return.rs` exercise the R2-4
    /// interleaving (a `drain` that stops at this gap) DETERMINISTICALLY on a
    /// single thread, without relying on thread scheduling — the real `push`
    /// completes both halves before returning, so the half-published state is
    /// otherwise unreachable from the public API.
    ///
    /// MUST be called on a quiescent ring (no concurrent `push`/`drain`) —
    /// there is no CAS (a plain store suffices under the single-writer test
    /// discipline), and no full-check (the test controls occupancy), and no
    /// sidecar-materialisation attempt (the reserved index is caller-chosen
    /// and must stay within `INLINE_CAP` for this hook — see the boundary
    /// check below). Leaves the reserved slot's `base` at
    /// [`ENTRY_EMPTY_BASE`], so a subsequent `drain` stops there exactly as
    /// it would against a real racing producer.
    ///
    /// R2-07 (independent src review round 2, task #2009): `&mut self`, not
    /// `&self` (same rationale as
    /// [`dbg_rollback_sidecar_sentinel_for_test`](Self::dbg_rollback_sidecar_sentinel_for_test)'s
    /// doc comment — obtainable only through a genuinely, exclusively owned
    /// standalone ring, never a shared production one), and the `t <
    /// INLINE_CAP` bound is now a real `assert!` that fires in EVERY build
    /// profile, not a `debug_assert!` that release builds compile out. The
    /// pre-fix `debug_assert!` let a release build (`internals` alone, no
    /// `debug_assertions`) advance `tail` past `INLINE_CAP` with the sidecar
    /// left unmaterialised; the next `drain`/`push` call on that index would
    /// reach [`slot`](Self::slot)'s sidecar branch with `self.sidecar` still
    /// `null`, and `slot`'s OWN precondition check was — until this same
    /// fix, see that method's doc comment — also only a `debug_assert!`,
    /// so the release build would dereference a null pointer through
    /// [`bootstrap::deref_overflow_sidecar`](crate::registry::bootstrap::deref_overflow_sidecar).
    /// Rejecting the call outright in all profiles closes that path at its
    /// one real origin (this hook is the only caller that can ever violate
    /// `slot`'s precondition — see `slot`'s doc comment for the full call-site
    /// audit) rather than only detecting it after the fact.
    ///
    /// # Panics
    ///
    /// Panics if the ring's current `tail` already sits at or past
    /// `INLINE_CAP` — this hook supports reserving only within the
    /// always-inline tier; the sidecar tier needs a real `ensure_sidecar`
    /// call to back an unpublished reservation soundly.
    #[doc(hidden)]
    pub fn dbg_reserve_unpublished_for_test(&mut self) {
        let t = self.tail.load(Ordering::Relaxed);
        assert!(
            t < INLINE_CAP,
            "dbg_reserve_unpublished_for_test: only supports reserving within the \
             always-inline tier (0..INLINE_CAP); the sidecar tier needs a real \
             ensure_sidecar call to back an unpublished reservation soundly"
        );
        self.tail.store(t.wrapping_add(1), Ordering::Relaxed);
        // Intentionally do NOT write `bases[idx]`/`packed[idx]`: the slot stays
        // at its initial `ENTRY_EMPTY_BASE`, which is exactly `drain`'s
        // publish-gate sentinel.
    }

    /// **Test surface** (`#[doc(hidden)] pub`): drive `tail` directly to
    /// `INLINE_CAP` by pushing `INLINE_CAP` synthetic entries and draining
    /// them, leaving the ring logically empty (`head == tail == INLINE_CAP`)
    /// but positioned exactly at the inline/sidecar boundary — the state a
    /// test needs to then push ONE more entry and observe sidecar
    /// materialisation without needing to push `INLINE_CAP + 1` real entries
    /// through the whole inline range every time. Returns the number of
    /// entries pushed (always `INLINE_CAP`, for the caller's own bookkeeping).
    ///
    /// Uses ordinary `push`, so this exercises the same code path a real
    /// producer would (no special-casing) — it exists only to avoid every
    /// sidecar test repeating the same `INLINE_CAP`-iteration setup loop.
    #[doc(hidden)]
    pub fn dbg_fill_and_drain_inline_tier_for_test(&self) -> usize {
        for i in 0..INLINE_CAP {
            let base = core::ptr::without_provenance_mut::<u8>((i + 1) * 64);
            assert!(
                self.push(base, i as u32),
                "dbg_fill_and_drain_inline_tier_for_test: inline-tier push must not fail"
            );
        }
        let mut drained = 0usize;
        self.try_drain(|_, _| drained += 1)
            .expect("dbg_fill_and_drain_inline_tier_for_test: drain must not be busy on a quiescent test ring");
        debug_assert_eq!(drained, INLINE_CAP);
        INLINE_CAP
    }
}
