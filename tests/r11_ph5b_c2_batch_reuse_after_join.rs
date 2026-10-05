//! Batch cross-thread free and owner reuse after worker join.
#![cfg(all(feature = "alloc-global", feature = "batch-api"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::thread;

use sefer_alloc::SeferAlloc;

static ALLOC: SeferAlloc = SeferAlloc::new();
const COUNT: usize = 8;
static BLOCKS: [AtomicPtr<u8>; COUNT] = [const { AtomicPtr::new(core::ptr::null_mut()) }; COUNT];
static ORIGINALS: [AtomicPtr<u8>; COUNT] = [const { AtomicPtr::new(core::ptr::null_mut()) }; COUNT];

#[repr(C)]
struct Small([u8; 512]);

const LAYOUT: Layout = Layout::new::<Small>();

#[test]
fn owner_batch_reuses_after_worker_batch_free_and_join() {
    // The main thread owns these allocations; the worker only performs the
    // cross-thread batch free. AtomicPtr transfers retain pointer provenance.
    unsafe {
        let mut blocks = [core::ptr::null_mut(); COUNT];
        assert_eq!(ALLOC.alloc_batch(LAYOUT, &mut blocks), COUNT);
        for (index, (slot, block)) in BLOCKS.iter().zip(blocks).enumerate() {
            core::ptr::write_bytes(block, 0x5a, LAYOUT.size());
            slot.store(block, Ordering::Relaxed);
            ORIGINALS[index].store(block, Ordering::Relaxed);
        }
    }

    thread::spawn(|| {
        let blocks: [*mut u8; COUNT] = core::array::from_fn(|index| {
            BLOCKS[index].swap(core::ptr::null_mut(), Ordering::Relaxed)
        });
        unsafe { ALLOC.dealloc_batch(LAYOUT, &blocks) };
    })
    .join()
    .expect("worker panicked");

    unsafe {
        let mut reused = [core::ptr::null_mut(); COUNT];
        assert_eq!(ALLOC.alloc_batch(LAYOUT, &mut reused), COUNT);
        let originals: [*mut u8; COUNT] =
            core::array::from_fn(|index| ORIGINALS[index].load(Ordering::Relaxed));
        for block in reused {
            assert!(
                originals.contains(&block),
                "owner did not reuse a freed block"
            );
            core::ptr::write_bytes(block, 0xa5, LAYOUT.size());
            assert_eq!(block.read(), 0xa5);
        }
        ALLOC.dealloc_batch(LAYOUT, &reused);
    }
}
