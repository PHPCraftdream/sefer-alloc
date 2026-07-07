// Canonical wall-clock comparison tables for `benches/global_alloc.rs`
// (SeferAlloc vs mimalloc vs System).
//
// Why this exists: every time a human asked "show me the comparative
// benchmarks", the answer came back in a DIFFERENT shape — sometimes raw
// µs-per-iteration-batch, sometimes ns-per-single-op, sometimes a subset of
// benches, sometimes with the vs-mimalloc ratio and sometimes without. The
// µs/batch vs ns/op confusion once looked like a 20ns->40ns regression that
// was actually just a unit mixup (µs for a 1024-op batch vs ns for one op).
// This script fixes the shape once: it runs the bench, parses criterion's
// stdout, and always prints the SAME four tables with the SAME units
// (ns per allocator operation) and the SAME vs-mimalloc ratio column, so a
// human comparing two runs (or two points in time) is comparing like for
// like. It is the wall-clock companion to `scripts/iai.mjs` (the
// deterministic instruction-count judge) — this one is inherently noisy
// (real wall-clock on a shared Windows host), iai.mjs is the tie-breaker
// when a wall-clock delta looks surprising.
//
// Usage (from repo root):
//   node scripts/bench-table.mjs
//   npm run bench:table
//
// Requires: a normal (non-WSL) `cargo bench` — this bench is plain criterion
// on the host toolchain, no Valgrind/WSL involved (unlike iai.mjs).

import { REPO_ROOT, run } from './lib.mjs';

const BENCH = 'global_alloc';
const FEATURES = 'production';

// Mirrors `benches/global_alloc.rs`'s own constants. OPS is the number of
// alloc/dealloc (or free+alloc churn) round-trips criterion's `b.iter`
// closure performs per measured iteration; dividing the per-iteration time
// by OPS gives a stable "ns per allocator operation" figure that is
// comparable across runs regardless of how many ops happen to be batched
// into one closure call. Vec_push's closure does a SINGLE alloc+dealloc
// pair (the growth loop's capacity jumps straight to its final size on the
// first growth — see the bench source) plus VEC_PUSHES stores, so it is
// reported as-is (one "op" = one whole closure call), not scaled by OPS.
const OPS = 1024;
const SIZES = ['16B', '64B', '256B', '1024B'];
const ARMS = ['SeferAlloc', 'mimalloc', 'System'];

// The three benchmark_group()s in benches/global_alloc.rs, in the order
// they're defined, each yielding a `group/{arm}/{size}` id plus (for
// `global_alloc` only) a `group/Vec_push/{arm}` id.
const SIZED_GROUPS = [
  { id: 'global_alloc', title: 'Cold-direct (`bench_direct_alloc`, no reuse — 1 alloc + 1 free per op)', scale: OPS },
  { id: 'global_alloc_churn', title: 'Churn, non-writing (`bench_churn_alloc`, working-set reuse — 1 free + 1 alloc per op)', scale: OPS },
  { id: 'global_alloc_churn_write', title: 'Churn + write (`bench_churn_alloc_write` — same as above, writes 16B after each alloc)', scale: OPS },
];

function unitToNs(value, unit) {
  const v = Number(value);
  switch (unit) {
    case 'ns': return v;
    case 'us':
    case 'µs': return v * 1e3;
    case 'ms': return v * 1e6;
    case 's': return v * 1e9;
    default: throw new Error(`unrecognized time unit: ${unit}`);
  }
}

/**
 * Parse criterion stdout into `{ id: "group/arm/size", ns }` entries, where
 * `ns` is the point-estimate (middle value) of criterion's `[lo mean hi]`
 * triple, converted to nanoseconds. Criterion prints the bench id either on
 * its own line right before `time:` (long ids) or on the same line (short
 * ids) — both forms are handled by tracking the last non-"Benchmarking"
 * line seen as the pending id.
 */
function parseBenchOutput(out) {
  const timeRe =
    /time:\s*\[\s*([\d.]+)\s*(ns|µs|us|ms|s)\s+([\d.]+)\s*(ns|µs|us|ms|s)\s+([\d.]+)\s*(ns|µs|us|ms|s)\s*\]/;
  const lines = out.split(/\r?\n/);
  const entries = [];
  let pendingId = null;
  for (const raw of lines) {
    const line = raw.trim();
    if (!line) continue;
    if (line.startsWith('Benchmarking')) continue;
    if (line.startsWith('Found') || line.startsWith('Performance')) continue;
    if (line.startsWith('change:') || line.startsWith('1 (') || line.startsWith('2 (')) continue;
    const m = timeRe.exec(line);
    if (m) {
      const idPart = line.slice(0, m.index).trim();
      const id = idPart || pendingId;
      if (id) {
        const meanNs = unitToNs(m[3], m[4]);
        entries.push({ id, ns: meanNs });
      }
      pendingId = null;
      continue;
    }
    // A bare id line (no colon, no brackets) — remember it as pending.
    if (!line.includes(':') && !line.includes('[')) {
      pendingId = line;
    }
  }
  const byId = new Map();
  for (const e of entries) byId.set(e.id, e.ns); // last write wins (final measured value)
  return byId;
}

