//! Smoke tests for `aligned-vmem`: reservation alignment, read/write, decommit
//! round-trip, RAII vs manual release, and contract rejection.

use aligned_vmem::{page_size, recommit, release, reserve_aligned, PAGE};

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
        // After recommit the page reads as zero (fresh OS page).
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
fn page_size_is_4k() {
    assert_eq!(page_size(), 4096);
    assert_eq!(PAGE, 4096);
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
