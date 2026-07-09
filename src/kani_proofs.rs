//! Kani proof harnesses for `alloc_core::node::Node` and
//! `concurrent::hand::AtomicSlot`.
//!
//! These harnesses are compiled only under `cfg(kani)` and verify the
//! round-trip correctness of the unsafe pointer primitives in `Node` and the
//! publication/eviction protocol of `AtomicSlot`.

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