function fmtNs(ns) {
  if (ns == null) return '-';
  return ns.toFixed(1);
}

function ratio(seferNs, mimallocNs) {
  if (seferNs == null || mimallocNs == null) return '-';
  const r = seferNs / mimallocNs;
  return r <= 1 ? `**${(1 / r).toFixed(2)}× faster**` : `${r.toFixed(2)}× slower`;
}

function printSizedTable(title, id, scale, byId) {
  console.log(`\n### ${title}\n`);
  console.log('| Size | SeferAlloc (ns/op) | mimalloc (ns/op) | System (ns/op) | Sefer vs mimalloc |');
  console.log('|---|---:|---:|---:|---:|');
  for (const size of SIZES) {
    const vals = {};
    for (const arm of ARMS) {
      const raw = byId.get(`${id}/${arm}/${size}`);
      vals[arm] = raw == null ? null : raw / scale;
    }
    console.log(
      `| ${size} | ${fmtNs(vals.SeferAlloc)} | ${fmtNs(vals.mimalloc)} | ${fmtNs(vals.System)} | ${ratio(vals.SeferAlloc, vals.mimalloc)} |`,
    );
  }
}

function printVecPushTable(byId) {
  console.log('\n### Vec_push (amortized `Vec<i64>` growth — one alloc+dealloc pair + stores per op, NOT scaled)\n');
  console.log('| SeferAlloc (ns/op) | mimalloc (ns/op) | System (ns/op) | Sefer vs mimalloc |');
  console.log('|---:|---:|---:|---:|');
  const vals = {};
  for (const arm of ARMS) {
    vals[arm] = byId.get(`global_alloc/Vec_push/${arm}`) ?? null;
  }
  console.log(
    `| ${fmtNs(vals.SeferAlloc)} | ${fmtNs(vals.mimalloc)} | ${fmtNs(vals.System)} | ${ratio(vals.SeferAlloc, vals.mimalloc)} |`,
  );
}

console.log(`[bench-table] repo: ${REPO_ROOT}`);
console.log(`[bench-table] bench: ${BENCH} | features: ${FEATURES}`);
console.log(
  '[bench-table] wall-clock is inherently noisy on a shared host (criterion sample_size(10)); ' +
    'treat this as a directional signal and cross-check surprising deltas against `npm run iai` ' +
    '(deterministic instruction count).',
);
console.log('\n[bench-table] running... (this takes a couple of minutes)\n');

let ok = true;
try {
  const { code, out } = await run(
    'cargo',
    ['bench', '--features', FEATURES, '--bench', BENCH],
    { cwd: REPO_ROOT, shell: true },
  );
  const compileErr = /^error(\[|:)/m.test(out);
  const byId = parseBenchOutput(out);

  console.log('\n\n============================================================');
  console.log(`  ${BENCH} — comparative wall-clock tables (ns per operation)`);
  console.log('============================================================');

  for (const g of SIZED_GROUPS) {
    printSizedTable(g.title, g.id, g.scale, byId);
  }
  printVecPushTable(byId);

  const expectedIds = [
    ...SIZED_GROUPS.flatMap((g) => SIZES.flatMap((s) => ARMS.map((a) => `${g.id}/${a}/${s}`))),
    ...ARMS.map((a) => `global_alloc/Vec_push/${a}`),
  ];
  const missing = expectedIds.filter((id) => !byId.has(id));
  ok = !compileErr && code === 0 && missing.length === 0;
  if (ok) {
    console.log(`\n[bench-table] PASS — ${byId.size} bench id(s) parsed, all ${expectedIds.length} expected ids present`);
  } else {
    console.log(
      '\n[bench-table] FAIL' +
        (compileErr ? ' (compile error)' : '') +
        (code !== 0 ? ` (cargo exit ${code})` : '') +
        (missing.length ? ` (missing ids: ${missing.join(', ')})` : ''),
    );
  }
} catch (e) {
  ok = false;
  console.log(`\n[bench-table] FAIL — ${e.message}`);
}
process.exit(ok ? 0 : 1);
