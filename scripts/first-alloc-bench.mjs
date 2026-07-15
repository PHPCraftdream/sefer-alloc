// RAD-1 / Phase 0(a): process-per-sample first-alloc RSS + latency judge.
//
// WHY THIS EXISTS. The registry bootstrap (src/registry/bootstrap.rs) writes
// `next_free` into all MAX_HEAPS=4096 slots at a 7040 B stride on the FIRST
// allocation of the process, dirtying ~4096 distinct pages (~16 MiB demand-zero
// RSS) exactly once per process. Criterion and the iai bench run many
// iterations inside ONE long-lived process, so after the first iteration every
// page is already resident and the first-touch cost is invisible to them. The
// only way to measure it is to sample a FRESH process each time.
//
// This runner builds `examples/first_alloc_process.rs` once, then launches it N
// times as SEPARATE processes, parses each run's `RESULT key=value` lines, and
// aggregates (min / median / max) the headline metrics:
//
//   - rss_after_1_heap_kib − rss_before_kib  (the bootstrap first-touch RSS)
//   - first_alloc_latency_ns
//   - the 8-heap and 64-heap RSS growth
//
// It runs NATIVELY on the host (Windows or Linux) — RSS is read from the OS in
// the example itself (K32GetProcessMemoryInfo on Windows, /proc/self/statm on
// Linux), so no WSL/valgrind is needed (unlike scripts/iai.mjs).
//
// Usage (from repo root):
//   node scripts/first-alloc-bench.mjs           # 15 samples (default)
//   node scripts/first-alloc-bench.mjs 40         # N samples
//   npm run first-alloc                            # if wired in package.json

import { REPO_ROOT, run } from './lib.mjs';

// MUST be the full `production` set: the registry footprint that exhibits the
// ~16 MiB `next_free` first-touch only manifests when the inline `HeapCore`
// includes the magazine (`fastbin`) + large-cache (`alloc-decommit`) state
// (~7.5 KiB/slot vs ~192 B without them). This is also the set CI / the iai gate
// use. See examples/first_alloc_process.rs module docs.
const FEATURES = 'production';
const EXAMPLE = 'first_alloc_process';

// Number of fresh-process samples. Each sample is one full process launch;
// keep it modest by default (fast cycle per repo convention), overridable.
const DEFAULT_SAMPLES = 15;
const samplesArg = process.argv.slice(2).find((a) => /^\d+$/.test(a));
const SAMPLES = samplesArg ? Number(samplesArg) : DEFAULT_SAMPLES;

const isWin = process.platform === 'win32';

/** Parse `RESULT key=value` lines out of one run's stdout into an object. */
function parseResult(out) {
  const r = {};
  for (const line of out.split(/\r?\n/)) {
    const m = /^RESULT\s+([a-z0-9_]+)=(\d+)$/.exec(line.trim());
    if (m) r[m[1]] = Number(m[2]);
  }
  return r;
}

function median(xs) {
  if (!xs.length) return null;
  const s = [...xs].sort((a, b) => a - b);
  const mid = Math.floor(s.length / 2);
  return s.length % 2 ? s[mid] : Math.round((s[mid - 1] + s[mid]) / 2);
}

function stat(xs) {
  if (!xs.length) return { min: null, med: null, max: null };
  return {
    min: Math.min(...xs),
    med: median(xs),
    max: Math.max(...xs),
  };
}

function fmtKib(n) {
  if (n == null) return '-';
  return `${n.toLocaleString('en-US')} KiB (${(n / 1024).toFixed(2)} MiB)`;
}

function fmtNs(n) {
  if (n == null) return '-';
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(3)} ms`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(3)} µs`;
  return `${n} ns`;
}

async function buildExample() {
  console.log(
    `[first-alloc] building example '${EXAMPLE}' (features: ${FEATURES})...`,
  );
  // Quote the multi-word features string so a shell (Windows) does not split
  // it into two argv tokens (cargo then rejects the second word as an
  // unexpected positional argument).
  const { code } = await run(
    'cargo',
    [
      'build',
      '--release',
      '--example',
      EXAMPLE,
      '--features',
      isWin ? `"${FEATURES}"` : FEATURES,
    ],
    { cwd: REPO_ROOT, shell: isWin },
  );
  if (code !== 0) throw new Error(`cargo build failed (exit ${code})`);
}

