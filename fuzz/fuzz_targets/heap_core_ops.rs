//! libFuzzer target for the **fastbin magazine** path — drives the flagship
//! `production` allocator face (`sefer_alloc::SeferAlloc`, the installed
//! `#[global_allocator]` type) with an `arbitrary`-derived stream of
//! alloc / alloc_zeroed / dealloc / realloc ops of MIXED SMALL size-classes so
//! the per-thread magazine (tcache) fill / flush / refill machinery and its M2
//! oracles get random coverage. It checks the same M-invariants from
//! `docs/INVARIANTS.md` as `global_alloc_ops`:
//!
//! - **M1 (validity):** every returned pointer is non-null, aligned to the
//!   requested align, and writable for the requested size (pattern write +
//!   read-back).
//! - **M2 (no double-free / UAF):** the model only frees live pointers.
//! - **M3 (no overlap):** two simultaneously-live allocations never share a
//!   byte (overlap check + a per-block fill so contamination is caught).
//! - **M4 (alignment & size fidelity):** the returned pointer satisfies the
//!   requested size and align.
//! - **alloc_zeroed contract:** every byte of a zeroed allocation reads as 0.
//! - **realloc:** the `min(old, new)` prefix is preserved.
//!
//! ## Why `SeferAlloc`, not `AllocCore`
//!
//! `AllocCore` (the `global_alloc_ops` target) is the segment substrate BELOW
//! the magazine; it never touches the fastbin/tcache layer. `SeferAlloc` is the
//! `GlobalAlloc` face that, under the `production` (`fastbin`) feature set,
//! routes small allocations through the per-thread magazine — the churn hot
//! path. We call its `GlobalAlloc` methods DIRECTLY (we do NOT install it as the
//! process `#[global_allocator]`; the harness's own allocations keep flowing
//! through the system allocator), so a fuzz input maps to a clean owned op
//! stream against one thread's magazine + heap.
//!
//! This target is single-threaded on purpose: the cross-thread ordering path is
//! covered by the TSan + aarch64 CI gates and the loom harnesses, not by this
//! structure-aware fuzzer.
//!
//! # How to run (Linux only)
//!
//! libFuzzer requires the nightly toolchain and does NOT run on Windows. From
//! the `fuzz/` directory:
//!
//! ```text
//! cargo +nightly fuzz run heap_core_ops
//! cargo +nightly fuzz run heap_core_ops -- -max_total_time=3600
//! cargo +nightly fuzz run heap_core_ops -- artifact.bin
//! ```

// This target drives the allocator's raw-pointer `GlobalAlloc` API; the
// writes/reads through returned pointers are inherently `unsafe`. The
// crate-under-test keeps its own `unsafe` confined; the harness only
// dereferences pointers the allocator just handed out for the size it asked.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::alloc::{GlobalAlloc, Layout};

use arbitrary::Arbitrary;
use sefer_alloc::SeferAlloc;

/// One operation against the allocator, derived from fuzzer bytes by
/// `arbitrary`. `index` fields are reduced modulo the live count so they are
/// always in range (mirrors `global_alloc_ops`).
#[derive(Arbitrary, Debug)]
enum Op {
    Alloc { size: u16, align_pow: u8 },
    AllocZeroed { size: u16, align_pow: u8 },
    Dealloc(usize),
    Realloc { i: usize, new_size: u16 },
}

/// A live allocation in the reference model.
struct Live {
    ptr: *mut u8,
    size: usize,
    align: usize,
    /// The fill byte written across the whole block (detects M3 contamination).
    fill: u8,
}

/// Whether `[a, a+asize)` and `[b, b+bsize)` overlap (touching endpoints are
/// adjacent, not overlapping).
fn ranges_overlap(a: usize, asize: usize, b: usize, bsize: usize) -> bool {
    !(a + asize <= b || b + bsize <= a)
}

/// Bound a fuzzer-derived size into the SMALL / MEDIUM size-class range so the
/// magazine (fastbin) path is the one actually exercised. `u16` input caps the
/// raw value at 65535; we map it into `1 ..= 8192` so allocations span the
/// magazine-managed small classes with a few just past into the medium range,
/// where the fill / flush / refill of the tcache is the hot path under test.
fn bound_size(raw: u16) -> usize {
    (raw as usize % 8192) + 1
}

/// Derive a power-of-two alignment in `[1, 4096]` from a fuzzer byte. Small
/// allocs keep small alignments so the routing stays on the magazine path
/// (large alignments would divert to the dedicated-segment large path, which is
/// the `global_alloc_ops` target's job).
fn bound_align(raw: u8) -> usize {
    1usize << (raw % 13) // 2^0 .. 2^12 == 1 .. 4096
}

