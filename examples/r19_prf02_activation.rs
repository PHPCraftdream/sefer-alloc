use core::alloc::Layout;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapRegistry};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "usage: A|B|C expected-lookups");
    let arm = &args[0];
    let expected: u64 = args[1].parse().expect("expected lookup count");
    assert_eq!(expected, if arm == "A" { 32 } else { 16 });
    assert!(matches!(arm.as_str(), "A" | "B" | "C"));
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut blocks = [core::ptr::null_mut(); 64];
    for block in &mut blocks {
        *block = heap.alloc(layout);
        assert!(!block.is_null());
    }
    assert_eq!(heap.dbg_tcache_count(0), 0);
    let hits_before = AllocCore::dbg_contains_base_tier1_hits();
    let misses_before = AllocCore::dbg_contains_base_tier1_misses();
    let scans_before = AllocCore::dbg_routed_miss_scans();
    for &block in &blocks[..16] {
        // SAFETY: distinct live owned blocks with their matching allocation layout.
        unsafe { heap.dealloc(block, layout) };
    }
    let hits = AllocCore::dbg_contains_base_tier1_hits() - hits_before;
    let misses = AllocCore::dbg_contains_base_tier1_misses() - misses_before;
    let scans = AllocCore::dbg_routed_miss_scans() - scans_before;
    let count = heap.dbg_tcache_count(0);
    println!("{{\"arm\":\"{arm}\",\"expected\":{expected},\"n\":16,\"preallocated\":64,\"hits\":{hits},\"misses\":{misses},\"tcache\":{count},\"routed_scans\":{scans}}}");
    assert_eq!(hits + misses, expected);
    assert_eq!(count, 16);
    assert_eq!(scans, 0);
    core::mem::forget(lease);
}
