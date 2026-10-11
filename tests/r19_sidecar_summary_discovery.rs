//! Actual production discovery and bitmap backing; no timing measurements.
#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::segment_route::{RouteDirectory, SmallSidecar};
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};

fn records(mut scan: sefer_alloc::registry::segment_route::RouteScan<'_>) -> Vec<(u32, u8)> {
    let mut records = Vec::new();
    while let Some(mut cut) = scan.next_cut() {
        while let Some(record) = cut.pop() {
            records.push((record.offset, record.class));
        }
    }
    records
}

#[test]
fn paused_actual_half_discovery_then_resume() {
    isolated("paused_actual_half_discovery_then_resume", || {
        use sefer_alloc::registry::segment_route::RouteKind;
        let directory = RouteDirectory::new();
        let reservation = aligned_vmem::reserve_aligned(4 * 1024 * 1024, 4 * 1024 * 1024).unwrap();
        let root = reservation.as_ptr();
        let route = directory
            .register(root, 4 * 1024 * 1024, root, 9, RouteKind::Small)
            .unwrap();
        assert!(route.prepare_small(0, 0));
        assert!(route.issue_small(0, 0));
        let sidecar = route.small_sidecar().unwrap();
        // SAFETY: this owner issued exactly one instance above and transfers it
        // once. Only sidecar owner scans follow, never a payload access/free.
        let paused = unsafe {
            directory
                .lookup(root)
                .unwrap()
                .dbg_pause_publish_for_test(0)
        }
        .unwrap();
        assert!(records(sidecar.scan_summarized_for_test(16).unwrap()).is_empty());
        paused.resume();
        assert!(
            directory.lookup(root).unwrap().pending_for_test(root),
            "resume must execute pending LP even under M5'; missing discovery is stranded pending, not missing publication"
        );
        assert_eq!(
            records(sidecar.scan_summarized_for_test(16).unwrap()),
            [(0, 0)]
        );
        assert!(records(sidecar.scan_summarized_for_test(16).unwrap()).is_empty());
    });
}

#[test]
fn strict_consumes_before_resume_benign_hint() {
    isolated("strict_consumes_before_resume_benign_hint", || {
        use sefer_alloc::registry::segment_route::RouteKind;
        let directory = RouteDirectory::new();
        let reservation = aligned_vmem::reserve_aligned(4 * 1024 * 1024, 4 * 1024 * 1024).unwrap();
        let root = reservation.as_ptr();
        let route = directory
            .register(root, 4 * 1024 * 1024, root, 9, RouteKind::Small)
            .unwrap();
        assert!(route.prepare_small(0, 0));
        assert!(route.issue_small(0, 0));
        let sidecar = route.small_sidecar().unwrap();
        // SAFETY: unique instance issued above; no payload access/free after LP.
        let paused = unsafe {
            directory
                .lookup(root)
                .unwrap()
                .dbg_pause_publish_for_test(0)
        }
        .unwrap();
        assert_eq!(records(sidecar.scan(16).unwrap()), [(0, 0)]);
        paused.resume();
        let mut discovery = sidecar.scan_summarized_for_test(16).unwrap();
        assert!(
            discovery.next_cut().unwrap().is_empty(),
            "late hint selects an empty pending word"
        );
        assert!(discovery.next_cut().is_none());
        assert!(
            records(sidecar.scan(16).unwrap()).is_empty(),
            "no second reclaim record"
        );
    });
}