function exePath() {
  const name = isWin ? `${EXAMPLE}.exe` : EXAMPLE;
  // Respect CARGO_TARGET_DIR (this repo's dev env redirects it to a shared
  // out-of-tree dir, e.g. D:\dev\rust\.cargo-target). Fall back to the
  // in-repo `target/` when it is unset.
  const targetDir = process.env.CARGO_TARGET_DIR
    ? process.env.CARGO_TARGET_DIR.replace(/\\/g, '/')
    : `${REPO_ROOT}/target`;
  return `${targetDir}/release/examples/${name}`;
}

async function runOnce() {
  // Run the prebuilt binary DIRECTLY (not `cargo run`) so each sample is a
  // clean, minimal process with no cargo/build machinery in its address space —
  // the RSS we measure is the example's own, not cargo's.
  const { out } = await run(exePath(), [], { cwd: REPO_ROOT });
  return parseResult(out);
}

console.log(`[first-alloc] host: ${process.platform} | samples: ${SAMPLES}`);
if (!isWin && process.platform !== 'linux') {
  console.log(
    '[first-alloc] NOTE: RSS is only read on Linux and Windows; on this\n' +
      '             platform rss_* fields will be 0 (latency is still valid).',
  );
}

let ok = true;
try {
  await buildExample();
  console.log('\n[first-alloc] sampling fresh processes...\n');

  const bootstrapRss = []; // rss_after_1_heap − rss_before
  const rss8 = []; //         rss_after_8_heaps − rss_before
  const rss64 = []; //        rss_after_64_heaps − rss_before
  const latency = [];
  // R6-OPT-A1 (Stage A judge fix): commit charge is a SEPARATE axis from RSS,
  // aggregated alongside it (not replacing it) the same min/median/max way.
  const bootstrapCommit = []; // commit_after_1_heap − commit_before
  const commit8 = []; //        commit_after_8_heaps − commit_before
  const commit64 = []; //       commit_after_64_heaps − commit_before
  let lastHighWater = null;

  for (let i = 0; i < SAMPLES; i++) {
    const r = await runOnce();
    if (r.rss_before_kib != null && r.rss_after_1_heap_kib != null) {
      bootstrapRss.push(r.rss_after_1_heap_kib - r.rss_before_kib);
    }
    if (r.rss_before_kib != null && r.rss_after_8_heaps_kib != null) {
      rss8.push(r.rss_after_8_heaps_kib - r.rss_before_kib);
    }
    if (r.rss_before_kib != null && r.rss_after_64_heaps_kib != null) {
      rss64.push(r.rss_after_64_heaps_kib - r.rss_before_kib);
    }
    if (r.commit_before_kib != null && r.commit_after_1_heap_kib != null) {
      bootstrapCommit.push(r.commit_after_1_heap_kib - r.commit_before_kib);
    }
    if (r.commit_before_kib != null && r.commit_after_8_heaps_kib != null) {
      commit8.push(r.commit_after_8_heaps_kib - r.commit_before_kib);
    }
    if (r.commit_before_kib != null && r.commit_after_64_heaps_kib != null) {
      commit64.push(r.commit_after_64_heaps_kib - r.commit_before_kib);
    }
    if (r.first_alloc_latency_ns != null) {
      latency.push(r.first_alloc_latency_ns);
    }
    if (r.heaps_claimed_high_water != null) {
      lastHighWater = r.heaps_claimed_high_water;
    }
    process.stdout.write('.');
  }
  process.stdout.write('\n\n');

  const b = stat(bootstrapRss);
  const s8 = stat(rss8);
  const s64 = stat(rss64);
  const l = stat(latency);
  const cb = stat(bootstrapCommit);
  const c8 = stat(commit8);
  const c64 = stat(commit64);

  console.log('  metric                                   min / median / max');
  console.log('  ---------------------------------------  ------------------------------------------------');
  console.log(
    `  RSS Δ  1 heap  (bootstrap first-touch)   ${fmtKib(b.min)}  /  ${fmtKib(b.med)}  /  ${fmtKib(b.max)}`,
  );
  console.log(
    `  RSS Δ  8 heaps                           ${fmtKib(s8.min)}  /  ${fmtKib(s8.med)}  /  ${fmtKib(s8.max)}`,
  );
  console.log(
    `  RSS Δ 64 heaps                           ${fmtKib(s64.min)}  /  ${fmtKib(s64.med)}  /  ${fmtKib(s64.max)}`,
  );
  console.log(
    `  first-alloc latency                      ${fmtNs(l.min)}  /  ${fmtNs(l.med)}  /  ${fmtNs(l.max)}`,
  );
  console.log(`\n  heaps_claimed_high_water (last run): ${lastHighWater}`);
  console.log(
    '\n  * "RSS Δ 1 heap" is the headline judge. BEFORE the RAD-1 lazy-`next_free`\n' +
      '    fix it INCLUDES the ~16 MiB registry `next_free` first-touch (4096 slots ×\n' +
      '    ~7488 B stride under `production` = 4096 distinct pages). AFTER the fix it\n' +
      '    collapses to the primordial-segment cost only (~0.1 MiB). Measured on this\n' +
      '    host: 16.11 MiB → 0.11 MiB; first-alloc latency 8.6 ms → 0.17 ms.',
  );

  // ── Commit charge (R6-OPT-A1: Stage A judge fix — NEW axis, not a replacement) ──
  console.log(
    '\n  metric (commit charge)                   min / median / max',
  );
  console.log('  ---------------------------------------  ------------------------------------------------');
  console.log(
    `  Commit Δ  1 heap  (bootstrap)             ${fmtKib(cb.min)}  /  ${fmtKib(cb.med)}  /  ${fmtKib(cb.max)}`,
  );
  console.log(
    `  Commit Δ  8 heaps                         ${fmtKib(c8.min)}  /  ${fmtKib(c8.med)}  /  ${fmtKib(c8.max)}`,
  );
  console.log(
    `  Commit Δ 64 heaps                         ${fmtKib(c64.min)}  /  ${fmtKib(c64.med)}  /  ${fmtKib(c64.max)}`,
  );
  console.log(
    '\n  * Commit charge (Windows `PagefileUsage` / Linux `/proc/self/statm` field 0,\n' +
      '    total VM size) is a SEPARATE axis from RSS, added here — it does NOT\n' +
      '    replace the RSS numbers above. WHY IT EXISTS (R6-OPT-A1, radical_\n' +
      '    optimization_review §4 P0-2 / §5.5 item 9 / §6 Stage A.3): on Windows,\n' +
      '    `crates/vmem` commits the FULL exact size of the Registry + inline\n' +
      '    `HeapOverflow` array in one `VirtualAlloc(MEM_COMMIT)` call, which is\n' +
      '    largely demand-zero and therefore invisible to RSS/`WorkingSetSize` until\n' +
      '    pages are actually touched — RSS alone hides this cost entirely. Expect\n' +
      '    "Commit Δ 1 heap" to be ~125 MiB LARGER than "RSS Δ 1 heap" (≈29 MiB\n' +
      '    registry + ≈96 MiB inline `HeapOverflow` across 4096 slots): that gap IS\n' +
      '    the metric this axis exists to surface, and is the quantity the follow-up\n' +
      '    task R6-OPT-P0-2 (chunked Registry + lazy HeapOverflow sidecar) is meant\n' +
      '    to shrink.',
  );

  ok = bootstrapRss.length > 0 || latency.length > 0;
  console.log(
    ok
      ? `\n[first-alloc] PASS — ${SAMPLES} samples collected`
      : '\n[first-alloc] FAIL — no results parsed',
  );
} catch (e) {
  ok = false;
  console.log(`\n[first-alloc] FAIL — ${e.message}`);
}
process.exit(ok ? 0 : 1);
