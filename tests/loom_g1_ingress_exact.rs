//! SC-6 A-exact (P23) actual-source Loom model: the REAL
//! `src/registry/segment_route/directory/ingress.rs` (per-route ARMED /
//! NOTIFIED state and the per-heap notification cell) compiled on loom
//! atomics. A segment's pending word is one loom `AtomicU64`; the producer
//! publishes into it with the production terminal ordering (`fetch_or`
//! SeqCst, pinned by `sidecar/publish-acqrel`) and then runs the real
//! `after_publish`; the owner runs the real `take` / `arm` and then drains the
//! word (`swap(0, AcqRel)`, as the production cut).
//!
//! Oracle (P23 I1/I2): after both threads finish, every published bit is
//! either drained by the owner or still covered by an outstanding
//! notification (route NOTIFIED and its slot bit in the heap mask). A lost
//! bit would be a free block the owner trusts is absent.
#![cfg(loom)]

mod alloc_core {
    pub(crate) mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            use std::io::Write;
            let _ = std::io::stderr().write_all(msg);
            std::process::abort()
        }
    }
}

// The included production file resolves `super::` atomics; these are loom's.
pub(crate) use loom::sync::atomic::{fence, AtomicU64, AtomicU8, Ordering};

#[path = "../src/ghost/mod.rs"]
mod ghost;
#[path = "../src/registry/segment_route/directory/ingress.rs"]
mod ingress;

use ingress::{HeapIngress, SegmentIngress};
use loom::sync::Arc;
use loom::thread;

const SLOT: u32 = 7;

struct World {
    seg: SegmentIngress,
    heap: HeapIngress,
    pending: AtomicU64,
}

fn produce(w: &World, bit: u64) {
    w.pending.fetch_or(bit, Ordering::SeqCst);
    w.seg.after_publish(|| Some(&w.heap));
}

/// Owner miss: take notifications, re-arm the notified route, fully drain it.
fn owner_round(w: &World) -> u64 {
    let mut slots = Vec::new();
    w.heap.take(|s| slots.push(s));
    let mut drained = 0;
    for s in slots {
        assert_eq!(s, SLOT as usize, "only the one route notifies");
        w.seg.arm();
        drained |= w.pending.swap(0, Ordering::AcqRel);
    }
    drained
}

/// Outstanding notification: route NOTIFIED and its slot bit still in the cell.
fn outstanding(w: &World) -> bool {
    let mut taken = false;
    w.heap.take(|s| taken |= s == SLOT as usize);
    taken && w.seg.is_notified()
}

/// L1, store-buffering core: the route is NOTIFIED with its bit in the cell
/// (an earlier notification), the owner takes, re-arms and drains while a new
/// publication races with it.
#[test]
fn l1_publish_races_take_arm_drain() {
    loom::model(|| {
        let w = Arc::new(World {
            seg: SegmentIngress::new(SLOT, true),
            heap: HeapIngress::new(),
            pending: AtomicU64::new(0),
        });
        // Earlier notification: ARMED -> NOTIFIED with the slot bit set.
        w.seg.after_publish(|| Some(&w.heap));
        let p = {
            let w = w.clone();
            thread::spawn(move || produce(&w, 1))
        };
        let drained = owner_round(&w);
        p.join().unwrap();
        let rest = w.pending.load(Ordering::Relaxed);
        assert!(
            drained & 1 != 0 || (rest & 1 != 0 && outstanding(&w)),
            "lost publication: drained={drained:#x} rest={rest:#x}"
        );
    });
}

/// L2: a fresh ARMED route; the owner trusts its negative answer, so the
/// producer must leave a notification behind.
#[test]
fn l2_fresh_route_notifies() {
    loom::model(|| {
        let w = Arc::new(World {
            seg: SegmentIngress::new(SLOT, true),
            heap: HeapIngress::new(),
            pending: AtomicU64::new(0),
        });
        let p = {
            let w = w.clone();
            thread::spawn(move || produce(&w, 1))
        };
        let drained = owner_round(&w);
        p.join().unwrap();
        let rest = w.pending.load(Ordering::Relaxed);
        assert!(drained & 1 != 0 || (rest & 1 != 0 && outstanding(&w)));
    });
}

/// L3: two producers publish different bits against one owner round.
#[test]
fn l3_two_producers_one_owner_round() {
    loom::model(|| {
        let w = Arc::new(World {
            seg: SegmentIngress::new(SLOT, true),
            heap: HeapIngress::new(),
            pending: AtomicU64::new(0),
        });
        let ps: Vec<_> = [1u64, 2]
            .into_iter()
            .map(|bit| {
                let w = w.clone();
                thread::spawn(move || produce(&w, bit))
            })
            .collect();
        let drained = owner_round(&w);
        for p in ps {
            p.join().unwrap();
        }
        let rest = w.pending.load(Ordering::Relaxed);
        assert_eq!(drained | rest, 3, "every bit is drained or still pending");
        if rest != 0 {
            assert!(
                outstanding(&w),
                "pending bits {rest:#x} without a notification"
            );
        }
    });
}
