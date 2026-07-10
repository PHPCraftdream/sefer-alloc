// Deterministic instruction-count (Ir) + cache-aware cycle perf judge via WSL
// + Valgrind/Callgrind.
//
// Why this exists: the crate's `iai-callgrind` perf gate
// (benches/perf_gate_iai.rs) is Linux-only — it compiles a normal binary, then
// iai-callgrind's runner drives it under `valgrind --tool=callgrind` and counts
// CPU *instructions retired* (`Ir`). Callgrind's Ir count is deterministic
// run-to-run on the same binary+input regardless of host contention, so on this
// Windows dev host we can PROVE a perf change (via WSL) instead of waiting for
// Linux CI. Wall-clock on Windows is noise; Ir is the judge.
//
// WHY CYCLES, NOT JUST Ir (task X3 / #184): Callgrind's cache simulation is ON,
// so the runner also prints L1 Hits, L2 (last-level) Hits, RAM Hits, and
// "Estimated Cycles" per bench (`= L1 + 5·L2 + 35·RAM`, callgrind's default
// cost model). Ir alone is BLIND to a critical regression class: it counts a
// `udiv` (a pure-ALU instruction) and a cache-MISSING load (a multi-hundred-
// cycle stall) IDENTICALLY — both retire as exactly 1 Ir. A change that swaps
// an in-register divide for an extra uncached memory access can be Ir-neutral
// or even Ir-NEGATIVE while being a large real-world pessimization; conversely
// a memcpy-elimination (X1's in-place Large realloc) can look modest in Ir but
// collapse in cycles. Estimated Cycles surfaces exactly that: the X-arc's
// memcpy floors show up as ~22× the cycle gap Ir alone reports (19×), because
// the cache-miss cost of the copies is invisible to Ir but real in cycles. We
// surface BOTH: Ir stays the PASS/FAIL judge (it is the deterministic,
// threshold-friendly metric CI regresses on); the cache columns are best-effort
// SIGNAL (a missing column must NOT fail the run — older callgrind builds, or a
// bench run with `--cache-sim=no`, simply omit them; we print "-" instead).
//
// Usage (from repo root):
//   node scripts/iai.mjs                          # all perf_gate benches
//   node scripts/iai.mjs cold_alloc_free_256x16b  # filter to one/some benches
//   node scripts/iai.mjs --features 'production hardened'  # override features
//   npm run iai
//
// Requires: WSL with valgrind + a cargo toolchain. It installs
// `iai-callgrind-runner` into WSL on first use (pinned to the `iai-callgrind`
// lib version from Cargo.toml — 0.14 → ^0.14, resolves to 0.14.2).
//
// Traps this encapsulates (mirrored from scripts/tsan.mjs):
//   1. RUSTC_WRAPPER / CARGO_BUILD_RUSTC_WRAPPER are inherited from the Windows
//      environment into WSL and point at `sccache.exe` — a Windows binary that
//      cannot drive the Linux rustc. We set both to empty strings on the cargo
//      process (cargo treats empty RUSTC_WRAPPER as "no wrapper").
//   2. A dedicated Linux target dir (/tmp/sefer-iai) so this never collides
//      with the Windows `target/` (different object format) or a Windows build.
//   3. The runner binary version MUST match the `iai-callgrind` LIB version in
//      Cargo.toml, else the runner refuses to run. We pin the same caret as CI.

import { REPO_ROOT, winToWsl, run } from './lib.mjs';

// Must match Cargo.toml's `iai-callgrind = "0.14"` (caret). CI installs the
// runner with `--version "^0.14"`; we mirror that exactly. ^0.14 resolves to
// the newest 0.14.x (0.14.2 at time of writing) — RUNNER_VER_PREFIX is what we
// grep the installed runner's `--version` against to decide "already installed,
// skip re-install" (keeps reruns fast).
const RUNNER_VERSION_REQ = '^0.14';
const RUNNER_VER_PREFIX = '0.14.';

