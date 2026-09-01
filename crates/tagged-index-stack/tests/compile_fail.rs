//! Consolidated compile-fail driver for `tagged-index-stack`: seven
//! negative-regression tests, each building a deliberately-broken fixture
//! crate under `tests/compile_fail/<fixture>/` in an out-of-process
//! `cargo build` and asserting it fails with SPECIFIC errors. The shared
//! child-cargo mechanics (manifest resolution, packaged-package skip
//! guard, spawn, diagnostic context string) live in
//! `tests/common/compile_fail.rs`; each `#[test]` below keeps only its own
//! assertions and its fixture-specific rationale.
//!
//! # Why hand-rolled and not `trybuild`
//!
//! This workspace's standing convention is to decline a `trybuild`
//! dev-dependency in favor of hand-rolled compile-fail tests, each decision
//! documented in-source where it was made — do not trust a count quoted in
//! any one comment (the notes accumulate over time): the accurate,
//! mechanically re-derivable way to find them is
//! `grep -rn trybuild --include=*.rs .` from the workspace root. This crate
//! follows the established alternative (see root
//! `tests/r34_3_internals_boundary_api.rs` and root
//! `examples/sol_f1_dbg_carve_batch_negative_probe.rs` plus root
//! `scripts/verify-internals-negative-boundary.mjs` — repository files,
//! not part of the published package): an out-of-process `cargo build` of a
//! deliberately-broken fixture, asserted to FAIL. This crate's own
//! `tests/stack_unit.rs` documents the compile-fail coverage that now
//! EXISTS (the tests in this file) rather than a decline.
//!
//! # Why not a `compile_fail` doctest
//!
//! This repo bans doctests outright (see CLAUDE.md, "No doctests").
//!
//! # Why an out-of-process cargo build
//!
//! A compile-FAIL property cannot be asserted by a normal `#[test]`: a test
//! binary that fails to compile never runs, so the assertion "this code must
//! not compile" has to be made from a process that compiles fine and then
//! invokes rustc/cargo on the fixture as a CHILD process — the same analysis
//! root `tests/r34_3_internals_boundary_api.rs` itself documents (a
//! repository file, not part of the published package).
//!
//! # Published-package behavior
//!
//! The fixture crates under `tests/compile_fail/` are git-checkout-only
//! test infrastructure: each has its own `Cargo.toml`, so cargo's
//! packaging rule auto-excludes them from the published `.crate` (now
//! stated explicitly via the `[package]` `exclude` in `Cargo.toml`). This
//! driver file itself IS packaged (a plain `.rs` file directly under
//! `tests/`), so `cargo test` inside a downloaded package reaches it —
//! but not usefully as a compile-fail test: the fixtures are simply not
//! there. The skip fires ONLY in a packaged context — detected via the
//! `Cargo.toml.orig` `cargo package` writes into every extracted package
//! (absent from any git checkout) — so a fixture that goes missing from a
//! real checkout FAILS LOUD instead (a bare
//! `manifest.exists()` guard silently skipped the test in a checkout whose
//! fixture directory had been renamed away, reporting a false `ok`).
//!
//! # RUSTFLAGS stripping
//!
//! Under `RUSTFLAGS="--cfg loom" cargo test`, an inherited `--cfg loom` would
//! make a fixture fail for the WRONG reason: the crate's own `--cfg
//! loom`-without-feature `compile_error!` (see `src/lib.rs`) fires before
//! any method resolution or bounds check, so assertions would pass even if
//! the regression under test had resurfaced. For six of the seven fixtures
//! the child env therefore REMOVES `RUSTFLAGS` (and
//! `CARGO_ENCODED_RUSTFLAGS`, which cargo prefers) so the fixture fails for
//! the RIGHT reason. The seventh — the loom-cfg fixture below — is the
//! INVERSE: the `--cfg loom` configuration is the whole point, so its child
//! env SETS `RUSTFLAGS` to the literal `--cfg loom` (still removing
//! `CARGO_ENCODED_RUSTFLAGS`).
#![cfg(not(loom))]

mod common;

use common::compile_fail::{build_fixture, failure_context, fixture_manifest};

