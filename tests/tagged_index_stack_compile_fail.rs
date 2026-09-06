//! Root compile-fail driver for `tagged-index-stack`: one
//! negative-regression test per fixture, each building a
//! deliberately-broken fixture crate under
//! `crates/tagged-index-stack/tests/compile_fail/<fixture>/`
//! in an out-of-process `cargo build` and asserting it fails with SPECIFIC
//! errors (the test count is deliberately not quoted here — it drifts as
//! new hazards get pinned; `tests/support/tagged_index_stack_compile_fail.rs`
//! gives the re-derivation command). The shared
//! child-cargo mechanics (manifest resolution, spawn, diagnostic context
//! string) live in
//! `tests/support/tagged_index_stack_compile_fail.rs`; each `#[test]` below
//! keeps only its own assertions and its fixture-specific rationale.
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
//! deliberately-broken fixture, asserted to FAIL. The member's
//! `tests/stack_unit.rs` documents the compile-fail coverage that now exists
//! in this root target rather than a decline.
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
//! The fixture crates are explicitly excluded from the published `.crate`.
//! This root driver and its helper are repository-only; every checkout run
//! reaches its child Cargo build, and a missing fixture fails loudly.
//!
//! # RUSTFLAGS stripping
//!
//! Under `RUSTFLAGS="--cfg loom" cargo test`, an inherited `--cfg loom` would
//! make a fixture fail for the WRONG reason: the crate's own `--cfg
//! loom`-without-feature `compile_error!` (see `src/lib.rs`) fires before
//! any method resolution or bounds check, so assertions would pass even if
//! the regression under test had resurfaced. Every fixture EXCEPT the
//! loom-cfg fixture below has its child env REMOVE `RUSTFLAGS` (and
//! `CARGO_ENCODED_RUSTFLAGS`, which cargo prefers) so the fixture fails for
//! the RIGHT reason. The loom-cfg fixture is the INVERSE: the `--cfg loom`
//! configuration is the whole point, so its child env SETS `RUSTFLAGS` to
//! the literal `--cfg loom` (still removing `CARGO_ENCODED_RUSTFLAGS`).
#![cfg(not(loom))]

#[path = "support/tagged_index_stack_compile_fail.rs"]
mod compile_fail_support;

use compile_fail_support::{
    build_fixture, build_fixture_with_json, cargo_error_diagnostics, failure_context,
    fixture_manifest, CargoErrorDiagnostic,
};

fn assert_exact_fixture_diagnostics(
    output: &std::process::Output,
    fixture_dir: &str,
    expected: &[(&str, &str)],
) {
    let errors = cargo_error_diagnostics(output);
    assert_eq!(
        errors.len(),
        expected.len(),
        "unexpected number of Cargo error diagnostics for {fixture_dir}: {errors:#?}"
    );
    let mut matched = vec![false; expected.len()];
    for error in &errors {
        let (code, highlighted) = diagnostic_parts(error, fixture_dir);
        let index = expected
            .iter()
            .enumerate()
            .find_map(|(index, &(expected_code, expected_source))| {
                (!matched[index]
                    && expected_code == code.as_str()
                    && expected_source == highlighted.as_str())
                .then_some(index)
            })
            .unwrap_or_else(|| {
                panic!(
                    "diagnostic has no unmatched expected code/source pair for {fixture_dir}: \
                     code={code}, primary highlight={highlighted:?}"
                )
            });
        matched[index] = true;
    }
    assert!(
        matched.into_iter().all(|was_matched| was_matched),
        "diagnostic multiset omitted an expected code/source pair for {fixture_dir}"
    );
}

fn diagnostic_parts(error: &CargoErrorDiagnostic, fixture_dir: &str) -> (String, String) {
    assert!(
        !error.code_is_null,
        "expected a non-null Rust error code: {error:#?}"
    );
    let code = error
        .code
        .as_deref()
        .expect("expected every fixture diagnostic to carry a Rust error code");
    let primary = error
        .spans
        .iter()
        .filter(|span| span.is_primary)
        .collect::<Vec<_>>();
    assert_eq!(
        primary.len(),
        1,
        "each expected diagnostic must have exactly one primary span: {error:#?}"
    );
    let primary = primary[0];
    let path = primary.file_name.replace('\\', "/");
    assert_eq!(
        path, "src/main.rs",
        "primary span came from the wrong source path for {fixture_dir}"
    );
    assert_eq!(
        primary.line_start, primary.line_end,
        "expected a single-line primary span: {primary:#?}"
    );
    assert!(
        primary.column_end > primary.column_start,
        "expected a non-empty primary span: {primary:#?}"
    );
    let highlighted = primary
        .highlighted_text()
        .expect("expected source text and highlight structure for primary span");
    let highlighted = highlighted.trim();
    assert!(
        !highlighted.is_empty() && !primary.text[0].text.is_empty(),
        "expected non-empty primary source text: {primary:#?}"
    );
    (code.to_owned(), highlighted.to_owned())
}

