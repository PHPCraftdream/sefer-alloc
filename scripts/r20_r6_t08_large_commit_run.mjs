// R6-T08 Large commit amplification gate RUNNER (r20-gates.yml stage runner).
// Executes the pipeline preregistered in
// docs/perf/R6_T08_LARGE_COMMIT_AMPLIFICATION_PREREG.md; the prereg is a
// frozen input and is never modified here.
//
// This is a SIMPLIFIED runner (modeled on scripts/r20_prf04_heap_reuse_run.mjs
// and scripts/r20_r6_ab_iai_run.mjs): subprocess wall-clock + proc-memstat,
// NOT iai-callgrind. The two arm feature-sets are hard-coded here (per the
// prereg) rather than carried in gate-run.json's single-string `features`
// field — no new config field is needed.
//
// Stages (invoke exactly one):
//   --prepare  record HEAD sha + clean-tree check into <out>/manifest.json
//              BEFORE any measurement; refuses to overwrite.
//   --build    build examples/r6_t08_large_commit_probe twice with cargo
//             directly (this platform's invocation, mirroring
//             r20_r6_ab_iai_run.mjs): features `production internals` (arm A)
//             and `production internals,exact-span-large` (arm B); `internals`
//             is added to BOTH arms purely for the dbg_* oracle hooks and
//             changes nothing measured — the arm's identity is
//             exact-span-large on/off. Each arm's executable is COPIED into
//             <out> as arm_<label>_probe.bin and the copied path + SHA256 are
//             recorded in builds.json, so the shared target/ dir can never
//             overwrite one arm's binary with the other's (no --target-dir
//             juggling; the shared target dir is used for both builds and the
//             copies are the immutable artifacts that --measure runs).
//   --measure  re-verifies the manifest identity (HEAD + clean tree), runs the
//             COPIED binaries from <out> (SHA256 re-checked against
//             builds.json), parses each CSV line into <out>/summary.csv,
//             evaluates the preregistered GO/NO-GO criterion (pure function,
//             see evaluateGoNoGo) into <out>/verdict.txt. Exits 1 on NO-GO.
//   --cleanup  remove <out> (under target/). MAINTENANCE ONLY: cleanup must
//             NOT appear in a results-producing .github/gate-run.json — it
//             deletes <out> (including verdict.txt) BEFORE the workflow could
//             upload artifacts. The recommended stage list is
//             'prepare,build,measure'.
//
// outDir: `target/r20_r6_t08_large_commit` (the committed .github/gate-run.json
// outDir regex only allows docs/perf/R20_* or target/r20[a-z0-9_-]* names; the
// simplest compliant choice is this target/ path, keeping docs/perf/ for the
// prereg and the human-readable report only). Overridable with --out-dir.
//
//   --selftest  pure in-memory checks ONLY (CSV parser, amplification math,
//               GO/NO-GO evaluation on synthetic rows). No cargo, no repo
//               writes.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const RUNNER_DIR = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(RUNNER_DIR, '..');
const DEFAULT_OUT = 'target/r20_r6_t08_large_commit';
const EXAMPLE = 'r6_t08_large_commit_probe';
const ARMS = [
  { label: 'A', mode: 'production', features: 'production internals' },
  { label: 'B', mode: 'production+exact-span-large', features: 'production internals,exact-span-large' },
];
const CELLS = [16, 64, 256];
const SEGMENT = 4 * 1024 * 1024;
// Prereg thresholds:
const AMPLIF_RATIO_MIN = 8;      // (a) B amplif >= 8x lower than A
const LATENCY_RATIO_MAX = 1.5;   // (d) B alloc_ns_median <= 1.5x A
const SPAN_B_MAX = 278528;       // (c) sub-SEGMENT page-rounded span in B
const SHA_RE = /^[0-9a-f]{40}$/;
// Column semantics: `reserved_delta_KiB` is proc-memstat's
// `charged_or_reserved_bytes()` delta — true commit charge on Windows, VA
// reservation size on Linux/macOS (the r20-gates workflow runs ubuntu-24.04).
const HEADER = 'arm,mode,N,reserved_delta_KiB,reserved_per_alloc_KiB,amplif_x,span_usable_bytes,segments_reserved_delta,rss_delta_KiB,alloc_ns_median';

const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');
const nowIso = () => new Date().toISOString();

