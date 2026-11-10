//! Task #53 (J13b VM-part): `Reservation::try_advise_huge_pages_range` —
//! range-limited huge-page advice. Contract tests: validation happens on
//! EVERY platform before any backend decision; the native Linux/Android
//! backend keeps contents and sets the `hg` hint flag exactly on the
//! advised subrange; unsupported backends produce the typed unsupported
//! `Err`, never a forged success.
//!
//! No file-level `#![cfg]`: per-test `#[cfg]` so every CI row (miri, mock,
//! Windows, macOS) compiles and runs the tests that apply to it.

use std::sync::{Mutex, MutexGuard};

use aligned_vmem::{page_size, reserve_aligned};

const MIB: usize = 1024 * 1024;

/// Process-global one-shot hook state (task #53 hooks) means every test in
/// this binary that reaches the native backend must hold this lock for its
/// whole body: a parallel test's call could otherwise consume an armed
/// failure or be failed by it. Uncontended and costless in builds without
/// `fault-injection`. Deliberately NOT sleeps or --test-threads=1.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// Catches: an impl that routes the empty range into the backend split or
/// errors on it — the zero-length no-op must precede any backend decision,
/// so `Ok(0)` holds on native Linux/Android, miri, mock, Windows and macOS.
#[test]
fn zero_length_is_ok_zero_on_every_platform() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let n = r.try_advise_huge_pages_range(0, 0).expect("empty range");
    assert_eq!(n, 0, "zero-length advice must report zero bytes advised");
}

/// Catches: an unchecked `offset + length` addition — both arguments wrap
/// into a small "in-range" value without `checked_add`, silently passing.
#[test]
fn overflow_is_invalid_argument() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let ps = page_size();
    for (offset, length) in [(usize::MAX, 1), (usize::MAX - ps, 2 * ps)] {
        let e = r
            .try_advise_huge_pages_range(offset, length)
            .expect_err("overflowing range must be rejected");
        assert!(
            e.is_invalid_argument(),
            "overflow must be invalid_argument, got {e:?}"
        );
    }
}

/// Catches: a missing bounds check — an end past `len()` must be a typed
/// `invalid_argument`, never clamped and never silently widened.
#[test]
fn out_of_bounds_is_invalid_argument() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let ps = page_size();
    let e = r
        .try_advise_huge_pages_range(ps, r.len())
        .expect_err("end past len must be rejected");
    assert!(e.is_invalid_argument(), "got {e:?}");
}

/// Catches: missing alignment validation, or validation against RELATIVE
/// offsets instead of ABSOLUTE addresses. `(1, ps)` fails because the
/// absolute start `base + 1` is never page-aligned: the 2 MiB alignment is a
/// page-size multiple on every supported host (2 MiB is a power-of-two
/// multiple of 4/16/64 KiB), so `base` is page-aligned and `base + 1` is not
/// — under miri the `System` allocator's 2-MiB alignment guarantees the same.
/// `(0, ps - 1)` fails the misaligned-length half of the same check.
#[test]
fn misalignment_is_invalid_argument() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let ps = page_size();
    let e = r
        .try_advise_huge_pages_range(1, ps)
        .expect_err("misaligned absolute start must be rejected");
    assert!(e.is_invalid_argument(), "got {e:?}");
    let e = r
        .try_advise_huge_pages_range(0, ps - 1)
        .expect_err("misaligned length must be rejected");
    assert!(e.is_invalid_argument(), "got {e:?}");
}

