#!/usr/bin/env node
// tis_p3_ab_runner.mjs — measurement driver for the link-ordering/CAS A/B study of
// `crates/tagged-index-stack`:
//   Link ordering: RegistryShapedStorage hook Acquire/Release vs Relaxed.
//   CAS strength/order: weak CAS and pop-success Relaxed are codegen negative
//         controls; store_elided is a scratch-only push-loop candidate.
//
// Modes:
//   --mode codegen   — materialize the five variants, `rustc --emit=asm`
//                      each DIRECTLY (no cargo), extract/normalize function
//                      blocks, run per-ISA oracles, emit logs/CSV/table.
//   --mode wallclock — materialize the three timing variants, run production
//                      timing, then deterministic and natural activation probes.
//   --mode summary  — read every per-leg CSV + its own raw-log provenance
//                      header and emit the compact summary CSV companion for
//                      the gate report. No build, no measurement. The
//                      required `--target <triple>` selects that target's
//                      wallclock CSV.
//   --mode build-check — materialize all timing-variant scratch CARGO crates
//                      from one dirty-compatible immutable source snapshot and
//                      compile production and cfg-enabled activation harnesses
//                      in separate target dirs; run only the deterministic
//                      cfg activation oracle; ALSO materialize+`rustc
//                      --emit=metadata` the separate `codegen_wrapper.rs.tmpl`
//                      template against the same current sources (the wall-
//                      clock harness and the codegen wrapper are two
//                      independent templates with no shared materialization
//                      code, so a break visible only through one is invisible
//                      to a check of the other). No timing, no docs/perf
//                      artifacts. An API break in `push`/`pop` fails regular per-PR CI
//                      instead of staying invisible until a measurement run.
//
// Node >= 20, zero npm dependencies, Windows-safe (no POSIX-only APIs).
// Scratch vs tracked outputs: build-check mode writes ONLY under its own
// invocation's scratch root target/tis_p3_ab-<mkdtemp>/ (the fixed children
// build-check/ and build-check-codegen-wrapper/; no docs/perf output at
// all). Wallclock and codegen
// modes DO write into docs/perf/ — a TRACKED directory — whenever they are
// run to (re)generate committed evidence (raw logs + summary CSVs).
// That artifact writing is those modes' documented purpose, not a defect —
// but the writes-nothing-tracked property belongs to build-check only.
// `--out-dir` is rejected. Every scratch path is
// <repoRoot>/target/tis_p3_ab-<mkdtemp>/<target> (plus fixed children) by
// construction, validateScratchLeaf pins the one variable segment to a
// single non-dot path component, the root is unpredictable and exclusively
// created by this process, and freshDir() refuses to create anything at or
// outside it.

import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// ── Paths ───────────────────────────────────────────────────────────────────
const scriptPath = fileURLToPath(import.meta.url);
const scriptDir = path.dirname(scriptPath);
// repo root is three levels up from this script
// (<repoRoot>/crates/tagged-index-stack/scripts/).
const repoRoot = path.resolve(scriptDir, '..', '..', '..');
const docsPerfDir = path.join(repoRoot, 'docs', 'perf');
const SOURCE_INPUT_RELATIVE_PATHS = Object.freeze([
  'crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs',
  '.cargo/config.toml',
  'crates/tagged-index-stack/src/lib.rs',
  'crates/tagged-index-stack/src/imp.rs',
  'crates/tagged-index-stack/scripts/tis_p3_ab/codegen_wrapper.rs.tmpl',
  'crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs',
  'crates/tagged-index-stack/scripts/tis_p3_ab/scratch_Cargo.toml.tmpl',
]);
const CARGO_CONFIG_RELATIVE_PATH = '.cargo/config.toml';
// Dedicated scratch root: the ONLY directory tree this runner ever creates
// or deletes inside. Created FRESH by each top-level mode invocation via
// mkdtemp under <repoRoot>/target/: the full path is
// unpredictable (random mkdtemp suffix) and it is created exclusively by
// THIS process, so nothing else could have planted a symlink/junction/
// reparse point anywhere on the path before this process's own first write.
// A fixed scratch path could be redirected by a planted reparse point; this
// per-invocation root prevents that before the first write.
// Each invocation removes its own root again on EVERY exit path — success,
// fail()-driven fatal error, unexpected exception — via the top-level
// finally around the dispatch; --keep-scratch opts out on purpose. A
// hard-killed run's leftover root is inert garbage under
// gitignored <repoRoot>/target/ (never re-entered, never deleted by a later
// invocation — later invocations get their own mkdtemp root).

// Module-level handle for the top-level finally: the ONE scratch root this
// invocation created (null until makeScratchRoot runs). Mode functions keep
// their own local copy; this handle exists so the cleanup site lives in ONE
// place around the dispatch instead of once per mode's success path.
let activeScratchBase = null;

function makeScratchRoot() {
  // mkdtemp requires its parent directory to already exist.
  fs.mkdirSync(path.join(repoRoot, 'target'), { recursive: true });
  activeScratchBase = fs.mkdtempSync(path.join(repoRoot, 'target', 'tis_p3_ab-'));
  if (process.env.TIS_P3_AB_TEST_UNEXPECTED_AFTER_MKDTEMP === '1') {
    throw new Error('deliberate post-mkdtemp ordinary Error for scratch cleanup test');
  }
  return activeScratchBase;
}

const VARIANTS = ['base', 'links_relaxed', 'cas_weak', 'pop_success_relaxed', 'store_elided'];
const WALLCLOCK_VARIANTS = ['base', 'links_relaxed', 'store_elided'];
const CODEGEN_TARGETS = ['x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu'];
const MIN_COMPARATIVE_SAMPLES = 6;
const PROFILE_ID = 'release-thin-lto-1cgu-no-incremental';
const FUNCTION_KEYS = ['load_next', 'store_next', 'push_index_impl', 'pop_index_impl'];

// ── Practical upper bounds ─────────────────────────────────────────────────
// The JS side enforces the SAME practical bounds as the Rust harness
// (scripts/tis_p3_ab/harness_bin.rs) — rejecting absurd inputs at argument
// validation, BEFORE any cargo build or harness process exists, instead of
// trusting the child to reject them after a multi-minute build (or, before
// this fix, never rejecting them at all JS-side).
// Mirrors harness_bin.rs's TIS_AB_THREADS bound exactly.
const MAX_THREADS = 256;
// 60 s: ~600x the largest documented run of this study (window_ms=100,
// docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md) and 60x the default
// (1000 ms), so no real measurement is excluded, while a fat-fingered
// `--window-ms 99999999999999` is rejected at argument validation instead of
// committing CI/a dev machine to a multi-year "measurement". At this cap the
// harness's deadline arithmetic (`Instant::now() + WARMUP + window`) is also
// trivially representable on any platform, so the Rust-side checked-deadline
// guard is unreachable noise rather than a real limit.
const MAX_WINDOW_MS = 60_000;
// 100 samples: smoke uses 1 and comparative runs default to 6; the cap bounds
// three wall-clock variants to under 6 hours at the maximum window and timeout.
const MAX_SAMPLES = 100;
// Child timeout for ONE harness invocation (one variant, one sample),
// derived the same way harness_bin.rs derives its margins: the fixed
// uncounted warm-up lead (WARMUP = 200 ms there) + the timed window + a
// bounded slack covering process spawn, stack prefill, barrier rendezvous,
// the bounded overshoot and exit. The normal path is ~300 ms + window; the
// 10 s slack is ~50x that fixed overhead — generous on a loaded CI machine,
// finite by construction. Without a timeout, a harness that never exits (a
// worker gone before the done-barrier rendezvous — std::sync::Barrier has no
// poison — or any other hang) hung the runner forever.
const HARNESS_WARMUP_MS = 200; // mirrors harness_bin.rs's WARMUP
const HARNESS_TIMEOUT_SLACK_MS = 10_000;
// Explicit inline-never probe symbols make extraction independent of generic
// mangling and inlining heuristics.
const LABEL_MATCHERS = {
  load_next: ['tis_probe_load_next'],
  store_next: ['tis_probe_store_next'],
  push_index_impl: ['tis_probe_push_index_impl'],
  pop_index_impl: ['tis_probe_pop_index_impl'],
};

// ── Text-exact substitution anchors (must each occur EXACTLY ONCE) ─────────
const ANCHORS = {
  PUSH_CAS: {
    find: 'match head_ref.compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed) {',
    replace:
      'match head_ref.head.compare_exchange_weak(head, new_head, Ordering::Release, Ordering::Relaxed) {',
  },
  POP_CAS: {
    find: 'match head_ref.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire) {',
    replace:
      'match head_ref.head.compare_exchange_weak(head, new_head, Ordering::Acquire, Ordering::Acquire) {',
  },
  POP_SUCCESS_RELAXED: {
    find: 'match head_ref.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire) {',
    replace:
      'match head_ref.compare_exchange(head, new_head, Ordering::Relaxed, Ordering::Acquire) {',
  },
};

const VARIANT_ANCHORS = {
  base: [],
  cas_weak: ['PUSH_CAS', 'POP_CAS'],
  links_relaxed: [],
  pop_success_relaxed: ['POP_SUCCESS_RELAXED'],
  store_elided: [],
};

// This is deliberately a source rewrite, not a production edit. The exact
// anchors are scoped to push_index_impl and keep the first attempt as a store;
// only a retry that observed the same (head index, next link) may elide it.
const STORE_ELIDED_REWRITES = [
  {
    find: `    let mut head = head_ref.load(Ordering::Relaxed);\n    let mut backoff = Backoff::new();\n    loop {`,
    replace: `    let mut head = head_ref.load(Ordering::Relaxed);\n    let mut last_stored_head: Option<(u32, u32)> = None;\n    let mut backoff = Backoff::new();\n    loop {`,
  },
  {
    find: `        unsafe {\n            s.store_next(index, next_link);\n        }`,
    replace: `        let observed_head = (cur_idx, next_link);\n        if last_stored_head != Some(observed_head) {\n            // SAFETY: same proof as the production store; this is only a\n            // scratch measurement substitution.\n            unsafe {\n                s.store_next(index, next_link);\n            }\n            last_stored_head = Some(observed_head);\n        }`,
  },
];

// ── CLI ─────────────────────────────────────────────────────────────────────
function parseArgs(argv) {
  const args = { mode: null, target: null, threads: 4, windowMs: 1000, samples: MIN_COMPARATIVE_SAMPLES, smoke: false, keepScratch: false };
  const provided = new Set();
  const markProvided = (option) => {
    if (provided.has(option)) fail(`duplicate option: ${option}`);
    provided.add(option);
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const need = () => {
      if (i + 1 >= argv.length) fail(`missing value for ${a}`);
      return argv[++i];
    };
    switch (a) {
      case '--mode': markProvided(a); args.mode = need(); break;
      case '--target': markProvided(a); args.target = need(); break;
      case '--out-dir':
        // Scratch output is created only under the runner-owned root.
        fail('--out-dir is not supported; scratch output goes to a fresh <repo>/target/tis_p3_ab-<mkdtemp>/<target> directory created by the runner.');
      case '--threads': markProvided(a); args.threads = Number(need()); break;
      case '--window-ms': markProvided(a); args.windowMs = Number(need()); break;
      case '--samples': markProvided(a); args.samples = Number(need()); break;
      case '--smoke': markProvided(a); args.smoke = true; break;
      case '--keep-scratch':
        markProvided(a);
        // Deliberate opt-out from scratch-tree removal for inspection.
        // Without this flag, cleanup runs on every exit path.
        args.keepScratch = true;
        break;
      default: fail(`unknown argument: ${a}`);
    }
  }
  if (args.mode !== 'codegen' && args.mode !== 'wallclock' && args.mode !== 'summary' && args.mode !== 'build-check') {
    fail(`--mode must be "codegen", "wallclock", "summary" or "build-check" (got ${JSON.stringify(args.mode)})`);
  }
  for (const option of ['--threads', '--window-ms', '--samples', '--smoke']) {
    if (provided.has(option) && args.mode !== 'wallclock') {
      fail(`${option} is valid only with --mode wallclock`);
    }
  }
  if (args.mode === 'build-check' && provided.has('--target')) {
    fail('--target is not accepted with --mode build-check; the verified rustc host is selected internally');
  }
  if (args.mode === 'summary' && !provided.has('--target')) {
    fail('--target is required with --mode summary');
  }
  // build-check runs `cargo build` natively (no cross target).
  if (args.mode !== 'summary' && args.mode !== 'build-check') {
    if (!args.target || !/^[A-Za-z0-9_.-]+$/.test(args.target)) {
      fail('--target must be a rust target triple');
    }
    // The target names the scratch leaf <repo>/target/tis_p3_ab-<mkdtemp>/<target>:
    // `.` or `..` would point freshDir() AT or ABOVE the dedicated scratch
    // root (both pass the charset check above). validateScratchLeaf() rejects
    // them here, before any filesystem access.
    validateScratchLeaf(args.target);
    if (args.mode === 'codegen' && !CODEGEN_TARGETS.includes(args.target)) {
      fail(`--mode codegen supports only ${CODEGEN_TARGETS.join(' or ')} (got ${JSON.stringify(args.target)})`);
    }
  }
  if (args.mode === 'wallclock' && args.smoke) {
    for (const option of ['--threads', '--window-ms', '--samples']) {
      if (provided.has(option)) {
        fail(`${option} cannot be provided with --smoke; smoke fixes threads=4, window-ms=100, samples=1`);
      }
    }
  }
  // The summary target uses the producer target charset and becomes part of a
  // docs/perf filename.
  if (args.mode === 'summary' && args.target !== null) {
    if (!/^[A-Za-z0-9_.-]+$/.test(args.target) || args.target === '.' || args.target === '..') {
      fail(`--target (summary mode) must be a single rust target triple (got ${JSON.stringify(args.target)})`);
    }
  }
  return args;
}

// fail() throws instead of calling process.exit() —
// process.exit() terminates the process on the spot and skips `finally`
// blocks, so the scratch-tree cleanup in the top-level finally below could
// never run for a fail() path (any expected build/oracle failure leaked the
// whole scratch tree under target/). The dispatch at the bottom catches the
// throw, prints the same FATAL line as before, and exits 1: identical
// observable behavior, but cleanup now happens on the way out.
class RunnerFatalError extends Error {}
function fail(msg) {
  throw new RunnerFatalError(msg);
}

function assert(cond, msg) {
  if (!cond) fail(`ORACLE/ASSERT failed: ${msg}`);
}

function assertActivationRecord(rec, variant, context, expectedSmoke) {
  assert(rec.variant === variant && rec.source_variant === variant, `${context}: materialized variant identity mismatch`);
  assert(rec.activation === true && rec.activation_probe === 'tag_only_retry', `${context}: not the deterministic activation record`);
  assert(rec.threads === 0 && rec.window_ms === 0 && rec.elapsed_ms === 0 && rec.ops_total === 0 && rec.ops_per_sec === 0, `${context}: activation record contains timing data`);
  assert(rec.smoke === expectedSmoke && !Object.hasOwn(rec, 'natural_push_attempts'), `${context}: activation record has unexpected ordinary/natural fields`);
  assert(rec.activation_push_retries === 1 && rec.activation_pop_retries === 0, `${context}: activation retry fields are not exact`);
  assert(rec.activation_store_next_calls === (variant === 'store_elided' ? 2 : 3), `${context}: activation store field is not exact`);
}

function assertNaturalActivationRecord(rec, variant, context, threads, windowMs) {
  assert(rec.variant === variant && rec.source_variant === variant, `${context}: materialized variant identity mismatch`);
  assert(rec.activation === true && rec.activation_probe === 'natural_workload' && rec.smoke === false, `${context}: not the natural workload activation record`);
  assert(!Object.hasOwn(rec, 'activation_push_retries') && !Object.hasOwn(rec, 'activation_pop_retries') && !Object.hasOwn(rec, 'activation_store_next_calls'), `${context}: natural record contains deterministic fields`);
  assert(rec.threads === threads && rec.window_ms === windowMs, `${context}: natural workload parameters differ from timing parameters`);
  assert(Number.isSafeInteger(rec.elapsed_ms) && rec.elapsed_ms > 0 && Number.isSafeInteger(rec.ops_total) && rec.ops_total > 0, `${context}: invalid natural elapsed/ops counters`);
  assert(Number.isFinite(rec.ops_per_sec) && rec.ops_per_sec > 0, `${context}: invalid natural ops_per_sec`);
  for (const field of ['natural_push_attempts', 'natural_store_next_calls', 'natural_store_elisions', 'push_retries', 'pop_retries']) {
    assert(Number.isSafeInteger(rec[field]) && rec[field] >= 0, `${context}: invalid ${field}`);
  }
  assert(rec.natural_push_attempts === rec.ops_total + rec.push_retries, `${context}: natural push attempts are not ops_total + push retries`);
  assert(rec.natural_store_next_calls <= rec.natural_push_attempts, `${context}: natural store calls exceed push attempts`);
  assert(rec.natural_store_elisions === rec.natural_push_attempts - rec.natural_store_next_calls, `${context}: natural store elisions arithmetic mismatch`);
  if (variant === 'store_elided') {
    assert(rec.natural_push_attempts > 0 && rec.natural_store_next_calls < rec.natural_push_attempts && rec.natural_store_elisions > 0, `${context}: store_elided did not elide a natural store`);
  } else {
    assert(rec.natural_push_attempts > 0 && rec.natural_store_next_calls === rec.natural_push_attempts && rec.natural_store_elisions === 0, `${context}: ${variant} natural store arithmetic is not exact`);
  }
}

function assertProductionRecord(rec, variant, context) {
  assert(rec.variant === variant && rec.smoke === false, `${context}: not the production timing record`);
  assert(!Object.hasOwn(rec, 'activation') && !Object.hasOwn(rec, 'source_variant') && !Object.hasOwn(rec, 'activation_probe') && !Object.hasOwn(rec, 'push_retries') && !Object.hasOwn(rec, 'pop_retries'), `${context}: production record contains activation/counter fields`);
  assert(Number.isSafeInteger(rec.threads) && rec.threads >= 1 && rec.threads <= MAX_THREADS, `${context}: invalid production threads`);
  assert(Number.isSafeInteger(rec.window_ms) && rec.window_ms >= 50 && rec.window_ms <= MAX_WINDOW_MS, `${context}: invalid production window`);
  assert(Number.isSafeInteger(rec.elapsed_ms) && rec.elapsed_ms > 0 && Number.isSafeInteger(rec.ops_total) && rec.ops_total > 0, `${context}: invalid production counters`);
  assert(Number.isFinite(rec.ops_per_sec) && rec.ops_per_sec > 0, `${context}: invalid production metric`);
}

