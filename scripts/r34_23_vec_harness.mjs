// R34-23 (task #542): real-Vec subprocess-isolated harness.
//
// Launches the three `r34_23_vec_worker_{sefer,mimalloc,system}` binaries as
// FRESH subprocesses, each with its OWN `#[global_allocator]`, in rotating
// alternation (same cyclic-order discipline as `r34_7_causal_harness.mjs`).
// Each worker runs a real `Vec<u8>` through `.push()`/`.shrink_to_fit()`/
// `.reserve_exact()` and emits `RESULT elapsed_ns=...` per shape per rep.
//
// This is deliverable #2 of R34-23: it measures what a REAL program pays for
// Vec growth through each allocator, with causal isolation (one process per
// arm — the R32/R33 §"P1" non-causality critique does not apply). The
// cross-allocator ratio on `growth_4mib` is the primary re-verification of
// the README's "~40× faster than mimalloc" / "~1,500×" realloc headline
// claims under real-Vec + subprocess-isolation conditions.
//
// USAGE:
//   node scripts/r34_23_vec_harness.mjs               # full: 8 reps × 3 allocators
//   node scripts/r34_23_vec_harness.mjs --quick        # smoke: 3 reps
//   node scripts/r34_23_vec_harness.mjs --iterations 8 # override timed rounds per process
//   node scripts/r34_23_vec_harness.mjs --reps 12      # override repetition count

import { execFileSync, spawn } from 'node:child_process';
import { writeFileSync, mkdirSync, unlinkSync } from 'node:fs';
import { cpus } from 'node:os';
import { createHash } from 'node:crypto';
import { REPO_ROOT, run } from './lib.mjs';

const isWin = process.platform === 'win32';

// ── CLI ────────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const quick = args.includes('--quick');
const iterArg = args.find((a, i) => args[i - 1] === '--iterations');
const repsArg = args.find((a, i) => args[i - 1] === '--reps');
const ITERATIONS = iterArg ? Number(iterArg) : quick ? 3 : 5;
const REPS = repsArg ? Number(repsArg) : quick ? 3 : 8;

const ALLOCATORS = ['sefer', 'mimalloc', 'system'];
const SHAPES = ['growth_4mib', 'growth_1mib', 'shrink_grow_1mib', 'reserve_exact_geom'];
const OUT_DIR = `${REPO_ROOT}/docs/perf/r34_23_runs`;

// ── Provenance ─────────────────────────────────────────────────────────────
function gitCommit() {
  try { return execFileSync('git', ['rev-parse', 'HEAD'], { cwd: REPO_ROOT }).toString().trim(); }
  catch { return 'unknown'; }
}
function gitDirty() {
  try { return execFileSync('git', ['status', '--porcelain'], { cwd: REPO_ROOT }).toString().trim().length > 0; }
  catch { return null; }
}
function gitWriteTree() {
  try {
    const tmpIndex = `r34_23_vec_tmp_${Date.now()}`;
    execFileSync('git', ['add', '-A'], {
      cwd: REPO_ROOT,
      env: { ...process.env, GIT_INDEX_FILE: `${REPO_ROOT}/.git/${tmpIndex}` },
      stdio: 'pipe',
    });
    const tree = execFileSync('git', ['write-tree'], {
      cwd: REPO_ROOT,
      env: { ...process.env, GIT_INDEX_FILE: `${REPO_ROOT}/.git/${tmpIndex}` },
    }).toString().trim();
    try { unlinkSync(`${REPO_ROOT}/.git/${tmpIndex}`); } catch {}
    return tree;
  } catch {
    try {
      const diff = execFileSync('git', ['diff', 'HEAD'], { cwd: REPO_ROOT }).toString();
      return 'patch:' + createHash('sha256').update(diff).digest('hex').slice(0, 16);
    } catch { return 'unknown'; }
  }
}
function rustcVersion() {
  try { return execFileSync('rustc', ['--version', '--verbose']).toString().trim(); }
  catch { return 'unavailable'; }
}
function cpuModel() {
  const c = cpus();
  return c.length > 0 ? `${c[0].model} (${c.length} cores)` : 'unknown';
}

