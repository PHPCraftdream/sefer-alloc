import assert from 'node:assert/strict';
import { j13LimitedReport } from '../scripts/j13_limited_report.mjs';

const MIB = 1048576;
const source = {
  commit: '0123456789abcdef0123456789abcdef01234567',
  tree: 'abcdef0123456789abcdef0123456789abcdef01',
  dirty: false,
};
const receipt = {
  schema: 'g1-big-study-ci-build-receipt.v1',
  build_context: {
    ...source, status_porcelain: '', rustc_version_verbose: 'rustc synthetic fixture',
    cargo_version: 'cargo synthetic fixture', rustc_host_target: 'x86_64-unknown-linux-gnu',
    target_selection: { explicit: true, target: 'x86_64-unknown-linux-gnu' },
    RUSTFLAGS: { set: false, value: null }, CARGO_ENCODED_RUSTFLAGS: { set: false, value: null },
  },
  build: {
    command: ['cargo', 'build', '--locked', '--release', '--target', 'x86_64-unknown-linux-gnu',
      '--features', 'production alloc-stats internals bench-internals', '--example', 'g1_big_judge',
      '--message-format=json-render-diagnostics'],
    executable_path: '/build/target/release/examples/g1_big_judge',
    sha256: '1'.repeat(64),
    artifact_features: ['default', 'production', 'alloc-stats', 'internals', 'bench-internals', 'serde'],
    artifact_target: { name: 'g1_big_judge', kind: ['example'] },
    artifact_executable: '/build/target/release/examples/g1_big_judge',
  },
  copied_artifact: { path: 'measurement-executable', sha256: '1'.repeat(64) },
  host_target: 'x86_64-unknown-linux-gnu',
};
const measurementHost = {
  scope: 'measurement_host', os: 'linux', arch: 'x86_64',
  cpu_info: 'Synthetic measurement CPU, 8 cores', base_page_bytes: 4096,
  thp_page_bytes: 2097152, thp_mode: 'madvise', captured_before: '2026-11-10T00:00:00.000Z',
  memory_available_bytes: 16 * 1024 * MIB, active_swap: false, active_pressure: false,
};
const order = ['A', 'B', 'B', 'A'];
const gradient = 'LargeThpGradient { down_shift: 2, up_shift: 5, mean_shift: 5, anchor_raw: 0, floor_raw: 214748365, gain_num: 5, gain_den: 4, curve: Linear }';

function producer(role, slot, block, repeat) {
  const always = role === 'AB' && slot === 'B';
  const base = role === 'AB' ? (slot === 'A' ? 1000 : 800) : 1000;
  return {
    study: true, diagnostics: 'native-advice-capture', scenario: 'cycle', arm: 'sefer',
    requested_bytes: 16 * MIB, reps: 1, k: 1,
    cold_ns: base + block * (role === 'AB' ? (slot === 'A' ? 10 : 7) : 10)
      + repeat * (role === 'AB' ? (slot === 'A' ? 400 : 300) : 4),
    warm_ns: null, cache_ns: null,
    timing_scope: 'allocation+touch+free; runner full-child timing distinct', rep: [[0, 0, 0]],
    mode_requested: always ? 'Always' : 'Off', mode_resolved: always ? 'Always' : 'Off',
    thp_min_requested_bytes: 8388608, thp_min_resolved_bytes: 8388608,
    thp_budget_requested_bytes: 0, thp_budget_resolved_bytes: 0,
    gradient_requested: 'CAUTIOUS', gradient_resolved: gradient,
    cache_requested: 'off', cache_resolved_budget_bytes: 0,
    cache_hit_confirmed: false, large_cache_hits: 0, config_conflicts: 0, thp_tuple_conflicts: 0,
    fresh_mappings: 1, fresh_requested_bytes: [16 * MIB], accepted_advice_count: always ? 1 : 0,
    accepted_advice: always ? [{ requested_bytes: 16 * MIB, usable_span_bytes: 16 * MIB,
      accepted_bytes: 16 * MIB }] : [],
    advice_granule_bytes: 2097152, capture_events: 4, capture_boundary: 4,
    capture_slot_storage_bytes: 64, capture_total_storage_bytes: 128,
    capture_tls_context_bytes_per_thread: 32,
    os: { faults: 3, peak_rss: 100000, end_rss: 90000, anon_huge_peak: always ? 16 * MIB : null },
  };
}

