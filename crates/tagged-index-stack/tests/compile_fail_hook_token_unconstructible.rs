//! Negative compile-fail regression, audit finding P2-1 closure (hook-witness
//! unconstructibility): the three `StackStorage` hooks each take a first
//! `_: &Hook` witness parameter and `Hook` is `pub struct Hook(())` — public
//! type, PRIVATE field — so no code outside this crate can obtain a witness.
//! A correct downstream implementor must therefore be unable to call
//! `head`/`load_next`/`store_next` from its own safe code. This pins BOTH
//! forgery routes in one fixture build:
//!
//! - route (a), the bare call `pool.store_next(1, 3)`, omitted witness:
//!   **E0061** ("this method takes 3 arguments but 2 arguments were supplied",
//!   with "argument #1 of type `&Hook` is missing");
//! - route (b), the tuple-struct forgery `pool.store_next(&Hook(()), 1, 3)`:
//!   **E0423** ("cannot initialize a tuple struct which contains private
//!   fields"). The struct-literal spelling `Hook { 0: () }` is the OTHER
//!   private-field code, **E0451** ("field `0` of struct `Hook` is private")
//!   — verified in a throwaway variant, not asserted here because main.rs
//!   pins only the tuple-struct spelling.
//!
//! Together these reproduce, as a compile-fail oracle, the audit run-5 attack
//! (its attempt A4, `p.store_next(1, 3)`, spliced a cycle and double-issued)
//! that this closure makes UNEXPRESSIBLE. Note `&Hook` (a reference, not an
//! owned token) is load-bearing: an owned non-Copy token could be stashed by
//! a cooperating implementor into a `Cell<Option<Hook>>` and re-exposed
//! through the implementor's own safe method; the reference form makes that a
//! lifetime error. Full rationale: the audit report + the repository ADR
//! (both repository files, not published).
//!
//! The fixture's `Cargo.toml`, child-build mechanics, published-package skip
//! guard and RUSTFLAGS stripping are identical to
//! `tests/compile_fail_unsafe_impl_required.rs` — see that driver's module
//! doc (and, through it, `tests/compile_fail_array_index_stack_head.rs`) for
//! the full rationale (why hand-rolled rather than `trybuild`, why not a
//! doctest, why an out-of-process cargo build, and why `RUSTFLAGS`/
//! `CARGO_ENCODED_RUSTFLAGS` must be stripped so the fixture fails for the
//! RIGHT reason). This driver does not repeat that prose; it differs only in
//! the asserted error codes and message fragments.
#![cfg(not(loom))]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn hook_witness_is_unconstructible_outside_the_crate() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join("hook_token_unconstructible")
        .join("Cargo.toml");
    // Same packaged-skip guard as `tests/compile_fail_unsafe_impl_required.rs`:
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
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("compile_fail_hook_token_unconstructible");

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(&cargo)
        .args(["build", "--offline", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &child_target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        // Same plain-text rationale as the unsafe_impl_required driver: CI's
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
        "a crate calling `store_next` with either the witness omitted or a \
         tuple-struct-forged `Hook` COMPILED — the witness became \
         constructible outside the crate (P2-1 reopened; hook forgery \
         against downstream implementors is expressible again):\n{context}"
    );
    // Route (a): omitted witness.
    assert!(
        stderr.contains("E0061"),
        "expected E0061 (missing `&Hook` argument) in the fixture's compile \
         errors — the arity guard regressed or the build failed for some \
         OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("this method takes 3 arguments but 2 arguments were supplied"),
        "expected the exact E0061 wording `this method takes 3 arguments but \
         2 arguments were supplied`:\n{context}"
    );
    // Route (b): forged witness.
    assert!(
        stderr.contains("E0423"),
        "expected E0423 (tuple struct with private fields) in the fixture's \
         compile errors — the witness became constructible outside the crate \
         or the build failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("cannot initialize a tuple struct which contains private fields"),
        "expected the exact E0423 wording `cannot initialize a tuple struct \
         which contains private fields`:\n{context}"
    );
    // Both routes attack the same hook.
    assert!(
        stderr.contains("store_next"),
        "expected the errors to name `store_next`, not some unrelated error:\n{context}"
    );
}
