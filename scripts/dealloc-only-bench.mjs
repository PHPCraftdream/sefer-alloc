// R6-OPT-A5 / Stage A.5: process-per-sample judge for "a thread that only
// ever FREES a foreign block, never allocates".
//
// WHY THIS EXISTS. `SeferAlloc::dealloc` (src/global/sefer_alloc.rs) calls
// `current_heap()` unconditionally, binding a FULL per-thread heap (registry
// slot claim + 4 MiB primordial segment reserve/commit) on a thread's FIRST
// EVER allocator call — whether that call is an `alloc` or a `dealloc`. This
// runner builds `examples/dealloc_only_unbound_thread.rs` once, then launches
// it many times as SEPARATE processes (thread-local heap bindings persist for
// a thread's lifetime, so "a never-before-bound thread" can only be tested
// fresh per sample — see the example's module doc), across a (B, T, mode)
// matrix, parses each run's `RESULT key=value` lines, and aggregates.
//
// The core deliverable: the delta between `mode=treatment` (worker's first
// call is the foreign free) and `mode=control` (worker allocs+frees once
// first, forcing the well-understood bind-via-alloc path, THEN frees the
// foreign block). Pre-fix, both bind identically via `current_heap()`, so
// this should show CONVERGENCE, not a large gap — see the example's module
// doc for why a large pre-fix gap would indicate a harness bug.
//
// Usage (from repo root):
//   node scripts/dealloc-only-bench.mjs              # quick matrix, 5 samples/cell
//   node scripts/dealloc-only-bench.mjs --full        # full (B,T) cross product
//   node scripts/dealloc-only-bench.mjs --samples 10  # override sample count
//   npm run dealloc-only                                # if wired in package.json

import { REPO_ROOT, run } from './lib.mjs';

const FEATURES = 'alloc-global';
const EXAMPLE = 'dealloc_only_unbound_thread';

const isWin = process.platform === 'win32';

const args = process.argv.slice(2);
const full = args.includes('--full');
const samplesArg = args.find((a, i) => args[i - 1] === '--samples');
// Fast default per repo convention ("benchmarks run fast by default"): a
// handful of process-per-sample launches per cell is enough to see the
// spread without turning this into a multi-minute run (each sample is a
// full process spawn + thread spawn/join, not a cheap in-process iteration).
const SAMPLES = samplesArg ? Number(samplesArg) : 5;

// Quick (default) matrix: representative slice of B x T, both modes.
// Full matrix (--full): the complete task-specified B x {1,64,400,4096} x
// T x {1,8,64,512,4096} x mode cross product — much slower (4096-thread
// cells spawn+join 4096 real OS threads per sample).
const QUICK_CELLS = [
  { b: 1, t: 1 },
  { b: 64, t: 8 },
  { b: 400, t: 64 },
  { b: 4096, t: 512 },
];
const FULL_B = [1, 64, 400, 4096];
const FULL_T = [1, 8, 64, 512, 4096];
const CELLS = full
  ? FULL_B.flatMap((b) => FULL_T.map((t) => ({ b, t })))
  : QUICK_CELLS;
const MODES = ['treatment', 'control'];

function parseResult(out) {
  const r = {};
  for (const line of out.split(/\r?\n/)) {
    const m = /^RESULT\s+([a-z0-9_]+)=(\w+)$/.exec(line.trim());
    if (m) r[m[1]] = /^\d+$/.test(m[2]) ? Number(m[2]) : m[2];
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
  return { min: Math.min(...xs), med: median(xs), max: Math.max(...xs) };
}

function fmtKib(n) {
  if (n == null) return '-';
  return `${n.toLocaleString('en-US')} KiB`;
}

function fmtNs(n) {
  if (n == null) return '-';
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(3)} ms`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(3)} us`;
  return `${n} ns`;
}

async function buildExample() {
  console.log(`[dealloc-only] building example '${EXAMPLE}' (features: ${FEATURES})...`);
  const { code } = await run(
    'cargo',
    ['build', '--release', '--example', EXAMPLE, '--features', FEATURES],
    { cwd: REPO_ROOT, shell: isWin },
  );
  if (code !== 0) throw new Error(`cargo build failed (exit ${code})`);
}

function exePath() {
  const name = isWin ? `${EXAMPLE}.exe` : EXAMPLE;
  const targetDir = process.env.CARGO_TARGET_DIR
    ? process.env.CARGO_TARGET_DIR.replace(/\\/g, '/')
    : `${REPO_ROOT}/target`;
  return `${targetDir}/release/examples/${name}`;
}

async function runOnce(b, t, mode) {
  const { out } = await run(exePath(), [String(b), String(t), mode], { cwd: REPO_ROOT });
  return parseResult(out);
}

