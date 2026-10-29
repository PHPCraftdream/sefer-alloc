//! Sequential Kani proofs over symbolic inputs, compiled only under `cfg(kani)`.
//!
//! - `node_proofs` checks pointer primitives on locally valid buffers, not
//!   caller obligations (bounds, exclusivity, or lifetime).
//! - `hand_proofs` checks two vacant-slot facts, not epoch concurrency.
//!   The Loom tests are handwritten shadow publication models, not proofs of
//!   the real `AtomicSlot` implementation or reclamation.
//! - `large_state_proofs` checks the real Large codec and all nine sequential
//!   transition methods, including rejection and generation exhaustion.
//! - `saturation_proofs` checks the real versioned hint after bounded
//!   publications and a fresh reachable snapshot. `internals` exposes only
//!   its local-word constructor; the proved methods are unchanged.
//!
//! - `depot_proofs` checks the B4 magazine depot store with its parked
//!   count (И-D1 capacity, И-D2 exact LIFO round trip, class isolation);
//!   not the `HeapSlot` binding or the MAG/live accounting around it.
//!
//! These proofs do not establish concurrent memory ordering, reservation
//! lifetime, ownership authority, or reclamation safety.

#[cfg(all(kani, feature = "alloc-core"))]
mod node_proofs {
    use crate::alloc_core::node::Node;
    use core::ptr::NonNull;

    // ── 1. write_next / read_next round-trip ─────────────────────────────

    #[kani::proof]
    fn write_read_next_roundtrip() {
        let mut buf = [0u8; 16];
        let block = NonNull::new(buf.as_mut_ptr()).unwrap();
        let next: *mut u8 = kani::any::<usize>() as *mut u8;
        Node::write_next(block, next);
        let got = Node::read_next(block);
        assert_eq!(got, next);
    }

    // ── 2. deref in-bounds ───────────────────────────────────────────────

    #[kani::proof]
    fn deref_in_bounds() {
        let mut buf = [0u8; 64];
        let base = buf.as_mut_ptr();
        let offset: usize = kani::any();
        kani::assume(offset < 64);
        let result = Node::deref(base, offset);
        assert_eq!(result, base.wrapping_add(offset));
    }

    // ── 3. offset in-bounds ──────────────────────────────────────────────

    #[kani::proof]
    fn offset_in_bounds() {
        let mut buf = [0u8; 64];
        let base = buf.as_mut_ptr();
        let off: usize = kani::any();
        kani::assume(off < 64);
        let result = Node::offset(base, off);
        assert_eq!(result, base.wrapping_add(off));
    }

    // ── 4. zero fills buffer ─────────────────────────────────────────────

    #[kani::proof]
    fn zero_fills_buffer() {
        let mut buf = [0xFFu8; 32];
        Node::zero(buf.as_mut_ptr(), 32);
        for i in 0..32 {
            assert_eq!(buf[i], 0);
        }
    }

    // ── 5. copy_nonoverlapping copies correctly ──────────────────────────

    #[kani::proof]
    fn copy_nonoverlapping_copies() {
        let src: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut dst = [0u8; 8];
        Node::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), 8);
        for i in 0..8 {
            assert_eq!(dst[i], src[i]);
        }
    }

    // ── 6. write_u8 / read_u8 round-trip ─────────────────────────────────

    #[kani::proof]
    fn write_read_u8_roundtrip() {
        let mut buf = [0u8; 1];
        let val: u8 = kani::any();
        Node::write_u8(buf.as_mut_ptr(), val);
        let got = Node::read_u8(buf.as_ptr());
        assert_eq!(got, val);
    }

    // ── 7. write_u32_unaligned / read_u32_unaligned round-trip ───────────

    #[kani::proof]
    fn write_read_u32_roundtrip() {
        let mut buf = [0u8; 4];
        let val: u32 = kani::any();
        let ptr = buf.as_mut_ptr() as *mut u32;
        Node::write_u32_unaligned(ptr, val);
        let got = Node::read_u32_unaligned(ptr as *const u32);
        assert_eq!(got, val);
    }

    // ── 8. write_struct / read_struct round-trip ─────────────────────────

    #[derive(Copy, Clone, PartialEq, Debug)]
    #[repr(C)]
    struct Small {
        a: u16,
        b: u32,
    }

    #[kani::proof]
    fn write_read_struct_roundtrip() {
        let a: u16 = kani::any();
        let b: u32 = kani::any();
        let val = Small { a, b };

        // Use an aligned buffer large enough for `Small`.
        let mut storage = core::mem::MaybeUninit::<Small>::uninit();
        let ptr = storage.as_mut_ptr();
        Node::write_struct(ptr, val);
        let got = Node::read_struct(ptr as *const Small);
        assert_eq!(got, val);
    }

    // ── 9. write_usize / read_usize round-trip ──────────────────────────

    #[kani::proof]
    fn write_read_usize_roundtrip() {
        let mut storage = 0usize;
        let val: usize = kani::any();
        let ptr = &mut storage as *mut usize;
        Node::write_usize(ptr, val);
        let got = Node::read_usize(ptr as *const usize);
        assert_eq!(got, val);
    }
}