#[test]
fn actual_strict_trim_consumes_before_resume_once() {
    isolated("actual_strict_trim_consumes_before_resume_once", || {
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(16, 16).unwrap();
        // SAFETY: valid layout, uniquely retained allocation until terminal LP.
        let block = unsafe { allocator.alloc(layout) };
        assert!(!block.is_null());
        let pin = RouteDirectory::global().lookup(block).unwrap();
        let observer = RouteDirectory::global().lookup(block).unwrap();
        let offset = (block.addr() & (sefer_alloc::SegmentLayout::SEGMENT - 1)) as u32;
        // SAFETY: pin/offset name this unique issued instance, transferred once.
        // No user access or deallocation follows successful publication.
        let paused = unsafe { pin.dbg_pause_publish_for_test(offset) }.unwrap();
        let before = allocator.dbg_sidecar_ingress_stats();
        allocator.trim_current_thread();
        let after = allocator.dbg_sidecar_ingress_stats();
        assert_eq!(
            after.1 - before.1,
            1,
            "public strict trim consumes before resume"
        );
        assert!(!observer.pending_for_test(block));
        paused.resume();
        assert!(
            !observer.pending_for_test(block),
            "late hint does not republish pending"
        );
        allocator.trim_current_thread();
        assert_eq!(
            allocator.dbg_sidecar_ingress_stats().1,
            after.1,
            "no double reclaim after resume"
        );
    });
}

#[test]
fn reservation_retired_before_resume_only_sidecar_write() {
    isolated(
        "reservation_retired_before_resume_only_sidecar_write",
        || {
            use sefer_alloc::registry::{segment_route::RouteKind, HeapRegistry};
            use sefer_alloc::{AllocCore, LargeCacheConfig, SegmentLayout, SmallSegmentPoolConfig};
            let config =
                LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
            let mut lease = HeapRegistry::dbg_claim_lease_with_config(config).unwrap();
            let heap = lease.core();
            let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 16).unwrap();
            let mut blocks = Vec::new();
            let mut first = None;
            for _ in 0..(3 * SegmentLayout::SEGMENT / SegmentLayout::SMALL_MAX + 256) {
                let block = heap.alloc(layout);
                assert!(!block.is_null());
                let pin = RouteDirectory::global().lookup(block).unwrap();
                let base = block.addr() & !(SegmentLayout::SEGMENT - 1);
                let next_small =
                    pin.kind() == RouteKind::Small && first.is_some_and(|first| first != base);
                if pin.kind() == RouteKind::Small && first.is_none() {
                    first = Some(base);
                }
                blocks.push(block);
                if next_small {
                    break;
                }
            }
            let first = first.expect("actual Small segment");
            assert!(blocks
                .iter()
                .any(
                    |block| block.addr() & !(SegmentLayout::SEGMENT - 1) != first
                        && RouteDirectory::global().lookup(*block).unwrap().kind()
                            == RouteKind::Small
                ));
            let victims: Vec<_> = blocks
                .iter()
                .copied()
                .filter(|block| block.addr() & !(SegmentLayout::SEGMENT - 1) == first)
                .collect();
            let victim = victims[0];
            let offset = (victim.addr() - first) as u32;
            // SAFETY: actual uniquely retained heap issue, transferred exactly once.
            let paused = unsafe {
                RouteDirectory::global()
                    .lookup(victim)
                    .unwrap()
                    .dbg_pause_publish_for_test(offset)
            }
            .unwrap();
            for &block in &victims[1..] {
                let offset = (block.addr() - first) as u32;
                // SAFETY: distinct current heap issues, transferred once each.
                assert!(unsafe {
                    RouteDirectory::global()
                        .lookup(block)
                        .unwrap()
                        .publish_small(offset)
                });
            }
            let released = AllocCore::dbg_segments_released_total();
            let sidecar_before_retirement = SmallSidecar::system_totals_for_test();
            assert_eq!(heap.dbg_drain_sidecar_ingress(), victims.len());
            heap.dbg_drain_small_pool();
            assert!(
                RouteDirectory::global().lookup(victim).is_none(),
                "table retirement unlinks route"
            );
            assert_eq!(
                AllocCore::dbg_segments_released_total() - released,
                1,
                "actual OS release path"
            );
            let held = SmallSidecar::system_totals_for_test();
            assert_eq!(
                held.2, sidecar_before_retirement.2,
                "paused pin retains sidecar across retirement"
            );
            paused.resume(); // Only pre-LP sidecar address is used, after OS/table retirement.
            let freed = SmallSidecar::system_totals_for_test();
            assert!(
                freed.2 > held.2,
                "last retained pin releases independent sidecar"
            );
            assert_eq!(heap.dbg_drain_sidecar_ingress(), 0, "no double retirement");
            for block in blocks {
                if block.addr() & !(SegmentLayout::SEGMENT - 1) != first {
                    let offset = (block.addr() & (SegmentLayout::SEGMENT - 1)) as u32;
                    // SAFETY: remaining unique issued blocks were not published above.
                    assert!(unsafe {
                        RouteDirectory::global()
                            .lookup(block)
                            .unwrap()
                            .publish_small(offset)
                    });
                }
            }
            heap.dbg_drain_sidecar_ingress();
        },
    );
}