const sha256hex = (s) => createHash('sha256').update(s, 'utf8').digest('hex');
const utf8Base64 = (s) => Buffer.from(s, 'utf8').toString('base64');
const CARGO_ENCODED_SEPARATOR = '\u001f';
const CANONICAL_CARGO_SET_KEYS = [
  'CARGO_ENCODED_RUSTFLAGS',
  'CARGO_HOME',
  'CARGO_INCREMENTAL',
  'CARGO_NET_OFFLINE',
  'CARGO_TARGET_DIR',
];
const CANONICAL_PRODUCTION_RUSTFLAGS =
  '--remap-path-prefix REPO=REPO --remap-path-prefix SCRATCH=SCRATCH';
const CANONICAL_ACTIVATION_RUSTFLAGS =
  `${CANONICAL_PRODUCTION_RUSTFLAGS} --cfg tagged_index_stack_test`;
const HOST_GITHUB_FIELDS = ['ImageOS', 'ImageVersion', 'RUNNER_ARCH'];
const HOST_CONTEXT_KEYS = ['platform', 'release', 'arch', 'cpu_model_set', 'logical_cpu_count', 'online_topology', 'physical_topology', 'github', 'scaling_governor'];

function hostUnavailable() {
  return { status: 'unavailable' };
}

function hostAvailable(value) {
  return { status: 'available', value };
}

function captureHostContext() {
  const cpus = os.cpus();
  const models = [...new Set(cpus.map((cpu) => cpu.model.trim()).filter((model) => model !== ''))].sort();
  const github = Object.fromEntries(HOST_GITHUB_FIELDS.map((name) => {
    const value = process.env[name];
    return [name, typeof value === 'string' && value.length > 0 ? hostAvailable(value) : hostUnavailable()];
  }));

  let onlineTopology = hostUnavailable();
  if (os.platform() === 'linux') {
    try {
      const raw = fs.readFileSync('/sys/devices/system/cpu/online', 'utf8').trim();
      onlineTopology = /^(?:\d+(?:-\d+)?)(?:,(?:\d+(?:-\d+)?))*$/.test(raw) ? hostAvailable(raw) : hostUnavailable();
    } catch {
      onlineTopology = hostUnavailable();
    }
  }

  let physicalTopology = hostUnavailable();
  if (os.platform() === 'linux') {
    try {
      const cpuDirs = fs.readdirSync('/sys/devices/system/cpu', { withFileTypes: true })
        .filter((entry) => entry.isDirectory() && /^cpu\d+$/.test(entry.name))
        .sort((a, b) => Number(a.name.slice(3)) - Number(b.name.slice(3)));
      const records = [];
      let readable = cpuDirs.length > 0;
      for (const entry of cpuDirs) {
        try {
          const packageId = fs.readFileSync(path.join('/sys/devices/system/cpu', entry.name, 'topology', 'physical_package_id'), 'utf8').trim();
          const coreId = fs.readFileSync(path.join('/sys/devices/system/cpu', entry.name, 'topology', 'core_id'), 'utf8').trim();
          if (!/^\d+$/.test(packageId) || !/^\d+$/.test(coreId)) readable = false;
          records.push({ cpu: entry.name, package: packageId, core: coreId });
        } catch {
          readable = false;
        }
      }
      physicalTopology = readable ? hostAvailable(records) : hostUnavailable();
    } catch {
      physicalTopology = hostUnavailable();
    }
  }

  let scalingGovernor = hostUnavailable();
  // Record policy only; never read dynamic frequency files.
  if (os.platform() === 'linux') {
    try {
      const cpuDirs = fs.readdirSync('/sys/devices/system/cpu', { withFileTypes: true })
        .filter((entry) => entry.isDirectory() && /^cpu\d+$/.test(entry.name))
        .sort((a, b) => Number(a.name.slice(3)) - Number(b.name.slice(3)));
      const values = [];
      let readable = cpuDirs.length > 0;
      for (const entry of cpuDirs) {
        try {
          const value = fs.readFileSync(path.join('/sys/devices/system/cpu', entry.name, 'cpufreq', 'scaling_governor'), 'utf8').trim();
          if (value === '') readable = false;
          else values.push(value);
        } catch {
          readable = false;
        }
      }
      const uniqueValues = [...new Set(values)].sort();
      scalingGovernor = readable && uniqueValues.length > 0 ? hostAvailable(uniqueValues) : hostUnavailable();
    } catch {
      scalingGovernor = hostUnavailable();
    }
  }

  return {
    platform: os.platform(),
    release: os.release(),
    arch: os.arch(),
    cpu_model_set: models.length > 0 ? hostAvailable(models) : hostUnavailable(),
    logical_cpu_count: cpus.length > 0 ? hostAvailable(cpus.length) : hostUnavailable(),
    online_topology: onlineTopology,
    physical_topology: physicalTopology,
    github,
    scaling_governor: scalingGovernor,
  };
}

function hostContextEvidence(context, file = 'host context') {
  assert(context !== null && typeof context === 'object' && !Array.isArray(context), `${file}: host context must be an object`);
  assert(JSON.stringify(Object.keys(context)) === JSON.stringify(HOST_CONTEXT_KEYS), `${file}: host context keys are not canonical`);
  for (const key of ['platform', 'release', 'arch']) {
    assert(typeof context[key] === 'string' && context[key].length > 0, `${file}: host ${key} is unavailable or malformed`);
  }
  const checkAvailability = (field, valueCheck) => {
    assert(valueCheck === undefined || valueCheck === null || typeof valueCheck === 'function', `${file}: internal host validator error`);
    assert(field !== null && typeof field === 'object' && !Array.isArray(field), `${file}: host availability field is malformed`);
    assert(JSON.stringify(Object.keys(field)) === JSON.stringify(field.status === 'available' ? ['status', 'value'] : ['status']), `${file}: host availability shape is not canonical`);
    assert(field.status === 'available' || field.status === 'unavailable', `${file}: host availability status is invalid`);
    if (field.status === 'available' && valueCheck !== undefined && valueCheck !== null) assert(valueCheck(field.value), `${file}: available host field value is malformed`);
  };
  checkAvailability(context.cpu_model_set, (value) => Array.isArray(value) && value.length > 0 && value.every((v) => typeof v === 'string' && v.length > 0) && JSON.stringify([...value].sort()) === JSON.stringify(value));
  checkAvailability(context.logical_cpu_count, (value) => Number.isSafeInteger(value) && value > 0);
  checkAvailability(context.online_topology, (value) => typeof value === 'string' && /^(?:\d+(?:-\d+)?)(?:,(?:\d+(?:-\d+)?))*$/.test(value));
  checkAvailability(context.physical_topology, (value) => Array.isArray(value) && value.length > 0 && value.every((record) => (
    record !== null && typeof record === 'object' && !Array.isArray(record) &&
    JSON.stringify(Object.keys(record)) === JSON.stringify(['cpu', 'package', 'core']) &&
    /^cpu\d+$/.test(record.cpu) && /^\d+$/.test(record.package) && /^\d+$/.test(record.core)
  )) && value.every((record, index) => index === 0 || Number(record.cpu.slice(3)) > Number(value[index - 1].cpu.slice(3))));
  assert(context.github !== null && typeof context.github === 'object' && !Array.isArray(context.github), `${file}: host github fields are malformed`);
  assert(JSON.stringify(Object.keys(context.github)) === JSON.stringify(HOST_GITHUB_FIELDS), `${file}: host github fields are not allowlisted/canonical`);
  for (const name of HOST_GITHUB_FIELDS) checkAvailability(context.github[name], (value) => typeof value === 'string' && value.length > 0);
  checkAvailability(context.scaling_governor, (value) => Array.isArray(value) && value.length > 0 && value.every((v) => typeof v === 'string' && v.length > 0) && JSON.stringify([...value].sort()) === JSON.stringify(value));
  return context;
}

function hostContextEvidenceParts(context, file = 'host context') {
  hostContextEvidence(context, file);
  const json = JSON.stringify(context);
  return { json, sha256: sha256hex(json), b64: utf8Base64(json) };
}

// Stable evidence identity only: no timestamps, generatedAt, or scratch paths.
// The order below is the canonical NUL-delimited bundle contract.
function evidenceBundleId({ headSha, treeSha, sourceInputDigest, target, mode, profileId, toolchain, productionRustflags, activationRustflags, cargoEncodedRustflags, hostContextSha256 }) {
  const fields = [headSha, treeSha, sourceInputDigest, target, mode, profileId, toolchain, productionRustflags, activationRustflags, cargoEncodedRustflags, hostContextSha256];
  assert(fields.every((field) => typeof field === 'string' && !field.includes('\0')), 'bundle identity fields must be NUL-free strings');
  return sha256hex(fields.join('\0'));
}

function stripMeasurementCfgs(raw) {
  if (raw.includes('\u001f')) {
    fail('RUSTFLAGS contains encoded separators; clear foreign RUSTFLAGS before running the A/B driver');
  }
  if (/["']/.test(raw)) {
    fail('RUSTFLAGS contains quotes; refusing ambiguous measurement flag parsing (use unquoted whitespace-separated flags or clear RUSTFLAGS)');
  }
  const tokens = raw.trim() === '' ? [] : raw.trim().split(/\s+/);
  const production = [];
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    let cfgValue = null;
    let cfgTokens = null;
    if (token === '--cfg') {
      assert(i + 1 < tokens.length, 'RUSTFLAGS ends with --cfg and has no value');
      cfgValue = tokens[++i];
      cfgTokens = ['--cfg', cfgValue];
    } else if (token.startsWith('--cfg=')) {
      cfgValue = token.slice('--cfg='.length);
      assert(cfgValue.length > 0, 'RUSTFLAGS contains --cfg= with no value');
      cfgTokens = [token];
    }
    if (cfgValue !== null) {
      const cfgName = cfgValue.split('=', 1)[0];
      if (cfgName === 'tagged_index_stack_test' || cfgName === 'loom') continue;
      production.push(...cfgTokens);
    } else {
      production.push(token);
    }
  }
  return production.join(' ');
}

function effectiveMeasurementRustflags() {
  const productionText = stripMeasurementCfgs(process.env.RUSTFLAGS ?? '');
  if (productionText !== '') {
    fail('measurement requires empty production RUSTFLAGS after removing only loom/test cfgs; clear RUSTFLAGS before running the A/B driver');
  }
  return {
    production: CANONICAL_PRODUCTION_RUSTFLAGS,
    activation: CANONICAL_ACTIVATION_RUSTFLAGS,
    cargoEncodedRustflags: 'canonical',
  };
}

const SANITIZED_ENV_NAMES = [
  'RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
  'CARGO_HOME', 'CARGO_INCREMENTAL', 'CARGO_NET_OFFLINE', 'RUSTDOCFLAGS', 'RUSTC_BOOTSTRAP',
  'CARGO_ENCODED_RUSTFLAGS',
];

function isSanitizedEnvKey(key) {
  const normalized = key.toUpperCase();
  return SANITIZED_ENV_NAMES.includes(normalized) || /^CARGO_(PROFILE|BUILD|TARGET)_/.test(normalized);
}

function isForbiddenChildEnvKey(key) {
  return isSanitizedEnvKey(key) && !CANONICAL_CARGO_SET_KEYS.includes(key.toUpperCase());
}

function sanitizedEnvState() {
  const removed = [...new Set(
    Object.keys(process.env).filter(isSanitizedEnvKey).map((key) => key.toUpperCase()),
  )].sort();
  return {
    removedKeys: removed,
    setKeys: CANONICAL_CARGO_SET_KEYS,
    secretValuesLogged: false,
  };
}

function sanitizedBaseChildEnv() {
  const childEnv = { ...process.env };
  for (const key of Object.keys(childEnv)) {
    if (isSanitizedEnvKey(key)) delete childEnv[key];
  }
  return childEnv;
}

function cargoChildEnv(rustflagTokens, targetDir, cargoHome) {
  assert(Array.isArray(rustflagTokens), 'canonical encoded RUSTFLAGS must be an argv array');
  assert(rustflagTokens.every((token) => typeof token === 'string' && !token.includes(CARGO_ENCODED_SEPARATOR)), 'canonical encoded RUSTFLAGS contain an invalid token');
  assert(path.isAbsolute(cargoHome) && fs.statSync(cargoHome).isDirectory(), 'canonical CARGO_HOME must be an existing absolute directory');
  const childEnv = sanitizedBaseChildEnv();
  childEnv.CARGO_ENCODED_RUSTFLAGS = rustflagTokens.join(CARGO_ENCODED_SEPARATOR);
  childEnv.CARGO_HOME = cargoHome;
  childEnv.CARGO_TARGET_DIR = targetDir;
  childEnv.CARGO_NET_OFFLINE = 'true';
  childEnv.CARGO_INCREMENTAL = '0';
  assert(childEnv.RUSTFLAGS === undefined, 'sanitized cargo environment retained RUSTFLAGS');
  assert(childEnv.CARGO_ENCODED_RUSTFLAGS === rustflagTokens.join(CARGO_ENCODED_SEPARATOR), 'cargo encoded RUSTFLAGS differ from canonical tokens');
  assert(!Object.keys(childEnv).some(isForbiddenChildEnvKey), 'sanitized cargo environment retained a forbidden override');
  return childEnv;
}

function directRustcChildEnv() {
  const childEnv = sanitizedBaseChildEnv();
  assert(!Object.keys(childEnv).some(isSanitizedEnvKey), 'sanitized rustc environment retained a foreign override');
  return childEnv;
}

function sourceInputManifest(readBytes = (_relativePath, file) => fs.readFileSync(file)) {
  // Keep one digest across codegen and wall-clock legs: every source or
  // template that any runner mode can consume is part of the identity.
  const manifest = SOURCE_INPUT_RELATIVE_PATHS.map((relativePath) => {
    const file = path.join(repoRoot, ...relativePath.split('/'));
    const bytes = readBytes(relativePath, file);
    assert(Buffer.isBuffer(bytes), `source input reader returned non-Buffer bytes for ${relativePath}`);
    return {
      path: relativePath,
      bytes: bytes.length,
      sha256: createHash('sha256').update(bytes).digest('hex'),
      snapshotBytes: bytes,
    };
  });
  const digest = createHash('sha256');
  for (const item of manifest) {
    const pathBytes = Buffer.from(item.path, 'utf8');
    digest.update(Buffer.from(String(pathBytes.length), 'utf8'));
    digest.update(Buffer.from([0]));
    digest.update(pathBytes);
    digest.update(Buffer.from(String(item.bytes), 'utf8'));
    digest.update(Buffer.from([0]));
    digest.update(Buffer.from(item.sha256, 'utf8'));
    digest.update(Buffer.from([0]));
  }
  return {
    digest: digest.digest('hex'),
    files: manifest.map(({ snapshotBytes, ...item }) => item),
    snapshot: new Map(manifest.map((item) => [item.path, item.snapshotBytes])),
  };
}

