#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, Ordering};
use loom::sync::{Arc, Mutex};
use loom::thread;

struct QueueProtocol {
    state: Mutex<FreeState>,
    remote: Mutex<Vec<u32>>,
    pending: AtomicBool,
    done: AtomicBool,
}

struct FreeState {
    free: Vec<u32>,
    scratch: Vec<u32>,
}

impl QueueProtocol {
    fn new() -> Self {
        Self {
            state: Mutex::new(FreeState {
                free: Vec::new(), // capacity-one slot is occupied
                scratch: Vec::new(),
            }),
            remote: Mutex::new(Vec::new()),
            pending: AtomicBool::new(false),
            done: AtomicBool::new(false),
        }
    }

    fn enqueue_remote(&self) {
        self.remote.lock().unwrap().push(0);
        self.pending.store(true, Ordering::Relaxed);
        self.done.store(true, Ordering::Relaxed);
    }

    fn drain(&self, state: &mut FreeState, force: bool) -> bool {
        if !force && !self.pending.load(Ordering::Relaxed) {
            return false;
        }
        let mut remote = self.remote.lock().unwrap();
        self.pending.store(false, Ordering::Relaxed);
        if remote.is_empty() {
            return true;
        }
        assert!(state.scratch.is_empty());
        core::mem::swap(&mut *remote, &mut state.scratch);
        drop(remote);
        for index in state.scratch.drain(..) {
            state.free.push(index);
        }
        true
    }

    fn insert(&self, force_on_empty: bool) -> bool {
        let mut state = self.state.lock().unwrap();
        let checked_queue = self.drain(&mut state, false);
        if force_on_empty && state.free.is_empty() && !checked_queue {
            self.drain(&mut state, true);
        }
        state.free.pop().is_some()
    }
}

fn check_completed_remote(force_on_empty: bool) {
    loom::model(move || {
        let protocol = Arc::new(QueueProtocol::new());
        let remote = Arc::clone(&protocol);
        let producer = thread::spawn(move || remote.enqueue_remote());

        // No join or acquire handoff before insertion: completion is a
        // distinct Relaxed atomic, just as in the reported counterexample.
        if protocol.done.load(Ordering::Relaxed) {
            assert!(
                protocol.insert(force_on_empty),
                "false-full after completed enqueue"
            );
        }
        producer.join().unwrap();
    });
}

#[test]
fn completed_enqueue_cannot_be_hidden_by_negative_hint() {
    check_completed_remote(true);
}

#[test]
#[should_panic(expected = "false-full after completed enqueue")]
fn old_hint_only_drain_has_a_negative_counterexample() {
    check_completed_remote(false);
}

#[test]
fn overlapping_enqueue_may_follow_insert_but_is_not_lost() {
    loom::model(|| {
        let protocol = Arc::new(QueueProtocol::new());
        let remote = Arc::clone(&protocol);
        let producer = thread::spawn(move || remote.enqueue_remote());
        let first = protocol.insert(true);
        producer.join().unwrap();
        if first {
            assert!(!protocol.insert(true), "slot already reoccupied");
        } else {
            assert!(
                protocol.insert(true),
                "overlapping enqueue must become reusable"
            );
        }
    });
}

#[test]
fn false_hint_and_spurious_true_are_benign_with_empty_queue() {
    loom::model(|| {
        let protocol = QueueProtocol::new();
        assert!(!protocol.insert(true));
        protocol.pending.store(true, Ordering::Relaxed);
        assert!(!protocol.insert(true));
        assert!(!protocol.pending.load(Ordering::Relaxed));
    });
}
