//! B4-ablate phase 1 smoke test: the generated variant copy
//! (`sefer-alloc-variant`, generated under `.rush/variant/`) LINKS next to
//! the main
//! `sefer-alloc` crate in ONE test binary and serves small alloc/dealloc
//! through its own `GlobalAlloc` face — the exact arm shape the
//! `ab_variant` judge mode drives (see `examples/_shared/
//! b4_interleave_core.rs`, slots 4/5).
//!
//! No heavy runs: no timing, no judge kernels — just link + identity +
//! one alloc/free round-trip per allocator.

use std::alloc::{GlobalAlloc, Layout};

#[test]
fn variant_links_and_serves_small_alloc() {
    // The two crates MUST be distinct packages/libs in the same binary.
    // (Both expose `SeferAlloc`; the paths differ by crate name.)
    let layout = Layout::from_size_align(64, 16).expect("layout");

    // Main crate arm (production face, same as the judge's arm A).
    let a = sefer_alloc::SeferAlloc::new();
    // Variant-copy arm (the generated copy; `null` variant = unpatched).
    let b = sefer_alloc_variant::SeferAlloc::new();

    unsafe {
        // SAFETY: valid layout; result null-checked before use.
        let pa = a.alloc(layout);
        assert!(!pa.is_null(), "main-crate SeferAlloc.alloc(64,16) == null");
        // SAFETY: write+read within the block; then free with the SAME layout.
        std::ptr::write_bytes(pa, 0xAB, 64);
        assert_eq!(*pa, 0xAB);

        let pb = b.alloc(layout);
        assert!(!pb.is_null(), "variant SeferAlloc.alloc(64,16) == null");
        std::ptr::write_bytes(pb, 0x5A, 64);
        assert_eq!(*pb, 0x5A);

        // Distinct allocators must not hand out aliased blocks.
        assert_ne!(pa, pb);

        // SAFETY: pa/pb from the matching alloc above with this layout.
        a.dealloc(pa, layout);
        b.dealloc(pb, layout);
    }
}

#[test]
fn variant_alloc_zeroed_and_realloc_face() {
    // The judge shims also drive realloc/alloc_zeroed shapes; smoke them on
    // the variant's GlobalAlloc face.
    let b = sefer_alloc_variant::SeferAlloc::new();
    let l16 = Layout::from_size_align(16, 8).expect("layout16");
    unsafe {
        // SAFETY: valid layout; null-checked.
        let p = b.alloc_zeroed(l16);
        assert!(!p.is_null());
        assert_eq!(*p, 0);
        // SAFETY: grow from exactly l16 to 128 bytes.
        let q = b.realloc(p, l16, 128);
        assert!(!q.is_null());
        // SAFETY: q from the realloc above; exact (128, 8) layout.
        b.dealloc(q, Layout::from_size_align(128, 8).expect("layout128"));
    }
}
