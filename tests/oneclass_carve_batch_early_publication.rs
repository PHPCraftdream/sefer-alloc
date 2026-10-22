//! D1′ round fix 3 — M-h: the batch carve must not publish a block's start
//! bit before that block's own per-block commit point.
//!
//! Production order inside `try_carve_batch` (one run, owner thread only):
//!   1. commit/decommit re-check (fallible, returns 0 on refusal),
//!   2. grow-on-carve `commit_pages` (fallible, returns 0 on refusal),
//!   3. `set_bump` ONCE past the whole batch,
//!   4. `add_live(n)` ONCE,
//!   5. PER BLOCK i, in order: `clear_pending_start_bit(off_i)` →
//!      `issue_run.issue(off_i)` (START BIT) → LIVE state byte.
//!
//! The early-publication defect window is therefore exactly
//! `[set_bump, first per-block issue]`: a producer thread publishing a
//! batch block there must observe start bit 0 (`publish == false`), because
//! that block has not reached its own commit point yet. Under the M-h
//! mutation (one mass issue of ALL batch start bits right after step 4)
//! every batch block is publicable while the run is still inside the loop.
//!
//! The oracle parks the owner INSIDE the window through the test-only seam
//! (`CARVE_BATCH_STAGE` / `CARVE_BATCH_PAUSE` / `CARVE_BATCH_RESUME`,
//! compiled out of every build without `internals`/`bench-internals`) and,
//! from the producer side, probes every `n = stage` batch offset with
//!   a) the read-only sidecar observer
//!      `RouteRegistration::start_state_at_global_address_for_test`, and
//!   b) the real producer terminal-publication path (`RoutePin::
//!      publish_small`, deliberate negative probe — a `false` verdict has
//!      no side effect),
//! then releases the owner. The batch start offsets are the anchor
//! segment's block-aligned bump cursor read BEFORE the decisive refill, so
//! the probe is exact, not a scan.
//!
//! Counterfactual: M-h (mass issue hoisted out of the per-block loop) makes
//! probe (a) see start bit 1 and probe (b) return `true` for the not-yet-
//! issued batch blocks → RED. The clean tree keeps both probes `false`/0 →
//! GREEN. No wall clock: the handshake is two atomic flags.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals",
    feature = "fastbin"
))]

use core::alloc::Layout;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RouteRegistration};
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::AllocCore;
use sefer_alloc::SegmentLayout;

const SEGMENT: usize = SegmentLayout::SEGMENT;
const CLASS_SIZE: usize = 32;

fn layout() -> Layout {
    Layout::from_size_align(CLASS_SIZE, 16).unwrap()
}

// One lease/heap per process; serialize against the other oneclass suites.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Full producer terminal-publication path through the global directory pin
/// (the same negative-probe pattern as `oneclass_starts_map::try_publish`).
/// Returns the publication verdict verbatim.
unsafe fn try_publish(ptr: *mut u8) -> bool {
    let Some(pin) = RouteDirectory::global().lookup(ptr) else {
        return false;
    };
    if !matches!(pin.kind(), RouteKind::Small | RouteKind::Primordial) {
        return false;
    }
    let offset = (ptr.addr() & (SEGMENT - 1)) as u32;
    // SAFETY: deliberate negative probe (see `oneclass_starts_map`): the
    // probe publishes a never-issued in-carve address and relies on the
    // filter's `false`; a `false` verdict leaves no side effect behind.
    unsafe { pin.publish_small(offset) }
}

/// Releases the parked owner even when an assertion unwinds — otherwise a
/// RED run would deadlock the join instead of failing.
struct ResumeOnDrop;
impl Drop for ResumeOnDrop {
    fn drop(&mut self) {
        AllocCore::dbg_carve_batch_resume();
    }
}

/// (bump, live, head) of the segment serving `ptr` — owner thread only.
fn owner_state(ptr: *mut u8, c: usize) -> (usize, u32, u32) {
    use sefer_alloc::registry::segment_route::SmallSidecar;
    // SAFETY: the heap holding `ptr` is claimed and exclusively owned by
    // this thread for the whole read (registry-sidecar fixture pattern).
    unsafe { SmallSidecar::owner_state_for_test(ptr.addr(), c) }.unwrap()
}