function syncAggregates(document) {
  const comparison = document.comparisons[0], launches = comparison.raw_process_launches;
  const means = slot => Array.from({ length: 12 }, (_, block) => {
    const pair = launches.filter(row => row.block === block && row.slot === slot).map(row => row.cold_ns);
    return (pair[0] + pair[1]) / 2;
  });
  comparison.samples_a_ns = means('A');
  comparison.samples_b_ns = means('B');
  comparison.deltas_a_minus_b_ns = comparison.samples_a_ns.map((value, i) => value - comparison.samples_b_ns[i]);
}

function setProducer(row, change) {
  change(row.parsed_row);
  for (const [key, value] of Object.entries(row.parsed_row)) {
    if (!['arm', 'block', 'slot', 'launch', 'wall_clock_iso'].includes(key)) row[key] = value;
  }
  row.parsed_row.rep = [[row.parsed_row.cold_ns - 2, 1, 1]];
  if (row.parsed_row.reps === 2) row.parsed_row.rep.push([row.parsed_row.cache_ns - 2, 1, 1]);
  row.rep = row.parsed_row.rep;
  row.launch.raw_stdout = JSON.stringify(row.parsed_row);
}

function fixture() {
  const input = {
    evidence_class: 'synthetic', build_receipt: structuredClone(receipt),
    measurement_host: structuredClone(measurementHost),
  };
  for (const role of ['AB', 'control']) {
    const raw = [];
    for (let block = 0; block < 12; block++) {
      for (let index = 0; index < 4; index++) {
        const slot = order[index];
        const repeat = slot === 'A' ? (index === 0 ? 0 : 1) : (index === 1 ? 0 : 1);
        const parsed = producer(role, slot, block, repeat);
        parsed.rep = [[parsed.cold_ns - 2, 1, 1]];
        const always = role === 'AB' && slot === 'B';
        const launch = {
          command: './measurement-executable', args: ['cycle', 'sefer', '16', '1'],
          executable_path: '/measurement/measurement-executable',
          binary_sha256: '1'.repeat(64), binary_sha256_after: '1'.repeat(64), child_wall_ns: 5000,
          env_overrides: { G1_BIG_THP: always ? 'always' : 'off', G1_BIG_CACHE: 'off' },
          child_exit_code: 0, signal: null, error: null, timed_out: false,
          raw_stdout: JSON.stringify(parsed), raw_stderr: '',
        };
        const offset = role === 'AB' ? 0 : 100000;
        const time = new Date(Date.parse(measurementHost.captured_before) + 1 + offset + raw.length).toISOString();
        raw.push({ ...parsed, block, slot, arm: role === 'AB' ? slot : 'A', wall_clock_iso: time,
          parsed_row: structuredClone(parsed), launch });
      }
    }
    const document = {
      timestamp: new Date(Date.parse(raw.at(-1).wall_clock_iso) + 1).toISOString(),
      result_format: 'json', source_identity: structuredClone(source), metric: 'cold_ns',
      pairs_per_comparison: 12,
      comparisons: [{ arm_a: 'A', arm_b: role === 'AB' ? 'B' : 'A', pairs: 12,
        samples_a_ns: [], samples_b_ns: [], deltas_a_minus_b_ns: [], paired_t_test: null,
        sign_test: null, raw_process_launches: raw }],
    };
    syncAggregates(document);
    input[role === 'AB' ? 'ab' : 'control'] = {
      document, facts: { cell: 'cycle-16MiB', regime: 'COLD' },
    };
  }
  return input;
}

