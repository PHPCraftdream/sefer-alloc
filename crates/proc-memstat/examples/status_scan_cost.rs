//! Measures what review P4-2 asks to measure BEFORE deciding to optimize:
//! how much the Linux reader's three separate buffer scans actually cost
//! versus fusing them into one pass.
//!
//! The finding is precise about the shape of the cost and this example does
//! not inflate it: three lookups walking the buffer from the start each time
//! is O(L) done three times, NOT O(L²). So the honest expectation going in is
//! a small constant-factor difference, and the question is whether it is worth
//! any API change at all.
//!
//! Run: `cargo run --release -p proc-memstat --example status_scan_cost`
//!
//! Intended to be run on LINUX CI rather than a loaded developer box — a
//! machine under a CPU quota produces numbers dominated by scheduling, not by
//! the code under test.
//!
//! The parser module is pulled in with `#[path]`, the same sanctioned pattern
//! `tests/status_parse.rs` uses, so measuring it needs no public API.
//!
//! # What the numbers said — VERDICT: NO-GO, do not adopt the fused reader
//!
//! Measured on GitHub Actions ubuntu-latest, commit `3119719`, run
//! `34249822373`, job `102141100998`. Raw output and the earlier run:
//! `docs/perf/_raw_proc_memstat_p4_2_scan_cost.log`; machine-readable
//! companion: `docs/perf/PROC_MEMSTAT_P4_2_SCAN_COST_summary.csv`.
//!
//! The finding's own shape claim holds: on the real 1510-byte
//! `/proc/self/status`, three scans cost 543.2 ns against 207.5 ns fused —
//! a 2.62x ratio, growing slowly with L (2.18x at 271 bytes, 4.90x at
//! 20 751 bytes), which is O(L) three times, not O(L²).
//!
//! Taken alone that ratio argues for the fix. It is the wrong figure to
//! decide on. One whole `snapshot()` costs 17 125.3 ns, because the
//! open/read/close round trip — and the kernel formatting the file's text on
//! demand — dominates a parse measured in hundreds of nanoseconds. What was
//! compared: two PARSE-ONLY loops, against a SEPARATELY measured old full
//! `snapshot()` — NOT a full A/B of two whole backends. So the saving of
//! 335.8 ns out of 17 125.3 ns per call, i.e. 2.0%, is an ESTIMATE of the
//! removable work's share of a call, not a measured speedup of a new
//! `snapshot()`. Two distinct quantities, not interchangeable: the full
//! three-scan parse itself is ~3.17% of that same call (543.2 / 17 125.3),
//! while the ~2.0% is what FUSING the scans SAVES of it (335.8 / 17 125.3).
//! The earlier run
//! (`4297f18`, run `34244081677`) measured the same real-file arm at a 1.95x
//! ratio and 196.0 ns saved, 1.1% of the same denominator; the parse arms
//! carry visible cross-run CI noise, and the verdict is unchanged across it
//! because both figures are negligible against the syscall.
//!
//! **What the cited figures are evidence of — and only of — THAT commit's
//! code.** The measured code has since changed: the shared field-value tail
//! the fused reader calls now additionally validates grammar/unit (round-2
//! P3-1 landed after the measurement), and the whole-`snapshot()`
//! denominator now acquires via `/proc/thread-self/status` (round-3 P2-1).
//! Re-running on current HEAD is therefore NOT expected to reproduce these
//! exact numbers. What IS unchanged: the outer loop structure of both
//! readers, and the shape of the finding — O(L) three times vs once,
//! negligible against the syscall.
//!
//! So the backend keeps its three `read_kib_field` calls. Fusing them would
//! buy 2% of a call that a probe makes a handful of times per process, in
//! exchange for a reader whose correctness has to be kept in step with the
//! per-field one.
//!
//! **What this measurement does NOT establish.** The large synthetic arms are
//! parse-only: no real `/proc/self/status` of 20 751 bytes was read, so their
//! ratios cannot be turned into a percentage of a call, and this verdict does
//! not cover a process with enough supplementary groups to reach that size.
//! Establishing that would need the read cost measured at the same length,
//! which this example does not do.

