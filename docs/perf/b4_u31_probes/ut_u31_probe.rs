// ut_u31_probe.rs — B4-29/U31 trigger probe (chunk 1, measurement only).
// Standalone example mirroring the six b4_u31_trigger_iai arms op-for-op so
// callgrind can profile each arm in isolation (iai runners are not used here).
// Usage: ut_u31_probe <sefer|mimalloc> <32768|65536|131072> [--counters]
//   --counters prints the B4_* observer deltas (bench-internals oracle) to
//   stdout as one JSON line; with it the workload still runs identically.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;

fn counters() -> [u64; 7] {
    sefer_alloc::SeferAlloc::dbg_b4_collapse_counters()
}

fn u31_counters() -> [u64; 4] {
    sefer_alloc::SeferAlloc::dbg_b4_u31_scan_counters()
}

fn sefer_body(n: usize) {
    // Mirrors prf01_support::R19Prf01::growth16: SeferAlloc::new, n blocks of
    // 16 B @ align(8), all live before ANY free, then all freed exactly once.
    let allocator = sefer_alloc::SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut addresses: Vec<usize> = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: valid nonzero layout; every allocation stays live until the
        // free loop below, freed exactly once by the owner.
        let block = unsafe { allocator.alloc(layout) };
        assert!(!block.is_null());
        addresses.push(block.expose_provenance());
    }
    black_box(&addresses);
    for address in addresses {
        let block = core::ptr::with_exposed_provenance_mut(address);
        // SAFETY: retained exposed provenance, uniquely owned live allocation,
        // exact original layout and allocator.
        unsafe { allocator.dealloc(block, layout) };
    }
}

fn mimalloc_body(n: usize) {
    let mi = mimalloc::MiMalloc;
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut addresses: Vec<usize> = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let block = unsafe { mi.alloc(layout) };
        assert!(!block.is_null());
        addresses.push(block.expose_provenance());
    }
    black_box(&addresses);
    for address in addresses {
        let block = core::ptr::with_exposed_provenance_mut(address);
        // SAFETY: uniquely owned live allocation returned by `mi.alloc` above
        // with the same layout; freed exactly once.
        unsafe { mi.dealloc(block, layout) };
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("sefer");
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(32_768);
    let want_counters = args.iter().any(|a| a == "--counters");
    assert!(mode == "sefer" || mode == "mimalloc");
    assert!([32_768_usize, 65_536, 131_072].contains(&n));
    let before = if want_counters { Some((counters(), u31_counters())) } else { None };
    match mode {
        "sefer" => sefer_body(n),
        _ => mimalloc_body(n),
    }
    if let Some((before, before_u31)) = before {
        let after = counters();
        let after_u31 = u31_counters();
        let delta = |i: usize| after[i] - before[i];
        println!(
            "{{\"mode\":\"{mode}\",\"growth16_n\":{n},\"find_segment_calls\":{},\"scan_calls\":{},\"scan_words\":{},\"foreign_publish_ok\":{},\"directory_lookups\":{},\"shard_lock_spins\":{},\"shard_lock_yields\":{}}}",
            delta(0),
            delta(1),
            delta(2),
            delta(3),
            delta(4),
            delta(5),
            delta(6),
        );
        let u31_seg = after_u31[0] - before_u31[0];
        let u31_base = after_u31[1] - before_u31[1];
        let u31_nopending = after_u31[2] - before_u31[2];
        let u31_disc = after_u31[3] - before_u31[3];
        println!(
            "{{\"growth16_n\":{n},\"u31_scan_segments\":{u31_seg},\"u31_holders_base\":{u31_base},\"u31_scan_no_pending\":{u31_nopending},\"u31_discovery_scans\":{u31_disc}}}"
        );
    }
}
