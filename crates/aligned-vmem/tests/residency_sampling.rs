//! Task #53 (J13b VM-part): `Reservation::try_sample_residency` — bounded
//! `mincore(2)` residency sampling. Contract tests: validation happens on
//! EVERY platform before the backend split; the native backend counts PAGES
//! (not regions), rounds misaligned windows inward, counts duplicates
//! per-occurrence, and never fabricates observations on unsupported
//! platforms.
//!
//! No file-level `#![cfg]`: per-test `#[cfg]` so every CI row (miri, mock,
//! Windows, macOS) compiles and runs the tests that apply to it.
//!
//! Determinism discipline: fresh anonymous pages are never resident until
//! faulted; a single byte write at a page's start faults the whole base
//! page, so touching one byte per page start makes its residency
//! deterministic. Under THP `always`/mTHP a faulted byte can make a WHOLE
//! huge page resident — the test-side `clean_base_page_control` control
//! premise removes that. No decommit, no RSS or wall-clock assertions; the
//! only `/proc` reading in this crate's tests is the smaps oracle in the
//! advise tests — the production sampler never reads `/proc`.

use std::sync::{Mutex, MutexGuard};

use aligned_vmem::{page_size, reserve_aligned, Reservation, ResidencyRegion};

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

#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
extern "C" {
    fn madvise(addr: *mut core::ffi::c_void, length: usize, advice: i32) -> i32;
}

/// Test-side CONTROL POLICY for exact base-page residency counts (task #53):
/// under THP `always` and mTHP, faulting one byte in a 2-MiB-aligned
/// anonymous mapping can fault a whole huge page, making exact per-page
/// counts impossible. `MADV_NOHUGEPAGE` on the whole reservation removes
/// that premise-breaker. Deliberately done HERE and not by the production
/// sampler — `try_sample_residency` applies no hint and reports whatever
/// the kernel says. A refusal fails loudly (never a silent skip): without
/// the control premise the exact-count assertions below are meaningless.
/// (`MAP_HUGETLB` reservations are huge-page granular and out of scope for
/// these base-page claims.)
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
fn clean_base_page_control(r: &aligned_vmem::Reservation) {
    const MADV_NOHUGEPAGE: i32 = 15;
    // SAFETY: `[as_ptr, as_ptr + len)` is the reservation's live, owned,
    // page-aligned mapping; MADV_NOHUGEPAGE only clears/sets a VMA hint
    // flag and changes no page contents.
    let ret = unsafe { madvise(r.as_ptr().cast(), r.len(), MADV_NOHUGEPAGE) };
    assert_eq!(
        ret, 0,
        "MADV_NOHUGEPAGE refused: host cannot provide the base-page control \
         premise this test's exact counts depend on"
    );
}

/// Catches: a missing 16-region cap — validation runs before any backend
/// decision, so the cap must hold on EVERY platform (miri, mock, Windows,
/// macOS included), not just where `mincore` exists.
#[test]
fn too_many_regions_rejected() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let ps = page_size();
    let regions: Vec<ResidencyRegion> = (0..Reservation::RESIDENCY_SAMPLE_MAX_REGIONS + 1)
        .map(|i| ResidencyRegion::new(i * ps, ps))
        .collect();
    let e = r
        .try_sample_residency(&regions)
        .expect_err("17 regions exceed the cap");
    assert!(e.is_invalid_argument(), "got {e:?}");
}

/// Catches: a missing 2-MiB per-region cap — the 4-MiB span keeps the
/// over-cap region in bounds, so ONLY the cap can reject it (a missing cap
/// would let the query through and overflow the statically-bounded
/// 512-entry mincore stack vector).
#[test]
fn region_cap_rejected() {
    let _serial = serial();
    let r = reserve_aligned(4 * MIB, 4 * MIB).expect("reserve");
    let ps = page_size();
    let regions = [ResidencyRegion::new(
        0,
        Reservation::RESIDENCY_SAMPLE_MAX_REGION_BYTES + ps,
    )];
    let e = r
        .try_sample_residency(&regions)
        .expect_err("region beyond the 2-MiB cap");
    assert!(e.is_invalid_argument(), "got {e:?}");
}

/// Catches: unchecked offset/length arithmetic (overflow) and a missing
/// bounds check (end past `len()`) — both must be typed `invalid_argument`.
#[test]
fn overflow_and_oob_rejected() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let ps = page_size();
    let e = r
        .try_sample_residency(&[ResidencyRegion::new(usize::MAX, 1)])
        .expect_err("overflowing region");
    assert!(e.is_invalid_argument(), "got {e:?}");
    let e = r
        .try_sample_residency(&[ResidencyRegion::new(0, r.len() + ps)])
        .expect_err("end past len");
    assert!(e.is_invalid_argument(), "got {e:?}");
}

