//! Kani proof harnesses — bounded, symbolic-input round-trip / invariant
//! proofs, compiled only under `cfg(kani)`.
//!
//! What these prove, and what they DO NOT:
//!
//! - `node_proofs` — smoke round-trip proofs on LOCALLY-VALID buffers for the
//!   `alloc_core::node::Node` pointer primitives (write→read round-trips,
//!   in-bounds `deref`/`offset`). They exercise the arithmetic; they do NOT
//!   model the caller contracts those primitives require (bounds, exclusivity,
//!   `'static` lifetime) — those are the caller's obligation, unmodelled here.
//! - `hand_proofs` — two no-concurrency invariants of `AtomicSlot`
//!   (`vacant().generation() == 0` and `drop_value()` on a vacant slot is a
//!   no-op). These do NOT model the publication/eviction protocol: Kani cannot
//!   model concurrency (see the `hand_proofs` module comment on `pin()`), so
//!   the CAS-uniqueness / no-torn-read properties are covered by loom, not here.
//! - `pack_proofs` — bounded round-trip / no-panic proofs over symbolic input
//!   for the registry's pure bit-packing arithmetic
//!   (`tagged_index_stack::TaggedIndex::pack/unpack` at `INDEX_BITS = 16`, the
//!   extracted `free_slots` packing — CRATE-P7). This is where Kani is genuinely strong:
//!   exhaustive-over-all-inputs bounded proofs with no concurrency and no
//!   caller contract to assume. (The abandoned-segment head-packing proofs
//!   that previously also lived here were removed with that substrate — task
//!   #97 / R4-5.)

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

// Kani does NOT support concurrency: `crossbeam_epoch::pin()` uses TLS
// (`pthread_key_create`) which CBMC cannot model, so every harness that calls
// `pin()` fails with "call to foreign C function pthread_key_create is not
// currently supported". The concurrent invariants of `AtomicSlot` (CAS
// uniqueness, no torn reads) are already verified by loom (11 harnesses in
// CI). We keep only harnesses that never touch the epoch runtime.
#[cfg(all(kani, feature = "experimental"))]
mod hand_proofs {
    use crate::concurrent::hand::AtomicSlot;

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

// Bounded proofs of the registry's pure bit-packing arithmetic. No pointers
// are dereferenced, no concurrency, no caller contract — Kani explores EVERY
// input in the modelled range and proves the round-trip / no-overflow
// invariants hold. These harnesses ARE the regression tests for the
// `free_slots` packing (a future INDEX_BITS change that broke round-trip or
// let a tag bleed into the value half would fail here).
#[cfg(all(kani, feature = "alloc-global"))]
mod pack_proofs {
    // CRATE-P7: the `free_slots` packing now lives in the `tagged-index-stack`
    // crate (`TaggedIndex<INDEX_BITS>`); the registry uses `INDEX_BITS = 16`.
    // These proofs bind the crate's `pack`/`unpack` at that width.
    use tagged_index_stack::TaggedIndex;

    const INDEX_BITS: u32 = 16;
    type Packed = TaggedIndex<INDEX_BITS>;

    // ── 1. TaggedIndex pack→unpack round-trip ────────────────────────────
    //
    // For any value that fits the low INDEX_BITS and any tag that fits the
    // high (64 - INDEX_BITS), pack then unpack recovers BOTH halves exactly.
    #[kani::proof]
    fn tagged_pack_unpack_roundtrip() {
        let index_mask: u64 = (1u64 << INDEX_BITS) - 1;
        let value: u64 = kani::any();
        let tag: u64 = kani::any();
        // The caller's documented invariant: value fits the index half, tag
        // fits the remaining high bits.
        kani::assume(value <= index_mask);
        kani::assume(tag < (1u64 << (64 - INDEX_BITS)));

        let word = Packed::pack(value, tag);
        let (got_value, got_tag) = Packed::unpack(word);
        assert_eq!(got_value, value);
        assert_eq!(got_tag, tag);
    }