// The bench's `required-features = ["alloc-global"]`, but the CI perf-gate
// (.github/workflows/perf-gate.yml) benches with `--features production`
// (alloc-global + alloc-xthread + alloc-decommit + fastbin) — the real-world
// default whose magazine/fastbin + large-cache fast paths are the whole point
// of the gate. We match CI so the Ir baseline we record here is the SAME number
// CI will produce. All eleven bench functions compile under `production`.
//
// X7-Ф5 (task #193): a `--features <set>` CLI override was added so the
// hardened-tier cost table can be recorded WITHOUT forking the script. The
// override is backward-compatible: with no `--features` arg, `production`
// remains the default (CI / `npm run iai` behaviour is byte-identical). The
// override ONLY changes which feature set cargo compiles + callgrind measures;
// the bench binary, the runner, the parser, and the report format are all
// feature-agnostic.
const DEFAULT_FEATURES = 'production';
const FEATURES_OVERRIDE_FLAG = '--features';

const BENCH = 'perf_gate_iai';
const LINUX_TARGET = '/tmp/sefer-iai';

// --- Marginal Ir/op decomposition (review finding F2, 2026-07-09) ---------
//
// WHY THIS COLUMN EXISTS. Every iai bench function builds a fresh `SeferAlloc`
// in its OWN process (`SeferAlloc::new()` at the top of each bench in
// benches/perf_gate_iai.rs), so EVERY raw Ir number includes the FULL one-time
// heap bootstrap (registry + primordial reserve + 32 KiB bitmap-init +
// Tcache-zero). That constant DOMINATES the small-op-count benches: the
// performance review (docs/reviews/2026-07-09-performance-review.md §F2)
// measured `small_churn_16b` = 81,423 Ir of which ~90 % is bootstrap, vs
// `cold_alloc_free_256x16b` = 125,354 Ir of which only ~58 % is. Consequence:
// a nominal "≤ +1 % Ir" threshold is 2–10× SOFTER or HARDER per-op depending
// purely on how much of a given bench's sum is the shared constant — the same
// headline percent measures wildly different per-operation strictness.
//
// The fix the review recommends (F2 rec. 1): subtract the bootstrap constant
// and divide by the bench's real operation count, giving a "marginal Ir/op"
// that is directly comparable ACROSS benches and is the honest unit to phrase
// future GO/NO-GO thresholds in.
//
// BOOTSTRAP PROXY. `large_alloc_free_cycle` issues exactly ONE Large segment
// and frees it — it touches NO magazine, NO small-class carve, NO freelist.
// Its entire Ir is therefore (bootstrap + the cost of one Large alloc+free),
// which is the cleanest bootstrap proxy the existing bench set offers (the
// same decomposition the X7 hardened-tier table in docs/perf/IAI_BASELINE.md
// already uses). We do NOT add a new "bootstrap-only" bench: that would add
// CPU time to the callgrind judge for no signal the proxy does not already
// give. We treat `large_alloc_free_cycle`'s raw Ir as the constant `B`; the
// proxy is a slight OVER-estimate of pure bootstrap (it includes one Large
// op-pair, a few hundred Ir), which makes the marginal column a mild
// LOWER-bound on per-op cost — the conservative direction for a regression
// guard. `large_alloc_free_cycle` itself has no meaningful "marginal per-op"
// (it IS the constant) and is reported as "-".
//
// OP COUNTS. The divisor is the number of alloc+free op-PAIRS each bench
// performs, derived DIRECTLY from the constants in benches/perf_gate_iai.rs
// (CHURN_OPS=64, COLD_BATCH=256, the 2-round recycle/multiseg loops,
// SEGCYCLE_ROUNDS×SEGCYCLE_BATCH). These mirror the bench source; if a bench's
// loop bounds change there, update the matching entry here. A bench absent from
// this map (or mapped to null) prints "-" in the marginal column — it is a
// best-effort SIGNAL column and MUST NOT affect pass/fail (Ir stays the judge).
const BOOTSTRAP_BENCH = 'large_alloc_free_cycle';
const BENCH_OPS = {
  // churn family: CHURN_OPS (64) alloc→dealloc pairs.
  small_churn_16b: 64,
  aligned_churn_640b_a128: 64,
  churn_256b: 64,
  churn_write_256b: 64,
  // the bootstrap proxy itself — 1 pair, but it DEFINES the constant, so its
  // marginal figure is meaningless; reported as "-" (mapped null explicitly).
  large_alloc_free_cycle: null,
  // realloc_grow: 1 initial alloc + 16 reallocs + 1 dealloc. The 16 growth
  // reallocs are the measured work (the C2 realloc-grow path); we use 16 as
  // the op count so the marginal figure is "Ir per realloc-grow step".
  realloc_grow: 16,
  // cold first-touch: COLD_BATCH (256) distinct blocks, one alloc + one free
  // each = 256 pairs.
  cold_alloc_free_256x16b: 256,
  cold_alloc_free_256x64b: 256,
  // recycle: 2 rounds × COLD_BATCH (256) = 512 pairs.
  recycle_alloc_free_256x16b: 512,
  recycle_alloc_free_256x64b: 512,
  // multiseg: 2 rounds × MULTISEG_BATCH (34) = 68 pairs.
  multiseg_cold_256k: 68,
  // seg-cycle decommit: SEGCYCLE_ROUNDS (6) × SEGCYCLE_BATCH (34) = 204 pairs.
  seg_cycle_decommit_256k: 204,
};
const wslRoot = winToWsl(REPO_ROOT);

