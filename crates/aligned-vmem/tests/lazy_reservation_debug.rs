//! `LazyReservation`'s hand-written `Debug` (task #1107 / finding I1).
//!
//! The watermark is the single most diagnostically important piece of state
//! in the type — the reason the type exists — so the test asserts it is
//! rendered. Assertions are SUBSTRING-based, not exact-string: the inner
//! `Reservation` prints raw pointers, which vary run to run, and pinning the
//! full string would make the test brittle for no gain. What IS pinned is
//! the leak surface: every identifier in the rendering must be one of the
//! sanctioned names the impl (plus `Reservation`'s own public `Debug`)
//! deliberately exposes.

#![cfg(feature = "lazy-commit")]

use aligned_vmem::{page_size, LazyReservation};

const SPAN: usize = 4 * 1024 * 1024;

fn fresh() -> LazyReservation {
    aligned_vmem::reserve_aligned_lazy(SPAN, SPAN, page_size()).expect("lazy reserve 4 MiB")
}

/// The watermark renders, at both its platform values (initial and grown).
#[test]
fn debug_shows_the_watermark() {
    let mut r = fresh();
    let initial = r.committed_len();
    let s0 = format!("{r:?}");
    assert!(
        s0.contains(&format!("committed_len: {initial}")),
        "initial Debug must show the watermark: {s0}"
    );

    r.ensure_committed(8 * page_size()).expect("grow");
    let s1 = format!("{r:?}");
    assert!(
        s1.contains(&format!("committed_len: {}", r.committed_len())),
        "grown Debug must show the updated watermark: {s1}"
    );
    assert_ne!(s0, s1, "growing the watermark must change the rendering");
}

/// The rendering exposes exactly the intended fields and nothing else — in
/// particular no inner field names beyond what `Reservation`'s own public
/// `Debug` already prints.
#[test]
fn debug_exposes_nothing_beyond_public_observables() {
    let r = fresh();
    let s = format!("{r:?}");
    assert!(s.starts_with("LazyReservation {"), "outer type name: {s}");
    assert!(
        s.contains("inner: Reservation {"),
        "inner reservation renders: {s}"
    );
    let sanctioned = [
        "LazyReservation",
        "Reservation",
        "committed_len",
        "inner",
        "base",
        "len",
        "reservation",
        "reservation_len",
        "align",
        "granted_huge",
        "false",
        "true",
    ];
    for token in s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if token
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            assert!(
                sanctioned.contains(&token),
                "unexpected identifier `{token}` in Debug output: {s}"
            );
        }
    }
}

/// A `LazyReservation` is never huge-backed, and the rendering shows it via
/// the inner reservation's own field — a cheap consistency check that the
/// inner rendering is the real `Reservation` Debug, not a paraphrase.
#[test]
fn debug_inner_reports_not_huge_and_true_geometry() {
    let r = fresh();
    let s = format!("{r:?}");
    assert!(
        s.contains("granted_huge: false"),
        "lazy reservations are never huge: {s}"
    );
    assert!(s.contains(&format!("len: {}", r.len())), "{s}");
    assert!(s.contains(&format!("align: {}", r.align())), "{s}");
}
