#![cfg(all(feature = "alloc-global", feature = "fastbin", feature = "std"))]

use core::alloc::{GlobalAlloc, Layout};
use std::collections::HashSet;

#[test]
fn facade_first_bind_hits_refills_and_byte_cap_preserve_live_blocks() {
    std::thread::spawn(|| {
        let allocator = sefer_alloc::SeferAlloc::new();
        for size in [16, 64, 512, 16_384] {
            let layout = Layout::from_size_align(size, 8).unwrap();
            for round in 0..4u8 {
                let mut unique = HashSet::new();
                let mut live = Vec::new();
                for index in 0..64u8 {
                    // SAFETY: layout ненулевой; pointer проверяется до записи.
                    let ptr = unsafe { allocator.alloc(layout) };
                    assert!(!ptr.is_null());
                    assert!(
                        unique.insert(ptr as usize),
                        "одновременная повторная выдача"
                    );
                    let value = index ^ round;
                    // SAFETY: уникальный живой блок содержит size доступных байтов.
                    unsafe {
                        ptr.write(value);
                        ptr.add(size - 1).write(value);
                    }
                    live.push((ptr, value));
                }
                for (ptr, value) in live {
                    // SAFETY: блок остаётся живым; освобождается ровно один раз.
                    unsafe {
                        assert_eq!(ptr.read(), value);
                        assert_eq!(ptr.add(size - 1).read(), value);
                        allocator.dealloc(ptr, layout);
                    }
                }
            }
        }
    })
    .join()
    .unwrap();
}

#[test]
fn facade_aligned_small_and_large_continuations_keep_layout_contract() {
    std::thread::spawn(|| {
        let allocator = sefer_alloc::SeferAlloc::new();
        let large = sefer_alloc::SegmentLayout::SMALL_MAX + 16;
        for (size, align) in [(64, 64), (8192, 4096), (large, 16)] {
            let layout = Layout::from_size_align(size, align).unwrap();
            for _ in 0..3 {
                // SAFETY: layout корректный и ненулевой.
                let ptr = unsafe { allocator.alloc(layout) };
                assert!(!ptr.is_null());
                assert_eq!(ptr.addr() % align, 0);
                // SAFETY: живой блок содержит size байтов и освобождается один раз.
                unsafe {
                    ptr.write(0x5a);
                    ptr.add(size - 1).write(0xa5);
                    assert_eq!(ptr.read(), 0x5a);
                    assert_eq!(ptr.add(size - 1).read(), 0xa5);
                    allocator.dealloc(ptr, layout);
                }
            }
        }
    })
    .join()
    .unwrap();
}
