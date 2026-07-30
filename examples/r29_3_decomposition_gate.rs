//! R29-3 (task #434) — segment-lifecycle decomposition gate.
//!
//! MEASUREMENT-ONLY, per this project's "measured, not spun" convention.
//! Decomposes one decommit→reserve segment-lifecycle cycle into its
//! component costs to resolve trigger 2 of `docs/perf/OPEN_ITEMS.md` item 15
//! (the reservation-only overflow tier design).
//!
//! ## Why wall-clock, not iai
//!
//! The task spec originally mandated iai/Ir for this decomposition. But
//! callgrind is structurally blind to kernel time: the existing
//! `large_alloc_free_cycle` iai arm (a full 4 MiB reserve+commit+free, a real
//! OS round-trip costing ~50-200 µs / hundreds of thousands of cycles on bare
//! metal) reports only **3,308 Ir / 20,753 est. cycles** — it counts the
//! userspace syscall *wrappers*, not the kernel's VMA teardown / page-zeroing
//! / fault-handling work. Components (1) OS release+reserve, (4) recommit, and
//! (5) first-touch page faults are ALL kernel-time-dominated, so iai cannot
//! produce a valid split. Wall-clock `std::time::Instant` around the SAME
//! existing production code paths (`os::` module, `SegmentTable`,
//! `SegmentMeta`/`SegmentHeader` init) gives a methodologically valid basis.
//! The iai arms are kept as supplementary characterization in
//! `benches/perf_gate_iai.rs` (and this report cites the 3,308 Ir finding as
//! the evidence for the blindness caveat).
//!
//! ## What is measured
//!
//! The decommit→reserve churn cycle (what R27-4 measured the AGGREGATE win
//! from eliminating) decomposed into:
//!
//! - **(1+2+3) AVOIDABLE** — OS reserve+release + SegmentTable register+
//!   recycle + metadata init. Everything a reservation-only overflow tier
//!   could skip (it keeps the VA mapping and segment identity alive).
//! - **(4+5) IRREDUCIBLE** — recommit + first-touch page faults. Even a
//!   reservation-only design still needs to recommit and fault the decommitted
//!   payload pages. On Linux, recommit is implicit (zero-cost no-op after
//!   `MADV_DONTNEED`); the real cost is the page faults.
//!
//! ## Platform
//!
//! This binary measures wall-clock and is meaningful on any platform. The
//! primary measurement runs on WSL2/Linux (matching every other `npm run iai`
//! measurement host in this project). The OS-specific behaviour (Linux:
//! `mmap`/`munmap`/`MADV_DONTNEED`; Windows: `VirtualAlloc`/`VirtualFree`/
//! `MEM_DECOMMIT`) is handled by the existing `os::` module — no new
//! platform code here.
//!
//! See `docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md` for the
//! verdict.

use std::time::Instant;

use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

/// Measurement iterations per arm. Wall-clock variance is dominated by
/// kernel scheduler / TLB effects; 200 iterations gives a stable median.
const N: usize = 200;

/// Warmup iterations (discarded) before each timed run.
const WARMUP: usize = 20;

