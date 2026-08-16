//! `bench-scale-tool` fixed-iteration benches for `aligned-vmem` (task #758).
//! This crate previously had zero benches of its own despite having the most
//! churn (20+ commits) in the workspace.
//!
//! Run:
//! ```text
//! cargo bench -p aligned-vmem --bench vmem_bench -- --calibrate 1
//! cargo bench -p aligned-vmem --bench vmem_bench
//! ```
//!
//! All workloads use REAL OS syscalls (mmap/madvise on Unix, VirtualAlloc/
//! MEM_DECOMMIT on Windows). Unlike numa-shim, no mock backend is needed —
//! these operations work deterministically on any platform without special
//! hardware.
//!
//! Each iteration is a complete self-contained cycle (reserve → ... → release)
//! to avoid VA space exhaustion during the millions-of-iterations calibration.
//!
//! # Iteration count manifest gap (task #850, V19)
//!
//! This crate's iteration counts are stored in `<workspace-root>/bench-iters.txt`,
//! not within `crates/vmem/`. When running from a published tarball (e.g., after
//! `cargo install aligned-vmem` then `cargo bench -p aligned-vmem`), the harness
//! cannot find the workspace-level manifest and falls back to JIT-calibrating
//! each workload at 1 second. This produces different iteration counts than the
//! in-repo run that uses the cached manifest entries. Within the workspace,
//! the workspace-level `bench-iters.txt` is canonical — see sefer-region's own
//! documentation (`crates/region/benches/region_bench.rs`) for the same pattern
//! and its history (task #820).

use std::hint::black_box;

use aligned_vmem::{decommit, recommit, reserve_aligned};
use bench_scale_tool::Harness;

/// Size for each reservation (64 KiB = 16 pages on 4 KiB systems).
///
/// Small enough that millions of iterations won't exhaust the process's
/// virtual address space, large enough to amortize syscall overhead and
/// approximate real allocator segment sizes.
const RESERVE_SIZE: usize = 64 * 1024;

/// Alignment for each reservation (same as size for simplicity).
///
/// Must be a power of two >= PAGE; using the same value as size keeps the
/// benchmark focused on the syscall cost, not alignment negotiation.
const RESERVE_ALIGN: usize = RESERVE_SIZE;

/// Helper: perform a complete reserve → release cycle.
fn reserve_release_cycle() {
    let r = black_box(reserve_aligned(
        black_box(RESERVE_SIZE),
        black_box(RESERVE_ALIGN),
    ));
    match r {
        Some(_reservation) => {} // Reservation dropped via RAII -> release
        None => panic!(
            "reserve_release_cycle: reserve_aligned failed (OOM or contract violation) -- benchmark aborted, not measuring a failed reservation"
        ),
    }
}

/// Helper: perform a reserve → decommit → release cycle.
///
/// Decomits the entire range immediately after reservation, measuring the
/// full lifecycle including decommit.
fn reserve_decommit_release_cycle() {
    let r = reserve_aligned(RESERVE_SIZE, RESERVE_ALIGN);
    match r {
        Some(reservation) => {
            let base = black_box(reservation.as_ptr());
            let len = black_box(reservation.len());
            // SAFETY: `base` is from a live reservation, [base, base+len) is within bounds.
            unsafe {
                // Decommit the entire range.
                decommit(base, 0, len);
            }
            // Reservation dropped -> release.
        }
        None => panic!(
            "reserve_decommit_release_cycle: reserve_aligned failed (OOM or contract violation) -- benchmark aborted, not measuring a failed reservation"
        ),
    }
}

/// Helper: perform a reserve → decommit → recommit → release cycle.
///
/// Measures the full churn pattern: allocate, decommit to return physical
/// memory to the OS, then recommit when the memory is needed again.
fn reserve_decommit_recommit_release_cycle() {
    let r = reserve_aligned(RESERVE_SIZE, RESERVE_ALIGN);
    match r {
        Some(reservation) => {
            let base = black_box(reservation.as_ptr());
            let len = black_box(reservation.len());
            // SAFETY: `base` is from a live reservation, [base, base+len) is within bounds.
            unsafe {
                // Decommit the entire range.
                decommit(base, 0, len);
                // Recommit it (no-op on Unix/miri, but measured on Windows).
                let ok = recommit(base, 0, len);
                // On Windows, recommit can fail due to commit-charge exhaustion.
                // This is a failure worth aborting the benchmark for.
                if !ok {
                    panic!(
                        "reserve_decommit_recommit_release_cycle: recommit failed (commit-charge exhaustion) -- benchmark aborted, not measuring a failed recommit"
                    );
                }
            }
            // Reservation dropped -> release.
        }
        None => panic!(
            "reserve_decommit_recommit_release_cycle: reserve_aligned failed (OOM or contract violation) -- benchmark aborted, not measuring a failed reservation"
        ),
    }
}

