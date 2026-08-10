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
///
/// Returns true on success, false on OOM (rare during a bench run but
/// handled to avoid panics).
fn reserve_release_cycle() -> bool {
    let r = black_box(reserve_aligned(
        black_box(RESERVE_SIZE),
        black_box(RESERVE_ALIGN),
    ));
    match r {
        Some(_reservation) => true, // Reservation dropped via RAII -> release
        None => false,              // OOM or contract violation
    }
}

/// Helper: perform a reserve → decommit → release cycle.
///
/// Decomits the entire range immediately after reservation, measuring the
/// full lifecycle including decommit.
fn reserve_decommit_release_cycle() -> bool {
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
            true
        }
        None => false,
    }
}

/// Helper: perform a reserve → decommit → recommit → release cycle.
///
/// Measures the full churn pattern: allocate, decommit to return physical
/// memory to the OS, then recommit when the memory is needed again.
fn reserve_decommit_recommit_release_cycle() -> bool {
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
                // We treat it as a failed iteration rather than panicking.
                if !ok {
                    return false;
                }
            }
            // Reservation dropped -> release.
            true
        }
        None => false,
    }
}

fn main() {
    let mut h = Harness::new("vmem_bench", env!("CARGO_MANIFEST_DIR"));

    // ── Reserve only ─────────────────────────────────────────────────────

    h.bench("reserve_release", || {
        let ok = black_box(reserve_release_cycle());
        // Prevent the result from being optimized away.
        black_box(ok);
    });

    // ── Reserve → Decommit → Release ──────────────────────────────────────

    h.bench("reserve_decommit_release", || {
        let ok = black_box(reserve_decommit_release_cycle());
        black_box(ok);
    });

    // ── Reserve → Decommit → Recommit → Release ───────────────────────────

    h.bench("reserve_decommit_recommit_release", || {
        let ok = black_box(reserve_decommit_recommit_release_cycle());
        black_box(ok);
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
            Some(_reservation) => {
                black_box(true);
            }
            None => {
                black_box(false);
            }
        }
    });

    h.run();
}
