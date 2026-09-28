//! oxx R2-02 (`docs/reviews/2026-09-28-154558-src-review-oxx-round-2.md`):
//! `AllocCore::safe_payload_read_span` used to treat every Small/Primordial
//! segment as fully committed (`SEGMENT - off`), regardless of feature
//! config. Under `primordial-lazy-commit` (part of plain `production`) on
//! real Windows, only a PREFIX of the primordial segment is actually
//! committed at first (`Segment::reserve_lazy` +
//! `Layout::lazy_initial_commit`) — the rest is `MEM_RESERVE`-only. So a
//! bogus/oversized `old_layout.size()` that stayed under `SEGMENT` but
//! reached past the real commit frontier passed the old guard, and the
//! move leg's `Node::copy_nonoverlapping` then read straight into an
//! unmapped page: an access violation, not the null return
//! `AllocCore::realloc`'s (`unsafe fn`) defence-in-depth contract promises for
//! a caller-supplied bogus layout (see `tests/regression_realloc_oob_old_layout.rs`,
//! which covers the OLDER `old_layout.size() >= SEGMENT` case this bug did
//! NOT catch, since `frontier < SEGMENT` was always true).
//!
//! ## The fix
//!
//! `safe_payload_read_span` now bounds an OWN-SEGMENT Small/Primordial read
//! by the owner-only `SegmentMeta::committed_payload_end_of()` frontier
//! (same-thread read, no race — see that method's doc for the full
//! `own_segment` argument, including why the cross-heap FOREIGN leg keeps
//! the coarser `SEGMENT`-wide bound instead).
//!
//! ## Counterfactual (non-vacuity)
//!
//! RED (fix reverted, i.e. `safe_payload_read_span` ignores the frontier and
//! always returns `SEGMENT - off` for Small/Primordial): the child process
//! below crashes with an access violation instead of the `realloc` call
//! returning null. GREEN (fix present): the size-consistency check rejects
//! the bogus layout before any copy, `realloc` returns null, and the
//! original 16-byte block is left untouched.
//!
//! A real access-violation is fatal to the whole PROCESS (cannot be caught
//! by `catch_unwind`), so — like `tests/r1_04_alloc_core_drop_stack_pressure.rs`
//! — the scenario re-execs this test binary as a fresh child process; the
//! parent test only asserts on the child's exit status, so a pre-fix crash
//! shows up as a failed child exit instead of killing the whole test binary.
//!
//! Gated on `primordial-lazy-commit` specifically (not `alloc-lazy-commit`'s
//! `small-segment-lazy-commit` sibling): the scenario targets the PRIMORDIAL
//! segment, matching the review's own dynamic reproduction (§1.2 point 1,
//! standalone `AllocCore`). `numa-aware` forces the eager reservation path
//! even with `primordial-lazy-commit` on (see that feature's own doc
//! comment), and non-Windows/miri hosts commit the whole segment eagerly on
//! reserve (R8-5) — on both legs there is no lazy frontier to under-shoot,
//! so the scenario cannot be reproduced there; those legs assert the eager
//! invariant instead of silently skipping.

#![cfg(all(feature = "primordial-lazy-commit", feature = "internals"))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// The segment size constant (4 MiB).
const SEGMENT: usize = SegmentLayout::SEGMENT;

/// A bogus `old_layout`/`new_size` well above the primordial lazy-commit
/// frontier (`small_meta_end() + LAZY_FIRST_CHUNK`, 256 KiB, so at most a
/// few hundred KiB) but comfortably under `SEGMENT` (4 MiB) — the pre-fix
/// guard (`SEGMENT - off`) let this through. Also above the largest
/// possible `SMALL_MAX` across every shipped feature combination (up to
/// ~1.75 MiB under the extended `medium-classes` class list — see
/// `src/alloc_core/platform/size_classes.rs`'s module doc), so
/// `class_for` always classifies it `Large` regardless of `medium-classes`:
/// the OPT-F Small-same-class in-place fast path never intercepts this
/// call before it reaches the guard-checked move leg under test.
const BOGUS_SIZE: usize = 2 * 1024 * 1024;