/// Catches: fabricated observations on unsupported platforms — a platform
/// with no `mincore` backend must return the typed unsupported `Err`
/// (no-code, not `invalid_argument`) and must NEVER return a forged
/// `Ok(0, 0)` "nothing resident" sample.
#[cfg(any(
    miri,
    aligned_vmem_mock,
    not(any(target_os = "linux", target_os = "android"))
))]
#[test]
fn unsupported_backend_is_typed_error_never_zero_sample() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let ps = page_size();
    let res = r.try_sample_residency(&[ResidencyRegion::new(0, 4 * ps)]);
    let e = match res {
        Ok(s) => panic!("forged sample on unsupported platform: {s:?}"),
        Err(e) => e,
    };
    assert!(!e.is_invalid_argument(), "not a contract violation: {e:?}");
    assert_eq!(e.os_code(), None, "unsupported is a no-code sentinel");
}

/// Catches: the empty slice falling through to the backend split — an empty
/// region slice is a defined no-op yielding a zero `ResidencySample` on
/// EVERY platform (this pins the early return before the `#[cfg]` arm).
#[test]
fn empty_slice_is_zero_sample_on_every_platform() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let s = r
        .try_sample_residency(&[])
        .expect("empty slice is a no-op on every platform");
    assert_eq!(
        (s.resident_pages, s.sampled_pages),
        (0, 0),
        "empty slice must yield the exact zero sample"
    );
}

/// Catches: inverted counts, counting regions instead of pages, or a
/// mincore low-bit error — a fresh anonymous mapping has NO resident pages
/// until first fault, so an untouched span deterministically samples as
/// zero resident over `len / ps` queried pages.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn untouched_span_is_zero_resident() {
    let _serial = serial();
    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    clean_base_page_control(&r);
    let ps = page_size();
    let s = r
        .try_sample_residency(&[ResidencyRegion::new(0, len)])
        .expect("sample");
    assert_eq!(s.resident_pages, 0, "untouched pages are never resident");
    assert_eq!(s.sampled_pages, len / ps, "every page queried exactly once");
}

/// Catches: rounding errors, off-by-one windows, wrong-bit counting —
/// exactly the touched pages must read resident, exactly the untouched
/// window's pages must be queried and read zero.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn touched_pages_are_resident() {
    let _serial = serial();
    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    clean_base_page_control(&r);
    let base = r.as_ptr();
    let ps = page_size();
    for i in 0..3 {
        // SAFETY: `base..base+len` is the committed span of `r`; `i * ps`
        // stays inside it. One byte faults the whole base page.
        unsafe { base.add(i * ps).write(0x11) };
    }
    let s = r
        .try_sample_residency(&[ResidencyRegion::new(0, 3 * ps)])
        .expect("sample touched window");
    assert_eq!(s.resident_pages, 3, "each touched page is resident");
    assert_eq!(s.sampled_pages, 3, "each page queried exactly once");
    let s = r
        .try_sample_residency(&[ResidencyRegion::new(4 * ps, 2 * ps)])
        .expect("sample untouched window");
    assert_eq!(s.resident_pages, 0, "untouched pages stay non-resident");
    assert_eq!(s.sampled_pages, 2, "both pages queried");
}

/// Catches: a "regions with any resident page" metric — touching pages 0
/// and 2 (skipping 1) of a 3-page window must report `resident_pages == 2`,
/// not 1; the counters are PAGES, as task #53 requires.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn counts_pages_not_regions() {
    let _serial = serial();
    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    clean_base_page_control(&r);
    let base = r.as_ptr();
    let ps = page_size();
    for i in [0usize, 2] {
        // SAFETY: `i * ps` stays within the committed span of `r`.
        unsafe { base.add(i * ps).write(0x22) };
    }
    let s = r
        .try_sample_residency(&[ResidencyRegion::new(0, 3 * ps)])
        .expect("sample");
    assert_eq!(s.sampled_pages, 3);
    assert_eq!(s.resident_pages, 2, "pages, not regions-with-any-resident");
}

/// Catches: start-round-up / end-round-down errors in the effective-window
/// computation — `(ps/2, 3*ps)` must query exactly pages 1 and 2, and a
/// misaligned sub-page sliver must be a DEFINED zero contribution, not an
/// error and not a silently widened query.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn misaligned_region_rounds_to_whole_pages() {
    let _serial = serial();
    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    clean_base_page_control(&r);
    let base = r.as_ptr();
    let ps = page_size();
    for i in 0..4 {
        // SAFETY: `i * ps` stays within the committed span of `r`; one byte
        // faults the whole base page.
        unsafe { base.add(i * ps).write(0x33) };
    }
    // Effective window: start rounds up to `ps`, end rounds down to `3*ps`
    // — pages 1 and 2 only.
    let s = r
        .try_sample_residency(&[ResidencyRegion::new(ps / 2, 3 * ps)])
        .expect("sample");
    assert_eq!(s.sampled_pages, 2, "window is [ps, 3*ps): two whole pages");
    assert_eq!(s.resident_pages, 2, "pages 1 and 2 were touched");
    // A misaligned sliver is a defined zero contribution.
    let s = r
        .try_sample_residency(&[ResidencyRegion::new(ps - 1, 1)])
        .expect("sliver is Ok, not an error");
    assert_eq!(s.sampled_pages, 0, "sliver rounds to an empty window");
    assert_eq!(s.resident_pages, 0);
}