const input = fixture();
const before = structuredClone(input);
const report = j13LimitedReport(input);
assert.deepEqual(input, before);
assert.deepEqual(j13LimitedReport(input), report);
assert.equal(report.rows.length, 9);
assert.ok(report.ci.ratio < 1);
assert.ok(report.aa_ci.lo <= 1 && report.aa_ci.hi >= 1);
assert.equal(report.rows.find(row => row.metric === 'anon_huge_peak').medians.AB_Always, 16 * MIB);
assert.equal(report.rows.find(row => row.metric === 'anon_huge_peak').medians.AB_Off, null);
assert.equal(report.rows.find(row => row.metric === 'child').mapping.source, 'runner_launch');
assert.equal(report.rows.find(row => row.metric === 'selected').mapping.field, 'cold_ns');
assert.match(report.csv, /runner_launch/);
assert.match(report.csv, /estimator_metadata/);
assert.match(report.markdown, /In CACHE, OS and full-child fields span cold\+cache\+trim as one same-process observation, not per-life memory evidence/);
assert.match(report.markdown, /no product GO\/veto/);
assert.match(report.markdown, /Materially new regressions require owner review/);
assert.match(report.markdown, /no default change/);
assert.match(report.markdown, /not exact physical maxima or allocator\/advice attribution/);
assert.deepEqual(report.facts, {
  AB: { cell: 'cycle-16MiB', regime: 'COLD' },
  control: { cell: 'cycle-16MiB', regime: 'COLD' },
});
assert.equal(report.facts.measurement_host, undefined);
assert.equal(report.measurement_host.cpu_info, 'Synthetic measurement CPU, 8 cores');
assert.equal(report.build_receipt.build_context.rustc_host_target, 'x86_64-unknown-linux-gnu');
assert.notEqual(report.measurement_host.cpu_info, report.build_receipt.build_context.cpu_info);
assert.equal(report.rows.find(row => row.metric === 'child').medians.AB_Off, 5000);
assert.match(report.markdown, /faults \(parsed_row\.os\.faults\).*process fault observation/);

{
  const value = fixture(), launches = value.ab.document.comparisons[0].raw_process_launches.filter(row => row.slot === 'A');
  launches.forEach((row, i) => setProducer(row, p => { p.os.faults = i < 12 ? 21 : i < 23 ? 22 : 100; }));
  const result = j13LimitedReport(value), faults = result.rows.find(row => row.metric === 'faults');
  assert.equal(faults.medians.AB_Off, 21.5);
  assert.equal(launches.reduce((sum, row) => sum + row.parsed_row.os.faults, 0) / launches.length, 24.75);
  const faultLine = result.markdown.split('\n').find(line => line.includes('| faults (parsed_row.os.faults) | process fault observation |'));
  assert.ok(faultLine);
  assert.equal(faultLine.split('|')[7].trim(), '21.5 → 3');
}

const raw = input.ab.document.comparisons[0].raw_process_launches;
const minimums = slot => Array.from({ length: 12 }, (_, block) => Math.min(
  ...raw.filter(row => row.block === block && row.slot === slot).map(row => row.cold_ns),
));
const minA = minimums('A'), minB = minimums('B');
const independentRatio = Math.exp(minB.reduce((sum, value, i) => sum + Math.log(value) - Math.log(minA[i]), 0) / 12);
assert.ok(Math.abs(report.ci.ratio - independentRatio) < 1e-12); // adjacent independent block-log arithmetic
const inverseRatio = Math.exp(minA.reduce((sum, value, i) => sum + Math.log(value) - Math.log(minB[i]), 0) / 12);
assert.ok(Math.abs(report.ci.ratio * inverseRatio - 1) < 1e-12);
const runnerMeans = input.ab.document.comparisons[0];
const meanRatio = Math.exp(runnerMeans.samples_b_ns.reduce((sum, value, i) =>
  sum + Math.log(value) - Math.log(runnerMeans.samples_a_ns[i]), 0) / 12);
assert.ok(Math.abs(report.ci.ratio - meanRatio) > 1e-4); // runner means are not the reducer statistic
const logs = minB.map((value, i) => Math.log(value) - Math.log(minA[i]));
const center = logs.reduce((sum, value) => sum + value, 0) / 12;
const sd = Math.sqrt(logs.reduce((sum, value) => sum + (value - center) ** 2, 0) / 11);
assert.ok(Math.abs(report.ci.lo - Math.exp(center - 2.201 * sd / Math.sqrt(12))) < 1e-12);
assert.ok(Math.abs(report.ci.hi - Math.exp(center + 2.201 * sd / Math.sqrt(12))) < 1e-12);
assert.ok(Math.abs(report.ci.ratio - Math.exp(center)) < 1e-12);

{
  const value = fixture(), row = value.ab.document.comparisons[0].raw_process_launches[0];
  setProducer(row, producer => { producer.os.faults = null; producer.os.peak_rss = null; producer.os.anon_huge_peak = null; });
  const rows = j13LimitedReport(value).rows;
  for (const metric of ['faults', 'peak_rss', 'anon_huge_peak'])
    assert.equal(rows.find(item => item.metric === metric).medians.AB_Off, null);
}