function readGitSourceBytes(headSha, relativePath) {
  assert(/^[0-9a-f]{40}$/.test(headSha), `malformed source snapshot HEAD ${headSha}`);
  const result = spawnSync('git', ['show', `${headSha}:${relativePath}`], {
    cwd: repoRoot,
    shell: false,
  });
  if (result.error) {
    fail(`could not read ${relativePath} from captured HEAD ${headSha}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const stderr = Buffer.isBuffer(result.stderr) ? result.stderr.toString('utf8') : String(result.stderr ?? '');
    fail(`could not read ${relativePath} from captured HEAD ${headSha} (git show ${result.status}): ${stderr}`);
  }
  assert(Buffer.isBuffer(result.stdout), `git show returned non-Buffer bytes for ${relativePath}`);
  return result.stdout;
}

function requireCapturedCargoConfig(header, phase) {
  const captured = header.sourceSnapshot.get(CARGO_CONFIG_RELATIVE_PATH);
  assert(Buffer.isBuffer(captured), `${phase}: captured .cargo/config.toml bytes are missing`);
  let live;
  try {
    live = fs.readFileSync(path.join(repoRoot, ...CARGO_CONFIG_RELATIVE_PATH.split('/')));
  } catch (error) {
    fail(`${phase}: live ancestor .cargo/config.toml could not be read: ${error.message}`);
  }
  assert(live.equals(captured), `${phase}: live ancestor .cargo/config.toml differs from the captured HEAD object`);
}

function runEvidenceCargoBuild(header, rustflagTokens, cargoArgs, cwd, targetDir, label) {
  // Cargo discovers the checkout's live ancestor config because evidence
  // crates are deliberately materialized below repoRoot. The source files
  // themselves come from the ODB snapshot; this check makes the config use
  // explicit and fail closed instead of implying Cargo read ODB bytes.
  requireSourceInputsAtHead({ files: header.sourceInputs }, header.identity.headSha, `before ${label}`);
  requireCapturedCargoConfig(header, `before ${label}`);
  let result;
  try {
    result = spawnSync('cargo', cargoArgs, {
      cwd,
      encoding: 'utf8',
      env: cargoChildEnv(rustflagTokens, targetDir, header.cargoHome),
    });
  } finally {
    requireCapturedCargoConfig(header, `after ${label}`);
  }
  requireSourceInputsAtHead({ files: header.sourceInputs }, header.identity.headSha, `after ${label}`);
  return result;
}

function requireSourceInputsAtHead(sourceInputs, expectedHead, phase) {
  assert(/^[0-9a-f]{40}$/.test(expectedHead), `${phase}: malformed expected HEAD`);
  const currentHead = runCapture('git', ['rev-parse', 'HEAD']).trim();
  assert(currentHead === expectedHead, `${phase}: HEAD changed from ${expectedHead} to ${currentHead}`);
  const paths = sourceInputs.files.map((input) => input.path);
  const result = spawnSync('git', ['diff', '--quiet', expectedHead, '--', ...paths], {
    cwd: repoRoot,
    encoding: 'utf8',
    shell: false,
  });
  if (result.error) {
    fail(`${phase}: source-input revalidation could not start: ${result.error.message}`);
  }
  if (result.status === 1) {
    fail(`${phase}: evidence source inputs differ from captured HEAD ${expectedHead}`);
  }
  if (result.status !== 0) {
    fail(`${phase}: source-input revalidation failed (${result.status}): ${result.stderr}`);
  }
}

function runCapture(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { cwd: opts.cwd ?? repoRoot, encoding: 'utf8', shell: false });
  if (r.status !== 0) {
    fail(`command failed (${r.status}): ${cmd} ${args.join(' ')}\nstderr:\n${r.stderr}`);
  }
  return r.stdout;
}

function rustcHostFromVerbose(rustcVersion) {
  const hostFields = rustcVersion.split(/\r?\n/).filter((line) => line.startsWith('host:'));
  assert(hostFields.length === 1, `rustc --version --verbose must contain exactly one host field (found ${hostFields.length})`);
  const match = /^host:\s+([A-Za-z0-9_.-]+)$/.exec(hostFields[0]);
  assert(match !== null, `rustc --version --verbose has malformed host field ${JSON.stringify(hostFields[0])}`);
  return match[1];
}

function snapshotText(header, relativePath) {
  const bytes = header.sourceSnapshot.get(relativePath);
  assert(bytes !== undefined, `source snapshot is missing ${relativePath}`);
  return bytes.toString('utf8');
}

function canonicalSanitizedEnvJson(state) {
  return JSON.stringify({
    removedKeys: state.removedKeys,
    setKeys: state.setKeys,
    secretValuesLogged: state.secretValuesLogged,
  });
}

function bindRunFlags(header, scratchBase) {
  assert(header.cargoHome === undefined, 'CARGO_HOME must be created exactly once per invocation');
  const cargoHome = path.join(scratchBase, 'cargo-home');
  freshDir(cargoHome, scratchBase);
  const production = rustcRemapArgs(scratchBase);
  const activation = [...production, '--cfg', 'tagged_index_stack_test'];
  header.actualRustflagTokens = { production, activation };
  header.cargoHome = cargoHome;
  header.effectiveRustflags = {
    production: CANONICAL_PRODUCTION_RUSTFLAGS,
    activation: CANONICAL_ACTIVATION_RUSTFLAGS,
    cargoEncodedRustflags: 'canonical',
  };
  header.sanitizedEnv = sanitizedEnvState();
  header.sanitizedEnvJson = canonicalSanitizedEnvJson(header.sanitizedEnv);
  header.sanitizedEnvSha256 = sha256hex(header.sanitizedEnvJson);
  header.sanitizedEnvB64 = utf8Base64(header.sanitizedEnvJson);
}

function rustcRemapArgs(scratchBase) {
  return [
    '--remap-path-prefix', `${repoRoot}=REPO`,
    '--remap-path-prefix', `${scratchBase}=SCRATCH`,
  ];
}

function materializeTemplate(template, variant) {
  const relaxed = variant === 'links_relaxed';
  return template
    .replaceAll('{{VARIANT_NAME}}', variant)
    .replaceAll('{{EXPECTED_STORE_NEXT_CALLS}}', variant === 'store_elided' ? '2' : '3')
    .replaceAll('{{LINK_LOAD_ORDERING}}', relaxed ? 'Ordering::Relaxed' : 'Ordering::Acquire')
    .replaceAll('{{LINK_STORE_ORDERING}}', relaxed ? 'Ordering::Relaxed' : 'Ordering::Release');
}

function materializeImp(impSrc, variant) {
  let out = applyAnchors(impSrc, VARIANT_ANCHORS[variant]);
  if (variant !== 'store_elided') return out;
  for (const [i, rewrite] of STORE_ELIDED_REWRITES.entries()) {
    const count = out.split(rewrite.find).length - 1;
    assert(count === 1, `store_elided rewrite ${i}: expected exactly 1 occurrence, found ${count}`);
    out = out.replace(rewrite.find, rewrite.replace);
  }
  return out;
}

function stageAndPublishArtifacts(header, scratchBase, artifacts) {
  if (header !== null) {
    requireSourceInputsAtHead({ files: header.sourceInputs }, header.identity.headSha, 'before artifact staging');
  }
  const stage = path.join(scratchBase, 'publish');
  freshDir(stage, scratchBase);
  const forbiddenPaths = [repoRoot, repoRoot.replaceAll('\\', '/'), scratchBase, scratchBase.replaceAll('\\', '/')];
  for (const [name, text] of Object.entries(artifacts)) {
    assert(!forbiddenPaths.some((forbidden) => text.includes(forbidden)), `${name}: artifact contains an absolute checkout or scratch path`);
    fs.writeFileSync(path.join(stage, name), text);
  }
  if (header !== null) {
    requireSourceInputsAtHead({ files: header.sourceInputs }, header.identity.headSha, 'before artifact publish');
  }
  fs.mkdirSync(docsPerfDir, { recursive: true });
  for (const [name] of Object.entries(artifacts)) {
    fs.copyFileSync(path.join(stage, name), path.join(docsPerfDir, name));
  }
}

// ── Snapshot and evidence capture ───────────────────────────────────────────
// Build-check consumes this dirty-compatible snapshot directly. Evidence
// modes pass an ODB reader bound to one already-pinned HEAD SHA.
function captureSnapshotContext(args, readBytes) {
  const sourceInputs = sourceInputManifest(readBytes);
  const effectiveRustflags = effectiveMeasurementRustflags();
  return {
    sourceInputDigest: sourceInputs.digest,
    sourceInputs: sourceInputs.files,
    sourceSnapshot: sourceInputs.snapshot,
    effectiveRustflags,
    sanitizedEnv: sanitizedEnvState(),
    profileId: PROFILE_ID,
    smoke: args.smoke,
    target: args.target,
    mode: args.mode,
    anchors: Object.fromEntries(Object.entries(ANCHORS).map(([k, v]) => [k, { find: v.find, replace: v.replace }])),
    driver: 'crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs',
  };
}

// Evidence modes pin HEAD before reading any source bytes. The snapshot is
// immutable ODB data; the worktree check below is a separate cleanliness gate.
function captureEvidenceHeader(args) {
  const headSha = runCapture('git', ['rev-parse', 'HEAD']).trim();
  assert(/^[0-9a-f]{40}$/.test(headSha), `malformed captured HEAD ${headSha}`);
  const context = captureSnapshotContext(args, (relativePath) => readGitSourceBytes(headSha, relativePath));
  requireSourceInputsAtHead({ files: context.sourceInputs }, headSha, 'initial evidence capture');
  const treeSha = runCapture('git', ['rev-parse', `${headSha}^{tree}`]).trim();
  const checkedHead = runCapture('git', ['rev-parse', 'HEAD']).trim();
  assert(checkedHead === headSha, 'HEAD changed while source snapshot was captured');
  const rustcVersion = runCapture('rustc', ['--version', '--verbose']).trim();
  const rustcHost = rustcHostFromVerbose(rustcVersion);
  const toolchain = rustcVersion.replace(/\r?\n/g, ' | ');
  const hostContext = hostContextEvidenceParts(captureHostContext(), 'initial evidence host context');
  const identity = {
    capturedAt: new Date().toISOString(),
    headSha,
    treeSha,
    sourceSnapshotDigest: context.sourceInputDigest,
    sourceInputsAtHead: true,
  };
  const bundleId = evidenceBundleId({
    headSha,
    treeSha,
    sourceInputDigest: context.sourceInputDigest,
    target: args.target,
    mode: args.mode,
    profileId: PROFILE_ID,
    toolchain,
    productionRustflags: context.effectiveRustflags.production,
    activationRustflags: context.effectiveRustflags.activation,
    cargoEncodedRustflags: context.effectiveRustflags.cargoEncodedRustflags,
    hostContextSha256: hostContext.sha256,
  });
  return {
    ...context,
    identity,
    rustcVersion,
    rustcHost,
    toolchain,
    sourceInputsAtHead: true,
    bundleId,
    hostContextJson: hostContext.json,
    hostContextSha256: hostContext.sha256,
    hostContextB64: hostContext.b64,
    generatedAt: identity.capturedAt,
  };
}

function headerComment(header) {
  return [
    '// =====================================================================',
    '// RAW MEASUREMENT ARTIFACT — generated by tis_p3_ab_runner.mjs, do not edit.',
    `// generatedAt: ${header.generatedAt}`,
    `// driver:      ${header.driver}`,
    `// mode:        ${header.mode}`,
    `// target:      ${header.target}`,
    `// rustc:       ${header.rustcVersion.replace(/\n/g, ' | ')}`,
    `// rustc-host:  ${header.rustcHost}`,
    `// identity:    ${JSON.stringify(header.identity)}`,
    `// bundle-id:   ${header.bundleId}`,
    `// profile-id:  ${header.profileId}`,
    `// host-context: ${header.hostContextJson}`,
    `// host-context-sha256: ${header.hostContextSha256}`,
    `// host-context-b64: ${header.hostContextB64}`,
    `// smoke:       ${header.smoke}`,
    `// source-input-digest: ${header.sourceInputDigest}`,
    `// source-inputs: ${JSON.stringify(header.sourceInputs)}`,
    `// source-inputs-at-head: ${header.sourceInputsAtHead}`,
    '// cargo-config-policy: Cargo reads live ancestor .cargo/config.toml; evidence checks it byte-for-byte against the captured HEAD object immediately before and after each Cargo build',
    `// toolchain:    ${header.toolchain}`,
    `// effective-production-rustflags: ${JSON.stringify(header.effectiveRustflags.production)}`,
    `// effective-activation-rustflags: ${JSON.stringify(header.effectiveRustflags.activation)}`,
    `// cargo-encoded-rustflags: ${header.effectiveRustflags.cargoEncodedRustflags}`,
    `// sanitized-cargo-env: ${header.sanitizedEnvJson}`,
    '// rustflags-policy: canonical CARGO_ENCODED_RUSTFLAGS remap argv; activation adds --cfg tagged_index_stack_test',
    '// substitution anchors (text-exact, each verified to occur exactly once):',
    ...Object.values(header.anchors).map((a) => `//   ${a.find}  ->  ${a.replace}`),
    '// =====================================================================',
    '',
  ].join('\n');
}

// ── Substitution engine ─────────────────────────────────────────────────────
function applyAnchors(impSrc, anchorNames) {
  let out = impSrc;
  for (const name of anchorNames) {
    const a = ANCHORS[name];
    const count = out.split(a.find).length - 1;
    assert(count === 1, `anchor ${name}: expected exactly 1 occurrence, found ${count}: ${a.find}`);
    out = out.replace(a.find, a.replace);
  }
  // Global sanity: every anchor in this variant's list must have been applied
  // exactly once (the replace above consumed the only occurrence).
  return out;
}

function verifyAllAnchorsOnce(impSrc) {
  for (const [name, a] of Object.entries(ANCHORS)) {
    const count = impSrc.split(a.find).length - 1;
    assert(count === 1, `anchor ${name}: expected exactly 1 occurrence in src/imp.rs, found ${count}`);
  }
}

// Single non-dot path segment naming a child of THIS invocation's mkdtemp
// scratch root. Rejects `.`,
// `..`, and anything containing a separator, so the joined path cannot
// escape the scratch root lexically. Enforced at the CLI layer (parseArgs,
// before any filesystem access) and re-checked where the segment is joined
// (scratchRoot below).
function validateScratchLeaf(name) {
  if (
    typeof name !== 'string' || name.length === 0 ||
    name === '.' || name === '..' ||
    name.includes('/') || name.includes('\\')
  ) {
    fail(`scratch directory name ${JSON.stringify(name)} must be a single non-dot path segment — scratch output is always <repo>/target/tis_p3_ab-<mkdtemp>/<name>, never anywhere else`);
  }
}

function freshDir(dir, root) {
  // Fail-if-exists creation of ONE new child directory under THIS
  // invocation's mkdtemp scratch root. Every call site passes a leaf whose
  // parent already exists, so plain mkdirSync intentionally throws if the
  // leaf already exists and exposes path reuse instead of clearing it. Its
  // safety rests on the mkdtemp root: `root` is unpredictable and was
  // created exclusively by THIS process, so nothing else could have planted
  // a symlink/junction/reparse point on the path before this process's own
  // first write to it. The lexical check below remains purely as a backstop
  // against caller bugs — it fails closed on anything at or outside `root`.
  const resolved = path.resolve(dir);
  const rel = path.relative(root, resolved);
  if (rel === '' || rel === '..' || rel.startsWith(`..${path.sep}`) || path.isAbsolute(rel)) {
    fail(`refusing to create ${resolved}: it is not strictly inside this invocation's dedicated scratch root ${root}`);
  }
  try {
    fs.mkdirSync(dir);
  } catch (e) {
    if (e.code === 'EEXIST') {
      fail(`scratch directory already exists at ${resolved}: each child path must be created exactly once per invocation (path-reuse bug)`);
    }
    fail(`failed to create scratch directory ${resolved}: ${e.message}`);
  }
}

// The scratch tree has exactly one variable segment (args.target, validated
// by validateScratchLeaf above) and no user-supplied base path. The base is
// this invocation's fresh mkdtemp root.
function scratchRoot(args, root) {
  validateScratchLeaf(args.target);
  return path.join(root, args.target);
}

// ── Assembler parsing ───────────────────────────────────────────────────────
const LABEL_RE = /^[A-Za-z_$][A-Za-z0-9_$.]*:$/;

function parseBlocks(asmText) {
  const blocks = [];
  let cur = null;
  for (const line of asmText.split(/\r?\n/)) {
    if (LABEL_RE.test(line)) {
      if (cur) blocks.push(cur);
      cur = { label: line.slice(0, -1), lines: [] };
    } else if (cur) {
      cur.lines.push(line);
    }
  }
  if (cur) blocks.push(cur);
  return blocks;
}

// Cross-variant comparison requires crate-name-independent normalized text:
// v0-mangled symbols embed the scratch crate name (which differs per
// variant), and LLVM `.Lanon.*` label hashes differ per compilation. Scrub
// both; mnemonics and register/immediate operands are untouched.
function scrubSymbols(line) {
  return line
    .replace(/\d+tis_p3ab_(base|links_relaxed|cas_weak|pop_success_relaxed|store_elided)/g, 'TISCRATE')
    .replace(/Cs[A-Za-z0-9]{8,16}_/g, 'CSHASH')
    .replace(/\.Lanon\.[0-9a-f]+/g, '.Lanon');
}

function normalizeBlock(lines, target) {
  const isAarch64 = target.startsWith('aarch64');
  const out = [];
  for (let raw of lines) {
    let line = raw.trim();
    if (line === '') continue;
    if (/^\s*\./.test(raw)) continue; // assembler directives
    // AArch64 uses `//` for comments and `#immediate` for operands. x86
    // uses `#` for comments; never strip an immediate as if it were prose.
    const comment = line.indexOf(isAarch64 ? '//' : '#');
    if (comment >= 0) line = line.slice(0, comment);
    line = scrubSymbols(line.trim());
    if (line === '') continue;
    out.push(line);
  }
  return out;
}

const MNEMONIC_FAMILIES = ['ldar', 'stlr', 'ldaxr', 'stlxr', 'ldapr', 'ldxr', 'stxr', 'ldr', 'str', 'mov'];

function countFamilies(normLines) {
  const counts = Object.fromEntries(MNEMONIC_FAMILIES.map((m) => [m, 0]));
  counts.lock = 0;
  counts.cmpxchg = 0;
  counts.cas = 0; // AArch64 LSE single-instruction CAS: first token matches /^cas[a-z]*$/
  counts.cas8 = 0; // AArch64 outlined-atomics calls: any token starting with __aarch64_cas8_
  for (const line of normLines) {
    const tokens = line.split(/[\s\t]+/);
    const first = tokens[0].toLowerCase();
    if (first === 'lock') {
      counts.lock += 1;
      // x86: `lock cmpxchgq ...` — the lock prefix carries the cmpxchg.
      if (tokens.length > 1 && tokens[1].toLowerCase().startsWith('cmpxchg')) counts.cmpxchg += 1;
      continue;
    }
    if (first.startsWith('cmpxchg')) counts.cmpxchg += 1;
    if (/^cas[a-z]*$/.test(first)) counts.cas += 1;
    if (tokens.some((t) => t.startsWith('__aarch64_cas8_'))) counts.cas8 += 1;
    if (Object.hasOwn(counts, first)) counts[first] += 1;
  }
  return counts;
}

function extractFunctions(asmText, target) {
  // A function key maps to the concatenation (in label order) of every block
  // whose label contains the key — deterministic across variants.
  const res = {};
  for (const key of FUNCTION_KEYS) {
    const blocks = parseBlocks(asmText).filter((b) => LABEL_MATCHERS[key].some((m) => b.label.includes(m)));
    const norm = blocks.flatMap((b) => normalizeBlock(b.lines, target));
    res[key] = {
      found: blocks.length > 0,
      blockCount: blocks.length,
      labels: blocks.map((b) => b.label),
      normalizedText: norm.join('\n'),
      sha256_16: sha256hex(norm.join('\n')).slice(0, 16),
      instrCount: norm.length,
      counts: countFamilies(norm),
    };
  }
  return res;
}

// ── Codegen mode ────────────────────────────────────────────────────────────
// Toolchain-observed lowering facts (rustc 1.97.0 / LLVM 22, aarch64-linux-gnu,
// verified by direct inspection of the emitted asm):
//   * DEFAULT feature set (baseline armv8-a): both compare_exchange and
//     compare_exchange_weak lower to OUTLINED atomic calls (`bl
//     __aarch64_cas8_acq` / `bl __aarch64_cas8_rel`); there are NO inline
//     ldaxr/stlxr instructions. After normalization, cas_weak's push/pop
//     blocks are byte-identical to base's — strong vs weak CAS is a full
//     codegen identity on this lowering.
//   * -C target-feature=+lse: each CAS lowers to a single casl/casa
//     instruction (2 casa + 2 casl across push+pop), zero __aarch64_cas8
//     calls, zero ldaxr/stlxr. cas_weak == base here too.
//   * Links ordering: base has ldar/stlr for RegistryShapedStorage accesses
//     (residual ldar in relaxed = pop_index_impl's own 64-bit Acquire HEAD
//     load (`head_ref.load(Ordering::Acquire)`), which must remain); links_relaxed drops link ldar to 0 / link stlr to 0.
// The cas_weak/pop_success_relaxed identity asserts below are DELIBERATE and
// load-bearing: they are self-updating negative controls. If a future toolchain reintroduces an inline
// LL/SC lowering where weak differs from strong, these asserts FAIL loudly
// and make the CAS lowering change visible instead of silently hiding it.
function modeCodegen(args, header) {
  const impSrc = snapshotText(header, 'crates/tagged-index-stack/src/imp.rs');
  const libSrc = snapshotText(header, 'crates/tagged-index-stack/src/lib.rs');
  const wrapperTemplate = snapshotText(header, 'crates/tagged-index-stack/scripts/tis_p3_ab/codegen_wrapper.rs.tmpl');
  verifyAllAnchorsOnce(impSrc);

  const scratchBase = makeScratchRoot();
  const root = scratchRoot(args, scratchBase);
  bindRunFlags(header, scratchBase);

  const isAarch64 = args.target.startsWith('aarch64');
  // aarch64 gets a second feature-set axis; supported x86_64 uses default only.
  const featureSets = isAarch64 ? ['default', 'lse'] : ['default'];

  const logLines = [headerComment(header)];
  if (isAarch64) {
    logLines.push(
      '// TOOLCHAIN-OBSERVED LOWERING (verified on emitted asm, rustc 1.97.0 / LLVM 22):',
      '//   default feature set: CAS = outlined __aarch64_cas8_acq/rel calls (no inline ldaxr/stlxr);',
      '//   +lse: CAS = single casl/casa instructions (zero outlined calls, zero ldaxr/stlxr);',
      '//   strong compare_exchange == compare_exchange_weak after normalization on BOTH feature',
      '//   sets. The cas_weak and pop_success_relaxed sha-identity asserts below are DELIBERATE',
      '//   negative controls: if a toolchain change makes either differ, they fail loudly.',
      '// Byte-exact sha identity is asserted for cas_weak/pop_success_relaxed and',
      '// store_elided untouched probes; links_relaxed and store_elided push use',
      '// semantic/activation oracles rather than hardcoded instruction counts.',
      '// push/pop: removing a link acquire/release legitimately shifts register allocation.',
      '// For links_relaxed the oracle instead asserts the exact acquire/release instruction',
      '// DELTA formulas derived from the base run: pop ldar == base_pop_ldar - base_load_next_ldar',
      '// (residual >= 1 is the head Acquire load, positively proving head ordering untouched);',
      '// push stlr == 0 with base stlr >= 1; plain ldr/str >= 1 where the link ordering was',
      '// dropped; CAS counts unchanged vs base.',
      '',
    );
  }

  const csvRows = [['target', 'features', 'function', 'variant', 'sha256_16', 'instr_count', 'ldar', 'stlr', 'ldaxr', 'stlxr', 'cmpxchg', 'cas', 'cas8', 'identical_to_base', 'source_input_digest', 'source_inputs_at_head', 'head_sha', 'tree_sha', 'toolchain', 'profile_id', 'bundle_id', 'production_rustflags_b64', 'activation_rustflags_b64', 'cargo_encoded_rustflags', 'sanitized_env_sha256', 'sanitized_env_b64', 'host_context_sha256', 'host_context_b64', 'host_context_before_timing_sha256', 'host_context_before_timing_b64', 'host_context_after_timing_sha256', 'host_context_after_timing_b64']];

  // Compile one feature set: variant -> { asmText, funcs, fallback }.
  function compileFeatureSet(fset) {
    const froot = fset === 'default' ? root : path.join(root, fset);
    freshDir(froot, scratchBase);
    const out = {};
    for (const variant of VARIANTS) {
      const vdir = path.join(froot, variant);
      freshDir(vdir, scratchBase);
      const imp = materializeImp(impSrc, variant);
      const wrapperSrc = materializeTemplate(wrapperTemplate, variant);
      fs.writeFileSync(path.join(vdir, 'lib.rs'), libSrc);
      fs.writeFileSync(path.join(vdir, 'imp.rs'), imp);
      fs.writeFileSync(path.join(vdir, 'force_codegen.rs'), wrapperSrc);

      const outFile = path.join(vdir, `${variant}.s`);
      let fallback = false;
      const baseArgs = [
        '--edition=2021', '--crate-type=lib', `--crate-name=tis_p3ab_${variant}`,
         '--emit=asm', '-C', 'opt-level=3', '-C', 'lto=thin', '-C', 'embed-bitcode=yes',
         '-C', 'codegen-units=1',
         '-C', 'debug-assertions=off', '-C', 'symbol-mangling-version=v0',
         ...rustcRemapArgs(scratchBase),
        ...(fset === 'lse' ? ['-C', 'target-feature=+lse'] : []),
        '--target', args.target, '-o', outFile, path.join(vdir, 'force_codegen.rs'),
      ];
      const directEnv = directRustcChildEnv();
      let r = spawnSync('rustc', baseArgs, { cwd: vdir, encoding: 'utf8', env: directEnv });
      if (r.status !== 0) {
        if (r.stderr.includes('symbol-mangling-version')) {
          fallback = true;
          const retryArgs = baseArgs.filter((a, i) => !(baseArgs[i] === 'symbol-mangling-version=v0' || (a === '-C' && baseArgs[i + 1] === 'symbol-mangling-version=v0')));
          r = spawnSync('rustc', retryArgs, { cwd: vdir, encoding: 'utf8', env: directEnv });
        }
        if (r.status !== 0) {
          process.stderr.write(r.stderr ?? '');
          fail(`rustc failed for variant ${variant} (target ${args.target}, features ${fset}), exit ${r.status}`);
        }
      }
      out[variant] = {
        asmText: fs.readFileSync(outFile, 'utf8'),
        funcs: extractFunctions(fs.readFileSync(outFile, 'utf8'), args.target),
        fallback,
      };
    }
    return out;
  }

  function assertProbeMatrix(fset, variants) {
    for (const variant of VARIANTS) {
      assert(variants[variant] !== undefined, `${args.target}/${fset}: missing variant ${variant}`);
      for (const key of FUNCTION_KEYS) {
        const fn = variants[variant].funcs[key];
        assert(fn.found, `${args.target}/${fset}/${variant}: missing function block ${key}`);
        assert(fn.instrCount > 0, `${args.target}/${fset}/${variant}/${key}: empty normalized block`);
      }
    }
  }

  // Oracles for one aarch64 feature set. `variants` is that set's compile map.
  function runAarch64Oracles(fset, variants) {
    const tag = `aarch64[${fset}]`;
    function identical(variant, key) {
      return variants[variant].funcs[key].sha256_16 === variants.base.funcs[key].sha256_16;
    }
    function printNorm(variant, key) {
      logLines.push(`--- normalized text: features=${fset} variant=${variant} function=${key} ---`);
      logLines.push(variants[variant].funcs[key].normalizedText || '(empty)');
      logLines.push('');
    }
    function shaFail(variant, key, what) {
      return `oracle failed (${tag}): ${what}: sha256(${key}, base)=${variants.base.funcs[key].sha256_16} != sha256(${key}, ${variant})=${variants[variant].funcs[key].sha256_16}`;
    }

    // (a) base CAS shape.
    for (const key of ['push_index_impl', 'pop_index_impl']) {
      const f = variants.base.funcs[key];
      if (fset === 'default') {
        // Outlined-atomics lowering: each block must contain >= 1 __aarch64_cas8_ call.
        if (f.counts.cas8 < 1) {
          printNorm('base', key);
          fail(`${tag} oracle failed: base ${key} __aarch64_cas8_ outlined-call count=${f.counts.cas8}, expected >= 1 (observed rustc 1.97.0 lowering is outlined atomics)`);
        }
      } else {
        // LSE lowering: single-instruction CAS, zero outlined calls.
        if (f.counts.cas < 1) {
          printNorm('base', key);
          fail(`${tag} oracle failed: base ${key} cas-family instruction count=${f.counts.cas}, expected >= 1 (+lse must lower CAS to casl/casa)`);
        }
        if (f.counts.cas8 !== 0) {
          printNorm('base', key);
          fail(`${tag} oracle failed: base ${key} __aarch64_cas8 call count=${f.counts.cas8}, expected 0 under +lse`);
        }
      }
    }

    // (b) links_relaxed ordering (identical story on both feature sets).
    //
    // ORACLE LEVEL NOTE: byte-exact sha identity is the WRONG oracle level for
    // links_relaxed push/pop. Removing a link acquire/release instruction
    // legitimately shifts register allocation, so the correct ground truth is
    // the exact acquire/release instruction DELTA (derived from the base run,
    // never hardcoded):
    //   * POP: the link Acquire contribution (== base load_next's ldar count)
    //     is removed; the residual relaxed ldar count is the head's own Acquire
    //     load and MUST be >= 1 — this positively proves the head ordering was
    //     untouched. A plain relaxed link load (ldr) must appear.
    //   * PUSH: base must contain link Release stores (stlr >= 1); all of them
    //     disappear (stlr == 0) and a plain relaxed link store (str) appears.
    //   * LOAD_NEXT/STORE_NEXT standalone blocks: base has the Acquire/Release
    //     instruction (>= 1); relaxed has none and a plain ldr/str instead.
    // Byte identity remains asserted for the negative controls; store_elided
    // additionally proves that only its push body is active.
    for (const key of ['load_next', 'store_next']) {
      const mne = key === 'load_next' ? 'ldar' : 'stlr';
      const plain = key === 'load_next' ? 'ldr' : 'str';
      const bf = variants.base.funcs[key];
      const rf = variants.links_relaxed.funcs[key];
      {
        if (bf.counts[mne] < 1) {
          printNorm('base', key);
          fail(`${tag} oracle failed: base ${key} ${mne}=${bf.counts[mne]}, expected >= 1`);
        }
        if (rf.counts[mne] !== 0) {
          printNorm('links_relaxed', key);
          fail(`${tag} oracle failed: links_relaxed ${key} ${mne}=${rf.counts[mne]}, expected 0 (a relaxed link cell must carry no ldar/stlr at all)`);
        }
        if (rf.counts[plain] < 1) {
          printNorm('links_relaxed', key);
          fail(`${tag} oracle failed: links_relaxed ${key} ${plain}=${rf.counts[plain]}, expected >= 1`);
        }
        logLines.push(`${tag}: links oracle basis: FUNCTION BLOCKS (per-function counts) for ${key}`);
      }
    }
    // POP: link Acquire contribution removed exactly; head Acquire load remains.
    for (const key of ['pop_index_impl']) {
      const bf = variants.base.funcs[key];
      const rf = variants.links_relaxed.funcs[key];
      const basePopLdar = bf.counts.ldar;
      const baseLoadNextLdar = variants.base.funcs.load_next.found
        ? variants.base.funcs.load_next.counts.ldar
        : 0;
      const expected = basePopLdar - baseLoadNextLdar;
      if (rf.counts.ldar !== expected) {
        printNorm('base', key);
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: pop ldar=${rf.counts.ldar}, expected ${expected} (= base pop ldar ${basePopLdar} - base load_next ldar ${baseLoadNextLdar}; the link Acquire contribution removed exactly)`);
      }
      if (rf.counts.ldar < 1) {
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: pop residual ldar=${rf.counts.ldar}, expected >= 1 — this residual IS pop_index_impl's own 64-bit Acquire HEAD load (imp.rs \`head_ref.load(Ordering::Acquire)\`), so the oracle positively proves the HEAD ordering was untouched`);
      }
      if (rf.counts.ldr < 1) {
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: pop ldr=${rf.counts.ldr}, expected >= 1 (plain relaxed link load must be present)`);
      }
      const baseCas = fset === 'default' ? bf.counts.cas8 : bf.counts.cas;
      const relCas = fset === 'default' ? rf.counts.cas8 : rf.counts.cas;
      if (relCas !== baseCas) {
        printNorm('base', key);
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: pop cas count=${relCas}, expected ${baseCas} (links substitution must not touch the CAS)`);
      }
    }
    // PUSH: all link Release stores removed; plain relaxed store appears.
    for (const key of ['push_index_impl']) {
      const bf = variants.base.funcs[key];
      const rf = variants.links_relaxed.funcs[key];
      if (bf.counts.stlr < 1) {
        printNorm('base', key);
        fail(`${tag} links_relaxed oracle failed: base push stlr=${bf.counts.stlr}, expected >= 1 (link Release stores must exist in base)`);
      }
      if (rf.counts.stlr !== 0) {
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: push stlr=${rf.counts.stlr}, expected 0 (link Release stores must be gone)`);
      }
      if (rf.counts.str < 1) {
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: push str=${rf.counts.str}, expected >= 1 (plain relaxed link store must be present)`);
      }
      const baseCas = fset === 'default' ? bf.counts.cas8 : bf.counts.cas;
      const relCas = fset === 'default' ? rf.counts.cas8 : rf.counts.cas;
      if (relCas !== baseCas) {
        printNorm('base', key);
        printNorm('links_relaxed', key);
        fail(`${tag} links_relaxed oracle failed: push cas count=${relCas}, expected ${baseCas} (links substitution must not touch the CAS)`);
      }
    }

    // (c) Negative controls: these source-ordering changes must remain byte-
    // identical after normalization on both AArch64 feature sets. The failure
    // ordering determines the target CAS lowering here, so this is an
    // identity oracle rather than an instruction-count guess.
    for (const key of FUNCTION_KEYS) {
      if (!identical('cas_weak', key)) {
        printNorm('base', key);
        printNorm('cas_weak', key);
        logLines.push(`CAS equivalence reopened: weak CAS now diverges from strong on ${tag} (${key}).`);
        fail(`${shaFail('cas_weak', key, 'deliberate strong==weak codegen identity assert')} Add cas_weak back to WALLCLOCK_VARIANTS before timing it.`);
      }
      if (!identical('pop_success_relaxed', key)) {
        printNorm('base', key);
        printNorm('pop_success_relaxed', key);
        fail(`${shaFail('pop_success_relaxed', key, 'deliberate pop-success Relaxed codegen identity assert')} Keep this variant out of WALLCLOCK_VARIANTS unless its target oracle is intentionally redesigned.`);
      }
    }
    // store_elided is an active scratch candidate: only the push body may
    // differ; all standalone hooks and pop must remain byte-identical. The
    // push distinction is asserted by normalized assembly/block identity, with
    // no hardcoded instruction-count expectation for any target feature set.
    for (const key of ['load_next', 'store_next', 'pop_index_impl']) {
      if (!identical('store_elided', key)) {
        printNorm('base', key);
        printNorm('store_elided', key);
        fail(`${shaFail('store_elided', key, 'store_elided touched an unrelated probe')}`);
      }
    }
    if (identical('store_elided', 'push_index_impl')) {
      printNorm('base', 'push_index_impl');
      printNorm('store_elided', 'push_index_impl');
      fail(`${tag} store_elided push codegen is byte-identical to base; the scratch candidate is not active`);
    }
  }

  // Compile + run oracles per feature set.
  const allRuns = {}; // fset -> { variants, fallback }
  for (const fset of featureSets) {
    const variants = compileFeatureSet(fset);
    // Fail closed before any cross-variant comparison or fallback oracle.
    assertProbeMatrix(fset, variants);
    if (isAarch64) {
      logLines.push(`================ FEATURE SET: ${fset} ================`);
      logLines.push('');
      runAarch64Oracles(fset, variants);
    } else {
      // Supported x86_64 target: link/CAS/pop-ordering negative controls are
      // all identity; store_elided changes only the push body.
      for (const key of FUNCTION_KEYS) {
        for (const variant of ['links_relaxed', 'cas_weak', 'pop_success_relaxed']) {
          if (variants[variant].funcs[key].sha256_16 !== variants.base.funcs[key].sha256_16) {
            logLines.push(`--- normalized text: variant=${variant} function=${key} ---`);
            logLines.push(variants[variant].funcs[key].normalizedText || '(empty)');
            logLines.push('');
            fail(`x86_64 identity oracle failed for function ${key} variant ${variant}`);
          }
        }
        if (key !== 'push_index_impl' && variants.store_elided.funcs[key].sha256_16 !== variants.base.funcs[key].sha256_16) {
          fail(`x86_64 store_elided identity oracle failed for untouched function ${key}`);
        }
      }
      if (variants.store_elided.funcs.push_index_impl.sha256_16 === variants.base.funcs.push_index_impl.sha256_16) {
        fail('x86_64 store_elided push codegen is byte-identical to base; scratch candidate is not active');
      }
      for (const key of ['push_index_impl', 'pop_index_impl']) {
        if (variants.base.funcs[key].counts.cmpxchg < 1) {
          fail(`x86_64 oracle failed: base ${key} has cmpxchg count ${variants.base.funcs[key].counts.cmpxchg}, expected >= 1 (CAS instruction absent)`);
        }
      }
    }
    allRuns[fset] = { variants, fallback: VARIANTS.filter((v) => variants[v].fallback) };
  }

  // ── Log body ──────────────────────────────────────────────────────────────
  logLines.push('');
  for (const fset of featureSets) {
    const { variants, fallback } = allRuns[fset];
    logLines.push(`===== features: ${fset} — variants =====`);
    logLines.push(`symbol-mangling-v0 fallback used: ${fallback.join(', ') || 'none'}`);
    logLines.push('');
    const identical = (variant, key) => variants[variant].funcs[key].sha256_16 === variants.base.funcs[key].sha256_16;
    for (const variant of VARIANTS) {
      logLines.push(`===== variant: ${variant} =====`);
      for (const key of FUNCTION_KEYS) {
        const f = variants[variant].funcs[key];
        logLines.push(`function ${key}: blocks=${f.blockCount} labels=${JSON.stringify(f.labels)}`);
        logLines.push(`  sha256_16=${f.sha256_16} instr_count=${f.instrCount}`);
        logLines.push(`  families=${JSON.stringify(f.counts)}`);
        if (variant === 'base' || !identical(variant, key)) {
          logLines.push(`--- normalized text: features=${fset} variant=${variant} function=${key} ---`);
          logLines.push(f.normalizedText || '(empty)');
          logLines.push('');
        } else {
          logLines.push(`  (normalized text omitted: identical to base)`);
        }
      }
      logLines.push('');
    }
    logLines.push('');
  }

  // ── Derived markdown table (with asserted arithmetic) ─────────────────────
  const md = [];
  md.push(`# TIS registry-shaped link-ordering/CAS codegen table — target ${args.target}`);
  md.push('');
  md.push('delta% is instr_count relative to base for the same function (derived, rounded to 3 decimals).');
  md.push('');
  md.push('| target | features | function | variant | sha256_16 | instr_count | ldar | stlr | ldaxr | stlxr | cmpxchg | cas | cas8 | delta% vs base |');
  md.push('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|');
  for (const fset of featureSets) {
    const { variants } = allRuns[fset];
    const identical = (variant, key) => variants[variant].funcs[key].sha256_16 === variants.base.funcs[key].sha256_16;
    for (const key of FUNCTION_KEYS) {
      for (const variant of VARIANTS) {
        const f = variants[variant].funcs[key];
        let deltaPct = '—';
        if (variant !== 'base') {
          const b = variants.base.funcs[key].instrCount;
          if (b > 0) {
            // Plain rounded computation, not a checked oracle: an assert
            // recomputing this exact expression and comparing it to itself
            // could never fail.
            const ratio = Math.round((f.instrCount / b) * 1000) / 1000;
            deltaPct = String(Math.round((ratio - 1) * 1000) / 10);
          }
        }
        md.push(`| ${args.target} | ${fset} | ${key} | ${variant} | ${f.sha256_16} | ${f.instrCount} | ${f.counts.ldar} | ${f.counts.stlr} | ${f.counts.ldaxr} | ${f.counts.stlxr} | ${f.counts.cmpxchg} | ${f.counts.cas} | ${f.counts.cas8} | ${deltaPct} |`);
        csvRows.push([args.target, fset, key, variant, f.sha256_16, f.instrCount, f.counts.ldar, f.counts.stlr, f.counts.ldaxr, f.counts.stlxr, f.counts.cmpxchg, f.counts.cas, f.counts.cas8, String(identical(variant, key)), header.sourceInputDigest, header.sourceInputsAtHead, header.identity.headSha, header.identity.treeSha, header.toolchain, header.profileId, header.bundleId, utf8Base64(header.effectiveRustflags.production), utf8Base64(header.effectiveRustflags.activation), header.effectiveRustflags.cargoEncodedRustflags, header.sanitizedEnvSha256, header.sanitizedEnvB64, header.hostContextSha256, header.hostContextB64, '', '', '', '']);
      }
    }
  }
  const mdText = md.join('\n') + '\n';
  logLines.push(mdText);
  const expectedCodegenRows = featureSets.length * VARIANTS.length * FUNCTION_KEYS.length;
  assert(csvRows.length - 1 === expectedCodegenRows, `codegen row count ${csvRows.length - 1} != exact Cartesian count ${expectedCodegenRows}`);

  // ── Artifacts ─────────────────────────────────────────────────────────────
  const csvText = csvRows.map((r) => r.join(',')).join('\n') + '\n';
  const asmAll = featureSets
    .map((fset) => VARIANTS.map((v) => `# ===== features: ${fset} variant: ${v} =====\n` + allRuns[fset].variants[v].asmText).join('\n'))
    .join('\n');
  logLines.push(`// csv-sha256: ${sha256hex(csvText)}`);
  logLines.push(`// asm-sha256: ${sha256hex(asmAll)}`);
  logLines.push(`// artifact-state: complete`);
  const codegenLog = logLines.join('\n') + '\n';
  stageAndPublishArtifacts(header, scratchBase, {
    [`_raw_tis_p3_ab_${args.target}_codegen.log`]: codegenLog,
    [`_raw_tis_p3_ab_${args.target}_codegen.s.all`]: asmAll,
    [`TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_${args.target}.csv`]: csvText,
  });

  console.log(mdText);
  console.log(`registry-shaped codegen OK: target=${args.target} (artifacts staged and published)`);
}

