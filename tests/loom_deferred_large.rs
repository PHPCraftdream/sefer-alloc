//! Loom model of the deferred Large claim, swap, ready-link and owner pop.
//! Run with `RUSTFLAGS="--cfg loom" cargo test --release --features
//! "alloc-core alloc-xthread" --test loom_deferred_large`.

#![cfg(loom)]

use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

const FREE: usize = usize::MAX;
const PUBLISHING: usize = usize::MAX - 1;
const TAIL: usize = 0;

struct Stack {
    head: AtomicUsize,
    next: [AtomicUsize; 2],
}

impl Stack {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            head: AtomicUsize::new(TAIL),
            next: std::array::from_fn(|_| AtomicUsize::new(FREE)),
        })
    }

    fn begin_push(&self, id: usize) -> Option<usize> {
        if self.next[id - 1]
            .compare_exchange(FREE, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return None;
        }
        Some(self.head.swap(id, Ordering::AcqRel))
    }

    fn finish_push(&self, id: usize, predecessor: usize) {
        self.next[id - 1].store(predecessor, Ordering::Release);
    }

    fn push(&self, id: usize) {
        if let Some(predecessor) = self.begin_push(id) {
            self.finish_push(id, predecessor);
        }
    }

    // The real owner waits at PUBLISHING; returning None lets the bounded
    // model examine the unpublished state without spinning on one schedule.
    fn try_pop(&self) -> Option<usize> {
        let cur = self.head.load(Ordering::Acquire);
        if cur == TAIL {
            return None;
        }
        let next = self.next[cur - 1].load(Ordering::Acquire);
        if next == PUBLISHING {
            return None;
        }
        self.head
            .compare_exchange(cur, next, Ordering::AcqRel, Ordering::Acquire)
            .ok()
    }
}

#[test]
fn distinct_producers_and_owner_pop_are_lossless() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let stack = Stack::new();
        let a = Arc::clone(&stack);
        let producer_a = thread::spawn(move || a.push(1));
        let b = Arc::clone(&stack);
        let producer_b = thread::spawn(move || b.push(2));
        let c = Arc::clone(&stack);
        let owner = thread::spawn(move || c.try_pop());

        producer_a.join().unwrap();
        producer_b.join().unwrap();
        let first = owner.join().unwrap();
        let mut seen = [0; 2];
        if let Some(id) = first {
            seen[id - 1] += 1;
        }
        while let Some(id) = stack.try_pop() {
            seen[id - 1] += 1;
        }
        assert_eq!(seen, [1, 1]);
        assert_eq!(stack.head.load(Ordering::Relaxed), TAIL);
    });
}

#[test]
fn duplicate_claim_is_a_noop() {
    loom::model(|| {
        let stack = Stack::new();
        let a = Arc::clone(&stack);
        let first = thread::spawn(move || a.push(1));
        let b = Arc::clone(&stack);
        let duplicate = thread::spawn(move || b.push(1));
        first.join().unwrap();
        duplicate.join().unwrap();
        assert_eq!(stack.try_pop(), Some(1));
        assert_eq!(stack.try_pop(), None);
    });
}

#[test]
fn owner_cannot_pop_a_swapped_but_unpublished_node() {
    loom::model(|| {
        let stack = Stack::new();
        stack.push(1);
        let predecessor = stack.begin_push(2).unwrap();
        assert_eq!(predecessor, 1);
        assert_eq!(stack.try_pop(), None);
        stack.finish_push(2, predecessor);
        assert_eq!(stack.try_pop(), Some(2));
        assert_eq!(stack.try_pop(), Some(1));
    });
}

#[test]
#[should_panic(expected = "unpublished link treated as a node")]
fn counterfactual_no_ready_check_reads_publishing_marker() {
    loom::model(|| {
        let stack = Stack::new();
        stack.push(1);
        stack.begin_push(2).unwrap();
        let head = stack.head.load(Ordering::Acquire);
        let link = stack.next[head - 1].load(Ordering::Acquire);
        assert!(link <= 2, "unpublished link treated as a node");
    });
}

#[test]
#[should_panic(expected = "self-link after duplicate publication")]
fn counterfactual_no_claim_self_links() {
    loom::model(|| {
        let stack = Stack::new();
        stack.push(1);
        let previous = stack.head.swap(1, Ordering::AcqRel);
        stack.next[0].store(previous, Ordering::Release);
        assert_eq!(stack.try_pop(), Some(1));
        assert_eq!(
            stack.head.load(Ordering::Acquire),
            TAIL,
            "self-link after duplicate publication"
        );
    });
}
