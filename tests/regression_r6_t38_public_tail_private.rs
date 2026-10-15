//! R6-T38 review fix — automated oracle for the compile-fail fixture at
//! `tests/compile_fail/r6_t38_public_tail_private/`.
//!
//! A compile-FAIL property cannot be asserted by a normal `#[test]`: a test
//! binary that fails to compile never runs, so the assertion "this code must
//! not compile" has to be made from a process that compiles fine and then
//! invokes `cargo build` on the fixture as a CHILD process — the same
//! out-of-process technique `tests/regression_r2_01_segment_bases_lifetime.rs`
//! established (see that file's own module doc for the full rationale).
//!
//! ## Why a standalone fixture crate, not a root `[[example]]`
//!
//! As with the R2-01 sibling fixture, this probe fails UNCONDITIONALLY once
//! `alloc-global` is enabled: there is no feature combination under which it
//! compiles. A standalone fixture crate under `tests/compile_fail/` — carrying
//! its own empty `[workspace]` table so Cargo's workspace/target discovery
//! never reaches it — is never part of `sefer-alloc`'s own target set, so no
//! `--all-targets`/`cargo test` invocation on the root crate ever touches it.
//!
//! This test pins the POST-R6-T38 state: both crate-tail methods
//! (`AllocCore::segment_bases` and `RouteDirectory::register`) must be
//! externally UNNAMEABLE — the fixture must fail with at least two **E0624**
//! diagnostics ("method `segment_bases` is private" / "method `register` is
//! private"), not succeed and not fail for an unrelated reason (E0599
//! no-method, E0433 unresolvable path, ...).

use std::path::PathBuf;
use std::process::Command;

fn fixture_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join("r6_t38_public_tail_private")
        .join("Cargo.toml")
}

#[test]
fn crate_tail_methods_not_externally_callable() {
    let manifest = fixture_manifest();
    assert!(
        manifest.is_file(),
        "compile-fail fixture missing from checkout: {}",
        manifest.display()
    );

    // Isolated child target dir (CARGO_TARGET_TMPDIR is a Cargo-provided,
    // per-test-binary temp dir — cached across runs, gitignored), mirroring
    // the established R2-01 runner pattern. RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS
    // stripped so an inherited `--cfg loom` (or similar) cannot make this
    // fixture fail for the WRONG reason.
    let child_target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("r6_t38_fixture");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(&cargo)
        .args(["build", "--offline", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &child_target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to spawn `cargo build` for the R6-T38 fixture");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "the R6-T38 fixture COMPILED — a crate-tail method is still \
         externally callable (`AllocCore::segment_bases` or \
         `RouteDirectory::register` leaked past `pub(crate)`). stderr:\n{stderr}"
    );
    assert!(
        stderr.matches("E0624").count() >= 2,
        "the R6-T38 fixture failed to compile, but NOT with two E0624 \
         (\"method is private\") diagnostics — it may be failing for an \
         unrelated reason (E0599 no-method, E0433 unresolvable path, ...). \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("method `segment_bases` is private"),
        "expected a `method `segment_bases` is private` E0624 diagnostic. \
         stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("method `register` is private"),
        "expected a `method `register` is private` E0624 diagnostic. \
         stderr:\n{stderr}"
    );
}