// Optional CLI filter: bench-function name substrings. iai-callgrind 0.14 does
// NOT support runtime bench-name filtering — passing a name after `--` is
// silently swallowed and matches nothing (verified: `cargo bench --bench
// perf_gate_iai -- cold_alloc_free_256x16b` produces zero output). So we ALWAYS
// run the whole group (the full 11-bench run is only ~6s under callgrind) and
// filter the REPORTED rows here instead. A row is kept if any CLI arg is a
// substring of its name. No args → report all.
//
// X7-Ф5: the `--features <set>` flag is parsed out FIRST (it takes the next
// argv token as its value) and is NOT treated as a bench-name filter.
// Remaining args (after the flag + its value are removed) are the bench-name
// substring filters, exactly as before.
const rawArgs = process.argv.slice(2);
let featuresOverride = null;
const filterArgs = [];
for (let i = 0; i < rawArgs.length; i++) {
  if (rawArgs[i] === FEATURES_OVERRIDE_FLAG) {
    if (i + 1 >= rawArgs.length) {
      console.error(
        `[iai] ${FEATURES_OVERRIDE_FLAG} requires a value (e.g. --features 'production hardened')`,
      );
      process.exit(2);
    }
    featuresOverride = rawArgs[i + 1];
    i++; // consume the value
  } else {
    filterArgs.push(rawArgs[i]);
  }
}
const FEATURES = featuresOverride ?? DEFAULT_FEATURES;
const filters = filterArgs;
const wanted = (name) =>
  filters.length === 0 || filters.some((f) => name.includes(f));

/** One `bash -lc` line: env scrub + cargo bench, sharing a login shell. */
function benchCmd() {
  return [
    `cd ${wslRoot}`,
    'unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER',
    [
      'RUSTC_WRAPPER=',
      'CARGO_BUILD_RUSTC_WRAPPER=',
      `CARGO_TARGET_DIR=${LINUX_TARGET}`,
      'cargo bench',
      `--bench ${BENCH}`,
      `--features '${FEATURES}'`,
    ].join(' '),
  ].join(' && ');
}

/**
 * Ensure iai-callgrind-runner is installed in WSL at the required version.
 * Fast path: if `iai-callgrind-runner --version` already reports a 0.14.x, skip
 * the (slow) install entirely. Returns the installed version string, or throws.
 */
