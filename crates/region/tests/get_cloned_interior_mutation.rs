//! Regression test for Clone panic interior side effects.
//!
//! Documents that a panicking `T::clone` inside `SyncRegion::get_cloned`
//! DOES NOT prevent interior side effects in `T`. The container structure
//! stays intact, but `T`'s internal state (Cell, atomics, etc.) may already
//! have changed before the panic.
//!
//! `SyncRegion` is `std`-only -- this whole file is a no-op without it.

#![cfg(feature = "std")]

use sefer_region::SyncRegion;
use std::cell::Cell;
use std::rc::Rc;

/// A value with interior mutation that panics after mutating.
///
/// `Rc<Cell<usize>>`, not `Arc<Cell<usize>>`: this test is single-threaded
/// (no `thread::spawn` anywhere), so there is no genuine need for a
/// thread-safe handle here, and `Cell` is not `Sync` -- wrapping it in
/// `Arc` would be misleading (clippy's `arc_with_non_send_sync` correctly
/// flags exactly this: an `Arc` around a non-`Sync` type does not actually
/// grant the cross-thread sharing an `Arc` implies).
#[derive(Debug)]
struct PanicAfterIncrement {
    counter: Rc<Cell<usize>>,
    panic_at: usize,
}

impl PanicAfterIncrement {
    fn new(counter: Rc<Cell<usize>>, panic_at: usize) -> Self {
        Self { counter, panic_at }
    }
}

impl Clone for PanicAfterIncrement {
    fn clone(&self) -> Self {
        // Increment BEFORE panic to prove interior side effect happens.
        let new_val = self.counter.get() + 1;
        self.counter.set(new_val);
        if new_val == self.panic_at {
            panic!("intentional panic in Clone at counter={}", new_val);
        }
        Self {
            counter: Rc::clone(&self.counter),
            panic_at: self.panic_at,
        }
    }
}

#[test]
fn get_cloned_preserves_container_but_allows_interior_side_effects() {
    let counter = Rc::new(Cell::new(0));
    let sr: SyncRegion<PanicAfterIncrement> = SyncRegion::new();

    let handle = sr
        .write()
        .insert(PanicAfterIncrement::new(Rc::clone(&counter), 3));

    // First two clones succeed without panic.
    let _ = sr.get_cloned(handle).unwrap();
    assert_eq!(counter.get(), 1, "first clone should have incremented");

    let _ = sr.get_cloned(handle).unwrap();
    assert_eq!(counter.get(), 2, "second clone should have incremented");

    // Third clone panics, but the increment already happened.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = sr.get_cloned(handle);
    }));
    assert!(result.is_err(), "third clone should have panicked");
    assert_eq!(counter.get(), 3, "interior mutation happened before panic");

    // Container structure remains intact: handle still resolves.
    assert!(
        sr.read().get(handle).is_some(),
        "container structure intact after panic"
    );

    // Fourth clone works again, incrementing from 3 to 4.
    let _ = sr.get_cloned(handle).unwrap();
    assert_eq!(counter.get(), 4, "fourth clone should have incremented");

    // The interior mutation persisted: we're now at 4, not reset.
    // This proves that interior side effects are T's responsibility, not SyncRegion's.
}