#[test]
fn no_block_of_an_in_flight_carve_batch_is_publicable() {
    let _guard = serialize();
    std::thread::scope(|scope| {
        let ready = Arc::new(AtomicBool::new(false));
        let base = Arc::new(AtomicUsize::new(0));
        let bump0 = Arc::new(AtomicUsize::new(0));

        let owner = {
            let (base, bump0, ready) = (base.clone(), bump0.clone(), ready.clone());
            scope.spawn(move || {
                let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
                let heap = lease.core();
                let c = heap.dbg_class_for(layout()).expect("32B is a small class");
                let want = heap.dbg_refill_n_for_class(c);
                assert!(want >= 2, "fixture needs a refill amount of at least 2");

                let anchor = heap.alloc(layout());
                assert!(!anchor.is_null());
                let mut held: Vec<*mut u8> = vec![anchor];
                // Warm-up: the magazine has been filled and drained at least once,
                // the substrate freelist is empty. This many 32B blocks never spills
                // past the anchor segment, so the refill's batch carve below runs on
                // the anchor segment (`small_cur == anchor`).
                for _ in 0..(2 * want + 1) {
                    let ptr = heap.alloc(layout());
                    assert!(!ptr.is_null());
                    held.push(ptr);
                }
                assert_eq!(owner_state(anchor, c).2, u32::MAX, "substrate empty");

                // Match the r11_ph3a fixture exactly: free ONE block (it parks in
                // the magazine), then drain the magazine with pure hits — so both
                // the magazine AND the substrate freelist are empty before the
                // overflow step.
                {
                    let ptr = held.pop().expect("held stock");
                    // SAFETY: `ptr` came from this heap with this exact layout.
                    unsafe { heap.dealloc(ptr, layout()) };
                }
                let mut resident = heap.dbg_tcache_count(c) as usize;
                while resident > 0 {
                    let ptr = heap.alloc(layout());
                    assert!(!ptr.is_null());
                    held.push(ptr);
                    resident = heap.dbg_tcache_count(c) as usize;
                }
                assert_eq!(resident, 0, "the magazine must be empty here");
                assert_eq!(owner_state(anchor, c).2, u32::MAX);

                // Overflow the park cap by exactly one block (the r11_ph3a fixture
                // pattern): this puts k ∈ [1, want) blocks on the anchor segment's
                // substrate freelist, then drain the magazine back to zero.
                let tcache_cap = heap
                    .dbg_class_for(Layout::from_size_align(16, 16).unwrap())
                    .map(|c16| heap.dbg_refill_n_for_class(c16))
                    .expect("16B is a small class");
                let k = if want == tcache_cap { want / 2 } else { 1 };
                for _ in 0..(want + 1) {
                    let ptr = held.pop().expect("held stock");
                    // SAFETY: `ptr` came from this heap with this exact layout.
                    unsafe { heap.dealloc(ptr, layout()) };
                }
                let mut resident = heap.dbg_tcache_count(c) as usize;
                while resident > 0 {
                    let ptr = heap.alloc(layout());
                    assert!(!ptr.is_null());
                    held.push(ptr);
                    resident = heap.dbg_tcache_count(c) as usize;
                }
                assert_ne!(
                    owner_state(anchor, c).2,
                    u32::MAX,
                    "the overflow must have put blocks on the substrate freelist"
                );
                assert!(
                    (1..want).contains(&k),
                    "the fixture must leave a short-but-nonempty freelist (k = {k})"
                );

                // The refill below drains the k freelist blocks, then batch-carves
                // `n = want - k` fresh blocks from the anchor segment's bump. Read
                // that bump NOW — the drain never moves it, so the carve's
                // `aligned_start` is exactly this block-aligned cursor.
                let bump = owner_state(anchor, c).0;
                let aligned = bump.div_ceil(CLASS_SIZE) * CLASS_SIZE;
                base.store(anchor.addr() & !(SEGMENT - 1), Ordering::Release);
                bump0.store(aligned, Ordering::Release);

                // Arm the window pause, then take the magazine miss that drives the
                // refill's `try_carve_batch`. The owner parks inside the window
                // until the producer side releases it.
                AllocCore::dbg_carve_batch_pause(true);
                ready.store(true, Ordering::Release);
                let issued = heap.alloc(layout());
                assert!(!issued.is_null(), "the refill must serve the miss");
                assert_eq!(
                    AllocCore::dbg_carve_batch_stage(),
                    0,
                    "the window must be closed again after the batch"
                );
                AllocCore::dbg_carve_batch_pause(false);
                held.push(issued);
                // The issued block and the parked magazine residents all came from
                // the anchor segment's refill.
                assert_eq!(
                    (issued.addr() & !(SEGMENT - 1)),
                    anchor.addr() & !(SEGMENT - 1),
                    "the decisive refill must have been served by the anchor segment"
                );

                while let Some(ptr) = held.pop() {
                    // SAFETY: each block came from this heap with this exact layout.
                    unsafe { heap.dealloc(ptr, layout()) };
                }
                drop(lease);
            })
        };
        // Producer side: wait for the armed owner, then for the in-window stage.
        while !ready.load(Ordering::Acquire) {
            assert!(!owner.is_finished(), "owner завершился до handshake");
            std::thread::yield_now();
        }
        let _resume = ResumeOnDrop;
        let n = loop {
            let stage = AllocCore::dbg_carve_batch_stage();
            if stage != 0 {
                break stage;
            }
            assert!(!owner.is_finished(), "owner завершился до carve-stage");
            std::thread::yield_now();
        };
        let base = base.load(Ordering::Acquire);
        let first = bump0.load(Ordering::Acquire);
        assert!(n >= 1, "the parked batch must carve at least one block");
        for i in 0..n {
            let addr = base + first + i * CLASS_SIZE;
            // (a) read-only sidecar observer: the start bit of a batch block
            // whose per-block issue has not run yet MUST be 0.
            let (start, _pending) = RouteRegistration::start_state_at_global_address_for_test(addr)
                .expect("the batch block's segment is routed");
            assert!(
                !start,
                "M-h: start bit of in-carve batch block {i}/{n} is visible \
             before its per-block issue point",
            );
            // (b) the real producer publication: must refuse (filter's `false`,
            // no side effect on the clean tree).
            // SAFETY: deliberate negative probe (see `try_publish`).
            assert!(
                !unsafe { try_publish(addr as *mut u8) },
                "M-h: a never-issued in-carve batch block {i}/{n} published",
            );
        }
        AllocCore::dbg_carve_batch_resume();
        owner.join().expect("owner thread");
    });
}
