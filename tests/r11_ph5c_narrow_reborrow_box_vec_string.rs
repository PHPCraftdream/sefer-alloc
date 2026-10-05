//! Ph5c gap (е)-P3: native (non-Miri) narrow-reborrow `Box<[u8; N]>` for
//! N = 1..=7 plus Vec/String growth, all through an *installed*
//! `#[global_allocator]`.
//!
//! Distinct from the C1/MODEL-LIMIT witnesses (`miri_global_box_acceptance`
//! is an expected-red Miri witness; the `r11_box_cap_gate_*` files are
//! system-Gate witnesses): this file is a plain green native test with NO
//! address/reuse-in-live-frame assertions — only content, health and
//! drop-correctness oracles.

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const PATTERN: u8 = 0x5c;

// `#[inline(never)]` so the Box is truly passed by value across a real call
// frame (no inlining folding the move away).
#[inline(never)]
fn consume<const N: usize>(mut b: Box<[u8; N]>) -> u8 {
    // Narrow reborrow INSIDE the callee, on the by-value box.
    let narrow: &mut [u8; N] = b.as_mut();
    narrow[0] = narrow[0].wrapping_add(1);
    narrow[N - 1]
}

#[test]
fn narrow_reborrow_box_arrays_n1_to_n7() {
    // Each size 1..=7 gets its own monomorphised ladder: build → wide
    // content check → by-value move across a real (non-inlined) call frame →
    // narrow reborrow inside the callee → drop → fresh reissue of the same
    // size (allocator health; NO address assertions).
    fn round<const N: usize>() {
        let b: Box<[u8; N]> = Box::new([PATTERN; N]);
        assert!(b.iter().all(|&x| x == PATTERN), "N={N} content");
        let last = consume(b);
        // consume() increments narrow[0]; for N=1 that IS the last byte.
        let expect_last = if N == 1 {
            PATTERN.wrapping_add(1)
        } else {
            PATTERN
        };
        assert_eq!(last, expect_last, "N={N} last byte");
        let c: Box<[u8; N]> = Box::new([PATTERN; N]);
        assert_eq!(c[N - 1], PATTERN, "N={N} post-reissue");
        drop(c);
        // alloc_zeroed leg: `Box::new([0; N])` compiles to __rust_alloc_zeroed
        // for every N in 1..=7 (Small class) — must come back all-zero, even
        // after the same size class was made DIRTY (alloc → 0xff fill → free),
        // so a recycled non-virgin block cannot pass by accident.
        let z: Box<[u8; N]> = Box::new([0u8; N]);
        assert!(z.iter().all(|&x| x == 0), "N={N} alloc_zeroed nonzero");
        drop(z);
        let dirty: Box<[u8; N]> = Box::new([0xffu8; N]);
        assert_eq!(dirty[0], 0xff);
        drop(dirty);
        let z2: Box<[u8; N]> = Box::new([0u8; N]);
        assert!(
            z2.iter().all(|&x| x == 0),
            "N={N} dirty-reuse alloc_zeroed nonzero"
        );
        drop(z2);
    }
    round::<1>();
    round::<2>();
    round::<3>();
    round::<4>();
    round::<5>();
    round::<6>();
    round::<7>();
}

#[test]
fn vec_growth_small_to_large_content_preserved() {
    // Start Small, promote through realloc into Large territory (>= SEGMENT
    // = 4 MiB; 64 KiB growth steps exercise multiple small reallocs first).
    let mut expected_len;
    let mut v: Vec<u8> = Vec::new();
    for &target in &[8usize, 1024, 64 * 1024, 5 * 1024 * 1024] {
        while v.len() < target {
            v.push((v.len() as u8) ^ 0x3d);
        }
        expected_len = v.len();
        // Spot-check preserved content at several indices, incl. the very
        // first byte written before every growth step.
        for &idx in &[0usize, 7, 511, 4095, expected_len / 2, expected_len - 1] {
            if idx < expected_len {
                assert_eq!(v[idx], (idx as u8) ^ 0x3d, "idx {idx} @ len {expected_len}");
            }
        }
    }
    assert_eq!(v.len(), 5 * 1024 * 1024);
    assert_eq!(v[0], 0x3d);
    assert_eq!(v[4 * 1024 * 1024], ((4 * 1024 * 1024usize) as u8) ^ 0x3d);
    drop(v);

    // Round-trip: allocator healthy after the Large drop.
    let w: Vec<u8> = vec![7u8; 4096];
    assert_eq!(w[4095], 7);
    // calloc-shaped path through the installed allocator (Small class), after
    // dirtying the class: alloc → fill 0xff → free → alloc_zeroed.
    let d: Vec<u8> = vec![0xffu8; 512];
    assert_eq!(d[0], 0xff);
    drop(d);
    let z: Vec<u8> = vec![0u8; 512];
    assert!(
        z.iter().all(|&x| x == 0),
        "dirty-reuse vec![0; 512] nonzero"
    );
}

#[test]
fn string_growth_utf8_preserved() {
    let mut s = String::new();
    let words = ["α", "β", "γ", "hello", "мир", "🌍"];
    for round in 0..4096 {
        for w in words {
            s.push_str(w);
            s.push(',');
        }
        if round % 512 == 0 {
            // UTF-8 validity + a stable prefix at every growth phase.
            assert!(s.starts_with("α,β,γ,hello,мир,🌍,"), "round {round}");
        }
    }
    assert!(s.ends_with("🌍,"));
    drop(s);
    let fresh = String::from("round-trip");
    assert_eq!(fresh, "round-trip");
}