#[path = "../src/status_parse.rs"]
mod status_parse;

use status_parse::{read_kib_field, read_kib_fields};
use std::time::Instant;

/// The three fields the Linux backend actually reads.
const FIELDS: [&[u8]; 3] = [b"VmRSS:", b"VmSize:", b"VmHWM:"];

/// Build a `/proc/self/status`-shaped buffer.
///
/// `groups_len` pads the `Groups:` line, which on a real system can be long
/// and — crucially for this finding — comes BEFORE the memory fields. That is
/// exactly why the review forbids "just read the first 4/8 KiB": the bytes a
/// truncating reader would skip are the ones a scan has to cross to reach
/// VmRSS.
fn synthetic_status(groups_len: usize) -> Vec<u8> {
    let mut s = Vec::new();
    s.extend_from_slice(b"Name:\tprobe\nUmask:\t0022\nState:\tR (running)\n");
    s.extend_from_slice(b"Tgid:\t1234\nNgid:\t0\nPid:\t1234\nPPid:\t1\n");
    s.extend_from_slice(b"Groups:\t");
    for i in 0..groups_len {
        s.extend_from_slice(format!("{} ", 1000 + (i % 100)).as_bytes());
    }
    s.push(b'\n');
    s.extend_from_slice(b"VmPeak:\t  123456 kB\nVmSize:\t  120000 kB\n");
    s.extend_from_slice(b"VmLck:\t       0 kB\nVmPin:\t       0 kB\n");
    s.extend_from_slice(b"VmHWM:\t   45678 kB\nVmRSS:\t   40000 kB\n");
    s.extend_from_slice(b"RssAnon:\t   30000 kB\nRssFile:\t   10000 kB\n");
    s.extend_from_slice(b"Threads:\t8\nSigQ:\t0/1000\n");
    s
}

/// Returns `(three_scans_ns, one_scan_ns)` per call so the caller can put the
/// saving over a denominator instead of reporting it bare.
///
/// `iters` is the size of ONE timed batch, and the reported figure is that
/// batch's total elapsed time divided by `iters` — a single mean with no
/// independent samples or variance behind it, not N separate measurements.
fn bench(label: &str, status: &[u8], iters: u32) -> (f64, f64) {
    // Three separate scans — what the backend does today.
    let t0 = Instant::now();
    let mut sink = 0u64;
    for _ in 0..iters {
        for f in FIELDS {
            sink = sink.wrapping_add(read_kib_field(std::hint::black_box(status), f).unwrap_or(0));
        }
    }
    let three = t0.elapsed();

    // One fused scan.
    let t1 = Instant::now();
    let mut sink2 = 0u64;
    for _ in 0..iters {
        let got = read_kib_fields(std::hint::black_box(status), FIELDS);
        for v in got {
            sink2 = sink2.wrapping_add(v.unwrap_or(0));
        }
    }
    let one = t1.elapsed();

    // Both arms must have read the same values, or the comparison is
    // meaningless — assert rather than trust.
    assert_eq!(sink, sink2, "the two readers disagreed on {label}");

    let three_ns = three.as_nanos() as f64 / f64::from(iters);
    let one_ns = one.as_nanos() as f64 / f64::from(iters);
    let ratio = three_ns / one_ns;
    println!(
        "{label:<28} bytes={:<7} three_scans={three_ns:>9.1} ns  one_scan={one_ns:>9.1} ns  \
         ratio={ratio:.2}x  saved={:>8.1} ns",
        status.len(),
        three_ns - one_ns
    );
    (three_ns, one_ns)
}

/// Measure what one whole `snapshot()` costs, so the parse-side saving above
/// has a DENOMINATOR.
///
/// Without this, `saved=196.0 ns` is a numerator with nothing under it, and
/// the same figure argues for opposite decisions depending on the total it is
/// a fraction of: meaningful against a ~400 ns call, noise against a ~5 us
/// one. On Linux `snapshot()` reads `/proc/thread-self/status` — the
/// calling thread's own task status, falling back to `/proc/self/status` on
/// pre-3.17 kernels — through `std::fs::read`, an open/read/close round trip
/// plus the kernel formatting the file's text on demand, so the parse is
/// only ever part of the cost.
///
/// Returns ns per call. `iters` is the iteration count of ONE timed batch
/// (after a 1 000-call warmup), not independent timing samples.
fn snapshot_cost_ns(iters: u32) -> f64 {
    for _ in 0..1_000 {
        std::hint::black_box(proc_memstat::snapshot());
    }
    let t = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(proc_memstat::snapshot());
    }
    t.elapsed().as_nanos() as f64 / f64::from(iters)
}

