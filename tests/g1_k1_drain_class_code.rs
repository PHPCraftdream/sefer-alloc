#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

//! G1-K1: `try_drain_freelist_batch` does not re-store a drained block's
//! `ClassLeaves` code; the code stored at the block's first issue must still
//! be there. Covers the drain itself (AllocCore) and the magazine refill that
//! reaches it through the global heap path (HeapCore).

use std::alloc::Layout;

use sefer_alloc::registry::segment_route::RouteRegistration;
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::{AllocCore, SegmentLayout};

fn class_at(ptr: *mut u8) -> Option<u8> {
    RouteRegistration::class_at_global_address_for_test(ptr.addr())
}

#[test]
fn drained_blocks_keep_their_first_issue_class_code() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let anchor = core.dbg_r11_scalar_carve_for_test(0).unwrap();
    let layout = Layout::from_size_align(16, 16).unwrap();
    // 300 blocks of class 0 span two class leaves (256 granules per leaf).
    let mut blocks = [core::ptr::null_mut(); 300];
    assert_eq!(core.dbg_carve_batch(0, &mut blocks), blocks.len());
    assert!(blocks.iter().all(|&p| class_at(p) == Some(0)));
    for ptr in blocks {
        // SAFETY: each block is a live carve of this core with this layout.
        unsafe { core.dealloc(ptr, layout) };
    }
    let mut out = [anchor; 300];
    // SAFETY: `anchor` is a live allocation of this exclusively owned core.
    let k = unsafe { core.dbg_drain_freelist_batch(anchor, 0, &mut out) };
    assert_eq!(k, blocks.len(), "the whole chain drains");
    for &p in &out[..k] {
        assert_eq!(
            class_at(p),
            Some(0),
            "drained block {p:p} lost its class code"
        );
    }
    for &p in out[..k].iter().chain([anchor].iter()) {
        // SAFETY: drained blocks and the anchor are live with this layout.
        unsafe { core.dealloc(p, layout) };
    }
}

#[test]
fn magazine_refills_from_the_bin_keep_class_codes() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    for size in [16usize, 64, 256, 1024] {
        let layout = Layout::from_size_align(size, 16).unwrap();
        let class = SegmentLayout::class_for(size, 16).unwrap() as u8;
        // Far more live blocks than magazine + depot hold, so the frees
        // flush into the bin and the second round refills by draining it.
        let n = 4096;
        let first: Vec<*mut u8> = (0..n).map(|_| heap.alloc(layout)).collect();
        assert!(first.iter().all(|p| !p.is_null()));
        for &p in &first {
            // SAFETY: p is live and was allocated with layout.
            unsafe { heap.dealloc(p, layout) };
        }
        let second: Vec<*mut u8> = (0..n).map(|_| heap.alloc(layout)).collect();
        let reused = second.iter().filter(|p| first.contains(p)).count();
        assert!(
            reused > n / 2,
            "the second round must reuse freed blocks ({reused}/{n})"
        );
        for &p in &second {
            assert!(!p.is_null());
            assert_eq!(class_at(p), Some(class), "size {size}: block {p:p}");
            // SAFETY: p is live and was allocated with layout.
            unsafe { heap.dealloc(p, layout) };
        }
    }
    #[cfg(feature = "fastbin")]
    heap.dbg_flush_all();
    drop(lease);
}