/// Helper: fault all pages in a reservation by writing to each page.
///
/// This ensures pages are backed by physical memory and have PTE entries,
/// making the subsequent decommit cost realistic (measuring the cost of
/// tearing down PTEs and TLB entries, not the cheap never-faulted path).
#[inline]
unsafe fn fault_pages(base: *mut u8, len: usize) {
    use std::ptr;
    // Write one byte to each real OS page to fault it in -- use the runtime
    // page_size(), not a hardcoded 4 KiB: on a host with a larger real page
    // size (e.g. 16 KiB on Apple Silicon macOS), stepping by 4 KiB would
    // still touch every page (over-faulting is harmless here), but using the
    // real page size keeps this consistent with the rest of the crate's own
    // page_size()-vs-PAGE discipline.
    let page_size = aligned_vmem::page_size();
    for offset in (0..len).step_by(page_size) {
        // SAFETY: `base` is from a live reservation, [base, base+offset) is within bounds.
        let ptr = unsafe { base.add(offset) };
        // SAFETY: `ptr` is within a committed region from a live reservation.
        unsafe { ptr::write_volatile(ptr, 0xAB) };
    }
}

/// Helper: perform a reserve → fault → decommit → release cycle.
///
/// Measures decommit cost on *dirty* (touched) pages, which have real PTEs
/// to tear down. This is the realistic allocator-churn pattern; the
/// cold/never-touched version (`reserve_decommit_release_cycle`) measures
/// the cheap path (no PTEs, no TLB shootdown).
fn reserve_fault_decommit_release_cycle() {
    let r = reserve_aligned(RESERVE_SIZE, RESERVE_ALIGN);
    match r {
        Some(reservation) => {
            let base = black_box(reservation.as_ptr());
            let len = black_box(reservation.len());
            // SAFETY: `base` is from a live reservation, [base, base+len) is within bounds.
            unsafe {
                // Fault all pages by writing to them.
                fault_pages(base, len);
                // Now decommit the dirty pages (realistic cost).
                decommit(base, 0, len);
            }
            // Reservation dropped -> release.
        }
        None => panic!(
            "reserve_fault_decommit_release_cycle: reserve_aligned failed (OOM or contract violation) -- benchmark aborted, not measuring a failed reservation"
        ),
    }
}

/// Helper: perform a reserve → fault → decommit → recommit → release cycle.
///
/// Measures the full realistic churn pattern on dirty pages: allocate,
/// fault to get real backing, decommit to return physical memory, then
/// recommit when needed again.
fn reserve_fault_decommit_recommit_release_cycle() {
    let r = reserve_aligned(RESERVE_SIZE, RESERVE_ALIGN);
    match r {
        Some(reservation) => {
            let base = black_box(reservation.as_ptr());
            let len = black_box(reservation.len());
            // SAFETY: `base` is from a live reservation, [base, base+len) is within bounds.
            unsafe {
                // Fault all pages by writing to them.
                fault_pages(base, len);
                // Decommit the dirty pages (realistic cost).
                decommit(base, 0, len);
                // Recommit it (no-op on Unix/miri, but measured on Windows).
                let ok = recommit(base, 0, len);
                // On Windows, recommit can fail due to commit-charge exhaustion.
                // This is a failure worth aborting the benchmark for.
                if !ok {
                    panic!(
                        "reserve_fault_decommit_recommit_release_cycle: recommit failed (commit-charge exhaustion) -- benchmark aborted, not measuring a failed recommit"
                    );
                }
            }
            // Reservation dropped -> release.
        }
        None => panic!(
            "reserve_fault_decommit_recommit_release_cycle: reserve_aligned failed (OOM or contract violation) -- benchmark aborted, not measuring a failed reservation"
        ),
    }
}

fn main() {
    let mut h = Harness::new("vmem_bench", env!("CARGO_MANIFEST_DIR"));

    // ── Reserve only ─────────────────────────────────────────────────────

    h.bench("reserve_release", || {
        reserve_release_cycle();
    });

    // ── Reserve → Decommit → Release ──────────────────────────────────────

    h.bench("reserve_decommit_release", || {
        reserve_decommit_release_cycle();
    });

    // ── Reserve → Decommit → Recommit → Release ───────────────────────────

    h.bench("reserve_decommit_recommit_release", || {
        reserve_decommit_recommit_release_cycle();
    });

    // ── Reserve → Fault → Decommit → Release (dirty/touched pages) ────────
    //
    // Measures decommit cost on pages that have been faulted in (realistic
    // allocator churn: decommitting pages that have actually been used).
    // The cold/never-touched version above measures the cheap path (no PTEs).

    h.bench("reserve_fault_decommit_release", || {
        reserve_fault_decommit_release_cycle();
    });

    // ── Reserve → Fault → Decommit → Recommit → Release (dirty/touched) ───

    h.bench("reserve_fault_decommit_recommit_release", || {
        reserve_fault_decommit_recommit_release_cycle();
    });

    // ── Larger allocation (1 MiB) — reserve only ──────────────────────────
    //
    // Measures whether syscall cost scales linearly with size or is dominated
    // by fixed overhead (alignment negotiation, kernel bookkeeping).

    const LARGE_SIZE: usize = 1024 * 1024;
    const LARGE_ALIGN: usize = LARGE_SIZE;

    h.bench("reserve_release_1mb", || {
        let r = black_box(reserve_aligned(
            black_box(LARGE_SIZE),
            black_box(LARGE_ALIGN),
        ));
        match r {
            Some(_reservation) => {}
            None => panic!(
                "reserve_release_1mb: reserve_aligned failed (OOM or contract violation) -- benchmark aborted, not measuring a failed reservation"
            ),
        }
    });

    h.run();
}
