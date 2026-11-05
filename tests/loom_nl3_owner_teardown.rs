//! NL3 lease recycle/trim handoff model. Actual sidecar/cut/scan publication source
//! and the modeled Release/Acquire lease handoff are covered. This does not model
//! production raw-pointer reclaim, which still reads the state byte in
//! `src/alloc_core/small/alloc_core_small_reclaim.rs`; no concurrent AllocCore Drop/registry
//! teardown is modeled. `UnsafeCell` free identities/credits are model bookkeeping.
#![cfg(all(
    loom,
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals"
))]
mod alloc_core {
    // Mirrors `alloc_core::state_byte::StateByte::shadow_reject` for the source-included
    // `SidecarBitmap::publish` (a debug-build abort for a rejected misuse; loom models do not take it).
    #[allow(dead_code)]
    pub mod state_byte {
        pub struct StateByte;
        impl StateByte {
            pub fn shadow_reject(_site: &'static [u8]) {}
        }
    }
    pub mod fatal {
        pub fn fatal(_: &'static [u8]) -> ! {
            panic!("fatal in Loom model")
        }
    }
    pub mod os {
        pub const SEGMENT: usize = 3 * 64 * 64;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = 64;
        pub const SMALL_CLASS_COUNT: usize = 8;
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize = 64 * 64;
    }
}
pub(crate) use loom::sync::atomic::{AtomicPtr, AtomicU64, AtomicU8, Ordering};
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_cut.rs"]
mod bitmap_cut;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_record.rs"]
mod bitmap_record;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"]
mod bitmap_scan;
#[path = "../src/ghost/mod.rs"]
mod ghost;
#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"]
mod sidecar_bitmap;
pub(crate) use bitmap_cut::BitmapCut;
pub(crate) use bitmap_record::BitmapRecord;
pub(crate) use bitmap_scan::BitmapScan;
mod remote_bitmap {
    pub(crate) use super::bitmap_cut::BitmapCut;
    pub(crate) use super::bitmap_record::BitmapRecord;
    pub(crate) use super::bitmap_scan::BitmapScan;
    pub(crate) use super::sidecar_bitmap::SidecarBitmap;
}
use loom::cell::UnsafeCell;
use loom::sync::Arc;
use loom::thread;
use remote_bitmap::SidecarBitmap;
use sefer_alloc::registry::heap_slot::SlotState;

type Id = (u32, u8);
struct Backing {
    words: Vec<AtomicU64>,
    classes: Vec<AtomicU8>,
}
impl Backing {
    fn new() -> Self {
        Self {
            words: (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
            classes: (0..SidecarBitmap::GRANULES)
                .map(|_| AtomicU8::new(0))
                .collect(),
        }
    }
    fn map(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_initialized(&self.words, &self.classes).unwrap()
    }
}
struct OwnerState {
    free: UnsafeCell<Vec<Id>>,
    credits: UnsafeCell<usize>,
    state: AtomicU8,
}
// ghost-bind: heap-lease/claim-free-live
const CLAIM: (Ordering, Ordering) = (Ordering::AcqRel, Ordering::Acquire);
fn claim(s: &OwnerState) -> bool {
    s.state
        .compare_exchange(
            SlotState::Free as u8,
            SlotState::Live as u8,
            CLAIM.0,
            CLAIM.1,
        )
        .is_ok()
}
// ghost-bind: heap-lease/drop-release
const DROP_LEASE: (Ordering, Ordering) = (Ordering::Release, Ordering::Relaxed);

struct Lease<'a> {
    state: &'a OwnerState,
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        release(self.state);
    }
}
fn trim_for_recycle(b: &Backing, s: &OwnerState) {
    drain_owner(b, s);
}
fn modeled_teardown(b: &Backing, s: &OwnerState, lease: Lease<'_>) {
    // ghost-bind: teardown/trim-before-release
    trim_for_recycle(b, s);
    drop(lease);
}

fn release(s: &OwnerState) {
    assert!(s
        .state
        .compare_exchange(
            SlotState::Live as u8,
            SlotState::Free as u8,
            DROP_LEASE.0,
            DROP_LEASE.1
        )
        .is_ok());
}
fn insert_record(s: &OwnerState, record: Id, double_retire: bool) {
    s.free.with_mut(|p| {
        // SAFETY: caller holds the unique owner lease through insertion.
        unsafe {
            (*p).push(record);
            if double_retire {
                (*p).push(record);
            }
        }
    });
    s.credits.with_mut(|p| {
        // SAFETY: caller holds the unique owner lease through credit retirement.
        unsafe {
            assert!(*p > 0, "credit underflow");
            *p -= 1;
        }
    });
}
fn drain_owner(b: &Backing, s: &OwnerState) {
    let mut scan = b.map().scan(3 * 64 * 64).unwrap();
    let mut steps = 0;
    while let Some(mut cut) = scan.next_cut() {
        while let Some(r) = cut.pop() {
            steps += 1;
            assert!(steps <= 2, "cut failed to retire bits");
            insert_record(s, (r.offset, r.class + 1), false);
        }
    }
}
fn snapshot(s: &OwnerState) -> (Vec<Id>, usize) {
    let free = s.free.with(|p| {
        // SAFETY: called by the current lease owner, or after all owners join.
        unsafe { (*p).clone() }
    });
    let credits = s.credits.with(|p| {
        // SAFETY: called by the current lease owner, or after all owners join.
        unsafe { *p }
    });
    (free, credits)
}
fn assert_outstanding_conservation(s: &OwnerState) {
    let (free, credits) = snapshot(s);
    assert!(free.iter().all(|id| *id == (1024, 2)));
    assert!(free.len() <= 1);
    assert_eq!(free.len() + credits, 2);
    s.credits.with_mut(|p| {
        // SAFETY: caller holds the unique lease; this owner mutation exercises handoff.
        unsafe { assert_eq!(*p, credits) }
    });
}

