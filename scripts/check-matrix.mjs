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
    features: 'hardened medium-classes',
    note:
      'R23-5/task #374: the hardened+medium-classes dead-code combination ' +
      'that hid 11 pre-existing lints for 3+ rounds',
  },

  // --- R30-5 (task #454): the two rows Round 29 proved are missing. ---
  {
    id: 'clippy-production',
    kind: 'clippy',
    features: 'production',
    note:
      'R29-4/task #435 escape: SegmentStateAccount/SegmentStateReconciliation ' +
      'were dead code under plain `production` (the actual shipping default) ' +
      'but no per-PR clippy row ever built exactly that feature string — ' +
      'every existing row was either narrower ("") or wider (--all-features, ' +
      'which turns on the bench-internals-gated consumer too)',
  },
  {
    id: 'check-perf-gate-iai-default',
    kind: 'check',
    features: 'production bench-internals',
    target: { flag: '--bench', name: 'perf_gate_iai' },
    note:
      'R29-16/task #447 escape: this is scripts/iai.mjs\'s own ' +
      'DEFAULT_FEATURES and npm run check\'s LAST step, yet was never an ' +
      'independent standalone CI row — a build break here (4x E0433, missing ' +
      'virgin-zero-skip stub) shipped undetected for a full round',
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
