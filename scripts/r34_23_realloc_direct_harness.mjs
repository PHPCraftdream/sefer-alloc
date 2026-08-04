// R34-23 (task #542): direct-GlobalAlloc::realloc gate driver.
//
// Launches `r34_23_realloc_direct_gate` as a FRESH subprocess per allocator
// (sefer / mimalloc / system) — one process each, so cross-allocator state
// leakage is impossible by construction (the R32/R33 review's §"P1"
// non-causality critique does not apply: each allocator gets its own empty
// process). Each process runs every (pattern × payload) cell for `--samples`
// timed grow-chains and emits CSV-ish SAMPLE/CELL lines (raw per-sample data
// per CLAUDE.md R22-14). This script collects them, computes summary stats
// (median/min/max per cell), and writes:
//   - raw per-sample JSON (docs/perf/r34_23_runs/<ts>_direct_raw.json)
//   - summary CSV (docs/perf/R34_23_REALLOC_DIRECT_summary.csv)
//
// The summary CSV + the report's Markdown tables are DERIVED from the raw
// JSON by this one script (CLAUDE.md "tables derived by one checked script"),
// not hand-transcribed.
//
// USAGE:
//   node scripts/r34_23_realloc_direct_harness.mjs              # full: 3 allocators, 30 samples/cell
//   node scripts/r34_23_realloc_direct_harness.mjs --quick       # smoke: 5 samples/cell
//   node scripts/r34_23_realloc_direct_harness.mjs --samples 50  # override sample count

import { execFileSync, spawn } from 'node:child_process';
import { writeFileSync, mkdirSync, unlinkSync } from 'node:fs';
import { cpus } from 'node:os';
import { createHash } from 'node:crypto';
import { REPO_ROOT, run } from './lib.mjs';

const isWin = process.platform === 'win32';

// ── CLI ────────────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
const quick = args.includes('--quick');
const samplesArg = args.find((a, i) => args[i - 1] === '--samples');
const SAMPLES = samplesArg ? Number(samplesArg) : quick ? 5 : 30;

const ALLOCATORS = ['sefer', 'mimalloc', 'system'];
const OUT_DIR = `${REPO_ROOT}/docs/perf/r34_23_runs`;

