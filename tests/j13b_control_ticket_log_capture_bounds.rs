//! J13b control telemetry capture bounds: the diagnostic ticket log must
//! REJECT unsupported capture instead of presenting it as usable evidence.
//! Every assertion below evaluates the production-used packing and validity
//! gate through the pure diagnostic hooks
//! (`dbg_thp_control_ticket_log_pack` / `dbg_thp_control_ticket_log_capture_supported`),
//! so the tests cannot pass by duplicating the formulas: a drifted or
//! unconditionally-accepting production implementation turns these asserts
//! red.
//!
//! Discriminators:
//! - unconditional-acceptance mutant: the gate/hook must return false/None
//!   for over-cap and invalid-marked windows; an always-accept mutant is red;
//! - over-cap capture: 129 real records through the installed GlobalAlloc
//!   leave the raw cursor above the cap and the hook returns None;
//! - invalid marker: a consumed window containing one marker-0 entry is
//!   rejected;
//! - packing threshold: ordinals at or above 2^62 pack to the marker 0;
//!   the boundary ordinal 2^62 - 1 and low ordinals round-trip losslessly.
//!
//! Memory bound: the over-cap leg holds 129 x 8 MiB = ~1.0 GiB live, fine
//! for CI runners.

#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const MIB: usize = 1024 * 1024;
const CAP: u64 = 128;
const SUPPORTED_MAX: u64 = 1u64 << 62;
const RECORDS: usize = 129; // one past the cap: the 129th record is dropped, cursor 129

#[test]
fn ticket_log_capture_rejects_unsupported_windows_and_packs_the_threshold_losslessly() {
    // Bootstrap the process tuple through an explicit config, then drop it.
    drop(
        AllocCore::new_with_config(LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto))
            .expect("primordial bootstrap"),
    );

    // --- Packing threshold and round-trip, through the production packer.
    let packed = AllocCore::dbg_thp_control_ticket_log_pack(SUPPORTED_MAX - 1, true);
    assert_eq!(
        packed >> 2,
        SUPPORTED_MAX - 1,
        "the boundary ordinal 2^62 - 1 must pack losslessly"
    );
    assert!(
        packed & 1 == 1,
        "a supported ordinal must carry the valid bit"
    );
    let packed = AllocCore::dbg_thp_control_ticket_log_pack(0, false);
    assert_eq!(
        packed, 0b001,
        "ordinal 0, not selected: 0 << 2 | 0 << 1 | 1"
    );
    let packed = AllocCore::dbg_thp_control_ticket_log_pack(5, true);
    assert_eq!(
        packed >> 2,
        5,
        "a supported ordinal must round-trip through the production packer"
    );
    assert_eq!(
        AllocCore::dbg_thp_control_ticket_log_pack(SUPPORTED_MAX, false),
        0,
        "the threshold ordinal 2^62 itself is outside the supported domain \
         and must pack to the explicit invalid marker 0"
    );
    assert_eq!(
        AllocCore::dbg_thp_control_ticket_log_pack(u64::MAX, true),
        0,
        "any ordinal at or above 2^62 must pack to the invalid marker"
    );

    // --- Gate bounds, through the production gate.
    let mut window = [0u64; 128];
    assert!(
        AllocCore::dbg_thp_control_ticket_log_capture_supported(&window, 0),
        "an empty window (cursor 0) is vacuously supported capture"
    );
    for entry in window.iter_mut() {
        *entry = 0b011; // valid, selected, ordinal 0
    }
    assert!(
        AllocCore::dbg_thp_control_ticket_log_capture_supported(&window, CAP),
        "exactly at the cap is supported: 128 valid records, cursor 128"
    );
    assert!(
        !AllocCore::dbg_thp_control_ticket_log_capture_supported(&window, CAP + 1),
        "a cursor above the cap means records were dropped: not supported"
    );
    window[64] = 0; // the explicit invalid marker inside the consumed window
    assert!(
        !AllocCore::dbg_thp_control_ticket_log_capture_supported(&window, CAP),
        "one invalid-marked record inside the window is not supported capture"
    );

    // --- Hook-level discrimination: the same production gate accepts a
    // handcrafted supported window and rejects the marker-0 variant.
    let mut valid_window = [0u64; 128];
    for entry in valid_window.iter_mut().take(64) {
        *entry = AllocCore::dbg_thp_control_ticket_log_pack(3, false);
    }
    assert!(
        AllocCore::dbg_thp_control_ticket_log_capture_supported(&valid_window, 64),
        "the production gate accepts this handcrafted supported window"
    );
    valid_window[63] = 0;
    assert!(
        !AllocCore::dbg_thp_control_ticket_log_capture_supported(&valid_window, 64),
        "the production gate rejects the marker-0 window"
    );

    // --- Real runtime over-cap leg: 129 eligible fresh records through the
    // installed GlobalAlloc leave cursor = 129 > 128 and the hook must
    // return None, binding the rejection to the actual recorder path.
    AllocCore::dbg_thp_control_ticket_log_reset();
    let ordinal_before = AllocCore::dbg_thp_control_ordinal();
    let (_, empty_cursor) = AllocCore::dbg_thp_control_ticket_log()
        .expect("an empty window after the reset is fully supported capture");
    assert_eq!(empty_cursor, 0, "the reset zeroes the raw cursor");

    let layout = Layout::from_size_align(8 * MIB, 8).expect("valid 8 MiB layout");
    let mut blocks = [core::ptr::null_mut(); RECORDS];
    for block in &mut blocks {
        // SAFETY: valid non-zero layout; the returned pointer is retained.
        *block = unsafe { GLOBAL.alloc(layout) };
        assert!(!block.is_null(), "the 8 MiB record allocation must succeed");
    }
    let ordinal_after = AllocCore::dbg_thp_control_ordinal();
    assert_eq!(
        ordinal_after - ordinal_before,
        RECORDS as u64,
        "exactly {RECORDS} eligible records were consumed: a skipped or extra \
         record changes the over-cap arithmetic"
    );
    assert!(
        AllocCore::dbg_thp_control_ticket_log().is_none(),
        "{RECORDS} real records push the raw cursor past the cap: the hook \
         must reject the over-cap window instead of presenting partial \
         capture as usable evidence"
    );

    for block in blocks.iter().rev() {
        // SAFETY: each pointer is the live 8 MiB allocation.
        unsafe { GLOBAL.dealloc(*block, layout) };
    }
}
