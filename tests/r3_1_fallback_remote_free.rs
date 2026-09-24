//! A real GlobalAlloc remote free must not lose fallback-owned blocks when
//! the owner is paused or has exited. Each parent test runs in a fresh child:
//! the pre-fix 257th free aborts, which must not kill the test harness.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(miri)
))]

use std::alloc::{GlobalAlloc, Layout};
use std::process::Command;

use sefer_alloc::alloc_core::remote_free_ring::RING_CAP;
use sefer_alloc::alloc_core::segment_header::OWNER_ID_FALLBACK;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{HeapCore, HeapRegistry};
use sefer_alloc::SeferAlloc;

const OVERFLOW_CAP: usize = 2048;
const CASE_ENV: &str = "SEFER_R3_1_CHILD_CASE";

fn child(case: &str) {
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .arg("--exact")
        .arg("fallback_remote_child")
        .arg("--ignored")
        .arg("--nocapture")
        .env(CASE_ENV, case)
        .output()
        .expect("run fallback child");
    assert!(
        output.status.success(),
        "fallback case {case} failed: status={}\nstdout={}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fallback_ring_cap_plus_one_paused_owner() {
    child("ring-plus-one");
}

#[test]
fn fallback_beyond_combined_capacity_reaches_spill() {
    child("spill");
}

#[test]
fn fallback_spill_backlog_survives_bounded_drain() {
    child("spill-multipass");
}

#[test]
fn fallback_owner_exit_preserves_remote_free() {
    child("exited");
}

#[test]
fn registry_owner_does_not_route_to_fallback_overflow() {
    child("wrong-owner");
}

#[test]
fn fallback_oom_rolls_back_before_binding() {
    child("oom");
}

#[test]
#[ignore = "run only in a fresh subprocess"]
fn fallback_remote_child() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    if case == "wrong-owner" {
        wrong_owner();
        return;
    }
    if case == "oom" {
        oom();
        return;
    }
    let n = match case.as_str() {
        "ring-plus-one" | "exited" => RING_CAP + 1,
        "spill" => RING_CAP + OVERFLOW_CAP + 1,
        "spill-multipass" => RING_CAP + OVERFLOW_CAP * 2 + 1,
        _ => panic!("unknown child case: {case}"),
    };
    let layout = Layout::from_size_align(16, 16).expect("16-byte layout");

    let allocate = move || {
        HeapCore::dbg_with_fallback_for_test(|heap| {
            let mut blocks = Vec::with_capacity(n + 1);
            for _ in 0..=n {
                let ptr = heap.alloc(layout);
                assert!(!ptr.is_null(), "fallback allocation failed");
                blocks.push(ptr as usize);
            }
            let base = heap.dbg_segment_base_of_ptr(blocks[0] as *mut u8);
            assert!(blocks
                .iter()
                .all(|&p| heap.dbg_segment_base_of_ptr(p as *mut u8) == base));
            assert_eq!(
                heap.dbg_owner_id_for(blocks[0] as *mut u8),
                Some(OWNER_ID_FALLBACK)
            );
            #[cfg(feature = "alloc-segment-directory")]
            assert!(!heap.dbg_has_dirty_bitmap_for_test());
            let before = heap
                .dbg_live_count_for(blocks[0] as *mut u8)
                .expect("small segment");
            (blocks, before)
        })
        .expect("fallback bootstrap must succeed")
    };

    let (blocks, before) = if case == "exited" {
        std::thread::spawn(allocate).join().expect("owner thread")
    } else {
        allocate()
    };
    let survivor = blocks[n];
    let freed = blocks[..n].to_vec();
    let freer = std::thread::spawn(move || {
        let alloc = SeferAlloc::new();
        for addr in freed {
            // SAFETY: each address is a distinct, live fallback allocation
            // with this exact layout, transferred to this thread once.
            unsafe { alloc.dealloc(addr as *mut u8, layout) };
        }
    });
    freer.join().expect("remote freer");

    HeapCore::dbg_with_fallback_for_test(|heap| {
        let (head, tail) = heap
            .dbg_segment_ring_cursors_for_test(survivor as *mut u8)
            .expect("owned segment ring");
        assert_eq!(head, 0, "owner unexpectedly drained while paused");
        assert_eq!(tail as usize, RING_CAP, "segment ring should be saturated");
        let (overflow_head, overflow_tail) = heap.dbg_overflow_cursors_for_test();
        assert_eq!(overflow_head, 0);
        assert_eq!(overflow_tail, (n - RING_CAP).min(OVERFLOW_CAP));
        let (spill_pushed, spill_popped) = heap.dbg_spill_ledger_for_test();
        assert_eq!(spill_pushed, n.saturating_sub(RING_CAP + OVERFLOW_CAP));
        assert_eq!(spill_popped, 0);

        // A real owner allocation on a fresh class enters the magazine-miss
        // slow path, which must drain fallback overflow and its spill even
        // though no registry-slot dirty bit can wake it.
        let trigger_layout =
            Layout::from_size_align(AllocCore::dbg_block_size(40), 8).expect("trigger layout");
        let trigger = heap.alloc(trigger_layout);
        assert!(!trigger.is_null());
        let target_base = heap.dbg_segment_base_of_ptr(survivor as *mut u8);
        let mut extra_in_target = u32::from(heap.dbg_segment_base_of_ptr(trigger) == target_base);
        let (overflow_head, overflow_tail) = heap.dbg_overflow_cursors_for_test();
        assert_eq!(
            overflow_head, overflow_tail,
            "owner alloc did not drain overflow"
        );
        let (_, spill_popped) = heap.dbg_spill_ledger_for_test();
        let mut second_trigger = None;
        if case == "spill-multipass" {
            assert_eq!(spill_popped, OVERFLOW_CAP);
            assert!(heap.dbg_spill_pending_for_test());
            let second_layout = Layout::from_size_align(AllocCore::dbg_block_size(41), 8)
                .expect("second trigger layout");
            let ptr = heap.alloc(second_layout);
            assert!(!ptr.is_null());
            extra_in_target += u32::from(heap.dbg_segment_base_of_ptr(ptr) == target_base);
            second_trigger = Some((ptr, second_layout));
        }
        let (spill_pushed, spill_popped) = heap.dbg_spill_ledger_for_test();
        assert_eq!(
            spill_pushed, spill_popped,
            "owner alloc did not drain spill"
        );
        heap.dbg_drain_all_rings();
        let (head, tail) = heap
            .dbg_segment_ring_cursors_for_test(survivor as *mut u8)
            .expect("owned segment ring");
        assert_eq!(head, tail, "segment ring retains occupied slots");
        let (overflow_head, overflow_tail) = heap.dbg_overflow_cursors_for_test();
        assert_eq!(
            overflow_head, overflow_tail,
            "overflow ring retains occupied slots"
        );
        let (spill_pushed, spill_popped) = heap.dbg_spill_ledger_for_test();
        assert_eq!(spill_pushed, spill_popped, "spill lost a legal free");
        assert!(!heap.dbg_spill_pending_for_test());
        assert_eq!(
            heap.dbg_live_count_for(survivor as *mut u8),
            Some(before + extra_in_target - n as u32),
            "accepted frees must all be reclaimed"
        );
        for &addr in &blocks[..n] {
            assert!(
                heap.dbg_is_free_for(addr as *mut u8),
                "unreclaimed block {addr:#x}"
            );
        }
        // SAFETY: the survivor has not been freed and still belongs to this
        // heap; it keeps the segment mapped through the post-drain oracles.
        unsafe { heap.dealloc(survivor as *mut u8, layout) };
        // SAFETY: the trigger was allocated by this owner and not shared.
        unsafe { heap.dealloc(trigger, trigger_layout) };
        if let Some((ptr, second_layout)) = second_trigger {
            // SAFETY: the second trigger was likewise allocated by this
            // owner and has not been shared or freed.
            unsafe { heap.dealloc(ptr, second_layout) };
        }
    })
    .expect("fallback owner resumes");
}