// `BOGUS_SIZE` must stay under `SEGMENT` — otherwise this degenerates into
// the ALREADY-covered `tests/regression_realloc_oob_old_layout.rs` case
// (`old_layout.size() >= SEGMENT`), not the lazy-commit-specific R2-02 gap.
// Both sides are `const`, so this is checked at compile time.
const _: () = assert!(BOGUS_SIZE < SEGMENT);

const CASE_ENV: &str = "SEFER_OXX_R2_02_CHILD_RUN";

/// Only called from the real Windows-lazy leg's test below — gated
/// identically so `--all-features` (which turns `numa-aware` on) does not
/// flag this as dead code.
#[cfg(all(windows, not(feature = "numa-aware")))]
fn run_child() -> std::process::Output {
    let exe = std::env::current_exe().expect("test executable");
    // Cargo applies its target runner to this test binary, but not to a
    // subprocess it starts (same aarch64-runner forwarding as
    // `tests/r1_04_alloc_core_drop_stack_pressure.rs` /
    // `tests/regression_r2_08_global_allocator_path_no_unwind.rs`).
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    let mut command =
        if let Ok(runner) = std::env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER") {
            let mut words = runner.split_ascii_whitespace();
            let program = words.next().expect("nonempty aarch64 target runner");
            let mut command = std::process::Command::new(program);
            command.args(words).arg(&exe);
            command
        } else {
            std::process::Command::new(&exe)
        };
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    let mut command = std::process::Command::new(&exe);
    command
        .arg("--exact")
        .arg("oxx_r2_02_realloc_lazy_commit_frontier_child")
        .arg("--ignored")
        .arg("--nocapture")
        .env(CASE_ENV, "1")
        .output()
        .expect("run oxx-r2-02 child")
}

