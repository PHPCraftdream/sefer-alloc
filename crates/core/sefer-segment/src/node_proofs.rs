// P2.1: proofs bind the canonical crate node (`RootNode`, `pub unsafe`
// forms) directly; no wrapper indirection remains.
use crate::node::RootNode;
use core::ptr::NonNull;

// ── 1. write_next / read_next round-trip ─────────────────────────────

#[kani::proof]
#[allow(unsafe_code)] // canonical pub unsafe RootNode forms (sefer-segment support/node.rs).
fn write_read_next_roundtrip() {
    let mut buf = [0u8; 16];
    let block = NonNull::new(buf.as_mut_ptr()).unwrap();
    let next: *mut u8 = kani::any::<usize>() as *mut u8;
    // SAFETY: `block` addresses the local `[u8; 16]` buffer, which the
    // proof exclusively owns and which exceeds `NODE_SIZE`
    // (`size_of::<*mut u8>()` = 8 on 64-bit) writable/readable bytes
    // for the next-pointer field.
    unsafe {
        RootNode::write_next(block, next);
    }
    // SAFETY: same exclusively owned live buffer as the write above;
    // the first `NODE_SIZE` bytes are initialized by that write.
    let got = unsafe { RootNode::read_next(block) };
    assert_eq!(got, next);
}

// ── 2. deref in-bounds ───────────────────────────────────────────────

#[kani::proof]
#[allow(unsafe_code)] // canonical pub unsafe RootNode forms (sefer-segment support/node.rs).
fn deref_in_bounds() {
    let mut buf = [0u8; 64];
    let base = buf.as_mut_ptr();
    let offset: usize = kani::any();
    kani::assume(offset < 64);
    // SAFETY: `offset < 64` is assumed, so the result stays inside the
    // live `[u8; 64]` buffer with provenance from `base`.
    let result = unsafe { RootNode::deref(base, offset) };
    assert_eq!(result, base.wrapping_add(offset));
}

// ── 3. offset in-bounds ──────────────────────────────────────────────

#[kani::proof]
#[allow(unsafe_code)] // canonical pub unsafe RootNode forms (sefer-segment support/node.rs).
fn offset_in_bounds() {
    let mut buf = [0u8; 64];
    let base = buf.as_mut_ptr();
    let off: usize = kani::any();
    kani::assume(off < 64);
    // SAFETY: `off < 64` is assumed, so the arithmetic stays in-bounds of
    // the live `[u8; 64]` buffer without wrapping past its allocation.
    let result = unsafe { RootNode::offset(base, off) };
    assert_eq!(result, base.wrapping_add(off));
}

// ── 4. zero fills buffer ─────────────────────────────────────────────

#[kani::proof]
#[allow(unsafe_code)] // canonical pub unsafe RootNode forms (sefer-segment support/node.rs).
fn zero_fills_buffer() {
    let mut buf = [0xFFu8; 32];
    // SAFETY: `buf` is a live `[u8; 32]` buffer and `len` is exactly 32 writable bytes.
    unsafe { RootNode::zero(buf.as_mut_ptr(), 32) };
    for i in 0..32 {
        assert_eq!(buf[i], 0);
    }
}

// ── 5. copy_nonoverlapping copies correctly ──────────────────────────

#[kani::proof]
#[allow(unsafe_code)] // canonical pub unsafe RootNode forms (sefer-segment support/node.rs).
fn copy_nonoverlapping_copies() {
    let src: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let mut dst = [0u8; 8];
    // SAFETY: `src` and `dst` are distinct local `[u8; 8]` buffers and
    // `count` is exactly 8 bytes in each live range.
    unsafe { RootNode::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), 8) };
    for i in 0..8 {
        assert_eq!(dst[i], src[i]);
    }
}

// ── 6. write_u8 / read_u8 round-trip ─────────────────────────────────

#[kani::proof]
#[allow(unsafe_code)] // canonical pub unsafe RootNode forms (sefer-segment support/node.rs).
fn write_read_u8_roundtrip() {
    let mut buf = [0u8; 1];
    let val: u8 = kani::any();
    // SAFETY: `buf` is one live, exclusively writable byte.
    unsafe { RootNode::write_u8(buf.as_mut_ptr(), val) };
    // SAFETY: same live byte, initialized by the write above.
    let got = unsafe { RootNode::read_u8(buf.as_ptr()) };
    assert_eq!(got, val);
}

// ── 7. write_u32_unaligned / read_u32_unaligned round-trip ───────────

#[kani::proof]
#[allow(unsafe_code)] // P2.1: canonical pub unsafe RootNode forms (crate support/node.rs).
fn write_read_u32_roundtrip() {
    let mut buf = [0u8; 4];
    let val: u32 = kani::any();
    let ptr = buf.as_mut_ptr() as *mut u32;
    // SAFETY: `ptr` addresses the local `[u8; 4]` buffer (4 writable/readable bytes).
    unsafe {
        RootNode::write_u32_unaligned(ptr, val);
        // SAFETY: same 4-byte local buffer, initialized by the write above.
        let got = RootNode::read_u32_unaligned(ptr as *const u32);
        assert_eq!(got, val);
    }
}

// ── 8. write_struct / read_struct round-trip ─────────────────────────

#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(C)]
struct Small {
    a: u16,
    b: u32,
}

#[kani::proof]
#[allow(unsafe_code)]
fn write_read_struct_roundtrip() {
    let a: u16 = kani::any();
    let b: u32 = kani::any();
    let val = Small { a, b };

    // Use an aligned buffer large enough for `Small`.
    let mut storage = core::mem::MaybeUninit::<Small>::uninit();
    let ptr = storage.as_mut_ptr();
    // SAFETY: `ptr` is live, aligned, exclusively writable `Small`
    // storage owned by this proof; `RootNode::write_struct` above fully
    // initialized it to `val` before this read.
    unsafe { RootNode::write_struct(ptr, val) };
    let got = unsafe { RootNode::read_struct(ptr as *const Small) };
    assert_eq!(got, val);
}

// ── 9. write_usize / read_usize round-trip ──────────────────────────

#[kani::proof]
#[allow(unsafe_code)] // canonical pub unsafe RootNode forms (sefer-segment support/node.rs).
fn write_read_usize_roundtrip() {
    let mut storage = 0usize;
    let val: usize = kani::any();
    let ptr = &mut storage as *mut usize;
    // SAFETY: `storage` is a live, aligned `usize` variable.
    unsafe { RootNode::write_usize(ptr, val) };
    // SAFETY: same aligned live storage, initialized by the write above.
    let got = unsafe { RootNode::read_usize(ptr as *const usize) };
    assert_eq!(got, val);
}