// --- pure helpers (covered by --selftest) -------------------------------------

// Parse the probe's CSV output: skips the header and any `#` comment lines,
// returns one row object per data line. `expectedModes` limits which modes
// are accepted (arm sanity).
function parseProbeCsv(out, expectedModes) {
  const rows = [];
  for (const raw of String(out).split(/\r?\n/)) {
    const line = raw.trim();
    if (line === '' || line.startsWith('#') || line === HEADER) continue;
    const f = line.split(',');
    assert.equal(f.length, HEADER.split(',').length, `malformed CSV line: ${line}`);
    const row = {
      arm: f[0],
      mode: f[1],
      n: Number(f[2]),
      reservedDeltaKiB: Number(f[3]),
      reservedPerAllocKiB: Number(f[4]),
      amplifX: Number(f[5]),
      spanUsableBytes: Number(f[6]),
      segmentsReservedDelta: Number(f[7]),
      rssDeltaKiB: Number(f[8]),
      allocNsMedian: Number(f[9]),
    };
    for (const [k, v] of Object.entries(row)) {
      if (k !== 'arm' && k !== 'mode') assert.ok(Number.isFinite(v), `non-finite ${k} in: ${line}`);
    }
    assert.ok(expectedModes.has(row.mode), `unexpected mode: ${row.mode}`);
    assert.ok(CELLS.includes(row.n), `unexpected N: ${row.n}`);
    rows.push(row);
  }
  return rows;
}

// Amplification = reserved_delta / requested bytes. Sanity-bounded.
function amplification(commitDeltaBytes, n) {
  const requested = n * 260 * 1024;
  assert.ok(Number.isInteger(requested) && requested > 0, 'requested bytes must be positive');
  const x = commitDeltaBytes / requested;
  assert.ok(Number.isFinite(x) && x > 0 && x <= 10000, `implausible amplification: ${x}`);
  return x;
}

function byKey(rows) {
  const m = new Map();
  for (const r of rows) m.set(`${r.mode}/${r.n}`, r);
  return m;
}

// Preregistered GO/NO-GO (docs/perf/R6_T08_LARGE_COMMIT_AMPLIFICATION_PREREG.md
// section 5). Pure: takes arm A rows and arm B rows, returns
// { go, reasons }.
function evaluateGoNoGo(rowsA, rowsB) {
  const reasons = [];
  const a = byKey(rowsA), b = byKey(rowsB);
  for (const n of CELLS) {
    const ra = a.get(`${ARMS[0].mode}/${n}`);
    const rb = b.get(`${ARMS[1].mode}/${n}`);
    if (!ra || !rb) return { go: false, reasons: [`missing row for N=${n}`] };
    // (a) reserved/commit amplification >= 8x lower in B
    if (!(rb.amplifX <= ra.amplifX / AMPLIF_RATIO_MIN)) {
      reasons.push(`(a) FAIL at N=${n}: amplif B=${rb.amplifX} not <= A/${AMPLIF_RATIO_MIN}=${(ra.amplifX / AMPLIF_RATIO_MIN).toFixed(4)}`);
    }
    // NOTE: RSS is deliberately NOT a GO condition (prereg §5): committed-but-
    // untouched pages need not be resident, so arm A's rounded-up tail may
    // never appear in RSS even with full-request touching. rss_delta_KiB is
    // recorded for information only.
    // (c) oracle: exact-span branch taken in B and ONLY in B
    if (ra.spanUsableBytes !== SEGMENT) {
      reasons.push(`(c) FAIL at N=${n}: arm A span_usable=${ra.spanUsableBytes}, expected ${SEGMENT}`);
    }
    if (!(rb.spanUsableBytes <= SPAN_B_MAX && rb.spanUsableBytes > 0)) {
      reasons.push(`(c) FAIL at N=${n}: arm B span_usable=${rb.spanUsableBytes}, expected >0 and <= ${SPAN_B_MAX}`);
    }
    if (ra.segmentsReservedDelta !== n || rb.segmentsReservedDelta !== n) {
      reasons.push(`(c) FAIL at N=${n}: segments_reserved_delta A=${ra.segmentsReservedDelta} B=${rb.segmentsReservedDelta}, expected ${n} in both`);
    }
    // (d) latency within 1.5x
    if (!(rb.allocNsMedian <= ra.allocNsMedian * LATENCY_RATIO_MAX)) {
      reasons.push(`(d) FAIL at N=${n}: alloc_ns_median B=${rb.allocNsMedian} > ${LATENCY_RATIO_MAX}x A=${ra.allocNsMedian}`);
    }
  }
  return { go: reasons.length === 0, reasons };
}