/// Real Windows-lazy leg: `primordial-lazy-commit` ON, `numa-aware` OFF,
/// genuine Windows (not miri) — the only configuration where the primordial
/// segment's commit frontier is actually below `SEGMENT` at the time of the
/// bogus `realloc` call, so this is the only leg that can reproduce R2-02.
#[cfg(all(windows, not(feature = "numa-aware")))]
#[test]
fn realloc_with_bogus_layout_past_lazy_commit_frontier_rejected_not_crashed() {
    let output = run_child();
    assert!(
        output.status.success(),
        "child must exit cleanly: the fix rejects a bogus `old_layout` that reaches \
         past the lazy-commit frontier with null, instead of reading past it and \
         faulting.\nstatus={}\nstdout={}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// `numa-aware` forces the EAGER primordial reservation even with
/// `primordial-lazy-commit` compiled in (see that feature's own doc comment
/// in `Cargo.toml`) — there is no lazy frontier to under-shoot on this leg,
/// so R2-02 cannot be reproduced here. Confirm the eager invariant instead
/// of silently skipping.
#[cfg(all(windows, feature = "numa-aware"))]
#[test]
fn primordial_frontier_is_eager_under_numa_aware() {
    let mut a = AllocCore::new().expect("AllocCore::new");
    let p = a.alloc(Layout::from_size_align(16, 16).unwrap());
    assert!(!p.is_null(), "setup: 16-byte primordial alloc failed");
    assert_eq!(
        a.dbg_committed_payload_end_for(p),
        Some(SEGMENT),
        "numa-aware must keep the primordial segment fully committed on reserve"
    );
    // SAFETY: `p` is a live 16-byte allocation made with this layout, freed
    // exactly once here.
    unsafe { a.dealloc(p, Layout::from_size_align(16, 16).unwrap()) };
}

/// Non-Windows hosts (and miri) commit the whole segment eagerly on reserve
/// regardless of `primordial-lazy-commit` (R8-5) — there is no lazy frontier
/// to under-shoot, so R2-02 cannot be reproduced there either. Confirm the
/// eager invariant instead of silently skipping.
#[cfg(not(windows))]
#[test]
fn primordial_frontier_is_eager_on_non_windows() {
    let mut a = AllocCore::new().expect("AllocCore::new");
    let p = a.alloc(Layout::from_size_align(16, 16).unwrap());
    assert!(!p.is_null(), "setup: 16-byte primordial alloc failed");
    assert_eq!(
        a.dbg_committed_payload_end_for(p),
        Some(SEGMENT),
        "non-Windows/miri must keep the primordial segment fully committed on reserve"
    );
    // SAFETY: `p` is a live 16-byte allocation made with this layout, freed
    // exactly once here.
    unsafe { a.dealloc(p, Layout::from_size_align(16, 16).unwrap()) };
}

#[test]
#[ignore = "run only in a fresh subprocess (see run_child)"]
fn oxx_r2_02_realloc_lazy_commit_frontier_child() {
    if std::env::var(CASE_ENV).is_err() {
        return;
    }
    let mut a = AllocCore::new().expect("AllocCore::new");
    let small = Layout::from_size_align(16, 16).unwrap();
    let p = a.alloc(small);
    assert!(!p.is_null(), "setup: 16-byte primordial alloc failed");
    // SAFETY: `p` is valid for 16 bytes per the alloc contract.
    unsafe { core::ptr::write_bytes(p, 0xA5, 16) };

    // Part 1 (task requirement): compare the guard's own source of truth —
    // the `committed_payload_end` frontier — against the bogus size we are
    // about to claim, proving BOTH that this leg genuinely has a
    // sub-SEGMENT frontier (the lazy path actually fired) and that
    // `BOGUS_SIZE` is a real lie relative to it (not accidentally a
    // legitimate size).
    let frontier = a
        .dbg_committed_payload_end_for(p)
        .expect("primordial segment must report a committed-frontier reading");
    assert!(
        frontier < SEGMENT,
        "test setup assumption violated: expected the real Windows-lazy leg to leave \
         the primordial segment partially uncommitted (frontier {frontier} < SEGMENT \
         {SEGMENT}) — if this fires, `primordial-lazy-commit` is not actually deferring \
         the commit on this host and the scenario cannot reproduce R2-02"
    );
    assert!(
        BOGUS_SIZE > frontier,
        "BOGUS_SIZE ({BOGUS_SIZE}) must lie past the real commit frontier ({frontier}) \
         for this to be a genuine R2-02 repro, not a legitimate size"
    );

    let bogus_old = Layout::from_size_align(BOGUS_SIZE, 16).unwrap();
    // SAFETY (R6-MS-1/2): `p` IS a live 16-byte allocation made with a
    // matching `old_layout` up to this point — `bogus_old`'s SIZE is
    // deliberately a lie (this is exactly the buggy-caller scenario R2-1/
    // R2-02 defend against under the `unsafe fn` contract's defence-in-depth
    // clause). The point of this test is that the allocator itself must not
    // turn that lie into an out-of-bounds read.
    let result = unsafe { a.realloc(p, bogus_old, BOGUS_SIZE) };
    assert!(
        result.is_null(),
        "realloc with old_layout ({BOGUS_SIZE}) past the lazy-commit frontier \
         ({frontier}) must return null (oxx R2-02 fix), not read past the committed span"
    );

    // Fix confirmed: a null return leaves `ptr` intact per the
    // `GlobalAlloc::realloc` contract.
    // SAFETY: `p` is still a live 16-byte allocation (the realloc above
    // returned null).
    unsafe {
        assert_eq!(
            core::ptr::read(p),
            0xA5,
            "original block disturbed by the rejected realloc"
        );
    }
    // SAFETY: `p` was returned by `alloc(small)` above, is still live, and
    // is freed exactly once here.
    unsafe { a.dealloc(p, small) };
}
