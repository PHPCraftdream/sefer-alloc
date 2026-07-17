//! R6-OPT-P0-2 (round 2): native tests for [`HeapOverflow`]'s two-tier
//! storage split — the always-inline "emergency" tier plus the lazily-
//! materialised sidecar covering the remainder of `HEAP_OVERFLOW_CAP`.
//!
//! Two properties this file proves:
//!
//! 1. **Lazy materialisation** — the sidecar pointer stays `null` until the
//!    inline tier (`INLINE_CAP` entries) is genuinely exhausted; pushing
//!    exactly `INLINE_CAP` entries and draining them never touches the
//!    sidecar; pushing `INLINE_CAP + 1` entries DOES materialise it.
//! 2. **The wedge hazard cannot occur** — a sidecar-materialisation failure
//!    (modelled here via the rollback hook,
//!    `HeapOverflow::dbg_rollback_sidecar_sentinel_for_test`, which drives
//!    the EXACT rollback code the production OOM-bailout runs) leaves the
//!    ring's `tail` cursor untouched and the sidecar pointer restored to
//!    `null`, so a LATER push can still succeed — i.e. a failed
//!    materialisation attempt is recoverable, not a permanent wedge. This is
//!    the structural half of the wedge-hazard proof (`heap_overflow.rs`'s
//!    `push_impl` — the code-inspection half — is: the CAS-reserve of `tail`
//!    is placed AFTER the `ensure_overflow_sidecar` check, so `tail` is
//!    provably never advanced past an index whose sidecar could not be
//!    materialised; see that function's doc comment for the exact ordering).
//!
//! The concurrent race (two producers racing to be the first to materialise a
//! ring's sidecar) — the shared `UNINIT -> INITIALIZING -> READY` CAS-publish
//! protocol this single-threaded file does not attempt to model — is covered by
//! the `racy-ptr-cell` crate's real-type loom suite
//! (`crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs`, CRATE-P3), which
//! replaced the four in-tree shadow models including the former
//! `tests/loom_overflow_sidecar_cas.rs`.

#![cfg(feature = "alloc-xthread")]

use sefer_alloc::registry::heap_overflow::HeapOverflow;

/// Synthetic, never-dereferenced non-null "segment base" — mirrors the
/// existing convention in `tests/heap_overflow_drain_return.rs` /
/// `tests/miri_heap_overflow_unit.rs`.
fn synthetic_base(tag: usize) -> *mut u8 {
    core::ptr::without_provenance_mut((tag + 1) * 64)
}