async function ensureRunner() {
  // NOTE: the runner prints its version to STDERR and exits non-zero for a
  // bare `--version` ("No version information found for iai-callgrind ... but
  // iai-callgrind-runner (0.14.2)") — it is a dispatch shim, not a normal CLI.
  // So we must NOT redirect stderr away, and we grep the version out of that
  // diagnostic. `command -v` first distinguishes "not installed" from "installed
  // but errored on --version".
  const probe = await run('wsl', [
    'bash',
    '-lc',
    'command -v iai-callgrind-runner >/dev/null 2>&1 && iai-callgrind-runner --version 2>&1 || echo __MISSING__',
  ]);
  const cur = probe.out.trim();
  const m = /(\d+\.\d+\.\d+)/.exec(cur);
  if (m && cur.includes(RUNNER_VER_PREFIX)) {
    console.log(`[iai] runner already installed: ${m[1]} (skip install)`);
    return m[1];
  }
  if (m && !cur.includes(RUNNER_VER_PREFIX)) {
    console.log(
      `[iai] runner ${m[1]} present but does not match ${RUNNER_VER_PREFIX}x; reinstalling`,
    );
  } else {
    console.log('[iai] runner not installed; installing');
  }
  // --locked: use the crate's committed Cargo.lock so a transitive dep does not
  // silently bump. Mirrors the CI `cargo install ... --version "^0.14" --locked`.
  const inst = await run('wsl', [
    'bash',
    '-lc',
    [
      'unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER',
      `RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= cargo install iai-callgrind-runner --version "${RUNNER_VERSION_REQ}" --locked`,
    ].join(' && '),
  ]);
  const after = await run('wsl', [
    'bash',
    '-lc',
    'command -v iai-callgrind-runner >/dev/null 2>&1 && iai-callgrind-runner --version 2>&1 || echo __MISSING__',
  ]);
  const m2 = /(\d+\.\d+\.\d+)/.exec(after.out.trim());
  if (!m2 || !after.out.includes(RUNNER_VER_PREFIX)) {
    throw new Error(
      `runner install failed (install exit ${inst.code}); ` +
        `version probe: "${after.out.trim()}"`,
    );
  }
  console.log(`[iai] runner installed: ${m2[1]}`);
  return m2[1];
}

/**
 * Parse iai-callgrind stdout into per-bench metric rows.
 *
 * iai-callgrind prints one block per bench, headed by a line like
 *   `perf_gate_iai::perf_gate::cold_alloc_free_256x16b ...`
 * followed by an indented metrics table whose rows look like
 *   `  Instructions:              12345|N/A     (...)`   (first run, no baseline)
 *   `  Instructions:              12345|12345   (No change)` (with baseline)
 * With cache-sim ON (the runner default), the same block also emits rows right
 * after `Instructions:`:
 *   `  L1 Hits:                   141802|N/A    (...)`
 *   `  L2 Hits:                   64|N/A        (...)`
 *   `  RAM Hits:                  5201|N/A      (...)`
 *   `  Estimated Cycles:          324157|N/A    (...)`
 * The FUNCTION NAME is the last `::`-separated segment of the header. We take
 * the first numeric column (the current run's absolute count) of each row,
 * which is baseline-independent. `Ir` is the "Instructions" metric; the four
 * cache rows are best-effort (older callgrind / `--cache-sim=no` omit them —
 * the caller treats a missing cache field as "-" and must NOT fail on it).
 *
 * Returns an array of `{ name, ir, l1, l2, ram, cycles }` where the cache
 * fields are `null` when the corresponding row was absent for that bench.
 */