/// Catches: hidden deduplication changing the estimator's denominator — the
/// same region passed twice is sampled twice, so both counters double; the
/// estimator stays per-occurrence-weighted by contract.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn duplicates_count_per_occurrence() {
    let _serial = serial();
    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    clean_base_page_control(&r);
    let base = r.as_ptr();
    let ps = page_size();
    for i in 0..2 {
        // SAFETY: `i * ps` stays within the committed span of `r`.
        unsafe { base.add(i * ps).write(0x44) };
    }
    let dup = [
        ResidencyRegion::new(0, 2 * ps),
        ResidencyRegion::new(0, 2 * ps),
    ];
    let s = r.try_sample_residency(&dup).expect("sample");
    assert_eq!(
        s.sampled_pages, 4,
        "no silent dedup: 2 pages x 2 occurrences"
    );
    assert_eq!(s.resident_pages, 4, "per-occurrence weighting, not a union");
}

/// Catches: the residency hook not being consulted, being consulted too
/// early (on empty-slice calls), consuming more than one call, or carrying
/// the wrong sentinel — an empty slice passes WITHOUT consuming the armed
/// one-shot (pre-backend by contract); the first VALID call fails without
/// touching the OS; the next call hits the REAL `mincore` backend and
/// reports the touched reality under the NOHUGEPAGE control.
#[cfg(all(
    feature = "fault-injection",
    not(aligned_vmem_mock),
    not(miri),
    any(target_os = "linux", target_os = "android")
))]
#[test]
fn fault_injection_hook() {
    use aligned_vmem::fault_injection::arm_fail_next_residency_sample;

    let _serial = serial();
    arm_fail_next_residency_sample(0); // disarm any residue from a prior test

    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    clean_base_page_control(&r);
    let base = r.as_ptr();
    let ps = page_size();
    for i in 0..2 {
        // SAFETY: `i * ps` stays within the committed span of `r`; one byte
        // faults the whole base page (under the NOHUGEPAGE control, exactly
        // the base page — no huge-page over-fault).
        unsafe { base.add(i * ps).write(0x55) };
    }

    arm_fail_next_residency_sample(1);

    // Empty slice is pre-backend by contract: it must succeed with the zero
    // sample WITHOUT consuming the armed one-shot.
    let s = r.try_sample_residency(&[]).expect("empty slice");
    assert_eq!(
        (s.resident_pages, s.sampled_pages),
        (0, 0),
        "empty slice must yield the exact zero sample"
    );

    // 17 regions are rejected by validation, which precedes the hook
    // consult: typed `invalid_argument`, again without consuming the armed
    // one-shot.
    let regions: Vec<ResidencyRegion> = (0..Reservation::RESIDENCY_SAMPLE_MAX_REGIONS + 1)
        .map(|i| ResidencyRegion::new(i * ps, ps))
        .collect();
    let e = r
        .try_sample_residency(&regions)
        .expect_err("17 regions exceed the cap");
    assert!(
        e.is_invalid_argument(),
        "validation-rejected call must be invalid_argument, got {e:?}"
    );

    // The first VALID call is where the armed one-shot fires: no-code
    // sentinel, not a contract violation.
    let e = r
        .try_sample_residency(&[ResidencyRegion::new(0, 2 * ps)])
        .expect_err("armed fault must fail the next valid call");
    assert!(!e.is_invalid_argument(), "not a contract violation: {e:?}");
    assert_eq!(
        e.os_code(),
        None,
        "injected failure is the no-code sentinel"
    );

    // One-shot consumed: the next call reaches the real backend and must
    // reflect the touched reality (2 resident of 2 sampled) under the
    // NOHUGEPAGE control.
    let s = r
        .try_sample_residency(&[ResidencyRegion::new(0, 2 * ps)])
        .expect("real sample");
    assert_eq!(s.sampled_pages, 2);
    assert_eq!(s.resident_pages, 2, "both touched pages are resident");

    arm_fail_next_residency_sample(0); // disarm
}

/// Catches: upper-edge rounding discarding the last page — a window ending
/// exactly at `len()` must include the final page, so touching its first
/// byte reports 1 resident of 1 sampled.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn span_edge_region_ending_at_len() {
    let _serial = serial();
    let len = 2 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve");
    clean_base_page_control(&r);
    let base = r.as_ptr();
    let ps = page_size();
    // SAFETY: `len - ps` is the last page's start, within the committed span.
    unsafe { base.add(len - ps).write(0x66) };
    let s = r
        .try_sample_residency(&[ResidencyRegion::new(len - ps, ps)])
        .expect("sample last page");
    assert_eq!(s.sampled_pages, 1, "the last page is queried");
    assert_eq!(s.resident_pages, 1, "the last page was touched");
}
