import { intervalContainsOne, pairedLogRatioCI } from './j13b_stats.mjs';

const M = 1048576, GRANULE = 2097152;
const ORDER = ['A', 'B', 'B', 'A'];
const CELLS = {
  'cycle-1MiB': ['cycle', M, 1, 1, ['cycle', 'sefer', '1', '1']],
  'cycle-8MiB-minus-1': ['cycle', 8*M-1, 1, 1, ['cycle', 'sefer', '8', '1']],
  'cycle-8MiB': ['cycle', 8*M, 1, 1, ['cycle', 'sefer', '8', '1']],
  'cycle-16MiB': ['cycle', 16*M, 1, 1, ['cycle', 'sefer', '16', '1']],
  'cycle-128MiB': ['cycle', 128*M, 1, 1, ['cycle', 'sefer', '128', '1']],
  'cycle-1024MiB': ['cycle', 1024*M, 1, 1, ['cycle', 'sefer', '1024', '1']],
  'grow-1GiB': ['grow', 1024*M, 1, 1, ['grow', 'sefer', '1024', '1']],
  'many-128x16MiB': ['many', 16*M, 1, 128, ['many', 'sefer', '16', '1', '128']],
  'edge-128MiB': ['edge', 128*M, 1, 1, ['edge', 'sefer', '128', '1']],
  'cache-16MiB': ['cycle', 16*M, 2, 1, ['cycle', 'sefer', '16', '2', '1']],
};
const FEATURE_SET = new Set(['production', 'alloc-stats', 'internals', 'bench-internals']);
const GRADIENT = 'LargeThpGradient { down_shift: 2, up_shift: 5, mean_shift: 5, anchor_raw: 0, floor_raw: 214748365, gain_num: 5, gain_den: 4, curve: Linear }';
const plain = x => x !== null && typeof x === 'object' && !Array.isArray(x);
const need = (ok, message) => { if (!ok) throw Error(message); };
const pos = x => Number.isSafeInteger(x) && x > 0;
const nonneg = x => Number.isSafeInteger(x) && x >= 0;
const text = x => typeof x === 'string' && !!x.trim() && !/^(unknown|none|unset|placeholder|unavailable|n\/a|tbd)$/i.test(x.trim());
const sha = x => typeof x === 'string' && /^[a-f\d]{64}$/i.test(x);
const revision = x => typeof x === 'string' && /^(?:[a-f\d]{40}|[a-f\d]{64})$/i.test(x);
const stable = x => Array.isArray(x) ? `[${x.map(stable)}]` : plain(x)
  ? `{${Object.keys(x).sort().map(k => `${JSON.stringify(k)}:${stable(x[k])}`).join(',')}}` : JSON.stringify(x);
const same = (a, b) => stable(a) === stable(b);
const iso = x => typeof x === 'string' && /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/.test(x)
  && Number.isFinite(Date.parse(x)) && new Date(Date.parse(x)).toISOString() === x;
