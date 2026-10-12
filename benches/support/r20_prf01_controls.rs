//! R20 PRF-01 v2 control-geometry cells (Ir gate support; NO counter reads).
//!
//! Entry point: `SeferAlloc` `GlobalAlloc` -> `HeapCore`, routed — the same
//! layer as `benches/perf_gate_iai.rs`. These are SEMANTIC COPIES of the four
//! original hot-control bodies (`small_churn_16b`, `churn_256b`,
//! `churn_write_256b`, `aligned_churn_640b_a128` in `benches/perf_gate_iai.rs`)
//! plus the direct construction cell, parameterised over the op count so each
//! family has its N / 2N / 4N siblings
//! (`docs/perf/R20_CONTROL_GEOMETRY_RULE.md` item 5). `perf_gate_iai.rs` is
//! over its 1000-line structure budget and must not grow, so this is a
//! separate iai target (`r20_prf01_controls_iai`).
//!
//! Body correspondence with the originals, verified by reading their source
//! (see the v2 preregistration):
//! - allocator: `SeferAlloc::new()` per cell body, exactly as each original
//!   (the construction cost — first sidecar registration — is inside every
//!   family body and is bounded separately by the start cell);
//! - loop: `for _ in 0..ops` with `alloc` -> `black_box(ptr)` -> null check ->
//!   `dealloc`, identical statement order and identical safety comments;
//! - sizes/layouts: 16 B @ 8, 256 B @ 8 (x2 families), 640 B @ 128;
//! - `churn_write_256b` keeps the two `write_volatile` stores of
//!   `0xA5A5_A5A5_A5A5_A5A5` after each non-null alloc;
//! - op counts: N = 64 (= `CHURN_OPS`), 2N = 128 (= `CHURN_OPS_2N`), 4N = 256.
//!   The `2n` bodies are byte-equivalent copies of the existing
//!   `small_churn_16b_2n` pattern (only the op count differs).

use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;

/// Op count of the N cell (identical to `CHURN_OPS` in `perf_gate_iai.rs`).
pub const PRF01_N: usize = 64;
/// Op count of the 2N cell (identical to `CHURN_OPS_2N`).
pub const PRF01_2N: usize = 128;
/// Op count of the 4N cell.
pub const PRF01_4N: usize = 256;

/// `small_churn_16b` family: 16 B @ align 8 alloc+dealloc churn.
pub fn small_churn_16b(ops: usize) {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    for _ in 0..ops {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

/// `churn_256b` family: 256 B @ align 8 alloc+dealloc churn.
pub fn churn_256b(ops: usize) {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(256, 8).unwrap();
    for _ in 0..ops {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

/// `churn_write_256b` family: 256 B churn with the two volatile stores.
pub fn churn_write_256b(ops: usize) {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(256, 8).unwrap();
    for _ in 0..ops {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr is a freshly allocated 256 B block; the first 16
            // bytes are in bounds and writable. `write_volatile` prevents the
            // stores being elided.
            unsafe { core::ptr::write_volatile(ptr.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5) };
            unsafe { core::ptr::write_volatile(ptr.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5) };
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

/// `aligned_churn_640b_a128` family: 640 B @ align 128 churn.
pub fn aligned_churn_640b_a128(ops: usize) {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(640, 128).unwrap();
    for _ in 0..ops {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

/// The DIRECT construction cell: `SeferAlloc::new()` plus exactly ONE
/// alloc/free pair of 16 B (the first sidecar registration). Its raw Ir is
/// the construction measurement; no intercept is ever derived from the
/// churn families.
pub fn start_one_alloc_free_16b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
    let ptr = unsafe { sefer.alloc(layout) };
    black_box(ptr);
    if !ptr.is_null() {
        // SAFETY: ptr was returned by the immediately preceding `alloc` call
        // with the same layout.
        unsafe { sefer.dealloc(ptr, layout) };
    }
}