fn main() {
    // Warm up so the first arm does not absorb page faults / branch-predictor
    // cold start, which on a short run is larger than the effect being
    // measured.
    let warm = synthetic_status(64);
    for _ in 0..10_000 {
        std::hint::black_box(read_kib_fields(&warm, FIELDS));
        for f in FIELDS {
            std::hint::black_box(read_kib_field(&warm, f));
        }
    }

    println!("proc-memstat P4-2: cost of three scans vs one fused scan");
    println!("(three_scans = today's backend; one_scan = read_kib_fields)");
    println!();

    // A real /proc/self/status is typically ~1-2 KiB; the larger sizes show
    // how the gap scales, since a constant-factor claim is only interesting
    // if it holds as L grows.
    for groups in [0usize, 64, 512, 4096] {
        let status = synthetic_status(groups);
        bench(&format!("groups={groups}"), &status, 200_000);
    }

    // And the real thing, where available — the synthetic buffer is a model,
    // and a model's numbers should be checked against the article itself.
    // This arm parses the leader-named `/proc/self/status`, while the
    // whole-`snapshot()` denominator below reads the calling thread's
    // `/proc/thread-self/status`; both reflect the same shared `mm` while
    // the leader is alive.
    let real_arms = match std::fs::read("/proc/self/status") {
        Ok(real) => Some(bench("REAL /proc/self/status", &real, 200_000)),
        Err(e) => {
            println!("(no /proc/self/status on this host: {e})");
            None
        }
    };

    // The decision figure. A bare "saved N ns" is a numerator with no
    // denominator, and the same N argues both ways depending on what it is a
    // fraction OF — so state both, and state which is which.
    let Some((three_ns, one_ns)) = real_arms else {
        println!("\n(no whole-snapshot() comparison: this host has no /proc/self/status)");
        return;
    };
    // Fewer iterations than the parse arms: each one is a real open/read/close
    // round trip, not an in-memory scan.
    let snap_ns = snapshot_cost_ns(20_000);
    let saved_ns = three_ns - one_ns;
    let pct = 100.0 * saved_ns / snap_ns;
    // Self-consistency insurance, and ONLY that: this recomputes the SAME
    // formula from the SAME variables the println! below formats, so it
    // proves the printed percentage is the quotient of these variables —
    // nothing more. It does NOT independently verify the measured values,
    // the printed output a human reads, or their transcription into the
    // summary CSV; a wrong measurement upstream passes this untouched.
    assert!(
        (pct - (100.0 * (three_ns - one_ns) / snap_ns)).abs() < 1e-9,
        "the printed percentage must be the quotient it claims to be"
    );
    println!();
    println!("proc-memstat P4-2: the saving OVER THE WHOLE CALL");
    println!(
        "  whole snapshot() ......... {snap_ns:.1} ns/call  (open+read+close of \
         the calling thread's task status — /proc/thread-self/status, \
         falling back to /proc/self/status on pre-3.17 kernels — plus the parse)"
    );
    println!(
        "  parse, three scans ....... {three_ns:.1} ns = {:.1}% of one snapshot()  \
         (today's backend: the parse's own SHARE of a call)",
        100.0 * three_ns / snap_ns
    );
    println!("  parse, one fused scan .... {one_ns:.1} ns  (read_kib_fields)");
    println!(
        "  saving ................... {saved_ns:.1} ns = {pct:.1}% of one snapshot() \
         ({saved_ns:.1} ns saved / {snap_ns:.1} ns per call) — the SAVING from \
         fusing, an estimate, not the parse's own share printed above"
    );
}