const A_SIDECAR_BYTES: usize = 41_984;

fn isolated(name: &str, body: impl FnOnce()) {
    // Miri cannot spawn subprocesses. Each witness runs directly and serially
    // in the narrow commands; native allocator/global-counter tests stay isolated.
    #[cfg(miri)]
    {
        let _ = name;
        body();
        return;
    }
    #[cfg(not(miri))]
    {
        if std::env::var("SEFER_PRF01_DISCOVERY_CHILD").ok().as_deref() == Some(name) {
            body();
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("SEFER_PRF01_DISCOVERY_CHILD", name)
            .status()
            .unwrap();
        assert!(status.success(), "isolated discovery test failed: {status}");
    }
}

#[test]
fn exact_sidecar_size_increment() {
    assert_eq!(core::mem::size_of::<SmallSidecar>(), A_SIDECAR_BYTES + 512);
}

#[test]
fn joined_foreign_free_reused_by_next_discovery() {
    isolated("joined_foreign_free_reused_by_next_discovery", || {
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(258_752, 8).unwrap();
        let blocks: Vec<_> = (0..34)
            .map(|_| {
                // SAFETY: valid nonzero layout; each result is uniquely retained.
                let block = unsafe { allocator.alloc(layout) };
                assert!(!block.is_null());
                block
            })
            .collect();
        let incarnations: std::collections::BTreeSet<_> = blocks
            .iter()
            .map(|&p| RouteDirectory::global().lookup(p).unwrap().incarnation())
            .collect();
        assert_eq!(incarnations.len(), 3, "actual three-segment geometry");
        let victim_address = blocks[0].expose_provenance();
        std::thread::scope(|scope| {
            let allocator = &allocator;
            scope
                .spawn(move || {
                    let victim = core::ptr::with_exposed_provenance_mut(victim_address);
                    // SAFETY: handoff transfers the unique first allocation's free
                    // obligation; owner retains allocator and performs no concurrent free.
                    unsafe { allocator.dealloc(victim, layout) };
                })
                .join()
                .unwrap();
        });
        // SAFETY: valid layout; joined free precedes this owner allocation.
        let reused = unsafe { allocator.alloc(layout) };
        assert_eq!(
            reused.addr(),
            victim_address,
            "next discovery must reuse joined free"
        );
        // SAFETY: first instance was retired remotely; all other instances plus
        // reused are distinct current allocations, each deallocated exactly once.
        unsafe {
            for &block in &blocks[1..] {
                allocator.dealloc(block, layout);
            }
            allocator.dealloc(reused, layout);
        }
    });
}

#[test]
fn strict_trim_includes_pending_without_summary() {
    isolated("strict_trim_includes_pending_without_summary", || {
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(16, 16).unwrap();
        // SAFETY: valid nonzero layout, unique ownership retained until LP.
        let block = unsafe { allocator.alloc(layout) };
        assert!(!block.is_null());
        let pin = RouteDirectory::global().lookup(block).unwrap();
        let offset = (block.addr() & (sefer_alloc::SegmentLayout::SEGMENT - 1)) as u32;
        // SAFETY: pin names this unique current issued Small block; consuming
        // the pin retains sidecar through LP. No access/free follows publication.
        assert!(unsafe { pin.dbg_publish_pending_only_for_test(offset) });
        let before = allocator.dbg_sidecar_ingress_stats();
        allocator.trim_current_thread();
        let after = allocator.dbg_sidecar_ingress_stats();
        assert_eq!(after.0 - before.0, 1);
        assert_eq!(
            after.1 - before.1,
            1,
            "strict must include completed pending LP"
        );
    });
}

#[cfg(r19_sidecar_summary_oracle)]
#[test]
fn growth_discovery_exchanges_no_pending_words() {
    isolated("growth_discovery_exchanges_no_pending_words", || {
        use sefer_alloc::registry::HeapCore;
        let allocator = SeferAlloc::new();
        let layout = Layout::from_size_align(258_752, 8).unwrap();
        let before = HeapCore::dbg_prf01_sidecar_oracle();
        let blocks: Vec<_> = (0..34)
            .map(|_| {
                // SAFETY: valid layout; uniquely owned blocks retained until cleanup.
                let block = unsafe { allocator.alloc(layout) };
                assert!(!block.is_null());
                block
            })
            .collect();
        let after = HeapCore::dbg_prf01_sidecar_oracle();
        assert!(after[4] > before[4], "actual discovery construction");
        assert_eq!(after[0] - before[0], 0);
        assert_eq!(after[1] - before[1], 0);
        assert!(after[5] > before[5], "actual summary exchanges");
        // SAFETY: each current unique allocation uses its original layout once.
        unsafe {
            for block in blocks {
                allocator.dealloc(block, layout);
            }
        }
    });
}

// Include production primitive (full geometry), not a reduced Loom shadow.
mod alloc_core {
    pub mod os {
        pub const SEGMENT: usize = sefer_alloc::SegmentLayout::SEGMENT;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = sefer_alloc::SegmentLayout::MIN_BLOCK;
        pub const SMALL_CLASS_COUNT: usize = sefer_alloc::SegmentLayout::SIZE_CLASS_TABLE.len();
    }
}
#[allow(dead_code)] // This fixture only tests summarized range selection.
#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"]
mod remote_bitmap;

#[test]
fn summary_bits_outside_high_water_are_dropped() {
    use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
    use remote_bitmap::SidecarBitmap;
    let pending: Vec<_> = (0..SidecarBitmap::WORDS)
        .map(|_| AtomicU64::new(0))
        .collect();
    let summary: Vec<_> = (0..SidecarBitmap::SUMMARY_WORDS)
        .map(|_| AtomicU64::new(0))
        .collect();
    let classes: Vec<_> = (0..SidecarBitmap::GRANULES)
        .map(|_| AtomicU8::new(0))
        .collect();
    let bitmap = SidecarBitmap::from_initialized(&pending, &summary, &classes).unwrap();
    // W0 below start, W1 inside range, W2/W63 above end. Outside pending
    // sentinels must stay intact: selection must not exchange discarded words.
    for word in [0, 1, 2, 63] {
        pending[word].store(1, Ordering::Relaxed);
    }
    summary[0].store(1 | 2 | 4 | (1u64 << 63), Ordering::Release);
    let mut scan = bitmap.scan_summarized_from(2048, 1).unwrap();
    assert!(!scan.next_cut().unwrap().is_empty());
    assert!(scan.next_cut().is_none());
    assert_eq!(
        summary[0].load(Ordering::Relaxed),
        0,
        "discard, not reinsert"
    );
    assert_eq!(pending[1].load(Ordering::Relaxed), 0);
    for word in [0, 2, 63] {
        assert_eq!(pending[word].load(Ordering::Relaxed), 1);
    }
    assert!(bitmap
        .scan_summarized_from(2048, 1)
        .unwrap()
        .next_cut()
        .is_none());
}
