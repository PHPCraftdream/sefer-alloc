//! B3 spike oracle (temporary): ≥95% of refill blocks must come from the
//! sub-leaf free structure on the recycle pattern (escalation §4.2).
#![cfg(all(feature = "production", feature = "bench-internals", feature = "internals"))]
use sefer_alloc::SeferAlloc;
use std::alloc::GlobalAlloc;

#[test]
fn b3s_recycle_refill_from_free_structure() {
    let sefer = SeferAlloc::new();
    let layout = core::alloc::Layout::from_size_align(16, 8).unwrap();
    let mut ptrs: [*mut u8; 256] = [core::ptr::null_mut(); 256];
    for round in 0..2 {
        let before = sefer_alloc::b3s_drain_from_free();
        for slot in ptrs.iter_mut() {
            // SAFETY: non-zero size, valid alignment; freed below same layout.
            *slot = unsafe { sefer.alloc(layout) };
        }
        for &p in &ptrs {
            assert!(!p.is_null(), "alloc failed in round {round}");
        }
        let after = sefer_alloc::b3s_drain_from_free();
        let from_free = after - before;
        println!("[b3s-oracle] round {round}: served=256 from_free={from_free} ({:.1}%)",
            from_free as f64 * 100.0 / 256.0);
        if round == 1 {
            // tcache (16 slots) still holds 16 blocks after round-1 free; those
            // are magazine HITS, not refill. Refill = served − tcache hits.
            const TCACHE_CAP: u64 = 16;
            let refill = 256 - TCACHE_CAP;
            assert!(from_free * 100 >= 95 * refill, "refill-from-free {from_free}/{refill} < 95%");
            println!("[b3s-oracle] round 1: refill={refill} from_free={from_free} ({:.1}%)",
                from_free as f64 * 100.0 / refill as f64);
        }
        // SAFETY: same layout as allocated above; freed exactly once.
        for &p in &ptrs {
            unsafe { sefer.dealloc(p, layout) };
        }
    }
}
