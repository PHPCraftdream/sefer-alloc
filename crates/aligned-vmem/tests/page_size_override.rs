//! Task #1085 (finding M1) — the page-size override's real-page FLOOR.
//!
//! `set_page_size_override(Some(ps))` historically validated `ps` only
//! against the compile-time floor (`PAGE` = 4 KiB, power of two) and never
//! against the machine's REAL page size — while `page_size_override`'s own
//! module docs claimed, unconditionally, that the override "can only make
//! validation STRICTER ... never accepts one the real page would reject".
//! On a host with real 16/64 KiB pages (macOS ARM64, aarch64-64k Linux) a
//! `Some(4096)` override made that claim FALSE: validators accept 4 KiB
//! multiples, and the OS rounds a decommit LENGTH up to the real page —
//! discarding live data outside the requested range through a 100%-safe
//! API. The fix: the setter now REJECTS any `ps` below the real OS page
//! size (a fresh, override-blind query), which makes the documented safety
//! claim true by construction.
//!
//! ## What this file can and cannot observe on a 4 KiB host
//!
//! The new rule's rejection set above the pre-existing `PAGE` floor is
//! `{ps : PAGE <= ps < real}` — EMPTY on any host whose real page is 4 KiB.
//! This test is therefore HOST-ADAPTIVE:
//!
//! - real > PAGE (16/64 KiB hosts): the discriminating arm runs — a
//!   power-of-two `ps` in `[PAGE, real)` MUST be rejected, and the
//!   rejection MUST leave the cached page size untouched.
//! - real == PAGE (the 4 KiB development host): that arm is structurally
//!   unreachable; the test prints the observed real page size and pins the
//!   unchanged boundaries instead (`Some(real)` stays accepted — an
//!   override equal to the real page is a harmless no-op; values below
//!   `PAGE` stay rejected by the pre-existing validation, which is NOT the
//!   task-#1085 delta).
//!
//! The natural run-both-ways counterfactual cannot be produced on a 4 KiB
//! host through the public API by any means: there is no injection point
//! for the OS page-size query (`GetSystemInfo`/`sysconf` are never
//! mock-gated — see `src/os/mod.rs`). Task #1085's recorded evidence run
//! therefore simulated a 16 KiB host by TEMPORARILY patching
//! `query_os_page_size` to return 16384: with the pre-fix setter the
//! discriminating arm FAILED (`Some(8192)` accepted); with the floor added
//! it PASSES. That patch is scaffolding for the evidence run only and is
//! NOT part of the committed change. No current CI row compiles the
//! override cfg on a >4 KiB-page runner (ci.yml wires it only into
//! `test-windows`), so the natural arm first goes live if a forced-page
//! row is ever added to the `test-macos` job (aarch64-apple-darwin, 16 KiB
//! real pages) — a ci.yml decision, not this crate's.
//!
//! ## Containment
//!
//! The override is process-global; per the established forced-page-file
//! discipline (tests/lazy_initial_commit_forced_page.rs,
//! tests/decomp_hooks_forced_page.rs) this binary contains exactly ONE
//! test so no sibling can race the override window, and a `Drop` guard
//! restores the real page size even on panic (`None` re-arms the
//! query-on-next-call cache sentinel).

#![cfg(aligned_vmem_page_size_override)]

use aligned_vmem::page_size;
use aligned_vmem::page_size_override::set_page_size_override;

/// The compile-time validation floor (`page::PAGE`), hardcoded so this test
/// cross-checks the VALUE rather than referencing the constant — same
/// convention as the root crate's forced-page test files.
const PAGE: usize = 4 * 1024;

/// Restores the real OS page size even if the test panics mid-observation:
/// `None` re-arms the query-on-next-call sentinel in the page-size cache.
struct RestoreRealPageSize;

impl Drop for RestoreRealPageSize {
    fn drop(&mut self) {
        set_page_size_override(None);
    }
}

/// The REAL OS page size with any override cleared and the cache re-queried.
fn real_page() -> usize {
    set_page_size_override(None);
    page_size()
}

