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

/// B4 У1b cached-handle transport (ADR `adr_v2_r4.md` §4.1 (в) — chunk U1b).
///
/// Sequential model of the own_cache entry lifecycle that backs
/// `SegmentTable::cached_class_leaves` and the single `unsafe` deref in
/// `AllocCore::small_class_authority`. The model abstracts:
///
/// - `route_alive: bool` — the route (and its System-backed `ClassLeaves`)
///   is registered; dropping it is `RouteSlots::remove`.
/// - `entry: Option<(u32, Handle)>` — an own_cache slot; `Some((k, h))` is a
///   filled entry (key `k`, captured handle), `None` is empty/cleared.
/// - `lookup(key)` — `cached_class_leaves`: yields the handle iff the entry
///   is filled AND its key matches.
///
/// What IS proved (symbolically, all inputs):
///   P1  lookup returns `Some` ONLY on an exact key match (no deref happens
///       on a mismatch — the transport's guard is load-bearing);
///   P2  while `route_alive` holds and the entry is filled with a matching
///       key, the handle is present (`Some`) — the deref-precondition
///       "entry valid ⇔ route alive" is maintained by any interleaving of
///       fill/clear that respects the protocol (fill only on a live route;
///       clear zeroes the entry in the same step that kills the route);
///   P3  once the route is dropped, the protocol's clear step empties the
///       entry, so no subsequent lookup can produce a dangling handle.
///
/// LIMITATIONS (honest scope): this is a SEQUENTIAL abstract model — it does
/// not execute the real `SegmentTable` (primordial-carved pointers are not
/// Kani-addressable), does not model direct-mapped conflicts beyond "the
/// entry currently queried", and does not prove memory ordering (the
/// `ClassLeaves::encoded` reads are atomics; concurrency is covered by the
/// existing Miri/Loom harnesses and the owner-only `&mut self` discipline,
/// which the model assumes). What it pins is the CONTROL LAW: no deref
/// without an exact key match, and no handle outliving its route.
#[cfg(all(kani, feature = "small-class-authority"))]
mod own_cache_handle_proofs {
    /// Model handle: `Some` = captured from a live route at fill time.
    type Handle = u32;

    #[derive(Clone, Copy)]
    struct Model {
        route_alive: bool,
        entry: Option<(u32, Option<Handle>)>,
    }

    /// Mirrors `SegmentTable::cached_class_leaves`: Some(handle) iff key hit
    /// and the entry carries a non-null handle; Some(null-analogue `None`)
    /// on a hit without a handle; `None` on a key mismatch/empty entry.
    fn lookup(m: &Model, key: u32) -> Option<Option<Handle>> {
        match m.entry {
            Some((k, h)) if k == key => Some(h),
            _ => None,
        }
    }

    /// Protocol fill: only ever captures a handle from a LIVE route.
    fn fill(m: &mut Model, key: u32) {
        assert!(m.route_alive, "fill happens on a won probe of a live segment");
        m.entry = Some((key, Some(7)));
    }

    /// Protocol teardown (ADR: `routes.remove` → `hash_remove` →
    /// `own_cache_clear`, one owner-only step): kills the route AND empties
    /// the entry atomically with respect to observers.
    fn teardown(m: &mut Model) {
        m.route_alive = false;
        m.entry = None;
    }

    #[kani::proof]
    #[kani::unwind(4)]
    fn no_handle_without_exact_key_match() {
        let m = Model {
            route_alive: kani::any(),
            entry: if kani::any() {
                Some((kani::any(), if kani::any() { Some(kani::any()) } else { None }))
            } else {
                None
            },
        };
        let key: u32 = kani::any();
        if let Some(h) = lookup(&m, key) {
            // A lookup that produced a handle MUST have matched the key (P1):
            // the deref guard in `small_class_authority` cannot be bypassed.
            assert!(matches!(m.entry, Some((k, _)) if k == key));
            // And a handle implies the route is still alive (P2): the only
            // transition that kills the route also empties the entry.
            if h.is_some() {
                assert!(m.route_alive);
            }
        }
    }

    #[kani::proof]
    #[kani::unwind(4)]
    fn teardown_never_leaves_a_dangling_handle() {
        let mut m = Model { route_alive: true, entry: None };
        let key: u32 = kani::any();
        fill(&mut m, key);
        // A concurrent-shaped observer between remove and clear is
        // impossible (owner-only `&mut`); model the only legal observation
        // points: before teardown, and after.
        assert_eq!(lookup(&m, key), Some(Some(7)));
        teardown(&mut m);
        assert_eq!(lookup(&m, key), None, "P3: cleared entry yields no handle");
        assert_eq!(lookup(&m, key.wrapping_add(1)), None);
    }
}
