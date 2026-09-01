//! Negative compile-fail regression, Group B of the storage-binding closure
//! (ADR `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`): a
//! storage impl whose hook bodies are CORRECT but whose declaration omits the
//! `unsafe` keyword must NOT compile. This pins Group B's actual mechanism —
//! the compiler-forced per-impl-site acknowledgment: `StackStorage` is an
//! `unsafe trait`, so no implementor can exist anywhere without asserting the
//! contract at the `unsafe impl` site. The asserted error is **E0200** ("the
//! trait `StackStorage<16>` requires an `unsafe impl` declaration").
//!
//! The compile-PASS counterpart — a correct `unsafe impl` compiles and
//! behaves correctly — is pinned by `vec_backed_storage_push_pop_round_trips`
//! and `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs`.
//! Counterfactually, this exact fixture COMPILED under the pre-conversion
//! safe trait, with no acknowledgment possible or required.
//!
//! The fixture's `Cargo.toml`, child-build mechanics, published-package skip
//! guard and RUSTFLAGS stripping are identical to
//! `tests/compile_fail_array_index_stack_head.rs` — see that driver's module
//! doc (and, through it, `tests/compile_fail_two_backings.rs`) for the full
//! rationale (why hand-rolled rather than `trybuild`, why not a doctest, why
//! an out-of-process cargo build, and why `RUSTFLAGS`/
//! `CARGO_ENCODED_RUSTFLAGS` must be stripped so the fixture fails for the
//! RIGHT reason). This driver does not repeat that prose; it differs only in
//! the asserted error code.
#![cfg(not(loom))]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn plain_impl_of_unsafe_stack_storage_must_not_compile() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join("unsafe_impl_required")
        .join("Cargo.toml");
    // Same packaged-skip guard as `tests/compile_fail_array_index_stack_head.rs`:
    // the fixture crates are excluded from the published .crate, so the skip fires ONLY where
    // `Cargo.toml.orig` proves a packaged context; in a real checkout a
    // missing fixture fails loud.
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
        return;
    }

    // Cached across runs; target/tmp is gitignored.
    let child_target =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("compile_fail_unsafe_impl_required");

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(&cargo)
        .args(["build", "--offline", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &child_target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        // Same plain-text rationale as the two_backings driver: CI's
        // CARGO_TERM_COLOR=always must not colorize the diagnostics the
        // substring assertions below match.
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to spawn cargo for the compile-fail fixture");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let context = format!(
        "fixture: {}\nstatus: {:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        manifest.display(),
        output.status.code()
    );

    assert!(
        !output.status.success(),
        "a plain (non-`unsafe`) impl of `StackStorage` COMPILED — the trait \
         stopped being `unsafe` (Group B reverted; the forced per-impl-site \
         acknowledgment regressed):\n{context}"
    );
    assert!(
        stderr.contains("E0200"),
        "expected E0200 (`unsafe impl` required) in the fixture's compile \
         errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("the trait `StackStorage<16>` requires an `unsafe impl` declaration"),
        "expected the exact E0200 wording `the trait `StackStorage<16>` \\
         requires an `unsafe impl` declaration`:\n{context}"
    );
    assert!(
        stderr.contains("PlainStorage"),
        "expected the error to name THIS impl site (`PlainStorage`), not \
         some unrelated error:\n{context}"
    );
}
