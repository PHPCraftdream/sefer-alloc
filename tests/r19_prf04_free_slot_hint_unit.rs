//! Real core-atomic FREE-slot hints, including nonempty concurrent consumption.
#![cfg(all(feature = "alloc-global", feature = "internals"))]
#![forbid(unsafe_code)]

use sefer_alloc::registry::bootstrap::{FreeSlotHint, MAX_HEAPS};
use std::sync::{Arc, Barrier};

#[test]
fn empty() {
    assert_eq!(FreeSlotHint::new().take(), None);
}

#[test]
fn lowest_index() {
    let set = FreeSlotHint::new();
    for index in [4095, 64, 63, 0, 2048] {
        set.publish(index);
    }
    for index in [0, 63, 64, 2048, 4095] {
        assert_eq!(set.take(), Some(index));
    }
    assert_eq!(set.take(), None);
}

#[test]
fn publication_is_idempotent() {
    let set = FreeSlotHint::new();
    for _ in 0..64 {
        set.publish(99);
    }
    assert_eq!(set.take(), Some(99));
    assert_eq!(set.take(), None);
}

#[test]
fn forget_preserves_neighbours() {
    let set = FreeSlotHint::new();
    for index in [62, 63, 64, 65] {
        set.publish(index);
    }
    set.forget(63);
    set.forget(63);
    set.forget(64);
    for index in [62, 65] {
        assert_eq!(set.take(), Some(index));
    }
    assert_eq!(set.take(), None);
    set.publish(63);
    assert_eq!(set.take(), Some(63));
    assert_eq!(set.take(), None);
}

#[test]
fn all_4096_are_unique() {
    let set = FreeSlotHint::new();
    for index in (0..MAX_HEAPS).rev() {
        set.publish(index);
    }
    for index in 0..MAX_HEAPS {
        assert_eq!(set.take(), Some(index));
    }
    assert_eq!(set.take(), None);
}

#[test]
fn concurrent_take_returns_only_won_bits() {
    const WORKERS: usize = 16;
    const ROUNDS: usize = 128;
    let set = Arc::new(FreeSlotHint::new());
    let start = Arc::new(Barrier::new(WORKERS + 1));
    let done = Arc::new(Barrier::new(WORKERS + 1));
    let counts = Arc::new(
        (0..64)
            .map(|_| std::sync::atomic::AtomicUsize::new(0))
            .collect::<Vec<_>>(),
    );
    let mut workers = Vec::with_capacity(WORKERS);
    for _ in 0..WORKERS {
        let set = Arc::clone(&set);
        let start = Arc::clone(&start);
        let done = Arc::clone(&done);
        let counts = Arc::clone(&counts);
        workers.push(std::thread::spawn(move || {
            for _ in 0..ROUNDS {
                start.wait();
                while let Some(index) = set.take() {
                    counts[index].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                done.wait();
            }
        }));
    }
    let mut mismatches = Vec::new();
    for round in 1..=ROUNDS {
        for index in 0..64 {
            set.publish(index);
        }
        start.wait();
        done.wait();
        if let Some(remaining) = set.take() {
            mismatches.push((round, None, remaining));
        }
        for (index, count) in counts.iter().enumerate() {
            let actual = count.load(std::sync::atomic::Ordering::Relaxed);
            if actual != round {
                mismatches.push((round, Some(index), actual));
            }
        }
    }
    // Finish every barrier round and join every worker before any assertion.
    let joins: Vec<_> = workers.into_iter().map(|worker| worker.join()).collect();
    assert!(joins.iter().all(Result::is_ok), "worker panicked");
    assert!(mismatches.is_empty(), "round mismatches: {mismatches:?}");
    assert_eq!(set.take(), None);
}
