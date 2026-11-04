//! NL-1: the `misuse-checks` switch of the per-block state byte.
//!
//! The byte only rejects a contract violation, so a build without the feature
//! neither writes nor reads it in release; debug builds keep it. This pins both
//! halves: the byte of an issued block is nonzero and becomes FREE after the
//! free exactly when it is tracked, and stays zero otherwise. It also pins that
//! `production` and `hardened` carry the feature. NL-3b: a debug build without the
//! feature keeps the byte, and a rejected free there aborts (safe code never reaches
//! the check); every other build treats it as the no-op it always was.
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

const CHILD: &str = "NL1_SHADOW_CHILD";
const SHADOW_ABORTS: bool = cfg!(all(debug_assertions, not(feature = "misuse-checks")));
// A release build without the feature neither writes nor reads the byte: a double free
// there corrupts the free list (undefined by contract), so it has no defined outcome to pin.
const NO_DEFINED_OUTCOME: bool = cfg!(all(not(debug_assertions), not(feature = "misuse-checks")));

#[test]
fn rejected_free_aborts_only_in_debug_without_the_feature() {
    if NO_DEFINED_OUTCOME {
        return;
    }
    if std::env::var_os(CHILD).is_some() {
        let layout = Layout::from_size_align(64, 16).unwrap();
        let mut ac = AllocCore::new().expect("primordial");
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        // SAFETY: the first free is valid; the second one is the contract violation
        // under test (a double free), which the build either rejects or aborts on.
        unsafe {
            ac.dealloc(p, layout);
            ac.dealloc(p, layout);
        }
        println!("NL1_SHADOW_CHILD_SURVIVED");
        return;
    }
    // Under a cross runner (qemu) the test binary cannot be exec'd directly: skip there.
    let exe = std::env::current_exe().expect("exe");
    let listed = std::process::Command::new(&exe).arg("--list").output();
    if !listed.is_ok_and(|o| {
        o.status.success()
            && String::from_utf8_lossy(&o.stdout).contains("rejected_free_aborts_only_in_debug")
    }) {
        return;
    }
    let out = std::process::Command::new(exe)
        .arg("rejected_free_aborts_only_in_debug_without_the_feature")
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD, "1")
        .output()
        .expect("spawn child");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if SHADOW_ABORTS {
        assert!(
            !out.status.success(),
            "double free must abort:
{stdout}
{stderr}"
        );
        assert!(
            stderr.contains("misuse check rejected a free in safe mode"),
            "abort names the shadow site:
{stderr}"
        );
        assert!(!stdout.contains("NL1_SHADOW_CHILD_SURVIVED"));
    } else {
        assert!(
            out.status.success(),
            "double free is a no-op here:
{stdout}
{stderr}"
        );
        assert!(stdout.contains("NL1_SHADOW_CHILD_SURVIVED"), "{stdout}");
    }
}
