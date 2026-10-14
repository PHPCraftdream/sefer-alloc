// Deterministic Markdown rendering of the evidence registry for plan step 7
// (#2098/Ph7).
//
// docs/evidence/registry.csv is the source of truth, but a raw 21-column CSV
// is not reviewable in a PR diff or a browser. This script renders it (together
// with required_cells.csv) into docs/evidence/REGISTRY.md: status/phase/scope
// summaries, the required-cells cross-check table, the C1–C7+COST gate view and
// the full row table. The output is FULLY DETERMINED by the two input files —
// no timestamps, no locale-dependent sorting — so that `--check` in CI can
// verify the committed REGISTRY.md is exactly what the current CSVs produce.
// Edit registry.csv, never REGISTRY.md.
//
// Usage:
//   node scripts/generate-evidence-registry-md.mjs             # write REGISTRY.md
//   node scripts/generate-evidence-registry-md.mjs --check     # verify, exit 1 on drift
//   node scripts/generate-evidence-registry-md.mjs --out <p>   # custom output path
//
// Exit codes: 0 = written / up to date, 1 = --check drift, 2 = usage/IO error.
// Pure stdlib; read-only with respect to the repository except for the
// generated file itself.

import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, '..');
const REGISTRY = join(repoRoot, 'docs', 'evidence', 'registry.csv');
const REQUIRED = join(repoRoot, 'docs', 'evidence', 'required_cells.csv');
const OUT = join(repoRoot, 'docs', 'evidence', 'REGISTRY.md');

// ---------------------------------------------------------------- CLI ---

const argv = process.argv.slice(2);
let check = false, out = OUT;
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === '--check') check = true;
  else if (argv[i] === '--out') { out = resolve(argv[++i] ?? ''); }
  else { console.error(`unknown argument: ${argv[i]}`); process.exit(2); }
}

// ---------------------------------------------------------------- CSV ---
// Same minimal RFC4180 parser as the judge: quoted fields, "" escapes,
// CRLF/CR normalized to LF, BOM stripped.
function parseCsv(text) {
  text = text.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  const rows = [];
  let row = [], field = '', inQuotes = false;
  const endField = () => { row.push(field); field = ''; };
  const endRow = () => { endField(); rows.push(row); row = []; };
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inQuotes) {
      if (c === '"') {
        if (text[i + 1] === '"') { field += '"'; i++; } else inQuotes = false;
      } else field += c;
    } else if (c === '"' && field === '') inQuotes = true;
    else if (c === ',') endField();
    else if (c === '\n') endRow();
    else field += c;
  }
  if (field !== '' || row.length > 0) endRow();
  return rows.filter((r) => !(r.length === 1 && r[0].trim() === ''));
}

const STATUS = ['PASS', 'FAIL', 'KNOWN-DEFECT', 'KNOWN-RED', 'MODEL-LIMIT',
  'NOT_RUN', 'CFG_EXCLUDED', 'BUILD_ONLY', 'INCONCLUSIVE', 'CI_PENDING',
  'CONDITIONAL'];
const GATES = ['C1', 'C2', 'C3', 'C4', 'C5', 'C6', 'C7', 'COST'];

const registryCsv = parseCsv(readFileSync(REGISTRY, 'utf8'));
const requiredCsv = parseCsv(readFileSync(REQUIRED, 'utf8'));
const regHeader = registryCsv[0];
const reqHeader = requiredCsv[0];
if (!regHeader || !regHeader.includes('id') || !regHeader.includes('status')) {
  console.error('registry.csv: unexpected header');
  process.exit(2);
}
const rows = registryCsv.slice(1).map((cells) =>
  Object.fromEntries(regHeader.map((h, i) => [h, cells[i] ?? ''])));
const requiredRows = requiredCsv.slice(1).map((cells) =>
  Object.fromEntries(reqHeader.map((h, i) => [h, cells[i] ?? ''])));
const byId = new Map(rows.map((r) => [r.id, r]));

// ----------------------------------------------------------- helpers ---

// Escape for a markdown table cell; deterministic truncation to 97 chars
// (codepoints) with a `…` suffix.
function cell(v, truncate = false) {
  let s = String(v ?? '');
  if (truncate && [...s].length > 97) s = [...s].slice(0, 97).join('') + '…';
  return s.replace(/\|/g, '\\|').replace(/\r?\n/g, '<br>');
}
const byCodepoint = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

// ------------------------------------------------------------ render ---

const L = [];
L.push('# Evidence registry (generated)');
L.push('');
L.push('> **ВНИМАНИЕ:** этот файл сгенерирован скриптом' +
  ' `scripts/generate-evidence-registry-md.mjs` из `docs/evidence/registry.csv`' +
  ' — НЕ править руками; правки только в `registry.csv`.');
L.push('');
L.push(`Источник: sha256 registry.csv = ${createHash('sha256').update(readFileSync(REGISTRY, 'utf8').replace(/\r\n?/g, '\n')).digest('hex').slice(0, 8)}`);
L.push('');