function parseMetrics(out) {
  const lines = out.split(/\r?\n/);
  const results = [];
  let current = null;
  const headerRe = /^([A-Za-z_][\w]*::)+([A-Za-z_]\w*)\b/;
  // Each metric row: leading whitespace, a fixed label, then the first integer
  // (with optional thousands separators) is the current-run absolute count.
  const rowRe = (label) =>
    new RegExp(`^\\s*${label}:\\s*([\\d,]+)`);
  const instrRe = rowRe('Instructions');
  const l1Re = rowRe('L1 Hits');
  const l2Re = rowRe('L2 Hits');
  const ramRe = rowRe('RAM Hits');
  const cycRe = rowRe('Estimated Cycles');
  const num = (m) => (m ? Number(m[1].replace(/,/g, '')) : null);
  for (const line of lines) {
    const h = headerRe.exec(line);
    if (h) {
      // A new bench header finalizes any in-progress row that never saw an
      // Instructions line (defensive — should not happen, but keeps a malformed
      // block from corrupting the next one).
      current = h[2];
      // Defer push until we see Instructions (the produced-signal gate).
      continue;
    }
    const im = instrRe.exec(line);
    if (im && current) {
      const ir = Number(im[1].replace(/,/g, ''));
      if (Number.isFinite(ir)) {
        results.push({
          name: current,
          ir,
          l1: null,
          l2: null,
          ram: null,
          cycles: null,
        });
        // Keep `current` pointing at this bench so the cache rows that follow
        // (no header between them and the Instructions row) attach to it. The
        // next header line resets `current`.
      }
      continue;
    }
    // Cache rows attach to the most-recently produced bench.
    if (current && results.length) {
      const last = results[results.length - 1];
      if (last.name === current) {
        let m;
        if ((m = l1Re.exec(line))) last.l1 = num(m);
        else if ((m = l2Re.exec(line))) last.l2 = num(m);
        else if ((m = ramRe.exec(line))) last.ram = num(m);
        else if ((m = cycRe.exec(line))) last.cycles = num(m);
      }
    }
  }
  // De-dupe by name, keep first occurrence (one block per bench).
  const seen = new Set();
  return results.filter((r) => (seen.has(r.name) ? false : seen.add(r.name)));
}

/** Format an integer with thousands separators, or "-" for null/undefined. */
function fmt(n) {
  return n == null || !Number.isFinite(n) ? '-' : n.toLocaleString('en-US');
}

/**
 * Marginal Ir per operation for one bench, given the bootstrap constant `B`
 * (the `large_alloc_free_cycle` raw Ir). Returns `(ir - B) / ops`, rounded to
 * one decimal, or `null` when the bench has no op-count entry, when it IS the
 * bootstrap proxy, or when `B` is unknown (bootstrap bench absent/filtered).
 * See the F2 block near the top of this file for the full rationale.
 */
function marginalIrPerOp(row, bootstrap) {
  if (bootstrap == null || !Number.isFinite(bootstrap)) return null;
  const ops = BENCH_OPS[row.name];
  if (ops == null || ops <= 0) return null;
  return Math.round(((row.ir - bootstrap) / ops) * 10) / 10;
}

/** Format a marginal Ir/op value (one decimal), or "-" for null. */
function fmtMarginal(n) {
  return n == null || !Number.isFinite(n)
    ? '-'
    : n.toLocaleString('en-US', {
        minimumFractionDigits: 1,
        maximumFractionDigits: 1,
      });
}