// ── Wallclock mode ──────────────────────────────────────────────────────────
function modeWallclock(args, header) {
  const impSrc = snapshotText(header, 'crates/tagged-index-stack/src/imp.rs');
  const libSrc = snapshotText(header, 'crates/tagged-index-stack/src/lib.rs');
  const cargoTmpl = snapshotText(header, 'crates/tagged-index-stack/scripts/tis_p3_ab/scratch_Cargo.toml.tmpl');
  const harnessTemplate = snapshotText(header, 'crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs');
  verifyAllAnchorsOnce(impSrc);
  if (args.target !== header.rustcHost) {
    fail(`--mode wallclock builds natively for rustc host ${header.rustcHost}; --target must match it exactly (got ${JSON.stringify(args.target)})`);
  }

  let { threads, windowMs, samples } = args;
  let smoke = args.smoke;
  if (smoke) {
    threads = 4;
    windowMs = 100;
    samples = 1;
  } else {
    assert(samples >= MIN_COMPARATIVE_SAMPLES, `--samples must be >= ${MIN_COMPARATIVE_SAMPLES} for comparative evidence (got ${samples})`);
    assert(samples % WALLCLOCK_VARIANTS.length === 0, `--samples must be a multiple of WALLCLOCK_VARIANTS.length=${WALLCLOCK_VARIANTS.length} (got ${samples})`);
  }
  assert(
    Number.isSafeInteger(threads) && threads >= 1 && threads <= MAX_THREADS,
    `--threads must be a safe integer in 1..=${MAX_THREADS} (got ${threads})`,
  );
  assert(
    Number.isSafeInteger(windowMs) && windowMs >= 50 && windowMs <= MAX_WINDOW_MS,
    `--window-ms must be a safe integer in 50..=${MAX_WINDOW_MS} (got ${windowMs})`,
  );
  assert(
    Number.isSafeInteger(samples) && samples >= 1 && samples <= MAX_SAMPLES,
    `--samples must be a safe integer in 1..=${MAX_SAMPLES} (got ${samples})`,
  );

  const scratchBase = makeScratchRoot();
  const root = scratchRoot(args, scratchBase);
  freshDir(root, scratchBase);
  bindRunFlags(header, scratchBase);

  const logLines = [headerComment(header)];
  logLines.push(`run params: threads=${threads} window_ms=${windowMs} samples=${samples} smoke=${smoke}`);
  logLines.push(`variant schedule: production variants [${WALLCLOCK_VARIANTS.join(', ')}], rotated by sample position`);
  logLines.push('');

  // Materialize a production timing crate and a separate cfg-enabled
  // activation crate for each wall-clock variant.
  const crates = {};
  for (const variant of WALLCLOCK_VARIANTS) {
    const crateName = `tis_p3ab_${variant}`;
    const cdir = path.join(root, variant);
    freshDir(cdir, scratchBase);
    fs.writeFileSync(path.join(cdir, 'Cargo.toml'), cargoTmpl.replaceAll('{{CRATE_NAME}}', crateName));
    fs.mkdirSync(path.join(cdir, 'src', 'bin'), { recursive: true });
    const imp = materializeImp(impSrc, variant);
    fs.writeFileSync(path.join(cdir, 'lib.rs'), libSrc);
    fs.writeFileSync(path.join(cdir, 'imp.rs'), imp);
    fs.writeFileSync(path.join(cdir, 'src', 'bin', 'harness.rs'), materializeTemplate(harnessTemplate, variant).replaceAll('{{CRATE_NAME}}', crateName));

    // Pin the target dir INSIDE the scratch crate: a global CARGO_TARGET_DIR
    // (common on dev machines) would otherwise send both variants'
    // artifacts to one shared directory — collisions and wrong exe paths.
    const build = runEvidenceCargoBuild(
      header,
      header.actualRustflagTokens.production,
      ['build', '--release', '--target', args.target],
      cdir,
      path.join(cdir, 'target'),
      `production cargo build for variant ${variant}`,
    );
    if (build.status !== 0) {
      process.stderr.write(build.stderr ?? '');
      fail(`cargo build --release --target ${args.target} failed for variant ${variant} (cwd ${cdir})`);
    }
    logLines.push(`built variant ${variant}: cargo build --release --target ${args.target} OK (cwd SCRATCH/${variant})`);
    const exeName = `harness${process.platform === 'win32' ? '.exe' : ''}`;
    crates[variant] = {
      productionExe: path.join(cdir, 'target', args.target, 'release', exeName),
      samples: [],
    };

    const activationName = `tis_p3ab_${variant}_activation`;
    const adir = path.join(root, `${variant}-activation`);
    freshDir(adir, scratchBase);
    fs.writeFileSync(path.join(adir, 'Cargo.toml'), cargoTmpl.replaceAll('{{CRATE_NAME}}', activationName));
    fs.mkdirSync(path.join(adir, 'src', 'bin'), { recursive: true });
    fs.writeFileSync(path.join(adir, 'lib.rs'), libSrc);
    fs.writeFileSync(path.join(adir, 'imp.rs'), imp);
    fs.writeFileSync(path.join(adir, 'src', 'bin', 'harness.rs'), materializeTemplate(harnessTemplate, variant).replaceAll('{{CRATE_NAME}}', activationName));
    const activationBuild = runEvidenceCargoBuild(
      header,
      header.actualRustflagTokens.activation,
      ['build', '--release', '--target', args.target],
      adir,
      path.join(adir, 'target'),
      `activation cargo build for variant ${variant}`,
    );
    if (activationBuild.status !== 0) {
      process.stderr.write(activationBuild.stderr ?? '');
      fail(`instrumented cargo build --release --target ${args.target} failed for variant ${variant} (cwd ${adir})`);
    }
    crates[variant].activationExe = path.join(adir, 'target', args.target, 'release', exeName);
    logLines.push(`built variant ${variant}: production + cfg-enabled activation binaries (cwd SCRATCH/${variant})`);
  }
  logLines.push('');
  const hostContextBeforeTiming = hostContextEvidenceParts(captureHostContext(), 'wallclock before-timing host context');
  assert(hostContextBeforeTiming.sha256 === header.hostContextSha256 && hostContextBeforeTiming.b64 === header.hostContextB64, 'host context changed before wallclock timing');
  header.hostContextBeforeTiming = hostContextBeforeTiming;
  logLines.push(`// host-context-before-timing: ${hostContextBeforeTiming.json}`);
  logLines.push(`// host-context-before-timing-sha256: ${hostContextBeforeTiming.sha256}`);
  logLines.push(`// host-context-before-timing-b64: ${hostContextBeforeTiming.b64}`);

  function runHarness(exe, variant, label, sample) {
    const env = {
      ...process.env,
      TIS_AB_THREADS: String(threads),
      TIS_AB_WINDOW_MS: String(windowMs),
      TIS_AB_SMOKE: smoke ? '1' : '0',
      TIS_AB_ACTIVATION_ORACLE: label === 'activation' || label === 'smoke-activation' ? '1' : '0',
    };
    const harnessTimeoutMs = HARNESS_WARMUP_MS + HARNESS_TIMEOUT_SLACK_MS + windowMs;
    const r = spawnSync(exe, [], {
      env,
      encoding: 'utf8',
      timeout: harnessTimeoutMs,
    });
    if (r.error?.code === 'ETIMEDOUT') {
      process.stderr.write(r.stderr ?? '');
      fail(`harness timed out for variant=${variant} label=${label} sample=${sample}`);
    }
    if (r.error) fail(`harness spawn failed for variant=${variant} label=${label}: ${r.error.message}`);
    if (r.status !== 0) {
      process.stderr.write(r.stderr ?? '');
      fail(`harness exited ${r.status} for variant=${variant} label=${label}`);
    }
    const records = [];
    for (const line of r.stdout.split(/\r?\n/)) {
      try {
        const json = JSON.parse(line);
        if (json && typeof json === 'object' && Object.hasOwn(json, 'ops_per_sec') && Object.hasOwn(json, 'variant')) records.push(json);
      } catch { /* ignore non-JSON diagnostics */ }
    }
    assert(records.length === 1, `harness emitted ${records.length} matching JSON records; expected exactly one for variant=${variant} label=${label}`);
    const rec = records[0];
    const activation = label === 'activation' || label === 'smoke-activation';
    const natural = label === 'natural-activation';
    if (activation) assertActivationRecord(rec, variant, `harness variant=${variant} label=${label}`, label === 'smoke-activation');
    else if (natural) assertNaturalActivationRecord(rec, variant, `harness variant=${variant} label=${label}`, threads, windowMs);
    else assertProductionRecord(rec, variant, `harness variant=${variant} label=${label}`);
    return { rec, stdout: r.stdout };
  }

  if (smoke) {
    logLines.push('SMOKE: non-comparative build/activation check; no timing ratio or verdict emitted.');
    for (const variant of WALLCLOCK_VARIANTS) {
      const activation = runHarness(crates[variant].activationExe, variant, 'smoke-activation', 1);
      assert(activation.rec.smoke === true, `smoke activation JSON did not preserve smoke=true for variant=${variant}`);
      logLines.push(`--- variant=${variant} smoke activation stdout (not timing evidence) ---`);
      logLines.push(activation.stdout.replaceAll(repoRoot, 'REPO'));
    }
    logLines.push('// artifact-state: complete');
    stageAndPublishArtifacts(header, scratchBase, {
      [`_raw_tis_p3_ab_${args.target}_wallclock_smoke.log`]: logLines.join('\n') + '\n',
    });
    console.log(`registry-shaped smoke OK: target=${args.target} (non-comparative; no evidence)`);
    return;
  }

  // Run the harness per SAMPLE, rotating the variant order every sample.
  //
  // The outer loop is the sample index; each sample runs all variants in
  // a deterministically rotated order. This prevents block boundaries from
  // co-varying with machine warm-up, DVFS, thermal throttling, or background
  // load, and gives each variant every position once per cycle. The schedule
  // favors no variant. The realized order is logged per sample (below) so
  // downstream analysis can pair samples across variants by POSITION instead
  // of trusting independent per-variant block medians as if they were
  // sampled under identical conditions.
  for (let sample = 1; sample <= samples; sample++) {
    const shift = (sample - 1) % WALLCLOCK_VARIANTS.length;
    const order = WALLCLOCK_VARIANTS.map((_, i) => WALLCLOCK_VARIANTS[(i + shift) % WALLCLOCK_VARIANTS.length]);
    logLines.push(`--- sample=${sample} realized variant order: ${order.join(' -> ')} ---`);
    for (const variant of order) {
      // Bounded child runtime. A harness that never exits (worker gone before
      // the done-barrier rendezvous — Barrier has
      // no poison — or any other hang) is killed here and failed loudly, not
      // allowed to hang CI/a dev machine indefinitely; never retried, never
      // reported as a sample.
      const { rec, stdout } = runHarness(crates[variant].productionExe, variant, 'production-timing', sample);
      logLines.push(`--- variant=${variant} sample=${sample} harness stdout (verbatim) ---`);
      logLines.push(stdout.replaceAll(repoRoot, 'REPO').replaceAll(scratchBase, 'SCRATCH'));
      // Re-derive the ratio the harness printed (asserted arithmetic).
      const derived = rec.ops_total / (rec.elapsed_ms / 1000);
      assert(
        Math.abs(derived - rec.ops_per_sec) < 0.02 * rec.ops_per_sec,
        `ops_per_sec mismatch for variant=${variant} sample=${sample}: reported ${rec.ops_per_sec}, derived ${derived}`,
      );
      assert(rec.ops_total > 0, `ops_total must be > 0 for variant=${variant} sample=${sample} (got ${rec.ops_total})`);
      assert(rec.elapsed_ms >= 0.5 * windowMs, `lateness guard: elapsed_ms ${rec.elapsed_ms} < 0.5*window_ms ${0.5 * windowMs} for variant=${variant} sample=${sample}`);
      crates[variant].samples.push({ sample, ...rec });
    }
  }
  const hostContextAfterTiming = hostContextEvidenceParts(captureHostContext(), 'wallclock after-timing host context');
  assert(hostContextAfterTiming.sha256 === hostContextBeforeTiming.sha256 && hostContextAfterTiming.b64 === hostContextBeforeTiming.b64, 'stable host context changed around wallclock timing');
  header.hostContextAfterTiming = hostContextAfterTiming;
  logLines.push(`// host-context-after-timing: ${hostContextAfterTiming.json}`);
  logLines.push(`// host-context-after-timing-sha256: ${hostContextAfterTiming.sha256}`);
  logLines.push(`// host-context-after-timing-b64: ${hostContextAfterTiming.b64}`);
  // Activation is a separate binary and separate observed windows. Its
  // counters and natural workload are never part of the production timing samples.
  for (const variant of WALLCLOCK_VARIANTS) {
    const activation = runHarness(crates[variant].activationExe, variant, 'activation', 'observed');
    crates[variant].activation = activation.rec;
    logLines.push(`--- variant=${variant} activation stdout (separate observed window) ---`);
    logLines.push(activation.stdout.replaceAll(repoRoot, 'REPO').replaceAll(scratchBase, 'SCRATCH'));
  }
  for (const variant of WALLCLOCK_VARIANTS) {
    const natural = runHarness(crates[variant].activationExe, variant, 'natural-activation', 'observed');
    crates[variant].natural = natural.rec;
    logLines.push(`--- variant=${variant} natural workload activation stdout (not timing evidence) ---`);
    logLines.push(natural.stdout.replaceAll(repoRoot, 'REPO').replaceAll(scratchBase, 'SCRATCH'));
  }

  // ── Summary (median; derived ratios) ──────────────────────────────────────
  function median(arr) {
    const s = [...arr].sort((a, b) => a - b);
    const n = s.length;
    return n % 2 === 1 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
  }
  const med = Object.fromEntries(WALLCLOCK_VARIANTS.map((v) => [v, median(crates[v].samples.map((s) => s.ops_per_sec))]));
  function ratioOf(v) {
    // Plain rounded computation, not a checked oracle:
    // an assert recomputing this exact expression and comparing it to
    // itself could never fail. Ratio VERIFICATION lives in --mode summary,
    // where the re-derived ratio is checked against the leg's own recorded
    // ratio_vs_base SUMMARY cell.
    return Math.round((med[v] / med.base) * 1000) / 1000;
  }

  const md = [];
  md.push(`# TIS production registry-shaped wallclock summary — target ${args.target}`);
  md.push('cas_weak and pop_success_relaxed are codegen negative controls; only base, links_relaxed, and store_elided are timed.');
  md.push('');
  md.push(`threads=${threads} window_ms=${windowMs} samples=${samples} smoke=${smoke}`);
  md.push('');
  md.push('| target | variant | median_ops_per_sec | ratio vs base | activation_push_retries | activation_pop_retries | activation_store_next_calls | natural_push_attempts | natural_store_next_calls | natural_store_elisions | natural_push_retries | natural_pop_retries |');
  md.push('|---|---|---|---|---|---|---|---|---|---|---|---|');
  for (const v of WALLCLOCK_VARIANTS) {
    md.push(`| ${args.target} | ${v} | ${med[v].toFixed(2)} | ${v === 'base' ? '1.0' : ratioOf(v).toFixed(3)} | ${crates[v].activation.activation_push_retries} | ${crates[v].activation.activation_pop_retries} | ${crates[v].activation.activation_store_next_calls} | ${crates[v].natural.natural_push_attempts} | ${crates[v].natural.natural_store_next_calls} | ${crates[v].natural.natural_store_elisions} | ${crates[v].natural.push_retries} | ${crates[v].natural.pop_retries} |`);
  }
  const mdText = md.join('\n') + '\n';
  logLines.push(mdText);

  // ── Artifacts ─────────────────────────────────────────────────────────────
  const csv = [['target', 'variant', 'source_variant', 'binary_kind', 'activation_probe', 'threads', 'window_ms', 'sample', 'ops_total', 'elapsed_ms', 'ops_per_sec', 'push_retries', 'pop_retries', 'activation_push_retries', 'activation_pop_retries', 'activation_store_next_calls', 'natural_push_attempts', 'natural_store_next_calls', 'natural_store_elisions', 'source_input_digest', 'source_inputs_at_head', 'head_sha', 'tree_sha', 'toolchain', 'profile_id', 'bundle_id', 'production_rustflags_b64', 'activation_rustflags_b64', 'cargo_encoded_rustflags', 'sanitized_env_sha256', 'sanitized_env_b64', 'host_context_sha256', 'host_context_b64', 'host_context_before_timing_sha256', 'host_context_before_timing_b64', 'host_context_after_timing_sha256', 'host_context_after_timing_b64', 'smoke']];
  for (const v of WALLCLOCK_VARIANTS) {
    for (const s of crates[v].samples) {
      csv.push([args.target, v, '', 'production', 'none', s.threads, s.window_ms, s.sample, s.ops_total, s.elapsed_ms, s.ops_per_sec, '', '', '', '', '', '', '', '', header.sourceInputDigest, header.sourceInputsAtHead, header.identity.headSha, header.identity.treeSha, header.toolchain, header.profileId, header.bundleId, utf8Base64(header.effectiveRustflags.production), utf8Base64(header.effectiveRustflags.activation), header.effectiveRustflags.cargoEncodedRustflags, header.sanitizedEnvSha256, header.sanitizedEnvB64, header.hostContextSha256, header.hostContextB64, hostContextBeforeTiming.sha256, hostContextBeforeTiming.b64, header.hostContextAfterTiming.sha256, header.hostContextAfterTiming.b64, 'false']);
    }
  }
  for (const v of WALLCLOCK_VARIANTS) {
    csv.push([args.target, v, v, 'SUMMARY', 'tag_only_retry', '', `median_ops_per_sec=${med[v].toFixed(2)}`, `ratio_vs_base=${v === 'base' ? '1.0' : ratioOf(v).toFixed(3)}`, '', '', '', '', '', crates[v].activation.activation_push_retries, crates[v].activation.activation_pop_retries, crates[v].activation.activation_store_next_calls, '', '', '', header.sourceInputDigest, header.sourceInputsAtHead, header.identity.headSha, header.identity.treeSha, header.toolchain, header.profileId, header.bundleId, utf8Base64(header.effectiveRustflags.production), utf8Base64(header.effectiveRustflags.activation), header.effectiveRustflags.cargoEncodedRustflags, header.sanitizedEnvSha256, header.sanitizedEnvB64, header.hostContextSha256, header.hostContextB64, hostContextBeforeTiming.sha256, hostContextBeforeTiming.b64, header.hostContextAfterTiming.sha256, header.hostContextAfterTiming.b64, 'false']);
    csv.push([args.target, v, v, 'SUMMARY', 'natural_workload', crates[v].natural.threads, crates[v].natural.window_ms, 'natural', '', '', '', crates[v].natural.push_retries, crates[v].natural.pop_retries, '', '', '', crates[v].natural.natural_push_attempts, crates[v].natural.natural_store_next_calls, crates[v].natural.natural_store_elisions, header.sourceInputDigest, header.sourceInputsAtHead, header.identity.headSha, header.identity.treeSha, header.toolchain, header.profileId, header.bundleId, utf8Base64(header.effectiveRustflags.production), utf8Base64(header.effectiveRustflags.activation), header.effectiveRustflags.cargoEncodedRustflags, header.sanitizedEnvSha256, header.sanitizedEnvB64, header.hostContextSha256, header.hostContextB64, hostContextBeforeTiming.sha256, hostContextBeforeTiming.b64, header.hostContextAfterTiming.sha256, header.hostContextAfterTiming.b64, 'false']);
  }
  const csvText = csv.map((r) => r.join(',')).join('\n') + '\n';
  logLines.push(`// csv-sha256: ${sha256hex(csvText)}`);
  logLines.push(`// artifact-state: complete`);
  stageAndPublishArtifacts(header, scratchBase, {
    [`_raw_tis_p3_ab_${args.target}_wallclock.log`]: logLines.join('\n') + '\n',
    [`TIS_LINK_ORDERING_WEAK_CAS_GATE_wallclock_${args.target}.csv`]: csvText,
  });

  console.log(mdText);
  console.log(`production wallclock OK: target=${args.target} variants=${WALLCLOCK_VARIANTS.join(',')} (cas_weak + pop_success_relaxed codegen negative controls excluded)`);
}

