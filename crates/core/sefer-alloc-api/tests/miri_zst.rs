use std::alloc::{Allocator, Layout};

use sefer_alloc_api::SeferAllocator;

#[test]
#[allow(unsafe_code)]
fn zero_sized_layouts_return_aligned_non_null_dangling_pointers() {
    let allocator = SeferAllocator::new();
    for align in [1, 8, 64, 4096] {
        let layout = Layout::from_size_align(0, align).unwrap();
        let block = allocator.allocate(layout).unwrap();
        assert_eq!(block.len(), 0);
        let address = block.cast::<u8>().as_ptr().addr();
        assert_ne!(address, 0);
        assert_eq!(address % align, 0);
        // SAFETY: zero-sized allocations are represented by aligned dangling pointers;
        // deallocation is a no-op for this exact zero-sized layout.
        unsafe { allocator.deallocate(block.cast::<u8>(), layout) };
    }
}
