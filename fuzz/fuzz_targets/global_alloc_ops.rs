//! libFuzzer target for the Phase 8–11 allocator descent — exercises
//! `sefer_alloc::AllocCore` (the segment substrate that `SeferAlloc` /
//! `GlobalAlloc` is built on) with an `arbitrary`-derived stream of
//! alloc / dealloc / realloc / alloc_zeroed ops of random sizes and
//! alignments, checking the M-invariants from `docs/INVARIANTS.md`:
//!
//! - **M1 (validity):** every returned pointer is non-null (we never exhaust
//!   memory in a bounded run), aligned to the requested align, and writable for
//!   the requested size (write a pattern, read it back).
//! - **M2 (no double-free / UAF):** the model only frees live pointers; a
//!   second `dealloc` of the same pointer is a no-op that must not corrupt the
//!   allocator.
//! - **M3 (no overlap):** two simultaneously-live allocations never share a
//!   byte (checked against every live block, plus a per-block fill that would
//!   detect cross-contamination).
//! - **M4 (alignment & size fidelity):** the class chosen always satisfies the
//!   requested size and align.
//! - **alloc_zeroed contract:** every byte of a zeroed allocation reads as 0.
//! - **realloc:** the `min(old, new)` prefix is preserved.
//!
//! ## Why `AllocCore`, not the installed `SeferAlloc` global allocator
//!
//! `AllocCore` is the single-threaded engine under the `GlobalAlloc` face; it
//! has a plain owned API (`new` / `alloc` / `dealloc` / `realloc` /
//! `alloc_zeroed`) that drops cleanly per fuzz input. Routing the libFuzzer
//! harness's own allocations through the installed process-wide `SeferAlloc`
//! `#[global_allocator]` is out of scope for op-stream invariant fuzzing (the
//! installed path is proven separately by `tests/global_alloc_installed.rs`
//! and `examples/tokio_burn_in.rs`).
//! The cross-thread ordering path is covered by the TSan + aarch64 CI gates and
//! the loom harnesses, not by this single-threaded structure-aware fuzzer.
//!
//! # How to run (Linux only)
//!
//! libFuzzer requires the nightly toolchain and does NOT run on Windows. From
//! the `fuzz/` directory:
//!
//! ```text
//! cargo +nightly fuzz run global_alloc_ops
//! cargo +nightly fuzz run global_alloc_ops -- -max_total_time=3600
//! cargo +nightly fuzz run global_alloc_ops -- artifact.bin
//! ```

// This target drives the allocator's raw-pointer API; the writes/reads through
// returned pointers are inherently `unsafe`. The crate-under-test keeps its own
// `unsafe` confined; the harness only dereferences pointers the allocator just
// handed out for the size it was asked for.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::alloc::Layout;

use arbitrary::Arbitrary;
use sefer_alloc::AllocCore;

/// One operation against the allocator, derived from fuzzer bytes by
/// `arbitrary`. `index` fields are reduced modulo the live count so they are
/// always in range (mirrors `tests/alloc_core_differential.rs`).
#[derive(Arbitrary, Debug)]
enum Op {
    Alloc { size: u32, align_pow: u8 },
    AllocZeroed { size: u32, align_pow: u8 },
    Dealloc(usize),
    Realloc { i: usize, new_size: u32 },
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

/// Bound a fuzzer-derived size into the small/medium range so a single op can't
/// ask the OS for gigabytes (which would just OOM the fuzzer, not find a bug).
/// We span the small free-list classes and a bit past the large threshold.
fn bound_size(raw: u32) -> usize {
    // 1 ..= ~2 MiB: covers small classes and the dedicated-segment large path.
    (raw as usize % (2 * 1024 * 1024)) + 1
}

/// Derive a power-of-two alignment in `[1, SEGMENT/2]` from a fuzzer byte.
///
/// SEGMENT is `1 << 22` (4 MiB). We span `2^0 .. 2^21` — up to but NOT
/// including SEGMENT. This exercises the full `align_up` / large-align
/// arithmetic that #130 hardened: any `align > 4096` with a sub-SEGMENT size
/// misses every small class (`class_for` returns `None`) and routes to
/// `alloc_large`, whose over-reserve + trim math must land a correctly-aligned
/// pointer. `align >= SEGMENT` is deliberately excluded — that is the rejected
/// corridor (`alloc_large` returns null by contract), covered by the unit
/// regression tests, not here (a bounded fuzz run should not spend inputs on a
/// guaranteed null).
fn bound_align(raw: u8) -> usize {
    1usize << (raw % 22) // 2^0 .. 2^21 == 1 .. 2 MiB (< SEGMENT = 4 MiB)
}

fuzz_target!(|data: &[u8]| {
    // Shape the raw bytes into a bounded op stream. Cap the length so a single
    // input cannot OOM the fuzzer with a giant sequence.
    let mut decoder = arbitrary::Unstructured::new(data);
    let iter = match decoder.arbitrary_iter::<Op>() {
        Ok(iter) => iter,
        Err(_) => return, // could not start a stream; skip.
    };
    // Each item is itself a `Result<Op>`; stop at the first decode error and cap
    // the length so a single input can't OOM the fuzzer with a giant sequence.
    let ops: Vec<Op> = iter.take(2048).filter_map(Result::ok).collect();

    let mut alloc = match AllocCore::new() {
        Some(a) => a,
        None => return, // primordial bootstrap failed (OS refused mmap); skip.
    };
    let mut live: Vec<Live> = Vec::new();
    let mut next_fill: u8 = 1;

    for op in ops {
        match op {
            Op::Alloc { size, align_pow } => {
                let size = bound_size(size);
                let align = bound_align(align_pow);
                let layout = Layout::from_size_align(size, align).unwrap();
                let ptr = alloc.alloc(layout);
                assert!(!ptr.is_null(), "M1: alloc returned null");
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
                let ptr = alloc.alloc_zeroed(layout);
                assert!(!ptr.is_null(), "M1: alloc_zeroed returned null");
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
                    alloc.dealloc(l.ptr, layout);
                    // M2: a second dealloc of the same pointer must be a no-op
                    // that does not corrupt the allocator.
                    alloc.dealloc(l.ptr, layout);
                }
            }
            Op::Realloc { i, new_size } => {
                if !live.is_empty() {
                    let i = i % live.len();
                    let new_size = bound_size(new_size);
                    let l = &live[i];
                    let old_layout = Layout::from_size_align(l.size, l.align).unwrap();
                    let (old_ptr, old_size, align, old_fill) = (l.ptr, l.size, l.align, l.fill);
                    let new_ptr = alloc.realloc(old_ptr, old_layout, new_size);
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
                    // M3 contamination checks and later reallocs stay coherent
                    // (the grown tail is legitimately uninitialised otherwise).
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

    // Free all survivors, then drop the allocator (M2: no double-free, no UAF
    // in the registry walk on drop).
    for l in &live {
        let layout = Layout::from_size_align(l.size, l.align).unwrap();
        alloc.dealloc(l.ptr, layout);
    }
    drop(live);
    drop(alloc);
});
