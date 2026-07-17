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
