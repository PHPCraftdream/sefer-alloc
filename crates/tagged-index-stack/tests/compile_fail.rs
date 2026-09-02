//! Consolidated compile-fail driver for `tagged-index-stack`: eight
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
//! the regression under test had resurfaced. For seven of the eight fixtures
//! the child env therefore REMOVES `RUSTFLAGS` (and
//! `CARGO_ENCODED_RUSTFLAGS`, which cargo prefers) so the fixture fails for
//! the RIGHT reason. The eighth — the loom-cfg fixture below — is the
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

/// Negative compile-fail regression: any of the three `StackStorage` hooks
/// (`head` / `load_next` / `store_next` — each an `unsafe fn` with a
/// per-method caller-side `# Safety` contract) called outside an `unsafe`
/// block must NOT compile, with **E0133** ("call to unsafe function `X` is
/// unsafe and requires unsafe function or block").
///
/// Supersession history: audit finding P2-1's caller-side forgery was first
/// closed by the `&Hook` witness — since removed, because fabricating a
/// witness value was not an unsafe operation, so that closure was prose-only
/// and unenforceable; the `unsafe fn` design replaces it with a
/// compiler-enforced unsafe boundary — the literal `GlobalAlloc` shape
/// (`unsafe trait` + `unsafe fn`) — not a compiler-checked contract: the
/// compiler only forces the `unsafe {}` acknowledgement.
///
/// # This fixture does NOT stand alone
///
/// The compile-PASS guarantee is equally load-bearing: the `unsafe fn` hooks
/// remain usable by legitimate implementors — a correct `unsafe impl` driving
/// the stack only through the safe `StackOps` API — and that is pinned by
/// `vec_backed_storage_push_pop_round_trips` and
/// `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs`. The
/// hooks are a barrier to MISUSE, not to legitimate use.
///
/// The fixture's implementor (`Pool`) is itself CORRECT — the only defects
/// are its three bare, unsafe-context-free hook calls in `main` — and the
/// calls are contract-shaped (index 2 was pushed through the same binding),
/// so the ONLY errors are the three E0133s, one per hook, each naming the
/// called method. (E0133 names the method, not the implementor type; the
/// fixture-specific type anchor below is the source snippet of each call
/// against `pool`, the `Pool` binding.)
#[test]
fn hook_call_requires_unsafe_block() {
    let Some(output) = build_fixture("hook_call_requires_unsafe", None) else {
        return; // packaged package: fixtures absent, skip.
    };
    let manifest = fixture_manifest("hook_call_requires_unsafe");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "the unsafe-call fixture COMPILED — the hooks became callable from \
         safe code (the caller-side `unsafe fn` boundary regressed):\n{context}"
    );
    assert!(
        stderr.contains("E0133"),
        "expected E0133 (call to unsafe function is unsafe) in the fixture's \
         compile errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("is unsafe and requires unsafe function or block"),
        "expected the exact E0133 wording `is unsafe and requires unsafe \
         function or block`:\n{context}"
    );
    assert!(
        stderr.contains("call to unsafe function `head`"),
        "expected an E0133 naming the `head` hook:\n{context}"
    );
    assert!(
        stderr.contains("call to unsafe function `load_next`"),
        "expected an E0133 naming the `load_next` hook:\n{context}"
    );
    assert!(
        stderr.contains("call to unsafe function `store_next`"),
        "expected an E0133 naming the `store_next` hook:\n{context}"
    );
    assert!(
        stderr.contains("pool.head()"),
        "expected the fixture's own implementor binding (`pool`, a `Pool`) \
         in the E0133 source snippets — the errors must come from THIS \
         fixture's calls, not an unrelated site:\n{context}"
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

/// Negative compile-fail regression: the two push entry points — the
/// blanket-impl [`StackOps::push_index`] and the owned type's inherent
/// [`ArrayIndexStack::push`] — are `unsafe fn` carrying the two-clause
/// caller-side contract (link domain + liveness), so a bare push outside an
/// `unsafe` block must NOT compile, with **E0133** ("call to unsafe function
/// `X` is unsafe and requires unsafe function or block") naming EACH entry
/// point. This pins the 2026-09-02 boundary move that made `push_index` (and
/// `push`) join the three `StackStorage` hooks on the compiler-enforced
/// caller-side `unsafe` surface.
///
/// # This fixture does NOT stand alone
///
/// The compile-PASS counterpart — in-domain, live-free pushes issued from
/// `unsafe` blocks — is pinned everywhere else in the suite: the fixture's
/// own setup pushes compile (properly wrapped, with SAFETY comments), and
/// `vec_backed_storage_push_pop_round_trips` +
/// `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs` drive
/// wrapped pushes end-to-end. The `unsafe fn` boundary is a barrier to
/// MISUSE, not to legitimate use.
///
/// The fixture's implementor (`Pool`) is itself CORRECT — the only defects
/// are its two bare, unsafe-context-free push calls in `main`, one through
/// each entry point — so the ONLY errors are the two E0133s, each naming
/// the called function (`push_index` and `push`). (E0133 names the
/// function, not the type; the fixture-specific anchors below are the
/// source snippets of each call against the fixture's own bindings
/// `pool` and `owned`.)
#[test]
fn push_index_requires_unsafe_block() {
    let Some(output) = build_fixture("push_index_requires_unsafe", None) else {
        return; // packaged package: fixtures absent, skip.
    };
    let manifest = fixture_manifest("push_index_requires_unsafe");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "the unsafe-push fixture COMPILED — the push entry points became \
         callable from safe code (the caller-side `unsafe fn` boundary on \
         `push_index`/`push` regressed):\n{context}"
    );
    assert!(
        stderr.contains("E0133"),
        "expected E0133 (call to unsafe function is unsafe) in the fixture's \
         compile errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("is unsafe and requires unsafe function or block"),
        "expected the exact E0133 wording `is unsafe and requires unsafe \
         function or block`:\n{context}"
    );
    assert!(
        stderr.contains("call to unsafe function `push_index`"),
        "expected an E0133 naming the `push_index` entry point:\n{context}"
    );
    assert!(
        stderr.contains("call to unsafe function `ArrayIndexStack::<B, N>::push`"),
        "expected an E0133 naming the owned type's `push` entry point (rustc \
         qualifies inherent unsafe methods as `Type::push`):\n{context}"
    );
    assert!(
        stderr.contains("pool.push_index(0)"),
        "expected the fixture's own implementor binding (`pool`, a `Pool`) \
         in the E0133 source snippets — the errors must come from THIS fixture's \
         calls, not an unrelated site:\n{context}"
    );
    assert!(
        stderr.contains("owned.push(0)"),
        "expected the fixture's own owned-type binding (`owned`) in the \
         E0133 source snippets — the errors must come from THIS fixture's \
         calls, not an unrelated site:\n{context}"
    );
}