fn is_tagged_index_stack_source(span: &compile_fail_support::CargoDiagnosticSpan) -> bool {
    span.file_name
        .replace('\\', "/")
        .ends_with("/crates/tagged-index-stack/src/lib.rs")
}

/// API-boundary regression: two `ArrayLinks` backings plus one `StackHead`
/// must NOT compile because `StackHead` has no
/// caller-supplied-backing operations. This is an API-boundary test, NOT a
/// safety-invariant proof: it pins that `StackHead` (the head word alone) has
/// no external `push(&links, idx)` / `pop(&links)` methods and no
/// caller-supplied-backing calling convention. A fixture that compiled would
/// violate that boundary.
///
/// The hazard CLASS itself — one head, two backings — is NOT closed by this
/// test and is NOT closed by the type system: it is re-expressible through
/// a custom `unsafe impl` that asserts the trait's `# Safety`
/// contract and then violates it (inventory shape 2; pinned at runtime by
/// `two_implementor_values_sharing_one_head_still_double_issue` in
/// `tests/custom_storage_impl.rs`). The structural closure that DOES exist —
/// no route from a shipped [`ArrayIndexStack`] to a `&StackHead`, so no
/// competing binding around its head — is proven by the compile-fail
/// oracle `competing_binding_around_array_index_stack_head_must_not_compile`
/// below (fixture under `crates/tagged-index-stack/tests/compile_fail/array_index_stack_head/`).
/// The storage-binding rationale is in
/// `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`.
///
/// The fixture at `crates/tagged-index-stack/tests/compile_fail/two_backings/src/main.rs` is exactly
/// two independent
/// `ArrayLinks` backings (`a`, `b`) plus one `StackHead<16>`, with the
/// stack's `push`/`pop` called externally against each. It must fail with
/// **E0599** ("no method named `push` / `pop` found for struct
/// `StackHead<16>`").
#[test]
fn two_arraylinks_backings_against_one_stackhead_must_not_compile() {
    let output = build_fixture_with_json("two_backings", None);
    let manifest = fixture_manifest("two_backings");
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "the two-backings fixture COMPILED — the caller-supplied-backing API (external \
         push/pop with a caller-supplied backing) has resurfaced; `StackHead` \
         must have no push/pop:\n{context}"
    );
    assert_exact_fixture_diagnostics(
        &output,
        "two_backings",
        &[("E0599", "push"), ("E0599", "pop")],
    );
}

/// Negative compile-fail regression from the storage-binding contract (ADR
/// `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`): a
/// competing binding built around a standalone [`ArrayIndexStack`]'s head must
/// NOT compile. The public type does not expose a route to its `StackHead`, so
/// the extraction route is unexpressible and this fixture is the structural
/// oracle.
///
/// The fixture at `crates/tagged-index-stack/tests/compile_fail/array_index_stack_head/src/main.rs` tries
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
/// reference requires the public `StackStorage` impl to be absent — the
/// generic-bound route (E0277), the inherent-method route (E0599), and the
/// `&dyn StackStorage` coercion route (E0277, third statement in the
/// fixture's `main`) — and the fixture asserts all of them fail. The
/// downstream construction (pairing a stolen head with fresh links under a
/// custom impl) adds no independent signal: its only live ingredient IS the
/// stolen head, which already fails to compile here. A competing binding
/// that does NOT involve this type — own a `StackHead`, hand it to two
/// custom `unsafe impl` values — remains expressible by design and is pinned
/// at runtime by
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
    let output = build_fixture("array_index_stack_head", None);
    let manifest = fixture_manifest("array_index_stack_head");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "the head-extraction fixture COMPILED — `ArrayIndexStack` must \
         NOT implement the public `StackStorage` trait (the sealing regressed):\n{context}"
    );
    assert!(
        stderr.contains("E0277"),
        "expected E0277 (unsatisfied `StackStorage` bound) in the fixture's \
         compile errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("ArrayIndexStack<16, 64>")
            && stderr.contains("steal_head(&owned)")
            && stderr.contains("&dyn StackStorage<16>"),
        "expected E0277 at THIS fixture's generic and dyn `StackStorage` routes:\n{context}"
    );
    assert!(
        stderr.contains("E0599"),
        "expected E0599 (no method named `head`) in the fixture's compile \
         errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("owned.head()"),
        "expected E0599 at THIS fixture's direct `owned.head()` call:\n{context}"
    );
}

