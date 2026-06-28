//! Phase OPT-F regression: in-place small→small realloc returns the same
//! pointer when the new size fits in the old class (new_class_idx <= old_class_idx).
//!
//! **Adapted from the task spec** — the original spec used 64→96→128 B examples
//! that happen to be *different* size classes in the actual `SIZE_CLASS_TABLE`
//! (class[3]=64, class[5]=112). Tests have been corrected to use sizes that
//! actually map to the same or smaller class:
//!
//! - class[3] block_size=64 covers sizes 49..=64
//! - class[2] block_size=48 covers sizes 33..=48
//! - class[1] block_size=32 covers sizes 17..=32
//!
//! The cross-class test grows 64→65 (class[3]→class[4]=80), which takes the
//! alloc+copy+dealloc path.

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

/// Realloc to the exact same size in the same class (no-op case).
#[test]
fn realloc_same_size_returns_same_ptr() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = ac.alloc(layout);
    assert!(!ptr.is_null());

    // Write a marker byte.
    unsafe { ptr.write(0xAA) };

    // 64 → 64: trivially same class[3] (block_size=64).
    let new_ptr = ac.realloc(ptr, layout, 64);
    assert!(!new_ptr.is_null());
    assert_eq!(
        ptr, new_ptr,
        "realloc 64->64 must reuse the same block (trivially same class)"
    );
    assert_eq!(unsafe { new_ptr.read() }, 0xAA, "marker must survive");

    ac.dealloc(new_ptr, Layout::from_size_align(64, 8).unwrap());
}

/// Realloc to a smaller size that stays in the same class (class[3]: 49..=64).
#[test]
fn realloc_shrink_within_same_class_returns_same_ptr() {
    let mut ac = AllocCore::new().expect("primordial");
    let old_layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    unsafe { ptr.write(0xBB) };

    // 64 → 56: still class[3] (block_size=64 covers 49..=64).
    let new_ptr = ac.realloc(ptr, old_layout, 56);
    assert!(!new_ptr.is_null());
    assert_eq!(
        ptr, new_ptr,
        "realloc 64->56 must reuse the same block (still class[3])"
    );
    assert_eq!(unsafe { new_ptr.read() }, 0xBB, "marker must survive");

    ac.dealloc(new_ptr, Layout::from_size_align(56, 8).unwrap());
}

/// Realloc that shrinks across classes: old class[3] (block_size=64),
/// new size=32 → class[1] (block_size=32). new_class_idx < old_class_idx,
/// so the block physically fits and must be reused in-place.
#[test]
fn realloc_shrink_cross_class_down_returns_same_ptr() {
    let mut ac = AllocCore::new().expect("primordial");
    let old_layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    unsafe { ptr.write(0xCC) };

    // 64 (class[3]) → 32 (class[1]): new_class_idx=1 < old_class_idx=3,
    // so the new size fits inside the existing 64-byte block.
    let new_ptr = ac.realloc(ptr, old_layout, 32);
    assert!(!new_ptr.is_null());
    assert_eq!(
        ptr, new_ptr,
        "realloc 64->32 must reuse the same block (shrink: class[1] <= class[3])"
    );
    assert_eq!(unsafe { new_ptr.read() }, 0xCC, "marker must survive");

    ac.dealloc(new_ptr, Layout::from_size_align(32, 8).unwrap());
}

/// Realloc that grows into a LARGER class: 64 (class[3]) → 65 (class[4],
/// block_size=80). new_class_idx > old_class_idx — must allocate a new block
/// and copy. The test verifies data is preserved but does NOT assert ptr equality
/// (the ptr may change; what matters is the content is preserved and the old
/// pointer is freed).
#[test]
fn realloc_cross_class_up_moves_and_preserves_data() {
    let mut ac = AllocCore::new().expect("primordial");
    let old_layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    // Write a pattern across all 64 bytes.
    unsafe {
        for i in 0..64usize {
            ptr.add(i).write((i as u8).wrapping_add(0x20));
        }
    }

    // 64 (class[3]) → 65 (class[4]=80): definitely a different, larger class.
    let new_ptr = ac.realloc(ptr, old_layout, 65);
    assert!(!new_ptr.is_null(), "realloc 64->65 must not return null");

    // The first 64 bytes of data must be preserved (copy happened).
    unsafe {
        for i in 0..64usize {
            assert_eq!(
                new_ptr.add(i).read(),
                (i as u8).wrapping_add(0x20),
                "prefix byte {} lost during cross-class realloc 64->65",
                i
            );
        }
    }

    ac.dealloc(new_ptr, Layout::from_size_align(65, 8).unwrap());
}

/// Sanity: realloc of a large allocation (size > SMALL_MAX) takes the slow
/// path. Data must be preserved.
#[test]
fn realloc_large_to_large_preserves_data() {
    let mut ac = AllocCore::new().expect("primordial");
    // Use a size larger than SMALL_MAX so both paths go through the large path.
    let old_size = 128 * 1024; // 128 KiB — definitely large.
    let old_layout = Layout::from_size_align(old_size, 16).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    unsafe {
        for i in 0..64usize {
            ptr.add(i).write((i as u8).wrapping_add(0x40));
        }
    }

    let new_size = 256 * 1024;
    let new_ptr = ac.realloc(ptr, old_layout, new_size);
    assert!(!new_ptr.is_null());

    unsafe {
        for i in 0..64usize {
            assert_eq!(
                new_ptr.add(i).read(),
                (i as u8).wrapping_add(0x40),
                "prefix byte {} lost during large realloc",
                i
            );
        }
    }

    ac.dealloc(new_ptr, Layout::from_size_align(new_size, 16).unwrap());
}
