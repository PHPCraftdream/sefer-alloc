use core::sync::atomic::Ordering;

use super::{HeapOverflow, ENTRY_EMPTY_BASE, HEAP_OVERFLOW_CAP, INLINE_CAP};

/// Outcome of [`HeapOverflow::room_check`]. See that function's doc for the
/// R1-05 stale-vs-full distinction this exists to make.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum RoomCheck {
    /// Confirmed room for reservation attempt `t`.
    Room,
    /// `t`'s snapshot predates `h` — stale, not full. Retry.
    Stale,
    /// Genuinely full for a coherent `(t, h)` pair.
    Full,
}

impl HeapOverflow {
    /// Push `(base, packed)` — a cross-thread-freed block's segment base and
    /// its already-packed `(offset, class)` word — onto this heap's
    /// second-chance overflow ring. Called after a segment-ring failure,
    /// before and during the bounded retry window. Returns `false` if this
    /// ring is full (bumping its `overflow_count` diagnostic) —
    /// OR if the reserved index falls in the sidecar range and the sidecar
    /// cannot be materialised (OOM). Both failure causes are
    /// indistinguishable to the caller: both mean "the ring could not accept
    /// this entry right now". If retries also fail, the caller uses the
    /// lossless intrusive spill.
    ///
    /// **R6-OPT-P0-2 (round 2) — the wedge-hazard fix, in the exact ordering
    /// that matters.** `tail`'s CAS reservation is an IRREVERSIBLE ratchet
    /// (there is no "give back my reservation"), so this method MUST NOT win
    /// the CAS for an index it cannot subsequently publish into. The loop
    /// therefore checks `t >= INLINE_CAP` and calls
    /// `bootstrap::ensure_overflow_sidecar` — mirroring `Registry::
    /// ensure_chunk`'s fast-path-Acquire-load / CAS-materialise shape —
    /// BEFORE attempting the CAS on `t`, not after. If the sidecar cannot be
    /// materialised, this returns `false` immediately: `tail` is never
    /// advanced, so no reservation is ever left stranded, so `drain` can
    /// never wedge on an unhonourable index. See the module doc's "wedge
    /// hazard" section for the full argument and
    /// `crates/once-ptr-cell/tests/loom_once_ptr_cell.rs` (CRATE-P3, which replaced the former `tests/loom_overflow_sidecar_cas.rs`) / the sidecar-OOM unit test for
    /// the proof.
    ///
    /// `base` MUST be a real, non-null segment base (never
    /// [`ENTRY_EMPTY_BASE`] — see that constant's doc comment for why a real
    /// segment base is never `0`). A null `base` returns `false` in every
    /// build, without reserving a cursor or bumping `overflow_count`.
    ///
    /// `pub` (doc-hidden, not stable API) ONLY so
    /// `tests/miri_heap_overflow_unit.rs` can drive the protocol directly —
    /// see [`new_boxed_for_test`](Self::new_boxed_for_test)'s doc comment.
    #[doc(hidden)]
    pub fn push(&self, base: *mut u8, packed: u32) -> bool {
        self.push_impl(base, packed, true)
    }