// ── Build-check mode ────────────────────────────────────────────────────────
// Static regression gate, not measurement evidence. It materializes one dirty-
// compatible snapshot and compiles every timing source variant plus its
// activation harness shape.
// Both Cargo builds name the verified rustc host explicitly. The same snapshot
// also materializes the independent codegen wrapper for metadata-only checking.
function modeBuildCheck(args, snapshot) {
  const impSrc = snapshotText(snapshot, 'crates/tagged-index-stack/src/imp.rs');
  const libSrc = snapshotText(snapshot, 'crates/tagged-index-stack/src/lib.rs');
  const cargoTmpl = snapshotText(snapshot, 'crates/tagged-index-stack/scripts/tis_p3_ab/scratch_Cargo.toml.tmpl');
  const harnessTmpl = snapshotText(snapshot, 'crates/tagged-index-stack/scripts/tis_p3_ab/harness_bin.rs');
  const codegenWrapperTmpl = snapshotText(snapshot, 'crates/tagged-index-stack/scripts/tis_p3_ab/codegen_wrapper.rs.tmpl');
  verifyAllAnchorsOnce(impSrc);
  const rustcVersion = runCapture('rustc', ['--version', '--verbose']).trim();
  const rustcHost = rustcHostFromVerbose(rustcVersion);

  const scratchBase = makeScratchRoot();
  const root = path.join(scratchBase, 'build-check');
  freshDir(root, scratchBase);
  bindRunFlags(snapshot, scratchBase);
  // Build every timing source variant. The production binaries are compile-
  // only; cfg activation binaries are executed through the deterministic
  // tag-only retry oracle, never through warmup or a timed window.
  for (const variant of WALLCLOCK_VARIANTS) {
    const variantRoot = variant === 'base' ? root : path.join(scratchBase, `build-check-${variant}`);
    if (variant !== 'base') freshDir(variantRoot, scratchBase);
    const crateName = `tis_p3ab_build_check_${variant}`;
    fs.writeFileSync(path.join(variantRoot, 'Cargo.toml'), cargoTmpl.replaceAll('{{CRATE_NAME}}', crateName));
    fs.mkdirSync(path.join(variantRoot, 'src', 'bin'), { recursive: true });
    fs.writeFileSync(path.join(variantRoot, 'lib.rs'), libSrc);
    fs.writeFileSync(path.join(variantRoot, 'imp.rs'), materializeImp(impSrc, variant));
    fs.writeFileSync(path.join(variantRoot, 'src', 'bin', 'harness.rs'), materializeTemplate(harnessTmpl, variant).replaceAll('{{CRATE_NAME}}', crateName));

    const productionBuild = spawnSync('cargo', ['build', '--target', rustcHost], {
      cwd: variantRoot,
      encoding: 'utf8',
      env: cargoChildEnv(snapshot.actualRustflagTokens.production, path.join(variantRoot, 'target-production'), snapshot.cargoHome),
    });
    if (productionBuild.status !== 0) {
      process.stderr.write(productionBuild.stderr ?? '');
      fail(`production cargo build --target ${rustcHost} failed for ${variant} (build-check mode, cwd ${variantRoot})`);
    }
    const activationBuild = spawnSync('cargo', ['build', '--target', rustcHost], {
      cwd: variantRoot,
      encoding: 'utf8',
      env: cargoChildEnv(snapshot.actualRustflagTokens.activation, path.join(variantRoot, 'target-activation'), snapshot.cargoHome),
    });
    if (activationBuild.status !== 0) {
      process.stderr.write(activationBuild.stderr ?? '');
      fail(`activation cargo build --target ${rustcHost} failed for ${variant} (build-check mode, cwd ${variantRoot})`);
    }
    const exeName = `harness${process.platform === 'win32' ? '.exe' : ''}`;
    const exe = path.join(variantRoot, 'target-activation', rustcHost, 'debug', exeName);
    const activationRun = spawnSync(exe, [], {
      cwd: variantRoot,
      encoding: 'utf8',
      timeout: 10_000,
      env: { ...sanitizedBaseChildEnv(), TIS_AB_ACTIVATION_ORACLE: '1' },
    });
    if (activationRun.error?.code === 'ETIMEDOUT') fail(`activation oracle timed out for ${variant} (build-check mode)`);
    if (activationRun.error) fail(`activation oracle spawn failed for ${variant}: ${activationRun.error.message}`);
    if (activationRun.status !== 0) {
      process.stderr.write(activationRun.stderr ?? '');
      fail(`activation oracle exited ${activationRun.status} for ${variant} (build-check mode)`);
    }
    const records = [];
    for (const line of activationRun.stdout.split(/\r?\n/)) {
      try {
        const json = JSON.parse(line);
        if (json && typeof json === 'object' && json.activation === true && Object.hasOwn(json, 'ops_per_sec')) records.push(json);
      } catch { /* ignore cargo/harness diagnostics */ }
    }
    assert(records.length === 1, `build-check activation oracle emitted ${records.length} matching JSON records; expected exactly one for ${variant}`);
    const rec = records[0];
    assertActivationRecord(rec, variant, `build-check activation variant=${variant}`, false);
    console.log(`build-check activation oracle OK: variant=${variant} target=${rustcHost}`);
  }
  console.log(`build-check mode OK: production + activation harness shapes target=${rustcHost} scratch=${root}`);

  // Second, independent check: compile the codegen wrapper for every source
  // variant directly with rustc, each in its own scratch directory. This
  // checks every exact anchor/rewrite without generating assembly artifacts.
  for (const variant of VARIANTS) {
    const cgRoot = path.join(scratchBase, `build-check-codegen-wrapper-${variant}`);
    freshDir(cgRoot, scratchBase);
    fs.writeFileSync(path.join(cgRoot, 'lib.rs'), libSrc);
    fs.writeFileSync(path.join(cgRoot, 'imp.rs'), materializeImp(impSrc, variant));
    fs.writeFileSync(path.join(cgRoot, 'force_codegen.rs'), materializeTemplate(codegenWrapperTmpl, variant));
    // cas_weak bypasses StackHead::compare_exchange; the other variants keep
    // strict dead-code diagnostics in this metadata-only check.
    const lintArgs = variant === 'cas_weak'
      ? ['-D', 'warnings', '-A', 'dead_code']
      : ['-D', 'warnings'];
    const cgBuild = spawnSync('rustc', [
      '--edition=2021', '--crate-type=lib', `--crate-name=tis_p3ab_build_check_codegen_${variant}`,
      '--emit=metadata', '-C', 'opt-level=3', '-C', 'lto=thin', '-C', 'embed-bitcode=yes',
      '-C', 'codegen-units=1', ...lintArgs,
      ...rustcRemapArgs(scratchBase),
      '-o', path.join(cgRoot, 'force_codegen.rmeta'), path.join(cgRoot, 'force_codegen.rs'),
    ], { cwd: cgRoot, encoding: 'utf8', env: directRustcChildEnv() });
    if (cgBuild.status !== 0) {
      process.stderr.write(cgBuild.stderr ?? '');
      fail(`rustc --emit=metadata failed for codegen variant ${variant} (build-check mode, cwd ${cgRoot})`);
    }
    console.log(`build-check mode OK: codegen wrapper variant=${variant} scratch=${cgRoot}`);
  }
}