/// API-REMOVAL regression (Group C): the pre-redesign API's minimal
/// two-`ArrayLinks`-backings + one-`StackHead` repro must NOT compile against
/// the post-redesign API — because the OLD unsafe-free-form API it used no
/// longer EXISTS. This is an API-removal regression test, NOT a
/// safety-invariant proof: it pins that `StackHead` (the head word alone) has
/// no external `push(&links, idx)` / `pop(&links)` methods and no
/// caller-supplied-backing calling convention. A fixture that compiled would
/// mean that old API resurfaced.
///
/// The hazard CLASS itself — one head, two backings — is NOT closed by this
/// test and is NOT closed by the type system: since the 2026-09-01 `unsafe
/// trait` conversion (`unsafe trait StackStorage`) it is re-expressible
/// through a custom `unsafe impl` that asserts the trait's `# Safety`
/// contract and then violates it (inventory shape 2; pinned at runtime by
/// `two_implementor_values_sharing_one_head_still_double_issue` in
/// `tests/custom_storage_impl.rs`). The structural closure that DOES exist —
/// no route from a shipped [`ArrayIndexStack`] to a `&StackHead`, so no
/// competing binding around its head — is proven by the Group A compile-fail
/// oracle `competing_binding_around_array_index_stack_head_must_not_compile`
/// below (fixture under `tests/compile_fail/array_index_stack_head/`); go
/// THERE for the real safety-invariant proof. (Earlier drafts of this doc
/// called the E0599 here "the structural fix itself" and the repro
/// "UNEXPRESSIBLE" — an overclaim that misled later rounds of this campaign's
/// own review history; see
/// `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md` for
/// what was actually decided and why.)
///
/// The fixture at `tests/compile_fail/two_backings/src/main.rs` is exactly
/// the old repro, adapted to the post-redesign type names: two independent
/// `ArrayLinks` backings (`a`, `b`) plus one `StackHead<16>`, with the
/// stack's `push`/`pop` called externally against each. It must fail with
/// **E0599** ("no method named `push` / `pop` found for struct
/// `StackHead<16>`").
#[test]
fn two_arraylinks_backings_against_one_stackhead_must_not_compile() {
    let Some(output) = build_fixture("two_backings", None) else {
        return; // packaged package: fixtures absent, skip.
    };
    let manifest = fixture_manifest("two_backings");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "the P1-1 two-backings repro COMPILED — the old per-call API (external \
         push/pop with a caller-supplied backing) has resurfaced; `StackHead` \
         must have no push/pop:\n{context}"
    );
    assert!(
        stderr.contains("E0599"),
        "expected E0599 (no method named `push`/`pop`) in the fixture's \
         compile errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("no method named `push` "),
        "expected `no method named `push`` on StackHead in the fixture's \
         compile errors:\n{context}"
    );
    assert!(
        stderr.contains("no method named `pop` "),
        "expected `no method named `pop`` on StackHead in the fixture's \
         compile errors:\n{context}"
    );
}