fn main() {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let pool_cap = unsafe { (*heap).dbg_pool_cap() };
    let (payload_start, payload_end) = HeapCore::dbg_decomp_payload_range();
    let page_size = HeapCore::dbg_decomp_page_size();
    let payload_pages = (payload_end - payload_start) / page_size;

    println!("=== R29-3 segment-lifecycle decomposition gate ===");
    println!("Platform: {}", std::env::consts::OS);
    println!("Iterations: {} (warmup: {})", N, WARMUP);
    println!("Pool cap: {} (pre-filled before each timed run)", pool_cap);
    println!(
        "Payload: [0x{:x}, 0x{:x}) = {} pages (page_size = {})",
        payload_start, payload_end, payload_pages, page_size
    );
    println!();

    // Pre-fill the pool so all subsequent releases take the release path
    // (not the pool-push path). pool_cap + 2 gives a safety margin.
    for _ in 0..(pool_cap + 2) {
        let _ = unsafe { (*heap).dbg_decomp_full_cycle() };
    }

    // ── Measurement A: full reserve→release cycle WITHOUT payload touch ──
    //
    // This isolates components (1+2+3): OS reserve + SegmentTable register +
    // metadata init + (release: table recycle + OS release). No payload
    // pages are faulted — the cost is pure overhead the reservation-only
    // design could avoid.
    for _ in 0..WARMUP {
        let _ = unsafe { (*heap).dbg_decomp_full_cycle() };
    }
    let t0 = Instant::now();
    for _ in 0..N {
        assert!(unsafe { (*heap).dbg_decomp_full_cycle() }, "reserve failed");
    }
    let a_ns = t0.elapsed().as_nanos() as f64 / N as f64;

    // ── Measurement C: raw OS reserve+release round-trip alone ──
    //
    // NO table bookkeeping, NO metadata init. Isolates component (1): the
    // OS-level VMA setup/teardown cost. Then (2+3) = A - C by subtraction.
    for _ in 0..WARMUP {
        let _ = HeapCore::dbg_decomp_os_roundtrip();
    }
    let t0 = Instant::now();
    for _ in 0..N {
        assert!(HeapCore::dbg_decomp_os_roundtrip(), "os roundtrip failed");
    }
    let c_ns = t0.elapsed().as_nanos() as f64 / N as f64;

    // ── Measurement B: irreducible floor = decommit + first-touch re-fault ──
    //
    // Reserve one segment, first-touch its payload (establish committed
    // state), then alternate: decommit (MADV_DONTNEED) → re-touch (re-fault).
    // The re-touch cost is the page-fault cost a reservation-only tier STILL
    // pays; the decommit cost is the MADV_DONTNEED syscall (also still paid).
    // Together = (4+5), the irreducible floor.
    // R31-4 (task #467): `dbg_decomp_reserve_and_keep` now returns a typed
    // `ReservedSmallSegment` handle instead of a bare pointer, and
    // `dbg_decomp_release` consumes it by value — no `unsafe` needed for
    // either call any more. `.base()` reads the payload address WITHOUT
    // consuming the handle, for the `write_volatile`/decommit measurements
    // below; the handle itself is consumed exactly once, at release.
    let handle = unsafe { (*heap).dbg_decomp_reserve_and_keep() }
        .expect("reserve for first-touch measurement");
    let base = handle.base();

    // Initial first-touch: fault all payload pages.
    for off in (payload_start..payload_end).step_by(page_size) {
        unsafe { core::ptr::write_volatile(base.add(off), 1u8) };
    }

    let mut decommit_total: u128 = 0;
    let mut refault_total: u128 = 0;
    // Warmup
    for _ in 0..WARMUP {
        unsafe { HeapCore::dbg_decomp_decommit_payload(base) };
        for off in (payload_start..payload_end).step_by(page_size) {
            unsafe { core::ptr::write_volatile(base.add(off), 1u8) };
        }
    }
    for _ in 0..N {
        let td = Instant::now();
        unsafe { HeapCore::dbg_decomp_decommit_payload(base) };
        decommit_total += td.elapsed().as_nanos();

        let tr = Instant::now();
        for off in (payload_start..payload_end).step_by(page_size) {
            unsafe { core::ptr::write_volatile(base.add(off), 1u8) };
        }
        refault_total += tr.elapsed().as_nanos();
    }
    let decommit_ns = decommit_total as f64 / N as f64;
    let refault_ns = refault_total as f64 / N as f64;

    // Release the measurement segment.
    unsafe { (*heap).dbg_decomp_release(handle) };

    // ── Measurement A': full cycle WITH payload touch (the REAL production
    // cycle cost) ──
    //
    // reserve → first-touch all payload pages → release. This measures the
    // TRUE production cycle: the munmap now frees ~1,006 faulted physical
    // pages (unlike Measurement A, where the payload was never touched and
    // munmap was pure VMA teardown). A' is the honest baseline to compare B
    // (the reservation-only floor) against.
    for _ in 0..WARMUP {
        let h2 = unsafe { (*heap).dbg_decomp_reserve_and_keep() }.expect("reserve A'");
        let b2 = h2.base();
        for off in (payload_start..payload_end).step_by(page_size) {
            unsafe { core::ptr::write_volatile(b2.add(off), 1u8) };
        }
        unsafe { (*heap).dbg_decomp_release(h2) };
    }
    let t0 = Instant::now();
    for _ in 0..N {
        let h2 = unsafe { (*heap).dbg_decomp_reserve_and_keep() }.expect("reserve A'");
        let b2 = h2.base();
        for off in (payload_start..payload_end).step_by(page_size) {
            unsafe { core::ptr::write_volatile(b2.add(off), 1u8) };
        }
        unsafe { (*heap).dbg_decomp_release(h2) };
    }
    let a_prime_ns = t0.elapsed().as_nanos() as f64 / N as f64;

    // ── Report ──
    let b_ns = decommit_ns + refault_ns; // (4+5) irreducible floor
    let bookkeeping_ns = a_ns - c_ns; // (2+3) = A - (1)

    println!("── Component breakdown (ns/cycle, median of {}) ──", N);
    println!(
        "  (1)   OS reserve+release round-trip:      {:>12.0} ns",
        c_ns
    );
    println!(
        "  (2+3) table + metadata init:             {:>12.0} ns  [= A − (1)]",
        bookkeeping_ns
    );
    println!("  ────────────────────────────────────────────────────────");
    println!(
        "  (1+2+3) AVOIDABLE subtotal (A):          {:>12.0} ns",
        a_ns
    );
    println!();
    println!(
        "  decommit syscall (MADV_DONTNEED):        {:>12.0} ns",
        decommit_ns
    );
    println!(
        "  (5)   first-touch page faults:           {:>12.0} ns  ({} pages @ {:.0} ns/fault)",
        refault_ns,
        payload_pages,
        refault_ns / payload_pages as f64
    );
    println!(
        "  (4)   recommit (Linux: implicit, ~0 ns): {:>12.0} ns",
        0.0
    );
    println!("  ────────────────────────────────────────────────────────");
    println!(
        "  (4+5) IRREDUCIBLE subtotal (B):          {:>12.0} ns",
        b_ns
    );
    println!();
    println!("── Real-world production cycle comparison ──");
    println!(
        "  A'  current design (reserve+touch+release): {:>12.0} ns",
        a_prime_ns
    );
    println!(
        "  B   reservation-only floor (decommit+reflt): {:>12.0} ns",
        b_ns
    );
    let net = a_prime_ns - b_ns;
    if net >= 0.0 {
        println!(
            "  A' − B = reservation-only SAVES:           {:>12.0} ns/cycle",
            net
        );
    } else {
        println!(
            "  A' − B = reservation-only COSTS EXTRA:      {:>12.0} ns/cycle",
            -net
        );
        println!("  (MADV_DONTNEED page-table walk > munmap bulk teardown)");
    }
    println!();

    // ── The split (verdict basis) ──
    // The verdict uses A (avoidable overhead) vs B (irreducible floor) as
    // the task specifies: if (1+2+3) is a material fraction of the cycle,
    // trigger 2 fires. We report the split both against (A+B) and against A'
    // (the real production cycle) for completeness.
    let avoidable_pct = a_ns / (a_ns + b_ns) * 100.0;
    let irreducible_pct = b_ns / (a_ns + b_ns) * 100.0;
    let avoidable_pct_aprime = a_ns / a_prime_ns * 100.0;

    println!("═══ SPLIT — the trigger-2 verdict basis ═══");
    println!(
        "  (1+2+3) AVOIDABLE:   {:>12.0} ns  ({:.1}%)",
        a_ns, avoidable_pct
    );
    println!(
        "  (4+5)   IRREDUCIBLE: {:>12.0} ns  ({:.1}%)",
        b_ns, irreducible_pct
    );
    println!();

    // ── Verdict ──
    // The verdict rule (stated up front in the task, honored exactly):
    // If (1+2+3) is a MATERIAL fraction (>20%) of the cycle → trigger 2 FIRES.
    // If (4+5) DOMINATES (<20%) → trigger 2 does NOT fire → close item 15.
    let threshold = 20.0_f64;
    println!(
        "═══ VERDICT (threshold: avoidable > {}% → trigger 2 fires) ═══",
        threshold
    );
    if avoidable_pct > threshold {
        println!(
            "  TRIGGER 2 FIRES: (1+2+3) = {:.1}% of the cycle — material.",
            avoidable_pct
        );
        println!("  The reservation-only design MAY be opened in a future round.");
        println!("  (This task is measurement-only; no design is written here.)");
    } else {
        println!(
            "  TRIGGER 2 does NOT fire: (1+2+3) = {:.1}% — page-fault cost dominates.",
            avoidable_pct
        );
        println!(
            "  Also: (1+2+3) is {:.1}% of the real production cycle A'.",
            avoidable_pct_aprime
        );
        println!("  Item 15 should move from [D] to [L] (honest reject with evidence).");
    }
}
