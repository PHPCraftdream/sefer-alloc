// SINGLE SOURCE OF TRUTH for the per-PR clippy/check/test feature-flag matrix.
//
// R30-5 (task #454): before this file, the feature/check matrix was
// reconstructed BY HAND, independently, in three places — `.github/
// workflows/ci.yml`'s job steps, `scripts/check-all.mjs`'s local pre-push
// steps, and ad-hoc per-task verification — and those three hand-maintained
// lists drifted out of sync with each other. Two concrete Round 29 escapes
// were caused directly by that drift:
//   1. `cargo check --bench perf_gate_iai --features "production
//      bench-internals"` — this is `scripts/iai.mjs`'s own `DEFAULT_FEATURES`
//      and is exercised as the LAST STEP of `npm run check`, yet it was never
//      an independent, always-run, standalone CI row of its own, so a build
//      break under exactly that feature string (R29-16's missing
//      `virgin-zero-skip` stub, 4x E0433) shipped undetected.
//   2. Plain `cargo clippy --features production -- -D warnings` — the
//      actual shipping default most users build with — was in NONE of
//      `ci.yml`'s clippy rows (`""`, `experimental`, `--all-features`,
//      `hardened medium-classes`), so R29-4's ungated dead-code structs
//      (`SegmentStateAccount`/`SegmentStateReconciliation`) shipped
//      undetected under it.
//
// EDIT HERE, NOT IN ci.yml / check-all.mjs DIRECTLY. This module is read by:
//   - scripts/check-all.mjs           (local `npm run check` pre-push gate)
//   - scripts/run-check-matrix.mjs    (the runner both check-all.mjs's
//                                      MATRIX-driven steps and a CI job use)
// `.github/workflows/ci.yml`'s hand-written per-combination job steps
// (`clippy`, `test-core`, `test-xthread`, etc.) are NOT converted to
// manifest-driven codegen by this task — see the header comment in
// `scripts/run-check-matrix.mjs` for why that would be more invasive than
// the value it buys here, and what was done instead (a single new CI step
// that loops this manifest's PER_PR_ROWS via that runner script).
//
// R31-11 (task #475): `ci.yml`'s `clippy` job stays hand-transcribed from
// this manifest's `clippy`-kind rows (kept as named per-step Actions-UI
// entries — see that job's own header comment for the UX rationale), which
// `run-check-matrix.mjs`'s `--kind check --kind test` filter does NOT
// re-verify (it structurally excludes `clippy`-kind rows). The actual
// enforcement that the transcription hasn't drifted is
// `tests/ci_clippy_matrix_consistency.rs`, a plain `cargo test` that reads
// this file's `PER_PR_ROWS` clippy rows and `ci.yml`'s `clippy` job steps as
// text and fails if they disagree in count, order, or feature string.
//
// SCOPE — this manifest intentionally holds a SMALL, curated set of
// high-value combinations, not an attempt at exhaustive feature-powerset
// coverage. CLAUDE.md's "cargo-hack feature-powerset CI" section already
// made a considered decision to run the ~308-invocation powerset sweep
// WEEKLY, not per-PR, because per-PR is the wrong cadence for that volume.
// `PER_PR_ROWS` below must stay small (a handful of entries, not dozens) —
// each entry here is either (a) a pre-existing check `ci.yml`/
// `check-all.mjs` already ran by hand, now just declared once instead of
// twice, or (b) one of the two specific commands Round 29 proved are
// missing. Do not grow this into a second powerset.

/**
 * @typedef {Object} CheckRow
 * @property {string} id           - stable short identifier (used in log
 *                                    output and as a de-dup key)
 * @property {'clippy'|'check'|'test'} kind
 * @property {string} features     - exact `--features` value; '' means no
 *                                    `--features` flag at all (default
 *                                    feature set only)
 * @property {string} [target]     - `--bench NAME` / `--example NAME` /
 *                                    `--test NAME` surface this row scopes
 *                                    to; absent = whole-crate (`--all-targets`
 *                                    for clippy, no target flag for
 *                                    check/test)
 * @property {string} note         - one-line rationale (why this row exists)
 */