// These harnesses never enter the epoch runtime. Loom covers a handwritten
// shadow publication model, not the real AtomicSlot or its reclamation.
#[cfg(all(kani, feature = "experimental"))]
mod hand_proofs {
    use crate::concurrent::epoch::hand::AtomicSlot;

    #[kani::proof]
    fn vacant_starts_at_generation_zero() {
        let slot = AtomicSlot::<u32>::vacant();
        assert_eq!(slot.generation(), 0);
    }

    #[kani::proof]
    fn drop_value_vacant_is_noop() {
        let mut slot = AtomicSlot::<u32>::vacant();
        slot.drop_value();
    }
}

#[cfg(all(
    kani,
    feature = "alloc-core",
    not(all(feature = "alloc-global", feature = "internals"))
))]
#[path = "alloc_core/large/reservation_state.rs"]
mod proof_reservation_state;
// `proof_reservation_state` takes its atomics from `super`.
#[cfg(all(
    kani,
    feature = "alloc-core",
    not(all(feature = "alloc-global", feature = "internals"))
))]
use core::sync::atomic::{AtomicU64, Ordering};

#[cfg(all(kani, feature = "alloc-core"))]
mod large_state_proofs {
    use crate::alloc_core::segment_header::{
        large_generation, large_phase, next_large_generation_bounded, pack_large_state, LargePhase,
        MAX_LARGE_GENERATION,
    };
    #[cfg(all(feature = "alloc-global", feature = "internals"))]
    use crate::alloc_core::LargeReservationState;
    // Compile the real production source when its public reexport is absent.
    #[cfg(not(all(feature = "alloc-global", feature = "internals")))]
    use super::proof_reservation_state::LargeReservationState;
    use core::sync::atomic::{AtomicU64, Ordering};

    #[kani::proof]
    fn large_codec_roundtrip_and_canonical() {
        let generation: u64 = kani::any();
        kani::assume(generation <= MAX_LARGE_GENERATION);
        let code: u64 = kani::any();
        kani::assume(code <= 6);
        let phase = large_phase(code).expect("valid phase code");
        let word = pack_large_state(phase, generation);
        assert_eq!(large_phase(word), Some(phase));
        assert_eq!(large_generation(word), generation);
        let arbitrary: u64 = kani::any();
        if let Some(decoded) = large_phase(arbitrary) {
            assert_eq!(
                pack_large_state(decoded, large_generation(arbitrary)),
                arbitrary
            );
        }
        kani::cover!(
            large_phase(arbitrary).is_none(),
            "invalid phase is reachable"
        );
    }

    #[kani::proof]
    fn next_generation_never_exceeds_limit() {
        let generation: u64 = kani::any();
        let limit: u64 = kani::any();
        let next = next_large_generation_bounded(generation, limit);
        match next {
            Some(value) => {
                assert!(limit <= MAX_LARGE_GENERATION);
                assert!(generation < limit);
                assert_eq!(value, generation + 1);
                assert!(value <= limit);
            }
            None => assert!(generation >= limit || limit > MAX_LARGE_GENERATION),
        }
        kani::cover!(next.is_some(), "generation can advance");
    }

