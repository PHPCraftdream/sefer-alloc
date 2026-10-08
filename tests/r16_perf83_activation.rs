//! R16 item83 single-process, single-scenario activation oracle, not a perf judge.
//! ARM=A or C0 checks unchanged source; ARM=B checks the future candidate.
//! SCENARIO names one of the five preregistered work fixtures.
//! Without env, ARM=A and the below-half 8->3 control run meaningfully.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "alloc-stats",
    feature = "bench-internals"
))]

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::AllocCore;
use std::alloc::Layout;

#[test]
fn single_realloc_activation() {
    let arm = match std::env::var("ARM") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => "A".to_owned(),
        Err(error) => panic!("invalid ARM encoding: {error}"),
    };
    assert!(matches!(arm.as_str(), "A" | "C0" | "B"), "invalid ARM");
    let scenario = match std::env::var("SCENARIO") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => "realloc_large_shrink_8_to_3mib".to_owned(),
        Err(error) => panic!("invalid SCENARIO encoding: {error}"),
    };
    let (old_size, new_size, moderate) = match scenario.as_str() {
        "realloc_large_shrink_8_to_6mib" => (8_388_608, 6_291_456, true),
        "realloc_large_shrink_8_to_4p5mib" => (8_388_608, 4_718_592, true),
        "realloc_large_shrink_8_to_3mib" => (8_388_608, 3_145_728, false),
        "realloc_large_grow_6_to_8mib" => (6_291_456, 8_388_608, false),
        "realloc_large_equal_8mib" => (8_388_608, 8_388_608, false),
        _ => panic!("invalid SCENARIO"),
    };
    let expected_inplace = new_size == old_size || (moderate && arm == "B");
    let old_layout = Layout::from_size_align(old_size, 16).unwrap();
    let new_layout = Layout::from_size_align(new_size, 16).unwrap();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    assert_eq!((*heap).dbg_class_for(old_layout), None, "old must be Large");
    assert_eq!((*heap).dbg_class_for(new_layout), None, "new must be Large");
    let ptr = (*heap).alloc(old_layout);
    assert!(!ptr.is_null());
    assert_eq!(ptr.addr() % 16, 0);
    // SAFETY: ptr is exclusively owned, live and writable for old_size bytes.
    unsafe { core::ptr::write_bytes(ptr, 0x5A, old_size) };
    let before = (
        AllocCore::dbg_reloc_inplace_large_count(),
        AllocCore::dbg_reloc_inplace_small_count(),
        AllocCore::dbg_reloc_fastpath_decline_count(),
    );
    // SAFETY: ptr is live in this heap with precisely old_layout.
    let resized = unsafe { (*heap).realloc(ptr, old_layout, new_size) };
    let after = (
        AllocCore::dbg_reloc_inplace_large_count(),
        AllocCore::dbg_reloc_inplace_small_count(),
        AllocCore::dbg_reloc_fastpath_decline_count(),
    );
    if resized.is_null() {
        // SAFETY: failed realloc preserves the original live allocation.
        unsafe { (*heap).dealloc(ptr, old_layout) };
        panic!("realloc failed");
    }
    let aligned = resized.addr() % 16 == 0;
    let moved = resized.addr() != ptr.addr();
    let deltas = (
        after.0.checked_sub(before.0),
        after.1.checked_sub(before.1),
        after.2.checked_sub(before.2),
    );
    // SAFETY: successful realloc provides new_size live bytes; only the
    // initialized min(old,new) prefix is read, with no aliasing writes.
    let preserved = unsafe { core::slice::from_raw_parts(resized, old_size.min(new_size)) };
    let prefix_ok = preserved.iter().all(|&byte| byte == 0x5A);
    // SAFETY: resized owns new_size writable bytes; the shared read is over.
    unsafe { core::ptr::write_bytes(resized, 0x5A, new_size) };
    // SAFETY: successful result is freed once with NEW layout in its owner heap.
    unsafe { (*heap).dealloc(resized, new_layout) };
    // Outcome assertions follow deallocation so an activation failure does
    // not leave the live result behind for lease teardown during unwinding.
    assert!(aligned, "result alignment");
    assert!(prefix_ok, "preserved prefix");
    assert_eq!(
        moved, !expected_inplace,
        "address activation {arm}/{scenario}"
    );
    assert_eq!(
        deltas,
        (
            Some(u64::from(expected_inplace)),
            Some(0),
            Some(u64::from(!expected_inplace))
        )
    );
    let deltas = (deltas.0.unwrap(), deltas.1.unwrap(), deltas.2.unwrap());
    println!(
        "{{\"arm\":\"{arm}\",\"scenario\":\"{scenario}\",\"pid\":{},\"old_bytes\":{old_size},\"new_bytes\":{new_size},\"moved\":{moved},\"inplace_large_delta\":{},\"inplace_small_delta\":{},\"decline_delta\":{},\"preserved_prefix\":true,\"new_layout_dealloc\":true}}",
        std::process::id(), deltas.0, deltas.1, deltas.2
    );
}