/** @type {CheckRow[]} */
export const PER_PR_ROWS = [
  // --- Pre-existing rows, now declared once instead of duplicated by hand
  // in both ci.yml's `clippy` job and check-all.mjs's step list. ---
  {
    id: 'clippy-default',
    kind: 'clippy',
    features: '',
    note: 'default feature set (CI matrix entry 1)',
  },
  {
    id: 'clippy-experimental',
    kind: 'clippy',
    features: 'experimental',
    note: 'CI matrix entry 2',
  },
  {
    id: 'clippy-all-features',
    kind: 'clippy',
    features: '__all__',
    note: 'CI matrix entry 3 (--all-features)',
  },
  {
    id: 'clippy-hardened-medium-classes',
    kind: 'clippy',
    features: 'hardened medium-classes internals',
    note:
      'R23-5/task #374: the hardened+medium-classes dead-code combination ' +
      'that hid 11 pre-existing lints for 3+ rounds. `internals` added ' +
      '(R34-3/task #522): `hardened` implies `fastbin` implies ' +
      '`alloc-global`, and this row is `--all-targets`, which reaches this ' +
      "repo's own white-box `tests/` suite.",
  },

  // --- R30-5 (task #454): the two rows Round 29 proved are missing. ---
  {
    id: 'clippy-production',
    kind: 'clippy',
    features: 'production',
    // R34-3 (task #522, finding B1): `--lib` (not `--all-targets`) as of
    // this feature's introduction. `alloc_core`/`global`/`registry` moved
    // behind the new `internals` feature (additive over `alloc-core`/
    // `alloc-global`, NOT implied by `production`), and this repo's own
    // white-box `tests/` suite (~106 files reaching those module paths
    // directly — `tests/` files are NOT `[[test]]` Cargo.toml targets, so
    // they cannot carry their own `required-features`) does not compile
    // without it. `--all-targets` would therefore fail this row for a
    // reason unrelated to what it exists to check: whether the LIBRARY
    // itself — the actual artifact a downstream `production`-only consumer
    // builds — compiles clean. See `clippy-production-internals` below for
    // the `--all-targets` coverage of this repo's own dev suite.
    libOnly: true,
    note:
      'R29-4/task #435 escape: SegmentStateAccount/SegmentStateReconciliation ' +
      'were dead code under plain `production` (the actual shipping default) ' +
      'but no per-PR clippy row ever built exactly that feature string — ' +
      'every existing row was either narrower ("") or wider (--all-features, ' +
      'which turns on the bench-internals-gated consumer too).',
  },
  {
    id: 'clippy-production-internals',
    kind: 'clippy',
    features: 'production internals',
    note:
      'R34-3/task #522 (finding B1): once `alloc_core`/`global`/`registry`\'s ' +
      '`pub mod` path moved behind the new `internals` feature (additive ' +
      'over `alloc-core`/`alloc-global`, NOT implied by `production`), this ' +
      "repo's own white-box test/bench/example suite (~106 `tests/` files " +
      'that reach those module paths directly, plus every `required-features' +
      '` bench/example that lists `internals`) needs its own `--all-targets` ' +
      'per-PR row — the plain `clippy-production` row above is `--lib`-only ' +
      'and never turns `internals` on.',
  },
  {
    id: 'check-perf-gate-iai-default',
    kind: 'check',
    features: 'production bench-internals internals',
    target: { flag: '--bench', name: 'perf_gate_iai' },
    note:
      'R29-16/task #447 escape: this is scripts/iai.mjs\'s own ' +
      'DEFAULT_FEATURES and npm run check\'s LAST step, yet was never an ' +
      'independent standalone CI row — a build break here (4x E0433, missing ' +
      'virgin-zero-skip stub) shipped undetected for a full round. ' +
      '`internals` added (R34-3/task #522): `perf_gate_iai.rs` reaches ' +
      'sefer_alloc::registry::bootstrap / HeapCore / HeapRegistry directly.',
  },
  {
    id: 'test-internals-boundary-no-internals',
    kind: 'test',
    features: 'alloc-core alloc-global alloc-decommit',
    target: { flag: '--test', name: 'r34_3_internals_boundary_api' },
    note:
      'R34 review finding F1 (P2, docs/reviews/2026-08-05-round34-readonly-review.md §5): ' +
      '`tests/r34_3_internals_boundary_api.rs` exists to prove the stable ' +
      'crate-root re-exports resolve WITHOUT `internals`, but every other ' +
      'row that satisfies its `#![cfg(all(alloc-core, alloc-global, ' +
      'alloc-decommit))]` also turns `internals` on (where every name ' +
      'resolves regardless). This row runs the file under PLAIN `alloc-core ' +
      'alloc-global alloc-decommit` (NO `internals`) — the exact ' +
      'configuration the guard exists to test.',
  },
];

/** Sentinel used in `features` to mean "--all-features" rather than a literal
 * feature-string value (there is no single feature named "__all__"). */
export const ALL_FEATURES_SENTINEL = '__all__';

/**
 * Resolve one row's cargo args for a given `kind`-appropriate subcommand.
 * Centralizes the "how do --features / --bench / target flags get built"
 * logic so `check-all.mjs` and `run-check-matrix.mjs` build byte-identical
 * argv from the same row.
 */
export function rowToCargoArgs(row) {
  const args = [];
  if (row.kind === 'clippy') {
    args.push('clippy');
    if (row.target) {
      args.push(row.target.flag, row.target.name);
    } else if (row.libOnly) {
      // R34-3 (task #522): `--lib` (not `--all-targets`) scopes this row to
      // the actual shipped-library semver surface — used for the plain
      // `production` (no `internals`) row, since `tests/`'s ~106 files that
      // reach `alloc_core`/`global`/`registry` directly cannot compile
      // without `internals` and `--all-targets` would therefore fail for a
      // reason unrelated to what this row exists to check (whether the
      // LIBRARY compiles clean under the actual shipping default).
      args.push('--lib');
    } else {
      args.push('--all-targets');
    }
  } else if (row.kind === 'check') {
    args.push('check');
    if (row.target) {
      args.push(row.target.flag, row.target.name);
    }
  } else if (row.kind === 'test') {
    args.push('test');
    if (row.target) {
      args.push(row.target.flag, row.target.name);
    }
  } else {
    throw new Error(`unknown check kind: ${row.kind}`);
  }

  if (row.features === ALL_FEATURES_SENTINEL) {
    args.push('--all-features');
  } else if (row.features) {
    args.push('--features', row.features);
  }

  if (row.kind === 'clippy') {
    args.push('--', '-D', 'warnings');
  }

  return args;
}

/** Human-readable label for a row, used in log headers. */
export function rowLabel(row) {
  const feat =
    row.features === ALL_FEATURES_SENTINEL
      ? '--all-features'
      : row.features
        ? `--features "${row.features}"`
        : '(default features)';
  const target = row.target ? ` ${row.target.flag} ${row.target.name}` : '';
  return `${row.kind}${target} ${feat}`;
}
