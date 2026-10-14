import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as C3Support from './r20_prf01_c3_support.mjs';

const CHECK_CFGS = [
  "cfg(r20_prf01_c3_oracle)",
  "cfg(r20_prf01_c3_producer_control)",
  "cfg(r19_prf01_producer_control)",
];
const REQUIRED_TARGETS = {
  'example:r20_prf01_c3_oracle': ['production', 'bench-internals', 'internals'],
  'bench:r20_prf01_c3_cells_iai': ['alloc-global', 'bench-internals', 'internals'],
  'bench:r20_prf01_trim_iai': ['production', 'bench-internals', 'internals'],
  'bench:r20_prf01_controls_iai': ['alloc-global', 'bench-internals', 'internals'],
};

function namedBlocks(text, kind) {
  const lines = text.split(/(?<=\n)/);
  const blocks = [];
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim() !== `[[${kind}]]`) continue;
    let end = i + 1;
    while (end < lines.length && !/^\s*\[\[?[^\]]+\]\]?\s*(?:#.*)?(?:\r?\n)?$/.test(lines[end])) end++;
    const body = lines.slice(i + 1, end).join('');
    const name = /^\s*name\s*=\s*"([^"]+)"/m.exec(body)?.[1];
    blocks.push({ name, text: lines.slice(i, end).join('') });
    i = end - 1;
  }
  return blocks;
}

function checkCfgMerge(cargo) {
  const start = cargo.indexOf('unexpected_cfgs');
  if (start < 0) throw new Error('Cargo.toml missing [workspace.lints.rust] unexpected_cfgs');
  const lineStart = cargo.lastIndexOf('\n', start) + 1;
  const lineEnd = cargo.indexOf('\n', start);
  const end = lineEnd < 0 ? cargo.length : lineEnd;
  const line = cargo.slice(lineStart, end);
  const key = /check-cfg\s*=\s*\[([\s\S]*?)\]/.exec(line);
  if (!key) throw new Error('Cargo.toml unexpected_cfgs missing check-cfg array');
  let array = key[1];
  for (const cfg of CHECK_CFGS) {
    const quoted = `'${cfg}'`;
    if (!array.includes(quoted) && !array.includes(`"${cfg}"`)) array += `${array.trim() ? ', ' : ''}${quoted}`;
  }
  return cargo.slice(0, lineStart) + line.replace(key[0], `check-cfg = [${array}]`) + cargo.slice(end);
}