    #[kani::proof]
    fn large_reservation_state_transition_relation() {
        use LargePhase::{Cached, Consuming, Initializing, Live, Pending, Released};
        let before: u64 = kani::any();
        let word = AtomicU64::new(before);
        let state = LargeReservationState::new(&word);
        let generation: u64 = kani::any();
        kani::assume(generation <= MAX_LARGE_GENERATION);
        let operation: u8 = kani::any();
        kani::assume(operation < 9);
        let old_generation = large_generation(before);
        // Independent lifecycle table from VER-04 §1.4 / §5.4: each
        // method must implement its own edge, not just any allowed edge.
        let (from, to) = match operation {
            0 => (Live, Pending),
            1 => (Pending, Consuming),
            2 => (Live, Consuming),
            3 => (Consuming, Cached),
            4 => (Consuming, Released),
            5 => (Cached, Released),
            6 => (Initializing, Released),
            7 => (Cached, Initializing),
            _ => (Initializing, Live), // operation == 8 by the guard
        };
        let expected_success = large_phase(before) == Some(from)
            && match operation {
                1 | 2 => true,
                7 => old_generation < MAX_LARGE_GENERATION,
                _ => old_generation == generation,
            };
        let mut returned_generation = None;
        let success = match operation {
            0 => state.publish_pending(generation),
            1 => {
                returned_generation = state.claim_pending();
                returned_generation.is_some()
            }
            2 => {
                returned_generation = state.claim_live();
                returned_generation.is_some()
            }
            3 => state.cache_consumed(generation),
            4 => state.release_consumed(generation),
            5 => state.release_cached(generation),
            6 => state.release_initializing(generation),
            7 => {
                returned_generation = state.begin_reuse();
                returned_generation.is_some()
            }
            _ => state.finish_reuse(generation),
        };
        let after = word.load(Ordering::Relaxed);
        let changed = after != before;
        // Kani 0.67 models strong CAS success flag loosely; `ok => changed`
        // is pinned natively by consumed_cache_hit_advances_generation_only_after_reset_phase.
        assert_eq!(changed, expected_success);
        assert!(!changed || success);
        if changed {
            let next_generation = if operation == 7 {
                old_generation + 1 // expected_success excludes exhaustion
            } else {
                old_generation
            };
            assert_eq!(large_phase(after), Some(to));
            assert_eq!(large_generation(after), next_generation);
            assert_eq!(after, pack_large_state(to, next_generation));
            if operation == 1 || operation == 2 || operation == 7 {
                assert_eq!(returned_generation, Some(next_generation));
            }
        } else {
            assert_eq!(after, before);
            if !success {
                assert_eq!(returned_generation, None);
            }
        }
        kani::cover!(changed, "a lifecycle transition changes the word");
    }
}

#[cfg(all(kani, feature = "alloc-global", feature = "internals"))]
mod saturation_proofs {
    use crate::registry::bootstrap::saturation::SaturationHint;

    #[kani::proof]
    #[kani::unwind(5)]
    fn stale_saturation_snapshot_never_marks() {
        let before: u64 = kani::any();
        let hint = SaturationHint::dbg_with_word_for_test(before);
        let stale = hint.snapshot();
        let publications: u8 = kani::any();
        kani::assume(publications >= 1 && publications <= 3);
        for _ in 0..publications {
            hint.publish_claimable();
        }
        let after = hint.snapshot();
        assert!(after == u64::MAX || (after > before && after & 1 == 0));
        if before == u64::MAX {
            assert_eq!(after, u64::MAX);
        }
        hint.try_mark_saturated(stale);
        assert_eq!(hint.snapshot(), after);
        assert!(!hint.is_saturated());
        kani::cover!(after != u64::MAX, "publication advances an enabled version");
    }

    #[kani::proof]
    fn fresh_snapshot_marks() {
        let before: u64 = kani::any();
        // Reachable enabled snapshots are even and at most MAX-3.
        // MAX-1 is unreachable through publish_claimable: marking it would
        // produce DISABLED rather than saturation, intentionally excluded here.
        kani::assume(before & 1 == 0 && before <= u64::MAX - 3);
        let hint = SaturationHint::dbg_with_word_for_test(before);
        hint.try_mark_saturated(hint.snapshot());
        assert_eq!(hint.snapshot(), before | 1);
        assert!(hint.is_saturated());
        kani::cover!(
            hint.is_saturated(),
            "fresh reachable snapshot marks saturation"
        );
    }
}

#[cfg(all(kani, feature = "alloc-global", feature = "fastbin"))]
mod depot_proofs {
    use crate::registry::heap_core::{Depot, DEPOT_CLASSES, DEPOT_K, TCACHE_CAP};

    fn any_magazine() -> [*mut u8; TCACHE_CAP] {
        let mut m = [core::ptr::null_mut(); TCACHE_CAP];
        for slot in &mut m {
            *slot = kani::any::<usize>() as *mut u8;
        }
        m
    }

    /// И-D1: a park succeeds iff the class holds fewer than `DEPOT_K[c]`
    /// magazines, and the parked count never exceeds that bound.
    #[kani::proof]
    #[kani::unwind(17)]
    fn depot_park_respects_capacity() {
        let mut d = Depot::empty();
        let c: usize = kani::any();
        kani::assume(c < DEPOT_CLASSES);
        let mut n: u8 = 0;
        let attempts: usize = kani::any();
        kani::assume(attempts <= 9);
        let mag = any_magazine();
        let mut i = 0;
        while i < attempts {
            let before = n;
            let ok = d.depot_park(c, &mut n, &mag);
            assert_eq!(ok, (before as usize) < DEPOT_K[c] as usize);
            assert_eq!(n, if ok { before + 1 } else { before });
            assert!(n as usize <= DEPOT_K[c] as usize);
            i += 1;
        }
    }