function rejects(mutator, pattern) {
  const value = fixture();
  mutator(value);
  assert.throws(() => j13LimitedReport(value), pattern);
}
const mutations = [
  [value => value.ab.document.comparisons[0].raw_process_launches.pop(), /exactly 12/],
  [value => value.ab.document.comparisons[0].raw_process_launches.push({ ...value.ab.document.comparisons[0].raw_process_launches[0] }), /exactly 12/],
  [value => { const rows = value.ab.document.comparisons[0].raw_process_launches; rows[1] = { ...rows[0] }; }, /block or slot|duplicate raw/],
  [value => { value.ab.document.comparisons[0].raw_process_launches[0].block = 11; }, /block or slot/],
  [value => { const rows = value.ab.document.comparisons[0].raw_process_launches; [rows[0], rows[1]] = [rows[1], rows[0]]; }, /block or slot/],
  [value => { value.ab.document.comparisons[0].raw_process_launches[0].wall_clock_iso = 'bad'; }, /timestamp/],
  [value => { value.ab.document.timestamp = '2026-02-30T00:00:00.000Z'; }, /observer identity\/timestamp/],
  [value => { value.ab.document.comparisons[0].samples_a_ns[0]++; }, /aggregates disagree/],
  [value => { value.ab.document.comparisons[0].deltas_a_minus_b_ns[0]++; }, /aggregates disagree/],
  [value => { value.ab.document.source_identity.tree = '0'.repeat(40); }, /observer source mismatch|does not match clean build source/],
  [value => { value.ab.document.source_identity.commit = '0'.repeat(40); }, /does not match clean build source/],
  [value => { value.control.document.source_identity.tree = '0'.repeat(40); }, /runner observer identity|does not match clean build source/],
  [value => { value.control.document.source_identity.commit = '0'.repeat(40); }, /runner observer identity|does not match clean build source/],
  [value => { value.ab.document.source_identity = undefined; }, /observer source mismatch|source_identity is required/],
  [value => { value.ab.document.comparisons[0].raw_process_launches[0].launch.binary_sha256_after = '2'.repeat(64); }, /launch hash/],
  [value => { value.build_receipt.build.sha256 = '2'.repeat(64); }, /receipt copied\/artifact hash mismatch/],
  [value => { value.build_receipt.build.artifact_features = value.build_receipt.build.artifact_features.filter(f => f !== 'alloc-stats'); }, /resolved artifact features/],
  [value => { value.build_receipt.build.artifact_features.push('numa-aware'); }, /resolved artifact features/],
  [value => { value.build_receipt.build.command[5] = 'wrong-target'; }, /build argv\/target/],
  [value => { value.build_receipt.build_context.RUSTFLAGS = { set: true, value: null }; }, /RUSTFLAGS/],
  [value => { value.build_receipt.build_context.CARGO_ENCODED_RUSTFLAGS = undefined; }, /CARGO_ENCODED_RUSTFLAGS/],
  [value => { const row = value.ab.document.comparisons[0].raw_process_launches[1]; setProducer(row, p => { p.mode_resolved = 'Off'; }); }, /resolved THP tuple/],
  [value => { value.ab.document.comparisons[0].raw_process_launches[1].launch.env_overrides.G1_BIG_THP = 'off'; }, /launch args\/environment/],
  [value => { const row = value.ab.document.comparisons[0].raw_process_launches[1]; setProducer(row, p => { p.thp_budget_resolved_bytes = 1; }); }, /resolved THP tuple/],
  [value => { const row = value.ab.document.comparisons[0].raw_process_launches[1]; setProducer(row, p => { p.cache_resolved_budget_bytes = 1; }); }, /cache\/config conflict/],
  [value => { const row = value.ab.document.comparisons[0].raw_process_launches[1]; setProducer(row, p => { p.accepted_advice[0].accepted_bytes = 1; }); }, /accepted advice prefix/],
  [value => { const row = value.ab.document.comparisons[0].raw_process_launches[0]; setProducer(row, p => { p.fresh_mappings = 2; }); }, /fresh mapping/],
  ...[0, -1, Infinity, NaN, Number.MAX_SAFE_INTEGER + 1, '7'].map(bad => [value => {
    const row = value.ab.document.comparisons[0].raw_process_launches[0]; setProducer(row, p => { p.cold_ns = bad; });
  }, /runner cold metric differs|raw stdout mismatch/]),
  [value => { value.build_receipt = undefined; }, /trusted CI build receipt schema required/],
  [value => { value.measurement_host = undefined; }, /measurement-host scope/],
  [value => { value.build_receipt.build_context.rustc_version_verbose = 'unavailable'; value.evidence_class = 'measured'; }, /compiler\/host facts unavailable/],
  [value => { value.measurement_host.cpu_info = 'unavailable'; value.evidence_class = 'measured'; }, /measurement-host scope\/identity required/],
  [value => { value.measurement_host.captured_before = '2026-11-10T00:00:10.000Z'; }, /measurement-host preflight/],
  [value => { value.measurement_host.memory_available_bytes = 1; }, /measurement-host preflight/],
  [value => { value.control.facts.regime = 'CACHE'; }, /unregistered cell\/regime|cell or regime mismatch/],
  [value => { value.ab.facts.source = { commit: source.commit, tree: source.tree, dirty: false }; }, /minimal cell\/regime facts/],
  [value => { value.ab.document.git_commit = source.commit; }, /actual JSON runner root/],
];
for (const [mutate, expected] of mutations) rejects(mutate, expected);

