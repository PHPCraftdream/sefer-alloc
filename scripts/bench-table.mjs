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
// into one closure call. Vec_push's closure does honest geometric Vec growth
// (capacity doubles 4→8→…→512, i.e. 8 grow steps of alloc-new + copy-old +
// dealloc-old) plus VEC_PUSHES stores per closure call; it is reported as-is
// (one "op" = one whole closure call), not scaled by OPS.
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
  // Appended last (not inserted in registration order) so the three original
  // sized tables keep their byte-for-byte position/format. DELIBERATE
  // diagnostic: teardown stays INSIDE the timed region (the pre-F7/F9
  // behavior), unlike the three groups above.
  { id: 'global_alloc_churn_with_teardown', title: 'Churn + teardown (`bench_global_alloc_churn_with_teardown`, DELIBERATE diagnostic — teardown stays inside timed region; the gap vs `global_alloc_churn` at the same size IS the segment decommit/release/re-reserve cost, see `benches/global_alloc.rs:628-637`)', scale: OPS },
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
 * Parse criterion stdout into a `Map<id, { ns, changePct, verdict }>` where
 * `ns` is the point-estimate (middle value) of criterion's `[lo mean hi]`
 * time triple, converted to nanoseconds; `changePct` is the point-estimate
 * (middle) of criterion's `change:` triple `[lo mid hi]%` (null when no
 * saved baseline exists in `target/criterion`, so no `change:` line was
 * emitted — e.g. the first run in a clean target/); `verdict` is
 * criterion's verdict string (e.g. "Performance has regressed.",
 * "No change in performance detected."). The `change:`/verdict pair is
 * associated with the immediately-preceding `time:` entry's id — criterion
 * emits them in that exact order (verified against real output: `id` →
 * `time:` → `change:` → verdict, with the "Found N outliers" bookkeeping
 * coming AFTER the verdict). Criterion prints the bench id either on its
 * own line right before `time:` (long ids) or on the same line (short ids);
 * both forms are handled by tracking the last bare line seen as pending.
 */
function parseBenchOutput(out) {
  const timeRe =
    /time:\s*\[\s*([\d.]+)\s*(ns|µs|us|ms|s)\s+([\d.]+)\s*(ns|µs|us|ms|s)\s+([\d.]+)\s*(ns|µs|us|ms|s)\s*\]/;
  const changeRe =
    /change:\s*\[\s*([-+]?[\d.]+)%\s+([-+]?[\d.]+)%\s+([-+]?[\d.]+)%\s*\]\s*\(p\s*=\s*([\d.]+)\s*([<>])\s*([\d.]+)\s*\)/;
  const verdictRe = /^(Performance has (?:improved|regressed)\.|No change in performance detected\.)/;
  const lines = out.split(/\r?\n/);
  const entries = [];
  let pendingId = null;
  let lastEntry = null; // most recent `time:` entry — owns the next change:/verdict
  let awaitingVerdict = false; // set after a `change:` line, cleared by the verdict line
  for (const raw of lines) {
    const line = raw.trim();
    if (!line) continue;
    if (line.startsWith('Benchmarking')) continue;
    if (line.startsWith('Found') || line.startsWith('1 (') || line.startsWith('2 (')) continue;
    const cm = changeRe.exec(line);
    if (cm) {
      if (lastEntry) lastEntry.changePct = Number(cm[2]); // middle of [lo mid hi]
      awaitingVerdict = true;
      continue;
    }
    if (awaitingVerdict && verdictRe.test(line)) {
      if (lastEntry) lastEntry.verdict = line;
      awaitingVerdict = false;
      continue;
    }
    if (line.startsWith('Performance')) continue; // stray verdict with no captured change:
    const m = timeRe.exec(line);
    if (m) {
      const idPart = line.slice(0, m.index).trim();
      const id = idPart || pendingId;
      if (id) {
        const meanNs = unitToNs(m[3], m[4]);
        const entry = { id, ns: meanNs, changePct: null, verdict: null };
        entries.push(entry);
        lastEntry = entry;
      }
      pendingId = null;
      awaitingVerdict = false;
      continue;
    }
    // A bare id line (no colon, no brackets) — remember it as pending.
    if (!line.includes(':') && !line.includes('[')) {
      pendingId = line;
    }
  }
  const byId = new Map();
  for (const e of entries) byId.set(e.id, e); // last write wins (final measured value)
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
      vals[arm] = raw == null ? null : raw.ns / scale;
    }
    console.log(
      `| ${size} | ${fmtNs(vals.SeferAlloc)} | ${fmtNs(vals.mimalloc)} | ${fmtNs(vals.System)} | ${ratio(vals.SeferAlloc, vals.mimalloc)} |`,
    );
  }
}

function printVecPushTable(byId) {
  console.log('\n### Vec_push (honest geometric `Vec<i64>` growth — 8 grow steps + stores per op, NOT scaled)\n');
  console.log('| SeferAlloc (ns/op) | mimalloc (ns/op) | System (ns/op) | Sefer vs mimalloc |');
  console.log('|---:|---:|---:|---:|');
  const vals = {};
  for (const arm of ARMS) {
    vals[arm] = byId.get(`global_alloc/Vec_push/${arm}`)?.ns ?? null;
  }
  console.log(
    `| ${fmtNs(vals.SeferAlloc)} | ${fmtNs(vals.mimalloc)} | ${fmtNs(vals.System)} | ${ratio(vals.SeferAlloc, vals.mimalloc)} |`,
  );
}

