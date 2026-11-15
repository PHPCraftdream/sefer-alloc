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

    // The growth arm needs a watermark that CAN grow. Where lazy commit is not
    // honored — under `aligned_vmem_mock`, and on any platform whose backend
    // commits the whole span at reserve time — the initial watermark is
    // already `len()`, so `ensure_committed` of anything smaller is a no-op and
    // the rendering is CORRECTLY unchanged. The first version of this test
    // asserted the rendering must differ unconditionally and went red in the
    // gate's mock row for exactly that reason; the regime is now branched on
    // explicitly and announced, so neither arm can pass vacuously unnoticed.
    if initial < r.len() {
        let target = (initial + page_size()).min(r.len());
        assert!(
            target > initial,
            "growth target must be strictly above the watermark"
        );
        r.ensure_committed(target).expect("grow");
        let s1 = format!("{r:?}");
        assert!(
            s1.contains(&format!("committed_len: {}", r.committed_len())),
            "grown Debug must show the updated watermark: {s1}"
        );
        assert_ne!(s0, s1, "growing the watermark must change the rendering");
        eprintln!(
            "regime: growable watermark ({initial} -> {})",
            r.committed_len()
        );
    } else {
        // HONEST LIMIT of this arm (task #1118, from the @oh review): this
        // `assert_eq!` is TAUTOLOGICAL given the branch condition — `initial >=
        // len` plus the type invariant `committed <= inner.len()` forces
        // equality — so it pins the invariant, not the rendering. In this
        // regime the only live assertion ABOUT `Debug` is the pre-branch
        // `s0.contains("committed_len: {initial}")`, and because `committed ==
        // len` here, this test structurally cannot distinguish a `Debug` impl
        // that printed `inner.len()` where it meant `self.committed`. Stated
        // rather than left for the next reviewer to rediscover.
        assert_eq!(
            initial,
            r.len(),
            "a non-growable watermark must be exactly the full span"
        );
        eprintln!(
            "regime: watermark already at the full span ({initial}) — growth arm not applicable"
        );
    }
    // NOTE on the two `eprintln!`s above (task #1118): libtest CAPTURES stdout
    // and stderr for a PASSING test, so a green `npm run check` log shows
    // neither line — `grep -c "regime: "` on it returns 0. They surface only
    // under `--nocapture` or when the test FAILS, which is the case they were
    // written for. Commit `66baf1f`'s body claimed "a reader of the log can now
    // see WHICH regime executed"; that is false for a green run, and the
    // anti-vacuity work is done by the branch plus the `assert_eq!`, not by the
    // announcement. Recorded here rather than by amending that commit, per this
    // project's non-retroactive posture.
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