function csvField(v) {
  const s = String(v);
  return /[",\r\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
}

function summaryCsv(rowsA, rowsB) {
  const line = (arm, r) => [arm, r.mode, r.n, r.reservedDeltaKiB, r.reservedPerAllocKiB,
    r.amplifX, r.spanUsableBytes, r.segmentsReservedDelta, r.rssDeltaKiB,
    r.allocNsMedian].map(csvField).join(',');
  return [HEADER,
    ...rowsA.map(r => line('A', r)),
    ...rowsB.map(r => line('B', r)),
  ].join('\n') + '\n';
}

// --- process helpers ----------------------------------------------------------

function git(args) {
  const r = spawnSync('git', ['--no-optional-locks', ...args], { cwd: REPO, encoding: 'utf8', maxBuffer: 1 << 26 });
  assert.equal(r.status, 0, `git ${args.join(' ')} failed: ${r.stderr}`);
  return r.stdout.trim();
}

function scrubEnv() {
  return { ...process.env, RUSTC_WRAPPER: '', CARGO_BUILD_RUSTC_WRAPPER: '' };
}

// Find the built executable from `cargo build --message-format=json` output
// (same approach as r20_r6_ab_iai_run.mjs).
function executableOf(jsonOut, target) {
  const hits = [];
  for (const line of jsonOut.split('\n').filter(Boolean)) {
    try {
      const m = JSON.parse(line);
      if (m.reason === 'compiler-artifact' && m.target?.name === target && m.executable) {
        hits.push(m.executable);
      }
    } catch { /* cargo human lines are interleaved; ignore */ }
  }
  assert.ok(hits.length >= 1, `no executable reported for ${target}`);
  return hits[hits.length - 1];
}

// --- stages -------------------------------------------------------------------

function outPaths(args) {
  if (args.includes('--out-dir')) {
    const rel = args[args.indexOf('--out-dir') + 1];
    assert.match(rel, /^(?:target\/r20[a-z0-9_-]*)$/,
      '--out-dir must stay under target/r20* (docs/perf/R20_* is reserved for the report)');
    return resolve(REPO, rel);
  }
  return resolve(REPO, DEFAULT_OUT);
}

function cmdPrepare(outDir) {
  const head = git(['rev-parse', 'HEAD']);
  assert.ok(SHA_RE.test(head), `HEAD is not a full SHA: ${head}`);
  const status = git(['status', '--porcelain=v1', '--untracked-files=all']);
  assert.equal(status, '', 'worktree must be clean before measuring (immutable source identity)');
  const manifest = {
    version: 1,
    createdAt: nowIso(),
    head,
    tree: git(['rev-parse', 'HEAD^{tree}']),
    cleanTree: true,
    example: EXAMPLE,
    arms: ARMS.map(({ label, mode, features }) => ({ label, mode, features })),
    cells: CELLS,
    prereg: 'docs/perf/R6_T08_LARGE_COMMIT_AMPLIFICATION_PREREG.md',
  };
  fs.mkdirSync(outDir, { recursive: true });
  const p = join(outDir, 'manifest.json');
  assert.ok(!fs.existsSync(p), `manifest exists, refusing to overwrite: ${p}`);
  fs.writeFileSync(p, JSON.stringify(manifest, null, 2) + '\n');
  console.log(`[prepare] HEAD=${head.slice(0, 12)} clean; manifest: ${p}`);
}

// Re-verify the source identity recorded by --prepare (finding 6): the runner
// refuses to build or measure anything but the exact prepared worktree.
function verifyIdentity(manifest) {
  const head = git(['rev-parse', 'HEAD']);
  assert.equal(head, manifest.head, 'worktree changed since --prepare (HEAD differs)');
  const status = git(['status', '--porcelain=v1', '--untracked-files=all']);
  assert.equal(status, '', 'worktree changed since --prepare (tree no longer clean)');
}

function cmdBuild(outDir) {
  const manifestPath = join(outDir, 'manifest.json');
  assert.ok(fs.existsSync(manifestPath), 'run --prepare first');
  verifyIdentity(JSON.parse(fs.readFileSync(manifestPath, 'utf8')));
  const binaries = {};
  for (const arm of ARMS) {
    const r = spawnSync('cargo', ['build', '--release', '--locked',
      '--message-format=json', '--example', EXAMPLE, '--features', arm.features],
      { cwd: REPO, encoding: 'utf8', maxBuffer: 1 << 28, env: scrubEnv() });
    assert.equal(r.status, 0, `cargo build failed for arm ${arm.label}: ${r.stderr}`);
    const exe = executableOf(r.stdout, EXAMPLE);
    // Copy the executable into outDir: both arms share the target dir, so the
    // second build overwrites the first's binary in place. The copies under
    // outDir are the immutable per-arm artifacts that --measure runs.
    const copied = `arm_${arm.label}_probe.bin`;
    fs.copyFileSync(exe, join(outDir, copied));
    const sha = sha256(fs.readFileSync(join(outDir, copied)));
    binaries[arm.label] = { mode: arm.mode, features: arm.features, exe, copiedTo: copied, sha256: sha };
    console.log(`[build] arm ${arm.label} (${arm.features}): ${sha.slice(0, 12)} -> ${copied}`);
  }
  fs.writeFileSync(join(outDir, 'builds.json'), JSON.stringify({ createdAt: nowIso(), binaries }, null, 2) + '\n');
}

function cmdMeasure(outDir) {
  const buildsPath = join(outDir, 'builds.json');
  assert.ok(fs.existsSync(buildsPath), 'run --build first');
  const builds = JSON.parse(fs.readFileSync(buildsPath, 'utf8')).binaries;
  verifyIdentity(JSON.parse(fs.readFileSync(join(outDir, 'manifest.json'), 'utf8')));
  const rows = { A: [], B: [] };
  for (const arm of ARMS) {
    const b = builds[arm.label];
    const binPath = join(outDir, b.copiedTo);
    assert.equal(sha256(fs.readFileSync(binPath)), b.sha256, `binary drift for arm ${arm.label} (${b.copiedTo})`);
    const r = spawnSync(binPath, [], { cwd: REPO, encoding: 'utf8', maxBuffer: 1 << 26, timeout: 600_000 });
    assert.equal(r.status, 0, `probe failed for arm ${arm.label}: ${r.stderr}`);
    fs.writeFileSync(join(outDir, `arm_${arm.label}_stdout.csv`), r.stdout);
    fs.writeFileSync(join(outDir, `arm_${arm.label}_stderr.log`), r.stderr);
    rows[arm.label] = parseProbeCsv(r.stdout, new Set([arm.mode]));
  }
  fs.writeFileSync(join(outDir, 'summary.csv'), summaryCsv(rows.A, rows.B));
  const verdict = evaluateGoNoGo(rows.A, rows.B);
  const text = verdict.go
    ? 'VERDICT: GO — exact-span-large evaluation may be promoted to a separate code-change task (no code changed by this run)\n'
    : `VERDICT: NO-GO (document-only outcome)\n${verdict.reasons.join('\n')}\n`;
  fs.writeFileSync(join(outDir, 'verdict.txt'), text);
  process.stdout.write(text);
  if (!verdict.go) process.exitCode = 1;
}

function cmdCleanup(outDir) {
  assert.ok(outDir.startsWith(join(REPO, 'target')), 'cleanup escaped this worktree\'s target/');
  if (fs.existsSync(outDir)) fs.rmSync(outDir, { recursive: true, force: true });
  console.log(`[cleanup] removed ${outDir}`);
}

// --- selftest (pure, in-memory only) -------------------------------------------

function selftest() {
  // parseProbeCsv: header + comment + 2 valid lines. Every line (including
  // the header) must have exactly the header's field count.
  const out = [
    HEADER,
    '# oracle: mode=production n=16 span_usable=4194304 segment_base=0x7ff0000000',
    'A,production,16,4096,256.0,16.00,4194304,16,4096,900',
    'B,production+exact-span-large,16,260,16.25,1.00,266240,16,256,1200',
  ].join('\n');
  const rows = parseProbeCsv(out, new Set(['production', 'production+exact-span-large']));
  assert.equal(rows.length, 2);
  assert.equal(rows[0].arm, 'A');
  assert.equal(rows[0].mode, 'production');
  assert.equal(rows[1].arm, 'B');
  assert.equal(rows[1].spanUsableBytes, 266240);
  // Header field count == every data row field count.
  const nFields = HEADER.split(',').length;
  for (const line of out.split('\n')) {
    const t = line.trim();
    if (t && !t.startsWith('#')) assert.equal(t.split(',').length, nFields, `field count mismatch: ${t}`);
  }

  // rejects malformed lines
  assert.throws(() => parseProbeCsv('production,16,1,2,3\n', new Set(['production'])));
  assert.throws(() => parseProbeCsv('bogus,16,1,2,3,4,5,6,7,8\n', new Set(['production'])));

  // amplification math
  assert.equal(amplification(16 * 16 * 260 * 1024, 16), 16);
  assert.throws(() => amplification(0, 16));

  // GO on prereg-conformant synthetic data (amplif B = A/16, latency 1.33x);
  // RSS numbers are deliberately adversarial (B > A) to prove RSS is NOT
  // consulted by the GO/NO-GO evaluation.
  const mk = (mode, n, amplif, rss, span, allocNs) => ({
    mode, n, reservedDeltaKiB: amplif * n * 260,
    reservedPerAllocKiB: amplif * 260, amplifX: amplif, spanUsableBytes: span,
    segmentsReservedDelta: n, rssDeltaKiB: rss, allocNsMedian: allocNs,
  });
  const goA = [16, 64, 256].map(n => mk('production', n, 16, 16 * n * 260, SEGMENT, 900));
  const goB = [16, 64, 256].map(n => mk('production+exact-span-large', n, 1, 99 * n * 260, 266240, 1200));
  assert.deepEqual(evaluateGoNoGo(goA, goB), { go: true, reasons: [] });

  // NO-GO: latency blowup in B
  const slowB = goB.map(r => ({ ...r, allocNsMedian: 5000 }));
  assert.equal(evaluateGoNoGo(goA, slowB).go, false);

  // NO-GO: span oracle unchanged in B (branch not taken)
  const badSpanB = goB.map(r => ({ ...r, spanUsableBytes: SEGMENT }));
  assert.ok(evaluateGoNoGo(goA, badSpanB).reasons.some(x => x.startsWith('(c)')));

  // NO-GO: amplification only 2x lower
  const weakB = goB.map(r => ({ ...r, amplifX: 8, reservedDeltaKiB: 8 * r.n * 260, reservedPerAllocKiB: 8 * 260 }));
  assert.ok(evaluateGoNoGo(goA, weakB).reasons.some(x => x.startsWith('(a)')));

  // NO-GO: missing arm B row
  assert.equal(evaluateGoNoGo(goA, goB.slice(0, 2)).go, false);

  // summaryCsv shape
  const csv = summaryCsv(goA, goB);
  assert.ok(csv.startsWith(HEADER + '\n'));
  assert.equal(csv.trim().split('\n').length, 7);
  const width = HEADER.split(',').length;
  for (const line of csv.trim().split('\n')) assert.equal(line.split(',').length, width, line);

  console.log('SELFTEST PASS');
}

// --- main ----------------------------------------------------------------------

function parseArgs(args) {
  let mode = null, outDir = null;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--out-dir') {
      assert.ok(outDir === null && args[i + 1] && !args[i + 1].startsWith('--'), '--out-dir requires one path');
      outDir = args[++i];
    } else {
      assert.ok(['--prepare', '--build', '--measure', '--cleanup', '--selftest'].includes(args[i]),
        'unsupported argument: ' + args[i]);
      assert.equal(mode, null, 'exactly one stage is allowed');
      mode = args[i];
    }
  }
  assert.ok(mode, 'one stage is required');
  return { mode, outDir };
}

function main(args = process.argv.slice(2)) {
  const { mode, outDir: outDirArg } = parseArgs(args);
  if (mode === '--selftest') return selftest();
  const outDir = outPaths(args);
  if (mode === '--prepare') return cmdPrepare(outDir);
  if (mode === '--build') return cmdBuild(outDir);
  if (mode === '--measure') return cmdMeasure(outDir);
  if (mode === '--cleanup') return cmdCleanup(outDir);
  throw new Error('unhandled stage');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
