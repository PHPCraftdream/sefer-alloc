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

fn bench(label: &str, status: &[u8], iters: u32) {
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
    match std::fs::read("/proc/self/status") {
        Ok(real) => bench("REAL /proc/self/status", &real, 200_000),
        Err(e) => println!("(no /proc/self/status on this host: {e})"),
    }
}
