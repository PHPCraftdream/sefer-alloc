//! Loom's atomics/cells model the production orderings and plain node bytes.
//! The real raw-pointer/provenance seam is covered by the primitive's unit
//! tests; loom checks publication/detach interleavings, not OS lifetime.

#![cfg(loom)]

use loom::cell::UnsafeCell;
use loom::sync::atomic::{AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

const EMPTY: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct Node {
    next: u32,
    class: u32,
}

struct Model {
    head: AtomicU32,
    nodes: [UnsafeCell<Node>; 2],
}

impl Model {
    fn new() -> Self {
        Self {
            head: AtomicU32::new(EMPTY),
            nodes: std::array::from_fn(|_| {
                UnsafeCell::new(Node {
                    next: EMPTY,
                    class: EMPTY,
                })
            }),
        }
    }

    fn publish(&self, own: u32, class: u32) {
        self.nodes[own as usize].with_mut(|p| {
            // SAFETY: each producer uniquely owns its node until CAS.
            unsafe { (*p).class = class };
        });
        let mut expected = self.head.load(Ordering::Acquire);
        loop {
            self.nodes[own as usize].with_mut(|p| {
                // SAFETY: failed CAS keeps this node private.
                unsafe { (*p).next = expected };
            });
            match self
                .head
                .compare_exchange(expected, own, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return,
                Err(current) => expected = current,
            }
        }
    }

    fn cut(&self) -> Vec<(u32, u32)> {
        let mut cursor = self.head.swap(EMPTY, Ordering::AcqRel);
        let mut records = Vec::new();
        while cursor != EMPTY {
            let node = self.nodes[cursor as usize].with(|p| {
                // SAFETY: the acquire swap took the whole published chain;
                // no producer touches a node after successful CAS.
                unsafe { *p }
            });
            records.push((cursor, node.class));
            cursor = node.next;
            assert!(records.len() <= 2, "detached chain must be finite");
        }
        records
    }
}

#[test]
fn concurrent_prepend_and_whole_chain_cut() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = Arc::new(Model::new());
        let a = Arc::clone(&model);
        let b = Arc::clone(&model);
        let owner = Arc::clone(&model);
        let ta = thread::spawn(move || a.publish(0, 10));
        let tb = thread::spawn(move || b.publish(1, 20));
        let tc = thread::spawn(move || owner.cut());
        ta.join().expect("producer A");
        tb.join().expect("producer B");
        let mut observed = tc.join().expect("owner cut");
        observed.extend(model.cut());
        observed.sort_unstable();
        assert_eq!(observed, vec![(0, 10), (1, 20)]);
    });
}