// ── Summary mode ────────────────────────────────────────────────────────────
// Reads every per-leg CSV and its own raw-log provenance header, emits the
// one compact machine-readable companion CSV for the gate report. Fails
// loudly if any referenced artifact is missing. Every emitted ratio is
// re-derived from the CSV's own sample rows and asserted against the ratio
// the leg itself recorded. Missing files fail closed below.

function readCsvOrDie(file) {
  const p = path.join(docsPerfDir, file);
  if (!fs.existsSync(p)) fail(`summary mode: required artifact missing: docs/perf/${file}`);
  const lines = fs.readFileSync(p, 'utf8').split(/\r?\n/).filter((l) => l !== '');
  if (lines.length < 2) fail(`summary mode: ${file} has no data rows`);
  const header = lines[0].split(',');
  assert(header.every((name, i) => name !== '' && header.indexOf(name) === i), `${file}: malformed or duplicate CSV header`);
  return { file, header, rows: lines.slice(1).map((l) => {
    const cells = l.split(',');
    assert(cells.length === header.length, `${file}: row has ${cells.length} cells, header has ${header.length}`);
    return Object.fromEntries(header.map((h, i) => [h, cells[i] ?? '']));
  }) };
}

function parseSanitizedEnv(raw, file) {
  let state;
  try {
    state = JSON.parse(raw);
  } catch (error) {
    fail(`${file}: malformed sanitized-cargo-env JSON: ${error.message}`);
  }
  assert(state !== null && typeof state === 'object' && !Array.isArray(state), `${file}: sanitized-cargo-env must be an object`);
  assert(JSON.stringify(Object.keys(state).sort()) === JSON.stringify(['removedKeys', 'secretValuesLogged', 'setKeys']), `${file}: sanitized-cargo-env has unexpected fields`);
  assert(Array.isArray(state.removedKeys) && state.removedKeys.every((key) => typeof key === 'string' && isSanitizedEnvKey(key)), `${file}: sanitized-cargo-env has malformed removedKeys`);
  assert(new Set(state.removedKeys).size === state.removedKeys.length && JSON.stringify([...state.removedKeys].sort()) === JSON.stringify(state.removedKeys), `${file}: sanitized-cargo-env removedKeys are not unique and sorted`);
  assert(JSON.stringify(state.setKeys) === JSON.stringify(CANONICAL_CARGO_SET_KEYS), `${file}: sanitized-cargo-env setKeys differ from canonical policy`);
  assert(state.secretValuesLogged === false, `${file}: sanitized-cargo-env logged secret values`);
  const canonicalJson = canonicalSanitizedEnvJson(state);
  assert(raw === canonicalJson, `${file}: sanitized-cargo-env JSON is not canonical`);
  return {
    state,
    json: canonicalJson,
    sha256: sha256hex(canonicalJson),
    base64: utf8Base64(canonicalJson),
  };
}