    /// И-D2: reload returns exactly the magazines parked, last-in first-out,
    /// then reports empty; destination and count are untouched on an empty
    /// reload. Another class's stored magazine is not disturbed.
    #[kani::proof]
    #[kani::unwind(17)]
    fn depot_reload_returns_parked_magazines_lifo() {
        let mut d = Depot::empty();
        let c: usize = kani::any();
        let other: usize = kani::any();
        kani::assume(c < DEPOT_CLASSES && other < DEPOT_CLASSES && other != c);
        let mut n: u8 = 0;
        let mut n_other: u8 = 0;
        let a = any_magazine();
        let b = any_magazine();
        let o = any_magazine();
        assert!(d.depot_park(other, &mut n_other, &o));
        assert!(d.depot_park(c, &mut n, &a));
        let two = DEPOT_K[c] >= 2;
        if two {
            assert!(d.depot_park(c, &mut n, &b));
        }
        let mut out = [core::ptr::null_mut(); TCACHE_CAP];
        if two {
            assert!(d.depot_reload(c, &mut n, &mut out));
            assert!(out == b);
        }
        assert!(d.depot_reload(c, &mut n, &mut out));
        assert!(out == a);
        let sentinel = any_magazine();
        let mut untouched = sentinel;
        assert!(!d.depot_reload(c, &mut n, &mut untouched));
        assert!(untouched == sentinel);
        assert_eq!(n, 0);
        assert!(d.depot_reload(other, &mut n_other, &mut out));
        assert!(out == o);
    }
}

#[cfg(all(
    kani,
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit"
))]
mod depot_overflow_proofs {
    use crate::alloc_core::size_classes::SIZE_CLASS_TABLE;
    use crate::alloc_core::LargeCacheConfig;
    use crate::registry::heap_core::{DepotOverflow, DEPOT_CLASSES, DEPOT_K, TCACHE_CAP};

    const CAP: usize = 3;
    const STEPS: usize = 5;

    /// И-O1: the ring against a queue model — park appends iff the ring and
    /// the budget have room, reload returns the newest record, decay the
    /// oldest (the ring wraps once decay moved its head), and the parked
    /// byte count is always `len * magazine bytes`.
    #[kani::proof]
    #[kani::unwind(18)]
    fn overflow_ring_matches_a_queue() {
        let mut buf = [[core::ptr::null_mut::<u8>(); TCACHE_CAP]; CAP];
        let c: usize = kani::any();
        kani::assume(c < DEPOT_CLASSES);
        let budget: usize = kani::any();
        kani::assume(budget <= 1 << 30);
        let mb = SIZE_CLASS_TABLE[c] * TCACHE_CAP;
        let mut o = DepotOverflow::kani_on_buffer(buf.as_mut_ptr().cast(), c, CAP as u16);
        let mut model = [0usize; STEPS];
        let (mut front, mut back) = (0usize, 0usize);
        let mut out = [core::ptr::null_mut::<u8>(); TCACHE_CAP];
        let mut step = 0;
        while step < STEPS {
            let n = back - front;
            let op: u8 = kani::any();
            if op % 3 == 0 {
                let id = step + 1;
                let mag = [id as *mut u8; TCACHE_CAP];
                let room = n < CAP && o.bytes() + mb <= budget;
                assert_eq!(o.park(c, n, &mag, budget), room);
                if room {
                    model[back] = id;
                    back += 1;
                }
            } else if op % 3 == 1 && n > 0 {
                o.reload(c, n, &mut out);
                back -= 1;
                assert!(out == [model[back] as *mut u8; TCACHE_CAP]);
            } else if n > 0 {
                o.pop_oldest(c, n, &mut out);
                assert!(out == [model[front] as *mut u8; TCACHE_CAP]);
                front += 1;
            }
            assert_eq!(o.bytes(), (back - front) * mb);
            step += 1;
        }
        kani::cover!(front > 0 && back - front == CAP, "the ring wrapped full");
    }

    /// И-O2: for every request the resolved budget is a multiple of 256 B
    /// and floor + overflow ring of any depot class fits `PerClass::limit`.
    #[kani::proof]
    fn resolved_budget_fits_the_park_limit() {
        let req: usize = kani::any();
        let c: usize = kani::any();
        kani::assume(c < DEPOT_CLASSES);
        let b = LargeCacheConfig::new()
            .depot_budget_bytes(req)
            .resolved_depot_budget_bytes();
        assert_eq!(b % 256, 0);
        assert!(b <= req);
        assert!(DEPOT_K[c] as usize + DepotOverflow::cap_for(b, c) <= u16::MAX as usize);
    }
}
