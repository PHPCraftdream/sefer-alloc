//! TH-1: `Reservation::advise_huge_pages` is advisory — the kernel accepts it
//! on Linux/Android, it is a no-op elsewhere, and the span keeps its contents
//! and stays writable either way.
#![cfg(not(miri))]

use aligned_vmem::reserve_aligned;

const MIB: usize = 1024 * 1024;

#[test]
fn advise_huge_pages_keeps_contents_and_reports_the_platform() {
    let size = 8 * MIB;
    let r = reserve_aligned(size, 4 * MIB).expect("reserve");
    let base = r.as_ptr();
    let step = 4096;
    let mut o = 0;
    while o < size {
        // SAFETY: `base..base+size` is the committed span of `r`.
        unsafe { base.add(o).write((o / step) as u8) };
        o += step;
    }
    let accepted = r.advise_huge_pages();
    let expected = cfg!(all(
        any(target_os = "linux", target_os = "android"),
        not(aligned_vmem_mock)
    ));
    assert_eq!(accepted, expected, "hint accepted exactly on Linux/Android");
    let mut o = 0;
    while o < size {
        // SAFETY: as above; the hint does not change the mapping.
        unsafe {
            assert_eq!(base.add(o).read(), (o / step) as u8, "contents kept at {o}");
            base.add(o).write(0xA5);
            assert_eq!(base.add(o).read(), 0xA5, "writable at {o}");
        }
        o += step;
    }
    // Idempotent.
    assert_eq!(r.advise_huge_pages(), expected);
}