function readRawProvenance(file, asmFile = null) {
  const p = path.join(docsPerfDir, file);
  if (!fs.existsSync(p)) fail(`summary mode: required raw log missing: docs/perf/${file}`);
  const text = fs.readFileSync(p, 'utf8');
  const line = (prefix) => text.split(/\r?\n/).find((entry) => entry.startsWith(prefix));
  const identityLine = line('// identity:');
  const bundleLine = line('// bundle-id:');
  const profileLine = line('// profile-id:');
  const modeLine = line('// mode:');
  const targetLine = line('// target:');
  const stateLine = line('// artifact-state:');
  const smokeLine = line('// smoke:');
  const sourceLine = line('// source-input-digest:');
  const sourceAtHeadLine = line('// source-inputs-at-head:');
  const toolchainLine = line('// toolchain:');
  const rustcHostLine = line('// rustc-host:');
  const productionRustflagsLine = line('// effective-production-rustflags:');
  const activationRustflagsLine = line('// effective-activation-rustflags:');
  const encodedRustflagsLine = line('// cargo-encoded-rustflags:');
  const sanitizedEnvLine = line('// sanitized-cargo-env:');
  const hostContextLine = line('// host-context:');
  const hostContextShaLine = line('// host-context-sha256:');
  const hostContextB64Line = line('// host-context-b64:');
  const hostContextBeforeLine = line('// host-context-before-timing:');
  const hostContextBeforeShaLine = line('// host-context-before-timing-sha256:');
  const hostContextBeforeB64Line = line('// host-context-before-timing-b64:');
  const hostContextAfterLine = line('// host-context-after-timing:');
  const hostContextAfterShaLine = line('// host-context-after-timing-sha256:');
  const hostContextAfterB64Line = line('// host-context-after-timing-b64:');
  const csvLine = line('// csv-sha256:');
  const asmLine = line('// asm-sha256:');
  assert(identityLine && bundleLine && profileLine && modeLine && targetLine && stateLine && smokeLine && sourceLine && sourceAtHeadLine && toolchainLine && rustcHostLine && productionRustflagsLine && activationRustflagsLine && encodedRustflagsLine && sanitizedEnvLine && hostContextLine && hostContextShaLine && hostContextB64Line && csvLine, `${file}: incomplete provenance header`);
  const identity = JSON.parse(identityLine.slice('// identity:'.length).trim());
  assert(JSON.stringify(Object.keys(identity)) === JSON.stringify(['capturedAt', 'headSha', 'treeSha', 'sourceSnapshotDigest', 'sourceInputsAtHead']), `${file}: identity JSON has non-canonical fields`);
  assert(typeof identity.capturedAt === 'string' && Number.isFinite(Date.parse(identity.capturedAt)), `${file}: identity capturedAt is malformed`);
  const sanitizedEnv = parseSanitizedEnv(sanitizedEnvLine.slice('// sanitized-cargo-env:'.length).trim(), file);
  let hostContext;
  try {
    hostContext = JSON.parse(hostContextLine.slice('// host-context:'.length).trim());
  } catch (error) {
    fail(`${file}: malformed host-context JSON: ${error.message}`);
  }
  const hostEvidence = hostContextEvidenceParts(hostContext, file);
  const hostContextSha256 = hostContextShaLine.slice('// host-context-sha256:'.length).trim();
  const hostContextB64 = hostContextB64Line.slice('// host-context-b64:'.length).trim();
  assert(hostEvidence.sha256 === hostContextSha256, `${file}: host-context SHA256 does not match canonical JSON`);
  assert(hostEvidence.b64 === hostContextB64, `${file}: host-context base64 does not match canonical JSON`);
  const parseOptionalHost = (jsonLine, shaLine, b64Line, prefix) => {
    assert((jsonLine === undefined) === (shaLine === undefined) && (jsonLine === undefined) === (b64Line === undefined), `${file}: incomplete ${prefix} host context linkage`);
    if (jsonLine === undefined) return null;
    let context;
    try {
      context = JSON.parse(jsonLine.slice(`// ${prefix}:`.length).trim());
    } catch (error) {
      fail(`${file}: malformed ${prefix} host-context JSON: ${error.message}`);
    }
    const evidence = hostContextEvidenceParts(context, file);
    const sha256 = shaLine.slice(`// ${prefix}-sha256:`.length).trim();
    const b64 = b64Line.slice(`// ${prefix}-b64:`.length).trim();
    assert(evidence.sha256 === sha256 && evidence.b64 === b64, `${file}: ${prefix} host-context linkage does not match canonical JSON`);
    return { json: evidence.json, sha256, b64 };
  };
  const hostContextBeforeTiming = parseOptionalHost(hostContextBeforeLine, hostContextBeforeShaLine, hostContextBeforeB64Line, 'host-context-before-timing');
  const hostContextAfterTiming = parseOptionalHost(hostContextAfterLine, hostContextAfterShaLine, hostContextAfterB64Line, 'host-context-after-timing');
  const provenance = {
    sourceInputDigest: sourceLine.slice('// source-input-digest:'.length).trim(),
    bundleId: bundleLine.slice('// bundle-id:'.length).trim(),
    profileId: profileLine.slice('// profile-id:'.length).trim(),
    mode: modeLine.slice('// mode:'.length).trim(),
    target: targetLine.slice('// target:'.length).trim(),
    artifactState: stateLine.slice('// artifact-state:'.length).trim(),
    smoke: smokeLine.slice('// smoke:'.length).trim() === 'true',
    sourceInputsAtHead: sourceAtHeadLine.slice('// source-inputs-at-head:'.length).trim() === 'true',
    headSha: identity.headSha,
    treeSha: identity.treeSha,
    toolchain: toolchainLine.slice('// toolchain:'.length).trim(),
    rustcHost: rustcHostLine.slice('// rustc-host:'.length).trim(),
    productionRustflags: JSON.parse(productionRustflagsLine.slice('// effective-production-rustflags:'.length).trim()),
    activationRustflags: JSON.parse(activationRustflagsLine.slice('// effective-activation-rustflags:'.length).trim()),
    cargoEncodedRustflags: encodedRustflagsLine.slice('// cargo-encoded-rustflags:'.length).trim(),
    sanitizedEnv: sanitizedEnv.state,
    sanitizedEnvSha256: sanitizedEnv.sha256,
    sanitizedEnvB64: sanitizedEnv.base64,
    hostContextJson: hostEvidence.json,
    hostContextSha256,
    hostContextB64,
    hostContextBeforeTiming,
    hostContextAfterTiming,
    rawText: text,
    csvSha256: csvLine.slice('// csv-sha256:'.length).trim(),
    asmSha256: asmLine ? asmLine.slice('// asm-sha256:'.length).trim() : null,
  };
  assert(/^[0-9a-f]{64}$/.test(provenance.sourceInputDigest), `${file}: malformed source-input digest`);
  assert(identity.sourceSnapshotDigest === provenance.sourceInputDigest && identity.sourceInputsAtHead === true, `${file}: identity is not tied to the captured source snapshot`);
  assert(provenance.sourceInputsAtHead === true, `${file}: source inputs are not canonical HEAD bytes`);
  assert(/^[0-9a-f]{40}$/.test(provenance.headSha), `${file}: malformed HEAD identity`);
  assert(/^[0-9a-f]{40}$/.test(provenance.treeSha), `${file}: malformed tree identity`);
  assert(/^[0-9a-f]{64}$/.test(provenance.bundleId), `${file}: malformed bundle id`);
  assert(provenance.bundleId === evidenceBundleId({
    headSha: provenance.headSha,
    treeSha: provenance.treeSha,
    sourceInputDigest: provenance.sourceInputDigest,
    target: provenance.target,
    mode: provenance.mode,
    profileId: provenance.profileId,
    toolchain: provenance.toolchain,
    productionRustflags: provenance.productionRustflags,
    activationRustflags: provenance.activationRustflags,
    cargoEncodedRustflags: provenance.cargoEncodedRustflags,
    hostContextSha256: provenance.hostContextSha256,
  }), `${file}: bundle id does not bind the canonical source/target/profile/toolchain/flags/host contract`);
  assert(provenance.profileId === PROFILE_ID, `${file}: non-canonical profile id`);
  assert(provenance.artifactState === 'complete', `${file}: artifact is not complete`);
  assert(provenance.toolchain.length > 0, `${file}: empty toolchain identity`);
  assert(provenance.rustcHost === rustcHostFromVerbose(provenance.toolchain.replaceAll(' | ', '\n')), `${file}: rustc host differs from toolchain identity`);
  assert(typeof provenance.productionRustflags === 'string', `${file}: malformed production RUSTFLAGS`);
  assert(typeof provenance.activationRustflags === 'string', `${file}: malformed activation RUSTFLAGS`);
  assert(provenance.cargoEncodedRustflags === 'canonical', `${file}: encoded RUSTFLAGS are not canonical`);
  assert(provenance.productionRustflags === CANONICAL_PRODUCTION_RUSTFLAGS, `${file}: production RUSTFLAGS display differs from canonical remap argv`);
  assert(provenance.activationRustflags === CANONICAL_ACTIVATION_RUSTFLAGS, `${file}: activation RUSTFLAGS display differs from canonical argv`);
  assert(/^[0-9a-f]{64}$/.test(provenance.sanitizedEnvSha256) && provenance.sanitizedEnvB64.length > 0, `${file}: malformed sanitized-env evidence`);
  assert(/^[0-9a-f]{64}$/.test(provenance.csvSha256), `${file}: malformed CSV digest`);
  if (provenance.mode === 'wallclock') {
    assert(provenance.hostContextBeforeTiming !== null && provenance.hostContextAfterTiming !== null, `${file}: wallclock raw log lacks before/after host context`);
    assert(provenance.hostContextBeforeTiming.sha256 === provenance.hostContextSha256 && provenance.hostContextAfterTiming.sha256 === provenance.hostContextSha256, `${file}: wallclock stable host context changed around timing`);
  } else {
    assert(provenance.hostContextBeforeTiming === null && provenance.hostContextAfterTiming === null, `${file}: non-wallclock raw log contains timing host context`);
  }
  if (asmFile !== null) {
    assert(provenance.mode === 'codegen', `${file}: codegen raw mode mismatch`);
    assert(provenance.asmSha256 !== null && /^[0-9a-f]{64}$/.test(provenance.asmSha256), `${file}: missing assembly digest`);
    const asmText = fs.readFileSync(path.join(docsPerfDir, asmFile), 'utf8');
    assert(sha256hex(asmText) === provenance.asmSha256, `${file}: assembly digest does not match ${asmFile}`);
  } else {
    assert(provenance.mode === 'wallclock', `${file}: wallclock raw mode mismatch`);
  }
  return provenance;
}

function assertCsvProvenance(csv, file, rawFile, provenance) {
  const csvText = fs.readFileSync(path.join(docsPerfDir, file), 'utf8');
  assert(sha256hex(csvText) === provenance.csvSha256, `${file}: CSV digest does not match ${rawFile}`);
  const required = ['source_input_digest', 'source_inputs_at_head', 'head_sha', 'tree_sha', 'toolchain', 'profile_id', 'bundle_id', 'production_rustflags_b64', 'activation_rustflags_b64', 'cargo_encoded_rustflags', 'sanitized_env_sha256', 'sanitized_env_b64', 'host_context_sha256', 'host_context_b64', 'host_context_before_timing_sha256', 'host_context_before_timing_b64', 'host_context_after_timing_sha256', 'host_context_after_timing_b64'];
  for (const column of required) assert(csv.header.includes(column), `${file}: missing ${column} provenance column`);
  for (const row of csv.rows) {
    assert(row.source_input_digest === provenance.sourceInputDigest, `${file}: source digest differs from ${rawFile}`);
    assert(row.source_inputs_at_head === 'true', `${file}: source_inputs_at_head is not true`);
    assert(row.head_sha === provenance.headSha, `${file}: HEAD differs from ${rawFile}`);
    assert(row.tree_sha === provenance.treeSha, `${file}: tree differs from ${rawFile}`);
    assert(row.toolchain === provenance.toolchain, `${file}: toolchain differs from ${rawFile}`);
    assert(row.profile_id === provenance.profileId, `${file}: profile differs from ${rawFile}`);
    assert(row.bundle_id === provenance.bundleId, `${file}: bundle differs from ${rawFile}`);
    assert(row.production_rustflags_b64 === utf8Base64(provenance.productionRustflags), `${file}: production RUSTFLAGS differ from ${rawFile}`);
    assert(row.activation_rustflags_b64 === utf8Base64(provenance.activationRustflags), `${file}: activation RUSTFLAGS differ from ${rawFile}`);
    assert(row.cargo_encoded_rustflags === provenance.cargoEncodedRustflags, `${file}: encoded RUSTFLAGS contract differs from ${rawFile}`);
    assert(row.sanitized_env_sha256 === provenance.sanitizedEnvSha256, `${file}: sanitized-env fingerprint differs from ${rawFile}`);
    assert(row.sanitized_env_b64 === provenance.sanitizedEnvB64, `${file}: sanitized-env base64 differs from ${rawFile}`);
    assert(row.host_context_sha256 === provenance.hostContextSha256, `${file}: host context SHA256 differs from ${rawFile}`);
    assert(row.host_context_b64 === provenance.hostContextB64, `${file}: host context base64 differs from ${rawFile}`);
    if (provenance.mode === 'wallclock') {
      assert(row.host_context_before_timing_sha256 === provenance.hostContextBeforeTiming.sha256 && row.host_context_before_timing_b64 === provenance.hostContextBeforeTiming.b64, `${file}: before-timing host context differs from ${rawFile}`);
      assert(row.host_context_after_timing_sha256 === provenance.hostContextAfterTiming.sha256 && row.host_context_after_timing_b64 === provenance.hostContextAfterTiming.b64, `${file}: after-timing host context differs from ${rawFile}`);
    } else {
      assert(row.host_context_before_timing_sha256 === '' && row.host_context_before_timing_b64 === '' && row.host_context_after_timing_sha256 === '' && row.host_context_after_timing_b64 === '', `${file}: non-wallclock CSV contains timing host context`);
    }
  }
}

