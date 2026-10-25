// Raw observations → CSV/Markdown; --check не перезаписывает артефакты.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const dir = path.join(root, 'docs/perf/scalar_issue_prepared');
const raw = path.join(dir, 'raw');
const identity = JSON.parse(fs.readFileSync(path.join(dir, 'source_identity.json'), 'utf8'));
const completed = JSON.parse(fs.readFileSync(path.join(dir, 'completed.json'), 'utf8'));
assert.equal(completed.canonical_sweeps, 4);
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
for (const arm of ['baseline', 'prepared']) {
  assert.equal(sha(fs.readFileSync(path.join(dir, `${arm}.patch`))), identity.identities[arm].patch_sha256);
  assert.equal(sha(fs.readFileSync(path.join(dir, 'scalar_issue_gate.rs'))), identity.identities[arm].new_example_sha256);
}
function mean(xs) { assert(xs.length > 0); return xs.reduce((a, b) => a + b, 0) / xs.length; }
function median(xs) { const s = [...xs].sort((a, b) => a - b); return (s[Math.floor((s.length - 1) / 2)] + s[Math.floor(s.length / 2)]) / 2; }
function ci(xs) {
  assert(xs.length === 10 || xs.length === 20);
  const m = mean(xs);
  const se = Math.sqrt(xs.reduce((s, x) => s + (x - m) ** 2, 0) / (xs.length - 1) / xs.length);
  const t = xs.length === 20 ? 2.093024054 : 2.262157163;
  return [m - t * se, m + t * se];
}
function ratio(a, b) { assert(Number.isFinite(a) && Number.isFinite(b) && a > 0 && b > 0); const r = a / b; assert(Math.abs(r * b - a) <= a * 1e-12); return r; }
const f = n => { assert(Number.isFinite(n)); return n.toFixed(4); };
const observations = fs.readFileSync(path.join(raw, 'scalar.ndjson'), 'utf8').trim().split('\n').map(JSON.parse);
assert.equal(observations.length, completed.scalar_processes);
assert.equal(observations.length, 200);
for (const row of observations) {
  assert.equal(row.oracle_ok, true);
  assert.equal(row.scalar_calls, row.n);
  assert(row.internal_ns > 0 && row.process_ns > row.internal_ns);
}
const scalar = [];
for (const n of [32768, 65536]) {
  const rows = observations.filter(x => !x.control && x.n === n);
  assert.equal(rows.length, 80);
  for (const metric of ['internal_ns', 'process_ns']) {
    const a = [], b = [], delta = [];
    for (let block = 0; block < 20; block++) {
      const group = rows.filter(x => x.block === block).sort((x, y) => x.position - y.position);
      assert.deepEqual(group.map(x => x.arm), ['baseline', 'prepared', 'prepared', 'baseline']);
      assert.deepEqual(group.map(x => x.position), [0, 1, 2, 3]);
      a.push(mean([group[0][metric], group[3][metric]]));
      b.push(mean([group[1][metric], group[2][metric]]));
      delta.push(b.at(-1) - a.at(-1));
    }
    const bounds = ci(delta), r = ratio(mean(b), mean(a));
    scalar.push({ n, metric, a: mean(a), b: mean(b), ratio: r, delta: mean(delta), low: bounds[0], high: bounds[1], pass: metric === 'internal_ns' && bounds[1] < 0 && r <= 0.99 });
  }
}
const aa = [];
for (const metric of ['internal_ns', 'process_ns']) {
  const delta = [];
  for (let block = 0; block < 10; block++) {
    const group = observations.filter(x => x.control && x.block === block).sort((a, b) => a.position - b.position);
    assert.equal(group.length, 4);
    assert(group.every(x => x.arm === 'baseline' && x.n === 65536));
    delta.push(mean([group[1][metric], group[2][metric]]) - mean([group[0][metric], group[3][metric]]));
  }
  const [low, high] = ci(delta);
  aa.push({ metric, delta: mean(delta), low, high, pass: low <= 0 && high >= 0 });
}
function cells(sweep) {
  const out = new Map();
  function walk(folder) {
    for (const e of fs.readdirSync(folder, { withFileTypes: true })) {
      const p = path.join(folder, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name === 'benchmark.json') {
        const meta = JSON.parse(fs.readFileSync(p));
        const sample = JSON.parse(fs.readFileSync(path.join(folder, 'sample.json')));
        const estimates = JSON.parse(fs.readFileSync(path.join(folder, 'estimates.json')));
        assert.equal(sample.times.length, 10);
        assert.equal(sample.iters.length, 10);
        const estimator = estimates.slope === null ? 'mean' : 'slope';
        const ns = estimates[estimator].point_estimate;
        assert(ns > 0 && sample.times.every(x => x > 0) && sample.iters.every(x => x > 0));
        if (estimator === 'slope') {
          const slope = sample.times.reduce((s, x, i) => s + x * sample.iters[i], 0) / sample.iters.reduce((s, x) => s + x * x, 0);
          assert(Math.abs(slope / ns - 1) < 1e-9, meta.full_id);
        } else {
          const m = mean(sample.times.map((x, i) => x / sample.iters[i]));
          assert(Math.abs(m / ns - 1) < 1e-9, meta.full_id);
        }
        assert(!out.has(meta.full_id));
        out.set(meta.full_id, { ns, estimator });
      }
    }
  }
  walk(path.join(raw, sweep));
  return out;
}
const runs = ['criterion-0-baseline', 'criterion-1-prepared', 'criterion-2-prepared', 'criterion-3-baseline'].map(cells);
assert.deepEqual([...runs[0].keys()].sort(), [...runs[1].keys()].sort());
for (const r of runs.slice(2)) assert.deepEqual([...runs[0].keys()].sort(), [...r.keys()].sort());
const table = [];
const canonicalGroups = new Set(['global_alloc', 'global_alloc_churn', 'global_alloc_churn_write', 'global_alloc_churn_with_teardown']);
for (const id of [...runs[0].keys()].filter(x => canonicalGroups.has(x.split('/')[0]) && x.includes('/SeferAlloc/')).sort()) {
  const miId = id.replace('/SeferAlloc/', '/mimalloc/');
  const sysId = id.replace('/SeferAlloc/', '/System/');
  assert(runs.every(r => r.has(miId) && r.has(sysId)));
  const scale = id.includes('/manual_realloc_sim/') ? 1 : 1024;
  const a = mean([runs[0].get(id).ns, runs[3].get(id).ns]) / scale;
  const b = mean([runs[1].get(id).ns, runs[2].get(id).ns]) / scale;
  const ma = mean([runs[0].get(miId).ns, runs[3].get(miId).ns]) / scale;
  const mb = mean([runs[1].get(miId).ns, runs[2].get(miId).ns]) / scale;
  const winA = median([ratio(runs[0].get(miId).ns, runs[0].get(id).ns), ratio(runs[3].get(miId).ns, runs[3].get(id).ns)]);
  const winB = median([ratio(runs[1].get(miId).ns, runs[1].get(id).ns), ratio(runs[2].get(miId).ns, runs[2].get(id).ns)]);
  table.push({ id, unit: scale === 1 ? 'ns/closure' : id.includes('with_teardown') ? 'ns/pair composite' : 'ns/pair', a, b, ma, mb, winA, winB, ba: ratio(b, a), miDrift: ratio(mb, ma) });
}
// manual_realloc_sim в Criterion имеет другой порядок компонентов ID.
const manualIds = [...runs[0].keys()].filter(x => x.includes('/manual_realloc_sim/'));
for (const id of manualIds) {
  if (id.endsWith('/SeferAlloc')) {
    const miId = id.replace(/SeferAlloc$/, 'mimalloc');
    const a = mean([runs[0].get(id).ns, runs[3].get(id).ns]);
    const b = mean([runs[1].get(id).ns, runs[2].get(id).ns]);
    const ma = mean([runs[0].get(miId).ns, runs[3].get(miId).ns]);
    const mb = mean([runs[1].get(miId).ns, runs[2].get(miId).ns]);
    table.push({ id, unit: 'ns/closure', a, b, ma, mb, winA: mean([ratio(runs[0].get(miId).ns, runs[0].get(id).ns), ratio(runs[3].get(miId).ns, runs[3].get(id).ns)]), winB: mean([ratio(runs[1].get(miId).ns, runs[1].get(id).ns), ratio(runs[2].get(miId).ns, runs[2].get(id).ns)]), ba: ratio(b, a), miDrift: ratio(mb, ma) });
  }
}
assert(table.length >= 16, `missing canonical cells: ${table.length}`);
const microUseful = scalar.filter(x => x.metric === 'internal_ns').every(x => x.pass) && aa.every(x => x.pass);
const verdict = microUseful ? 'EARLY MICRO-GATE GO; PROMOTION NOT VERIFIED' : 'INCONCLUSIVE / DO NOT PROMOTE';
const csvCell = v => /[,"\r\n]/.test(String(v)) ? `"${String(v).replaceAll('"', '""')}"` : String(v);
const metadata = [identity.identities.baseline.base, identity.identities.prepared.patch_sha256, identity.features, identity.cpu, `${identity.platform}/${identity.release}/${identity.arch}`, 2, 10];
const csv = [['workload', 'unit', 'before', 'after', 'mimalloc_before', 'mimalloc_after', 'mimalloc_over_before', 'mimalloc_over_after', 'after_over_before', 'mimalloc_drift', 'base_sha', 'candidate_patch_sha256', 'features', 'cpu', 'os', 'sweeps_per_arm', 'samples_per_sweep'], ...table.map(x => [x.id, x.unit, x.a, x.b, x.ma, x.mb, x.winA, x.winB, x.ba, x.miDrift, ...metadata])].map(r => r.map(csvCell).join(',')).join('\n') + '\n';
const scalarCsv = [['n', 'metric', 'mean_before_ns', 'mean_after_ns', 'after_over_before', 'paired_delta_ns', 'ci95_low_ns', 'ci95_high_ns'], ...scalar.map(x => [x.n, x.metric, x.a, x.b, x.ratio, x.delta, x.low, x.high])].map(r => r.join(',')).join('\n') + '\n';
const lines = [
  '# Scalar prepared issue — экспериментальная доработка и сравнительные замеры', '',
  `Дата: 2026-10-24. Verdict: **${verdict}**.`, '',
  'Изменены два production-файла: удалены повторные prepared-проверки scalar commit; final checked issue, Release store и публичный auto-prepare owner API сохранены. D1′ не используется. B4-R3 batch path не менялся.', '',
  '## Источники и метод', '',
  `Base: \`${identity.identities.baseline.base}\`. Точные patches, SHA-256 и toolchain — [source_identity.json](scalar_issue_prepared/source_identity.json), captured до сборки. Features: \`${identity.features}\`. ${identity.platform}/${identity.arch}, ${identity.cpu}.`, '',
  'Пререгистрация: `docs/design/2026-10-24-scalar-issue-measurement-plan.md`. Raw данные: `scalar_issue_prepared/raw/`; canonical estimates восстановлены из Criterion samples и проверены генератором. Каноническая таблица — direct GlobalAlloc facade, НЕ installed allocator. Scalar microgate — отдельный AllocCore diagnostic, он не доказывает global speedup.', '',
  '## Относительно mimalloc до и после', '',
  'Четыре полных canonical sweep A/B/B/A; два sweep на arm. Before/after и mimalloc — арифметические средние двух Criterion point estimates. Win — медиана двух внутрипроцессных отношений mimalloc/Sefer. Win > 1 означает Sefer быстрее, Win < 1 — медленнее. B/A < 1 означает снижение времени Sefer. Различные единицы и composite teardown не смешиваются. Это описательные числа; два sweep на arm не дают убедительного paired CI изменения.', '',
  '| Workload | Unit | Before | After | mimalloc A | mimalloc B | Win before | Win after | B/A | mi B/A |',
  '|---|---|---:|---:|---:|---:|---:|---:|---:|---:|',
  ...table.map(x => `| ${x.id} | ${x.unit} | ${f(x.a)} | ${f(x.b)} | ${f(x.ma)} | ${f(x.mb)} | ${f(x.winA)} | ${f(x.winB)} | ${f(x.ba)} | ${f(x.miDrift)} |`), '',
  'mi B/A показывает drift неизменённого конкурента; нельзя приписать все колебания B/A патчу. Группы global_alloc — warm bulk bursts, не первый холодный alloc. Отсутствие scalar activation в отдельной canonical строке не трактуется как ошибка: основные fastbin refills идут через неизменённый batch path; её числа не служат атрибуцией scalar механизма.', '',
  '## Ранний scalar gate', '',
  '20 fresh-process ABBA blocks на N. В таблице — mean двух одноимённых процессов в block, затем mean по 20 blocks. CI относится к среднему paired B−A; внутреннее окно и полный process wall разделены. Oracle после timed region проверяет каждый start и отсутствие class у interior granule. Setup/teardown, oracle и запуск процесса входят в process wall, не internal_ns.', '',
  '| N | Metric | Mean A ns | Mean B ns | B/A | Paired delta ns | 95% CI ns |',
  '|---:|---|---:|---:|---:|---:|---|',
  ...scalar.map(x => `| ${x.n} | ${x.metric} | ${f(x.a)} | ${f(x.b)} | ${f(x.ratio)} | ${f(x.delta)} | ${f(x.low)} … ${f(x.high)} |`), '',
  '### A/A controls', '',
  '| Metric | Mean pseudo-B−A ns | 95% CI ns | Includes zero |',
  '|---|---:|---|---|',
  ...aa.map(x => `| ${x.metric} | ${f(x.delta)} | ${f(x.low)} … ${f(x.high)} | ${x.pass} |`), '',
  '### N/2N slope', '',
];
for (const metric of ['internal_ns', 'process_ns']) {
  const a = scalar.find(x => x.metric === metric && x.n === 32768), b = scalar.find(x => x.metric === metric && x.n === 65536);
  lines.push(`- ${metric}: marginal mean A = ${f((b.a - a.a) / 32768)} ns/scalar call; B = ${f((b.b - a.b) / 32768)} ns/scalar call. Разность двух means включает изменение cold page/leaf стоимости; не отдельный чистый instruction counter.`);
}
lines.push('', '## Граница результата', '',
  'Локально прошли rustfmt --check затронутых Rust-файлов, Clippy -D warnings для lib production и lib/test/example production internals bench-internals, cargo check default и core-only internals negative row нового теста. Correctness execution, Miri/Loom и exact-head CI ещё НЕ получены: локальные тесты не разрешены, push не запрошен. Добавлена отдельная regression suite scalar_issue_prepared; cargo check не означает test PASS. Нет разрешения merge. Все сравнительные процессы здесь — Windows, не native Linux и не WSL.', '',
  'Контрфактуальный source review: пропуск final class store обнаруживается проверкой class/start; пропуск fallible preflight — проверкой отказа без изменения owner state; потеря owner auto-prepare — проверкой свежего route. Возврат лишних prepared-проверок сохраняет поведение и не обязан ломать correctness test: его цена проверяется performance gate. Существующий ClassLeaves::issue проверяет null leaf и bounds без promotion; root/live-slot/fatal и Acquire/Release сохранены. Новых unsafe-блоков в production нет; unsafe в новом test/probe ограничен освобождением уникальных живых блоков с исходным layout. Это узкий single-context review, не полный rust-intel аудит проекта.', '',
  'Полная promotion-матрица IAI/dense16/registration/Larson/memory не объявляется закрытой. Общий выигрыш нельзя вывести из удаления двух source-level checks или из одной удачной строки. При непрохождении раннего gate кандидат не принимать; текущая main не меняется.', '',
  'Воспроизведение: применить соответствующий patch к base, скопировать сохранённый scalar_issue_gate.rs в examples/, использовать отдельные target directories; запустить scalar probe N=32768/65536 по ABBA и A/A, затем npm run bench:table A/B/B/A. Полный runner сохранён в scalar_issue_prepared/reproduction/measure.mjs; его ожидаемый layout описан рядом. Проверка отчёта: `node scripts/scalar_issue_report.mjs --check`.', '');
function emit(file, text) {
  if (process.argv.includes('--check')) assert.equal(fs.readFileSync(file, 'utf8'), text, `stale ${path.basename(file)}`);
  else fs.writeFileSync(file, text);
}
emit(path.join(root, 'docs/perf/SCALAR_ISSUE_PREPARED_GATE.md'), lines.join('\n'));
emit(path.join(root, 'docs/perf/SCALAR_ISSUE_PREPARED_GATE_summary.csv'), csv);
emit(path.join(dir, 'scalar_summary.csv'), scalarCsv);
console.log(JSON.stringify({ verdict, cells: table.length, scalar, aa }, null, 2));