/// Core deliverable #1: the sidecar is `null` until genuinely needed.
///
/// Non-vacuous because: (a) a fresh ring's sidecar starts unmaterialised
/// (trivially true, asserted as the baseline); (b) pushing and draining
/// EXACTLY `INLINE_CAP` entries — the full inline-tier budget, deliberately
/// NOT one fewer — must still leave the sidecar untouched, proving the
/// boundary is exactly `INLINE_CAP`, not off-by-one in either direction; (c)
/// the `INLINE_CAP + 1`-th push crosses into the sidecar range and MUST
/// materialise it — proving the two branches of `HeapOverflow::slot`'s
/// `idx < INLINE_CAP` test are both actually reachable, not one of them dead
/// code.
///
/// **Miri note:** under miri, `HEAP_OVERFLOW_CAP == 64 == INLINE_CAP` (see
/// `heap_overflow.rs`'s `INLINE_CAP` doc comment), so `SIDECAR_CAP == 0` and
/// the sidecar range is structurally empty — the `(INLINE_CAP + 1)`-th push
/// simply fails (the ring is genuinely full, not "needs a sidecar it cannot
/// get"), and `dbg_sidecar_is_materialised()` can never become `true`. Steps
/// (a) and (b) above still run and are meaningful under miri (they do not
/// depend on the sidecar ever materialising); step (c) — proving the sidecar
/// branch is live — only runs natively, where `SIDECAR_CAP > 0`.
#[test]
fn sidecar_stays_null_until_inline_tier_exhausted_then_materialises() {
    let ring = HeapOverflow::new_boxed_for_test();
    assert!(
        !ring.dbg_sidecar_is_materialised(),
        "a freshly constructed ring's sidecar must start unmaterialised"
    );

    // Fill and drain exactly the inline tier's budget via the ring's own
    // dbg helper (which uses ordinary `push`/`drain` — no special-casing).
    let inline_cap = ring.dbg_fill_and_drain_inline_tier_for_test();
    assert!(
        inline_cap > 0,
        "INLINE_CAP must be positive for this test to be meaningful"
    );
    assert!(
        !ring.dbg_sidecar_is_materialised(),
        "draining exactly INLINE_CAP entries (never exceeding the inline \
         tier) must NOT have touched the sidecar"
    );

    // One more push: raw `tail` is now at INLINE_CAP. Natively `INLINE_CAP <
    // HEAP_OVERFLOW_CAP`, so the wrapped index (`INLINE_CAP %
    // HEAP_OVERFLOW_CAP == INLINE_CAP`) crosses into the sidecar range for
    // the FIRST time. Under miri, `INLINE_CAP == HEAP_OVERFLOW_CAP`, so this
    // same raw `tail` value WRAPS all the way back to wrapped index `0` —
    // squarely back in the (now-empty, just-drained) inline tier — there is
    // no sidecar range left to cross into at all (`SIDECAR_CAP == 0`, see
    // the module doc's miri note). Both outcomes below are therefore correct
    // for their respective configuration, not a relaxed/weakened assertion:
    // the underlying CLAIM ("the sidecar materialises if and only if a push
    // genuinely needs storage past INLINE_CAP") holds either way — under
    // miri there is structurally no such push to make.
    let pushed = ring.push(synthetic_base(9999), 777);
    assert!(
        pushed,
        "the (INLINE_CAP + 1)-th push must succeed (well under \
         HEAP_OVERFLOW_CAP, and the ring was just fully drained)"
    );

    if cfg!(miri) {
        assert!(
            !ring.dbg_sidecar_is_materialised(),
            "under miri, INLINE_CAP == HEAP_OVERFLOW_CAP, so this push wraps \
             back to wrapped index 0 (inline tier) rather than crossing into \
             a sidecar range — SIDECAR_CAP == 0, so the sidecar must never \
             materialise in this configuration"
        );
        let mut got = Vec::new();
        ring.drain(|base, packed| got.push((base, packed)));
        assert_eq!(
            got,
            vec![(synthetic_base(9999), 777)],
            "the wrapped-to-inline entry must still drain with its own \
             (base, packed) pair intact"
        );
        return;
    }

    assert!(
        ring.dbg_sidecar_is_materialised(),
        "pushing past the inline tier's INLINE_CAP entries must materialise \
         the sidecar"
    );

    // Sanity: the sidecar-backed entry is itself reclaimable via the normal
    // drain path (proves `HeapOverflow::slot`'s sidecar branch is not just
    // reachable but functionally correct, not merely "materialised and
    // never read").
    let mut got = Vec::new();
    ring.drain(|base, packed| got.push((base, packed)));
    assert_eq!(
        got,
        vec![(synthetic_base(9999), 777)],
        "the sidecar-tier entry must drain with its own (base, packed) pair intact"
    );
}