// (a) status summary
L.push('## Сводка по статусам');
L.push('');
L.push('| Статус | Строк |');
L.push('|---|---:|');
const byStatus = Object.fromEntries(STATUS.map((s) => [s, 0]));
for (const r of rows) if (r.status in byStatus) byStatus[r.status]++;
for (const s of STATUS) L.push(`| ${s} | ${byStatus[s]} |`);
L.push(`| TOTAL | ${rows.length} |`);
L.push('');

// (b) owner_phase summary
L.push('## Сводка по owner_phase');
L.push('');
L.push('| Фаза | Всего | PASS | NOT_RUN | Прочие |');
L.push('|---|---:|---:|---:|---:|');
const byPhase = new Map();
for (const r of rows) {
  const p = byPhase.get(r.owner_phase) ?? { total: 0, pass: 0, notRun: 0 };
  p.total++;
  if (r.status === 'PASS') p.pass++;
  if (r.status === 'NOT_RUN') p.notRun++;
  byPhase.set(r.owner_phase, p);
}
for (const p of [...byPhase.keys()].sort(byCodepoint)) {
  const { total, pass, notRun } = byPhase.get(p);
  L.push(`| ${cell(p)} | ${total} | ${pass} | ${notRun} | ${total - pass - notRun} |`);
}
L.push('');

// (c) scope summary
L.push('## Сводка по scope');
L.push('');
L.push('| Scope | Строк |');
L.push('|---|---:|');
const byScope = new Map();
for (const r of rows) byScope.set(r.scope, (byScope.get(r.scope) ?? 0) + 1);
for (const s of [...byScope.keys()].sort(byCodepoint)) {
  L.push(`| ${cell(s)} | ${byScope.get(s)} |`);
}
L.push('');

// (d) required cells
L.push('## Обязательные клетки (required_cells)');
L.push('');
L.push('| cell_id | gate | class | expects | Статус в реестре | note |');
L.push('|---|---|---|---|---|---|');
for (const q of [...requiredRows].sort((a, b) => byCodepoint(a.cell_id, b.cell_id))) {
  const row = byId.get(q.cell_id);
  const status = row ? row.status : 'MISSING → NOT_RUN';
  L.push(`| ${cell(q.cell_id)} | ${cell(q.gate)} | ${cell(q.class)} | ${cell(q.expects)} | ${cell(status)} | ${cell(q.note)} |`);
}
L.push('');

// (e) gates C1–C7 + COST
L.push('## Гейты C1–C7 + COST');
L.push('');
L.push('| Gate | Клетки | Статусы |');
L.push('|---|---|---|');
for (const g of GATES) {
  const cells = requiredRows.filter((q) => q.gate === g)
    .sort((a, b) => byCodepoint(a.cell_id, b.cell_id));
  if (cells.length === 0) continue;
  const statuses = cells.map((q) => {
    const row = byId.get(q.cell_id);
    return `${q.cell_id}=${row ? row.status : 'MISSING → NOT_RUN'}`;
  });
  L.push(`| ${g} | ${cells.length} | ${cell(statuses.join(', '))} |`);
}
L.push('');

L.push('');
L.push('## Re-verification on CI');
L.push('');
L.push('Для перепроверки PR orchestrator коммитит на candidate branch временные `.github/evidence-reverify.json` (`{"rows":[...]}`; список строк можно скопировать из `docs/evidence/reverify-examples/p1-2.json`) и `.github/gate-run.json` (`{"runner":"scripts/r20_evidence_reverify_run.mjs","stages":"prepare,build,oracle","outDir":"target/r20-evidence-reverify"}`), затем добавляет метку `gate-run`. Эти конфиги относятся к candidate PR, не создаются этой задачей; `r20-gates` запускается по метке, отдельный workflow_dispatch не нужен.');
L.push('');
L.push('Runner допускает выполнение только в Linux GitHub Actions, на чистом checkout; source identity — HEAD SHA и `HEAD^{tree}`. `prepare` проверяет recipe, activation hash и source fragment без toolchain; `build` ставит nightly+Miri только при необходимости; `oracle` запускает только JSON argv через spawn без shell; `measure` только проверяет готовые результаты. PR linkage рекомендован, но dispatch не ограничен дополнительными env-переменными.');
L.push('Recipe `docs/evidence/reverify-recipes/<id>.json` строго `{id,registryActivationHash,steps,mutant}`; hash — SHA-256 точных UTF-8 activation bytes registry. Step строго `{name,env,argv,expect}`; env только MIRIFLAGS/RUSTFLAGS/CARGO_*/LOOM_* string. Команды только cargo [+nightly] test|miri, node scripts/*.mjs либо rustup. Неизвестные ключи/типы, hash mismatch, небезопасная команда/путь и mutant fragment не ровно один раз дают INVALID/nonzero; отсутствующая recipe — needs-manual с причиной.');
L.push('В `steps[].expect` допустимы только integer `exit` и массив `stdoutRegex`; runner объединяет stdout/stderr процесса, но поле схемы остаётся единым. Mutant aggregate expectation проверяет общий output mutant steps и nonzero exit; spawn/signal failure всегда отвергается.');
L.push('R5b-13 verifies producer publication: `SidecarBitmap::publish` fetch_or AcqRel becomes Relaxed and the named pin test fails. R5b-14 weakens all three owner pending.swap branches in one unique multiline fragment; because that pin only requires one AcqRel occurrence, changing one branch survives. LF-normalized matching retains the original byte buffer/SHA for restoration.');
L.push('R0-21 launches three direct Cargo release test targets (3+2+2); the two named built-in should-panic witnesses are required in their target outputs. R4c-05/06 preserve KNOWN-RED only after strict-red signatures including target source location and named plain positive test both pass.');
L.push('Raw stdout and stderr stream to per-ID receipts with byte-offset boundaries; every recipe step regex scans only the child byte range, never command/environment metadata.');
L.push('');
L.push('The selftest synthesizes registry/selector and artifact manifests; its async mutant callback verifies replacement still exists after an awaited turn before throwing, then checks byte-exact rollback. It also tests fragment counts 0/2 and receipt/status/duplicate/proposal tampering without invoking toolchains.');
L.push('');
L.push('Stage semantics: `prepare` only preflight/plans and never invokes rustc; `build` performs needed CI toolchain setup only; combined `build,oracle` continues into oracle; `measure` is standalone and verifies artifacts without rewriting them.');
L.push('');
L.push('Размер raw-log receipt: менее 200 KiB — хранить полный текст verbatim. Для 200 KiB–2 MiB сохранить в Git выборочные исходные sections с маркером `# TRUNCATED`, точной командой воспроизведения и SHA-256 полного лога; либо приложить gzip полного лога как архив, а текстовый receipt оставить для LF SHA verifier. Более 2 MiB полного лога не помещать в Git (включая сжатый вариант): хранить его как CI artifact, а в Git добавить hash manifest, точную команду, минимальный excerpt <200 KiB и summary.');

