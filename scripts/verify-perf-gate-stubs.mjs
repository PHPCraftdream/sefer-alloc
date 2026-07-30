// R30-5 (task #454): mechanical, generated check for the "feature ABSENT"
// stub pattern `benches/perf_gate_iai.rs` establishes for conditionally
// compiled `library_benchmark_group!` members (see the R24-8 stub block
// around the file's `dealloc_batch_fresh_*` functions, and the R29-16
// `virgin-zero-skip` stub block this task's own investigation found — both
// pre-date this script).
//
// THE BUG THIS CATCHES: R29-16 added 4 new iai bench arms gated on
// `feature = "virgin-zero-skip"` (an OPT-IN feature, NOT part of
// `production`) to `library_benchmark_group!`'s member list, without adding
// the matching `not(feature = "virgin-zero-skip")` no-op stub functions the
// same file already established as the pattern for `batch-api` (another
// opt-in feature). Compiling `benches/perf_gate_iai.rs` under `--features
// "production bench-internals"` (the DEFAULT feature set for this bench —
// see `scripts/iai.mjs`'s `DEFAULT_FEATURES` and `Cargo.toml`'s
// `required-features` for this `[[bench]]`) does NOT turn `virgin-zero-skip`
// on, so those 4 function names appeared in the `library_benchmark_group!`
// macro's member list with NO matching item definition at all under that
// feature set --- 4x E0433 ("failed to resolve").
//
// THE RULE THIS ENFORCES: every function name referenced inside
// `library_benchmark_group!`'s `benchmarks = ...` list that has an ON
// definition gated on `feature = "X"` for an X that is NOT part of
// `production`'s always-on feature set must ALSO have an OFF definition
// (same function name) gated on `not(feature = "X")` (bitwise: the same
// feature token appearing negated), so the group macro resolves whether or
// not X is enabled. A function gated ONLY on features that are already part
// of `production` (e.g. `alloc-xthread`, `alloc-decommit`) does not need a
// stub, because `production bench-internals` — the feature set every
// per-PR/local check actually compiles this bench under — always turns
// those on; there is no way to reach a "feature absent" state for them
// without dropping `production` itself, which is out of scope for this
// bench (see its `required-features = ["alloc-global", "bench-internals"]`
// in Cargo.toml — `alloc-global` alone does not pull in `production`, but
// every actual check in this repo's manifest/CI/local-gate builds this
// bench WITH `production`, never bare `alloc-global`).
//
// This is intentionally a regex/text scan, not a real Rust parser — the
// established style in this project's `scripts/*.mjs` tooling (see e.g.
// `scripts/iai.mjs`'s own stdout-parsing regexes) is "good enough text
// scanning over a known, hand-authored file shape", not a heavyweight AST
// dependency for a single source file.
//
// Usage:
//   node scripts/verify-perf-gate-stubs.mjs
//   npm run check:stubs

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const BENCH_FILE = resolve(REPO_ROOT, 'benches/perf_gate_iai.rs');

// The always-on feature set every check in this repo's manifest/CI/local
// gate builds `perf_gate_iai` under (see `scripts/iai.mjs`'s
// `DEFAULT_FEATURES` and `scripts/check-matrix.mjs`'s
// `check-perf-gate-iai-default` row). A bench fn gated ONLY on a subset of
// these tokens can never observe a "feature absent" state under any check
// this project actually runs, so it needs no stub.
const ALWAYS_ON = new Set([
  'production',
  'bench-internals',
  'alloc-global',
  'alloc-xthread',
  'alloc-decommit',
  'fastbin',
  'alloc-segment-directory',
  'primordial-lazy-commit',
  'class-aware-dirty',
  // alloc-core is a transitive dependency of alloc-xthread/alloc-decommit
  // (see Cargo.toml's feature graph) — also always on once `production` is.
  'alloc-core',
]);

const text = readFileSync(BENCH_FILE, 'utf8');
const lines = text.split(/\r?\n/);

/**
 * Parse the file into a list of `{ name, cfgText, line }` records — one per
 * `fn NAME(...)` immediately preceded (allowing an intervening
 * `#[library_benchmark]`) by zero or one `#[cfg(...)]` attribute (which may
 * span multiple lines, e.g. `#[cfg(all(\n  ...\n))]`).
 *
 * Implementation: walk lines once, maintaining `pendingCfg` (the most
 * recently closed `#[cfg(...)]` attribute's text + start line, or `null`).
 * `#[cfg(` opens a paren-balanced accumulation that may span several lines;
 * once its parens balance to zero, `pendingCfg` is set and held until the
 * very next `fn` line consumes it (an intervening `#[library_benchmark]`
 * line is transparently skipped, not treated as "content that clears
 * pendingCfg"). This is a single unambiguous state machine (no dual-branch
 * re-entry bug): every line is handled by EXACTLY ONE of the four arms
 * below, never both an accumulation check and a fresh-attribute check on
 * the same iteration.
 */