// ── Binary path ────────────────────────────────────────────────────────────
function exePath(name) {
  const exe = isWin ? `${name}.exe` : name;
  const targetDir = process.env.CARGO_TARGET_DIR
    ? process.env.CARGO_TARGET_DIR.replace(/\\/g, '/')
    : `${REPO_ROOT}/target`;
  return `${targetDir}/release/examples/${exe}`;
}

// ── Build ──────────────────────────────────────────────────────────────────
async function buildWorkers() {
  console.log('[r34-23-vec] building 3 worker binaries (release, production alloc-stats)...');
  const exampleFlags = ALLOCATORS.map((a) => ['--example', `r34_23_vec_worker_${a}`]).flat();
  const { code } = await run('cargo', [
    'build', '--release', ...exampleFlags, '--features', 'production alloc-stats',
  ], { cwd: REPO_ROOT });
  if (code !== 0) throw new Error(`cargo build failed (exit ${code})`);
  console.log('[r34-23-vec] build OK');
}

// ── Run one worker subprocess ──────────────────────────────────────────────
function runQuiet(cmd, cmdArgs) {
  return new Promise((res, rej) => {
    const child = spawn(cmd, cmdArgs, { cwd: REPO_ROOT });
    let out = '';
    child.stdout.on('data', (buf) => { out += buf.toString(); });
    child.stderr.on('data', (buf) => { out += buf.toString(); });
    child.on('error', rej);
    child.on('close', (code) => res({ code: code ?? 1, out }));
  });
}

// ── Parse RESULT lines ─────────────────────────────────────────────────────
function parseResults(raw) {
  const lines = [];
  for (const line of raw.split(/\r?\n/)) {
    const m = /^RESULT\s+([a-z0-9_]+)=(\S+)$/.exec(line.trim());
    if (m) lines.push({ key: m[1], val: m[2] });
  }
  return lines;
}

/// Group the flat RESULT key=val lines from one worker run into per-shape
/// records. The worker emits shape/rep/elapsed_ns/realloc_count in sequence
/// (one shape's `iterations` reps, then the next shape), then the oracle
/// deltas at the end. We reconstruct: a record starts when we see `shape=`.
function groupWorkerOutput(kv) {
  const shapes = [];
  let cur = null;
  let oracle = { inplace_large: 0, inplace_small: 0, decline: 0 };
  let arm = null, iterations = null, rss = 0, commit = 0, segs = 0;
  for (const { key, val } of kv) {
    const num = /^-?\d+(\.\d+)?$/.test(val) ? Number(val) : val;
    if (key === 'arm') arm = val;
    else if (key === 'iterations') iterations = num;
    else if (key === 'rss_bytes') rss = num;
    else if (key === 'commit_bytes') commit = num;
    else if (key === 'segments_reserved_total') segs = num;
    else if (key === 'shape') { cur = { shape: val, elapsed_ns: 0, realloc_count: 0 }; shapes.push(cur); }
    else if (key === 'rep') { /* rep tracked by order within shape */ }
    else if (key === 'elapsed_ns' && cur) cur.elapsed_ns = num;
    else if (key === 'realloc_count' && cur) cur.realloc_count = num;
    else if (key === 'oracle_inplace_large_delta') oracle.inplace_large = num;
    else if (key === 'oracle_inplace_small_delta') oracle.inplace_small = num;
    else if (key === 'oracle_decline_delta') oracle.decline = num;
  }
  return { arm, iterations, rss, commit, segs, shapes, oracle };
}