// `segment_decommit_cycle` — 3 arms but a SINGLE fixed size (253 KiB),
// measured as one `b.iter` batch of 34 allocs + 34 frees (`SEG_BATCH = 34`).
// There is no established ops-per-iteration constant for this group (unlike
// the OPS-scaled sized groups), so it is reported UNSCALED — "ns per batch" —
// mirroring how `printVecPushTable` documents its own unscaled numbers. Shape
// is the same single-row 3-arm form as `printVecPushTable`.
function printSegmentDecommitCycleTable(byId) {
  console.log(
    '\n### `segment_decommit_cycle` (253 KiB small-segment decommit→release→re-reserve; UNSCALED — ns per batch of 34 alloc + 34 free, `bench_segment_decommit_cycle`)\n',
  );
  console.log('| SeferAlloc (ns/batch) | mimalloc (ns/batch) | System (ns/batch) | Sefer vs mimalloc |');
  console.log('|---:|---:|---:|---:|');
  const vals = {};
  for (const arm of ARMS) {
    vals[arm] = byId.get(`segment_decommit_cycle/${arm}/253KiB`)?.ns ?? null;
  }
  console.log(
    `| ${fmtNs(vals.SeferAlloc)} | ${fmtNs(vals.mimalloc)} | ${fmtNs(vals.System)} | ${ratio(vals.SeferAlloc, vals.mimalloc)} |`,
  );
}

// `working_set_cycle` — SeferAlloc-ONLY (the doc comment at
// `benches/global_alloc.rs:673-676` explains why mimalloc/System are absent:
// this measures Sefer's specific decommit/reuse lifecycle, which the others
// don't share). 4 sizes, no ratio column possible. Each iteration processes
// `N_WORKING_SETS` (64) working sets each doing one free+alloc oscillation
// per block; no established single-op scale factor is defined in the bench,
// so reported UNSCALED — "ns per batch".
function printWorkingSetCycleTable(byId) {
  console.log(
    '\n### `working_set_cycle` (Mechanism-2 decommit/reuse judge — SeferAlloc-ONLY, no mimalloc/System arm; UNSCALED — ns per batch of 64 working sets × one free+alloc oscillation per block, `bench_working_set_cycle`)\n',
  );
  console.log('| Size | SeferAlloc (ns/batch) |');
  console.log('|---|---:|');
  for (const size of SIZES) {
    const ns = byId.get(`working_set_cycle/SeferAlloc/${size}`)?.ns ?? null;
    console.log(`| ${size} | ${fmtNs(ns)} |`);
  }
}

// `pool_cap_sweep` — diagnostic-only. Its own doc comment
// (`benches/global_alloc.rs:932-942`) states the sweep's signal is the
// `eprintln!` counter deltas (decommit_calls per swept cap), not the
// criterion timing table — the spread/drain construction cost swamps the
// drain. Building a real timing table here would fabricate a misleading
// number the bench's own author calls not meaningful. Its ids are NOT added
// to `expectedIds` (nor are the two groups above) — they were never gated,
// and adding them would create a new failure mode for a diagnostic.
function printPoolCapSweepNote() {
  console.log(
    '\n### `pool_cap_sweep` — diagnostic-only, intentionally excluded from wall-clock comparison\n',
  );
  console.log(
    "Its own doc comment (`benches/global_alloc.rs:932-942`) states the sweep's signal is the " +
      '`eprintln!` counter deltas (decommit_calls per swept cap), not the criterion timing table — ' +
      'the spread/drain construction cost swamps the drain itself, so the criterion timing column ' +
      'is not meaningful here. Those deltas are emitted to stderr during the bench run and are not ' +
      'parsed by this script; read them in the raw `cargo bench` output if needed.',
  );
}

// Run-over-run appendix: surface criterion's `change:` signal that the old
// parser silently discarded. Empty on the first run against a clean
// `target/` (criterion needs a saved baseline to compute a change).
function printChangeAppendix(byId) {
  console.log(
    "\n---\n## Run-over-run change (vs `target/criterion`'s saved baseline, when one exists)\n",
  );
  const changed = [];
  for (const [, e] of byId) {
    if (e.changePct != null) changed.push(e);
  }
  if (changed.length === 0) {
    console.log(
      '_No `change:` lines were emitted by criterion this run — this happens on the first run ' +
        'against a clean `target/` (no saved baseline to compare against). Re-run `npm run bench:table` ' +
        'and this appendix will populate with the run-over-run deltas._',
    );
    return;
  }
  changed.sort((a, b) => a.id.localeCompare(b.id));
  console.log('| id | ns (point estimate) | change % (point estimate) | verdict |');
  console.log('|---|---:|---:|---|');
  for (const e of changed) {
    const sign = e.changePct > 0 ? '+' : '';
    const pct = `${sign}${e.changePct.toFixed(2)}%`;
    const verdict = e.verdict ?? '-';
    console.log(`| ${e.id} | ${fmtNs(e.ns)} | ${pct} | ${verdict} |`);
  }
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

  // --- Additional groups (not part of the canonical 4-table headline) ---
  // The three groups below are registered benchmark_group()s the old script
  // silently dropped. Each is printed with its own honest unit label (none
  // fits the canonical `{arm}/{size}` × 3-arm × scaled-by-OPS shape).
  console.log('\n---\n## Additional groups (not part of the canonical 4-table headline)\n');
  printSegmentDecommitCycleTable(byId);
  printWorkingSetCycleTable(byId);
  printPoolCapSweepNote();
  printChangeAppendix(byId);
} catch (e) {
  ok = false;
  console.log(`\n[bench-table] FAIL — ${e.message}`);
}
process.exit(ok ? 0 : 1);