/// Negative compile-fail regression: any of the three `StackStorage` hooks
/// (`head` / `load_next` / `store_next` — each an `unsafe fn` with a
/// per-method caller-side `# Safety` contract) called outside an `unsafe`
/// block must NOT compile, with **E0133** ("call to unsafe function `X` is
/// unsafe and requires unsafe function or block").
///
/// The hooks use the literal `GlobalAlloc` shape (`unsafe trait` + `unsafe
/// fn`): a compiler-enforced unsafe boundary, not a compiler-checked
/// contract — the compiler forces the `unsafe {}` acknowledgement at each
/// call site (E0133) and checks nothing beyond it.
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
    let output = build_fixture_with_json("hook_call_requires_unsafe", None);
    let manifest = fixture_manifest("hook_call_requires_unsafe");
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "the unsafe-call fixture COMPILED — the hooks became callable from \
         safe code (the caller-side `unsafe fn` boundary regressed):\n{context}"
    );
    assert_exact_fixture_diagnostics(
        &output,
        "hook_call_requires_unsafe",
        &[
            ("E0133", "pool.head()"),
            ("E0133", "pool.load_next(2)"),
            ("E0133", "pool.store_next(2, TAIL)"),
        ],
    );
}

/// Negative compile-fail regression: `TaggedIndex`'s `INDEX_BITS` const-generic
/// bounds must stay enforced — a `TaggedIndex<0>` and a `TaggedIndex<17>`
/// must NOT compile.
///
/// The fixtures at `crates/tagged-index-stack/tests/compile_fail/index_bits_zero/src/main.rs` and
/// `crates/tagged-index-stack/tests/compile_fail/index_bits_seventeen/src/main.rs` each read the
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
/// that motivated the shared root helper itself): builds the named
/// fixture and asserts it fails with the `_CHECK_BITS` E0080 range
/// requirement.
fn assert_index_bits_fixture_must_not_compile(fixture_dir: &str) {
    let output = build_fixture(fixture_dir, None);
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

/// Negative compile-fail regression: an owned array-backed stack must reject
/// N > TaggedIndex::<B>::INDEX_MASK when its constructor is instantiated.
/// The fixture also type-checks Default::default() for the same invalid
/// shape; both paths are required to route through the checked constructor.
/// For B = 4, the mask is 15, so N = 16 is the first invalid capacity.
///
/// This asserts the exact crate-owned const-evaluation diagnostic and the
/// single error-level diagnostic multiset. A bare stderr substring would also
/// pass if an unrelated error happened to mention INDEX_MASK.
#[test]
fn array_index_stack_capacity_above_index_mask_must_not_compile() {
    let output = build_fixture_with_json("array_index_stack_capacity", None);
    let manifest = fixture_manifest("array_index_stack_capacity");
    let context = failure_context(&manifest, &output);
    const EXPECTED_MESSAGE: &str =
        "evaluation panicked: ArrayIndexStack capacity N must be <= INDEX_MASK";

    assert!(
        !output.status.success(),
        "the over-capacity ArrayIndexStack fixture COMPILED — N > INDEX_MASK was not \
         checked at construction:\n{context}"
    );
    let errors = cargo_error_diagnostics(&output);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one error-level diagnostic and no coincidental \
         secondary failure:\n{context}"
    );
    let error = &errors[0];
    assert_eq!(
        error.code.as_deref(),
        Some("E0080"),
        "expected const-evaluation failure from the capacity assertion:\n{context}"
    );
    assert_eq!(
        error.message, EXPECTED_MESSAGE,
        "expected the exact crate-owned capacity diagnostic:\n{context}"
    );
    assert!(
        error
            .rendered
            .contains("ArrayIndexStack::<4, 16>::_CHECK_N")
            && error
                .rendered
                .contains("ArrayIndexStack capacity N must be <= INDEX_MASK"),
        "expected the diagnostic to identify this invalid instantiation and \
        the named crate assertion:\n{context}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ArrayIndexStack::<4, 16>::new()"),
        "expected the error's instantiation note to point at the fixture's \
         `new()` call:\n{context}"
    );
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
/// The fixture at `crates/tagged-index-stack/tests/compile_fail/loom_cfg_without_feature/src/main.rs`
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
    let output = build_fixture_with_json("loom_cfg_without_feature", Some("--cfg loom"));
    let manifest = fixture_manifest("loom_cfg_without_feature");
    let context = failure_context(&manifest, &output);
    const EXPECTED_MESSAGE: &str =
        "building with --cfg loom requires --features loom (loom is now an optional dependency)";

    assert!(
        !output.status.success(),
        "the `--cfg loom` WITHOUT `--features loom` fixture COMPILED — the \
         crate's fast-fail compile_error! regressed:\n{context}"
    );
    let errors = cargo_error_diagnostics(&output);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one error-level diagnostic — the crate-owned \
         compile_error! — and no secondary errors:\n{context}"
    );
    let error = &errors[0];
    assert_eq!(
        error.message, EXPECTED_MESSAGE,
        "expected the exact crate-owned compile_error! message, not another \
         failure mechanism:\n{context}"
    );
    assert!(
        error.code.is_none() && error.code_is_null,
        "expected the named compile_error! diagnostic to have no rustc error \
         code:\n{context}"
    );
    assert!(
        error.rendered.contains(EXPECTED_MESSAGE),
        "expected the rendered compiler diagnostic to contain the named \
         compile_error!:\n{context}"
    );
    assert!(
        error.spans.iter().any(is_tagged_index_stack_source),
        "expected the sole error's span to be in the crate-owned \
         `tagged-index-stack/src/lib.rs`, not in the fixture or Cargo:\n{context}"
    );
}