#[test]
fn teardown_credit_survives_cut_then_cross_thread_owner_drains_and_reuses_taken_block() {
    loom::model(|| {
        let b = Arc::new(Backing::new());
        let s = Arc::new(OwnerState {
            free: UnsafeCell::new(Vec::new()),
            credits: UnsafeCell::new(2),
            state: AtomicU8::new(SlotState::Live as u8),
        });
        assert!(b.map().issue(0, 0));
        assert!(b.map().issue(1024, 1));
        let producer = {
            let b = b.clone();
            thread::spawn(move || {
                thread::yield_now();
                assert!(b.map().publish(1024));
            })
        };
        let claimant_b = b.clone();
        let claimant_s = s.clone();
        // Claimant starts before any owner bookkeeping mutation. It can only
        // touch the cells after winning the Release/Acquire lease transfer.
        let claimant = thread::spawn(move || {
            if claim(&claimant_s) {
                let lease = Lease { state: &claimant_s };
                assert_outstanding_conservation(&claimant_s);
                modeled_teardown(&claimant_b, &claimant_s, lease);
                true
            } else {
                false
            }
        });
        // Initial owner-side bookkeeping remains under its live lease.
        let initial_lease = Lease { state: &s };
        assert_outstanding_conservation(&s);
        modeled_teardown(&b, &s, initial_lease);
        producer.join().unwrap();
        let _claimant_won = claimant.join().unwrap();
        // Recovery owner handles a publication later than claimant's fixed cut.
        assert!(claim(&s));
        drain_owner(&b, &s);
        let mut sorted = snapshot(&s).0;
        sorted.sort_unstable();
        assert!(sorted.iter().all(|id| *id == (1024, 2)));
        assert!(sorted.len() <= 1);
        let credits = snapshot(&s).1;
        assert_eq!(sorted.len() + credits, 2);
        assert_eq!(credits, 2 - sorted.len());
        assert!(
            sorted.contains(&(1024, 2)),
            "published block must be reclaimed"
        );
        // Take the published identity, independent of its free-list position.
        let reused = s.free.with_mut(|p| {
            // SAFETY: current thread won the lease and is the sole owner.
            let list = unsafe { &mut *p };
            let index = list.iter().position(|id| *id == (1024, 2)).unwrap();
            list.swap_remove(index)
        });
        s.credits.with_mut(|p| {
            // SAFETY: current thread retains the exclusive lease.
            unsafe { *p += 1 }
        });
        let (free_after_take, credits_after_take) = snapshot(&s);
        assert!(free_after_take.is_empty());
        assert_eq!(credits_after_take, 2);
        assert!(b.map().issue(reused.0, reused.1 - 1));
        assert!(b.map().publish(reused.0));
        drain_owner(&b, &s);
        let (reused_free, reused_credits) = snapshot(&s);
        assert_eq!(reused_free, vec![(1024, 2)]);
        assert_eq!(reused_credits, 1);
        release(&s);
    });
}

#[test]
fn prepublished_same_word_pair_preserves_both_outstanding_credits() {
    loom::model(|| {
        let b = Backing::new();
        let s = OwnerState {
            free: UnsafeCell::new(Vec::new()),
            credits: UnsafeCell::new(2),
            state: AtomicU8::new(SlotState::Live as u8),
        };
        assert!(b.map().issue(64, 0));
        assert!(b.map().issue(128, 1));
        assert!(b.map().publish(64));
        assert!(b.map().publish(128));
        let lease = Lease { state: &s };
        modeled_teardown(&b, &s, lease);
        let (mut free, credits) = snapshot(&s);
        free.sort_unstable();
        assert_eq!(free, vec![(64, 1), (128, 2)]);
        assert_eq!(credits, 0);
    });
}

#[test]
#[should_panic(expected = "owner identity/class multiset")]
fn modeled_owner_double_retirement_negative_control() {
    loom::model(|| {
        let s = OwnerState {
            free: UnsafeCell::new(Vec::new()),
            credits: UnsafeCell::new(1),
            state: AtomicU8::new(SlotState::Live as u8),
        };
        insert_record(&s, (64, 1), true);
        let actual = snapshot(&s).0;
        assert_eq!(actual, vec![(64, 1)], "owner identity/class multiset");
    });
}