function modeSummary(args) {
  const summaryRows = [['kind', 'target', 'features', 'function_or_variant', 'variant', 'metric', 'value', 'unit']];
  const emit = (kind, target, features, fov, variant, metric, value, unit) =>
    summaryRows.push([kind, target, features, fov, variant, metric, String(value), unit]);

  const wallclockTarget = args.target;
  const legSpecs = [
    ...CODEGEN_TARGETS.map((target) => ({
      kind: 'codegen', target,
      csvFile: `TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_${target}.csv`,
      rawFile: `_raw_tis_p3_ab_${target}_codegen.log`,
      asmFile: `_raw_tis_p3_ab_${target}_codegen.s.all`,
    })),
    {
      kind: 'wallclock', target: wallclockTarget,
      csvFile: `TIS_LINK_ORDERING_WEAK_CAS_GATE_wallclock_${wallclockTarget}.csv`,
      rawFile: `_raw_tis_p3_ab_${wallclockTarget}_wallclock.log`,
    },
  ];
  const legs = legSpecs.map((spec) => {
    const provenance = readRawProvenance(spec.rawFile, spec.asmFile ?? null);
    const csv = readCsvOrDie(spec.csvFile);
    assertCsvProvenance(csv, spec.csvFile, spec.rawFile, provenance);
    assert(provenance.target === spec.target, `${spec.rawFile}: raw target mismatch`);
    return { ...spec, provenance, csv };
  });
  const reference = legs[0].provenance;
  for (const leg of legs) {
    // Per-leg removedKeys reflect harmless ambient noise and may differ. Each
    // leg validates its own canonical sanitized-env form above; cross-leg
    // equality applies only to the effective child contract and provenance.
    for (const key of ['sourceInputDigest', 'sourceInputsAtHead', 'headSha', 'treeSha', 'toolchain', 'profileId', 'smoke', 'productionRustflags', 'activationRustflags', 'cargoEncodedRustflags', 'hostContextSha256', 'hostContextB64']) {
      assert(leg.provenance[key] === reference[key], `summary mode: leg ${leg.target} mixes ${key} with another leg`);
    }
    assert(leg.provenance.bundleId.length === 64, `summary mode: ${leg.target} lacks a complete run/bundle id`);
    emit('identity', leg.target, '', '', '', 'source_input_digest', leg.provenance.sourceInputDigest, 'sha256');
    emit('identity', leg.target, '', '', '', 'source_inputs_at_head', leg.provenance.sourceInputsAtHead, 'boolean');
    emit('identity', leg.target, '', '', '', 'head_sha', leg.provenance.headSha, 'sha');
    emit('identity', leg.target, '', '', '', 'tree_sha', leg.provenance.treeSha, 'sha');
    emit('identity', leg.target, '', '', '', 'toolchain', leg.provenance.toolchain, 'identity');
    emit('identity', leg.target, '', '', '', 'rustc_host', leg.provenance.rustcHost, 'target');
    emit('identity', leg.target, '', '', '', 'production_rustflags_b64', utf8Base64(leg.provenance.productionRustflags), 'base64');
    emit('identity', leg.target, '', '', '', 'activation_rustflags_b64', utf8Base64(leg.provenance.activationRustflags), 'base64');
    emit('identity', leg.target, '', '', '', 'cargo_encoded_rustflags', leg.provenance.cargoEncodedRustflags, 'state');
    emit('identity', leg.target, '', '', '', 'sanitized_env_sha256', leg.provenance.sanitizedEnvSha256, 'sha256');
    emit('identity', leg.target, '', '', '', 'sanitized_env_b64', leg.provenance.sanitizedEnvB64, 'base64');
    emit('identity', leg.target, '', '', '', 'host_context_sha256', leg.provenance.hostContextSha256, 'sha256');
    emit('identity', leg.target, '', '', '', 'host_context_b64', leg.provenance.hostContextB64, 'base64');
    emit('identity', leg.target, '', '', '', 'profile_id', leg.provenance.profileId, 'profile');
    emit('identity', leg.target, '', '', '', 'bundle_id', leg.provenance.bundleId, 'sha256');
  }

  // (a)+(b) codegen legs.
  const familyCols = ['ldar', 'stlr', 'ldaxr', 'stlxr', 'cmpxchg', 'cas', 'cas8'];
  const codegenCsvs = legs.filter((leg) => leg.kind === 'codegen');
  const familyNonzero = new Set(); // families nonzero somewhere across all codegen CSVs
  for (const { csv } of codegenCsvs) {
    for (const fam of familyCols) {
      if (csv.rows.some((r) => Number(r[fam]) !== 0)) familyNonzero.add(fam);
    }
  }
  for (const { target, csv } of codegenCsvs) {
    const file = csv.file;
    const expectedHeader = ['target', 'features', 'function', 'variant', 'sha256_16', 'instr_count', 'ldar', 'stlr', 'ldaxr', 'stlxr', 'cmpxchg', 'cas', 'cas8', 'identical_to_base', 'source_input_digest', 'source_inputs_at_head', 'head_sha', 'tree_sha', 'toolchain', 'profile_id', 'bundle_id', 'production_rustflags_b64', 'activation_rustflags_b64', 'cargo_encoded_rustflags', 'sanitized_env_sha256', 'sanitized_env_b64', 'host_context_sha256', 'host_context_b64', 'host_context_before_timing_sha256', 'host_context_before_timing_b64', 'host_context_after_timing_sha256', 'host_context_after_timing_b64'];
    assert(JSON.stringify(csv.header) === JSON.stringify(expectedHeader), `${file}: unexpected header ${csv.header.join(',')}`);
    const expectedFeatures = target.startsWith('aarch64') ? ['default', 'lse'] : ['default'];
    const expectedKeys = new Set(expectedFeatures.flatMap((features) => FUNCTION_KEYS.flatMap((fn) => VARIANTS.map((variant) => `${features}|${fn}|${variant}`))));
    const seenKeys = new Set();
    assert(csv.rows.length === expectedKeys.size, `${file}: expected exact Cartesian row count ${expectedKeys.size}, got ${csv.rows.length}`);
    for (const r of csv.rows) {
      assert(r.target === target, `${file}: row target ${r.target} != ${target}`);
      const key = `${r.features}|${r.function}|${r.variant}`;
      assert(expectedKeys.has(key) && !seenKeys.has(key), `${file}: duplicate, missing, or extra key ${key}`);
      seenKeys.add(key);
      assert(expectedFeatures.includes(r.features) && FUNCTION_KEYS.includes(r.function) && VARIANTS.includes(r.variant), `${file}: malformed codegen key ${key}`);
      for (const numeric of ['instr_count', ...familyCols]) {
        const value = Number(r[numeric]);
        assert(Number.isSafeInteger(value) && value >= 0, `${file}: malformed ${numeric} for ${key}`);
      }
      assert(/^[0-9a-f]{16}$/.test(r.sha256_16) && (r.identical_to_base === 'true' || r.identical_to_base === 'false'), `${file}: malformed codegen identity for ${key}`);
      emit('codegen', target, r.features, r.function, r.variant, 'instr_count', r.instr_count, 'instructions');
      for (const fam of familyCols) {
        if (familyNonzero.has(fam) && Number(r[fam]) !== 0) {
          emit('codegen', target, r.features, r.function, r.variant, fam, r[fam], 'instructions');
        }
      }
    }
    // (b) per (target, features, function) identity facts.
    const groups = new Map();
    for (const r of csv.rows) {
      const k = `${r.features}|${r.function}`;
      if (!groups.has(k)) groups.set(k, {});
      groups.get(k)[r.variant] = r;
    }
    for (const [k, byVariant] of groups) {
      const [features, fn] = k.split('|');
      assert(Object.keys(byVariant).length === VARIANTS.length, `${file}: incomplete variant set for ${k}`);
      assert('cas_weak' in byVariant, `${file}: missing cas_weak row for ${k}`);
      for (const variant of VARIANTS) {
        const expectedIdentity = byVariant[variant].sha256_16 === byVariant.base.sha256_16;
        assert(byVariant[variant].identical_to_base === String(expectedIdentity), `${file}: identical_to_base disagrees with SHA equality for ${k}/${variant}`);
      }
      assert(byVariant.cas_weak.identical_to_base === 'true', `${file}: cas_weak negative control diverged for ${k}`);
      assert(byVariant.pop_success_relaxed.identical_to_base === 'true', `${file}: pop_success_relaxed negative control diverged for ${k}`);
      if (fn !== 'push_index_impl') {
        assert(byVariant.store_elided.identical_to_base === 'true', `${file}: store_elided touched ${fn}`);
      } else {
        assert(byVariant.store_elided.identical_to_base === 'false', `${file}: store_elided push is not codegen-distinguishable`);
      }
      emit('codegen_identity', target, features, fn, 'cas_weak', 'identical_to_base', Number(byVariant.cas_weak.sha256_16 === byVariant.base.sha256_16), 'boolean');
      if (target.startsWith('x86_64')) {
        assert('links_relaxed' in byVariant, `${file}: missing links_relaxed row for ${k}`);
        emit('codegen_identity', target, features, fn, 'links_relaxed', 'identical_to_base', Number(byVariant.links_relaxed.sha256_16 === byVariant.base.sha256_16), 'boolean');
      }
    }
  }

  // (c) wallclock production leg: medians re-derived from sample rows, ratios
  // re-derived from the medians, both asserted against the leg's own SUMMARY.
  // `--target` selects the provided wallclock CSV.
  const wallclockLeg = legs.find((leg) => leg.kind === 'wallclock');
  assert(wallclockLeg.provenance.smoke === false, `${wallclockLeg.csv.file}: smoke output cannot be evidence`);
  assert(wallclockLeg.provenance.target === wallclockLeg.provenance.rustcHost, `${wallclockLeg.csv.file}: wallclock target differs from rustc host`);
  const wcFile = wallclockLeg.csv.file;
  const wc = wallclockLeg.csv;
  const rawActivationRecords = [];
  for (const line of wallclockLeg.provenance.rawText.split(/\r?\n/)) {
    try {
      const record = JSON.parse(line);
      if (record && typeof record === 'object' && record.activation === true) rawActivationRecords.push(record);
    } catch { /* raw logs also contain prose and markdown */ }
  }
  assert(rawActivationRecords.length === WALLCLOCK_VARIANTS.length * 2, `${wcFile}: raw log must contain exactly two activation records per variant`);
  for (const variant of WALLCLOCK_VARIANTS) {
    const deterministic = rawActivationRecords.filter((record) => record.variant === variant && record.activation_probe === 'tag_only_retry');
    const natural = rawActivationRecords.filter((record) => record.variant === variant && record.activation_probe === 'natural_workload');
    assert(deterministic.length === 1 && natural.length === 1, `${wcFile}: raw log activation record count is not exact for ${variant}`);
    assertActivationRecord(deterministic[0], variant, `${wcFile}: raw deterministic activation ${variant}`, false);
    assertNaturalActivationRecord(natural[0], variant, `${wcFile}: raw natural activation ${variant}`, natural[0].threads, natural[0].window_ms);
  }
  const wcHeader = ['target', 'variant', 'source_variant', 'binary_kind', 'activation_probe', 'threads', 'window_ms', 'sample', 'ops_total', 'elapsed_ms', 'ops_per_sec', 'push_retries', 'pop_retries', 'activation_push_retries', 'activation_pop_retries', 'activation_store_next_calls', 'natural_push_attempts', 'natural_store_next_calls', 'natural_store_elisions', 'source_input_digest', 'source_inputs_at_head', 'head_sha', 'tree_sha', 'toolchain', 'profile_id', 'bundle_id', 'production_rustflags_b64', 'activation_rustflags_b64', 'cargo_encoded_rustflags', 'sanitized_env_sha256', 'sanitized_env_b64', 'host_context_sha256', 'host_context_b64', 'host_context_before_timing_sha256', 'host_context_before_timing_b64', 'host_context_after_timing_sha256', 'host_context_after_timing_b64', 'smoke'];
  assert(JSON.stringify(wc.header) === JSON.stringify(wcHeader), `${wcFile}: unexpected header ${wc.header.join(',')}`);
  function median(arr) {
    const s = [...arr].sort((a, b) => a - b);
    const n = s.length;
    return n % 2 === 1 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
  }
  const summaryRowsWc = {};
  const timingRows = [];
  for (const r of wc.rows) {
    if (r.binary_kind === 'SUMMARY') {
      assert(WALLCLOCK_VARIANTS.includes(r.variant) && r.source_variant === r.variant, `${wcFile}: malformed SUMMARY variant identity`);
      assert(r.target === wallclockTarget, `${wcFile}: SUMMARY target ${r.target} != ${wallclockTarget}`);
      assert(r.activation_probe === 'tag_only_retry' || r.activation_probe === 'natural_workload', `${wcFile}: SUMMARY activation probe is not recognized`);
      const summaryKey = `${r.variant}|${r.activation_probe}`;
      assert(summaryRowsWc[summaryKey] === undefined, `${wcFile}: duplicate SUMMARY ${summaryKey}`);
      if (r.activation_probe === 'tag_only_retry') {
        const expectedStoreCalls = r.variant === 'store_elided' ? 2 : 3;
        assert(r.threads === '' && r.window_ms.startsWith('median_ops_per_sec=') && r.sample.startsWith('ratio_vs_base='), `${wcFile}: deterministic SUMMARY timing cells are malformed`);
        assert(r.push_retries === '' && r.pop_retries === '', `${wcFile}: deterministic SUMMARY ordinary retry fields must be blank`);
        assert(r.activation_push_retries === '1' && r.activation_pop_retries === '0', `${wcFile}: deterministic SUMMARY retry fields are not exact`);
        assert(r.activation_store_next_calls === String(expectedStoreCalls), `${wcFile}: deterministic SUMMARY store_next_calls is not exact for ${r.variant}`);
        assert(r.natural_push_attempts === '' && r.natural_store_next_calls === '' && r.natural_store_elisions === '', `${wcFile}: deterministic SUMMARY contains natural fields`);
      } else {
        assert(r.threads !== '' && r.window_ms !== '' && r.sample === 'natural', `${wcFile}: natural SUMMARY parameter cells are malformed`);
        assert(r.ops_total === '' && r.elapsed_ms === '' && r.ops_per_sec === '', `${wcFile}: natural SUMMARY mixes throughput/elapsed into timing columns`);
        assert(r.activation_push_retries === '' && r.activation_pop_retries === '' && r.activation_store_next_calls === '', `${wcFile}: natural SUMMARY contains deterministic fields`);
        assert(Number.isSafeInteger(Number(r.push_retries)) && Number(r.push_retries) >= 0 && Number.isSafeInteger(Number(r.pop_retries)) && Number(r.pop_retries) >= 0, `${wcFile}: natural SUMMARY retry counts are malformed`);
        for (const field of ['natural_push_attempts', 'natural_store_next_calls', 'natural_store_elisions']) {
          const value = Number(r[field]);
          assert(Number.isSafeInteger(value) && value >= 0, `${wcFile}: natural SUMMARY ${field} is malformed`);
        }
        const attempts = Number(r.natural_push_attempts);
        const stores = Number(r.natural_store_next_calls);
        const elisions = Number(r.natural_store_elisions);
        assert(attempts > 0 && stores <= attempts && elisions === attempts - stores, `${wcFile}: natural SUMMARY store arithmetic is invalid`);
        if (r.variant === 'store_elided') assert(stores < attempts && elisions > 0, `${wcFile}: natural SUMMARY store_elided has no elisions`);
        else assert(stores === attempts && elisions === 0, `${wcFile}: natural SUMMARY ${r.variant} store arithmetic is not exact`);
      }
      assert(r.smoke === 'false', `${wcFile}: SUMMARY row cannot be smoke evidence`);
      summaryRowsWc[summaryKey] = r;
      continue;
    }
    assert(r.target === wallclockTarget && r.source_variant === '' && r.binary_kind === 'production' && WALLCLOCK_VARIANTS.includes(r.variant), `${wcFile}: non-production timing row`);
    assert(r.activation_probe === 'none' && r.source_inputs_at_head === 'true' && r.push_retries === '' && r.pop_retries === '', `${wcFile}: timing row is not production evidence`);
    assert(r.activation_push_retries === '' && r.activation_pop_retries === '' && r.activation_store_next_calls === '' && r.natural_push_attempts === '' && r.natural_store_next_calls === '' && r.natural_store_elisions === '', `${wcFile}: timing row contains activation evidence`);
    assert(r.smoke === 'false', `${wcFile}: timing row is not production evidence`);
    assert(Number.isSafeInteger(Number(r.threads)) && Number(r.threads) >= 1 && Number(r.threads) <= MAX_THREADS, `${wcFile}: malformed threads`);
    assert(Number.isSafeInteger(Number(r.window_ms)) && Number(r.window_ms) >= 50 && Number(r.window_ms) <= MAX_WINDOW_MS, `${wcFile}: malformed window_ms`);
    assert(Number.isSafeInteger(Number(r.sample)) && Number(r.sample) >= 1 && Number(r.sample) <= MAX_SAMPLES, `${wcFile}: malformed sample id`);
    for (const field of ['ops_total', 'elapsed_ms', 'ops_per_sec']) {
      const value = Number(r[field]);
      assert(Number.isFinite(value) && value > 0, `${wcFile}: ${field} must be finite and positive`);
    }
    const derived = Number(r.ops_total) / (Number(r.elapsed_ms) / 1000);
    const reported = Number(r.ops_per_sec);
    assert(Math.abs(derived - reported) < 0.02 * reported, `${wcFile}: ops_per_sec mismatch for ${r.variant}: reported ${reported}, derived ${derived}`);
    timingRows.push(r);
  }
  assert(Object.keys(summaryRowsWc).length === WALLCLOCK_VARIANTS.length * 2, `${wcFile}: expected exactly two SUMMARY activation records per production variant`);
  assert(timingRows.length >= MIN_COMPARATIVE_SAMPLES * WALLCLOCK_VARIANTS.length, `${wcFile}: fewer than ${MIN_COMPARATIVE_SAMPLES} samples per variant`);
  assert(timingRows.length % WALLCLOCK_VARIANTS.length === 0, `${wcFile}: timing row count is not divisible by ${WALLCLOCK_VARIANTS.length}`);
  const parameterSets = new Set(timingRows.map((r) => `${r.threads}|${r.window_ms}|false`));
  assert(parameterSets.size === 1, `${wcFile}: timing rows do not share threads/window/smoke=false`);
  const [timingThreads, timingWindow] = [...parameterSets][0].split('|');
  for (const variant of WALLCLOCK_VARIANTS) {
    const naturalSummary = summaryRowsWc[`${variant}|natural_workload`];
    assert(naturalSummary.threads === timingThreads && naturalSummary.window_ms === timingWindow, `${wcFile}: natural SUMMARY parameters differ from timing parameters for ${variant}`);
  }
  const sampleIds = new Map(WALLCLOCK_VARIANTS.map((v) => [v, timingRows.filter((r) => r.variant === v).map((r) => Number(r.sample))]));
  const sampleCount = sampleIds.get(WALLCLOCK_VARIANTS[0]).length;
  assert(sampleCount >= MIN_COMPARATIVE_SAMPLES && sampleCount % WALLCLOCK_VARIANTS.length === 0, `${wcFile}: invalid configured sample count ${sampleCount}`);
  const expectedSampleSet = Array.from({ length: sampleCount }, (_, i) => i + 1).join(',');
  for (const variant of WALLCLOCK_VARIANTS) {
    const ids = sampleIds.get(variant).sort((a, b) => a - b);
    assert(ids.join(',') === expectedSampleSet, `${wcFile}: variant ${variant} does not have the same sequential sample set`);
  }
  const meds = {};
  for (const v of WALLCLOCK_VARIANTS) {
    const samples = wc.rows.filter((r) => r.binary_kind !== 'SUMMARY' && r.variant === v);
    assert(samples.length === sampleCount, `${wcFile}: variant ${v} has inconsistent sample count`);
    meds[v] = median(samples.map((s) => Number(s.ops_per_sec)));
    assert(Number.isFinite(meds[v]) && meds[v] > 0, `${wcFile}: invalid median for ${v}`);
    emit('wallclock', wallclockTarget, '', '', v, 'median_ops_per_sec', meds[v].toFixed(2), 'ops/s');
  }
  for (const v of WALLCLOCK_VARIANTS) {
    const summary = summaryRowsWc[`${v}|tag_only_retry`];
    const naturalSummary = summaryRowsWc[`${v}|natural_workload`];
    const rawDeterministic = rawActivationRecords.find((record) => record.variant === v && record.activation_probe === 'tag_only_retry');
    const rawNatural = rawActivationRecords.find((record) => record.variant === v && record.activation_probe === 'natural_workload');
    const summaryCell = (prefix) => Object.values(summary).find((c) => typeof c === 'string' && c.startsWith(`${prefix}=`))?.split('=')[1];
    const statedMedian = Number(summaryCell('median_ops_per_sec'));
    const activationPush = Number(summary.activation_push_retries);
    const activationPop = Number(summary.activation_pop_retries);
    const activationStores = Number(summary.activation_store_next_calls);
    assert(Number.isFinite(statedMedian) && statedMedian > 0, `${wcFile}: invalid median SUMMARY for ${v}`);
    assert(statedMedian.toFixed(2) === meds[v].toFixed(2), `${wcFile}: median SUMMARY disagrees for ${v}`);
    assert(activationPush === 1, `${wcFile}: activation push must equal 1 for ${v}`);
    assert(activationPop === 0, `${wcFile}: activation pop must equal 0 for ${v}`);
    assert(activationStores === (v === 'store_elided' ? 2 : 3), `${wcFile}: activation store_next_calls is not exact for ${v}`);
    assert(summary.activation_push_retries === String(rawDeterministic.activation_push_retries) && summary.activation_pop_retries === String(rawDeterministic.activation_pop_retries) && summary.activation_store_next_calls === String(rawDeterministic.activation_store_next_calls), `${wcFile}: deterministic SUMMARY differs from raw activation record for ${v}`);
    assert(Number(naturalSummary.natural_push_attempts) > 0, `${wcFile}: natural SUMMARY missing attempts for ${v}`);
    assert(naturalSummary.threads === String(rawNatural.threads) && naturalSummary.window_ms === String(rawNatural.window_ms), `${wcFile}: natural SUMMARY parameters differ from raw natural record for ${v}`);
    assert(Number(naturalSummary.natural_push_attempts) === Number(rawNatural.ops_total) + Number(rawNatural.push_retries), `${wcFile}: natural SUMMARY attempts differ from raw ops_total + push_retries for ${v}`);
    for (const field of ['natural_push_attempts', 'natural_store_next_calls', 'natural_store_elisions', 'push_retries', 'pop_retries']) {
      assert(naturalSummary[field] === String(rawNatural[field]), `${wcFile}: natural SUMMARY ${field} differs from raw natural record for ${v}`);
    }
    const stated = Object.values(summary).find((c) => typeof c === 'string' && c.startsWith('ratio_vs_base='))?.split('=')[1];
    assert(stated !== undefined, `${wcFile}: no ratio_vs_base SUMMARY cell for variant ${v}`);
    const r = Math.round((meds[v] / meds.base) * 1000) / 1000;
    assert(Number.isFinite(r) && r > 0, `${wcFile}: non-finite or non-positive ratio for ${v}`);
    assert(Number.isFinite(Number(stated)) && Number(stated) > 0, `${wcFile}: non-finite or non-positive stated ratio for ${v}`);
    assert(Math.abs(r - Number(stated)) < 5e-4, `${wcFile}: ratio_vs_base for ${v}: leg says ${stated}, re-derived ${r}`);
    emit('wallclock', wallclockTarget, '', '', v, 'ratio_vs_base', r.toFixed(3), 'ratio');
  }

  const summaryText = summaryRows.map((r) => r.join(',')).join('\n') + '\n';
  const scratchBase = makeScratchRoot();
  stageAndPublishArtifacts(null, scratchBase, {
    'TIS_LINK_ORDERING_WEAK_CAS_GATE_summary.csv': summaryText,
  });
  console.log(`exact summary OK: ${summaryRows.length - 1} validated rows -> docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE_summary.csv`);
}

// ── Main ────────────────────────────────────────────────────────────────────
// The whole dispatch lives in one try/catch/finally so
// the invocation's scratch root is removed on EVERY exit path — success,
// fail()-driven fatal error, or unexpected exception. This replaces
// success-only cleanup inside mode functions. --keep-scratch opts out
// (inspect a failed run's scratch tree); the default must never leak.
// The finally block is also the only place reporting the scratch-tree
// lifecycle: the outcome is reported here, after it
// actually happened — never speculatively, before cleanup runs.
let args = null;
try {
  args = parseArgs(process.argv.slice(2));
  if (args.mode === 'summary') {
    modeSummary(args);
  } else if (args.mode === 'build-check') {
    modeBuildCheck(args, captureSnapshotContext(args));
  } else {
    const header = captureEvidenceHeader(args);
    if (args.mode === 'codegen') modeCodegen(args, header);
    else modeWallclock(args, header);
  }
} catch (e) {
  if (!(e instanceof RunnerFatalError)) throw e; // real bug: full stack trace
  console.error(`tis_p3_ab_runner: FATAL: ${e.message}`);
  process.exitCode = 1;
} finally {
  if (activeScratchBase !== null) {
    if (args !== null && args.keepScratch) {
      console.error(`tis_p3_ab_runner: --keep-scratch: scratch tree left in place for inspection: ${activeScratchBase}`);
    } else {
      fs.rmSync(activeScratchBase, { recursive: true, force: true });
      console.error(`tis_p3_ab_runner: scratch tree removed: ${activeScratchBase}`);
    }
  }
}