    // ── 2. TaggedIndex unpack never loses / mixes bits on ANY word ───────
    //
    // For a fully arbitrary 64-bit word (no assumptions), unpack splits it at
    // the INDEX_BITS boundary with no overlap: value is exactly the low bits,
    // tag is exactly the high bits, and re-packing them is the identity.
    #[kani::proof]
    fn tagged_unpack_is_clean_split() {
        let index_mask: u64 = (1u64 << INDEX_BITS) - 1;
        let word: u64 = kani::any();
        let (value, tag) = Packed::unpack(word);
        // Halves never overlap: value occupies only the low bits.
        assert!(value <= index_mask);
        // The split is lossless: recombining reproduces the original word.
        assert_eq!(Packed::pack(value, tag), word);
    }
}

// R15 (gap-audit item 18, task #611/K16): bounded proof of `RemoteFreeRing`'s
// `u32` cursor wrap-safety — that `t.wrapping_sub(h)` correctly reports the
// number of pushes separating `head` from `tail` for EVERY possible `head`
// (not just the handful of hand-picked near-`u32::MAX` values
// `tests/regression_ring_cursor_wrap.rs`'s native tests already pin), across
// the `u32::MAX -> 0` boundary. Pure `u32` arithmetic, no pointers, no
// concurrency, no caller contract beyond "tail was produced from head by
// `n <= RING_CAP` `wrapping_add(1)` steps" (the ring's own invariant,
// `RemoteFreeRing::push`'s doc comment) — an ideal Kani target. This module
// does NOT touch `RemoteFreeRing` itself (Kani cannot model the atomics the
// real type's `push`/`drain` use); it proves the underlying modular-
// arithmetic identity those methods rely on, generalised to every `head` and
// every occupancy in `0..=RING_CAP`, which the existing native tests check
// only pointwise.
#[cfg(all(kani, feature = "alloc-core"))]
mod ring_wrap_proofs {
    use crate::alloc_core::remote_free_ring::RING_CAP;

    // ── 1. wrapping_sub recovers the exact advance count, for ANY head ────
    //
    // For any `head` (including values within `RING_CAP` of `u32::MAX`, so
    // the wrap is exercised) and any advance count `n` in `0..=RING_CAP`,
    // `tail = head.wrapping_add(n)` followed by `tail.wrapping_sub(head)`
    // recovers `n` exactly — the occupancy count survives the wrap.
    #[kani::proof]
    fn wrapping_sub_recovers_advance_count() {
        let head: u32 = kani::any();
        let n: u32 = kani::any();
        kani::assume(n <= RING_CAP as u32);

        let tail = head.wrapping_add(n);
        assert_eq!(tail.wrapping_sub(head), n);
    }

    // ── 2. the `< RING_CAP` / `>= RING_CAP` full-ring check is exact ──────
    //
    // The production "is the ring full" check (`remote_free_ring.rs`,
    // `push`'s admission test) is `t.wrapping_sub(h) >= RING_CAP` — proves
    // that check agrees EXACTLY with the real occupancy `n` at both sides of
    // the boundary: not-full (`n < RING_CAP`) reads `< RING_CAP`, and
    // exactly-full (`n == RING_CAP`) reads `>= RING_CAP`, for every `head`.
    #[kani::proof]
    fn full_check_matches_true_occupancy_at_the_boundary() {
        let head: u32 = kani::any();
        let n: u32 = kani::any();
        kani::assume(n <= RING_CAP as u32);

        let tail = head.wrapping_add(n);
        let occupancy = tail.wrapping_sub(head);
        if n < RING_CAP as u32 {
            assert!(
                occupancy < RING_CAP as u32,
                "under-full must read < RING_CAP"
            );
        } else {
            // n == RING_CAP here (the only value `assume` still allows).
            assert!(
                occupancy >= RING_CAP as u32,
                "exactly-full must read >= RING_CAP"
            );
        }
    }
}

// R15 (gap-audit item 18, task #611/K16): bounded round-trip proofs for
// `RemoteFreeRing`'s packed-entry encodings — the non-hardened
// `pack_entry`/`unpack_entry` pair (always compiled whenever `alloc-core`
// is, the production format under `alloc-xthread`) and, separately, the
// `hardened`-only `pack_entry_hardened`/`unpack_entry_hardened` pair. Pure
// `u32` bit arithmetic, no pointers, no concurrency — proves, over every
// symbolic input the real caller contract allows, that (a) pack then unpack
// recovers every field exactly, and (b) the packed word never collides with
// [`RING_SLOT_EMPTY`] (`u32::MAX`) — the property the two `const _: ()
// assert!`s next to `pack_entry`/`pack_entry_hardened` in
// `remote_free_ring.rs` already pin at the TYPE-BOUND level (does
// `SMALL_CLASS_COUNT` fit); this proof additionally exercises the actual
// bit-shifting round trip Kani can check exhaustively where a compile-time
// assert cannot.
#[cfg(all(kani, feature = "alloc-core"))]
mod ring_entry_pack_proofs {
    use crate::alloc_core::remote_free_ring::{
        pack_entry, unpack_entry, ENTRY_OFF_BITS, RING_SLOT_EMPTY,
    };
    use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

