//! Bounded model of the intrusive third-tier MPSC stack's CAS protocol.
//! Node indices stand for distinct live blocks; a relaxed atomic models the
//! private next word so Loom can inspect its Release/Acquire publication.

#![cfg(loom)]

use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

struct Spill {
    head: AtomicUsize,
    next: [AtomicUsize; 2],
    seen: [AtomicUsize; 2],
}

impl Spill {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            head: AtomicUsize::new(0),
            next: std::array::from_fn(|_| AtomicUsize::new(0)),
            seen: std::array::from_fn(|_| AtomicUsize::new(0)),
        })
    }

    fn push(&self, id: usize) {
        let mut head = self.head.load(Ordering::Acquire);
        loop {
            self.next[id - 1].store(head, Ordering::Relaxed);
            match self
                .head
                .compare_exchange_weak(head, id, Ordering::Release, Ordering::Acquire)
            {
                Ok(_) => return,
                Err(observed) => head = observed,
            }
        }
    }

    fn pop(&self, broken_clear: bool) -> Option<usize> {
        let mut head = self.head.load(Ordering::Acquire);
        loop {
            if head == 0 {
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
        assert_eq!(spill.seen[0].load(Ordering::Relaxed), 1);
        assert_eq!(spill.seen[1].load(Ordering::Relaxed), 1);
        assert_eq!(spill.head.load(Ordering::Relaxed), 0);
    });
}

#[test]
fn concurrent_prepend_and_pop_are_lossless() {
    check(false);
}

#[test]
#[should_panic(expected = "assertion")]
fn counterfactual_unconditional_clear_loses_a_prepend() {
    check(true);
}
