// Only reached by the ring's atomic push/drain methods, which are themselves
// only reachable on builds that exercise cross-thread free
// (`alloc-xthread`); unused under `--features alloc-core` alone.
#[cfg_attr(not(feature = "alloc-xthread"), allow(unused_imports))]
use core::sync::atomic::Ordering;

use crate::alloc_core::node::Node;

// `allow(unused_imports)`: every method that consumes these is
// `alloc-xthread`-gated (the ring's only compiled surface under plain
// `alloc-core` is the always-compiled struct, whose `FOOTPRINT` the segment
// `Layout` references), so the imports would warn in non-xthread configs.
// Same cfg_attr discipline as the `Ordering` import above.
#[cfg_attr(not(feature = "alloc-xthread"), allow(unused_imports))]
use super::{
    PushOverflow, RemoteFreeRing, CACHED_HEAD_OFF, DBG_RING_OVERFLOW, HEAD_OFF, OVERFLOW_OFF,
    RING_CAP, RING_SLOT_EMPTY, SLOTS_OFF, TAIL_OFF,
};
// The shadow-oracle statics themselves are `bench-internals`-gated.
#[cfg(feature = "bench-internals")]
use super::{DBG_RING_PUSH_SHADOW_FAST, DBG_RING_PUSH_SHADOW_SLOW};
// The guard type itself is `alloc-xthread`-gated, so its import must be too.
#[cfg(feature = "alloc-xthread")]
use super::DrainHeadPublish;

#[cfg(feature = "alloc-xthread")]
impl Drop for DrainHeadPublish {
    fn drop(&mut self) {
        // Publish the new head so producers' full-check sees the freed space.
        // Release: pairs with their Acquire head load in `push`/`full_check`.
        // `self.h` is the most-recently-advanced value (updated inside the loop
        // after each successful reclaim+clear), so on the unwind path only the
        // offsets that were FULLY processed (reclaimed + cleared + advanced) are
        // committed — the panicking iteration's offset is NOT advanced past,
        // matching the pre-F-7 invariant that `head` only marks fully-drained
        // slots.
        self.head.store(self.h, Ordering::Release);
    }
}

