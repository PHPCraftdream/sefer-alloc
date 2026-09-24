//! Bounded model of the intrusive third-tier MPSC stack's CAS protocol.
//! Node indices stand for distinct live blocks; relaxed atomics model the
//! in-block next/ready words under Release/Acquire publication.

#![cfg(loom)]

use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

struct Spill {
    head: AtomicUsize,
    next: [AtomicUsize; 2],
    ready: [AtomicUsize; 2],
    seen: [AtomicUsize; 2],
}

impl Spill {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            head: AtomicUsize::new(0),
            next: std::array::from_fn(|_| AtomicUsize::new(0)),
            ready: std::array::from_fn(|_| AtomicUsize::new(0)),
            seen: std::array::from_fn(|_| AtomicUsize::new(0)),
        })
    }

    fn push(&self, id: usize) {
        let previous = self.begin_push(id);
        self.finish_push(id, previous);
    }

    fn begin_push(&self, id: usize) -> usize {
        self.ready[id - 1].store(0, Ordering::Relaxed);
        self.next[id - 1].store(0, Ordering::Relaxed);
        self.head.swap(id, Ordering::AcqRel)
    }

    fn finish_push(&self, id: usize, previous: usize) {
        self.next[id - 1].store(previous, Ordering::Relaxed);
        self.ready[id - 1].store(1, Ordering::Release);
    }

    fn pop(&self, broken_clear: bool) -> Option<usize> {
        let mut head = self.head.load(Ordering::Acquire);
        loop {
            if head == 0 {
                return None;
            }
            if self.ready[head - 1].load(Ordering::Acquire) == 0 {
                return None;
            }
            let next = self.next[head - 1].load(Ordering::Relaxed);
            if broken_clear {
                // Counterfactual: a consumer clearing a sampled head can
                // erase a producer's intervening prepend.
                self.head.store(next, Ordering::Release);
                self.seen[head - 1].fetch_add(1, Ordering::Relaxed);
                return Some(head);
            }
            match self
                .head
                .compare_exchange_weak(head, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    self.seen[head - 1].fetch_add(1, Ordering::Relaxed);
                    return Some(head);
                }
                Err(observed) => head = observed,
            }
        }
    }
}

fn check(broken_clear: bool) {
    loom::model(move || {
        let spill = Spill::new();
        spill.push(1);
        let producer = {
            let spill = Arc::clone(&spill);
            thread::spawn(move || spill.push(2))
        };
        let consumer = {
            let spill = Arc::clone(&spill);
            thread::spawn(move || {
                let _ = spill.pop(broken_clear);
            })
        };
        producer.join().unwrap();
        consumer.join().unwrap();
        while spill.pop(false).is_some() {}
        assert_eq!(
            spill.seen[0].load(Ordering::Relaxed),
            1,
            "lost or duplicated note"
        );
        assert_eq!(
            spill.seen[1].load(Ordering::Relaxed),
            1,
            "lost or duplicated note"
        );
        assert_eq!(spill.head.load(Ordering::Relaxed), 0);
    });
}

#[test]
fn concurrent_prepend_and_pop_are_lossless() {
    check(false);
}

#[test]
fn unpublished_swap_blocks_pop_until_link_ready() {
    loom::model(|| {
        let spill = Spill::new();
        spill.push(1);
        let previous = spill.begin_push(2);
        assert_eq!(previous, 1);
        assert_eq!(spill.pop(false), None);
        spill.finish_push(2, previous);
        assert_eq!(spill.pop(false), Some(2));
        assert_eq!(spill.pop(false), Some(1));
    });
}

#[test]
#[should_panic(expected = "lost or duplicated note")]
fn counterfactual_unconditional_clear_loses_a_prepend() {
    check(true);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ModelPtr {
    address: usize,
    generation: usize,
}

fn reused_address_script(old_cas: bool) -> (ModelPtr, ModelPtr) {
    let old_head = ModelPtr {
        address: 0x1000,
        generation: 1,
    };
    let mut head = Some(old_head);
    // Producer A samples old_head, then owner pops and releases its segment.
    let sampled = head.unwrap();
    assert_eq!(head.take(), Some(old_head));
    assert_eq!(head, None);
    // The OS remaps another segment at the same virtual address; producer B
    // publishes its node there with a different allocation provenance.
    let actual_head = ModelPtr {
        address: 0x1000,
        generation: 2,
    };
    head = Some(actual_head);
    let our_node = ModelPtr {
        address: 0x2000,
        generation: 1,
    };
    let next = if old_cas {
        // AtomicPtr CAS compares addresses, so A's stale expectation wins.
        assert_eq!(head.unwrap().address, sampled.address);
        head = Some(our_node);
        sampled
    } else {
        // AtomicPtr::swap returns the pointer actually displaced.
        head.replace(our_node).unwrap()
    };
    assert_eq!(head, Some(our_node));
    (next, actual_head)
}

#[test]
#[should_panic(expected = "stale provenance link")]
fn counterfactual_address_aba_publishes_old_provenance() {
    let (next, actual_head) = reused_address_script(true);
    assert_eq!(next, actual_head, "stale provenance link");
}

#[test]
fn swap_return_keeps_reused_address_provenance() {
    let (next, actual_head) = reused_address_script(false);
    assert_eq!(next, actual_head);
}