L.push('');
L.push('### Re-verification handoff rules');
L.push('');
L.push('Реестр обновляйте по ID: строки из `proposed_rows.csv` сливайте в `registry.csv`, не заменяя весь файл.');
L.push('Raw logs переносите в `docs/evidence` по трем CLAUDE.md tiers: <200 KiB verbatim; 200 KiB–2 MiB — excerpt с marker и командой либо gzip; >2 MiB — вне Git, в Git hash, команда, excerpt и summary.');
L.push('Указывайте фактически протестированные пути в `source_scope.csv`; пустой `scope_base` привязывает scope к `source_sha`. Закрепите source и receipt hashes; нормализуйте receipt в LF и пересчитайте hash после финального хранения. Для усеченного receipt хэшируйте сохраненный текст; полный лог закрепите manifest hash.');
L.push('После слияния по ID перегенерируйте REGISTRY.md и проверьте `node scripts/generate-evidence-registry-md.mjs --check`; запустите строгий verifier, например `node scripts/verify-evidence-registry.mjs --strict --baseline origin/main`.');
L.push('В infrastructure PR `.github/evidence-reverify.json` намеренно отсутствует. Для dry-run используйте supplied config или `--rows`; отсутствующая recipe остается needs-manual с причиной.');
L.push('');

// (f) full row table
L.push('## Полная таблица строк');
L.push('');
L.push('| id | status | owner_phase | entry | features | os | profile | activation | positive_marker | mutant | receipt | scope | mutant_rejected | source_sha | status_reason |');
L.push('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|');
const fullCols = ['id', 'status', 'owner_phase', 'entry', 'features', 'os',
  'profile', 'activation', 'positive_marker', 'mutant', 'receipt', 'scope', 'mutant_rejected', 'source_sha', 'status_reason'];
for (const r of [...rows].sort((a, b) => byCodepoint(a.id, b.id))) {
  const tds = fullCols.map((c) => {
    if (c === 'receipt') {
      const tok = String(r.receipt ?? '').split(';').map((t) => t.trim()).filter((t) => t !== '')[0] ?? '';
      return cell(tok, true);
    }
    return cell(r[c], c === 'status_reason' ? r.status === 'PASS' : true);
  });
  L.push(`| ${tds.join(' | ')} |`);
}
L.push('');

const md = L.join('\n') + '\n';

// ------------------------------------------------------------- write ---

if (check) {
  if (!existsSync(out)) {
    console.error(`GENERATED-MD: missing ${out}`);
    process.exit(1);
  }
  const current = readFileSync(out, 'utf8').replace(/\r\n/g, '\n');
  if (current === md) {
    console.log('GENERATED-MD: up to date');
    process.exit(0);
  }
  const a = current.split('\n'), b = md.split('\n');
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  console.error(`GENERATED-MD: DRIFT at line ${i + 1} of ${out}`);
  console.error(`  expected: ${JSON.stringify(b[i] ?? '<eof>')}`);
  console.error(`  found:    ${JSON.stringify(a[i] ?? '<eof>')}`);
  process.exit(1);
} else {
  writeFileSync(out, md);
  console.log(`GENERATED-MD: wrote ${out} (${md.split('\n').length} lines)`);
}
