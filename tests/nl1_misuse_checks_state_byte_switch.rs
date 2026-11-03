//! NL-1: the `misuse-checks` switch of the per-block state byte.
//!
//! The byte only rejects a contract violation, so a build without the feature
//! neither writes nor reads it in release; debug builds keep it. This pins both
//! halves: the byte of an issued block is nonzero and becomes FREE after the
//! free exactly when it is tracked, and stays zero otherwise. It also pins that
//! `production` and `hardened` carry the feature.
#![cfg(all(feature = "alloc-core", feature = "internals"))]

#[cfg(all(feature = "production", not(feature = "misuse-checks")))]
compile_error!("`production` must include `misuse-checks` until the default is flipped");
#[cfg(all(feature = "hardened", not(feature = "misuse-checks")))]
compile_error!("`hardened` must imply `misuse-checks`");

use core::alloc::Layout;

use sefer_alloc::AllocCore;

const SEGMENT: usize = 4 * 1024 * 1024;
const TRACKED: bool = cfg!(any(feature = "misuse-checks", debug_assertions));

/// State byte of the granule that holds `p`.
fn state_byte_of(ac: &AllocCore, p: *mut u8) -> u8 {
    let off = p as usize & (SEGMENT - 1);
    let g = off >> 4;
    let mut prefix = vec![0u8; g + 1];
    // SAFETY: `p` was issued by `ac`; its segment is owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(p, &mut prefix) };
    prefix[g]
}

#[test]
fn state_byte_is_written_and_cleared_iff_tracked() {
    let layout = Layout::from_size_align(64, 16).unwrap();
    let mut ac = AllocCore::new().expect("primordial");
    let p = ac.alloc(layout);
    assert!(!p.is_null());
    let issued = state_byte_of(&ac, p);
    assert_eq!(
        issued != 0,
        TRACKED,
        "issued byte {issued:#x}, tracked {TRACKED}"
    );

    // SAFETY: `p` is a live block of `layout`, freed once.
    unsafe { ac.dealloc(p, layout) };
    assert_eq!(
        ac.dbg_is_free_for(p),
        TRACKED,
        "free state visible iff tracked"
    );
    assert_eq!(state_byte_of(&ac, p) >> 6 == 2, TRACKED);

    // The freed block is reissued either way.
    let q = ac.alloc(layout);
    assert_eq!(q, p, "the LIFO bin reissues the freed block");
}
