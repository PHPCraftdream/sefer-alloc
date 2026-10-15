//! R6-T08 THROWAWAY measurement harness — NOT a shipping artifact.
//!
//! Measures Large **commit amplification**: N simultaneously-live 260 KiB
//! Large allocations per fresh `AllocCore`, for N in {16, 64, 256}, in the
//! caller's single feature mode. Modeled directly on
//! `examples/r12_3_exact_span_measure.rs` (same APIs: `proc_memstat::snapshot()`,
//! `AllocCore::alloc(layout)`, `dbg_span_usable_of` — hence `internals`).
//!
//! SINGLE-MODE RULE (R31-1): cost AND benefit come from ONE mode per run.
//! Run arm A and arm B separately and compare the CSV lines:
//!
//! Build/run WITHOUT the feature (arm A, whole-SEGMENT rounding):
//!   cargo run --release --example r6_t08_large_commit_probe --features "production internals"
//!
//! Build/run WITH the feature (arm B, page-exact spans):
//!   cargo run --release --example r6_t08_large_commit_probe --features "production internals,exact-span-large"
//!
//! `internals` is present in BOTH arms (it gates the dbg_* oracle hooks) and
//! changes nothing measured — the arm's identity is exact-span-large on/off.
//!
//! Measured axis: **reserved+committed memory** as reported by
//! `proc_memstat::snapshot().charged_or_reserved_bytes()` — a true commit
//! charge on Windows, a VA reservation size on Linux/macOS. The SEGMENT-rounding
//! amplification is visible on BOTH readings, but only Windows gives the
//! literal commit-limit figure. Each allocation is write-touched across its
//! FULL REQUESTED 260 KiB so RSS at least reflects the requested bytes in both
//! arms; committed-but-untouched tail pages (arm A's rounded-up remainder) need
//! not be resident, so RSS is informational only — the discriminating axes are
//! `reserved_delta_KiB` and the allocator-side oracle `dbg_span_usable_of`.
//!
//! Output: CSV to stdout, one line per N, header:
//! `arm,mode,N,reserved_delta_KiB,reserved_per_alloc_KiB,amplif_x,span_usable_bytes,segments_reserved_delta,rss_delta_KiB,alloc_ns_median`
//! (`mode` replaces the feature comma with `+` to stay CSV-quote-free).

use core::alloc::Layout;
use proc_memstat::snapshot;
use sefer_alloc::{AllocCore, SegmentLayout};

const KIB: u64 = 1024;
const SAMPLE: u64 = 260 * KIB;
const NS: [usize; 3] = [16, 64, 256];

fn main() {
    let exact_span = cfg!(feature = "exact-span-large");
    let arm = if exact_span { "B" } else { "A" };
    let mode = if exact_span {
        "production+exact-span-large"
    } else {
        "production"
    };
    println!("arm,mode,N,reserved_delta_KiB,reserved_per_alloc_KiB,amplif_x,span_usable_bytes,segments_reserved_delta,rss_delta_KiB,alloc_ns_median");

    for &n in &NS {
        // Fresh AllocCore per N — isolates each cell from prior segments.
        let mut ac = AllocCore::new().expect("primordial");
        let layout = Layout::from_size_align(SAMPLE as usize, 8).unwrap();

        let before = snapshot();
        let segs_before = AllocCore::dbg_segments_reserved_total();

        let mut ptrs = Vec::with_capacity(n);
        let mut times_ns: Vec<u128> = Vec::with_capacity(n);
        for _ in 0..n {
            let t0 = std::time::Instant::now();
            let ptr = ac.alloc(layout);
            times_ns.push(t0.elapsed().as_nanos());
            assert!(!ptr.is_null(), "OOM allocating {n} x {SAMPLE} KiB");
            ptrs.push(ptr);
        }

        // Write-touch the FULL REQUESTED region so RSS at least reflects the
        // requested bytes in both arms (reserved tail pages need not fault in;
        // see module doc — RSS is informational, the reserved delta is the
        // discriminating axis).
        for &ptr in &ptrs {
            // SAFETY: `ptr` is a live `layout`-sized allocation; exactly
            // `SAMPLE` bytes of it are written.
            unsafe { core::ptr::write_bytes(ptr, 0xAB, SAMPLE as usize) };
        }

        let after = snapshot();
        let segs_delta = AllocCore::dbg_segments_reserved_total() - segs_before;

        // Oracle: physical span of the FIRST live block. 4194304 (a whole
        // SEGMENT) in arm A; page-rounded (~264-272 KiB) in arm B.
        let span_usable = ac.dbg_span_usable_of(ptrs[0]);
        let base = SegmentLayout::segment_base_of(ptrs[0] as usize);

        let rss_delta = after.rss.saturating_sub(before.rss);
        let reserved_delta = after
            .charged_or_reserved_bytes()
            .saturating_sub(before.charged_or_reserved_bytes());
        let amplif = reserved_delta as f64 / (n as u64 * SAMPLE) as f64;
        times_ns.sort_unstable();
        let median_ns = times_ns[n / 2];

        // Oracle line goes to STDERR so stdout stays pure CSV.
        eprintln!("# oracle: mode={mode} n={n} span_usable={span_usable} segment_base=0x{base:x}");
        println!(
            "{arm},{mode},{n},{},{},{amplif:.2},{span_usable},{segs_delta},{},{median_ns}",
            reserved_delta / 1024,
            reserved_delta as f64 / n as f64 / 1024.0,
            rss_delta / 1024,
        );

        // Leak check: everything returns to baseline.
        for &ptr in ptrs.iter() {
            // SAFETY (R6-MS-1/2): each `ptr` is a live allocation from this
            // AllocCore made with `layout`, freed exactly once here.
            unsafe { ac.dealloc(ptr, layout) };
        }
        drop(ac);
        let released = snapshot();
        let drift = released
            .charged_or_reserved_bytes()
            .abs_diff(before.charged_or_reserved_bytes());
        // Generous tolerance: other process activity may shift a page or two.
        assert!(
            drift <= 4 * 1024 * 1024,
            "commit charge / virtual size did not return to baseline after free+drop: {drift} bytes drift"
        );
    }
}
