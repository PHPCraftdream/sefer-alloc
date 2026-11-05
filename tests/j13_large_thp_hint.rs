//! J13: a fresh Large reservation for a request of at least 8 MiB is advised `MADV_HUGEPAGE` (the
//! `hg` flag of its mapping in `/proc/self/smaps`); a Large request below 8 MiB is not. Not built with `numa-aware` (eager NUMA reservation, no hint).
#![cfg(all(
    target_os = "linux",
    feature = "alloc-global",
    not(feature = "numa-aware"),
    not(miri)
))]

use core::alloc::{GlobalAlloc, Layout};
use sefer_alloc::SeferAlloc;

// Private (not global) instance: the harness and its buffers stay on the system heap.
static A: SeferAlloc = SeferAlloc::new();

/// `VmFlags` of the mapping that contains `addr`.
fn vm_flags(addr: usize) -> Vec<String> {
    let smaps = std::fs::read_to_string("/proc/self/smaps").expect("read smaps");
    let mut inside = false;
    for line in smaps.lines() {
        let range = line.split_whitespace().next().unwrap_or("");
        if let Some((lo, hi)) = range.split_once('-') {
            if let (Ok(lo), Ok(hi)) = (usize::from_str_radix(lo, 16), usize::from_str_radix(hi, 16))
            {
                inside = (lo..hi).contains(&addr);
                continue;
            }
        }
        if inside {
            if let Some(f) = line.strip_prefix("VmFlags:") {
                return f.split_whitespace().map(str::to_owned).collect();
            }
        }
    }
    panic!("no mapping contains {addr:#x}");
}

#[test]
fn fresh_large_reservation_is_hinted_from_eight_mib() {
    if !std::path::Path::new("/sys/kernel/mm/transparent_hugepage").exists() {
        return; // a kernel without THP: the hint cannot be recorded
    }
    let big = Layout::from_size_align(16 << 20, 16).unwrap();
    let small = Layout::from_size_align(5 << 20, 16).unwrap();
    // SAFETY: non-zero sizes; both blocks are freed below with their layouts.
    let (pb, ps) = unsafe { (A.alloc(big), A.alloc(small)) };
    assert!(!pb.is_null() && !ps.is_null());
    let (fb, fs) = (vm_flags(pb as usize), vm_flags(ps as usize));
    assert!(
        fb.iter().any(|f| f == "hg"),
        "16 MiB block not advised: {fb:?}"
    );
    assert!(!fs.iter().any(|f| f == "hg"), "5 MiB block advised: {fs:?}");
    // SAFETY: the blocks came from `A` with these layouts and are freed once.
    unsafe {
        A.dealloc(pb, big);
        A.dealloc(ps, small);
    }
}