// ── Stats ──────────────────────────────────────────────────────────────────
function median(arr) {
  const s = [...arr].sort((a, b) => a - b);
  const n = s.length;
  return n % 2 === 1 ? s[Math.floor(n / 2)] : (s[n / 2 - 1] + s[n / 2]) / 2;
}
function mean(arr) { return arr.reduce((a, b) => a + b, 0) / arr.length; }
function stdev(arr) {
  if (arr.length < 2) return 0;
  const m = mean(arr);
  return Math.sqrt(arr.reduce((s, v) => s + (v - m) ** 2, 0) / (arr.length - 1));
}

// ── Alternation (cyclic rotation per rep) ──────────────────────────────────
function orderForRep(rep) {
  const start = rep % ALLOCATORS.length;
  return ALLOCATORS.map((_, i) => ALLOCATORS[(start + i) % ALLOCATORS.length]);
}

// ── Main ───────────────────────────────────────────────────────────────────
async function main() {
  mkdirSync(OUT_DIR, { recursive: true });

  const writeTree = gitWriteTree();
  const identity = {
    git_commit: gitCommit(),
    git_dirty: gitDirty(),
    git_write_tree: writeTree,
    rustc_version_verbose: rustcVersion(),
    cpu_model: cpuModel(),
    platform: process.platform,
    iterations: ITERATIONS,
    reps: REPS,
  };

  console.log('╔══════════════════════════════════════════════════════════════════╗');
  console.log('║  R34-23 Real-Vec Subprocess Gate — fresh process per arm        ║');
  console.log('╚══════════════════════════════════════════════════════════════════╝');
  console.log(`  git HEAD:     ${identity.git_commit}`);
  console.log(`  git dirty:    ${identity.git_dirty}`);
  console.log(`  git tree:     ${identity.git_write_tree}`);
  console.log(`  CPU:          ${identity.cpu_model}`);
  console.log(`  iterations:   ${ITERATIONS} timed rounds/shape/process`);
  console.log(`  reps:         ${REPS}`);
  console.log();

  await buildWorkers();

  // ── Sanity: one launch per allocator ───────────────────────────────────
  console.log('── Sanity check ──────────────────────────────────────────────────');
  for (const alloc of ALLOCATORS) {
    const { code, out } = await runQuiet(exePath(`r34_23_vec_worker_${alloc}`), ['--iterations', '1']);
    if (code !== 0) throw new Error(`${alloc} sanity exited ${code}\n${out}`);
    const g = groupWorkerOutput(parseResults(out));
    const ok = alloc === 'sefer' ? g.segs > 0 : g.segs === 0;
    if (!ok) throw new Error(`${alloc} sanity: segments_reserved_total=${g.segs} unexpected`);
    console.log(`  ${alloc.padEnd(8)}: ✓ (segs=${g.segs})`);
  }
  console.log();

  // ── Measurement: fresh subprocess per (allocator, rep) ─────────────────
  console.log('── Measurement (fresh subprocess per launch) ─────────────────────');
  // raw[shape][allocator] = array of per-rep median elapsed_ns
  const rawSamples = []; // {alloc, rep, shape, elapsed_ns, realloc_count}
  const oracleByAlloc = { sefer: [], mimalloc: [], system: [] };

  for (let rep = 0; rep < REPS; rep++) {
    const order = orderForRep(rep);
    for (const alloc of order) {
      const { code, out } = await runQuiet(
        exePath(`r34_23_vec_worker_${alloc}`),
        ['--iterations', String(ITERATIONS)],
      );
      if (code !== 0) throw new Error(`${alloc} rep ${rep} exited ${code}\n${out}`);
      const g = groupWorkerOutput(parseResults(out));
      oracleByAlloc[alloc].push(g.oracle);
      for (const s of g.shapes) {
        rawSamples.push({ alloc, rep, shape: s.shape, elapsed_ns: s.elapsed_ns, realloc_count: s.realloc_count });
      }
      process.stdout.write('.');
    }
    process.stdout.write(` [rep ${rep + 1}/${REPS}]\n`);
  }
  console.log();

  // ── Write raw per-sample JSON ──────────────────────────────────────────
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  const rawFile = `${OUT_DIR}/${ts}_vec_raw.json`;
  writeFileSync(rawFile, JSON.stringify({
    timestamp: new Date().toISOString(),
    task: 'R34-23-vec',
    identity,
    samples: rawSamples,
  }, null, 2));
  console.log(`[r34-23-vec] raw JSON: ${rawFile}`);

  // ── Derive summary: median elapsed_ns per (shape, allocator) ───────────
  // Cross-allocator ratio table + summary CSV (derived, not hand-transcribed).
  const summary = {}; // summary[shape][alloc] = {median, min, max, mean, stdev, cv, reallocs_median}
  for (const shape of SHAPES) {
    summary[shape] = {};
    for (const alloc of ALLOCATORS) {
      const vals = rawSamples
        .filter((s) => s.shape === shape && s.alloc === alloc)
        .map((s) => s.elapsed_ns);
      const reallocs = rawSamples
        .filter((s) => s.shape === shape && s.alloc === alloc)
        .map((s) => s.realloc_count);
      summary[shape][alloc] = {
        median: median(vals), min: Math.min(...vals), max: Math.max(...vals),
        mean: mean(vals), stdev: stdev(vals),
        cv: mean(vals) > 0 ? (stdev(vals) / mean(vals)) * 100 : 0,
        reallocs_median: reallocs.length > 0 ? median(reallocs) : 0,
        n: vals.length,
      };
    }
  }

  // ── Cross-allocator ratio table ───────────────────────────────────────
  console.log('\n── Cross-allocator median elapsed_ns (real Vec) ──────────────────');
  console.log('shape,sefer,mimalloc,system,sefer/mi,sefer/sys,reallocs(sefer)');
  const csvLines = ['shape,allocator,samples,median_ns,min_ns,max_ns,mean_ns,stdev_ns,cv_pct,realloc_count_median'];
  for (const shape of SHAPES) {
    const s = summary[shape];
    const smi = s.mimalloc.median > 0 ? (s.sefer.median / s.mimalloc.median).toFixed(2) : '-';
    const ssys = s.system.median > 0 ? (s.sefer.median / s.system.median).toFixed(2) : '-';
    console.log(`${shape},${s.sefer.median},${s.mimalloc.median},${s.system.median},${smi},${ssys},${s.sefer.reallocs_median}`);
    for (const alloc of ALLOCATORS) {
      csvLines.push([shape, alloc, s[alloc].n, s[alloc].median, s[alloc].min, s[alloc].max,
        Math.round(s[alloc].mean), Math.round(s[alloc].stdev), s[alloc].cv.toFixed(1),
        s[alloc].reallocs_median].join(','));
    }
  }
  const csvFile = `${REPO_ROOT}/docs/perf/R34_23_REAL_VEC_summary.csv`;
  writeFileSync(csvFile, csvLines.join('\n') + '\n');
  console.log(`\n[r34-23-vec] summary CSV: ${csvFile}`);

  // ── Oracle summary (sefer) ─────────────────────────────────────────────
  console.log('\n── Path-activation oracle (sefer, summed over all shapes per rep) ──');
  const oracleSum = { inplace_large: 0, inplace_small: 0, decline: 0 };
  for (const o of oracleByAlloc.sefer) {
    oracleSum.inplace_large += o.inplace_large;
    oracleSum.inplace_small += o.inplace_small;
    oracleSum.decline += o.decline;
  }
  const total = oracleSum.inplace_large + oracleSum.inplace_small + oracleSum.decline;
  const pct = total > 0 ? (((oracleSum.inplace_large + oracleSum.inplace_small) / total) * 100).toFixed(1) : '-';
  console.log(`  inplace_large=${oracleSum.inplace_large}  inplace_small=${oracleSum.inplace_small}  decline=${oracleSum.decline}  inplace_pct=${pct}`);
  console.log();
}

main().catch((e) => {
  console.error(`\n[r34-23-vec] FAIL -- ${e.message}`);
  process.exit(1);
});