fn wrong_owner() {
    let layout = Layout::from_size_align(16, 16).expect("16-byte layout");
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null());
    // SAFETY: claim gave this thread exclusive ownership of a live heap.
    let heap = unsafe { &mut *heap_ptr };
    let mut blocks = Vec::with_capacity(RING_CAP + 2);
    for _ in 0..RING_CAP + 2 {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        blocks.push(ptr as usize);
    }
    let survivor = blocks[RING_CAP + 1] as *mut u8;
    let base = heap.dbg_segment_base_of_ptr(survivor);
    assert!(blocks
        .iter()
        .all(|&p| heap.dbg_segment_base_of_ptr(p as *mut u8) == base));
    assert_ne!(heap.dbg_owner_id_for(survivor), Some(OWNER_ID_FALLBACK));
    let before = HeapCore::dbg_with_fallback_for_test(|h| h.dbg_overflow_cursors_for_test())
        .expect("fallback bootstrap");
    let freed = blocks[..RING_CAP + 1].to_vec();
    std::thread::spawn(move || {
        for addr in freed {
            // SAFETY: each distinct live block was transferred from the
            // registry owner and freed once with its original layout.
            unsafe { SeferAlloc::new().dealloc(addr as *mut u8, layout) };
        }
    })
    .join()
    .expect("remote free");
    let after = HeapCore::dbg_with_fallback_for_test(|h| h.dbg_overflow_cursors_for_test())
        .expect("fallback remains live");
    assert_eq!(before, after, "registry owner was misrouted to fallback");
    assert_eq!(heap.dbg_overflow_cursors_for_test(), (0, 1));
    heap.dbg_drain_heap_overflow_for_test();
    heap.dbg_drain_all_rings();
    for &addr in &blocks[..RING_CAP + 1] {
        assert!(heap.dbg_is_free_for(addr as *mut u8));
    }
    // SAFETY: the survivor is still live on this owned heap.
    unsafe { heap.dealloc(survivor, layout) };
    // SAFETY: this thread claimed the heap, and no other thread accesses its
    // owner-only fields; the remote free has joined and been drained.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}

fn oom() {
    HeapCore::dbg_inject_fallback_oom_for_test(true);
    assert!(
        HeapCore::dbg_with_fallback_for_test(|_| ()).is_none(),
        "forced primordial OOM must not publish an unbound heap"
    );
    HeapCore::dbg_inject_fallback_oom_for_test(false);
    HeapCore::dbg_with_fallback_for_test(|heap| {
        let layout = Layout::from_size_align(16, 16).expect("16-byte layout");
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null(), "retry after OOM must initialise fallback");
        assert_eq!(heap.dbg_owner_id_for(ptr), Some(OWNER_ID_FALLBACK));
        // SAFETY: this block was just allocated from the fallback heap and
        // has not been shared; its exact original layout is used.
        unsafe { heap.dealloc(ptr, layout) };
    })
    .expect("fallback must retry after primordial OOM");
}