    // ── 1. non-hardened pack/unpack round trip, for every real (off, class) ─
    #[kani::proof]
    fn pack_unpack_roundtrip() {
        let off_mask: u32 = (1u32 << ENTRY_OFF_BITS) - 1;
        let off: u32 = kani::any();
        let class_idx: u32 = kani::any();
        // The caller's documented contract (pack_entry's own doc comment):
        // `off` is a segment offset (< 2^22, i.e. fits ENTRY_OFF_BITS);
        // `class_idx < SMALL_CLASS_COUNT`.
        kani::assume(off <= off_mask);
        kani::assume(class_idx < SMALL_CLASS_COUNT as u32);

        let packed = pack_entry(off, class_idx);
        let (got_off, got_class) = unpack_entry(packed);
        assert_eq!(got_off, off);
        assert_eq!(got_class, class_idx);
    }

    // ── 2. non-hardened pack never produces the ring-slot sentinel ────────
    #[kani::proof]
    fn pack_never_collides_with_ring_slot_empty() {
        let off_mask: u32 = (1u32 << ENTRY_OFF_BITS) - 1;
        let off: u32 = kani::any();
        let class_idx: u32 = kani::any();
        kani::assume(off <= off_mask);
        kani::assume(class_idx < SMALL_CLASS_COUNT as u32);

        let packed = pack_entry(off, class_idx);
        assert_ne!(packed, RING_SLOT_EMPTY);
    }

    #[cfg(feature = "hardened")]
    mod hardened {
        use super::*;
        use crate::alloc_core::remote_free_ring::{
            pack_entry_hardened, unpack_entry_hardened, ENTRY_OFF16_MASK,
        };
        use crate::alloc_core::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT};

        // A real `off` is a MIN_BLOCK-aligned, segment-relative byte offset
        // (`< SEGMENT = 1 << 22`); `off >> MIN_BLOCK_SHIFT` must then fit
        // `ENTRY_OFF16_MASK` (18 bits) — see `pack_entry_hardened`'s own
        // `debug_assert`s, which this proof exercises symbolically instead
        // of pointwise.
        fn any_valid_off() -> u32 {
            let off16: u32 = kani::any();
            kani::assume(off16 <= ENTRY_OFF16_MASK);
            off16 << MIN_BLOCK_SHIFT
        }

        // ── 3. hardened pack/unpack round trip, for every real (gen, class, off) ─
        #[kani::proof]
        fn pack_unpack_hardened_roundtrip() {
            let gen: u8 = kani::any();
            let class_idx: u32 = kani::any();
            kani::assume(class_idx < SMALL_CLASS_COUNT as u32);
            let off = any_valid_off();

            let packed = pack_entry_hardened(gen, class_idx, off);
            let (got_gen, got_class, got_off) = unpack_entry_hardened(packed);
            assert_eq!(got_gen, gen);
            assert_eq!(got_class, class_idx);
            assert_eq!(got_off, off);
        }

        // ── 4. hardened pack never produces the ring-slot sentinel ────────
        //
        // Mirrors the non-hardened proof above (#2), for the tighter
        // `gen:8|class:6|off16:18` layout — the module's own comment above
        // `pack_entry_hardened` names `class == 0x3F` (63) as the one field
        // that can never reach its all-ones value for a real
        // `SMALL_CLASS_COUNT <= 62`; this proof checks the FULL packed word,
        // not just that one field's bound.
        #[kani::proof]
        fn pack_hardened_never_collides_with_ring_slot_empty() {
            let gen: u8 = kani::any();
            let class_idx: u32 = kani::any();
            kani::assume(class_idx < SMALL_CLASS_COUNT as u32);
            let off = any_valid_off();

            let packed = pack_entry_hardened(gen, class_idx, off);
            assert_ne!(packed, RING_SLOT_EMPTY);
        }
    }
}