    /// R6-OPT-P0-4: byte-identical push/CAS/publish protocol to [`push`](
    /// Self::push), EXCEPT the "ring full" branch does NOT bump
    /// `overflow_count`.
    ///
    /// Exists ONLY for `HeapCore::push_with_overflow_retry`'s bounded
    /// spin-retry loop — the rare double-saturation tier reached only after
    /// BOTH the segment ring's one counted attempt AND an immediate
    /// `push_to_heap_overflow` attempt have already failed. Every poll inside
    /// that loop is a re-check of an already-known-full-or-recovering ring,
    /// not a new diagnostic event; counting each of up to
    /// `RETRY_ROUND_SPINS` × `RETRY_ROUND_SAFETY_CAP` re-polls (see
    /// `heap_core_xthread`'s probe-round model) would tax `overflow_count`
    /// with a
    /// locked RMW per poll for no informational gain — mirrors
    /// `RemoteFreeRing::try_push_uncounted`'s identical rationale (see that
    /// method's doc comment) applied to this ring's own counter. The ONE
    /// counted `push` attempt the caller already made (the immediate
    /// step-2 attempt in `push_with_overflow_retry`, or the single
    /// owner-not-live attempt) remains the signal "this heap's overflow ring
    /// saturated at all"; this uncounted variant must never be used at either
    /// of those two one-shot call sites, only inside the bounded retry loop.
    ///
    /// `base` MUST be a real, non-null segment base — same contract as
    /// [`push`](Self::push), including the null-`base` rejection. Same
    /// wedge-hazard-safe sidecar-materialisation ordering as `push` — see
    /// that method's doc comment.
    ///
    /// `pub` (doc-hidden, not stable API) for the same reason as
    /// [`push`](Self::push) — kept `pub` for test-surface symmetry even
    /// though production code reaches it only through `HeapCore`.
    #[doc(hidden)]
    pub fn push_uncounted(&self, base: *mut u8, packed: u32) -> bool {
        self.push_impl(base, packed, false)
    }

    /// Classify a `(t, h)` cursor pair read together at the top of
    /// [`push_impl`](Self::push_impl)'s loop, distinguishing a genuinely
    /// full ring from a STALE `t` snapshot (R1-05,
    /// `docs/reviews/2026-09-28-005939-src-review-oxx-round-1.md`).
    ///
    /// `t`/`h` are `usize` cursors that wrap via `wrapping_add` (never a
    /// terminal sentinel like `RemoteFreeRing`'s non-wrapping `u64` pair —
    /// see `ops.rs`'s `full_check` for that ring's analogous fix). `head`
    /// never exceeds a COHERENT tail (`try_drain`'s own `while h != t` bound
    /// never advances `head` past the `tail` value it observed at entry), so
    /// for any pair read at the SAME real instant, `t.wrapping_sub(h)` is a
    /// small value in `[0, HEAP_OVERFLOW_CAP]`. But `t` and `h` here are read
    /// in TWO SEPARATE, non-atomic loads (`push_impl`'s `let t = ...; let h =
    /// ...;`) — a producer preempted between them can have its `t` overtaken:
    /// other producers push further (advancing the REAL tail past `t`) and
    /// the owner fully drains (advancing `h` past `t` too), so the freshly
    /// re-read `h` ends up `> t`. In that case `t.wrapping_sub(h)` computes
    /// as `2^BITS - (h - t)` (unsigned wraparound), a value near `usize::MAX`
    /// — reinterpreting it as `isize` recovers the true SIGNED distance:
    /// negative means `t` is logically behind `h` (stale), which can only
    /// happen from this preemption, never from real occupancy exceeding
    /// `HEAP_OVERFLOW_CAP` (occupancy is always `< 2^63`, far short of where
    /// a genuine wrap of `t`/`h` themselves could plausibly occur — the same
    /// "never really wraps in practice" assumption `wrapping_sub` already
    /// relied on before this fix, just now also used to disambiguate its
    /// sign). A real full ring instead yields a small POSITIVE diff `>=
    /// HEAP_OVERFLOW_CAP`, which this correctly reports as `Full`.
    #[inline(always)]
    fn room_check(t: usize, h: usize) -> RoomCheck {
        let diff = t.wrapping_sub(h);
        if (diff as isize) < 0 {
            RoomCheck::Stale
        } else if diff >= HEAP_OVERFLOW_CAP {
            RoomCheck::Full
        } else {
            RoomCheck::Room
        }
    }

    /// **Test surface**: exposes [`room_check`](Self::room_check)'s
    /// Room/Stale/Full classification for caller-supplied `(t, h)` values
    /// (R1-05). A pure function of its two arguments — no allocator state,
    /// no side effect — so it is safe to call in any build; not gated behind
    /// `bench-internals` (it is a pure observer per the `dbg_*` hook
    /// classification in `tests/dbg_hook_safety_tripwire.rs`). Returns `0` =
    /// Room, `1` = Stale, `2` = Full.
    #[doc(hidden)]
    pub fn dbg_room_check_code(t: usize, h: usize) -> u8 {
        match Self::room_check(t, h) {
            RoomCheck::Room => 0,
            RoomCheck::Stale => 1,
            RoomCheck::Full => 2,
        }
    }

