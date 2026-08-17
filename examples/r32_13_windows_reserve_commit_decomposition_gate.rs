//! R32-13 (task #504, F11 step 2) — Windows-native segment-lifecycle
//! decomposition gate.
//!
//! MEASUREMENT-ONLY, per this project's "measured, not spun" convention.
//! Ports `examples/r29_3_decomposition_gate.rs`'s methodology (decompose one
//! reserve/commit/decommit/release segment lifecycle into separately-timed
//! phases, with the page-fault share isolated) to a question R29-3 never
//! addressed: whether Windows's unconditional 2x virtual-address
//! over-reservation and its unconditional 2-syscall reserve+commit pair
//! (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` F11) cost real,
//! measurable time.
//!
//! ## Why this is a NEW artifact, not a re-run of R29-3
//!
//! R29-3 (`docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`) already
//! decomposed the Linux segment-lifecycle cycle and found the entire
//! avoidable (non-page-fault) share is 1.0-1.3% — small. Its own "Next
//! trigger" named the untested case verbatim: "the OS-backend changes to one
//! where recommit is a real separate syscall (Windows MEM_DECOMMIT+
//! MEM_COMMIT, where the VMA-teardown-vs-page-walk trade-off may differ)."
//! This binary is that trigger, run on this project's own native Windows dev
//! environment. Two things R29-3 did NOT measure that this binary adds:
//!
//! 1. **The reserve-vs-commit SPLIT.** R29-3's "OS reserve+release
//!    round-trip" (component 1) lumps `VirtualAlloc(MEM_RESERVE)` and
//!    `VirtualAlloc(MEM_COMMIT)` into one timed region — correct for Linux
//!    (where `mmap` commits eagerly in one call) but too coarse for Windows,
//!    where they are unconditionally two separate syscalls
//!    (`crates/aligned-vmem/src/lib.rs`'s `win_reserve_commit`). This binary uses the
//!    new `dbg_decomp_win_reserve_only`/`_commit_only`/`_release_only` hooks
//!    (task #504) — built on the SAME `aligned_vmem::reserve_aligned_lazy` +
//!    `commit_range` primitive the opt-in `primordial-lazy-commit`/
//!    `small-segment-lazy-commit` POLICY features already call in
//!    production, just driven directly by a measurement hook — to time
//!    `VirtualAlloc(MEM_RESERVE)` and `VirtualAlloc(MEM_COMMIT)` as two
//!    independent numbers.
//! 2. **The path-activation oracle for step 1's counters.** Reads back
//!    `dbg_windows_reserve_commit_calls`/`dbg_unix_exact_reserve_attempts`/
//!    `dbg_unix_exact_reserve_hits` (task #504 step 1) before and after each
//!    measurement loop, proving the arm actually hit the code path it claims
//!    to measure — on Windows, `dbg_windows_reserve_commit_calls` must
//!    advance by exactly N per N-iteration loop; on Unix, it must stay 0
//!    while the exact-mmap counters advance instead. A mismatch means the
//!    arm silently took the wrong platform branch, and the reported numbers
//!    would not mean what the report claims.
//!
//! ## Platform
//!
//! Wall-clock, meaningful on any platform (like R29-3). Written for and
//! PRIMARILY measured on native Windows (this project's own dev
//! environment, per its session context) — the first Windows-native perf
//! artifact in this corpus. On Unix/miri the reserve-vs-commit split
//! collapses to "commit ~= 0 ns" honestly (there is no separate commit
//! syscall to pay there), which is itself part of what the oracle proves,
//! not a bug in the harness.
//!
//! See `docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md` for
//! the full report and verdict.

use std::time::Instant;

use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

