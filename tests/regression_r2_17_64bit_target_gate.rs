//! R2-17 (independent src review round 2, docs/reviews/2026-09-22-120730-src-review-xa-round-2.md §R2-17) —
//! automated oracle for the 64-bit-only allocator support gate
//! (`#[cfg(all(feature = "alloc-core", not(target_pointer_width = "64")))] compile_error!`)
//! at the root of `src/lib.rs`.
//!
//! ## What is pinned
//!
//! 1. An allocator-feature build (`--features production`) for a 32-bit
//!    target MUST fail to compile, and MUST fail with the gate's marker
//!    message. Before R2-17 the same command failed too, but only with two
//!    cryptic `error[E0080]: evaluation panicked: assertion failed:
//!    size_of::<SegmentHeader>() == 144` / `... offset_of!(PerClass, slots)
//!    == 8` diagnostics and no stated support boundary — the review's
//!    complaint was the *missing declaration*, not the missing failure.
//! 2. The region-only surface (`--no-default-features`, the `Region<T>`
//!    handle store, `no_std` + `alloc` only) MUST still compile on a 32-bit
//!    target. This is the review's explicit carve-out: "Region-only no_std
//!    поверхность отделена и этим утверждением не дисквалифицируется", and
//!    the R2-17 gate must not disown it.
//!
//! ## Why out-of-process
//!
//! Both properties are COMPILE-fail / compile-succeed properties of the
//! library itself, not of this test binary. A `#[test]` that fails to compile
//! never runs, so it cannot assert "this must not compile" — the assertion
//! has to be made by a process that compiles fine and then spawns
//! `cargo check` on the library as a CHILD process. Same rationale as
//! `tests/regression_r2_01_segment_bases_lifetime.rs` (and
//! `tests/tagged_index_stack_compile_fail.rs`), including the
//! `CARGO_TARGET_TMPDIR`-scoped child target dir, the `--offline` flag and
//! the `RUSTFLAGS` / `CARGO_ENCODED_RUSTFLAGS` stripping (an inherited
//! `--cfg`/`-D` must not make the child fail for the WRONG reason).
//!
//! ## Observed reality (recorded, not asserted)
//!
//! On `i686-pc-windows-msvc` the gated child currently emits the gate's
//! `compile_error!` AND the two pre-existing E0080 layout pins together
//! (the gate fires at crate root without short-circuiting the rest of the
//! crate's const-eval diagnostics). That is fine and expected — the pins are
//! deliberately unconditional (see the R2-17 notes in
//! `src/alloc_core/segment/segment_header/layout_asserts.rs` and
//! `src/registry/heap_core/state/tcache.rs`), so if the crate-root gate is
//! ever removed they keep failing loudly. Test 1 therefore asserts that the
//! build fails AND that the gate message is present, and deliberately does
//! NOT assert error-exclusivity (a stricter "only the gate fired" oracle
//! would be one edit away from breaking by design).
//!
//! ## Manual verification (machines without a 32-bit std target installed)
//!
//! ```text
//! rustup target add i686-pc-windows-msvc
//! cargo check --target i686-pc-windows-msvc --features production   # must FAIL with the gate message
//! cargo check --target i686-pc-windows-msvc                          # optional: succeeds (default features are region-only)
//! cargo check --target i686-pc-windows-msvc --no-default-features    # must SUCCEED (region-only surface)
//! ```
//!
//! ## Skip semantics
//!
//! Which targets are installed is machine-dependent. The helper
//! `first_installed_32bit_target` prefers `i686-pc-windows-msvc`, then
//! `i686-unknown-linux-gnu`, then `i686-unknown-linux-musl`. If `rustup` is
//! absent or reports no 32-bit std target, both tests SKIP with a printed
//! reason — a skip is NOT a pass, it is an honest "not verified on this
//! machine". CI must install at least one 32-bit target for these rows to
//! mean anything.

use std::path::PathBuf;
use std::process::Command;

/// First 32-bit std target installed on this machine, in preference order.
///
/// `None` when `rustup` is unavailable, the query fails, or no 32-bit std
/// target is installed — the caller's cue to skip with a printed reason
/// instead of failing.
fn first_installed_32bit_target() -> Option<&'static str> {
    const CANDIDATES: [&str; 3] = [
        "i686-pc-windows-msvc",
        "i686-unknown-linux-gnu",
        "i686-unknown-linux-musl",
    ];

    let rustup = match Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        // rustup missing / non-zero exit: no opinion, let the caller skip.
        _ => return None,
    };

    let stdout = String::from_utf8_lossy(&rustup.stdout);
    CANDIDATES
        .into_iter()
        .find(|candidate| stdout.split_whitespace().any(|word| word == *candidate))
}

/// `cargo check` against the manifest in this worktree, for one target.
///
/// `CARGO_TARGET_DIR` is nested under Cargo's per-test-binary `CARGO_TARGET_TMPDIR`
/// so the cross-compilation artifacts stay cacheable and isolated from the
/// host build dir; `RUSTFLAGS` / `CARGO_ENCODED_RUSTFLAGS` are stripped
/// (same rationale as `tests/regression_r2_01_segment_bases_lifetime.rs`)
/// and colour is off so the stderr assertions below stay literal.
fn check_args(target: &str, features_args: &[&str]) -> Command {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let child_target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("r2_17_64bit_gate");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let mut command = Command::new(&cargo);
    command
        .args(["check", "--offline"])
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--target")
        .arg(target)
        .args(features_args)
        .env("CARGO_TARGET_DIR", &child_target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env("CARGO_TERM_COLOR", "never");
    command
}

#[test]
fn unsupported_32bit_target_with_alloc_features_is_rejected_by_gate() {
    let Some(target) = first_installed_32bit_target() else {
        // Machine-dependent: no 32-bit std target installed, so this row
        // cannot be verified here. Install one (see the module doc) rather
        // than count this as a pass.
        eprintln!(
            "R2-17 gate oracle SKIPPED: no 32-bit std target installed \
             (looked for i686-pc-windows-msvc, i686-unknown-linux-gnu, \
             i686-unknown-linux-musl). Run `rustup target add \
             i686-pc-windows-msvc` to enable this check."
        );
        return;
    };

    let mut child = check_args(target, &["--features", "production"]);
    let output = child
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `cargo check --target {target}`: {e}"));
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "sefer-alloc COMPILED with `--features production` for the 32-bit \
         target {target} — either the R2-17 crate-root gate was removed, or \
         32-bit pointer widths stopped being rejected. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("sefer-alloc: allocator features require a 64-bit target"),
        "`cargo check --target {target} --features production` failed, but NOT \
         with the R2-17 gate message — the declared support boundary is no \
         longer what rejects 32-bit allocator builds (the E0080 layout pins \
         may be firing alone again). stderr:\n{stderr}"
    );
}

#[test]
fn unsupported_32bit_target_region_only_surface_still_compiles() {
    let Some(target) = first_installed_32bit_target() else {
        // Same machine-dependent skip as the gate test above.
        eprintln!(
            "R2-17 region-only oracle SKIPPED: no 32-bit std target installed \
             (looked for i686-pc-windows-msvc, i686-unknown-linux-gnu, \
             i686-unknown-linux-musl). Run `rustup target add \
             i686-pc-windows-msvc` to enable this check."
        );
        return;
    };

    let mut child = check_args(target, &["--no-default-features"]);
    let output = child
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `cargo check --target {target}`: {e}"));
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "the region-only surface (`--no-default-features`) no longer compiles \
         for the 32-bit target {target}: R2-17's carve-out for `Region<T>` on \
         32-bit regressed. stderr:\n{stderr}"
    );
}