function printTable(rows, bootstrap) {
  if (!rows.length) {
    console.log('[iai] (no Ir parsed)');
    return;
  }
  const w = Math.max(...rows.map((r) => r.name.length), 'bench'.length);
  // Column widths sized for the largest value seen (realloc_grow ~1.5M Ir,
  // ~7.2M cycles) so the table stays aligned across all benches.
  const cw = 12;
  const head = (s) => s.padStart(cw);
  console.log(
    `\n  ${'bench'.padEnd(w)}  ${head('Ir')}  ${head('L1')}  ${head('L2')}  ${head('RAM')}  ${head('EstCycles')}  ${head('Ir/op*')}`,
  );
  console.log(
    `  ${'-'.repeat(w)}  ${'-'.repeat(cw)}  ${'-'.repeat(cw)}  ${'-'.repeat(cw)}  ${'-'.repeat(cw)}  ${'-'.repeat(cw)}  ${'-'.repeat(cw)}`,
  );
  for (const r of rows) {
    const marg = marginalIrPerOp(r, bootstrap);
    console.log(
      `  ${r.name.padEnd(w)}  ${fmt(r.ir).padStart(cw)}  ${fmt(r.l1).padStart(cw)}  ${fmt(r.l2).padStart(cw)}  ${fmt(r.ram).padStart(cw)}  ${fmt(r.cycles).padStart(cw)}  ${fmtMarginal(marg).padStart(cw)}`,
    );
  }
  // Footnote: make the column's meaning + the constant used unmissable in the
  // raw report (a reader diffing two runs must know this is bootstrap-adjusted,
  // not a raw metric). See finding F2.
  if (bootstrap != null && Number.isFinite(bootstrap)) {
    console.log(
      `\n  * Ir/op = (Ir − ${fmt(bootstrap)}) / ops — marginal instruction count per\n` +
        `    operation, with the one-time process bootstrap subtracted (constant taken\n` +
        `    from ${BOOTSTRAP_BENCH}). Comparable across benches; the honest unit for\n` +
        `    per-op thresholds (review finding F2). "-" = bootstrap proxy / no op-count.`,
    );
  } else {
    console.log(
      `\n  * Ir/op omitted: ${BOOTSTRAP_BENCH} (the bootstrap constant) was not in this\n` +
        `    run (filtered out?). Run without a filter, or include ${BOOTSTRAP_BENCH}.`,
    );
  }
}

console.log(`[iai] wsl: ${wslRoot}`);
console.log(`[iai] features: ${FEATURES} | target: ${LINUX_TARGET}`);
if (filters.length) console.log(`[iai] filter: ${filters.join(', ')}`);

let ok = true;
try {
  await ensureRunner();
  console.log('\n[iai] running benches...\n');
  const { code, out } = await run('wsl', ['bash', '-lc', benchCmd()]);
  const compileErr = /^error(\[|:)/m.test(out);
  const allRows = parseMetrics(out);
  const rows = allRows.filter((r) => wanted(r.name));
  // Bootstrap constant for the marginal Ir/op column is taken from the FULL
  // (unfiltered) run, so the column still works when the user filters the
  // report down to benches that exclude `large_alloc_free_cycle`. Null if the
  // proxy bench was not produced at all (e.g. a filter passed to cargo).
  const bootstrapRow = allRows.find((r) => r.name === BOOTSTRAP_BENCH);
  const bootstrap = bootstrapRow ? bootstrapRow.ir : null;
  printTable(rows, bootstrap);

  // For a MEASUREMENT tool, "pass" = it ran and produced an Ir for every
  // requested bench. A compile error, a missing runner, or a requested bench
  // that produced no Ir is a FAIL. With no filter, "requested" = all benches in
  // the group, so we simply require at least one parsed row (the group is
  // non-empty). With a filter, every filter term must have matched a row.
  const unmatched = filters.filter(
    (f) => !allRows.some((r) => r.name.includes(f)),
  );
  const gotAll = rows.length > 0 && unmatched.length === 0;
  ok = !compileErr && code === 0 && gotAll;
  if (ok) {
    console.log(`\n[iai] PASS — ${rows.length} bench(es) produced Ir`);
  } else {
    console.log(
      '\n[iai] FAIL' +
        (compileErr ? ' (compile error)' : '') +
        (code !== 0 ? ` (cargo exit ${code})` : '') +
        (unmatched.length ? ` (no bench matched: ${unmatched.join(', ')})` : '') +
        (!rows.length && !unmatched.length ? ' (no Ir parsed)' : ''),
    );
  }
} catch (e) {
  ok = false;
  console.log(`\n[iai] FAIL — ${e.message}`);
}
process.exit(ok ? 0 : 1);
