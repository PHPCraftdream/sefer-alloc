//! Negative compile-fail regression, Group A of the storage-binding closure
//! (ADR `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`): a
//! competing binding built around a standalone [`ArrayIndexStack`]'s head must
//! NOT compile. This driver is the REPLACEMENT of the deleted runtime test
//! `array_index_stack_head_still_double_issue` in
//! `tests/custom_storage_impl.rs`: that test extracted `&StackHead` off the
//! owned type via the then-public `StackStorage` impl and demonstrated a real
//! double-issue; Group A removes the type's public trait impl in favor of a
//! `pub(crate)`-sealed accessor, so the extraction route is now UNEXPRESSIBLE
//! and the runtime demonstration becomes a compile-fail oracle pinned HERE.
//!
//! The fixture at `tests/compile_fail/array_index_stack_head/src/main.rs` tries
//! both routes: a generic `fn steal_head<S: StackStorage<16>>` (must fail with
//! **E0277**, the trait bound `ArrayIndexStack<16, 64>: StackStorage<16>` not
//! satisfied), a direct `owned.head()` method call (must fail with **E0599**,
//! `no method named 'head'`), and a `&dyn StackStorage` coercion (must fail
//! with **E0277**, the same unsatisfied bound). Those compile errors ARE the
//! structural fix itself, not a runtime panic.
//!
//! # Why the "competing binding" angle needs no separate fixture
//!
//! A competing binding (a second `StackOps`-callable value sharing this
//! head) has exactly ONE prerequisite expressible against a shipped
//! `ArrayIndexStack`: obtaining its `&StackHead`. Every route to that
//! reference requires the public `StackStorage` impl Group A removed — the
//! generic-bound route (E0277), the inherent-method route (E0599), and the
//! `&dyn StackStorage` coercion route (E0277, third statement in the
//! fixture's `main`) — and the fixture asserts all of them fail. The
//! downstream construction (pairing a stolen head with fresh links under a
//! custom impl) adds no independent signal: its only live ingredient IS the
//! stolen head, which already fails to compile here. A competing binding
//! that does NOT involve this type — own a `StackHead`, hand it to two
//! custom `unsafe impl` values — remains expressible by Group B's deliberate
//! design and is pinned at runtime by
//! `two_implementor_values_sharing_one_head_still_double_issue` in
//! `tests/custom_storage_impl.rs`.
//!
//! # The seal does not rest on this fixture alone
//!
//! This fixture pins ONE instantiation (`<16, 64>`), but the guarantee is
//! instantiation-independent and held by COHERENCE: any in-crate
//! `impl StackStorage<B> for ArrayIndexStack<B, N>` fails with **E0119**
//! (it would overlap the `pub(crate)` `SealedStorage` blanket), and any
//! out-of-crate attempt fails with **E0117** (orphan rule).
//!
//! The fixture's `Cargo.toml`, child-build mechanics, published-package skip
//! guard and RUSTFLAGS stripping are identical to
//! `tests/compile_fail_two_backings.rs` — see that driver's module doc for the
//! full rationale (why hand-rolled rather than `trybuild`, why not a doctest,
//! why an out-of-process cargo build, and why `RUSTFLAGS`/`CARGO_ENCODED_RUSTFLAGS`
//! must be stripped so the fixture fails for the RIGHT reason). This driver
//! does not repeat that prose; it differs only in the asserted error codes.
#![cfg(not(loom))]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn competing_binding_around_array_index_stack_head_must_not_compile() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join("array_index_stack_head")
        .join("Cargo.toml");
    // Same packaged-skip guard as `tests/compile_fail_two_backings.rs`:
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
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("compile_fail_array_index_stack_head");

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
        "the Group A head-extraction repro COMPILED — `ArrayIndexStack` must \
         NOT implement the public `StackStorage` trait (the sealing regressed):\n{context}"
    );
    assert!(
        stderr.contains("E0277"),
        "expected E0277 (unsatisfied `StackStorage` bound) in the fixture's \
         compile errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains(
            "the trait bound `ArrayIndexStack<16, 64>: StackStorage<16>` is not satisfied"
        ),
        "expected the exact E0277 wording `the trait bound \\
         `ArrayIndexStack<16, 64>: StackStorage<16>` is not satisfied`:\n{context}"
    );
    assert!(
        stderr.contains("E0599"),
        "expected E0599 (no method named `head`) in the fixture's compile \
         errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("no method named `head`"),
        "expected `no method named `head`` on ArrayIndexStack in the \
         fixture's compile errors:\n{context}"
    );
}
