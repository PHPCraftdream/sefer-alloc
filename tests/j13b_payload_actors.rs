//! Actual-source persistent actor ownership with bounded System-backed buffers.
#[path = "../examples/_shared/j13b_owned_block.rs"]
mod owned;
#[path = "../examples/_shared/j13b_payload_actors.rs"]
mod payload_actors;
#[path = "../examples/_shared/j13b_run_stats.rs"]
mod run_stats;
#[path = "../examples/_shared/j13b_window_kernel.rs"]
#[allow(dead_code)]
mod window_kernel;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use payload_actors::PayloadActors;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::thread::ThreadId;
use workload::{Shape, TouchPlan, MAX_LIVE_BYTES};

#[derive(Default)]
struct Events {
    attempts: usize,
    allocations: Vec<ThreadId>,
    frees: Vec<ThreadId>,
}
#[derive(Default)]
struct Trace {
    fail_at: usize,
    events: Mutex<Events>,
}
// SAFETY: admitted operations delegate unchanged to System; a selected allocation
// returns null before allocation. This fixture is not global, so logging cannot recurse.
unsafe impl GlobalAlloc for Trace {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut events = self.events.lock().unwrap();
        events.attempts += 1;
        if events.attempts == self.fail_at {
            return std::ptr::null_mut();
        }
        // SAFETY: the GlobalAlloc caller supplied a valid nonzero layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            events.allocations.push(std::thread::current().id());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        self.events
            .lock()
            .unwrap()
            .frees
            .push(std::thread::current().id());
        // SAFETY: this is uniquely owned System storage with the matching live layout.
        unsafe { System.dealloc(pointer, layout) };
    }
}
fn plan() -> TouchPlan {
    TouchPlan {
        bytes: 64,
        shape: Shape::Quarter,
    }
}

#[test]
fn persistent_actors_transfer_both_directions_on_actual_threads() {
    let allocator = Trace::default();
    let parent = std::thread::current().id();
    PayloadActors::with(
        &allocator,
        2,
        16,
        |_, _| Ok(()),
        |actors| {
            let (allocated, a) = actors.allocate(0, &[plan()])?;
            assert_eq!(allocated.allocations, 1);
            let (from, b) = actors.transfer(0, 1)?;
            assert_eq!(from, a);
            assert_ne!(a, b);
            assert_ne!(a, parent);
            assert_ne!(b, parent);
            let (touched, actor) = actors.touch(1)?;
            assert_eq!(actor, b);
            assert_eq!(touched.bytes_written, 16);
            let (freed, actor) = actors.free(1)?;
            assert_eq!(actor, b);
            assert_eq!(freed.frees, 1);
            let (_, actor) = actors.allocate(1, &[plan()])?;
            assert_eq!(actor, b, "actor persists across requests");
            assert_eq!(actors.transfer(1, 0)?, (b, a));
            assert_eq!(actors.touch(0)?.1, a);
            assert_eq!(actors.free(0)?.1, a);
            Ok(())
        },
    )
    .expect("two actual persistent actors");
    let events = allocator.events.lock().unwrap();
    assert_eq!(events.allocations.len(), 2);
    assert_eq!(events.frees.len(), 2);
    assert_eq!(events.allocations[0], events.frees[1]);
    assert_eq!(events.allocations[1], events.frees[0]);
}

#[test]
fn return_to_original_actor_retires_on_the_allocation_thread() {
    let allocator = Trace::default();
    PayloadActors::with(
        &allocator,
        2,
        16,
        |_, _| Ok(()),
        |actors| {
            let (_, owner) = actors.allocate(0, &[plan()])?;
            let (_, recipient) = actors.transfer(0, 1)?;
            assert_eq!(actors.touch(1)?.1, recipient);
            assert_eq!(actors.transfer(1, 0)?, (recipient, owner));
            assert_eq!(actors.free(0)?.1, owner);
            Ok(())
        },
    )
    .unwrap();
    let events = allocator.events.lock().unwrap();
    assert_eq!(events.allocations, events.frees);
}

#[test]
fn controller_panic_joins_actors_and_releases_all_held_payloads() {
    let allocator = Trace::default();
    let result = PayloadActors::with(
        &allocator,
        2,
        16,
        |_, _| Ok(()),
        |actors| {
            actors.allocate(0, &[plan()])?;
            actors.allocate(1, &[plan()])?;
            panic!("controlled controller failure");
            #[allow(unreachable_code)]
            Ok(())
        },
    );
    assert!(result.is_err());
    let events = allocator.events.lock().unwrap();
    assert_eq!(events.allocations.len(), 2);
    assert_eq!(events.frees.len(), 2, "cleanup completed before return");
    for owner in &events.allocations {
        assert_eq!(
            events
                .frees
                .iter()
                .filter(|thread| *thread == owner)
                .count(),
            1
        );
    }
}

#[test]
fn preflight_failure_and_panic_release_claimed_payload_before_return() {
    for panic in [false, true] {
        let allocator = Trace::default();
        let result = PayloadActors::with(
            &allocator,
            2,
            16,
            |_, actual| {
                assert_eq!(
                    actual.events.lock().unwrap().allocations.len(),
                    1,
                    "preflight follows real allocation"
                );
                if panic {
                    panic!("controlled actor preflight failure");
                }
                Err(std::io::Error::other("controlled preflight rejection"))
            },
            |actors| {
                assert!(actors.allocate(1, &[plan()]).is_err());
                Ok(())
            },
        );
        assert!(result.is_err());
        let events = allocator.events.lock().unwrap();
        assert_eq!(events.allocations.len(), 1);
        assert_eq!(events.allocations, events.frees);
    }
}

#[test]
fn partial_allocation_failure_retires_every_admitted_actor_block() {
    let allocator = Trace {
        fail_at: 2,
        ..Trace::default()
    };
    let result = PayloadActors::with(
        &allocator,
        2,
        16,
        |_, _| panic!("partial allocation cannot reach preflight"),
        |actors| actors.allocate(0, &[plan(), plan()]).map(|_| ()),
    );
    assert!(result.is_err());
    let events = allocator.events.lock().unwrap();
    assert_eq!(events.attempts, 2);
    assert_eq!(events.allocations.len(), 1);
    assert_eq!(events.allocations, events.frees);
}

#[test]
fn aggregate_live_bounds_and_occupied_transfer_reject_before_allocation() {
    let allocator = Trace::default();
    PayloadActors::with(
        &allocator,
        2,
        16,
        |_, _| Ok(()),
        |actors| {
            actors.allocate(0, &[plan()])?;
            assert!(actors
                .allocate(
                    1,
                    &[TouchPlan {
                        bytes: MAX_LIVE_BYTES,
                        shape: Shape::Sparse
                    }]
                )
                .is_err());
            assert!(actors.allocate(0, &[plan()]).is_err());
            assert!(actors.transfer(0, 0).is_err());
            actors.allocate(1, &[plan()])?;
            assert!(actors.transfer(0, 1).is_err());
            Ok(())
        },
    )
    .unwrap();
    let events = allocator.events.lock().unwrap();
    assert_eq!(events.attempts, 2);
    assert_eq!(events.allocations.len(), events.frees.len());
}