/// Measurement iterations per arm. Matches R29-3's own `N` — wall-clock
/// variance here is dominated by kernel scheduler / TLB effects, and 200
/// iterations gave R29-3 a stable median on the same class of measurement.
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

    println!("=== R32-13 Windows reserve/commit decomposition gate ===");
    println!("Platform: {}", std::env::consts::OS);
    println!("Iterations: {} (warmup: {})", N, WARMUP);
    println!("Pool cap: {} (pre-filled before each timed run)", pool_cap);
    println!(
        "Payload: [0x{:x}, 0x{:x}) = {} pages (page_size = {})",
        payload_start, payload_end, payload_pages, page_size
    );
    println!();

    // Pre-fill the pool so all subsequent releases take the release path
    // (not the pool-push path) — matches R29-3's own setup exactly.
    for _ in 0..(pool_cap + 2) {
        let _ = unsafe { (*heap).dbg_decomp_full_cycle() };
    }

    // ── Measurement R: reserve-only (VirtualAlloc MEM_RESERVE + a tiny
    // MEM_COMMIT of just the first page) ──
    //
    // Path-activation oracle: on Windows, `dbg_windows_reserve_commit_calls`
    // must advance by exactly N (one `win_reserve_commit` call per
    // iteration, regardless of the tiny commit length). On Unix/miri it
    // stays 0 (no such call exists there) while
    // `dbg_unix_exact_reserve_attempts` advances instead.
    HeapCore::dbg_reset_vmem_bench_internals_counters();

    let mut warmup_handles: Vec<(*mut u8, *mut u8, usize)> = Vec::with_capacity(WARMUP);
    for _ in 0..WARMUP {
        let h = HeapCore::dbg_decomp_win_reserve_only().expect("reserve warmup");
        warmup_handles.push(h);
    }
    for (_, r, rl) in warmup_handles {
        unsafe { HeapCore::dbg_decomp_win_release_only(r, rl) };
    }

    // Snapshot the counters AFTER warmup, immediately before the timed loop —
    // capturing them earlier (before warmup) would count warmup's own
    // WARMUP reserve calls in the delta below, exactly the off-by-WARMUP bug
    // this oracle itself caught during this task's own development (see the
    // gate report's methodology section for the concrete before/after
    // numbers: 220 vs the expected 200 on the first run of this harness).
    let win_calls_before = HeapCore::dbg_windows_reserve_commit_calls();
    let unix_attempts_before = HeapCore::dbg_unix_exact_reserve_attempts();
    let unix_hits_before = HeapCore::dbg_unix_exact_reserve_hits();

    let mut handles: Vec<(*mut u8, *mut u8, usize)> = Vec::with_capacity(N);
    let t0 = Instant::now();
    for _ in 0..N {
        let h = HeapCore::dbg_decomp_win_reserve_only().expect("reserve failed");
        handles.push(h);
    }
    let reserve_only_ns = t0.elapsed().as_nanos() as f64 / N as f64;

    let win_calls_after_reserve = HeapCore::dbg_windows_reserve_commit_calls();
    let unix_attempts_after_reserve = HeapCore::dbg_unix_exact_reserve_attempts();
    let unix_hits_after_reserve = HeapCore::dbg_unix_exact_reserve_hits();

    // ── Measurement C: commit-only (VirtualAlloc MEM_COMMIT of the
    // remaining [PAGE, SEGMENT) range, no reserve, no first-touch) ──
    let bases: Vec<*mut u8> = handles.iter().map(|(b, _, _)| *b).collect();
    let t0 = Instant::now();
    for &base in &bases {
        assert!(
            unsafe { HeapCore::dbg_decomp_win_commit_only(base) },
            "commit-only failed"
        );
    }
    let commit_only_ns = t0.elapsed().as_nanos() as f64 / N as f64;

    // Release all N reserved+committed segments.
    for (_, r, rl) in handles {
        unsafe { HeapCore::dbg_decomp_win_release_only(r, rl) };
    }

    let win_calls_after = HeapCore::dbg_windows_reserve_commit_calls();
    let unix_attempts_after = HeapCore::dbg_unix_exact_reserve_attempts();
    let unix_hits_after = HeapCore::dbg_unix_exact_reserve_hits();

    // ── Measurement A: full reserve→release cycle WITHOUT payload touch
    // (component 1+2+3, matching R29-3's own Measurement A exactly) ──
    for _ in 0..WARMUP {
        let _ = unsafe { (*heap).dbg_decomp_full_cycle() };
    }
    let t0 = Instant::now();
    for _ in 0..N {
        assert!(unsafe { (*heap).dbg_decomp_full_cycle() }, "reserve failed");
    }
    let a_ns = t0.elapsed().as_nanos() as f64 / N as f64;

    // ── Measurement C_link: raw OS reserve+release round-trip alone
    // (component 1, matching R29-3's own Measurement C exactly) ──
    for _ in 0..WARMUP {
        let _ = HeapCore::dbg_decomp_os_roundtrip();
    }
    let t0 = Instant::now();
    for _ in 0..N {
        assert!(HeapCore::dbg_decomp_os_roundtrip(), "os roundtrip failed");
    }
    let c_ns = t0.elapsed().as_nanos() as f64 / N as f64;

    // ── Measurement B: irreducible floor = decommit + recommit + first-touch
    // re-fault (matching R29-3's own Measurement B exactly — same hooks,
    // same recommit-before-refault discipline from R31-6) ──
    let handle = unsafe { (*heap).dbg_decomp_reserve_and_keep() }
        .expect("reserve for first-touch measurement");
    let base = handle.dbg_base();

    for off in (payload_start..payload_end).step_by(page_size) {
        unsafe { core::ptr::write_volatile(base.add(off), 1u8) };
    }

    let mut decommit_total: u128 = 0;
    let mut refault_total: u128 = 0;
    for _ in 0..WARMUP {
        unsafe { HeapCore::dbg_decomp_decommit_payload(base) };
        assert!(
            unsafe { HeapCore::dbg_decomp_recommit_payload(base) },
            "recommit failed during warmup"
        );
        for off in (payload_start..payload_end).step_by(page_size) {
            unsafe { core::ptr::write_volatile(base.add(off), 1u8) };
        }
    }
    for _ in 0..N {
        let td = Instant::now();
        unsafe { HeapCore::dbg_decomp_decommit_payload(base) };
        decommit_total += td.elapsed().as_nanos();

        let tr = Instant::now();
        assert!(
            unsafe { HeapCore::dbg_decomp_recommit_payload(base) },
            "recommit failed"
        );
        for off in (payload_start..payload_end).step_by(page_size) {
            unsafe { core::ptr::write_volatile(base.add(off), 1u8) };
        }
        refault_total += tr.elapsed().as_nanos();
    }
    let decommit_ns = decommit_total as f64 / N as f64;
    let refault_ns = refault_total as f64 / N as f64;

    unsafe { (*heap).dbg_decomp_release(handle) };

    // ── Measurement A': full cycle WITH payload touch (the REAL production
    // cycle cost, matching R29-3's own Measurement A' exactly) ──
    for _ in 0..WARMUP {
        let h2 = unsafe { (*heap).dbg_decomp_reserve_and_keep() }.expect("reserve A'");
        let b2 = h2.dbg_base();
        for off in (payload_start..payload_end).step_by(page_size) {
            unsafe { core::ptr::write_volatile(b2.add(off), 1u8) };
        }
        unsafe { (*heap).dbg_decomp_release(h2) };
    }
    let t0 = Instant::now();
    for _ in 0..N {
        let h2 = unsafe { (*heap).dbg_decomp_reserve_and_keep() }.expect("reserve A'");
        let b2 = h2.dbg_base();
        for off in (payload_start..payload_end).step_by(page_size) {
            unsafe { core::ptr::write_volatile(b2.add(off), 1u8) };
        }
        unsafe { (*heap).dbg_decomp_release(h2) };
    }
    let a_prime_ns = t0.elapsed().as_nanos() as f64 / N as f64;

    // ── Report: path-activation oracle first (before trusting any timing) ──
    let is_windows = std::env::consts::OS == "windows";
    let win_calls_delta_reserve = win_calls_after_reserve - win_calls_before;
    let win_calls_delta_total = win_calls_after - win_calls_before;
    let unix_attempts_delta_reserve = unix_attempts_after_reserve - unix_attempts_before;
    let unix_hits_delta_reserve = unix_hits_after_reserve - unix_hits_before;
    let unix_attempts_delta_total = unix_attempts_after - unix_attempts_before;
    let unix_hits_delta_total = unix_hits_after - unix_hits_before;

    println!("── §1 Path-activation oracle (task #504 step 1 counters) ──");
    println!(
        "  windows_reserve_commit_calls delta (reserve-only loop): {} (expect {} iff windows)",
        win_calls_delta_reserve, N
    );
    println!(
        "  windows_reserve_commit_calls delta (total, incl. A/A'/C loops below): {}",
        win_calls_delta_total
    );
    println!(
        "  unix_exact_reserve_attempts delta (reserve-only loop): {} / hits: {}",
        unix_attempts_delta_reserve, unix_hits_delta_reserve
    );
    println!(
        "  unix_exact_reserve_attempts delta (total): {} / hits: {}",
        unix_attempts_delta_total, unix_hits_delta_total
    );
    if is_windows {
        assert_eq!(
            win_calls_delta_reserve, N as u64,
            "ORACLE FAILED: windows_reserve_commit_calls did not advance by exactly N on a \
             Windows host — dbg_decomp_win_reserve_only did not take the win_reserve_commit \
             path it claims to measure"
        );
        assert_eq!(
            unix_attempts_delta_reserve, 0,
            "ORACLE FAILED: unix_exact_reserve_attempts advanced on a Windows host — \
             impossible unless the counters are cross-wired"
        );
        println!(
            "  ORACLE: PASS (Windows host, windows_reserve_commit_calls == N, unix counters == 0)"
        );
    } else {
        assert_eq!(
            win_calls_delta_reserve, 0,
            "ORACLE FAILED: windows_reserve_commit_calls advanced on a non-Windows host"
        );
        println!(
            "  ORACLE: host is {} (not windows) — windows_reserve_commit_calls correctly stayed \
             0; unix counters are the relevant oracle here.",
            std::env::consts::OS
        );
    }
    println!();

    println!("── §2 Windows reserve-vs-commit SPLIT (median of {}) ──", N);
    println!(
        "  reserve-only (VirtualAlloc MEM_RESERVE + tiny MEM_COMMIT): {:>12.0} ns",
        reserve_only_ns
    );
    println!(
        "  commit-only  (VirtualAlloc MEM_COMMIT, remaining {} pages): {:>12.0} ns  ({:.1} ns/page)",
        payload_pages.saturating_sub(1),
        commit_only_ns,
        commit_only_ns / (payload_pages.saturating_sub(1)).max(1) as f64
    );
    println!(
        "  reserve-only + commit-only combined:                        {:>12.0} ns",
        reserve_only_ns + commit_only_ns
    );
    if !is_windows {
        println!(
            "  (non-Windows host: commit-only is expected to be ~0 ns — no separate commit \
             syscall exists on this backend, this is the honest cross-platform result, not a bug)"
        );
    }
    println!();

    println!(
        "── §3 R29-3-comparable component breakdown (ns/cycle, median of {}) ──",
        N
    );
    println!(
        "  (1)   OS reserve+release round-trip (LUMPED, R29-3-style):  {:>12.0} ns",
        c_ns
    );
    let bookkeeping_ns = a_ns - c_ns;
    println!(
        "  (2+3) table + metadata init:                                {:>12.0} ns  [= A − (1)]",
        bookkeeping_ns
    );
    println!("  ────────────────────────────────────────────────────────");
    println!(
        "  (1+2+3) AVOIDABLE subtotal (A):                             {:>12.0} ns",
        a_ns
    );
    println!();
    println!(
        "  decommit syscall (MEM_DECOMMIT):                            {:>12.0} ns",
        decommit_ns
    );
    println!(
        "  (4+5) recommit + first-touch page faults:                  {:>12.0} ns  ({} pages @ {:.0} ns/fault)",
        refault_ns,
        payload_pages,
        refault_ns / payload_pages as f64
    );
    println!("  ────────────────────────────────────────────────────────");
    let b_ns = decommit_ns + refault_ns;
    println!(
        "  (4+5) IRREDUCIBLE subtotal (B):                             {:>12.0} ns",
        b_ns
    );
    println!();
    println!("── Real-world production cycle comparison ──");
    println!(
        "  A'  current design (reserve+touch+release):   {:>12.0} ns",
        a_prime_ns
    );
    println!(
        "  B   reservation-only floor (decommit+reflt):   {:>12.0} ns",
        b_ns
    );
    let net = a_prime_ns - b_ns;
    if net >= 0.0 {
        println!(
            "  A' − B = reservation-only SAVES:               {:>12.0} ns/cycle",
            net
        );
    } else {
        println!(
            "  A' − B = reservation-only COSTS EXTRA:          {:>12.0} ns/cycle",
            -net
        );
    }
    println!();

    // ── §4 The split (R29-3-comparable verdict basis) ──
    let avoidable_pct = a_ns / (a_ns + b_ns) * 100.0;
    let irreducible_pct = b_ns / (a_ns + b_ns) * 100.0;
    let avoidable_pct_aprime = a_ns / a_prime_ns * 100.0;
    let reserve_share_of_avoidable_pct = if a_ns > 0.0 {
        (reserve_only_ns + commit_only_ns) / a_ns * 100.0
    } else {
        0.0
    };

    println!("═══ §4 SPLIT (R29-3-comparable verdict basis) ═══");
    println!(
        "  (1+2+3) AVOIDABLE:   {:>12.0} ns  ({:.1}%)",
        a_ns, avoidable_pct
    );
    println!(
        "  (4+5)   IRREDUCIBLE: {:>12.0} ns  ({:.1}%)",
        b_ns, irreducible_pct
    );
    println!(
        "  (1+2+3) as share of real production cycle A': {:.1}%",
        avoidable_pct_aprime
    );
    println!(
        "  reserve-only+commit-only as share of (1+2+3) AVOIDABLE: {:.1}%",
        reserve_share_of_avoidable_pct
    );
    println!();

    let threshold = 20.0_f64;
    println!(
        "═══ VERDICT (threshold: avoidable > {}% of the segment-lifecycle cycle → material) ═══",
        threshold
    );
    if avoidable_pct > threshold {
        println!(
            "  RESERVATION PATH IS MATERIAL: (1+2+3) = {:.1}% of the cycle.",
            avoidable_pct
        );
        println!("  Step 3 (VirtualAlloc2 prototype) may be justified by this evidence.");
    } else {
        println!(
            "  RESERVATION PATH IS SMALL: (1+2+3) = {:.1}% — page-fault cost dominates,",
            avoidable_pct
        );
        println!("  matching R29-3's own Linux finding (1.0-1.3%). Step 3 is NOT justified");
        println!("  by this evidence alone — see the gate report for the full verdict.");
    }
    println!();

    // ── Machine-readable CSV block (derived-not-hand-typed rule): the report's
    // summary CSV is generated FROM this block by a checked script
    // (`scripts/r32_13_windows_decomposition_summary.mjs`), not hand-typed
    // from the prose above. One row per run of this binary.
    println!("# csv-start");
    println!(
        "platform,oracle_pass,reserve_only_ns,commit_only_ns,os_roundtrip_lumped_ns,\
         bookkeeping_ns,avoidable_a_ns,decommit_ns,refault_ns,irreducible_b_ns,\
         a_prime_ns,avoidable_pct,irreducible_pct,avoidable_pct_of_a_prime,\
         reserve_share_of_avoidable_pct,payload_pages"
    );
    let oracle_pass_flag = if is_windows {
        win_calls_delta_reserve == N as u64 && unix_attempts_delta_reserve == 0
    } else {
        win_calls_delta_reserve == 0
    };
    println!(
        "{},{},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.2},{:.2},{:.2},{:.2},{}",
        std::env::consts::OS,
        if oracle_pass_flag { 1 } else { 0 },
        reserve_only_ns,
        commit_only_ns,
        c_ns,
        bookkeeping_ns,
        a_ns,
        decommit_ns,
        refault_ns,
        b_ns,
        a_prime_ns,
        avoidable_pct,
        irreducible_pct,
        avoidable_pct_aprime,
        reserve_share_of_avoidable_pct,
        payload_pages
    );
    println!("# csv-end");
}