    /// Shared implementation of [`push`](Self::push) /
    /// [`push_uncounted`](Self::push_uncounted); `counted` selects whether
    /// the "ring full" branch bumps `overflow_count` (see each public
    /// method's doc comment). Factored out so the sidecar-materialisation
    /// ordering fix (round 2) is written exactly once rather than duplicated
    /// across two near-identical loops, which was measured (`push`/
    /// `push_uncounted`'s pre-round-2 code) to already be the single largest
    /// source of drift risk between the two methods.
    #[inline]
    fn push_impl(&self, base: *mut u8, packed: u32, counted: bool) -> bool {
        // Every build: a published null slot would wedge `try_drain`. Reject
        // before touching `tail`; not counted as overflow.
        if base == ENTRY_EMPTY_BASE {
            return false;
        }
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            match Self::room_check(t, h) {
                RoomCheck::Room => {}
                // R1-05: `t` is a stale snapshot, not a full ring — reload
                // both cursors (top of loop) and retry. No counter bump:
                // nothing overflowed. Bounded by the same system-wide
                // forward-progress argument as `RemoteFreeRing::push`'s
                // `Stale` arm — see `ops.rs`'s doc comment on that arm.
                RoomCheck::Stale => continue,
                RoomCheck::Full => {
                    if counted {
                        self.overflow_count.fetch_add(1, Ordering::Relaxed);
                    }
                    return false;
                }
            }
            // R6-OPT-P0-2 (round 2) — the wedge-hazard fix: if this
            // reservation attempt targets the sidecar range, ensure the
            // sidecar exists BEFORE attempting the CAS. `tail` is an
            // irreversible ratchet; winning the CAS for an index whose
            // backing store cannot be materialised would strand that index
            // unpublished forever, wedging `drain` — see the module doc's
            // "wedge hazard" section. On OOM, return `false` WITHOUT ever
            // touching `tail` (identical externally-observable outcome to
            // "the ring is full" — the caller's existing bounded-leak
            // handling covers this with no changes needed there).
            //
            // `t % HEAP_OVERFLOW_CAP` (NOT the raw, monotonically-increasing
            // `t`): `t` keeps growing across wraps (it is never reset), so
            // after the ring has wrapped once a raw `t` far larger than
            // `INLINE_CAP` can still land on a WRAPPED index inside the
            // inline tier (e.g. `t == HEAP_OVERFLOW_CAP` wraps to index `0`,
            // squarely inline) — comparing the raw cursor against `INLINE_CAP`
            // would wrongly demand a materialised sidecar for an inline-tier
            // slot on every wrap. `slot()` performs the same `%
            // HEAP_OVERFLOW_CAP` reduction; this check mirrors it exactly so
            // the two agree on which tier `t` targets.
            let wrapped_idx = t % HEAP_OVERFLOW_CAP;
            if wrapped_idx >= INLINE_CAP
                && !crate::registry::bootstrap::ensure_overflow_sidecar(&self.sidecar)
            {
                if counted {
                    self.overflow_count.fetch_add(1, Ordering::Relaxed);
                }
                return false;
            }
            match self.tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let (base_slot, packed_slot) = self.slot(t);
                    // Publish `packed` BEFORE `base`: the drain side reads
                    // `base` first (its "is this slot published" gate) and
                    // `packed` second, so `base` must be the LAST-published
                    // half of the pair — see `drain`'s read order below for
                    // the matching half of this Release/Acquire handshake.
                    packed_slot.store(packed, Ordering::Relaxed);
                    base_slot.store(base, Ordering::Release);
                    return true;
                }
                Err(_) => continue,
            }
        }
    }
}