/// Negative compile-fail regression from the storage-binding contract (ADR
/// `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`): a
/// storage impl whose hook bodies are CORRECT but whose declaration omits the
/// `unsafe` keyword must NOT compile. This pins the mechanism —
/// the compiler-forced per-impl-site acknowledgment: `StackStorage` is an
/// `unsafe trait`, so no implementor can exist anywhere without asserting the
/// contract at the `unsafe impl` site. The asserted error is **E0200** ("the
/// trait `StackStorage<16>` requires an `unsafe impl` declaration").
///
/// The compile-PASS counterpart — a correct `unsafe impl` compiles and
/// behaves correctly — is pinned by `vec_backed_storage_push_pop_round_trips`
/// and `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs`.
/// Counterfactual: a safe trait would compile this fixture without an
/// acknowledgment.
#[test]
fn plain_impl_of_unsafe_stack_storage_must_not_compile() {
    let output = build_fixture("unsafe_impl_required", None);
    let manifest = fixture_manifest("unsafe_impl_required");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "a plain (non-`unsafe`) impl of `StackStorage` COMPILED — the trait \
         stopped being `unsafe` (the forced per-impl-site \
         acknowledgment regressed):\n{context}"
    );
    assert!(
        stderr.contains("E0200"),
        "expected E0200 (`unsafe impl` required) in the fixture's compile \
         errors — it failed for some OTHER reason:\n{context}"
    );
    assert!(
        stderr.contains("StackStorage<16>")
            && stderr.contains("impl StackStorage<16> for PlainStorage"),
        "expected E0200 at THIS `PlainStorage` impl site:\n{context}"
    );
}

/// Negative compile-fail regression: the two push entry points — the
/// blanket-impl [`StackOps::push_index`] and the owned type's inherent
/// [`ArrayIndexStack::push`] — are `unsafe fn` carrying the three-clause
/// caller-side contract (link domain + liveness + exclusive ownership), so a bare push outside an
/// `unsafe` block must NOT compile, with **E0133** ("call to unsafe function
/// `X` is unsafe and requires unsafe function or block") naming EACH entry
/// point. This pins that `push_index` (and `push`) sit on the same
/// compiler-enforced caller-side `unsafe` surface as the three
/// `StackStorage` hooks.
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
    let output = build_fixture_with_json("push_index_requires_unsafe", None);
    let manifest = fixture_manifest("push_index_requires_unsafe");
    let context = failure_context(&manifest, &output);

    assert!(
        !output.status.success(),
        "the unsafe-push fixture COMPILED — the push entry points became \
         callable from safe code (the caller-side `unsafe fn` boundary on \
         `push_index`/`push` regressed):\n{context}"
    );
    assert_exact_fixture_diagnostics(
        &output,
        "push_index_requires_unsafe",
        &[("E0133", "pool.push_index(0)"), ("E0133", "owned.push(0)")],
    );
}
