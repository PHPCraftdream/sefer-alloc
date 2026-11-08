//! Actual-source ownership, realloc-failure and thread-transfer checks; tiny under Miri.
#[path = "../examples/_shared/j13b_owned_block.rs"]
mod owned;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;

use owned::OwnedBlock;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, ThreadId};
use workload::{touch_slots_with_page, Shape, TOUCH_BYTE};

#[derive(Debug, Eq, PartialEq)]
enum Operation {
    Allocate(usize, usize),
    Reallocate(usize, usize, usize),
    Free(usize, usize),
}

#[derive(Debug)]
struct Call {
    operation: Operation,
    thread: ThreadId,
}

#[derive(Default)]
struct State {
    calls: Vec<Call>,
    fail_allocate: bool,
    fail_reallocate: bool,
}

#[derive(Default)]
struct RecordingAllocator(Mutex<State>);

impl RecordingAllocator {
    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

// SAFETY: all actual allocation ownership and layout operations delegate to
// System. Failed allocations/reallocations return null without consuming live
// storage. The bounded fixture log is mutex-protected and does not read payload.
// This allocator is not installed globally, so recording cannot recurse into it.
unsafe impl GlobalAlloc for RecordingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut state = self.state();
        state.calls.push(Call {
            operation: Operation::Allocate(layout.size(), layout.align()),
            thread: thread::current().id(),
        });
        let fail = state.fail_allocate;
        drop(state);
        if fail {
            return std::ptr::null_mut();
        }
        // SAFETY: GlobalAlloc's caller supplied a valid nonzero layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, bytes: usize) -> *mut u8 {
        let mut state = self.state();
        state.calls.push(Call {
            operation: Operation::Reallocate(layout.size(), bytes, layout.align()),
            thread: thread::current().id(),
        });
        let fail = state.fail_reallocate;
        drop(state);
        if fail {
            return std::ptr::null_mut();
        }
        // SAFETY: the caller retains a live System allocation with its exact
        // layout; new size is nonzero and valid for the unchanged alignment.
        unsafe { System.realloc(pointer, layout, bytes) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        self.state().calls.push(Call {
            operation: Operation::Free(layout.size(), layout.align()),
            thread: thread::current().id(),
        });
        // SAFETY: every admitted pointer came from System and is freed with
        // its exact current layout, once, under GlobalAlloc's caller contract.
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[test]
fn layout_bounds_and_null_allocation_never_create_an_owner_or_free() {
    let allocator = RecordingAllocator::default();
    for bytes in [
        0,
        OwnedBlock::<RecordingAllocator>::MAX_BYTES + 1,
        usize::MAX,
    ] {
        assert!(OwnedBlock::allocate(&allocator, bytes).is_err());
    }
    assert!(allocator.state().calls.is_empty());
    let maximum = OwnedBlock::<RecordingAllocator>::MAX_BYTES;
    assert_eq!(
        OwnedBlock::<RecordingAllocator>::checked_layout(maximum).is_ok(),
        usize::BITS == 64,
        "2 GiB fits Layout only on the supported 64-bit targets"
    );
    allocator.state().fail_allocate = true;
    assert_eq!(
        OwnedBlock::allocate(&allocator, 32).err(),
        Some("allocation failed")
    );
    let state = allocator.state();
    assert_eq!(state.calls.len(), 1);
    assert_eq!(state.calls[0].operation, Operation::Allocate(32, 8));
}

#[test]
fn actual_owned_storage_uses_runtime_edge_geometry_without_widening_writes() {
    let allocator = RecordingAllocator::default();
    let mut block = OwnedBlock::allocate(&allocator, 64).expect("small fixture allocation");
    for slot in block.slots() {
        slot.write(0xa5);
    }
    assert_eq!(
        touch_slots_with_page(block.slots(), Shape::EdgeTwoPages, 16),
        2
    );
    for (offset, slot) in block.slots().iter().enumerate() {
        let expected = if offset == 0 || offset == 48 {
            TOUCH_BYTE
        } else {
            0xa5
        };
        // SAFETY: every byte was initialized above; the kernel only writes
        // initialized u8 values and no other thread can access this block.
        assert_eq!(unsafe { slot.assume_init() }, expected, "offset {offset}");
    }
    drop(block);
    let state = allocator.state();
    assert_eq!(state.calls.len(), 2);
    assert_eq!(state.calls[1].operation, Operation::Free(64, 8));
}

#[test]
fn growth_shrink_and_failed_realloc_preserve_exact_ownership_and_prefix() {
    let allocator = RecordingAllocator::default();
    let mut block = OwnedBlock::allocate(&allocator, 32).expect("small fixture allocation");
    for slot in block.slots() {
        slot.write(0x31);
    }
    block.reallocate(96).expect("real growing realloc");
    assert_eq!(block.requested_bytes(), 96);
    for slot in &block.slots()[..32] {
        // SAFETY: these 32 initialized bytes are preserved by successful realloc.
        assert_eq!(unsafe { slot.assume_init() }, 0x31);
    }
    for slot in block.slots() {
        slot.write(0x5a);
    }
    block.reallocate(16).expect("real shrinking realloc");
    allocator.state().fail_reallocate = true;
    assert_eq!(block.reallocate(128), Err("reallocation failed"));
    assert_eq!(block.requested_bytes(), 16);
    for slot in block.slots().iter() {
        // SAFETY: the initialized prefix survived shrink and the failed realloc.
        assert_eq!(unsafe { slot.assume_init() }, 0x5a);
    }
    assert!(block.reallocate(0).is_err());
    assert!(block.reallocate(usize::MAX).is_err());
    drop(block);
    let state = allocator.state();
    let operations: Vec<_> = state.calls.iter().map(|call| &call.operation).collect();
    assert_eq!(
        operations,
        vec![
            &Operation::Allocate(32, 8),
            &Operation::Reallocate(32, 96, 8),
            &Operation::Reallocate(96, 16, 8),
            &Operation::Reallocate(16, 128, 8),
            &Operation::Free(16, 8),
        ]
    );
}

#[test]
fn ownership_transfers_both_directions_and_is_freed_on_the_actual_destination() {
    let allocator = RecordingAllocator::default();
    let parent = thread::current().id();
    let mut first = OwnedBlock::allocate(&allocator, 32).expect("small allocation on parent");
    for slot in first.slots() {
        slot.write(0xa5);
    }
    let mut first = thread::scope(|scope| {
        scope
            .spawn(move || {
                let mut first = first;
                assert_eq!(touch_slots_with_page(first.slots(), Shape::Dense, 16), 32);
                first
            })
            .join()
            .expect("touch worker joined")
    });
    for slot in first.slots().iter() {
        // SAFETY: all bytes were initialized before transfer; the joined worker
        // only wrote initialized u8 values and retains no payload alias.
        assert_eq!(unsafe { slot.assume_init() }, TOUCH_BYTE);
    }
    drop(first);
    let reference = &allocator;
    thread::scope(|scope| {
        let block = scope
            .spawn(move || OwnedBlock::allocate(reference, 32).expect("worker allocation"))
            .join()
            .expect("allocation worker joined");
        drop(block);
    });
    let third = OwnedBlock::allocate(&allocator, 32).expect("parent allocation for remote drop");
    thread::scope(|scope| {
        scope
            .spawn(move || drop(third))
            .join()
            .expect("remote free worker joined");
    });
    let state = allocator.state();
    assert_eq!(state.calls.len(), 6);
    assert_eq!(state.calls[0].thread, parent);
    assert_eq!(state.calls[1].thread, parent);
    assert_ne!(state.calls[2].thread, parent);
    assert_eq!(state.calls[3].thread, parent);
    assert_eq!(state.calls[3].operation, Operation::Free(32, 8));
    assert_eq!(state.calls[4].thread, parent);
    assert_ne!(state.calls[5].thread, parent);
    assert_eq!(state.calls[5].operation, Operation::Free(32, 8));
}

#[test]
fn a_panicking_worker_frees_its_owned_allocation_and_the_join_is_observed() {
    let allocator = RecordingAllocator::default();
    let parent = thread::current().id();
    thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let _block = OwnedBlock::allocate(&allocator, 32).expect("worker allocation");
            panic!("bounded ownership fixture panic");
        });
        let error = worker.join().expect_err("intentional worker panic");
        assert_eq!(
            error.downcast_ref::<&str>(),
            Some(&"bounded ownership fixture panic")
        );
    });
    let state = allocator.state();
    assert_eq!(state.calls.len(), 2);
    assert_ne!(state.calls[0].thread, parent);
    assert_eq!(state.calls[0].thread, state.calls[1].thread);
    assert_eq!(state.calls[1].operation, Operation::Free(32, 8));
}

#[test]
fn invalid_or_overflowing_page_geometry_never_touches_the_slots() {
    for page in [0, 3, 1usize << (usize::BITS - 1)] {
        let mut slots = [0xa5; 64];
        assert_eq!(
            touch_slots_with_page(&mut slots, Shape::EdgeTwoPages, page),
            0
        );
        assert_eq!(slots, [0xa5; 64]);
    }
}