const median = xs => {
  const a = [...xs].sort((x, y) => x - y), mid = a.length >> 1;
  return a.length % 2 ? a[mid] : (a[mid-1] + a[mid]) / 2;
};
const csvField = x => {
  const s = x == null ? '' : String(x);
  return /[",\r\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
};

function validateReceipt(receipt, source) {
  need(plain(receipt) && receipt.schema === 'g1-big-study-ci-build-receipt.v1'
    && plain(receipt.build_context) && plain(receipt.build), 'trusted CI build receipt schema required');
  need(plain(source), 'observer source_identity is required');
  const c = receipt.build_context, b = receipt.build;
  need(revision(c.commit) && revision(c.tree) && c.dirty === false && c.status_porcelain === '', 'clean receipt source required');
  need(source.commit === c.commit && source.tree === c.tree && source.dirty === false, 'observer checkout does not match clean build source');
  need(text(c.rustc_version_verbose) && text(c.cargo_version) && text(c.rustc_host_target), 'receipt compiler/host facts unavailable');
  for (const key of ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS']) {
    const f = c[key];
    need(plain(f) && typeof f.set === 'boolean' && (f.set ? typeof f.value === 'string' : f.value === null), `receipt ${key} set/value unavailable`);
  }
  const target = c.rustc_host_target;
  const command = ['cargo','build','--locked','--release','--target',target,'--features',
    'production alloc-stats internals bench-internals','--example','g1_big_judge','--message-format=json-render-diagnostics'];
  need(same(b.command, command) && receipt.host_target === target, 'receipt build argv/target mismatch');
  need(plain(c.target_selection) && c.target_selection.explicit === true && c.target_selection.target === target,
    'receipt explicit target selection required');
  need(sha(b.sha256) && sha(receipt.copied_artifact?.sha256) && b.sha256 === receipt.copied_artifact.sha256,
    'receipt copied/artifact hash mismatch');
  need(b.artifact_executable === b.executable_path && text(b.executable_path), 'receipt compiler artifact executable mismatch');
  need(plain(b.artifact_target) && b.artifact_target.name === 'g1_big_judge' && same(b.artifact_target.kind, ['example']),
    'receipt example target mismatch');
  need(Array.isArray(b.artifact_features) && b.artifact_features.every(x => typeof x === 'string')
    && [...FEATURE_SET].every(x => b.artifact_features.includes(x)) && !b.artifact_features.includes('numa-aware'),
    'resolved artifact features do not satisfy study');
  need(plain(receipt.copied_artifact) && text(receipt.copied_artifact.path), 'copied artifact path required');
  return receipt;
}

function validateHost(h, firstLaunch) {
  need(plain(h) && h.scope === 'measurement_host' && h.os === 'linux' && h.arch === 'x86_64' && text(h.cpu_info),
    'measurement-host scope/identity required');
  need(h.base_page_bytes === 4096 && h.thp_page_bytes === GRANULE && h.thp_mode === 'madvise',
    'measurement-host page/THP facts mismatch');
  need(iso(h.captured_before) && Date.parse(h.captured_before) <= firstLaunch && pos(h.memory_available_bytes)
    && h.memory_available_bytes >= 8*1024*M, 'measurement-host preflight timing/memory unavailable');
  need(h.active_swap === false && h.active_pressure === false, 'measurement-host swap/pressure admission failed');
}

function validateProducer(row, cell, slot, role, receipt) {
  const [scenario, bytes, reps, k, args] = CELLS[cell], cache = cell === 'cache-16MiB';
  const p = row.parsed_row, l = row.launch, always = role === 'AB' && slot === 'B';
  need(plain(p) && plain(l) && typeof l.raw_stdout === 'string' && typeof l.raw_stderr === 'string'
    && same(JSON.parse(l.raw_stdout), p), `${role} ${slot}: complete producer/raw stdout mismatch`);
  for (const key of Object.keys(p)) if (!['arm','block','slot','launch','wall_clock_iso'].includes(key))
    need(same(row[key], p[key]), `${role} ${slot}: root producer spread differs at ${key}`);
  need(p.study === true && p.diagnostics === 'native-advice-capture' && p.arm === 'sefer'
    && p.scenario === scenario && p.requested_bytes === bytes && p.reps === reps && p.k === k,
    `${role} ${slot}: producer workload mismatch`);
  const mode = always ? 'Always' : 'Off';
  need(p.mode_requested === mode && p.mode_resolved === mode && p.thp_min_requested_bytes === 8388608
    && p.thp_min_resolved_bytes === 8388608 && p.thp_budget_requested_bytes === 0
    && p.thp_budget_resolved_bytes === 0 && p.gradient_requested === 'CAUTIOUS'
    && p.gradient_resolved === GRADIENT, `${role} ${slot}: resolved THP tuple mismatch`);
  need(p.cache_requested === (cache ? 'on' : 'off') && p.cache_resolved_budget_bytes === (cache ? 33554432 : 0)
    && p.config_conflicts === 0 && p.thp_tuple_conflicts === 0, `${role} ${slot}: cache/config conflict mismatch`);
  const env = { G1_BIG_THP: always ? 'always' : 'off', G1_BIG_CACHE: cache ? 'on' : 'off' };
  if (cell === 'cycle-8MiB-minus-1' || l.env_overrides?.G1_BIG_BYTES != null) env.G1_BIG_BYTES = String(bytes);
  need(same(l.env_overrides, env) && (same(l.args, args)
    || scenario !== 'many' && same(l.args, [...args, '1'])), `${role} ${slot}: actual launch args/environment mismatch`);
  need(pos(l.child_wall_ns) && l.child_exit_code === 0 && l.signal === null && l.error === null && l.timed_out === false,
    `${role} ${slot}: unsuccessful child launch`);
  need(sha(l.binary_sha256) && l.binary_sha256 === receipt.build.sha256 && l.binary_sha256_after === l.binary_sha256,
    `${role} ${slot}: launch hash differs from trusted receipt`);
  need(text(l.command) && text(l.executable_path), `${role} ${slot}: command/executable path unavailable`);

  const fresh = p.fresh_requested_bytes;
  need(Array.isArray(fresh) && p.fresh_mappings === fresh.length && fresh.every(pos), `${role} ${slot}: fresh mapping evidence malformed`);
  const grow = [M];
  while (grow.at(-1) < bytes) grow.push(Math.min(bytes, grow.at(-1) * 2));
  const valid = scenario === 'grow'
    ? fresh.length > 0 && fresh.length <= grow.length
      && fresh.every((v, i) => grow.indexOf(v) > (i ? grow.indexOf(fresh[i-1]) : -1))
    : fresh.length === (scenario === 'many' ? k : 1) && fresh.every(v => v === bytes);
  need(valid, `${role} ${slot}: fresh mapping count/order/size mismatch`);
  const eligible = fresh.filter(v => v >= 8388608), advice = p.accepted_advice;
  need(Array.isArray(advice) && p.accepted_advice_count === advice.length
    && advice.length === (always ? eligible.length : 0), `${role} ${slot}: advice count/mode mismatch`);
  need(advice.every((a, i) => plain(a) && a.requested_bytes === eligible[i] && pos(a.usable_span_bytes)
    && pos(a.accepted_bytes) && a.accepted_bytes === Math.floor(a.usable_span_bytes/GRANULE)*GRANULE),
    `${role} ${slot}: accepted advice prefix mismatch`);
  need(p.advice_granule_bytes === GRANULE, `${role} ${slot}: advice granule mismatch`);
  need(p.cache_hit_confirmed === cache && p.large_cache_hits === (cache ? 1 : 0), `${role} ${slot}: actual cache hit evidence mismatch`);
  for (const key of ['capture_slot_storage_bytes','capture_total_storage_bytes','capture_tls_context_bytes_per_thread'])
    need(nonneg(p[key]), `${role} ${slot}: capture storage ${key} unavailable`);
  need(plain(p.os), `${role} ${slot}: nested OS observations required`);
  for (const key of ['faults','peak_rss','end_rss','anon_huge_peak'])
    need(p.os[key] === null || nonneg(p.os[key]), `${role} ${slot}: invalid nested OS ${key}`);
  need(pos(p.cold_ns) && row.cold_ns === p.cold_ns, `${role} ${slot}: runner cold metric differs from producer`);
  if (cache) need(pos(p.cache_ns), `${role} ${slot}: cache metric unavailable`);
}

function validateDocument(doc, facts, role, receipt) {
  need(plain(facts) && !('source' in facts) && !('arms' in facts) && Object.hasOwn(facts, 'cell') && Object.hasOwn(facts, 'regime'),
    `${role}: minimal cell/regime facts required`);
  const oldRoots = ['git_commit','git_dirty','rustc_version_verbose','cpu_info','platform','cargo_features_built_with'];
  need(plain(doc) && !oldRoots.some(k => k in doc) && doc.result_format === 'json'
    && doc.metric === 'cold_ns' && doc.pairs_per_comparison === 12 && Array.isArray(doc.comparisons)
    && doc.comparisons.length === 1, `${role}: actual JSON runner root required`);
  const src = doc.source_identity, cell = facts.cell, cache = cell === 'cache-16MiB';
  need(plain(src) && src.commit === receipt.build_context.commit && src.tree === receipt.build_context.tree
    && src.dirty === false && iso(doc.timestamp), `${role}: runner observer identity/timestamp mismatch`);
  need(Object.hasOwn(CELLS, cell) && facts.regime === (cache ? 'CACHE' : 'COLD'), `${role}: unregistered cell/regime`);
  const cmp = doc.comparisons[0], armB = role === 'AB' ? 'B' : 'A';
  need(cmp.arm_a === 'A' && cmp.arm_b === armB && cmp.pairs === 12
    && Array.isArray(cmp.raw_process_launches) && cmp.raw_process_launches.length === 48,
    `${role}: exactly 12 A/B/B/A blocks required`);
  const by = { A: Array.from({length:12}, () => []), B: Array.from({length:12}, () => []) };
  let previous = -Infinity;
  cmp.raw_process_launches.forEach((row, i) => {
    const slot = ORDER[i%4], block = i >> 2, time = Date.parse(row?.wall_clock_iso);
    need(plain(row) && row.block === block && row.slot === slot && row.arm === (role === 'AB' ? slot : 'A'),
      `${role}: missing/extra/duplicate/stale block or slot ${i}`);
    need(iso(row.wall_clock_iso) && time >= previous, `${role}: invalid/out-of-order launch timestamp`);
    previous = time;
    validateProducer(row, facts.cell, slot, role, receipt);
    by[slot][block].push(row);
  });
  need(Date.parse(doc.timestamp) >= previous, `${role}: root timestamp precedes last launch`);
  const means = slot => by[slot].map(g => (g[0].cold_ns + g[1].cold_ns) / 2);
  const a = means('A'), b = means('B'), d = a.map((v, i) => v - b[i]);
  need(same(cmp.samples_a_ns, a) && same(cmp.samples_b_ns, b) && same(cmp.deltas_a_minus_b_ns, d), `${role}: runner aggregates disagree with raw cold_ns`);
  return { doc, facts, cmp, by };
}

function validatePair(ab, control, receipt) {
  need(ab.facts.cell === control.facts.cell && ab.facts.regime === control.facts.regime, 'AB/control cell or regime mismatch');
  const rows = [...ab.cmp.raw_process_launches, ...control.cmp.raw_process_launches];
  const first = rows[0].launch;
  need(rows.every(r => r.launch.command === first.command && same(r.launch.args, first.args)
    && r.launch.executable_path === first.executable_path),
    'AB/control command/path/args/build identity mismatch');
}

function selected(row, regime) { return regime === 'CACHE' ? row.parsed_row.cache_ns : row.cold_ns; }
function minima(parsed, slot) {
  return parsed.by[slot].map(group => Math.min(...group.map(row => selected(row, parsed.facts.regime))));
}
function summarize(parsed, slot, metric) {
  const fields = {
    selected: row => selected(row, parsed.facts.regime), cold_first_life: row => row.cold_ns,
    child: row => row.launch.child_wall_ns, faults: row => row.parsed_row.os.faults,
    peak_rss: row => row.parsed_row.os.peak_rss, end_rss: row => row.parsed_row.os.end_rss,
    anon_huge_peak: row => row.parsed_row.os.anon_huge_peak,
    capture_slot: row => row.parsed_row.capture_slot_storage_bytes, capture_total: row => row.parsed_row.capture_total_storage_bytes,
    capture_tls: row => row.parsed_row.capture_tls_context_bytes_per_thread,
  };
  const values = parsed.cmp.raw_process_launches.filter(row => row.slot === slot).map(fields[metric]);
  return values.some(value => value == null) ? null : median(values);
}

function render(rows, facts, host, receipt, evidence, estimator) {
  const columns = ['evidence_class','cell','regime','metric','source','field','unit','scope','numerator','denominator',
    'ratio','ci95_lo','ci95_hi','aa_payload_ratio','aa_payload_ci95_lo','aa_payload_ci95_hi','aa_readability_only',
    'AB_Off_median','AB_Always_median','control_A_median','control_B_median','estimator_metadata','facts_json',
    'measurement_host_json','build_receipt_json'];
  const csvRows = rows.map(r => [evidence,r.cell,r.regime,r.metric,r.mapping.source,r.mapping.field,r.mapping.unit,
    r.mapping.scope,'Always','Off',r.ratio?.ratio,r.ratio?.lo,r.ratio?.hi,r.aa?.ratio,r.aa?.lo,r.aa?.hi,r.claimNote,
    r.medians.AB_Off,r.medians.AB_Always,r.medians.control_A,r.medians.control_B,estimator,
    JSON.stringify(facts),JSON.stringify(host),JSON.stringify(receipt)].map(csvField).join(','));
  const csv = `${columns.join(',')}\n${csvRows.join('\n')}\n`;
  const table = rows.map(r => {
    const ci = r.ratio ? `${r.ratio.ratio.toFixed(4)} (${r.ratio.lo.toFixed(4)}–${r.ratio.hi.toFixed(4)}) Always/Off` : 'descriptive; no ratio';
    const aa = r.aa ? `${r.aa.ratio.toFixed(4)} (${r.aa.lo.toFixed(4)}–${r.aa.hi.toFixed(4)}); ${r.claimNote}` : r.claimNote;
    return `| ${r.cell} | ${r.regime} | ${r.metric} (${r.mapping.source}.${r.mapping.field}) | ${r.mapping.scope} | ${ci} | ${aa} | ${r.medians.AB_Off ?? 'unavailable'} → ${r.medians.AB_Always ?? 'unavailable'} |`;
  });
  const markdown = ['# J13 limited report',`Evidence: ${evidence}; no product GO/veto.`,`Estimator: ${estimator}`,
    'Build receipt/source and measurement-host observations are separate scopes. Receipt is trusted CI evidence, not cryptographic authenticity.',
    'Measurement-host preflight checks this narrow host contract only; it does not prove or enforce parent operational bounds.',
    'Runner root cold_ns remains the runner comparison metric. CACHE rows report first-life COLD separately and use producer cache_ns only for second-life CACHE statistics.',
    'In CACHE, OS and full-child fields span cold+cache+trim as one same-process observation, not per-life memory evidence; null OS values make that metric median unavailable, never substitute zero.',
    'RSS is approximate; faults and AnonHugePages corroborate activity only, not exact physical maxima or allocator/advice attribution.',
    'A/A is payload readability only, never speed evidence or memory-metric CI. No product threshold or GO/veto is applied.',
    'Materially new regressions require owner review; no default change is inferred.',
    '| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |',
    '|---|---|---|---|---:|---:|---:|',...table,'',`Minimal facts: ${JSON.stringify(facts)}`,
    `Measurement-host scope: ${JSON.stringify(host)}`,`Trusted build-receipt scope: ${JSON.stringify(receipt)}`].join('\n');
  return { csv, markdown };
}

export function j13LimitedReport(input) {
  need(plain(input) && ['measured','synthetic'].includes(input.evidence_class), 'explicit measured/synthetic evidence class required');
  const source = input.ab?.document?.source_identity, receipt = validateReceipt(input.build_receipt, source);
  const ab = validateDocument(input.ab.document, input.ab.facts, 'AB', receipt);
  const control = validateDocument(input.control.document, input.control.facts, 'control', receipt);
  validatePair(ab, control, receipt);
  const times = [...ab.cmp.raw_process_launches,...control.cmp.raw_process_launches].map(row => Date.parse(row.wall_clock_iso));
  validateHost(input.measurement_host, Math.min(...times));
  const off = minima(ab, 'A'), always = minima(ab, 'B'), ci = pairedLogRatioCI(always, off);
  const expected = Math.exp(always.reduce((s, v, i) => s + Math.log(v) - Math.log(off[i]), 0) / 12);
  need(Math.abs(ci.ratio - expected) < 1e-12, 'paired log-ratio arithmetic mismatch');
  const aa = pairedLogRatioCI(minima(control, 'B'), minima(control, 'A')), readable = intervalContainsOne(aa);
  const estimator = 'minimum of exactly 2 raw launches per slot/block; exp(mean of 12 paired log ratios); two-sided 95% paired t CI, df=11, t*=2.201';
  const definitions = {
    selected: [ab.facts.regime === 'CACHE' ? 'parsed_row' : 'runner_root',
      ab.facts.regime === 'CACHE' ? 'cache_ns' : 'cold_ns', 'ns', 'allocation+touch+free'],
    child: ['runner_launch', 'child_wall_ns', 'ns', 'full direct child'],
    faults: ['parsed_row.os', 'faults', 'count', 'process fault observation'],
    peak_rss: ['parsed_row.os', 'peak_rss', 'bytes', 'approximate process peak RSS'],
    end_rss: ['parsed_row.os', 'end_rss', 'bytes', 'process end RSS'],
    anon_huge_peak: ['parsed_row.os', 'anon_huge_peak', 'bytes', 'live AnonHugePages observation'],
    capture_slot: ['parsed_row', 'capture_slot_storage_bytes', 'bytes', 'native advice capture storage'],
    capture_total: ['parsed_row', 'capture_total_storage_bytes', 'bytes', 'native advice capture storage'],
    capture_tls: ['parsed_row', 'capture_tls_context_bytes_per_thread', 'bytes', 'native advice capture storage'],
  };
  if (ab.facts.regime === 'CACHE') definitions.cold_first_life = ['runner_root', 'cold_ns', 'ns', 'first-life allocation+touch+free; COLD'];
  const rows = Object.entries(definitions).map(([metric, [src, field, unit, scope]]) => ({
    evidence_class: input.evidence_class, cell: ab.facts.cell, regime: metric === 'cold_first_life' ? 'COLD' : ab.facts.regime,
    metric, mapping: { source: src, field, unit, scope },
    ratio: metric === 'selected' ? ci : null,
    aa: metric === 'cold_first_life' ? null : aa,
    aaReadable: metric === 'cold_first_life' ? null : readable,
    claimNote: metric === 'cold_first_life' ? 'COLD first-life median; descriptive, no CI' : readable ? 'A/A interval contains 1; readability only, not speed evidence' : 'A/A unreadable; speed claim unsupported only',
    medians: { AB_Off: summarize(ab, 'A', metric), AB_Always: summarize(ab, 'B', metric),
      control_A: summarize(control, 'A', metric), control_B: summarize(control, 'B', metric) },
  }));
  const facts = { AB: ab.facts, control: control.facts }, host = input.measurement_host;
  const rendered = render(rows, facts, host, receipt, input.evidence_class, estimator);
  return { evidence_class:input.evidence_class, contexts:{source_identity:source}, facts, build_receipt:receipt, measurement_host:host, documents:{AB:ab.doc,control:control.doc}, ci, aa_ci:aa, estimator, rows, ...rendered };
}
