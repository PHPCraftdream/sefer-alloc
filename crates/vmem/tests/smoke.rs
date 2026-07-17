//! Smoke tests for `aligned-vmem`: reservation alignment, read/write, decommit
//! round-trip, RAII vs manual release, and contract rejection.

use aligned_vmem::{
    decommit_lazy, leak_zeroed_pages, page_size, recommit, release, reserve_aligned,
    try_reserve_aligned, VmemError, PAGE,
};

const MIB: usize = 1024 * 1024;

#[test]
fn reserve_is_aligned_and_writable() {
    let span = 4 * MIB;
    let r = reserve_aligned(span, span).expect("reserve 4 MiB aligned 4 MiB");
    let base = r.as_ptr();
    assert!(!base.is_null());
    assert_eq!(base as usize % span, 0, "base must be span-aligned");
    assert_eq!(r.len(), span);

    // Write/readback the whole span at page stride to fault pages in.
    // SAFETY: base is valid for r.len() bytes; we own it exclusively.
    unsafe {
        let mut off = 0;
        while off < span {
            base.add(off).write(0xA5);
            assert_eq!(base.add(off).read(), 0xA5);
            off += PAGE;
        }
    }
    // RAII: dropping `r` releases the reservation.
}

#[test]
fn manual_release_via_into_parts() {
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: valid for r.len().
    unsafe { base.write(0x11) };
    let (raw, raw_len, raw_align) = r.into_parts();
    assert!(!raw.is_null());
    assert_eq!(raw_align, span);
    // SAFETY: triple came from into_parts, released exactly once.
    unsafe { release(raw, raw_len, raw_align) };
}

#[test]
fn decommit_recommit_roundtrip() {
    let span = 4 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: write into the second half.
    unsafe {
        base.add(span / 2).write(0x77);
        assert_eq!(base.add(span / 2).read(), 0x77);
    }
    // Decommit the second half, then recommit it.
    // SAFETY: base is a live reservation; [span/2, span) is page-aligned and
    // contains nothing we still need.
    unsafe {
        assert!(recommit(base, 0, 0), "empty range no-op reports success");
        aligned_vmem::decommit(base, span / 2, span);
        assert!(
            recommit(base, span / 2, span),
            "recommit of a live reservation's decommitted range must succeed"
        );
        // After recommit the page reads as zero (fresh OS page). Skipped under
        // miri AND under the `mock` feature: both model decommit/recommit as
        // no-ops (no real RSS / zero-fill-on-recommit), so the previously-
        // written byte legally persists — this zero-fill guarantee is a
        // real-OS property.
        #[cfg(not(any(miri, feature = "mock")))]
        assert_eq!(
            base.add(span / 2).read(),
            0,
            "recommitted page must be zeroed"
        );
    }
}

#[test]
fn recommit_is_fallible_and_reports_success_on_the_happy_path() {
    // Non-regression for the fallible `recommit` API (bug-hunt 2026-07-09):
    // `recommit` now returns `bool` (`true` = committed / no-op, `false` = OS
    // refused). We cannot portably force a commit-charge failure without an FFI
    // test seam, so this locks the SUCCESS contract: a well-formed recommit of a
    // decommitted range on a live reservation returns `true`, and misformed /
    // empty ranges return `true` as a no-op. A `false` from a genuine OOM is the
    // path `carve_block`/`carve_batch` translate into a null carve.
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: base is a live reservation for `span` bytes.
    unsafe {
        assert!(recommit(base, 0, 0), "empty range is a success no-op");
        assert!(recommit(base, span, span + PAGE), "start>=end no-op");
        assert!(recommit(base, 1, PAGE), "misaligned start no-op");
        aligned_vmem::decommit(base, span / 2, span);
        assert!(
            recommit(base, span / 2, span),
            "recommit of decommitted range on a live reservation succeeds"
        );
        // Writing into the now-committed range must not fault.
        base.add(span / 2).write(0x5C);
        assert_eq!(base.add(span / 2).read(), 0x5C);
    }
}

#[test]
fn rejects_bad_contracts() {
    assert!(reserve_aligned(0, PAGE).is_none(), "zero size rejected");
    assert!(
        reserve_aligned(PAGE, 3).is_none(),
        "non-pow2 align rejected"
    );
    assert!(reserve_aligned(PAGE, 64).is_none(), "align < PAGE rejected");
    assert!(
        reserve_aligned(PAGE + 1, PAGE).is_none(),
        "non-page-multiple size rejected"
    );
}

#[test]
fn page_size_is_a_valid_os_page() {
    // 0.2: `page_size()` now queries the OS (was a hardcoded 4 KiB). It must be
    // a non-zero power of two and at least the crate's minimum granularity
    // `PAGE` (4 KiB). On x86_64/aarch64-4k it is 4 KiB; on Apple Silicon macOS
    // 16 KiB; on some Linux configs 64 KiB — all satisfy this invariant.
    let ps = page_size();
    assert!(ps.is_power_of_two(), "page size must be a power of two");
    assert!(ps >= PAGE, "OS page size must be at least PAGE (4 KiB)");
    // Cached: a second call returns the same value.
    assert_eq!(page_size(), ps);
    assert_eq!(PAGE, 4096);
}

#[test]
fn try_reserve_reports_invalid_argument() {
    // 0.2 fallible API: a contract violation yields InvalidArgument (no OS call).
    let e = match try_reserve_aligned(0, PAGE) {
        Ok(_) => panic!("zero size must be rejected"),
        Err(e) => e,
    };
    assert!(e.is_invalid_argument());
    assert_eq!(e.os_code(), None);
    assert_eq!(e, VmemError::invalid_argument());
    // A well-formed request succeeds.
    let r = try_reserve_aligned(2 * MIB, 2 * MIB).expect("valid request");
    assert_eq!(r.len(), 2 * MIB);
}

#[test]
fn decommit_lazy_roundtrip() {
    // 0.2 MADV_FREE variant: decommit_lazy then recommit and write.
    let span = 4 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: write, lazily decommit, recommit, write again.
    unsafe {
        base.add(span / 2).write(0x9E);
        decommit_lazy(base, span / 2, span);
        assert!(
            recommit(base, span / 2, span),
            "recommit after decommit_lazy must succeed"
        );
        base.add(span / 2).write(0x3C);
        assert_eq!(base.add(span / 2).read(), 0x3C);
    }
}

#[test]
fn leak_zeroed_pages_is_zeroed_and_static() {
    // 0.2 helper: reserve zeroed pages leaked for the process lifetime.
    let size = 3 * PAGE + 7; // rounds up to 4 pages
    let p = leak_zeroed_pages(size).expect("leak zeroed");
    let base = p.as_ptr();
    assert_eq!(base as usize % PAGE, 0, "PAGE-aligned");
    // SAFETY: valid for at least `size` bytes, guaranteed zeroed on every backend.
    unsafe {
        for off in 0..size {
            assert_eq!(base.add(off).read(), 0, "byte {off} must be zero");
        }
        // Writable.
        base.write(0x42);
        assert_eq!(base.read(), 0x42);
    }
    assert!(leak_zeroed_pages(0).is_none(), "zero size rejected");
}

#[test]
fn distinct_reservations_do_not_overlap() {
    let span = 2 * MIB;
    let a = reserve_aligned(span, span).expect("a");
    let b = reserve_aligned(span, span).expect("b");
    let pa = a.as_ptr() as usize;
    let pb = b.as_ptr() as usize;
    // Non-overlapping usable spans.
    assert!(pa + span <= pb || pb + span <= pa, "reservations overlap");
}