#[test]
fn override_floor_is_the_real_os_page_size() {
    // ONE test on purpose: the override is process-global (see module docs).
    let _restore = RestoreRealPageSize;

    let real = real_page();
    eprintln!("[task-1085] real OS page size observed on this host: {real}");
    assert!(
        real.is_power_of_two() && real >= PAGE,
        "the real page size ({real}) must be a power of two >= PAGE ({PAGE})"
    );

    // (a) THE rule, host-adaptive. The rejection set above the PAGE floor
    // is [PAGE, real); a power-of-two candidate exists in it iff
    // real > PAGE (for a power-of-two real, real/2 is the largest
    // power of two strictly below it).
    let candidate = real / 2;
    if real > PAGE && candidate >= PAGE && candidate.is_power_of_two() {
        eprintln!(
            "[task-1085] discriminating arm LIVE: Some({candidate}) must be rejected \
             (real = {real})"
        );
        assert!(
            !set_page_size_override(Some(candidate)),
            "Some({candidate}) is below the real page size ({real}) but was ACCEPTED: the \
             override would loosen every page-multiple validator below the real page and \
             let the OS round decommit lengths up across live data outside the requested \
             range (task #1085 / finding M1)"
        );
        // A rejected override must leave the cache untouched: the cached
        // value stays the real page, not the rejected candidate.
        assert_eq!(
            page_size(),
            real,
            "a rejected override must leave the cached page size at the real value ({real})"
        );
    } else {
        eprintln!(
            "[task-1085] discriminating arm SKIPPED (real == PAGE == {PAGE}): the \
             task-#1085 rejection set above the PAGE floor is empty on this host — see \
             the module docs for where this arm actually runs and how the counterfactual \
             was recorded"
        );
        // NOT the task-#1085 delta — both assertions hold with and without
        // the fix. They pin the boundary so the new rule cannot over-reject:
        assert!(
            set_page_size_override(Some(real)),
            "an override EQUAL to the real page size ({real}) is a no-op and must stay \
             accepted"
        );
        assert_eq!(
            page_size(),
            real,
            "the accepted equal-to-real override must take effect (as a no-op)"
        );
        assert!(
            !set_page_size_override(Some(PAGE / 2)),
            "Some({}) is below the pre-existing PAGE floor and must stay rejected",
            PAGE / 2
        );
    }

    // (b) The floor is the REAL page, NOT the previously-armed override.
    // The forced-page suites' actual shape (task #1096 corrected this
    // comment, which claimed they "arm 64 KiB, then downshift to 16 KiB
    // with no restore in between"): every root-crate suite that arms the
    // override UPSHIFTS or arms a single value, and constructs its
    // restore guard INSIDE the loop — `decomp_hooks_forced_page.rs` and
    // `segment_state_reconciliation_oracle.rs` iterate [16 KiB, 64 KiB]
    // with a per-iteration restore; `lazy_initial_commit_forced_page.rs`
    // arms one 64 KiB. No existing suite downshifts; THIS loop (64 KiB
    // then 16 KiB, no restore between iterations) is itself the only
    // downshift, and the smaller value must still be accepted whenever it
    // is >= the real page — 16 KiB over a 4 KiB host is a legal override.
    // That pins the rule against the wrong variant "monotonic vs the
    // current cache value", which would reject every legal downshift. The
    // design is right on its own merits regardless of the suites' shape:
    // comparing against the armed value instead of a fresh real-page query
    // is what would be wrong.
    set_page_size_override(None);
    for &forced in &[64 * 1024, 16 * 1024] {
        if forced < real {
            continue;
        }
        assert!(
            set_page_size_override(Some(forced)),
            "Some({forced}) is a power of two >= the real page ({real}) and must be \
             accepted — the root crate's forced-page suites depend on exactly this shape"
        );
        assert_eq!(
            page_size(),
            forced,
            "an accepted override must be visible through page_size()"
        );
    }

    // (c) Restoration contract: `None` re-arms the query sentinel and the
    // next `page_size()` call reports the real page again.
    assert!(set_page_size_override(None));
    assert_eq!(
        page_size(),
        real,
        "None must restore the real page size ({real})"
    );
}
