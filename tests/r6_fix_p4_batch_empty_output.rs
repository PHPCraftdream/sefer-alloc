#![cfg(all(feature = "alloc-global", feature = "batch-api"))]

use std::alloc::Layout;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static ALLOCATOR: SeferAlloc = SeferAlloc::new();

#[test]
fn empty_output_is_a_zero_sized_batch_not_allocation_failure() {
    let layout = Layout::from_size_align(32, 8).unwrap();
    let mut output = [];

    // SAFETY: `layout` is non-zero and valid; the output contains no slots,
    // so no returned allocation can require deallocation.
    let filled = unsafe { ALLOCATOR.alloc_batch(layout, &mut output) };
    assert_eq!(filled, 0);
}