// ── Provenance (immutable source identity, per R29-6) ──────────────────────
function gitCommit() {
  try { return execFileSync('git', ['rev-parse', 'HEAD'], { cwd: REPO_ROOT }).toString().trim(); }
  catch { return 'unknown'; }
}
function gitDirty() {
  try { return execFileSync('git', ['status', '--porcelain'], { cwd: REPO_ROOT }).toString().trim().length > 0; }
  catch { return null; }
}
function gitWriteTree() {
  // Captures the EXACT working-tree contents as a git tree object SHA — an
  // immutable identity that survives even if the working tree is later
  // mutated or the task's diff is discarded (R29-6 option 2).
  try {
    const tmpIndex = `r34_23_tmp_index_${Date.now()}`;
    // `git add -A` into a temp index to reflect the WORKING tree, then
    // `write-tree` to capture it as an immutable tree object.
    execFileSync('git', ['add', '-A'], {
      cwd: REPO_ROOT,
      env: { ...process.env, GIT_INDEX_FILE: `${REPO_ROOT}/.git/${tmpIndex}` },
      stdio: 'pipe',
    });
    const tree = execFileSync('git', ['write-tree'], {
      cwd: REPO_ROOT,
      env: { ...process.env, GIT_INDEX_FILE: `${REPO_ROOT}/.git/${tmpIndex}` },
    }).toString().trim();
    // Clean up the temp index file (best-effort).
    try { unlinkSync(`${REPO_ROOT}/.git/${tmpIndex}`); } catch {}
    return tree;
  } catch {
    // Fallback: patch hash over HEAD (R29-6 option 3).
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
function exePath() {
  const exe = isWin ? 'r34_23_realloc_direct_gate.exe' : 'r34_23_realloc_direct_gate';
  const targetDir = process.env.CARGO_TARGET_DIR
    ? process.env.CARGO_TARGET_DIR.replace(/\\/g, '/')
    : `${REPO_ROOT}/target`;
  return `${targetDir}/release/examples/${exe}`;
}

// ── Build ──────────────────────────────────────────────────────────────────
async function buildGate() {
  console.log('[r34-23-direct] building gate binary (release, features=production alloc-stats)...');
  const { code } = await run('cargo', [
    'build', '--release', '--example', 'r34_23_realloc_direct_gate',
    '--features', 'production alloc-stats',
  ], { cwd: REPO_ROOT });
  if (code !== 0) throw new Error(`cargo build failed (exit ${code})`);
  console.log('[r34-23-direct] build OK');
}

// ── Run one allocator subprocess, collect CSV lines ────────────────────────
function runAllocator(allocator, samples) {
  return new Promise((res, rej) => {
    const child = spawn(exePath(), ['--allocator', allocator, '--samples', String(samples)], { cwd: REPO_ROOT });
    let out = '';
    child.stdout.on('data', (buf) => { out += buf.toString(); });
    child.stderr.on('data', (buf) => { out += buf.toString(); });
    child.on('error', rej);
    child.on('close', (code) => {
      if (code !== 0) return rej(new Error(`${allocator} exited ${code}\n${out}`));
      res(out);
    });
  });
}

// ── Parse CSV-ish lines ────────────────────────────────────────────────────
function parseLines(raw, allocator) {
  const samples = [];
  const cells = [];
  for (const line of raw.split(/\r?\n/)) {
    const t = line.trim();
    if (t.startsWith('SAMPLE,')) {
      const [_, pattern, payload, rep, ns, rss, commit] = t.split(',');
      samples.push({
        allocator, pattern, payload,
        rep: Number(rep), ns_per_chain: Number(ns),
        rss_bytes_before: Number(rss), commit_bytes_before: Number(commit),
      });
    } else if (t.startsWith('CELL,')) {
      const parts = t.split(',');
      cells.push({
        allocator,
        pattern: parts[1], payload: parts[2], samples: Number(parts[3]),
        median_ns: Number(parts[4]), min_ns: Number(parts[5]), max_ns: Number(parts[6]),
        inplace_large_delta: Number(parts[7]),
        inplace_small_delta: Number(parts[8]),
        decline_delta: Number(parts[9]),
        rss_bytes_before: Number(parts[10]),
        commit_bytes_before: Number(parts[11]),
        rss_bytes_after: Number(parts[12]),
        commit_bytes_after: Number(parts[13]),
      });
    }
  }
  return { samples, cells };
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
    samples_per_cell: SAMPLES,
  };

  console.log('╔══════════════════════════════════════════════════════════════════╗');
  console.log('║  R34-23 Direct GlobalAlloc::realloc Gate — subprocess-per-arm   ║');
  console.log('╚══════════════════════════════════════════════════════════════════╝');
  console.log(`  git HEAD:     ${identity.git_commit}`);
  console.log(`  git dirty:    ${identity.git_dirty}`);
  console.log(`  git tree:     ${identity.git_write_tree}`);
  console.log(`  CPU:          ${identity.cpu_model}`);
  console.log(`  samples/cell: ${SAMPLES}`);
  console.log();

  await buildGate();

  const allSamples = [];
  const allCells = [];

  for (const alloc of ALLOCATORS) {
    console.log(`── Running ${alloc} (fresh subprocess) ──────────────────────────`);
    const raw = await runAllocator(alloc, SAMPLES);
    const { samples, cells } = parseLines(raw, alloc);
    allSamples.push(...samples);
    allCells.push(...cells);
    console.log(`  ${alloc}: ${samples.length} samples, ${cells.length} cells`);
  }

  // ── Write raw per-sample JSON ──────────────────────────────────────────
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  const rawFile = `${OUT_DIR}/${ts}_direct_raw.json`;
  writeFileSync(rawFile, JSON.stringify({
    timestamp: new Date().toISOString(),
    task: 'R34-23-direct',
    identity,
    samples: allSamples,
    cells: allCells,
  }, null, 2));
  console.log(`\n[r34-23-direct] raw JSON: ${rawFile}`);

  // ── Derive summary CSV from raw (one checked script) ───────────────────
  // Columns: allocator,pattern,payload,samples,median_ns,min_ns,max_ns,
  //          mean_ns,stdev_ns,cv_pct,inplace_large_delta,inplace_small_delta,
  //          decline_delta,rss_bytes_after,commit_bytes_after
  const csvLines = [
    'allocator,pattern,payload,samples,median_ns,min_ns,max_ns,mean_ns,stdev_ns,cv_pct,inplace_large_delta,inplace_small_delta,decline_delta,rss_bytes_after,commit_bytes_after',
  ];
  for (const c of allCells) {
    // Recompute mean/stdev/cv from raw samples for this cell (derived, not
    // hand-transcribed from the binary's CELL line — cross-check median).
    const cellRaw = allSamples.filter(
      (s) => s.allocator === c.allocator && s.pattern === c.pattern && s.payload === c.payload,
    ).map((s) => s.ns_per_chain);
    const m = mean(cellRaw);
    const sd = stdev(cellRaw);
    const cv = m > 0 ? (sd / m) * 100 : 0;
    csvLines.push([
      c.allocator, c.pattern, c.payload, c.samples,
      c.median_ns, c.min_ns, c.max_ns,
      Math.round(m), Math.round(sd), cv.toFixed(1),
      c.inplace_large_delta, c.inplace_small_delta, c.decline_delta,
      c.rss_bytes_after, c.commit_bytes_after,
    ].join(','));
  }
  const csvFile = `${REPO_ROOT}/docs/perf/R34_23_REALLOC_DIRECT_summary.csv`;
  writeFileSync(csvFile, csvLines.join('\n') + '\n');
  console.log(`[r34-23-direct] summary CSV: ${csvFile}`);

  // ── Cross-allocator ratio table (README re-verification focus) ─────────
  console.log('\n── Cross-allocator median ns/chain (README re-verification) ──────');
  console.log('pattern,payload,sefer,mimalloc,system,sefer/mi,sefer/sys');
  for (const pattern of [...new Set(allCells.map((c) => c.pattern))]) {
    for (const payload of [...new Set(allCells.map((c) => c.payload))]) {
      const get = (a) => allCells.find(
        (c) => c.allocator === a && c.pattern === pattern && c.payload === payload,
      )?.median_ns;
      const sefer = get('sefer'), mi = get('mimalloc'), sys = get('system');
      const smi = mi ? (sefer / mi).toFixed(2) : '-';
      const ssys = sys ? (sefer / sys).toFixed(2) : '-';
      console.log(`${pattern},${payload},${sefer ?? '-'},${mi ?? '-'},${sys ?? '-'},${smi},${ssys}`);
    }
  }

  // ── Path-activation oracle summary (sefer only) ────────────────────────
  console.log('\n── Path-activation oracle (sefer arm, alloc-stats deltas) ────────');
  console.log('pattern,payload,inplace_large,inplace_small,decline,inplace_pct');
  for (const c of allCells.filter((c) => c.allocator === 'sefer')) {
    const total = c.inplace_large_delta + c.inplace_small_delta + c.decline_delta;
    const pct = total > 0
      ? (((c.inplace_large_delta + c.inplace_small_delta) / total) * 100).toFixed(1)
      : '-';
    console.log(`${c.pattern},${c.payload},${c.inplace_large_delta},${c.inplace_small_delta},${c.decline_delta},${pct}`);
  }
  console.log();
}

main().catch((e) => {
  console.error(`\n[r34-23-direct] FAIL -- ${e.message}`);
  process.exit(1);
});