fuzz_target!(|data: &[u8]| {
    // Shape the raw bytes into a bounded op stream. Cap the length so a single
    // input cannot OOM the fuzzer with a giant sequence.
    let mut decoder = arbitrary::Unstructured::new(data);
    let iter = match decoder.arbitrary_iter::<Op>() {
        Ok(iter) => iter,
        Err(_) => return, // could not start a stream; skip.
    };
    let ops: Vec<Op> = iter.take(2048).filter_map(Result::ok).collect();

    // The GlobalAlloc face. We call its methods directly rather than installing
    // it as the process allocator: a fuzz input becomes a clean owned op stream
    // against THIS thread's magazine + heap.
    let alloc = SeferAlloc::new();
    let mut live: Vec<Live> = Vec::new();
    let mut next_fill: u8 = 1;

    for op in ops {
        match op {
            Op::Alloc { size, align_pow } => {
                let size = bound_size(size);
                let align = bound_align(align_pow);
                let layout = Layout::from_size_align(size, align).unwrap();
                // SAFETY: valid non-zero-size layout; the returned pointer (if
                // non-null) is ours for `size` bytes.
                let ptr = unsafe { alloc.alloc(layout) };
                if ptr.is_null() {
                    continue; // legal alloc-failure signal; skip.
                }
                assert_eq!((ptr as usize) % align, 0, "M1/M4: pointer not aligned");
                // M3: no overlap with any live allocation.
                for other in &live {
                    assert!(
                        !ranges_overlap(ptr as usize, size, other.ptr as usize, other.size),
                        "M3: new alloc overlaps a live block"
                    );
                }
                let fill = next_fill;
                next_fill = next_fill.wrapping_add(1).max(1);
                // M1: writable for `size`, and read-back matches.
                unsafe {
                    for b in 0..size {
                        ptr.add(b).write(fill);
                    }
                    for b in 0..size {
                        assert_eq!(ptr.add(b).read(), fill, "M1: byte did not read back");
                    }
                }
                live.push(Live { ptr, size, align, fill });
            }
            Op::AllocZeroed { size, align_pow } => {
                let size = bound_size(size);
                let align = bound_align(align_pow);
                let layout = Layout::from_size_align(size, align).unwrap();
                // SAFETY: as in `Alloc`.
                let ptr = unsafe { alloc.alloc_zeroed(layout) };
                if ptr.is_null() {
                    continue;
                }
                assert_eq!((ptr as usize) % align, 0, "M1/M4: not aligned");
                // zero contract: every byte is 0.
                unsafe {
                    for b in 0..size {
                        assert_eq!(ptr.add(b).read(), 0, "alloc_zeroed: byte not zeroed");
                    }
                }
                // Re-fill so later M3 contamination checks have a marker.
                let fill = next_fill;
                next_fill = next_fill.wrapping_add(1).max(1);
                unsafe {
                    for b in 0..size {
                        ptr.add(b).write(fill);
                    }
                }
                live.push(Live { ptr, size, align, fill });
            }
            Op::Dealloc(i) => {
                if !live.is_empty() {
                    let i = i % live.len();
                    let l = live.swap_remove(i);
                    let layout = Layout::from_size_align(l.size, l.align).unwrap();
                    // SAFETY: `l.ptr` was returned by this allocator for exactly
                    // `layout` and is freed once (the model removed it from
                    // `live`, so it is never freed again).
                    unsafe { alloc.dealloc(l.ptr, layout) };
                }
            }
            Op::Realloc { i, new_size } => {
                if !live.is_empty() {
                    let i = i % live.len();
                    let new_size = bound_size(new_size);
                    let l = &live[i];
                    let old_layout = Layout::from_size_align(l.size, l.align).unwrap();
                    let (old_ptr, old_size, align, old_fill) = (l.ptr, l.size, l.align, l.fill);
                    // SAFETY: `old_ptr` was returned for `old_layout`; `new_size`
                    // is non-zero and forms a valid layout with `align`.
                    let new_ptr = unsafe { alloc.realloc(old_ptr, old_layout, new_size) };
                    if new_ptr.is_null() {
                        // Realloc failed: the old block is still live & valid.
                        continue;
                    }
                    assert_eq!((new_ptr as usize) % align, 0, "M1: realloc not aligned");
                    let keep = old_size.min(new_size);
                    // The preserved prefix must still hold the old fill byte.
                    unsafe {
                        for b in 0..keep {
                            assert_eq!(
                                new_ptr.add(b).read(),
                                old_fill,
                                "realloc lost a prefix byte"
                            );
                        }
                    }
                    // Re-establish a fresh fill across the whole new extent so
                    // M3 contamination checks and later reallocs stay coherent.
                    let fill = next_fill;
                    next_fill = next_fill.wrapping_add(1).max(1);
                    unsafe {
                        for b in 0..new_size {
                            new_ptr.add(b).write(fill);
                        }
                    }
                    live[i] = Live { ptr: new_ptr, size: new_size, align, fill };
                }
            }
        }
    }

    // M3 at run end: every survivor still holds its own fill (no block was
    // silently clobbered by another live allocation).
    for l in &live {
        unsafe {
            for b in 0..l.size {
                assert_eq!(l.ptr.add(b).read(), l.fill, "M3: live block clobbered");
            }
        }
    }

    // Free all survivors (M2: no double-free — each survivor freed exactly once).
    for l in &live {
        let layout = Layout::from_size_align(l.size, l.align).unwrap();
        // SAFETY: `l.ptr` was returned for `layout` and is freed exactly once.
        unsafe { alloc.dealloc(l.ptr, layout) };
    }
    drop(live);
});
