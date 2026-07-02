//! Phase OPT-F regression: in-place small→small realloc returns the SAME
//! pointer only when the new size stays in the SAME size class
//! (`new_class_idx == old_class_idx`).
//!
//! **The `==`-not-`<=` correctness rule (0.3.0):** OPT-F originally took the
//! in-place fast path for `new_class_idx <= old_class_idx` — i.e. it also
//! aliased a *cross-class shrink* (a shrink that lands in a strictly smaller
//! class) in place. That is unsound: a block is carved at an offset that is a
//! multiple of ITS class's `block_size`, which is not necessarily a multiple
//! of a smaller class's `block_size`; and `dealloc` (post-#114) derives the
//! class from the caller's `Layout` alone, so a later free of the reused
//! pointer with the smaller layout would push the block onto the smaller
//! class's free list at a misaligned offset — corrupting it. The fix
//! restricts in-place to `==`; a cross-class shrink RELOCATES (alloc in the
//! smaller class + copy + dealloc old). See
//! `tests/regression_realloc_cross_class_shrink.rs` for the dedicated gate
//! and its counterfactual.
//!
//! Size-class reference (actual `SIZE_CLASS_TABLE`):
//! - class[3] block_size=64 covers sizes 49..=64
//! - class[2] block_size=48 covers sizes 33..=48
//! - class[1] block_size=32 covers sizes 17..=32
//!
//! So: 64→56 stays in class[3] (in-place, same ptr); 64→32 crosses to
//! class[1] (relocates, ptr may change, prefix preserved); 64→65 grows to
//! class[4]=80 (relocates, prefix preserved).

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

/// Realloc that shrinks ACROSS classes: old class[3] (block_size=64),
/// new size=32 → class[1] (block_size=32). `new_class_idx (1) !=
/// old_class_idx (3)`, so — under the `==`-not-`<=` correctness rule
/// (0.3.0) — this must RELOCATE (alloc a fresh block in the smaller class +
/// copy + dealloc the old block), NOT alias the old block in place.
///
/// Aliasing it in place (the old `<=` behaviour) was unsound: a later free
/// of the reused pointer with the 32-byte layout would push the block onto
/// class[1]'s free list at an offset that is a multiple of 64 (class[3]'s
/// block_size) but NOT of 32 — wait, 64 IS a multiple of 32, so this
/// particular pair would not corrupt; the general defect (see the module
/// doc and `regression_realloc_cross_class_shrink.rs`, e.g. the 6144→4096
/// pair where `6144 % 4096 != 0`) does corrupt. We keep the invariant
/// uniform — cross-class shrink ALWAYS relocates — rather than special-case
/// the pairs that happen to divide, so the rule is simple and the free-list
/// placement is always valid.
///
/// This test therefore asserts the block RELOCATES-or-stays but ALWAYS
/// preserves the `min(old,new)` prefix; it no longer asserts pointer
/// identity (that was the old bug's contract).
#[test]
fn realloc_shrink_cross_class_down_relocates_and_preserves_data() {
    let mut ac = AllocCore::new().expect("primordial");
    let old_layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = ac.alloc(old_layout);
    assert!(!ptr.is_null());

    // Write a full-block pattern so a relocation's copy is verifiable across
    // the whole preserved prefix (min(64, 32) = 32 bytes).
    unsafe {
        for i in 0..64usize {
            ptr.add(i).write((i as u8).wrapping_add(0x11));
        }
    }

    // 64 (class[3]) → 32 (class[1]): cross-class shrink → relocates under the
    // `==` rule. We do NOT assert ptr identity (the old `<=` bug did).
    let new_ptr = ac.realloc(ptr, old_layout, 32);
    assert!(!new_ptr.is_null(), "realloc 64->32 must not return null");

    // The min(old,new)=32-byte prefix must be preserved regardless of whether
    // the block moved.
    unsafe {
        for i in 0..32usize {
            assert_eq!(
                new_ptr.add(i).read(),
                (i as u8).wrapping_add(0x11),
                "prefix byte {i} lost during cross-class shrink 64->32"
            );
        }
    }

    // Free with the NEW (32-byte) layout — the GlobalAlloc contract. This
    // must route to class[1]'s free list soundly (the whole point of the
    // relocation: the block now genuinely lives in class[1]).
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