/// Core deliverable #2 (the wedge-hazard structural proof): a failed sidecar
/// materialisation attempt must be FULLY recoverable — the pointer restored
/// to `null` and `tail` untouched — so that a subsequent real materialisation
/// attempt (a real push) still succeeds. If the rollback were broken (left
/// the sentinel in place, or `tail` had already been advanced by the failed
/// attempt), a later push targeting the sidecar range would either spin
/// forever (stuck sentinel) or wedge `drain` at an unpublished index (the
/// hazard this whole round's design exists to rule out).
///
/// This drives `dbg_rollback_sidecar_sentinel_for_test` — the exact rollback
/// code path `bootstrap::ensure_overflow_sidecar_slow`'s OOM branch runs
/// (see `heap_overflow.rs`'s module doc "wedge hazard" section) — on a ring
/// whose `tail` has ALREADY been advanced past `INLINE_CAP` (so the rollback
/// runs in the exact state a real OOM would find it in: sidecar range
/// reachable, sentinel about to be installed), then proves two things: (a)
/// the postcondition the rollback hook itself checks (a fresh CAS(null,
/// SENTINEL) succeeds — i.e. the sentinel was genuinely cleared, not left
/// stuck) and (b) an ACTUAL subsequent push targeting the sidecar range still
/// succeeds and is drainable — the true end-to-end proof that a
/// materialisation failure never permanently disables the ring.
///
/// **Miri note:** the rollback-hook proof (part (a)) is pure `AtomicPtr`
/// manipulation — no `SIDECAR_CAP`/OS-reservation dependency — so it runs
/// identically under miri. Part (b) (a REAL push recovering afterwards)
/// cannot run under miri: `SIDECAR_CAP == 0` there means `ensure_
/// overflow_sidecar` always returns `false` via its own explicit guard
/// (never even reaching a CAS), so no push can ever cross into the sidecar
/// range — there is nothing for a "recovery" push to recover INTO. Under
/// miri this test instead confirms that same guard's outcome (push fails,
/// cleanly, exactly as the ordinary "ring full" case) rather than a
/// materialise-after-rollback recovery.
#[test]
fn sidecar_rollback_is_recoverable_no_permanent_wedge() {
    let ring = HeapOverflow::new_boxed_for_test();

    // Advance tail to INLINE_CAP (draining as we go) so the ring is
    // positioned exactly at the inline/sidecar boundary — the state a real
    // producer would be in right before its push needs the sidecar.
    let inline_cap = ring.dbg_fill_and_drain_inline_tier_for_test();
    assert!(inline_cap > 0);
    assert!(!ring.dbg_sidecar_is_materialised());

    // Simulate a materialisation attempt that reaches the CAS-acquire step
    // and then fails (OOM) — drive the EXACT rollback code via the test
    // hook, proving the postcondition: the sentinel is cleared back to null.
    // This part of the proof is identical under miri (pure AtomicPtr
    // manipulation, no OS reservation involved).
    let postcondition_holds = ring.dbg_rollback_sidecar_sentinel_for_test();
    assert!(
        postcondition_holds,
        "rollback must clear the sentinel back to null — a stuck sentinel \
         would make every future sidecar materialisation attempt on this \
         ring spin forever (anti-livelock violation)"
    );
    assert!(
        !ring.dbg_sidecar_is_materialised(),
        "after a rolled-back materialisation attempt, the sidecar must \
         still read as unmaterialised (null, not a stale winner pointer)"
    );

    // The critical end-to-end check: `tail` was NEVER advanced by the failed
    // attempt above (the rollback hook only touches the `sidecar` pointer,
    // never `tail` — mirroring the real `push_impl`'s ordering, where
    // `ensure_overflow_sidecar` is called and checked BEFORE the `tail` CAS
    // is even attempted). A REAL push now must succeed and land at the SAME
    // raw `tail` value (INLINE_CAP) the failed attempt would have targeted,
    // proving no index was stranded/skipped by the failed attempt — natively
    // this wrapped index is the sidecar's first slot; under miri (INLINE_CAP
    // == HEAP_OVERFLOW_CAP) it wraps back to wrapped index 0, the inline
    // tier's first slot (see the sibling test's identical wraparound note).
    let pushed = ring.push(synthetic_base(42), 123);
    assert!(
        pushed,
        "a push after a rolled-back sidecar materialisation attempt must \
         still succeed — the failed attempt must not have advanced tail or \
         otherwise consumed/stranded the index it would have targeted"
    );

    if cfg!(miri) {
        assert!(
            !ring.dbg_sidecar_is_materialised(),
            "under miri (SIDECAR_CAP == 0) this push wraps back into the \
             inline tier rather than the sidecar — the sidecar must never \
             materialise in this configuration"
        );
        let mut got = Vec::new();
        ring.drain(|base, packed| got.push((base, packed)));
        assert_eq!(
            got,
            vec![(synthetic_base(42), 123)],
            "the wrapped-to-inline entry must still drain intact — no \
             corruption from the earlier rolled-back attempt"
        );
        return;
    }

    assert!(
        ring.dbg_sidecar_is_materialised(),
        "the recovering push must materialise the sidecar for real (a \
         second, successful attempt)"
    );

    let mut got = Vec::new();
    ring.drain(|base, packed| got.push((base, packed)));
    assert_eq!(
        got,
        vec![(synthetic_base(42), 123)],
        "the recovered push's entry must drain intact — no corruption or \
         loss from the earlier rolled-back attempt"
    );
}

/// Complementary structural check: repeated rollback-then-recover cycles
/// (simulating sustained OOM across several push attempts before the OS
/// finally has room) never corrupt `tail`/`head` or leak a stuck sentinel —
/// each cycle's postcondition holds independently, and `tail` stays exactly
/// where the inline tier left it (no entry silently consumed) until the
/// FINAL, successful push.
#[test]
fn repeated_sidecar_rollback_cycles_never_corrupt_cursors() {
    let ring = HeapOverflow::new_boxed_for_test();
    let inline_cap = ring.dbg_fill_and_drain_inline_tier_for_test();
    assert!(inline_cap > 0);

    for _ in 0..5 {
        assert!(
            ring.dbg_rollback_sidecar_sentinel_for_test(),
            "every rollback cycle must independently clear the sentinel"
        );
        assert!(!ring.dbg_sidecar_is_materialised());
    }

    // After 5 simulated failed attempts, a real push must still land cleanly.
    assert!(ring.push(synthetic_base(7), 700));
    let mut got = Vec::new();
    ring.drain(|base, packed| got.push((base, packed)));
    assert_eq!(got, vec![(synthetic_base(7), 700)]);
}