/// Catches: a wrong advice constant (e.g. `MADV_DONTNEED` zeroes pages on
/// Linux readback), a range arm that touches/decommits instead of hinting,
/// and any contents corruption — advisory advice must leave every page's
/// contents intact and the span writable.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn contents_preserved_and_writable_on_success() {
    let _serial = serial();
    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    let base = r.as_ptr();
    let ps = page_size();
    let mut o = 0;
    while o < len {
        // SAFETY: `base..base+len` is the committed span of `r`. The pattern
        // is always nonzero (the `| 0x80` keeps page index 256 from wrapping
        // to 0), so a zero readback can never pass vacuously.
        let expected = ((o / ps) as u8) | 0x80;
        unsafe { base.add(o).write(expected) };
        o += ps;
    }
    let advised = r.try_advise_huge_pages_range(0, len).expect("advise");
    assert_eq!(advised, len, "exactly `length` bytes must be advised");
    let mut o = 0;
    while o < len {
        // SAFETY: as above; the hint does not change the mapping.
        let expected = ((o / ps) as u8) | 0x80;
        unsafe {
            assert_eq!(base.add(o).read(), expected, "contents at {o}");
            base.add(o).write(0xA5);
            assert_eq!(base.add(o).read(), 0xA5, "writable at {o}");
        }
        o += ps;
    }
}

/// Catches: a forged success or false-zero on platforms without a backend —
/// the unsupported fallback must be the typed unsupported `Err`
/// (`os_refusal_unknown_code`), NOT `invalid_argument` and NOT an `Ok`.
#[cfg(any(
    miri,
    aligned_vmem_mock,
    not(any(target_os = "linux", target_os = "android"))
))]
#[test]
fn unsupported_backend_typed_error() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let ps = page_size();
    let e = r
        .try_advise_huge_pages_range(0, ps)
        .expect_err("no backend on this platform");
    assert!(!e.is_invalid_argument(), "not a contract violation: {e:?}");
    assert_eq!(e.os_code(), None, "unsupported is a no-code sentinel");
}

/// Catches: the fault-injection hook not being consulted, being consulted
/// too early (on validation-rejected or zero-length calls), consuming more
/// than one call, or carrying the wrong sentinel on the injected path.
/// One-shot: zero-length and misaligned calls pass WITHOUT consuming the
/// armed failure (validation precedes the hook consult by contract); the
/// first VALID call fails without touching the OS; the immediately
/// following valid call hits the REAL `madvise` backend and succeeds.
#[cfg(all(
    feature = "fault-injection",
    not(aligned_vmem_mock),
    not(miri),
    any(target_os = "linux", target_os = "android")
))]
#[test]
fn fault_injection_hook() {
    use aligned_vmem::fault_injection::arm_fail_next_advise_range;

    let _serial = serial();
    arm_fail_next_advise_range(0); // disarm any residue from a prior test

    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    let ps = page_size();

    arm_fail_next_advise_range(1);

    // Zero-length is pre-backend by contract: it must succeed with `Ok(0)`
    // WITHOUT consuming the armed one-shot — an impl that consults the hook
    // here breaks the contract.
    let n = r.try_advise_huge_pages_range(0, 0).expect("empty range");
    assert_eq!(n, 0, "zero-length advice must report zero bytes advised");

    // Misaligned is rejected by validation, which precedes the hook consult:
    // it must be the typed `invalid_argument`, again without consuming the
    // armed one-shot.
    let e = r
        .try_advise_huge_pages_range(1, ps)
        .expect_err("misaligned absolute start must be rejected");
    assert!(
        e.is_invalid_argument(),
        "validation-rejected call must be invalid_argument, got {e:?}"
    );

    // The first VALID call is where the armed one-shot fires: no-code
    // sentinel, not a contract violation.
    let e = r
        .try_advise_huge_pages_range(0, len)
        .expect_err("armed fault must fail the next valid call");
    assert!(!e.is_invalid_argument(), "not a contract violation: {e:?}");
    assert_eq!(
        e.os_code(),
        None,
        "injected failure is the no-code sentinel"
    );

    // One-shot consumed: the next call reaches the real backend and must
    // genuinely succeed with the full budget.
    let n = r.try_advise_huge_pages_range(0, len).expect("real advise");
    assert_eq!(n, len, "real backend advises exactly `length` bytes");

    arm_fail_next_advise_range(0); // disarm
}