impl RemoteFreeRing {
    /// Construct the view over ring metadata at `base + off`. The caller (the
    /// bootstrap / `SegmentMeta::remote_ring`) guarantees the byte range
    /// `[base + off, base + off + FOOTPRINT)` is carved, 4-byte-aligned, and
    /// inside a live segment.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn at(base: *mut u8, off: usize) -> Self {
        Self {
            base: Node::offset(base, off),
        }
    }

    /// **Test surface** (`#[doc(hidden)] pub`): construct a ring view over an
    /// arbitrary aligned byte buffer at offset 0. Used ONLY by the isolated
    /// ring unit test (`tests/remote_ring_unit.rs`), which builds a ring over a
    /// plain `Box<[u8]>` (NOT a segment, NOT an allocator) to prove the ring's
    /// MPSC correctness in isolation from the allocator / ABA concerns.
    ///
    /// Production code MUST use [`at`](Self::at) with a segment-relative offset
    /// from [`Layout::remote_ring_off`](crate::alloc_core::segment_header::Layout::remote_ring_off).
    ///
    /// R2-3: the null + 4-byte-alignment preconditions are checked by a
    /// RELEASE-surviving `assert!` (not `debug_assert!`), so a null/misaligned
    /// base panics in every build.
    ///
    /// # Safety
    ///
    /// The caller MUST guarantee that `base` points to at least `FOOTPRINT`
    /// writable, 4-byte-aligned bytes that are exclusively owned by the caller
    /// and live for the ring's use (e.g. an `alloc::vec![0u8; FOOTPRINT]` boxed
    /// slice). The `FOOTPRINT`-writability / liveness / exclusivity half of the
    /// contract cannot be checked at runtime — the only documented use is an
    /// owned boxed buffer. Passing a too-short, dangling, shared, or
    /// non-`FOOTPRINT`-valid base is undefined behaviour.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary — the
                          // validity/size/alignment/lifetime/exclusivity of the caller-supplied
                          // pointer is unverifiable by the callee, so the contract MUST live in the
                          // signature, not in prose. The body is safe (delegates to `Self::at`).
    pub unsafe fn over_test_buffer(base: *mut u8) -> Self {
        assert!(
            !base.is_null() && (base as usize).is_multiple_of(4),
            "over_test_buffer: base must be non-null and 4-byte-aligned (R2-3 release guard)"
        );
        Self::at(base, 0)
    }

    /// **Test surface**: initialise a fresh ring at `base` (offset 0). Same as
    /// [`init_in_place`](Self::init_in_place) but for a standalone buffer (no
    /// segment-relative offset). See [`over_test_buffer`](Self::over_test_buffer).
    ///
    /// R2-3: carries the same release-surviving null + 4-byte-alignment `assert!`
    /// as [`over_test_buffer`](Self::over_test_buffer).
    ///
    /// # Safety
    ///
    /// Same contract as [`over_test_buffer`](Self::over_test_buffer#safety):
    /// `base` MUST point to at least `FOOTPRINT` writable, 4-byte-aligned,
    /// exclusively-owned bytes that are live for the ring's use. The callee
    /// writes cursors and all slots starting at `base`, so a too-short, dangling
    /// or shared buffer is undefined behaviour.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn init_test_buffer(base: *mut u8) {
        assert!(
            !base.is_null() && (base as usize).is_multiple_of(4),
            "init_test_buffer: base must be non-null and 4-byte-aligned (R2-3 release guard)"
        );
        Self::init_in_place(base, 0)
    }

    /// **Test surface**: the overflow counter's current value (diagnostic). Used
    /// by the isolated ring test to assert `reclaimed + overflowed == pushed`.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn overflow_count(&self) -> u32 {
        self.overflow().load(Ordering::Acquire)
    }

    /// **Test surface** (task: long-run u32 wrap): preset the `head` and `tail`
    /// cursors directly so a test can drive the ring across the `u32::MAX → 0`
    /// boundary without first pushing 2^32 entries. Writes the atomics with
    /// `Release` (mirrors the production drain's `head` publish / push's `tail`
    /// reservation visibility) so a subsequently spawned producer/consumer sees
    /// the preset. MUST be called on a quiescent ring (no concurrent push/drain)
    /// and MUST leave `tail.wrapping_sub(head) <= RING_CAP` (the ring's full
    /// invariant) — the caller is responsible for a consistent preset.
    ///
    /// F10 (task #502): also resets `cached_head` to the new `head` value.
    /// Without this, a preset that MOVES `head` (e.g. from its `init_in_place`
    /// zero to a wrap-boundary value) would leave a STALE `cached_head` behind
    /// — harmless by the shadow's own soundness argument (a stale-low shadow
    /// only ever forces the conservative slow path, never an unsound fast-path
    /// accept — see the module doc), but needlessly forces every subsequent
    /// push in the test to pay the slow path, which is not representative of
    /// what a real preset-then-drive scenario should measure. Resetting here
    /// keeps `dbg_set_cursors` an honest "quiescent ring, consistent state"
    /// preset rather than relying on the shadow's stale-low safety margin to
    /// paper over an inconsistency this seam itself introduced.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn dbg_set_cursors(&self, head: u32, tail: u32) {
        self.head().store(head, Ordering::Release);
        self.tail().store(tail, Ordering::Release);
        self.cached_head().store(head, Ordering::Relaxed);
    }

    /// F10 (task #502) **test surface**: advance ONLY the real `head` cursor
    /// (`Release`, mirroring the production drain's own store), deliberately
    /// NOT touching `cached_head` — the inverse of `dbg_set_cursors`'s
    /// consistency-preserving reset. Lets a test simulate "the owner drained
    /// but no producer has refreshed its shadow yet", i.e. deliberately
    /// STALE the shadow relative to the real head, to drive the shadow's
    /// slow path on demand and prove it still re-derives correctly (see
    /// `tests/remote_ring_shadow_head.rs`'s adversarial-regime path-
    /// activation coverage). MUST be called on a quiescent ring (no
    /// concurrent push/drain), same precondition as `dbg_set_cursors`,
    /// and MUST NOT regress `head` below its current value — storing a
    /// value lower than the current `head` would leave `cached_head`
    /// above the regressed `head` (a STALE-HIGH shadow), which the module
    /// doc's F10 monotonicity argument declares impossible and which
    /// could let the fast path admit a push into a full ring. The hook's
    /// only real caller (`tests/remote_ring_shadow_head.rs`) uses
    /// `wrapping_add(1)` — an advance, never a regression.
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn dbg_advance_head_only(&self, head: u32) {
        self.head().store(head, Ordering::Release);
    }

    /// **Test surface** (task: long-run u32 wrap): read the current `(head,
    /// tail)` cursor pair. Lets a test assert occupancy (`tail.wrapping_sub(
    /// head)`) across the wrap. `Acquire` loads (uniform with the drain/push).
    #[cfg(feature = "alloc-xthread")]
    #[doc(hidden)]
    pub fn dbg_cursors(&self) -> (u32, u32) {
        (
            self.head().load(Ordering::Acquire),
            self.tail().load(Ordering::Acquire),
        )
    }

    /// Initialise a fresh ring at `base + off`: zero the cursors and mark every
    /// slot `RING_SLOT_EMPTY`. Called by the bootstrap when a small/primordial
    /// segment is reserved. The segment is exclusively owned at init time
    /// (single-writer), so plain writes suffice — no atomics needed here.
    ///
    /// `base + off` MUST point to `FOOTPRINT` writable bytes.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn init_in_place(base: *mut u8, off: usize) {
        let ring = Self::at(base, off);
        // Cursors: zero (empty ring). Plain writes — bootstrap is single-writer.
        Node::write_u32(Node::offset(ring.base, HEAD_OFF) as *mut u32, 0);
        Node::write_u32(Node::offset(ring.base, TAIL_OFF) as *mut u32, 0);
        Node::write_u32(Node::offset(ring.base, OVERFLOW_OFF) as *mut u32, 0);
        // F10: cached_head starts at 0, matching the real head's initial value
        // (the shadow's own invariant — it only ever holds a value that was
        // once really `head` — holds trivially at init since both start at 0).
        Node::write_u32(Node::offset(ring.base, CACHED_HEAD_OFF) as *mut u32, 0);
        // Every slot empty.
        for i in 0..RING_CAP {
            let slot =
                Node::offset(ring.base, SLOTS_OFF + i * core::mem::size_of::<u32>()) as *mut u32;
            Node::write_u32(slot, RING_SLOT_EMPTY);
        }
    }

    /// The `&AtomicU32` head cursor (consumer drain position).
    #[cfg(feature = "alloc-xthread")]
    fn head(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, HEAD_OFF)
    }
    /// The `&AtomicU32` tail cursor (producer reserve position).
    #[cfg(feature = "alloc-xthread")]
    fn tail(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, TAIL_OFF)
    }
    /// The `&AtomicU32` overflow counter (diagnostic; number of discarded
    /// pushes due to a full ring).
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    fn overflow(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, OVERFLOW_OFF)
    }
    /// F10 (task #502): the `&AtomicU32` producer-line shadow replica of
    /// `head`. Same cache line as `tail`/`overflow` — reading it costs no
    /// cross-core coherence traffic beyond what `push`'s own `tail` load
    /// already pays. See the module doc's "F10 — shadow/cached head" section
    /// for the full soundness argument for why a stale value here is always
    /// safe.
    #[cfg(feature = "alloc-xthread")]
    fn cached_head(&self) -> &'static core::sync::atomic::AtomicU32 {
        Node::atomic_u32_at(self.base, CACHED_HEAD_OFF)
    }
    /// The `&AtomicU32` slot at reservation index `i` (`i % RING_CAP`).
    #[cfg(feature = "alloc-xthread")]
    fn slot(&self, i: usize) -> &'static core::sync::atomic::AtomicU32 {
        let idx = i % RING_CAP;
        Node::atomic_u32_at(self.base, SLOTS_OFF + idx * core::mem::size_of::<u32>())
    }

    /// F10 (task #502): the shared full-check used by both [`push`](Self::push)
    /// and [`try_push_uncounted`](Self::try_push_uncounted). Returns `Ok(())`
    /// if reservation `t` is provably within capacity; `Err(())` if the ring
    /// is (really, `Acquire`-confirmed) full.
    ///
    /// **Fast path (shadow):** `ch = cached_head.load(Acquire)` — same
    /// producer cache line as `tail`, no cross-core traffic. If
    /// `t.wrapping_sub(ch) < RING_CAP`, the ring provably has room (the
    /// module doc's "F10" soundness section proves `cached_head <= head`
    /// always, so this can only UNDER-estimate available room, never
    /// over-estimate it) — return `Ok(())` immediately without touching the
    /// consumer's `head` line at all. The `Acquire` (R34-6, task #525,
    /// finding F-1) restores the happens-before edge the pre-F10
    /// `head.load(Acquire)` supplied: a producer whose slow path refreshed
    /// `cached_head` with a `Release` store (below) carries the consumer's
    /// `slot.store(EMPTY)` in its history, and THIS `Acquire` load
    /// synchronizes-with that store — so a later producer that wins the
    /// tail CAS into a recycled slot is guaranteed to observe the clear
    /// before it publishes. On x86-TSO this `Acquire` load compiles to the
    /// SAME `mov` as the old `Relaxed` (all x86 loads are acquire); the
    /// cost is fence *strength*, not a fence instruction.
    ///
    /// **Slow path (real check + shadow refresh):** only reached when the
    /// shadow suggests the ring MIGHT be full. Performs the exact pre-F10
    /// `head.load(Acquire)`, refreshes `cached_head` from it (`Release` —
    /// the refresh now carries the synchronisation edge that the fast
    /// path's `Acquire` load pairs with; see the ordering note above),
    /// and re-checks against the REAL value before returning `Err(())`.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    fn full_check(&self, t: u32) -> Result<(), ()> {
        // R34-6 (task #525, finding F-1): Acquire — restores the happens-
        // before edge that the pre-F10 `head.load(Acquire)` supplied (see
        // the module doc's F10 ordering supplement). On x86-TSO this is a
        // plain `mov` (identical to the old `Relaxed`); on aarch64 it is
        // one `ldapr` instead of `ldr`.
        let ch = self.cached_head().load(Ordering::Acquire);
        if t.wrapping_sub(ch) < RING_CAP as u32 {
            // Shadow proves room exists (stale-low cached_head only makes
            // this branch LESS likely to fire, never falsely fire — see the
            // module doc soundness section). Skip the real Acquire load.
            #[cfg(feature = "bench-internals")]
            DBG_RING_PUSH_SHADOW_FAST.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        // Shadow suggests full (or has never been refreshed since init, both
        // starting at 0): fall through to the real, Acquire-ordered check —
        // byte-identical to the pre-F10 protocol on this branch.
        #[cfg(feature = "bench-internals")]
        DBG_RING_PUSH_SHADOW_SLOW.fetch_add(1, Ordering::Relaxed);
        let h = self.head().load(Ordering::Acquire);
        // R34-6 (task #525, finding F-1): Release — pairs with the fast
        // path's `Acquire` load so a later producer that reads this
        // refreshed value carries the consumer's `slot.store(EMPTY)` in
        // its happens-before past. On x86-TSO this is a plain `mov`
        // (identical to the old `Relaxed`); on aarch64 it is one `stlr`.
        self.cached_head().store(h, Ordering::Release);
        if t.wrapping_sub(h) >= RING_CAP as u32 {
            return Err(());
        }
        Ok(())
    }

    /// Push a freed block's segment-relative `offset` into the ring. Called by
    /// a NON-OWNER thread (a cross-thread freer). Returns `Err(PushOverflow)`
    /// if the ring is full — the caller MUST then discard the block (bounded
    /// leak, sound).
    ///
    /// `offset` MUST be `< SEGMENT` (a real block offset, not the sentinel).
    #[cfg(feature = "alloc-xthread")]
    pub fn push(&self, offset: u32) -> Result<(), PushOverflow> {
        debug_assert_ne!(offset, RING_SLOT_EMPTY, "offset must not be the sentinel");
        loop {
            let t = self.tail().load(Ordering::Relaxed);
            // F10: shadow-checked full-check (see `full_check`'s doc for the
            // fast/slow path split and the module doc for the soundness
            // argument). Semantically identical to the pre-F10
            // `t.wrapping_sub(head.load(Acquire)) >= RING_CAP` check.
            if self.full_check(t).is_err() {
                // Ring full: bounded leak. Count it (diagnostic, both the
                // per-segment cursor-block counter AND the process-wide D2
                // counter) and bail.
                let _ = self.overflow().fetch_add(1, Ordering::Relaxed);
                DBG_RING_OVERFLOW.fetch_add(1, Ordering::Relaxed);
                return Err(PushOverflow);
            }
            // Reserve slot `t`: CAS tail t → t+1. AcqRel on success — the
            // reservation is the linearization point; Acquire pairs with a
            // prior producer's Release publish (harmless here, but uniform with
            // the drain's view). Relaxed on failure: retry, no side-effect.
            match self.tail().compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Publish: write the offset into the reserved slot. Release
                    // so the consumer's Acquire slot load sees this write.
                    self.slot(t as usize).store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue, // Another producer reserved `t`; retry.
            }
        }
    }

    /// R6-OPT-P0-4: byte-identical push/CAS/publish protocol to [`push`](
    /// Self::push), EXCEPT the "ring full" branch does NOT bump either
    /// diagnostic counter (`self.overflow()` / [`DBG_RING_OVERFLOW`]).
    ///
    /// Exists ONLY for `HeapCore::push_with_overflow_retry`'s bounded
    /// spin-retry loop, which (under the R6-OPT-P0-4 "overflow-first"
    /// policy) is now reached only in the genuinely rare case where BOTH the
    /// segment ring's one counted attempt AND an immediate
    /// `push_to_heap_overflow` attempt have already failed — i.e. every
    /// failed poll inside that loop is a re-check of an already-known-full
    /// ring, not a new diagnostic event. Counting each of up to
    /// `RING_PUSH_RETRY_SPINS` (8,192) re-polls would tax the diagnostic
    /// counters with a locked RMW per poll for no informational gain: the ONE
    /// counted [`push`](Self::push) attempt the caller already made is the
    /// signal "this ring overflowed at all"; the retry loop's OWN outcome is
    /// separately, meaningfully counted by the caller via
    /// `DBG_RING_PUSH_RETRIED` (single bump, on eventual success) and
    /// `DBG_RING_PUSH_RETRY_EXHAUSTED` (single bump, if the whole budget is
    /// exhausted) — see that caller's doc comment for the full accounting.
    ///
    /// `offset` MUST be `< SEGMENT` (a real block offset, not the sentinel) —
    /// same contract as [`push`](Self::push).
    #[cfg(feature = "alloc-xthread")]
    pub fn try_push_uncounted(&self, offset: u32) -> Result<(), PushOverflow> {
        debug_assert_ne!(offset, RING_SLOT_EMPTY, "offset must not be the sentinel");
        loop {
            let t = self.tail().load(Ordering::Relaxed);
            // F10: identical shadow-checked full-check as `push` (see
            // `full_check`'s doc + the module doc's soundness section).
            if self.full_check(t).is_err() {
                // Ring full: bounded leak, SAME as `push` — but deliberately
                // uncounted (see doc comment above for why).
                return Err(PushOverflow);
            }
            // Reserve slot `t`: identical CAS/publish protocol to `push`.
            match self.tail().compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.slot(t as usize).store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue, // Another producer reserved `t`; retry.
            }
        }
    }

    /// Drain all published offsets from the ring, passing each to `reclaim`.
    /// Called ONLY by the owning thread (single consumer). `reclaim` receives
    /// the block's segment-relative offset; the caller turns it back into a
    /// pointer and routes it to the segment's `BinTable`.
    ///
    /// Stops at the first not-yet-published reserved slot (a producer won the
    /// reservation CAS but hasn't stored the offset yet) — order is preserved by
    /// the cursors, so a later drain picks it up.
    ///
    /// Returns the final `head` value written (i.e. the drain cursor after
    /// this call). PERF-PASS-4 (G9/C2, task #52): callers that maintain an
    /// owner-private cached copy of `head` (to skip future empty drains — see
    /// [`RemoteFreeRing::is_likely_empty`]) use this to refresh their cache
    /// without a second atomic load; callers that don't care simply ignore it
    /// (existing call sites are source- and behaviour-compatible).
    ///
    /// **Unwind contract if `reclaim` panics:** see [`DrainHeadPublish`]'s
    /// doc comment for the exact guarantee (no loss of already-fully-processed
    /// prior elements) and the exact non-guarantee (no exactly-once for the
    /// element `reclaim` was processing when it panicked — that offset may be
    /// re-passed to `reclaim` on a subsequent `drain` call after a
    /// `catch_unwind`). The loop body below calls `reclaim(off)` BEFORE
    /// clearing the slot and BEFORE advancing `h`, which is the reason the
    /// non-guarantee exists.
    #[cfg(feature = "alloc-xthread")]
    pub fn drain<F: FnMut(u32)>(&self, mut reclaim: F) -> u32 {
        // Acquire: see every producer's Release reservation (tail CAS) and
        // their Release publish (slot store).
        let t = self.tail().load(Ordering::Acquire);
        // Relaxed is sound here despite `head` being written (below) with a
        // Release store and read here without an Acquire: the ring has a SINGLE
        // consumer, but consumer IDENTITY moves with slot ownership. A ring
        // belongs to a segment; when that segment is recycled and re-claimed by
        // a new owner thread, the registry recycle→claim handshake is itself a
        // Release/Acquire pair that establishes happens-before between the
        // previous owner's LAST `head` Release store and the new owner's first
        // drain. So the new owner-consumer is guaranteed to observe the prior
        // owner's final `head` value; no per-load Acquire on `head` is needed
        // because there is never a concurrent writer to `head` — only a prior
        // one, already fenced by the ownership transfer (review B, Finding 4).
        let mut h = self.head().load(Ordering::Relaxed);
        // F-7 (R34-17/task #536): RAII-publish the drain cursor so a `reclaim`
        // closure that unwinds mid-drain still publishes the progress actually
        // made. WITHOUT this guard, a panic propagating out of `reclaim(off)`
        // would skip the `head.store(h, Release)` below entirely (it sits AFTER
        // the loop) — so the next `drain` re-reads the stale `head` and, since
        // the slots of any fully-processed offsets are now `EMPTY`, breaks
        // immediately at the first cleared slot, leaking every offset from the
        // panicking iteration onward (a stuck "false-empty" until the segment is
        // recycled and the ring reset). The guard publishes EXACTLY ONCE: on the
        // happy path its `Drop` runs at scope end; on the unwind path its `Drop`
        // runs during unwind — either way `h` holds the most-recently-advanced
        // value, so only real progress is published.
        let mut publish = DrainHeadPublish {
            head: self.head(),
            h,
        };
        // Wrap-correct drain: both cursors are monotonic wrapping counters
        // (incremented by `wrapping_add(1)`), so the undrained count is
        // `t.wrapping_sub(h)` — NOT `t - h`, which overflows on cursor wrap.
        // `while h < t` would silently stop draining once `tail` wraps past
        // `u32::MAX` while `head` has not, leaking every subsequent offset
        // (and, worse, a later drain could re-process a slot whose offset was
        // already reclaimed before the wrap if `head` were ever advanced past
        // `tail` — impossible while `head <= tail` by the full-check, but the
        // `<` comparison is still wrong and must be `!=`). The full-check in
        // `push` guarantees `t.wrapping_sub(h) < RING_CAP` at all times, so
        // `h == t` is exactly the empty condition and `h != t` the non-empty
        // one — order is preserved by the cursors, never by the comparison.
        while h != t {
            let slot = self.slot(h as usize);
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                // Reserved but not yet published. Cannot skip (cursor order);
                // a later drain will pick it up once the producer publishes.
                break;
            }
            // Reclaim the offset. Done BEFORE clearing the slot so a concurrent
            // producer cannot reuse this slot before we've consumed it (the
            // full-check prevents reuse while undrained, and clearing marks it
            // drained for the next wrap).
            reclaim(off);
            // Clear the slot for the next wrap. Relaxed: the next producer to
            // touch this slot will Release-store its offset; our drain reads
            // Acquire. No cross-thread dependency on this clear's ordering.
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
            publish.h = h;
        }
        // The guard's `Drop` publishes `h` with Release — the sole head store,
        // covering both the happy path (scope-end drop) and the unwind path
        // (drop during unwind). No explicit store is needed here.
        h
    }

    /// PERF-PASS-4 (G9/C2, task #52) — pre-drain empty-guard primitive: a
    /// cheap Relaxed load of `tail` ONLY (no `head` load at all — the caller
    /// already holds its own owner-private cached copy of `head`, refreshed
    /// from [`drain`](Self::drain)'s return value).
    ///
    /// **Why `Relaxed` is sound here (extends the existing single-consumer
    /// argument at [`drain`](Self::drain)'s doc comment):** the sole purpose
    /// of this load is to decide "has ANY producer reserved a slot since we
    /// last drained". A push's `tail` CAS is `AcqRel`; a Relaxed load here may
    /// observe an OLDER value of `tail` than the most recent CAS (no
    /// synchronizes-with edge), but it can NEVER observe a value that skips a
    /// real advance: `tail` is monotonic (only ever `wrapping_add(1)`-ed by a
    /// winning CAS), so ANY Relaxed load of it returns either the cached
    /// value or a LATER one — never a value that hides a genuine push. Three
    /// outcomes:
    ///   - `tail_relaxed() == cached_head` → the ring is PROVABLY unchanged
    ///     since the cache was taken (no push can have landed without moving
    ///     `tail` off `cached_head`, and `cached_head` was itself set FROM a
    ///     real `head` value that only advances up to a real `tail`) — safe
    ///     to skip the drain entirely.
    ///   - `tail_relaxed() != cached_head` but a push landed AFTER this load
    ///     returns → exactly the same as today's drain missing a push that
    ///     lands after `drain`'s own `tail.load(Acquire)` returns: the
    ///     "later drain picks it up" contract (module docs) already covers
    ///     this window, unconditionally, regardless of whether THIS call
    ///     skipped or ran a real drain.
    ///   - A push landed and is visible: `tail_relaxed() != cached_head`, the
    ///     caller falls through to a real `drain()`, which re-establishes
    ///     ordering via its own `Acquire` tail load — this Relaxed load is
    ///     ONLY a pre-filter, never the operation that reads the pushed data.
    ///
    /// The slot re-claim boundary (a segment's ring surviving a `HeapSlot`
    /// recycle→claim, per the whole-slot-reuse discipline — see
    /// `crate::registry::heap_registry`'s module doc and
    /// `AbandonGuard::drop`'s "Phase 12.5 (architectural turn)" note) needs NO
    /// extra fence
    /// here: the cache lives in the segment's OWN header
    /// (`SegmentHeader::ring_drain_head`), which is reset to `0` only when a
    /// segment is freshly reserved (`SegmentHeader::small`), exactly mirroring
    /// the ring's own `head`/`tail` reset in `RemoteFreeRing::init_in_place`
    /// at the SAME call site (`reserve_small_segment`). A recycled `HeapSlot`
    /// re-claimed by a new owner thread reuses the SAME `HeapCore` (and hence
    /// the SAME live segments/rings) whole — there is no "new owner, old
    /// ring" combination in this codebase's shard-reuse model, so there is no
    /// window where a stale cached head from a different logical owner could
    /// leak across a re-claim.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn tail_relaxed(&self) -> u32 {
        self.tail().load(Ordering::Relaxed)
    }

    /// R6-REGRESSION-2 (progress-detection stop condition in
    /// `HeapCore::push_with_overflow_retry`): the ring's current DRAIN cursor
    /// (`head`) as a single `Relaxed` load — the production (non-`dbg_*`)
    /// sibling of the test-only [`dbg_cursors`](Self::dbg_cursors) hook,
    /// exposing ONLY the consumer-advanced half of the cursor pair.
    ///
    /// **Purpose.** A producer stuck in the bounded retry loop needs to
    /// distinguish "the owner is draining, however slowly" (keep waiting)
    /// from "the owner is making zero drain progress" (concede to the
    /// documented bounded leak). `head` is advanced ONLY by the owner's
    /// [`drain`](Self::drain) — producers never write it — so observing it
    /// move between probe rounds is an exact "the owner drained something"
    /// signal, and observing it NOT move is an exact "the owner drained
    /// nothing in that window" signal.
    ///
    /// **Why `Relaxed` is sound (same monotonicity argument as
    /// [`tail_relaxed`](Self::tail_relaxed), applied to `head`):** `head` is
    /// monotonic (only ever advanced by the single consumer's `Release`
    /// store), so a `Relaxed` load returns either the latest value or an
    /// older one — never a fabricated future value. The caller compares two
    /// such loads taken hundreds of microseconds apart purely to detect
    /// MOVEMENT: a stale read can only UNDER-report progress (delaying the
    /// "progressed" verdict to the next probe round — one extra cheap round,
    /// never a correctness hazard), and can never fabricate progress that did
    /// not happen. No payload is read through this value, so no
    /// Acquire-ordered visibility is needed here — the retry loop's own
    /// `try_push_uncounted` re-establishes ordering via its `Acquire` head
    /// load when it actually attempts the push.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn head_relaxed(&self) -> u32 {
        self.head().load(Ordering::Relaxed)
    }
}