export function mergeHarnessCargo(targetCargo, sourceCargo) {
  let result = checkCfgMerge(targetCargo);
  for (const [key, features] of Object.entries(REQUIRED_TARGETS)) {
    const split = key.indexOf(':');
    const kind = key.slice(0, split), name = key.slice(split + 1);
    const blocks = namedBlocks(result, kind).filter(block => block.name === name);
    if (blocks.length > 1) throw new Error(`duplicate Cargo target block ${key}`);
    if (blocks.length) {
      const actual = /^\s*required-features\s*=\s*\[([^\]]*)\]/m.exec(blocks[0].text)?.[1]
        ?.split(',').map(item => item.trim().replace(/^['"]|['"]$/g, '')).filter(Boolean);
      if (!actual || JSON.stringify(actual) !== JSON.stringify(features))
        throw new Error(`Cargo target ${key} has incorrect required-features; expected ${JSON.stringify(features)}`);
      if (kind === 'bench' && !/^\s*harness\s*=\s*false\s*$/m.test(blocks[0].text))
        throw new Error(`Cargo target ${key} has incorrect harness; expected false`);
      continue;
    }
    const source = namedBlocks(sourceCargo, kind).find(block => block.name === name);
    if (!source) throw new Error(`source Cargo.toml missing target block ${key}`);
    result = `${result.replace(/\s*$/, '')}\n\n${source.text.trimEnd()}\n`;
  }
  return result;
}

export function applyHarnessOverlay({ sourceRoot, targetRoot, frozenFiles }) {
  const allowed = new Set(['Cargo.toml', 'examples/r20_prf01_c3_oracle.rs',
    'benches/r20_prf01_c3_cells_iai.rs', 'benches/r20_prf01_trim_iai.rs',
    'benches/r20_prf01_controls_iai.rs', 'benches/support/r20_prf01_controls.rs',
    'benches/support/r19_prf01.rs', 'benches/perf_gate_iai.rs']);
  const planned = [...allowed].filter(file => frozenFiles.includes(file) && fs.existsSync(path.join(sourceRoot, file)));
  if (planned.includes('Cargo.toml')) {
    const source = fs.readFileSync(path.join(sourceRoot, 'Cargo.toml'), 'utf8');
    const target = path.join(targetRoot, 'Cargo.toml');
    const current = fs.readFileSync(target, 'utf8');
    const merged = mergeHarnessCargo(current, source);
    if (merged !== current) fs.writeFileSync(target, merged);
  }
  const immutable = new Set(['benches/support/r19_prf01.rs', 'benches/perf_gate_iai.rs']);
  for (const file of planned.filter(file => file !== 'Cargo.toml')) {
    const source = path.join(sourceRoot, file), target = path.join(targetRoot, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    const bytes = fs.readFileSync(source);
    if (fs.existsSync(target)) {
      const existing = fs.readFileSync(target);
      if (immutable.has(file) && !existing.equals(bytes)) throw new Error(`immutable frozen overlay differs: ${file}`);
      if (!existing.equals(bytes)) fs.writeFileSync(target, bytes);
    } else fs.writeFileSync(target, bytes);
  }
  return planned;
}

export function validateC3OracleCounters({ oracle, arm, rehearsal, needed, required, candidateOnly }) {
  for (const key of required) {
    if (!Object.hasOwn(oracle, key)) throw new Error(`missing required oracle key ${key}`);
    const value = oracle[key], reason = oracle.unobservedReasons?.[key];
    const candidate = candidateOnly.includes(key);
    const expectedObserved = candidate && arm === 'B' && !rehearsal;
    if (candidate && !expectedObserved) {
      if (value !== null || typeof reason !== 'string' || !reason.length) throw new Error(`expected unobserved ${key} with reason`);
    } else if (value === null) {
      if (needed || typeof reason !== 'string' || !reason.length) throw new Error(`unexpected null ${key}`);
    } else if (!Number.isSafeInteger(value) || value < 0) {
      throw new Error(`invalid required ${key}`);
    } else if (reason !== undefined) {
      throw new Error(`reason attached to observed ${key}`);
    }
  }
  return true;
}

export function selfTestC3OracleCounters() {
  const required = ['pending_words_visited', 'summary_words_visited', 'summary_hint_loads', 'summary_hint_rmw', 'discovery_records_reclaimed', 'quiescent_summary_missing_bits', 'sizeof_small_sidecar'];
  const candidateOnly = ['summary_words_visited', 'summary_hint_loads', 'summary_hint_rmw', 'quiescent_summary_missing_bits'];
  const mk = () => ({ pending_words_visited: 12, summary_words_visited: null, summary_hint_loads: null, summary_hint_rmw: null, discovery_records_reclaimed: 3, quiescent_summary_missing_bits: null, sizeof_small_sidecar: 64, unobservedReasons: Object.fromEntries(candidateOnly.map(k => [k, 'not observed'])) });
  validateC3OracleCounters({ oracle: mk(), arm: 'A', rehearsal: false, needed: true, required, candidateOnly });
  for (const mutate of [o => { o.pending_words_visited = null; }, o => { o.sizeof_small_sidecar = null; }, o => { o.unobservedReasons.summary_hint_loads = ''; }]) {
    const oracle = mk(); mutate(oracle);
    let rejected = false;
    try { validateC3OracleCounters({ oracle, arm: 'A', rehearsal: false, needed: true, required, candidateOnly }); } catch { rejected = true; }
    if (!rejected) throw new Error('invalid oracle counter record accepted');
  }
  const officialB = mk(); officialB.summary_words_visited = null;
  let rejected = false;
  try { validateC3OracleCounters({ oracle: officialB, arm: 'B', rehearsal: false, needed: true, required, candidateOnly }); } catch { rejected = true; }
  if (!rejected) throw new Error('official B candidate counter null accepted');

  const observedB = mk();
  for (const key of candidateOnly) {
    observedB[key] = 1;
    delete observedB.unobservedReasons[key];
  }
  validateC3OracleCounters({ oracle: observedB, arm: 'B', rehearsal: false, needed: true, required, candidateOnly });

  const rehearsalB = mk();
  validateC3OracleCounters({ oracle: rehearsalB, arm: 'B', rehearsal: true, needed: true, required, candidateOnly });

  const unavailableMimallocB = mk();
  for (const key of required) unavailableMimallocB[key] = null;
  unavailableMimallocB.unobservedReasons = Object.fromEntries(required.map(key => [key, 'not available']));
  validateC3OracleCounters({ oracle: unavailableMimallocB, arm: 'B', rehearsal: false, needed: false, required, candidateOnly });
  return true;
}
export function selfTestMergeHarnessCargo() {
  const cfgs = CHECK_CFGS.map(cfg => `'${cfg}'`).join(', ');
  const targets = Object.entries(REQUIRED_TARGETS).map(([key, features]) => {
    const [kind, name] = key.split(':');
    return `[[${kind}]]\nname = "${name}"\n${kind === 'bench' ? 'harness = false\n' : ''}required-features = [${features.map(value => `"${value}"`).join(', ')}]\n`;
  }).join('');
  const source = `[workspace.lints.rust]\nunexpected_cfgs = { level = "warn", check-cfg = [${cfgs}] }\n${targets}`;
  const candidate = `[workspace.dependencies]\ncandidate_marker = "preserve-me"\n[workspace.lints.rust]\nunexpected_cfgs = { level = "warn", check-cfg = ['cfg(existing)'] }\n`;
  const merged = mergeHarnessCargo(candidate, source);
  if (!merged.includes('candidate_marker = "preserve-me"')) throw new Error('candidate dependency removed');
  if (!merged.includes("'cfg(existing)'")) throw new Error('existing cfg removed');
  if (CHECK_CFGS.some(cfg => merged.split(`'${cfg}'`).length !== 2)) throw new Error('cfg was not added exactly once');
  if (mergeHarnessCargo(merged, source) !== merged) throw new Error('merge is not idempotent');
  return true;
}

if (process.argv.includes('--selftest') && process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  selfTestC3OracleCounters();
  selfTestMergeHarnessCargo();
  selfTestMeasureToInput();
  const cargo = fs.readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8');
  mergeHarnessCargo(cargo, cargo);
  console.log('C3 runner support selftest PASS: counter contract, Cargo merge, measure input');
}

export function selfTestMeasureToInput() {
  const C3 = C3Support;
  const chosen = [C3.V2_BENEFIT[0], C3.V2_BENEFIT[1], C3.HOT_CELLS[0], C3.MIMALLOC[0], 'control-cell'];
  const mkCtx = rehearsal => {
    const records = {};
    for (const label of ['A1', 'B1', 'C0']) {
      records[label] = {};
      for (const name of chosen) {
        const cfg = name === C3.MIMALLOC[0] ? null : { requested: { segmentCount: 4 }, resolved: { segmentCount: 4 }, runId: `${label}-${name}`, pid: 7, configConflictsDelta: 0 };
        const values = Object.fromEntries(C3.ALL_COUNTERS.map((key, i) => [key, i + 3]));
        if (name === C3.MIMALLOC[0]) for (const key of C3.ALL_COUNTERS) values[key] = null;
        if (name === C3.V2_BENEFIT[1] && label === 'A1') { for (const key of C3.ALL_COUNTERS) values[key] = null; values.pending_words_visited = 21; values.sizeof_small_sidecar = 11; }
        if (name === C3.V2_BENEFIT[0] && ['A1', 'B1'].includes(label)) values.sizeof_small_sidecar = 6;
        if (name === C3.V2_BENEFIT[1] && label === 'B1' && rehearsal) { for (const key of C3.C3_ORACLE_COUNTERS.filter(key => key !== 'sizeof_small_sidecar')) values[key] = null; }
        if (name === 'control-cell') for (const key of C3.ALL_COUNTERS) values[key] = null;
        const reasons = Object.fromEntries(Object.entries(values).filter(([, value]) => value === null).map(([key]) => [key, `reason:${key}`]));
        const cfgList = label === 'B1' && !rehearsal ? ['r19_sidecar_summary_oracle', 'r20_prf01_c3_oracle'] : ['r19_sidecar_summary_oracle'];
        records[label][name] = { binarySha256: 'b'.repeat(64), source: { tree: label }, row: { oracle: { ...values, unobservedReasons: reasons }, config: cfg, cfg: cfgList, pid: 7, startedAt: 'now' } };
      }
    }
    return { rehearsal, builds: { environmentSha256: 'e'.repeat(64), buildStartedAt: 'now', arms: { A: { iai: { binarySha256: 'a'.repeat(64) } }, B: { iai: { binarySha256: 'b'.repeat(64) } }, C: Object.fromEntries(['iai', 'controls', 'c3', 'trim', 'c3Control', 'control', 'oracle'].map(key => [key, { binarySha256: 'c'.repeat(64) }])) } }, manifest: { arches: { A: { head: 'a'.repeat(40) }, B: { head: 'b'.repeat(40) } } }, oracle: { records } };
  };
  const assert = (condition, message) => { if (!condition) throw new Error(`measureToInput selftest: ${message}`); };
  const execute = rehearsal => {
    const ctx = mkCtx(rehearsal), stored = {};
    const runs = ['A1', 'B1', 'C0'].map(label => ({ label, arm: label[0] === 'C' ? 'C' : label[0], cells: [{ key: 'iai', cells: chosen, ir: Object.fromEntries(chosen.map(name => [name, 100])), pids: Object.fromEntries(chosen.map(name => [name, 7])), startedAt: 'now', log: { path: 'logs/run.log' } }] }));
    const result = measureToInput(runs, [], { ctx, C3, shaFile: () => 'd'.repeat(64), REPO: 'repo', sourceOf: value => ({ ...value }), ORACLE_CFG: 'cfg(r19_sidecar_summary_oracle)', SUMMARY_ORACLE_CFG: 'cfg(r20_prf01_c3_oracle)', PRODUCER_SPECS: {}, local: value => value, putJson: (name, value) => { stored[name] = value; }, putText: () => {}, assert: { ok: (value, msg) => assert(value, msg), equal: (a, b, msg) => assert(a === b, msg) }, fs: { readFileSync: () => 'iai-callgrind-runner 0.14.2' }, join: (...parts) => parts.join('/') });
    return { result, stored };
  };
  const official = execute(false).result;
  assert(official.iai.length === 15, 'selected rows missing');
  for (const name of [C3.V2_BENEFIT[0], C3.V2_BENEFIT[1]]) {
    const row = official.iai.find(item => item.name === name && item.arm === 'A1');
    assert(row, `${name} generated row absent`);
  }
  const pendingA = official.iai.find(item => item.name === C3.V2_BENEFIT[1] && item.arm === 'A1');
  assert(pendingA.oracle.pending_words_visited === 21 && pendingA.oracle.sizeof_small_sidecar === 11, 'A raw counters were not retained');
  assert(official.iai.find(item => item.name === C3.V2_BENEFIT[1] && item.arm === 'B1').oracle.linkage.cfg.includes('r20_prf01_c3_oracle'), 'official B cfg missing candidate cfg');
  assert(official.identities.C0.oracleBinarySha256 === 'c'.repeat(64), 'C0 oracle binary pin missing');
  const hot = official.iai.find(item => item.name === C3.HOT_CELLS[0] && item.arm === 'C0');
  assert(C3.ALL_COUNTERS.every(key => hot.oracle[key] === null) && hot.oracle.linkage === null, 'hot row should carry null oracle and linkage');
  assert(hot.oracle.linkage === null && official.identities.C0.oracleBinarySha256 === 'c'.repeat(64), 'hot row should carry null oracle and candidate oracle binary pin');
  const mimalloc = official.iai.find(item => item.name === C3.MIMALLOC[0] && item.arm === 'B1');
  assert(C3.ALL_COUNTERS.every(key => mimalloc.oracle[key] === null) && mimalloc.config.notApplicableReason, 'mimalloc must remain unobserved');
  assert(pendingA.oracle.unobservedReasons.summary_words_visited === 'reason:summary_words_visited', 'raw null reason lost');
  assert(official.sizeOf.A === 6 && official.sizeOf.B === 6, 'observed sizes absent');
  const { result: rehearsal } = execute(true);
  assert(rehearsal.iai.find(item => item.name === C3.V2_BENEFIT[1] && item.arm === 'B1').oracle.linkage.cfg.length === 1, 'rehearsal B cfg should contain only r19 oracle cfg');
  assert(rehearsal.iai.find(item => item.name === C3.V2_BENEFIT[1] && item.arm === 'B1').oracle.linkage.cfg[0] === 'r19_sidecar_summary_oracle', 'rehearsal B cfg mismatch');
  return true;
}

export function measureToInput(runs, producerRuns, {
  ctx, C3, shaFile, REPO, sourceOf, ORACLE_CFG, SUMMARY_ORACLE_CFG, PRODUCER_SPECS, local, putJson, putText,
  assert, fs, join,
}) {
  const hashes = C3.FROZEN_FILES.map(path => ({
    path,
    sha256: shaFile(join(REPO, path)),
  }));
  const environmentSha256 = ctx.builds.environmentSha256;
  const identities = {};

  for (const arm of ['A', 'B']) {
    identities[arm] = {
      label: arm,
      measurementCommit: ctx.manifest.arches[arm].head,
      source: sourceOf(ctx.manifest.arches[arm]),
      environmentSha256,
      binaries: Object.fromEntries(
        Object.entries(ctx.builds.arms[arm]).map(([key, value]) => [
          key,
          value.binarySha256,
        ]),
      ),
      buildStartedAt: ctx.builds.buildStartedAt,
      buildReceipt: 'builds.json',
      features: C3.FEATURES,
      primaryCfg: [],
      oracleCfg: arm === 'B' && !ctx.rehearsal ? [ORACLE_CFG, SUMMARY_ORACLE_CFG] : [ORACLE_CFG],
      toolchain: {
        rustc: 'environment.log',
        target: 'environment.log',
        profile: 'release',
        flags: 'role cfg only',
      },
    };
  }

  identities.C0 = {
    source: identities.A.source,
    binarySha256: ctx.builds.arms.C.iai.binarySha256,
    controlsBinarySha256: ctx.builds.arms.C.controls.binarySha256,
    c3BinarySha256: ctx.builds.arms.C.c3.binarySha256,
    oracleBinarySha256: ctx.builds.arms.C.oracle.binarySha256,
    trimBinarySha256: ctx.builds.arms.C.trim.binarySha256,
    c3ControlBinarySha256: ctx.builds.arms.C.c3Control.binarySha256,
    controlBinarySha256: ctx.builds.arms.C.control.binarySha256,
    buildStartedAt: ctx.builds.buildStartedAt,
    buildReceipt: 'builds.json',
  };
  for (const [key, value] of Object.entries({
    iai: ctx.builds.arms.C.iai.binarySha256,
    controls: ctx.builds.arms.C.controls.binarySha256,
    c3: ctx.builds.arms.C.c3.binarySha256,
    trim: ctx.builds.arms.C.trim.binarySha256,
    c3Control: ctx.builds.arms.C.c3Control.binarySha256,
    control: ctx.builds.arms.C.control.binarySha256,
  })) {
    identities.C0[`${key}BinarySha256`] = value;
  }
  identities.C0.source = sourceOf(ctx.manifest.arches.A);

  const runnerVersionMatch = /(\d+[.]\d+[.]\d+)/.exec(
    fs.readFileSync(local('environment.log'), 'utf8')
      .split('\n')
      .find(line => line.includes('iai-callgrind-runner')) ?? '',
  );
  assert.ok(runnerVersionMatch, 'runner version absent from environment');
  const runnerVersion = runnerVersionMatch[1];
  const input = {
    version: 1,
    hashes,
    environmentSha256,
    ...(ctx.rehearsal ? { rehearsal: true } : {}),
    ...(ctx.rehearsal ? { rehearsal: true } : {}),
    inventoryReceipt: 'manifest.json',
    identityReviewReceipt: 'manifest.json; per-stage identity revalidation',
    environmentReceipt: 'environment.log',
    oracleReceipt: 'oracle.json',
    entry: 'SeferAlloc::GlobalAlloc->HeapCore routed',
    nativeDeferred: {
      deferred: true,
      method: 'fixed-batch native paired A/B on a quiet bare-metal host; '
        + 'per-process batch times divided by batch size (deferred; not launched)',
    },
    inventory: [...C3.INVENTORY],
    identities,
    iai: [],
    sizeOf: { A: null, B: null, receipt: 'oracle.json' },
    ancestry: {
      result: 'A ancestor B',
      chain: [ctx.manifest.arches.A.head, ctx.manifest.arches.B.head],
      receipt: 'manifest.json',
    },
  };
  let runId = 0;

  for (const run of runs) {
    const source = sourceOf(ctx.manifest.arches[run.arm]);
    for (const batch of run.cells) {
      for (const name of batch.cells) {
        const oracleRec = ctx.oracle.records[run.label]?.[name];
        const rawSource = oracleRec?.row?.oracle ?? {};
        const needed = C3.V2_BENEFIT.includes(name)
          || name === C3.PUBLICATION
          || Object.hasOwn(C3.PRODUCERS, name)
          || C3.TRIM_CELLS.includes(name);
        const raw = Object.fromEntries(
          [...C3.ALL_COUNTERS, 'unique_pending_words'].map(key => [
            key,
            needed && Object.hasOwn(rawSource, key) ? rawSource[key] : null,
          ]),
        );
        const binarySha256 = ctx.builds.arms[run.arm][batch.key].binarySha256;

        const actualOraclePid = oracleRec?.row?.pid ?? oracleRec?.row?.config?.pid;
        const config = oracleRec?.row?.config ?? {
          requested: null,
          resolved: null,
          configConflictsDelta: null,
          processBoundary: 'isolated-process',
          runId: `${run.label}-${name}-${runId}`,
          pid: batch.pids[name],
          notApplicableReason: 'control cell has no SeferAlloc config oracle',
        };
        const hot = C3.HOT_CELLS.includes(name) || name === C3.CONSTRUCTION;
        const hotConfig = hot ? {
          requested: { ...(config.requested ?? {}), segmentCount: null },
          resolved: { ...(config.resolved ?? {}), segmentCount: null },
          unobservedReasons: {
            ...(config.unobservedReasons ?? {}),
            segmentCount: 'hot geometry declaration per frozen bench',
          },
          declaredProvenance: {
            receipt: 'manifest.json',
            cell: name,
            fields: ['workCount', 'blockBytes', 'alignment'],
            alignment: name.includes('a128') ? 128 : 8,
          },
        } : {};
        input.iai.push({
          name,
          arm: run.label,
          Ir: batch.ir[name],
          environmentSha256,
          binarySha256,
          source,
          startedAt: batch.startedAt,
          features: C3.FEATURES,
          receipt: batch.log.path,
          config: {
            ...config,
            ...hotConfig,
            ...(hot ? { resolvedProvenance: {
              arm: run.label.startsWith('B') ? 'B' : 'A',
              runId: oracleRec?.row?.config?.runId,
              pid: actualOraclePid,
              binarySha256: oracleRec?.binarySha256,
              receipt: `oracle/${run.label}-${name}.json`,
              claimPath: 'global',
              copiedFields: ['cachePolicy', 'configConflictsDelta', 'maintenance'],
            } } : {}),
            pid: batch.pids[name],
            runId: `${run.label}-${name}-${++runId}`,
          },
          oracle: {
            ...raw,
            unobservedReasons: Object.fromEntries(
              Object.keys(raw)
                .filter(key => raw[key] === null)
                .map(key => [key, rawSource.unobservedReasons?.[key]
                  ?? `not observed for ${run.label}/${name}`]),
            ),
            linkage: needed && oracleRec ? {
              binarySha256: oracleRec.binarySha256,
              primaryBinarySha256: binarySha256,
              source: oracleRec.source,
              features: C3.FEATURES,
              cfg: oracleRec.row.cfg,
              environmentSha256,
              receipt: `oracle/${run.label}-${name}.json`,
              featureExactReceipt: 'oracle.json',
              startedAt: oracleRec.row.startedAt,
              config: oracleRec.row.config,
            } : null,
          },
        });
      }
    }
  }

  for (const producer of producerRuns) {
    const spec = PRODUCER_SPECS[producer.name];
    const row = input.iai.find(
      item => item.name === producer.name && item.arm === producer.label,
    );
    assert.ok(row);
    const primary = producer.primary;
    const control = producer.control;
    assert.equal(primary.totalIr, row.Ir, `producer rerun Ir mismatch @${producer.label}`);
    const workload = primary.childIr - control.childIr;
    assert.ok(workload > 0, `empty producer workload @${producer.label}`);
    row.producerIrReceipt = {
      binarySha256: row.binarySha256,
      source: row.source,
      environmentSha256,
      features: C3.FEATURES,
      cfg: [],
      receipt: primary.log.path,
      symbolReceipt: `boundary/${producer.label}-${producer.name}-primary-nm.log`,
      profileReceipt: `profiles/${producer.label}-${producer.name}-primary-inventory.json`,
      symbol: spec.symbol,
      runnerVersion,
      valgrindVersion: 'environment.log',
      collectAtStart: false,
      defaultToggle: false,
      entryPoint: 'Custom',
      separateThreads: true,
      toggleCollect: spec.symbol,
      generatedArguments: primary.generatedArguments,
      aggregation: 'sum of all thread profile Ir',
      childFreeCount: spec.freeCount,
      childIr: primary.childIr,
      boundaryOverheadIr: control.childIr,
      workloadIr: workload,
      boundaryExecutions: 1,
      totalIr: row.Ir,
      startedAt: primary.startedAt,
      absoluteTolerance: producer.name === C3.LARGE_REMOTE_FREE ? 'T_s' : null,
      control: {
        receipt: control.log.path,
        workload: 'child remote frees removed',
        source: row.source,
        binarySha256: control.binarySha256,
        environmentSha256,
        childIr: control.childIr,
        workloadIr: 0,
        childFreeCount: 0,
        boundaryExecutions: 1,
        symbolReceipt: `boundary/${producer.label}-${producer.name}-control-nm.log`,
        profileReceipt: `profiles/${producer.label}-${producer.name}-control-inventory.json`,
        symbol: spec.symbol,
        toggleCollect: spec.symbol,
        cfg: [spec.cfg],
        features: C3.FEATURES,
        generatedArguments: control.generatedArguments,
        runnerVersion,
        valgrindVersion: 'environment.log',
        collectAtStart: false,
        defaultToggle: false,
        entryPoint: 'Custom',
        separateThreads: true,
        aggregation: 'sum of all thread profile Ir',
        totalIr: control.totalIr,
        deltaIr: primary.totalIr - control.totalIr,
        parentSetupUnchanged: true,
        collectionUnchanged: true,
      },
    };
  }

  const observedSize = arm => {
    const name = C3.V2_BENEFIT[0];
    const row = ctx.oracle.records[arm]?.[name]?.row;
    const value = row?.oracle?.sizeof_small_sidecar ?? row?.sizeof_small_sidecar;
    assert.ok(Number.isSafeInteger(value) && value > 0, `oracle must observe positive sidecar size for ${arm}`);
    return value;
  };
  input.sizeOf = { A: observedSize('A1'), B: observedSize('B1'), receipt: 'oracle.json' };
  const receipts = {};
  const scan = value => {
    if (typeof value === 'string'
      && /^(?:environment[.]log|manifest[.]json|builds[.]json|oracle[.]json|(?:logs|boundary|profiles|oracle)\/)/
        .test(value)
      && !(value in receipts)) {
      receipts[value] = shaFile(local(value));
    } else if (Array.isArray(value)) {
      value.forEach(scan);
    } else if (value && typeof value === 'object') {
      Object.values(value).forEach(scan);
    }
  };
  scan(input);
  input.receiptHashes = receipts;
  if (ctx.rehearsal) input.rehearsal = true;
  putJson('input.json', input);
  putText('summary.csv', [
    ...(ctx.rehearsal ? ['# rehearsal: harness validation only; never gate data'] : []),
    'target,arm,name,ir,binarySha256',
    ...input.iai.map(row => `${C3.targetFor(row.name)},${row.arm},${row.name},${row.Ir},${row.binarySha256}`),
  ].join('\n') + '\n');
  return input;
}