/// Catches: an impl that widens the hint to the whole mapping — the range
/// arm must set `hg` (VM_HUGEPAGE) on the VMA(s) of the ADVISED subrange and
/// leave it absent on the VMA(s) of the UNADVISED adjacent portion. Asserts
/// the HINT FLAG only, never physical THP allocation.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn advised_subrange_sets_hg_only_on_targeted_vma() {
    // Test-only /proc/self/smaps read: the production sampler never reads
    // /proc. Task #53 hint-flag oracle: `hg` (VM_HUGEPAGE) must be present
    // on the VMA(s) of the advised subrange and absent from the VMA(s) of
    // the unadvised adjacent portion — an impl that widens the hint to the
    // whole mapping fails. Asserts the HINT FLAG only, never physical THP.
    let _serial = serial();
    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    let half = len / 2; // 1 MiB — a multiple of any supported page size.
    let advised = r
        .try_advise_huge_pages_range(0, half)
        .expect("advise subrange");
    assert_eq!(advised, half, "exactly the requested bytes advised");
    let base = r.as_ptr() as usize;
    let (advised_vmas, unadvised_vmas) =
        smaps_vma_flags(base, base + half, base + half, base + len);
    assert!(
        !advised_vmas.is_empty(),
        "no VMA overlaps the advised range"
    );
    assert!(
        !unadvised_vmas.is_empty(),
        "no VMA overlaps the unadvised range"
    );
    for (_, _, flags) in &advised_vmas {
        assert!(
            flags.split_whitespace().any(|t| t == "hg"),
            "advised VMA must carry the hg hint flag, got VmFlags: {flags}"
        );
    }
    for (_, _, flags) in &unadvised_vmas {
        assert!(
            !flags.split_whitespace().any(|t| t == "hg"),
            "unadvised VMA must NOT carry hg, got VmFlags: {flags}"
        );
    }
}

/// Parse `/proc/self/smaps` and return `(start, end, VmFlags line)` for
/// every VMA overlapping `[a_start, a_end)` or `[b_start, b_end)` (the two
/// target windows). Blocks without a `VmFlags:` line are ignored; a missing
/// file or missing windows FAIL the caller (hard panic at the call site) —
/// never a silent skip.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
type VmaFlags = (usize, usize, String);

#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
fn smaps_vma_flags(
    a_start: usize,
    a_end: usize,
    b_start: usize,
    b_end: usize,
) -> (Vec<VmaFlags>, Vec<VmaFlags>) {
    let text = std::fs::read_to_string("/proc/self/smaps")
        .expect("reading /proc/self/smaps: required for the task-#53 hint-flag oracle");
    let mut advised = Vec::new();
    let mut unadvised = Vec::new();
    let mut cur: Option<(usize, usize)> = None;
    for line in text.lines() {
        let header = line.split(' ').next().unwrap_or("");
        if let Some((s, e)) = header.split_once('-').and_then(|(s, e)| {
            Some((
                usize::from_str_radix(s, 16).ok()?,
                usize::from_str_radix(e, 16).ok()?,
            ))
        }) {
            cur = Some((s, e));
        } else if let (Some((s, e)), Some(rest)) = (cur, line.strip_prefix("VmFlags:")) {
            let overlaps = |w: (usize, usize)| s < w.1 && e > w.0;
            let flags = rest.trim().to_string();
            if overlaps((a_start, a_end)) {
                advised.push((s, e, flags.clone()));
            }
            if overlaps((b_start, b_end)) {
                unadvised.push((s, e, flags));
            }
        }
    }
    (advised, unadvised)
}

/// Catches: an off-by-one at the upper span edge — a region ending exactly
/// at `len()` is in bounds and must succeed, not be rejected as over-range.
#[test]
fn span_edge_region_ending_at_len() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let ps = page_size();
    let res = r.try_advise_huge_pages_range(r.len() - ps, ps);
    if cfg!(all(
        any(target_os = "linux", target_os = "android"),
        not(miri),
        not(aligned_vmem_mock)
    )) {
        assert_eq!(res.expect("upper-edge range"), ps, "exactly ps advised");
    } else {
        let e = res.expect_err("no backend on this platform");
        assert!(!e.is_invalid_argument(), "typed unsupported: {e:?}");
        assert_eq!(e.os_code(), None);
    }
}
