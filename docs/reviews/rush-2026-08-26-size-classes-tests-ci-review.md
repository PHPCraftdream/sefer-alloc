# `size-classes`: test-oracle strength & CI gate coverage review (Rush)

**Reviewer:** Rush (test oracles / CI gates scope; sibling reviewers: `size-classes-review-numerics`, `size-classes-review-api-docs`)

**Date:** 2026-08-26

**Mode:** read-only static review — no build/test/clippy/doc/publish commands, no git writes, no file edits other than this report.

**HEAD reviewed:** `d00788e` (moved during this review: the session began at `61a5b62` — the brief's stated `1a908c0` was already two commits stale — and three commits landed mid-review: `d1eb74b` (run-6 P2-1+P3-1 doc fix), `0cd60c3` (run-6 P4-1 test), `d00788e` (run-6 P4-2 CI-comment fix)). All findings below were verified against `d00788e`.

## Verdict

**GO** — from the test-oracle / CI-gate standpoint specifically. No P0/P1/P2 findings in this scope. Two P3s (one CI comment claims coverage that does not exist; one group of documented `# Panics` conditions with zero tests) and three P4 nits, each with a concrete fix sketch. Neither P3 blocks the first crates.io release on its own merits; both are cheap to close and in this repo's own finding classes ("comment-claims-coverage-it-does-not-have" and "documented panic without a test").

Baseline note: all four findings of Sol-codex run 6 were fixed by the three commits that landed during this review (verified by reading each diff); none are restated here.

## Findings

### T1 — P3: the size-classes MSRV comment claims `bench-scale-tool` coverage that `cargo test --no-run` does not provide

**Where:** `.github/workflows/ci.yml:2055-2066` (the claim at 2060-2063; the two rows at 2065-2066).

**Gap:** The comment justifies the two rows (`cargo check -p size-classes`, `cargo test -p size-classes --no-run`) by saying the crate's "`tests/builder.rs`/`tests/proptest_builder.rs` and the `proptest`/`bench-scale-tool` dev-dependencies were never compiled on the pinned 1.88 toolchain by anything in this job". The `proptest` half is true; the `bench-scale-tool` half is not closed by these rows. `benches/size_classes_bench.rs` is a `harness = false` bench (`crates/size-classes/Cargo.toml:32-34` has no `test` key), and this repository has already established — empirically, in task #1395's commit message (`86e9a83`, for once-ptr-cell's identical bench shape) — that "`cargo test -p <crate> --no-run` lists exactly three executables and no bench": `cargo test` does not compile `harness = false` bench targets. once-ptr-cell subsequently pinned its intent with an explicit `test = false` plus a manifest comment ("`cargo test` must not treat this bench as a test target … an implicit default is not something a workflow comment should be citing", `crates/once-ptr-cell/Cargo.toml:38-42`); size-classes has the same bench shape but no such row or key.

Concretely: **nothing in any CI job compiles `benches/size_classes_bench.rs` or `bench-scale-tool` on the 1.88 MSRV toolchain.** The only rows that compile the bench at all are the stable-toolchain `cargo clippy -p size-classes --all-targets` (ci.yml:1865) and the stable `test-workspace` job's build graph. An MSRV-incompatible construct introduced into the bench (or a future bench-scale-tool 0.x that requires >1.88) would pass every gate and surface only when a human runs `cargo bench`. This is the same class as the task #1144 correction at ci.yml:1765-1781 (a CI comment asserting MSRV coverage that no job provided), which this repo treats as a finding in its own right.

**Fix:** add one row to the `msrv` job after ci.yml:2066 and correct the comment to match reality:

```yaml
      - run: cargo bench -p size-classes --no-run
```

(`--no-run` compiles the bench + `bench-scale-tool` on 1.88 without measuring anything; `cargo check -p size-classes --benches` would also do if a `bench` invocation is unwanted in a check job.) Either reword the comment to claim only `proptest`, or keep the dev-deps claim and let the new row actually deliver it. Verification command for the fixer (one local run settles the cargo-semantics premise independently of task #1395's record): `cargo test -p size-classes --no-run --message-format=json 2>/dev/null | grep -c size_classes_bench` → expect `0`.

**Counterfactual:** the new row fails the day `benches/size_classes_bench.rs` (or `bench-scale-tool`) stops compiling on 1.88; today nothing fails.

**Out-of-scope observation (once-ptr-cell):** the once-ptr-cell MSRV comment at ci.yml:2042-2054 makes the same half-claim ("the dev-dependency `bench-scale-tool` were never compiled … before this row") for a bench that is *doubly* excluded (`harness = false` **and** explicit `test = false`). Not fixed here — different crate, outside this review's scope; flagging for the orchestrator.

### T2 — P3: eight documented `# Panics` conditions have no test anywhere in the crate

**Where:** production assert sites in `crates/size-classes/src/lib.rs`; absence verified by grepping every message string against `tests/` and `benches/` (all return zero matches at HEAD).

**Gap:** the crate's own convention for precondition panics is runtime `#[should_panic]` invocation of the same `const fn`s (`tests/builder.rs:255-262` says so explicitly, and the `Params::extras` trio got exactly such tests). Eight documented panic conditions never got them. A regression that deletes any of these guards, or reorders/mangles its message, ships silently. Full inventory, with a concrete input for each:

| # | Documented condition (site) | Message | Input that must panic |
|---|---|---|---|
| 1 | `size2class_len` doc `# Panics` (lib.rs:132-137), assert 145-148 | `size2class_len: min_block must be a power of two` | `size2class_len(64, 12)` |
| 2 | `build_table` doc `# Panics` (lib.rs:171-189), assert 193-196 | `min_block must be a power of two` | `build_table::<1>(&Params::new(12, (5,4), 1, &[], 1<<20))` |
| 3 | `build_size2class` doc `# Panics` (lib.rs:390-399), assert 406-409 | `min_block must be a power of two` | `build_size2class::<3, 4>(&[16,32,48], 12)` |
| 4 | `build_table`, assert 197 | `geo_count must be > 0` | `build_table::<2>(&Params::new(16, (5,4), 0, &[16,32], 1<<20))` |
| 5 | `build_table`, assert 202 | `growth denominator must be > 0` | `build_table::<1>(&Params::new(16, (1,0), 1, &[], 1<<20))` — the guard exists specifically to replace a bare "attempt to divide by zero"; deleting it degrades the diagnostic silently |
| 6 | `build_table`, assert 209-213 | `N must equal geo_count + extras.len()` | `build_table::<5>(&Params::new(16, (5,4), 3, &[64], 1<<20))` — the single likeliest real-user error when hand-deriving the const generic |
| 7 | `build_table`, assert 246-251 (per-entry, among `extras` themselves) | `Params::extras: must be strictly increasing` | `build_table::<6>(&Params::new(16, (5,4), 4, &[64, 32], 1<<20))` — note this is a *different* check from the merged-table one; the split-test comment at builder.rs:282-298 exists precisely because these two messages once collided in a `#[should_panic]`, yet the per-entry message itself has never been pinned |
| 8 | `build_size2class`, assert 405 (`table must be non-empty`) and assert 444-447 (`L must equal size2class_len(max_class, min_block)`, plain non-overflow variant) | as quoted | `build_size2class::<0, 1>(&[], 16)`; `build_size2class::<3, 5>(&[16,32,48], 16)` (correct `L` is 4) |

Plus one public *method*: `SizeClasses::block_size` documents `# Panics` "if `idx >= N`" (lib.rs:621-624; the panic is the bare `self.table[idx]` at 626) and no test ever passes an out-of-range index — every call site uses an index obtained from `class_for`. A `#[should_panic(expected = "index out of bounds")]` on `DOMAIN_SC.block_size(DOMAIN_N)` (fixtures already exist at builder.rs:148-153) closes it.

**Why P3 and not higher:** all ten asserts exist today and are one-liners; the risk is undetected removal/drift, not a live bug. But this crate has had a `#[should_panic]` pass *for the wrong reason* within living memory (run-1 P2-2), and five audit rounds tightened oracles everywhere else; these are the remaining uncovered ones.

**Counterfactual:** each proposed test fails if its guard is deleted (the call then either succeeds, panics with a different/unmatched message, or — for #5 — panics with the bare divide-by-zero message).

### T3 — P4: `min_block()` and `small_align_max()` accessors are never called by any test or bench

**Where:** `src/lib.rs:584-602` (both accessors); zero call sites in `tests/builder.rs`, `tests/proptest_builder.rs`, `benches/size_classes_bench.rs` (grep for `\.min_block\(\)` / `small_align_max\(\)` returns nothing).

**Gap:** an accessor returning the wrong field — e.g. `min_block()` returning `huge_threshold`, or `small_align_max()` returning `min_block_shift as usize` — passes the entire suite. The *fields* are partially covered indirectly (`class_for`'s sweep would catch a wrong `small_align_max` field for swept aligns, since `align = 2*min_block` is in the sweep), but the accessor surface itself is untested.

**Fix:** two lines inside `sefer_table_matches_reference_and_is_strictly_increasing` (or the domain test):

```rust
assert_eq!(SEFER_SC.min_block(), SEFER_MIN_BLOCK);
assert_eq!(SEFER_SC.small_align_max(), SEFER_MIN_BLOCK); // documented == min_block
```

**Counterfactual:** fails on any accessor/field mismatch; today nothing does.

### T4 — P4: the bench↔test fixture duplication is comment-enforced only, and the bench's "production-like" scheme has no identity oracle against the real consumer

**Where:** `benches/size_classes_bench.rs:22-44` (full `SEFER_*` block + `JUMP_A`/`JUMP_B`), duplicated from `tests/builder.rs:70-84` and 235-236, joined only by "Keep in sync with …" comments on both sides; consumer scheme at `src/alloc_core/size_classes.rs:97-153`.

**Gap:** the path-activation oracle `sefer_bench_jump_rows_genuinely_exercise_the_slow_path` pins only the *test file's* copy of `JUMP_A`/`JUMP_B` and the `SEFER_*` params. If someone edits the bench's pairs (or its `SEFER_EXTRAS`) without editing the test, the oracle keeps passing while pinning pairs the bench no longer uses — the exact inertness the oracle was built to prevent (review-2 F2). Separately, the bench comment's "the realistic production-like configuration the actual allocator uses" is honest for the *default* feature set (verified: `GROWTH=(5,4)`, `GEO_COUNT=40`, 9-entry `EXTRAS`, and `HUGE_THRESHOLD = 4*1024*1024 == os::SEGMENT = 1<<22` all match `src/alloc_core/size_classes.rs`) but the consumer's `EXTRAS` is cfg-gated to 15/18 entries under `medium-classes`/`medium-classes-wide`; if the *default* ever changes on the consumer side, every bench row and its activation oracle keep measuring a stale scheme with nothing failing.

**Fix (sketch, two independent options):**
1. Share the constants mechanically: move `SEFER_*` + `JUMP_A`/`JUMP_B` into `tests/common/mod.rs` and pull it into the bench via `#[path = "../tests/common/mod.rs"] mod common;` — benches are free to include arbitrary paths, and the "keep in sync" comments disappear.
2. Add a root-crate identity test (root tests can see `pub(crate)` items of `alloc_core`): rebuild the table with the bench's exact params via `size_classes::build_table` and `assert_eq!` against `alloc_core::size_classes::SIZE_CLASS_TABLE` — pins "bench scheme == production default scheme" so consumer drift fails a test.

**Counterfactual:** option 1 fails on any single-sided fixture edit; option 2 fails when `src/alloc_core/size_classes.rs`'s default params change without the bench following. Today neither fails.

### T5 — P4: the align-driven early-rejection input shape is never fed to `class_for`

**Where:** `tests/builder.rs:108-129` (sweep aligns stop at `SEFER_MAX`), `tests/proptest_builder.rs:98-147` (`pow2_up_to(A_MAX)` etc. caps align at each scheme's `small_max`).

**Gap:** the `need > small_max → None` branch is exercised only with `size` as the driver (sizes go to `SEFER_MAX+1` / `2 * A_MAX`, but aligns never exceed `small_max`, so `need = max(size, align)` is never pushed over by `align` alone). The branch is covered, so this is an input-shape nit, not branch coverage: a small-size/huge-align request (`class_for(32, 2*SEFER_MAX)` → must be `None` *before* any lookup) is the shape a caller with an exotic `Layout::from_align_align` would actually produce.

**Fix:** one line in the sweep's align list — `aligns.push(2 * SEFER_MAX);` — the reference scan already handles it (`position` of `b >= need` with `need = 2*SEFER_MAX` → `None`).

**Counterfactual:** fails if the early rejection ever consults the table or the align predicate before rejecting; today that input is simply never generated.

## Verified strong (checked against the code, not restated from prior reports)

- **Bench path-activation oracle satisfies CLAUDE.md's R30-8 rule.** `sefer_bench_jump_rows_genuinely_exercise_the_slow_path` (builder.rs:221-253) proves, per row, that the seed class is *not* align-divisible (so the jump-loop round-up body must run) and that the result is divisible and `>= need`; the fast-path `small_hit` row cannot silently degrade to the slow path (the branch depends only on `align <= small_align_max`, structurally impossible to flip by table edits); the `near_small_max` rows' claims (`SEFER_MAX-1`/`SEFER_MAX` resolve, `SEFER_MAX+1` → `None`) are pinned by the exhaustive sweep at exactly those sizes for `align = 1`; `is_huge` rows measure a single comparison with nothing to activate. The one residual is T4's sync mechanism.
- **No vacuous oracles found in the two test files.** All 32 `#[test]` functions reviewed one by one: the `#[should_panic]` `expected` substrings are specific and, where two sites could emit similar messages, deliberately disjoint (builder.rs:282-298); the `size2class_raw_domain_first_out_of_bounds_size_panics` test pre-asserts `idx == L` before indexing, so it cannot pass against an input that doesn't reach the boundary; `is_huge` proves parameterization with two schemes giving opposite verdicts for one size (not the prose-only claim it once was); the golden table was re-derived by hand here and is arithmetically correct.
- **proptest configuration matches the repo convention**: `ProptestConfig::with_cases(64)` (proptest_builder.rs:105), matching CLAUDE.md "Speed: short scenario by default" (~64 cases as a smoke-check). The three schemes are genuinely structurally distinct, not three copies of one path: `min_block` 16/8/64 (so the fast/slow-path boundary `align <= min_block` sits at three different positions and the shift arithmetic runs at shifts 4/3/6), growth 1.25×/1.5×/1.125×, extras interleaved-with-the-run (A: all 5 extras below the geo top), absent (B), and appended-past-the-run (C: geo top ≈2 KiB, extras 8 KiB–64 KiB).
- **Triple-oracle shape is sound**: jump (`class_for`) vs walk (pre-jump step-by-1) vs scan (independent `position` predicate) — a LUT-seeding bug that jump and walk would share is still caught by the scan.
- **CI checklist for this crate, all present and correctly scoped** (`-p size-classes` in every row): debug tests (ci.yml:1850), release tests (1851), bare-metal `no_std` build for `thumbv7em-none-eabi` (1832), `clippy --all-targets -- -D warnings` (1865), `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` (1866), MSRV lib+tests compile (2065-2066; bench gap = T1), `cargo publish --dry-run` (734, with an honest scoping comment at 711-724 explaining the absent semver-checks row pre-first-publish). `release.yml` correctly wires `size-classes-v*` tags and `workflow_dispatch` for the eventual publish. `scripts/check-matrix.mjs` legitimately has no size-classes rows (it is the root-crate feature matrix; this crate has no features).
- **README example mirror is verbatim** (README.md:39-58 vs builder.rs `readme_example_compiles_and_derives_its_generics`) — compared field by field, no drift.
- **Doc-contract changes across the 6 prior rounds all carry tests where testable**: extras preconditions (run 1) → `extras_*` panic tests; 256/257 boundary (run 2) → both-sides tests; raw-LUT domain (run 5 → fixed in `657deab`, tested in `1a908c0`, extended to the overflow-activating extreme64 fixture in `0cd60c3` during this review); the stride-vs-address contract (run 3) is untestable in-crate by design (no addresses) and is documented as caller-owned.

## Not restated

All six Sol-codex runs' findings were re-checked for currency: every one is either closed in the tree or was closed by `d1eb74b`/`0cd60c3`/`d00788e` during this review. No prior finding is re-reported here. The only overlap with a sibling reviewer: run-6 P2-1/P3-1 territory (`size2class()` doc) is `size-classes-review-api-docs`'s scope; I note only that the concurrent fix landed and the new doc text plus its new extreme64 test are internally consistent.
