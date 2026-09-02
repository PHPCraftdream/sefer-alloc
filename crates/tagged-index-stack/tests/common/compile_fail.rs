//! Shared child-cargo mechanics for the crate's compile-fail drivers
//! (all seven tests now live in `tests/compile_fail.rs`).
//!
//! Every compile-fail test used to duplicate the same ~55 lines of
//! boilerplate: manifest-path resolution, the packaged-package skip guard,
//! the out-of-process `cargo build`, and the diagnostic context string.
//! This module is that boilerplate, stated once. The assertion logic —
//! which error codes and message substrings each fixture must produce —
//! stays in the individual tests in `tests/compile_fail.rs`.
//!
//! # Published-package behavior (shared by all seven fixtures)
//!
//! The fixture crates under `tests/compile_fail/` are git-checkout-only
//! test infrastructure: each has its own `Cargo.toml`, so cargo's
//! packaging rule auto-excludes them from the published `.crate` (now
//! stated explicitly via the `[package]` `exclude` in `Cargo.toml`). The
//! driver file itself IS packaged (a plain `.rs` file directly under
//! `tests/`, and so is this helper), so `cargo test` inside a downloaded
//! package reaches it — but not usefully as a compile-fail test: the
//! fixtures are simply not there. The skip fires ONLY in a packaged
//! context — detected via the `Cargo.toml.orig` `cargo package` writes
//! into every extracted package (absent from any git checkout) — so a
//! fixture that goes missing from a real checkout FAILS LOUD instead (a
//! bare `manifest.exists()` guard silently skipped the test in a checkout
//! whose fixture directory had been renamed away, reporting a false `ok`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Resolves the fixture manifest `tests/compile_fail/<fixture_dir>/Cargo.toml`
/// (public so callers can build their failure context from the same path).
pub fn fixture_manifest(fixture_dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join(fixture_dir)
        .join("Cargo.toml")
}

/// Builds the named compile-fail fixture in a child cargo process and
/// returns its output.
///
/// Returns `None` ONLY in the packaged-package skip case: the fixture
/// manifest is absent AND `Cargo.toml.orig` (written by `cargo package`
/// into every extracted package, never present in a git checkout) proves a
/// packaged context. In that case the caller just returns — the test is
/// skipped with a printed notice. If the manifest is absent in a real
/// checkout, this FAILS LOUD (a bad rename or accidental deletion must not
/// report a false `ok`).
///
/// `rustflags` selects the child `RUSTFLAGS` handling: `Some(f)` SETS
/// `RUSTFLAGS=f` (the loom-cfg fixture is the inverse case — the `--cfg
/// loom` configuration is the whole point); `None` REMOVES `RUSTFLAGS` so
/// an inherited `--cfg loom` cannot make a fixture fail for the WRONG
/// reason. Both cases `env_remove("CARGO_ENCODED_RUSTFLAGS")`: cargo
/// prefers the encoded variable over `RUSTFLAGS`, so a merely-empty-or-
/// inherited encoded value silently cancels either the strip (nothing to
/// cancel, but hygiene) or the override (an empty
/// `CARGO_ENCODED_RUSTFLAGS=""` was observed to suppress the RUSTFLAGS
/// override entirely while developing the loom-cfg test).
///
/// The child target dir is `CARGO_TARGET_TMPDIR/<fixture_dir>` — cached
/// across runs; target/tmp is gitignored.
pub fn build_fixture(fixture_dir: &str, rustflags: Option<&str>) -> Option<Output> {
    let manifest = fixture_manifest(fixture_dir);
    let packaged = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("Cargo.toml.orig")
        .exists();
    if !manifest.exists() {
        assert!(
            packaged,
            "compile-fail fixture missing from a git checkout: {}",
            manifest.display()
        );
        eprintln!(
            "skipping: compile-fail fixture not present ({}) — fixture \
             crates are git-checkout-only test infrastructure, excluded \
             from the published .crate",
            manifest.display()
        );
        return None;
    }

    let child_target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(fixture_dir);

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(&cargo);
    command
        .args(["build", "--offline", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &child_target);
    match rustflags {
        // The cfg-is-the-point case (loom fixture): SET the override.
        Some(flags) => {
            command.env("RUSTFLAGS", flags);
        }
        // The default case: STRIP the flags so an inherited `--cfg loom`
        // cannot mask the real failure.
        None => {
            command.env_remove("RUSTFLAGS");
        }
    }
    // Either way, the encoded variant must go or it silently wins over
    // `RUSTFLAGS` (see above).
    command.env_remove("CARGO_ENCODED_RUSTFLAGS");
    // CI's workflow-level CARGO_TERM_COLOR=always is inherited all the way
    // down to this child build; force plain-text rustc diagnostics so the
    // substring assertions in the callers match the same text in CI as
    // locally (same class as the earlier CI color bug fixed in fcae3ad
    // with --color=never).
    command.env("CARGO_TERM_COLOR", "never");
    Some(
        command
            .output()
            .expect("failed to spawn cargo for the compile-fail fixture"),
    )
}

/// The shared failure-context string every assertion message in
/// `tests/compile_fail.rs` embeds: fixture path, exit status, and both
/// output streams. Byte-identical format to the string the seven pre-
/// consolidation drivers each built by hand.
pub fn failure_context(manifest: &Path, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    format!(
        "fixture: {}\nstatus: {:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        manifest.display(),
        output.status.code()
    )
}