/// Negative compile-fail regression, Group A of the storage-binding closure
/// (ADR `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`): a
/// competing binding built around a standalone [`ArrayIndexStack`]'s head must
/// NOT compile. This test is the REPLACEMENT of the deleted runtime test
/// `array_index_stack_head_still_double_issue` in
/// `tests/custom_storage_impl.rs`: that test extracted `&StackHead` off the
/// owned type via the then-public `StackStorage` impl and demonstrated a real
/// double-issue; Group A removes the type's public trait impl in favor of a
/// `pub(crate)`-sealed accessor, so the extraction route is now UNEXPRESSIBLE
/// and the runtime demonstration becomes a compile-fail oracle pinned HERE.
///
/// The fixture at `tests/compile_fail/array_index_stack_head/src/main.rs` tries
/// both routes: a generic `fn steal_head<S: StackStorage<16>>` (must fail with
/// **E0277**, the trait bound `ArrayIndexStack<16, 64>: StackStorage<16>` not
/// satisfied), a direct `owned.head()` method call (must fail with **E0599**,
/// `no method named 'head'`), and a `&dyn StackStorage` coercion (must fail
/// with **E0277**, the same unsatisfied bound). Those compile errors ARE the
/// structural fix itself, not a runtime panic.
///
/// # Why the "competing binding" angle needs no separate fixture
///
/// A competing binding (a second `StackOps`-callable value sharing this
/// head) has exactly ONE prerequisite expressible against a shipped
/// `ArrayIndexStack`: obtaining its `&StackHead`. Every route to that
/// reference requires the public `StackStorage` impl Group A removed — the
/// generic-bound route (E0277), the inherent-method route (E0599), and the
/// `&dyn StackStorage` coercion route (E0277, third statement in the
/// fixture's `main`) — and the fixture asserts all of them fail. The
/// downstream construction (pairing a stolen head with fresh links under a
/// custom impl) adds no independent signal: its only live ingredient IS the
/// stolen head, which already fails to compile here. A competing binding
/// that does NOT involve this type — own a `StackHead`, hand it to two
/// custom `unsafe impl` values — remains expressible by Group B's deliberate
/// design and is pinned at runtime by
/// `two_implementor_values_sharing_one_head_still_double_issue` in
/// `tests/custom_storage_impl.rs`.
///
/// # The seal does not rest on this fixture alone
///
/// This fixture pins ONE instantiation (`<16, 64>`), but the guarantee is
/// instantiation-independent and held by COHERENCE: any in-crate
/// `impl StackStorage<B> for ArrayIndexStack<B, N>` fails with **E0119**
/// (it would overlap the `pub(crate)` `SealedStorage` blanket), and any
/// out-of-crate attempt fails with **E0117** (orphan rule).
#[test]
fn competing_binding_around_array_index_stack_head_must_not_compile() {
    let Some(output) = build_fixture("array_index_stack_head", None) else {
        return; // packaged package: fixtures absent, skip.
    };
    let manifest = fixture_manifest("array_index_stack_head");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

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

/// Negative compile-fail regression, audit finding P2-1 closure (hook-witness
/// unconstructibility): the three `StackStorage` hooks each take a first
/// `_: &Hook` witness parameter and `Hook` is `pub struct Hook(())` — public
/// type, PRIVATE field — so no code outside this crate can obtain a witness.
/// A correct downstream implementor must therefore be unable to call
/// `head`/`load_next`/`store_next` from its own safe code. This pins BOTH
/// forgery routes in one fixture build:
///
/// - route (a), the bare call `pool.store_next(1, 3)`, omitted witness:
///   **E0061** ("this method takes 3 arguments but 2 arguments were supplied",
///   with "argument #1 of type `&Hook` is missing");
/// - route (b), the tuple-struct forgery `pool.store_next(&Hook(()), 1, 3)`:
///   **E0423** ("cannot initialize a tuple struct which contains private
///   fields"). The struct-literal spelling `Hook { 0: () }` is the OTHER
///   private-field code, **E0451** ("field `0` of struct `Hook` is private")
///   — verified in a throwaway variant, not asserted here because main.rs
///   pins only the tuple-struct spelling.
///
/// Together these reproduce, as a compile-fail oracle, the audit run-5 attack
/// (its attempt A4, `p.store_next(1, 3)`, spliced a cycle and double-issued)
/// that this closure makes UNEXPRESSIBLE. Note `&Hook` (a reference, not an
/// owned token) is load-bearing: an owned non-Copy token could be stashed by
/// a cooperating implementor into a `Cell<Option<Hook>>` and re-exposed
/// through the implementor's own safe method; the reference form makes that a
/// lifetime error. Full rationale: the audit report + the repository ADR
/// (both repository files, not published).
#[test]
fn hook_witness_is_unconstructible_outside_the_crate() {
    let Some(output) = build_fixture("hook_token_unconstructible", None) else {
        return; // packaged package: fixtures absent, skip.
    };
    let manifest = fixture_manifest("hook_token_unconstructible");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

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

/// Negative compile-fail regression: `TaggedIndex`'s `INDEX_BITS` const-generic
/// bounds must stay enforced — a `TaggedIndex<0>` and a `TaggedIndex<17>`
/// must NOT compile.
///
/// The fixtures at `tests/compile_fail/index_bits_zero/src/main.rs` and
/// `tests/compile_fail/index_bits_seventeen/src/main.rs` each read the
/// associated constant of an out-of-range instantiation (`INDEX_BITS` of 0
/// and 17 respectively). Both must fail with **E0080** carrying the crate's
/// own assert message: `INDEX_BITS must be in 1..=16: the tag half must keep
/// at least 48 bits ...` (the `_CHECK_BITS` assert's minimum-48-bit-tag /
/// sentinel argument).
///
/// # Why the assertion names the range requirement
///
/// Asserting only "the build failed" would be vacuous: the fixture could
/// fail for an UNRELATED reason (a syntax error, a dependency issue, an
/// inherited `--cfg loom` firing the crate's loom-cfg `compile_error!`)
/// and the test would still pass. Naming `E0080` and the exact range
/// message pins that the failure is the bounds check itself.
///
/// Shared helper for the two index-bits fixtures (the in-file precedent
/// that motivated `tests/common/compile_fail.rs` itself): builds the named
/// fixture and asserts it fails with the `_CHECK_BITS` E0080 range
/// requirement.
fn assert_index_bits_fixture_must_not_compile(fixture_dir: &str) {
    let Some(output) = build_fixture(fixture_dir, None) else {
        return; // packaged package: fixtures absent, skip.
    };
    let manifest = fixture_manifest(fixture_dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "the out-of-range INDEX_BITS fixture COMPILED — the \
         `INDEX_BITS must be in 1..=16` bounds check regressed:\n{context}"
    );
    assert!(
        stderr.contains("E0080"),
        "expected E0080 (evaluation panicked) in the fixture's compile \
         errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("INDEX_BITS must be in 1..=16"),
        "expected the `_CHECK_BITS` range requirement \
         (`INDEX_BITS must be in 1..=16`) in the fixture's compile \
         errors:\n{context}"
    );
}

#[test]
fn index_bits_zero_must_not_compile() {
    assert_index_bits_fixture_must_not_compile("index_bits_zero");
}

#[test]
fn index_bits_seventeen_must_not_compile() {
    assert_index_bits_fixture_must_not_compile("index_bits_seventeen");
}

/// Negative compile-fail regression: the crate promises that a `--cfg loom` build
/// WITHOUT its `loom` feature fails fast with ONLY the crate's own named
/// `compile_error!` — `building with --cfg loom requires --features loom
/// (loom is now an optional dependency)`.
///
/// The implementation module is cfg'd out entirely under
/// that invalid configuration, so NO secondary name-resolution error may
/// appear alongside the named one. This fixture pins BOTH halves:
///
/// 1. the named error IS present, and
/// 2. no secondary error (specifically `error[E0433]: cannot find module or
///    crate \`loom\`` from the loom-aliasing `use`) IS present.
///
/// That second half is exactly the regression class this test pins: before the
/// fix, the build also produced the E0433 unresolved-crate error on top of the
/// `compile_error!`.
///
/// The fixture at `tests/compile_fail/loom_cfg_without_feature/src/main.rs`
/// deliberately references NO crate items: it only declares the path
/// dependency (so the crate compiles) and its `fn main() {}` is empty — the
/// dependency's `compile_error!` is the only diagnostic expected.
///
/// This test is the INVERSE of the RUSTFLAGS-stripping default: the
/// `--cfg loom` configuration is the whole point, so the child env SETS
/// `RUSTFLAGS` to the literal `--cfg loom` (see the module-level
/// "RUSTFLAGS stripping" section; `CARGO_ENCODED_RUSTFLAGS` still goes, or
/// it silently cancels the override).
#[test]
fn loom_cfg_without_feature_fails_with_only_the_named_error() {
    let Some(output) = build_fixture("loom_cfg_without_feature", Some("--cfg loom")) else {
        return; // packaged package: fixtures absent, skip.
    };
    let manifest = fixture_manifest("loom_cfg_without_feature");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "the `--cfg loom` WITHOUT `--features loom` fixture COMPILED — the \
         crate's fast-fail compile_error! regressed:\n{context}"
    );
    assert!(
        stderr.contains(
            "building with --cfg loom requires --features loom \
             (loom is now an optional dependency)"
        ),
        "expected the crate's exact named compile_error! text in the \
         fixture's compile errors — it failed for some OTHER reason:\n\
         {context}"
    );
    assert!(
        !stderr.contains("E0433"),
        "expected NO secondary name-resolution error (E0433) alongside the \
         named compile_error! — the P2-4 regression (implementation module \
         not fully cfg'd out under the invalid configuration):\n{context}"
    );
    assert!(
        !stderr.contains("cannot find module or crate `loom`"),
        "expected NO `cannot find module or crate `loom`` error alongside \
         the named compile_error! — the P2-4 regression:\n{context}"
    );
}

/// Negative compile-fail regression, Group B of the storage-binding closure
/// (ADR `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`): a
/// storage impl whose hook bodies are CORRECT but whose declaration omits the
/// `unsafe` keyword must NOT compile. This pins Group B's actual mechanism —
/// the compiler-forced per-impl-site acknowledgment: `StackStorage` is an
/// `unsafe trait`, so no implementor can exist anywhere without asserting the
/// contract at the `unsafe impl` site. The asserted error is **E0200** ("the
/// trait `StackStorage<16>` requires an `unsafe impl` declaration").
///
/// The compile-PASS counterpart — a correct `unsafe impl` compiles and
/// behaves correctly — is pinned by `vec_backed_storage_push_pop_round_trips`
/// and `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs`.
/// Counterfactually, this exact fixture COMPILED under the pre-conversion
/// safe trait, with no acknowledgment possible or required.
#[test]
fn plain_impl_of_unsafe_stack_storage_must_not_compile() {
    let Some(output) = build_fixture("unsafe_impl_required", None) else {
        return; // packaged package: fixtures absent, skip.
    };
    let manifest = fixture_manifest("unsafe_impl_required");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

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