// Extra transitive Cargo features are accepted when required resolved features are present.
{
  const value = fixture();
  value.build_receipt.build.artifact_features.push('fixture-transitive-feature');
  assert.doesNotThrow(() => j13LimitedReport(value));
}

// An A/A interval outside one affects readability only, not the speed direction.
{
  const value = fixture(), launches = value.control.document.comparisons[0].raw_process_launches;
  for (const row of launches.filter(item => item.slot === 'B'))
    setProducer(row, producer => { producer.cold_ns += 100; });
  syncAggregates(value.control.document);
  const result = j13LimitedReport(value);
  assert.equal(result.rows[0].aaReadable, false);
  assert.match(result.rows[0].claimNote, /speed claim unsupported only/);
  assert.ok(result.aa_ci.ratio > 1);
}

// Cache rows retain first-life COLD and separately select second-life cache_ns.
{
  const value = fixture();
  value.ab.facts.cell = value.control.facts.cell = 'cache-16MiB';
  value.ab.facts.regime = value.control.facts.regime = 'CACHE';
  for (const [role, pair] of [['AB', value.ab], ['control', value.control]]) {
    for (const row of pair.document.comparisons[0].raw_process_launches) {
      const always = role === 'AB' && row.slot === 'B';
      setProducer(row, producer => {
        producer.reps = 2;
        producer.cache_ns = (always ? 700 : 900) + row.block;
        producer.cache_requested = 'on';
        producer.cache_resolved_budget_bytes = 33554432;
        producer.cache_hit_confirmed = true;
        producer.large_cache_hits = 1;
        producer.accepted_advice = always
          ? [{ requested_bytes: 16 * MIB, usable_span_bytes: 16 * MIB, accepted_bytes: 16 * MIB }] : [];
        producer.accepted_advice_count = always ? 1 : 0;
      });
      row.launch.args = ['cycle', 'sefer', '16', '2', '1'];
      row.launch.env_overrides.G1_BIG_CACHE = 'on';
    }
  }
  const result = j13LimitedReport(value);
  const cacheRatio = Math.exp(Array.from({ length: 12 }, (_, block) => Math.log(700 + block) - Math.log(900 + block)).reduce((sum, log) => sum + log, 0) / 12);
  assert.ok(Math.abs(result.ci.ratio - cacheRatio) < 1e-12);
  assert.ok(Math.abs(result.ci.ratio - report.ci.ratio) > 1e-3, 'cold_ns selection must fail the cache oracle');
  for (const pair of [value.ab, value.control]) for (const row of pair.document.comparisons[0].raw_process_launches) {
    assert.equal(row.parsed_row.rep.length, 2);
    assert.equal(row.parsed_row.rep[0].reduce((sum, ns) => sum + ns, 0), row.parsed_row.cold_ns);
    assert.equal(row.parsed_row.rep[1].reduce((sum, ns) => sum + ns, 0), row.parsed_row.cache_ns);
  }
  assert.equal(result.rows.find(row => row.metric === 'selected').mapping.source, 'parsed_row');
  assert.equal(result.rows.find(row => row.metric === 'selected').mapping.field, 'cache_ns');
  assert.equal(result.rows.find(row => row.metric === 'cold_first_life').mapping.field, 'cold_ns');
  assert.equal(result.rows.find(row => row.metric === 'cold_first_life').ratio, null);
  assert.match(result.rows.find(row => row.metric === 'cold_first_life').claimNote, /descriptive, no CI/);
}

console.log(`j13 limited report synthetic fixtures passed: ${mutations.length} fail-closed mutations plus cold/cache statistics, rendering, and null observations`);
