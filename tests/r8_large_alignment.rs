#![cfg(all(feature = "alloc-core", feature = "internals"))]
use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

#[test]
fn oversegment_alignment_preserves_geometry_and_realloc_prefix() {
    for multiple in [1, 2, 16] {
        let align = multiple * SegmentLayout::SEGMENT;
        let mut core = AllocCore::new().unwrap();
        let layout = Layout::from_size_align(37, align).unwrap();
        let ptr = core.alloc_zeroed(layout);
        assert!(!ptr.is_null(), "alignment {align}");
        assert_eq!(ptr.addr() % align, 0);
        let (token, token_len, root, offset, capacity) = core.large_geometry_for_test(ptr).unwrap();
        assert_eq!(root + offset, ptr.addr());
        assert_ne!(root, ptr.addr());
        assert_ne!(token, root);
        assert!(token_len >= capacity);
        // SAFETY: successful allocation owns at least layout.size() writable bytes.
        unsafe {
            for i in 0..37 {
                assert_eq!(ptr.add(i).read(), 0);
                ptr.add(i).write(i as u8 ^ 0xa5);
            }
        }
        // SAFETY: uniquely owned current allocation and exact original Layout.
        let grown = unsafe { core.realloc(ptr, layout, 8193) };
        assert!(!grown.is_null());
        assert_eq!(grown.addr() % align, 0);
        // SAFETY: realloc preserved the original 37-byte initialized prefix.
        unsafe {
            for i in 0..37 {
                assert_eq!(grown.add(i).read(), i as u8 ^ 0xa5);
            }
        }
        let grown_layout = Layout::from_size_align(8193, align).unwrap();
        // An invalid new Layout is a deterministic failure, not a giant allocation.
        // SAFETY: source remains live; failure must not consume it.
        assert!(unsafe { core.realloc(grown, grown_layout, usize::MAX) }.is_null());
        // SAFETY: the failed realloc left this source intact.
        unsafe {
            for i in 0..37 {
                assert_eq!(grown.add(i).read(), i as u8 ^ 0xa5);
            }
            core.dealloc(grown, grown_layout);
        }
        assert!(!core.dbg_contains_base(grown));
    }
}

#[test]
fn narrow_payload_pointer_uses_owner_canonical_root() {
    let mut core = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(1, 2 * SegmentLayout::SEGMENT).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    // SAFETY: one byte is valid and uniquely owned. The derived pointer may
    // have only the reborrow's permission, not permission to read metadata.
    let narrow = unsafe {
        ptr.write(0);
        &mut *ptr
    } as *mut u8;
    let key_only = core::ptr::without_provenance_mut(narrow.addr());
    assert!(core.large_geometry_for_test(key_only).is_some());
    // SAFETY: exact issued start and Layout, transferred uniquely once.
    unsafe { core.dealloc(narrow, layout) };
    assert!(!core.dbg_contains_base(key_only));
}

#[cfg(all(
    feature = "lazy-commit-fault-injection",
    not(feature = "numa-aware"),
    not(miri)
))]
#[test]
fn biased_commit_failure_preserves_source() {
    // Fault injection is process-wide; isolate this real backend scenario so
    // unrelated suite threads cannot steal or observe the injected commit.
    if std::env::var_os("SEFER_R8_COMMIT_CHILD").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "biased_commit_failure_preserves_source"])
            .env("SEFER_R8_COMMIT_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated biased commit rollback scenario");
        return;
    }
    let mut core = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(37, SegmentLayout::SEGMENT).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    let capacity = core.large_geometry_for_test(ptr).unwrap().4;
    // SAFETY: live 37-byte source is uniquely owned.
    unsafe { ptr.write_bytes(0x4b, 37) };
    let reserve_failures_before = AllocCore::dbg_segments_reserve_failed_total();
    core.dbg_arm_commit_fail(1);
    // SAFETY: exact original Layout; crossing capacity forces a moving realloc
    // whose destination biased-window commit is deterministically refused.
    let result = unsafe { core.realloc(ptr, layout, capacity) };
    core.dbg_arm_commit_fail(0);
    assert!(result.is_null());
    assert_eq!(
        AllocCore::dbg_segments_reserve_failed_total(),
        reserve_failures_before,
        "useful-window commit refusal is not an OS reservation refusal"
    );
    assert!(core.large_geometry_for_test(ptr).is_some());
    // SAFETY: the failed commit did not consume or overwrite the source.
    unsafe {
        for i in 0..37 {
            assert_eq!(ptr.add(i).read(), 0x4b);
        }
        core.dealloc(ptr, layout);
    }
}