function parseFnDefs() {
  const defs = [];
  let pendingCfg = null; // { text, line } | null
  let accum = null; // { text, depth } | null — mid multi-line #[cfg(...)]

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (accum) {
      accum.text += '\n' + line;
      accum.depth += (line.match(/\(/g) || []).length - (line.match(/\)/g) || []).length;
      if (accum.depth <= 0) {
        pendingCfg = { text: accum.text, line: accum.startLine };
        accum = null;
      }
      continue;
    }

    const cfgStart = /^\s*#\[cfg\((.*)$/.exec(line);
    if (cfgStart) {
      const rest = cfgStart[1];
      const depth = 1 + (rest.match(/\(/g) || []).length - (rest.match(/\)/g) || []).length;
      const text = `#[cfg(${rest}`;
      if (depth <= 0) {
        pendingCfg = { text, line: i + 1 };
      } else {
        accum = { text, depth, startLine: i + 1 };
      }
      continue;
    }

    if (/^\s*#\[library_benchmark\]\s*$/.test(line)) continue; // transparent

    const fnMatch = /^\s*fn\s+(\w+)\s*\(/.exec(line);
    if (fnMatch) {
      defs.push({
        name: fnMatch[1],
        cfgText: pendingCfg ? pendingCfg.text : null,
        line: pendingCfg ? pendingCfg.line : i + 1,
      });
      pendingCfg = null;
      continue;
    }

    // Any other non-blank, non-comment line clears a stale pendingCfg
    // (defensive — this file's actual layout never has one, since cfg is
    // always immediately followed by `#[library_benchmark]` then `fn`).
    if (pendingCfg && line.trim() && !line.trim().startsWith('//')) {
      pendingCfg = null;
    }
  }
  return defs;
}

/** Extract every `feature = "X"` token (positive, not inside a `not(...)`)
 * and every `not(feature = "X")` token from a cfg predicate's text. Returns
 * `{ positive: Set<string>, negated: Set<string> }`. This is a simple
 * text scan, not full predicate evaluation — sufficient for this file's
 * `all(...)`/bare-token/`not(feature = "X")` shapes (verified against every
 * cfg in the file below by cross-checking the enumerated set against a
 * manual read).
 */
function extractFeatureTokens(cfgText) {
  const positive = new Set();
  const negated = new Set();
  if (!cfgText) return { positive, negated };
  // not(feature = "X") first, so its feature token is not ALSO counted as
  // positive by the generic feature-scan below.
  const notRe = /not\(\s*feature\s*=\s*"([^"]+)"\s*\)/g;
  let m;
  const notSpans = [];
  while ((m = notRe.exec(cfgText))) {
    negated.add(m[1]);
    notSpans.push([m.index, m.index + m[0].length]);
  }
  const featRe = /feature\s*=\s*"([^"]+)"/g;
  while ((m = featRe.exec(cfgText))) {
    const insideNot = notSpans.some(([s, e]) => m.index >= s && m.index < e);
    if (!insideNot) positive.add(m[1]);
  }
  return { positive, negated };
}

function extractGroupMembers(fileText) {
  const gm =
    /library_benchmark_group!\(\s*name\s*=\s*perf_gate;\s*benchmarks\s*=\s*([\s\S]*?)\);/.exec(
      fileText,
    );
  if (!gm) throw new Error('could not find library_benchmark_group! block');
  return gm[1]
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
}

const defs = parseFnDefs();
const groupMembers = extractGroupMembers(text);

// Index defs by name -> list of { positive, negated, line }
const byName = new Map();
for (const d of defs) {
  const { positive, negated } = extractFeatureTokens(d.cfgText);
  const list = byName.get(d.name) ?? [];
  list.push({ positive, negated, line: d.line });
  byName.set(d.name, list);
}

const problems = [];
const optInFeaturesSeen = new Set();

for (const name of groupMembers) {
  const variants = byName.get(name);
  if (!variants || variants.length === 0) {
    problems.push(`${name}: referenced in library_benchmark_group! but no fn definition found at all`);
    continue;
  }
  // Collect every opt-in (non-ALWAYS_ON) feature gating any ON variant of
  // this function (a variant with no `not(...)` on that same token).
  for (const v of variants) {
    const optInPositives = [...v.positive].filter((f) => !ALWAYS_ON.has(f));
    for (const feat of optInPositives) {
      optInFeaturesSeen.add(feat);
      // Does SOME variant of this same fn name carry `not(feature = feat)`?
      const hasStub = variants.some((other) => other.negated.has(feat));
      if (!hasStub) {
        problems.push(
          `${name} (line ${v.line}): gated on opt-in feature "${feat}" with ` +
            `no matching not(feature = "${feat}") stub variant of the same fn name`,
        );
      }
    }
  }
}

console.log(`[verify-perf-gate-stubs] scanned: ${BENCH_FILE}`);
console.log(`[verify-perf-gate-stubs] group members: ${groupMembers.length}`);
console.log(
  `[verify-perf-gate-stubs] opt-in features referenced by ON gates: ${
    optInFeaturesSeen.size ? [...optInFeaturesSeen].sort().join(', ') : '(none)'
  }`,
);

if (problems.length === 0) {
  console.log(
    `\n[verify-perf-gate-stubs] PASS — every opt-in-feature-gated group member has a ` +
      `not(feature = ...) stub (or is gated only on always-on production features)`,
  );
  process.exit(0);
} else {
  console.log(`\n[verify-perf-gate-stubs] FAIL — ${problems.length} problem(s):`);
  for (const p of problems) console.log(`  - ${p}`);
  console.log(
    `\n  Fix: add a #[cfg(all(target_os = "linux", not(feature = "X")))] no-op stub\n` +
      `  (see the R24-8/R29-16 stub blocks in benches/perf_gate_iai.rs for the pattern).`,
  );
  process.exit(1);
}