async function runCell(b, t, mode) {
  const rssDelta = [];
  const rssJoinDelta = [];
  const commitDelta = [];
  const commitJoinDelta = [];
  const firstLatency = [];
  const steadyLatency = [];
  let lastHighWater = null;
  let lastSegDelta = null;

  for (let i = 0; i < SAMPLES; i++) {
    const r = await runOnce(b, t, mode);
    if (r.rss_before_kib != null && r.rss_after_kib != null) {
      rssDelta.push(r.rss_after_kib - r.rss_before_kib);
    }
    if (r.rss_before_kib != null && r.rss_after_join_kib != null) {
      rssJoinDelta.push(r.rss_after_join_kib - r.rss_before_kib);
    }
    if (r.commit_before_kib != null && r.commit_after_kib != null) {
      commitDelta.push(r.commit_after_kib - r.commit_before_kib);
    }
    if (r.commit_before_kib != null && r.commit_after_join_kib != null) {
      commitJoinDelta.push(r.commit_after_join_kib - r.commit_before_kib);
    }
    if (r.first_dealloc_latency_ns != null) firstLatency.push(r.first_dealloc_latency_ns);
    if (r.steady_dealloc_latency_ns != null) steadyLatency.push(r.steady_dealloc_latency_ns);
    if (r.heaps_claimed_high_water != null) lastHighWater = r.heaps_claimed_high_water;
    if (r.segments_reserved_before != null && r.segments_reserved_after != null) {
      lastSegDelta = r.segments_reserved_after - r.segments_reserved_before;
    }
    process.stdout.write('.');
  }

  return {
    b,
    t,
    mode,
    rss: stat(rssDelta),
    rssJoin: stat(rssJoinDelta),
    commit: stat(commitDelta),
    commitJoin: stat(commitJoinDelta),
    first: stat(firstLatency),
    steady: stat(steadyLatency),
    highWater: lastHighWater,
    segDelta: lastSegDelta,
  };
}

console.log(
  `[dealloc-only] host: ${process.platform} | samples/cell: ${SAMPLES} | cells: ${CELLS.length * MODES.length} | matrix: ${full ? 'full' : 'quick'}`,
);

let ok = true;
try {
  await buildExample();
  console.log('\n[dealloc-only] sampling fresh processes...\n');

  const results = [];
  for (const { b, t } of CELLS) {
    for (const mode of MODES) {
      const res = await runCell(b, t, mode);
      results.push(res);
      process.stdout.write(` [b=${b} t=${t} mode=${mode}]\n`);
    }
  }

  console.log(
    '\n  b      t      mode        first-dealloc(min/med/max)        steady-dealloc(min/med/max)        RSS-Δ-join(min/med/max)              commit-Δ-join(min/med/max)           high-water  seg-Δ',
  );
  console.log('  ' + '-'.repeat(200));
  for (const r of results) {
    console.log(
      `  ${String(r.b).padEnd(6)} ${String(r.t).padEnd(6)} ${r.mode.padEnd(11)} ` +
        `${fmtNs(r.first.min)} / ${fmtNs(r.first.med)} / ${fmtNs(r.first.max)}`.padEnd(36) +
        `  ${fmtNs(r.steady.min)} / ${fmtNs(r.steady.med)} / ${fmtNs(r.steady.max)}`.padEnd(38) +
        `  ${fmtKib(r.rssJoin.min)} / ${fmtKib(r.rssJoin.med)} / ${fmtKib(r.rssJoin.max)}`.padEnd(
          40,
        ) +
        `  ${fmtKib(r.commitJoin.min)} / ${fmtKib(r.commitJoin.med)} / ${fmtKib(r.commitJoin.max)}`.padEnd(
          40,
        ) +
        `  ${r.highWater}  ${r.segDelta}`,
    );
  }

  // ── Treatment vs control comparison (the core deliverable) ──────────────
  console.log('\n  === treatment vs control: first-op latency + commit-Δ (retained, post-join) ===');
  for (const { b, t } of CELLS) {
    const treat = results.find((r) => r.b === b && r.t === t && r.mode === 'treatment');
    const ctrl = results.find((r) => r.b === b && r.t === t && r.mode === 'control');
    if (!treat || !ctrl) continue;
    const latRatio =
      treat.first.med && ctrl.first.med ? (treat.first.med / ctrl.first.med).toFixed(2) : '-';
    const commitRatio =
      treat.commitJoin.med && ctrl.commitJoin.med
        ? (treat.commitJoin.med / ctrl.commitJoin.med).toFixed(2)
        : 'n/a (near-zero baseline)';
    console.log(
      `  b=${b} t=${t}: first-dealloc treatment=${fmtNs(treat.first.med)} vs control=${fmtNs(ctrl.first.med)} (ratio ${latRatio}x) | ` +
        `commit-Δ-join treatment=${fmtKib(treat.commitJoin.med)} vs control=${fmtKib(ctrl.commitJoin.med)} (ratio ${commitRatio})`,
    );
  }
  console.log(
    '\n  * PRE-fix expectation: ratios close to 1x (both treatment and control bind a full\n' +
      '    heap via current_heap() on their first-ever allocator call, regardless of whether\n' +
      '    that call was alloc or dealloc). A large ratio would indicate a harness bug, not\n' +
      '    a real pre-fix effect — see examples/dealloc_only_unbound_thread.rs module doc.\n' +
      '    AFTER a future P0-1 fix, this ratio is exactly the ratio expected to widen.',
  );

  ok = results.length > 0;
  console.log(ok ? `\n[dealloc-only] PASS — ${results.length} cells sampled` : '\n[dealloc-only] FAIL — no results parsed');
} catch (e) {
  ok = false;
  console.log(`\n[dealloc-only] FAIL — ${e.message}`);
}
process.exit(ok ? 0 : 1);
