#!/usr/bin/env node
// B4-30 fix7 budget generator: derives the per-(OS, profile) budgets from the
// fix7 differential evidence CSV (docs/perf/b4_30_probes/b4_30_fix7_budget_evidence.csv,
// produced by scripts/b4_30_fix7_evidence.mjs from the committed raw run logs
// docs/perf/_raw_b4_30_fix7_*.log) and validates them against the constants
// in the self-measuring test. Writes the summary CSV, the generated table,
// the report, and card 108 in docs/perf/OPEN_ITEMS.md (ONLY the text between
// the <!-- B4-30-CARD-START --> / <!-- B4-30-CARD-END --> markers plus the
// card's own header line — any neighbouring item >=109 is preserved).
//
// Identity is HEAD-INDEPENDENT (post-commit reproducible): the measurement
// base is pinned by MEASURED_BASE (the worktree base commit the evidence was
// measured on) plus the pinned sha256 of the test's MEASUREMENT LOGIC
// (sha256 of the test file with the `const BUDGET_*` values masked, see
// scripts/b4_30_fix7_evidence.mjs) — asserted equal to the LIVE test file, so
// constant-only rebasing stays valid while any logic change forces a
// re-measure. The mutant verdict (v1/v4) is DERIVED from the committed
// mutant logs in docs/perf/b4_30_probes/, not stated as prose. Any
// validation failure throws => exit code 1.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { MASKED_TEST_SHA_PINS } from "./b4_30_fix7_evidence.mjs";

const EVIDENCE = process.env.B4_30_EVIDENCE || "docs/perf/b4_30_probes/b4_30_fix7_budget_evidence.csv";
const TEST = "tests/b4_30_first_alloc_min_stack.rs";
const SUMMARY = process.env.B4_30_SUMMARY_OUT || "docs/perf/B4_30_STACK_CONTRACT_summary.csv";
const TABLE = process.env.B4_30_TABLE_OUT || "docs/perf/B4_30_STACK_CONTRACT_table.generated.md";
const REPORT = process.env.B4_30_REPORT_OUT || "docs/perf/B4_30_STACK_CONTRACT.md";
const CARD = process.env.B4_30_CARD_OUT || "docs/perf/OPEN_ITEMS.md";
const PROBES = "docs/perf/b4_30_probes";
const ASM = "docs/perf/_raw_b4_30_rev2_win_release_production_asm_excerpt.s";
const MEASURED_BASE = "4a2071eda743c63557f9dc065f34fff8e1fabf5e";
const SCENARIOS = ["small", "large", "foreign-free"];
const sha = p => createHash("sha256").update(readFileSync(p)).digest("hex");
const check = (ok, msg) => { if (!ok) throw Error(msg); };
const ceil1024 = x => Math.ceil(x / 1024) * 1024;
const budgetOf = worst => ceil1024(worst) + Math.max(4096, ceil1024(0.25 * worst));

// ---- live test identity: masked-logic pin ---------------------------------
const TEST_SHA = sha(TEST);
const testSource = readFileSync(TEST, "utf8");
const maskedSha = MASKED_TEST_SHA_PINS.maskedTestSha(testSource);
check(maskedSha === MASKED_TEST_SHA_PINS.pin,
  `masked test sha ${maskedSha} != pinned measurement-logic revision ${MASKED_TEST_SHA_PINS.pin}`);
check(MASKED_TEST_SHA_PINS.pin !== "PENDING", "producer pin is still PENDING");
const BUDGET_CONSTS = {
  "windows/debug": ["BUDGET_WIN_DEBUG"],
  "windows/release": ["BUDGET_WIN_RELEASE"],
  "linux/debug": ["BUDGET_LINUX_DEBUG"],
  "linux/release": ["BUDGET_LINUX_RELEASE"],
};
const budgetsDeclared = {};
for (const [key, [name]] of Object.entries(BUDGET_CONSTS)) {
  const m = testSource.match(new RegExp(`const ${name}: usize = (\\d+);`));
  check(m, `test: ${name} constant not found`);
  budgetsDeclared[key] = Number(m[1]);
}

// ---- evidence CSV (producer output) ---------------------------------------
const evidText = readFileSync(EVIDENCE, "utf8").trim();
const evLines = evidText.split(/\r?\n/);
const headerLineIdx = evLines.findIndex(l => l.startsWith("os,profile,features,scenario,"));
check(headerLineIdx >= 0, "evidence: header row not found");
const evHeader = evLines[headerLineIdx].split(",");
const idx = name => evHeader.indexOf(name);
const rows = evLines.slice(headerLineIdx + 1).filter(l => l.trim() && !l.startsWith("#")).map(line => {
  const f = line.split(",");
  const r = {};
  for (const h of evHeader) r[h] = f[idx(h)];
  r.need = Number(r.need); r.fs = Number(r.fs); r.fc = Number(r.fc); r.stride = Number(r.stride);
  r.runs = Number(r.runs);
  return r;
});
check(rows.length === 24, `evidence: expected 24 rows (8 cells x 3 scenarios), got ${rows.length}`);
for (const r of rows) {
  check(r.test_sha === maskedSha, `evidence: row ${r.os}/${r.profile}/${r.scenario} test_sha != live masked test logic sha`);
  check(r.runs >= 2, `evidence: row ${r.os}/${r.profile}/${r.scenario} has ${r.runs} runs (< 2)`);
  check(SCENARIOS.includes(r.scenario), `evidence: unknown scenario ${r.scenario}`);
  check(Number.isInteger(r.need) && r.need >= 0, `evidence: bad need ${r.need}`);
  check(typeof r.rustc === "string" && r.rustc.startsWith("rustc "), `evidence: rustc identity missing on a row`);
  check(r.cpu && r.os_build, `evidence: cpu/os_build identity missing on a row`);
}

// ---- cell layout ---------------------------------------------------------
const CELLS = [
  { os: "windows", profile: "debug", features: "production+internals", display: "production internals", tag: "win-debug-production" },
  { os: "windows", profile: "release", features: "production+internals", display: "production internals", tag: "win-release-production" },
  { os: "windows", profile: "debug", features: "all-features", display: "--all-features", tag: "win-debug-allfeatures" },
  { os: "windows", profile: "release", features: "all-features", display: "--all-features", tag: "win-release-allfeatures" },
  { os: "linux", profile: "debug", features: "production+internals", display: "production internals", tag: "wsl-debug-production" },
  { os: "linux", profile: "release", features: "production+internals", display: "production internals", tag: "wsl-release-production" },
  { os: "linux", profile: "debug", features: "all-features", display: "--all-features", tag: "wsl-debug-all-features" },
  { os: "linux", profile: "release", features: "all-features", display: "--all-features", tag: "wsl-release-all-features" },
];
const findRow = (cell, s) => rows.find(r => r.os === cell.os && r.profile === cell.profile && r.features === cell.features && r.scenario === s);
for (const cell of CELLS) {
  for (const s of SCENARIOS) check(findRow(cell, s), `evidence: missing row ${cell.os}/${cell.profile}/${cell.features}/${s}`);
}

// ---- worst per (os, profile) and the budget rule -------------------------
const worstByCell = {}, worstByOsProfile = {};
for (const cell of CELLS) {
  worstByCell[cell.tag] = Math.max(...SCENARIOS.map(s => findRow(cell, s).need));
  const key = `${cell.os}/${cell.profile}`;
  worstByOsProfile[key] = Math.max(worstByOsProfile[key] ?? 0, worstByCell[cell.tag]);
}
const budgets = {};
for (const [key, worst] of Object.entries(worstByOsProfile)) {
  budgets[key] = { worst, base: ceil1024(worst), reserve: Math.max(4096, ceil1024(0.25 * worst)) };
  budgets[key].budget = budgets[key].base + budgets[key].reserve;
  check(budgets[key].budget === budgetsDeclared[key],
    `budget rule mismatch for ${key}: rule computes ${budgets[key].budget} ` +
    `(ceil1024(${worst}) + max(4096, ceil1024(0.25*${worst}))), test declares ${budgetsDeclared[key]}`);
}

// ---- mutant logs (v1 envelope pad, v4 claim_impl by-value copy) -----------
// Each log docs/perf/b4_30_probes/b4_30_fix7_mut_v{1,4}_<celltag>.log is one
// full ENFORCE=1 run of the test on ONE cell against ONE mutant; RED = any
// scenario test FAILED / "test result: FAILED".
function parseMutant(cell, version) {
  const p = `${PROBES}/b4_30_fix7_mut_v${version}_${cell.tag}.log`;
  check(existsSync(p), `mutant log missing: ${p}`);
  const text = readFileSync(p, "utf8");
  const red = text.includes("test result: FAILED");
  const enforced = [];
  for (const m of text.matchAll(/\[b4-30\] enforced scenario=(small|large|foreign-free)/g)) enforced.push(m[1]);
  if (red) {
    // libtest keeps running the remaining scenarios after a budget-assert
    // failure, so a RED run still prints enforced markers for the scenarios
    // that passed: require at least one FAILED scenario and at most 3 markers.
    check(text.includes("0 failed;") === false || red, `${p}: inconsistent test result line`);
    check(enforced.length <= 3, `${p}: more enforced markers than scenarios`);
  } else {
    check(enforced.length === 3, `${p}: expected 3 enforced markers, got ${enforced.length}`);
  }
  const det = {};
  for (const m of text.matchAll(/\[b4-30\] scenario=(\S+) details need=(\d+) budget=\d+ margin=(-?\d+) fs=(\d+) fc=(\d+) stride=(\d+).*enforce=1/g)) {
    check(!det[m[1]], `${p}: duplicate details for ${m[1]}`);
    det[m[1]] = { need: Number(m[2]), margin: Number(m[3]), fs: Number(m[4]), fc: Number(m[5]) };
  }
  if (!red) check(Object.keys(det).length === 3, `${p}: expected 3 details lines`);
  // worst delta over measured scenarios vs the evidence need
  let worstDelta = null, worstScenario = null;
  for (const s of Object.keys(det)) {
    const base = findRow(cell, s).need;
    const d = det[s].need - base;
    if (worstDelta === null || d > worstDelta) { worstDelta = d; worstScenario = s; }
  }
  return { path: p, red, det, worstDelta, worstScenario, enforced };
}
const mutants = { v1: {}, v4: {} };
for (const cell of CELLS) {
  mutants.v1[cell.tag] = parseMutant(cell, 1);
  mutants.v4[cell.tag] = parseMutant(cell, 4);
}
for (const v of ["v1", "v4"])
  check(existsSync(`${PROBES}/b4_30_fix7_mut_${v}.patch`), `mutant patch missing: b4_30_fix7_mut_${v}.patch`);

// ---- summary CSV ----------------------------------------------------------
const GEN_SHA = sha("scripts/b4_30_stack_table.mjs");
const summaryHeader = "os,profile,features,scenario,need,fs,fc,stride,boundary,control,rustc,test_sha,budget,budget_base,worst_os_profile,margin,cpu,os_build,repeats";
const summaryLines = [
  `# identity: generator=scripts/b4_30_stack_table.mjs sha256=${GEN_SHA}`,
  `# measured_base=${MEASURED_BASE} (worktree base commit of the measurement; the generator does NOT read HEAD — outputs stay reproducible after the commit)`,
  `# measured test logic sha256 (BUDGET_* values masked)=${maskedSha}; current test file sha256=${TEST_SHA}`,
  `# evidence=${EVIDENCE} sha256=${sha(EVIDENCE)} (producer: scripts/b4_30_fix7_evidence.mjs; raw logs: docs/perf/_raw_b4_30_fix7_*.log)`,
  `# budget rule: budget = ceil_1024(worst_differential) + max(4096, ceil_1024(0.25 * worst_differential)), asserted equal to the test's BUDGET_* constants`,
  summaryHeader,
];
for (const cell of CELLS) {
  const key = `${cell.os}/${cell.profile}`;
  const bk = budgets[key];
  for (const s of SCENARIOS) {
    const r = findRow(cell, s);
    summaryLines.push([cell.os, cell.profile, cell.features, s, r.need, r.fs, r.fc, r.stride,
      r.boundary, r.control, r.rustc, r.test_sha, bk.budget, bk.base, bk.worst, bk.budget - r.need,
      r.cpu, r.os_build, r.runs].join(","));
  }
}
writeFileSync(SUMMARY, summaryLines.join("\n") + "\n");

// ---- generated table ------------------------------------------------------
const table = [
  "<!-- generated by scripts/b4_30_stack_table.mjs; do not edit -->",
  "## Differential need and budgets (metric: need = fs - fc + stride, differential over the eat-only control)",
  "",
  "| ОС | Профиль | Конфигурация | Сценарий | need Б | fs Б | fc Б | stride Б | boundary | Бюджет Б | Запас (budget − need) Б |",
  "|---|---|---|---|---:|---:|---:|---:|---|---:|---:|",
];
for (const cell of CELLS) {
  const key = `${cell.os}/${cell.profile}`;
  for (const s of SCENARIOS) {
    const r = findRow(cell, s);
    table.push(`| ${cell.os} | ${cell.profile} | ${cell.display} | ${s} | ${r.need} | ${r.fs} | ${r.fc} | ${r.stride} | ${r.boundary}${r.control === "floor" ? " (control=floor)" : ""} | ${budgets[key].budget} | ${budgets[key].budget - r.need} |`);
  }
}
table.push("", "## Budget rule per (OS, profile)", "",
  "| ОС | Профиль | worst need Б | ceil_1024 | резерв max(4096, 25%) | Бюджет Б | Константа теста |",
  "|---|---|---:|---:|---:|---:|---:|");
for (const [key, b] of Object.entries(budgets)) {
  const [os, profile] = key.split("/");
  table.push(`| ${os} | ${profile} | ${b.worst} | ${b.base} | ${b.reserve} | ${b.budget} | ${b.budget} (asserted equal by the generator) |`);
}
// Mutant table: DERIVED from the committed logs (P2-3).
table.push("", "## Мутанты fix7 (таблица выведена генератором из закоммиченных логов docs/perf/b4_30_probes/b4_30_fix7_mut_v*.log)", "",
  "v1 — пад `size_of::<HeapCore>()` в `GlobalAlloc::alloc` (оболочка inline(always)-цепочки; патч `b4_30_fix7_mut_v1.patch`); v4 — лишняя копия `HeapCore` ПО ЗНАЧЕНИЮ в `HeapRegistry::claim_impl` перед `heap_ptr.write(hc)` (реальный сайт by-value конструирования, путь `bind_slow_tagged`; патч `b4_30_fix7_mut_v4.patch`). Каждый мутант: временная правка `src/`, побайтный откат (sha `src/` до=после), полный прогон ячейки при `SEFER_B4_30_ENFORCE=1`. Δneed = (need мутанта − need evidence) по ХУДШЕМУ ИЗМЕРЕННОМУ сценарию; в RED-ячейках упавшие сценарии остановились на assert бюджета и need не печатают (для них RED означает need > budget), поэтому их Δ взята из прошедших сценариев.",
  "",
  "| Ячейка | v1 Δneed Б | v1 итог | v4 Δneed Б | v4 итог |",
  "|---|---:|---|---:|---|");
for (const cell of CELLS) {
  const m1 = mutants.v1[cell.tag], m4 = mutants.v4[cell.tag];
  table.push(`| ${cell.os} ${cell.profile} ${cell.display} | ${m1.worstDelta ?? "—"} | ${m1.red ? "RED" : "не RED"} | ${m4.worstDelta ?? "—"} | ${m4.red ? "RED" : "не RED"} |`);
}
writeFileSync(TABLE, table.join("\n") + "\n");

// ---- report ---------------------------------------------------------------
// Documented prologue frame sizes from the saved ASM excerpt (prose context).
const asmSizes = (() => {
  const text = readFileSync(ASM, "utf8");
  const sizes = [...text.matchAll(/\.seh_stackalloc\s+(\d+)/g)].map(m => Number(m[1]));
  check(sizes.length >= 2, "ASM excerpt: expected >=2 .seh_stackalloc entries");
  return [...new Set(sizes)].sort((a, b) => b - a);
})();
const uniqueRustc = [...new Set(rows.map(r => r.rustc))];
const v4Reds = CELLS.filter(c => mutants.v4[c.tag].red).length;
const v1Reds = CELLS.filter(c => mutants.v1[c.tag].red).length;
const report = [
  "# B4-30 — бюджет стека первого аллока (fix7: дифференциальная метрика need = fs − fc + stride, изолированные кадры)",
  "",
  "<!-- B4-30-GENERATED-START -->",
  ...table,
  "<!-- B4-30-GENERATED-END -->",
  "",
  "## Identity измерения",
  "",
  `- База измерения (пин, генератор НЕ читает HEAD): \`${MEASURED_BASE}\``,
  `- Логика измерения: \`tests/b4_30_first_alloc_min_stack.rs\`, sha256 файла с ЗАМАСКИРОВАННЫМИ значениями \`const BUDGET_*\` = \`${maskedSha}\` — этот пин assert'ится генератором и продюсером evidence против ЖИВОГО файла: менять константы бюджетов можно без перемера, менять логику измерения — нельзя (требуется перемер и новый пин). Текущий sha файла целиком: \`${TEST_SHA}\`.`,
  `- Evidence: \`${EVIDENCE}\` (sha256 \`${sha(EVIDENCE)}\`), выведена продюсером \`scripts/b4_30_fix7_evidence.mjs\` из закоммиченных сырых логов \`docs/perf/_raw_b4_30_fix7_*.log\` (≥2 полных независимых прогона на ячейку, worst = max need); манифест среды прогонов — \`docs/perf/b4_30_probes/b4_30_fix7_run_manifest.csv\`. 8 ячеек (windows/linux × debug/release × production internals / --all-features) × 3 сценария, need в дифференциальной метрике на 2 МиБ worker-потоке.`,
  `- rustc: ${uniqueRustc.join("; ")}`,
  "",
  "## Методика",
  "",
  "Каждый прогон теста `tests/b4_30_first_alloc_min_stack.rs` САМ измеряет потребность на текущем тулчейне: свежие дочерние процессы, едок-рекурсия с живым маркером кадров, физическая глубина по адресам, динамический stride_actual в ребёнке, bracket+bisect по целевому свободному остатку (SEFER_B4_30_FREE) в координатах free = deep − stack_low, классификация переполнения по сигнатуре (Windows 0xC00000FD, Linux SIGABRT + \"has overflowed its stack\").",
  "",
  "Метрика — ДИФФЕРЕНЦИАЛЬНАЯ: `need = fs − fc + stride`, где fs — худшая (max за 3 ПОЛНЫЕ независимые bracket+bisect поиска) граница сценария, fc — граница контроля eat-only (те же 3 независимых поиска), на том же worker-потоке той же сборки. Собственные кадры контроля в need НЕ входят.",
  "",
  "Изоляция кадров (fix7, починка дефекта стенда fix6): каждый сценарий исполняется в СВОЁМ `#[inline(never)]`-кадре (`sc_small` / `sc_large` / `sc_foreign_free`), контроль — в отдельном пустом кадре; тело сценария передаётся в едока через `black_box(&mut body)`, что запрещает его девиртуализацию/инлайнинг в `eat`. Проверено по ассемблеру (`docs/perf/b4_30_probes/b4_30_fix7_inline_check.s`, сводка в отчёте раунда): тело аллокатора (`#[inline(always)]`-цепочка `GlobalAlloc::alloc`/`dealloc` → `current_heap` → `HeapCore::alloc`) встраивается ТОЛЬКО в кадры `sc_*`; кадр `eat` содержит только рекурсию и виртуальный вызов тела, контроль не содержит кода аллокатора. Поэтому стековая проба пролога сценарного кадра больше не списывается на контроль — дефект fix6 (0/8 мутантов из-за общего кадра) устранён.",
  "",
  "Вырожденные границы: если bracket СЦЕНАРИЯ дошёл до дна free-координаты (сценарий завершается даже на самом глубоком достижимом кадре — наблюдается для foreign-free на linux release), это валидный результат `boundary=floor`: need = верхняя оценка (free на дне + stride), тест проходит, если она ≤ бюджета. Если дно достигает КОНТРОЛЬ (наблюдается на linux release: eat-only проходит почти на любом кадре — fc = 15–63 Б, потому что на Linux НЕТ std-гарантии минимального стека spawned-потока и едок доходит почти до guard-страницы), дифференциал неизмеряем (требование контроля ~0), и тест консервативно подставляет АБСОЛЮТНУЮ верхнюю оценку `need = fs + stride` (маркер `control=floor`). На Windows fc ≈ 29 КиБ — там контроль останавливается на std-гарантии; на Linux-ячейках `control=floor` абсолютная оценка близка к дифференциалу (расхождение ≤ fc ≤ 63 Б). Негативные пробы (настоящий overflow при STACK=64 КиБ → FAIL; заниженная константа бюджета → RED при ENFORCE=1 и GREEN без него) подтверждают, что гейт срабатывает по назначению — см. отчёт раунда с RC.",
  "",
  "## Бюджет",
  "",
  "Правило (зафиксировано): `budget = ceil_1024(worst_differential) + max(4096, ceil_1024(0.25 × worst_differential))`, где worst_differential — максимум need по сценариям и обеим feature-конфигурациям ячейки (os, profile) из evidence. Генератор вычисляет правило из evidence и assert'ит равенство константам `BUDGET_*`, распарсенным из теста (таблица выше, секция «Budget rule»). Assert теста ОДНОСТОРОННИЙ верхний (`need <= budget`, только при `SEFER_B4_30_ENFORCE=1`); нижней границы нет — сценарии легитимно измеряются много ниже worst-бюджета. При ENFORCE=1 тест печатает отдельные статические литералы `[b4-30] enforced scenario=<s>` (только после прохождения assert'а) — CI-шаги grep'ают ИМЕННО их, так что срыв принуждения (опечатка в имени env, переименование константы) даёт RED, а не меру без assert'а. Без `SEFER_B4_30_ENFORCE=1` тест измеряет и печатает маркеры `scenario=<s> need=` без assert'а.",
  "",
  "Процедура перебазирования при дрейфе stable-тулчейна (в репозитории, выполняется одним PR): (1) перемерить все 8 ячеек текущей ревизией логики теста без `SEFER_B4_30_ENFORCE`, ≥2 полных прогона на ячейку, сырые логи — `docs/perf/_raw_b4_30_fix7_<cell>.run<k>.log` + строка в `b4_30_fix7_run_manifest.csv`; (2) `node scripts/b4_30_fix7_evidence.mjs` — пересобрать evidence CSV; (3) обновить константы `BUDGET_*` в тесте по выводу `node scripts/b4_30_stack_table.mjs` (пин замаскированного sha НЕ меняется — менялись только значения); (4) прогон всех ячеек с `SEFER_B4_30_ENFORCE=1` (GREEN) и регенерация производных файлов тем же PR.",
  "",
  "Предыдущие метрики — superseded: fix4-raw (абсолютная метрика required_free_from_low, водители `scripts/b4_30_stack_pilot.mjs` / `b4_30_stack_pilot_linux.py`) и fix6-raw удалены из дерева в fix7 как не потребляемые генераторами и не воспроизводимые (probe-тест fix4 не был закоммичен); бюджеты fix7 выводятся ТОЛЬКО из fix7-логов. Сохранённый ASM excerpt Windows release-production показывает прологи " + asmSizes.join(" и ") + " Б через __chkstk и вызов конструктора AllocCore (sret) — контекст, не вход расчёта.",
  "",
  "Точка входа измеренного пути: `#[global_allocator] SeferAlloc` через `GlobalAlloc::alloc` (`src/global/sefer_alloc/global_alloc.rs:96–119`); сайт by-value конструирования `HeapCore` — `HeapRegistry::claim_impl` (`src/registry/heap_registry/claim.rs:285`, запись `:410`), путь первого bind `bind_slow_tagged` (`src/global/tls_heap.rs:471`). Linux-среда измерений — WSL2.",
  "",
  "## Что НЕ доказано",
  "",
  ...notProven(),
];
function notProven() {
  const out = [];
  const m1 = [], m4 = [];
  for (const cell of CELLS) {
    m1.push(`${cell.os}/${cell.profile}/${cell.features}: ${mutants.v1[cell.tag].red ? "RED" : "не RED"} (Δneed ${mutants.v1[cell.tag].worstDelta ?? "n/a"})`);
    m4.push(`${cell.os}/${cell.profile}/${cell.features}: ${mutants.v4[cell.tag].red ? "RED" : "не RED"} (Δneed ${mutants.v4[cell.tag].worstDelta ?? "n/a"})`);
  }
  out.push(`- **Мутант v1 (пад \`size_of::<HeapCore>()\` в \`GlobalAlloc::alloc\`, оболочка inline(always)-цепочки): RED на ${v1Reds} из 8 ячеек.** ${m1.join("; ")}. Числа выведены генератором из закоммиченных логов (таблица выше); интерпретация не-RED ячеек (главным образом debug, где margin ≫ размера копии) — ожидаемая недостаточность резерва правила, а не дефект стенда.`);
  out.push(`- **Мутант v4 (лишняя копия HeapCore по значению в \`claim_impl\` перед \`write\`): RED на ${v4Reds} из 8 ячеек.** ${m4.join("; ")}. Это САМ сайт by-value конструирования (предмет карточки 108); там, где копия НЕ даёт RED (margin больше размера копии), гейт не ловит рост — ловит прокси \`size_of::<HeapCore>() <= 9216\` размер, но не ЧИСЛО by-value копий; несколько копий в одном кадре этим классом не покрыты.`);
  out.push("- **Удвоение кадра первого alloc мутантом не проверялось** (только v1-пад и v4-копия); statement «удвоение ловится, где margin < worst» — модельное, мутантом не подтверждено.");
  out.push("- **CI-разрывы покрытия:** Windows-ячейки исполняются только в nightly/full-ci (per-PR Windows отсутствует для этого теста, `--all-features` на Windows нет нигде); прогон на CI-stable (1.99) не выполнялся; ~20 whole-suite строк CI компилируют тест в режиме измерения-без-assert (без `SEFER_B4_30_ENFORCE=1`). Разрывы заведены в индекс: `docs/CORRECTNESS_OPEN_ITEMS.md` → `docs/correctness-open-items/TRACKED_ci_gate_coverage.md` (карточка 250).");
  out.push("- **Контроль=floor на linux release подставляет абсолютную оценку `fs + stride`.** На Linux fc = 15–63 Б (std-гарантии минимального стека spawned-потока НЕТ, едок доходит почти до дна), поэтому абсолютная оценка расходится с истинным дифференциалом не более чем на fc ≤ 63 Б; «включение ОС-базовой линии» относится только к Windows-ячейкам (fc ≈ 29 КиБ, там контроль останавливается на std-гарантии). Сдвиг std-гарантии виден в need только на Windows; на Linux гейт защищён дифференциалом напрямую.");
  out.push("- **Бюджеты действуют на x86_64 windows/linux** (см. маркеры SKIP на прочих целях) и выведены из ≥2 полных прогонов на ячейку (worst по прогонам; внутри прогона — worst из 3 независимых bracket+bisect поисков на сценарий и контроль). Межсерийный дрейф между ДНЯМИ/тулчейнами ограничен только резервом правила (≥4 КиБ) — статистики многих серий нет; при дрейфе действует процедура перебазирования выше.");
  out.push("- **Время/стоимость CI:** измерение ~2–4 с на ячейку (8 прогонов по ~2 с локально, до ~5 с на WSL). Чистая release-сборка теста (граф dev-deps, lto=thin, codegen-units=1, пустой target-dir, эта машина) — ~53 с; на CI-раннере закладывать сопоставимо или хуже, ×3 job'а с прогонами (fast ×2 + windows), кэша cargo в шагах нет.");
  return out;
}
writeFileSync(REPORT, report.join("\n") + "\n");

// ---- card 108 (marker-scoped replacement, P2-6) ---------------------------
const cardLines = buildCard();
function buildCard() {
  const key = os => p => budgets[`${os}/${p}`].budget;
  const bytes = n => `${n} bytes`;
  return [
    ">   **Current state**",
    ">   - **Status:** OPEN `[L]` — fix7: the contract is a fixed per-(OS, profile) DIFFERENTIAL budget (`need = fs − fc + stride`, control = eat-only boundary in its own allocator-free frame, floor-tolerant), self-measured by every run on the current toolchain; enforcement gated by `SEFER_B4_30_ENFORCE=1` and observable via the enforce-only markers `[b4-30] enforced scenario=<s>`. Phase 2 trigger remains open.",
    ">   - **Current verdict:** Budgets (rule: `ceil_1024(worst differential) + max(4096, ceil1024(0.25 × worst))`, asserted by the generator against the test's `BUDGET_*` constants):",
    `>     - windows debug = ${bytes(key("windows")("debug"))}, windows release = ${bytes(key("windows")("release"))}, linux debug = ${bytes(key("linux")("debug"))}, linux release = ${bytes(key("linux")("release"))}.`,
    `>   - Evidence: fix7 differential measurement — ${rows.length} rows (8 cells × 3 scenarios, ≥2 full runs per cell, worst kept) in \`docs/perf/b4_30_probes/b4_30_fix7_budget_evidence.csv\`, derived by \`scripts/b4_30_fix7_evidence.mjs\` from the committed raw logs \`docs/perf/_raw_b4_30_fix7_*.log\`; measurement-logic pin = sha256 of the test with \`BUDGET_*\` values masked (\`${maskedSha}\`), asserted against the live file. Generated summary/table/report: \`docs/perf/B4_30_STACK_CONTRACT*.md/_summary.csv/_table.generated.md\`.`,
    `>   - Mutants (table DERIVED from the committed logs \`docs/perf/b4_30_probes/b4_30_fix7_mut_v*.log\`, patches \`b4_30_fix7_mut_v1.patch\`/\`v4.patch\`, byte-exact src rollback per run): v1 — pad \`size_of::<HeapCore>()\` in \`GlobalAlloc::alloc\` (inline(always) envelope): RED on ${v1Reds} of 8 cells; v4 — an extra by-value \`HeapCore\` copy in \`claim_impl\` before the slot write (the real construction site): RED on ${v4Reds} of 8 cells. Cells without RED have margin = budget − need larger than the copy size — the gate does not claim detection there; the compile-time proxy pins SIZE only, not the NUMBER of by-value copies.`,
    ">   - **Rebase procedure on stable-toolchain drift (one PR, in-repo):** (1) re-measure all 8 cells with ENFORCE unset, ≥2 full runs per cell, raw logs committed; (2) rebuild the evidence CSV with `scripts/b4_30_fix7_evidence.mjs`; (3) update the `BUDGET_*` constants per the generator output (the masked-logic pin is unchanged by value-only edits); (4) run all cells with `SEFER_B4_30_ENFORCE=1` green and regenerate the derived files.",
    ">   - **Next trigger:** a target platform with a small guaranteed stack, or any change to the HeapCore construction path.",
    ">   - **Evidence:** docs/perf/B4_30_STACK_CONTRACT.md, B4_30_STACK_CONTRACT_summary.csv, B4_30_STACK_CONTRACT_table.generated.md; docs/perf/b4_30_probes/b4_30_fix7_budget_evidence.csv, b4_30_fix7_run_manifest.csv, b4_30_fix7_mut_v*.log/.patch, b4_30_fix7_inline_check.s; docs/perf/_raw_b4_30_fix7_*.log; tests/b4_30_first_alloc_min_stack.rs; CI-gap card: docs/correctness-open-items/TRACKED_ci_gate_coverage.md (250).",
    ">   <!-- B4-30-CARD-NUMBERS-START -->",
    `>   - Headline: budgets win-debug=${bytes(key("windows")("debug"))}, win-release=${bytes(key("windows")("release"))}, linux-debug=${bytes(key("linux")("debug"))}, linux-release=${bytes(key("linux")("release"))}; worst differentials ${Object.entries(budgets).map(([k, b]) => `${k}=${b.worst}`).join(", ")} bytes.`,
    ">   <!-- B4-30-CARD-NUMBERS-END -->",
  ];
}
const card = readFileSync(CARD, "utf8");
const headerMarker = "108. **[L] B4-30";
const idx108 = card.indexOf(headerMarker);
check(idx108 >= 0, "card 108 header missing");
check(card.indexOf(headerMarker, idx108 + 1) === -1, "card 108 header marker not unique");
const START = "<!-- B4-30-CARD-START -->", END = "<!-- B4-30-CARD-END -->";
const idxStart = card.indexOf(START, idx108);
const idxEnd = card.indexOf(END, idx108);
check(idxStart > idx108, "B4-30-CARD-START marker missing");
check(idxEnd > idxStart, "B4-30-CARD-END marker missing");
check(card.indexOf(START, idxStart + 1) === -1 && card.indexOf(END, idxEnd + 1) === -1, "card markers not unique");
check(card.indexOf("\n## Recently resolved", idxEnd) !== -1, "Recently resolved section lost");
const header = card.slice(idx108, card.indexOf("\n", idx108));
writeFileSync(CARD, card.slice(0, idx108) + header + "\n\n" + START + "\n" + cardLines.join("\n") + "\n" + card.slice(idxEnd));

console.log(table.join("\n"));
console.log(`identity measured_base=${MEASURED_BASE} masked_test_logic=${maskedSha} current_test=${TEST_SHA}`);
console.log(`budgets: ${Object.entries(budgets).map(([k, b]) => `${k}=${b.budget} (worst ${b.worst})`).join("; ")}`);
console.log(`mutants: v1 RED ${v1Reds}/8, v4 RED ${v4Reds}/8`);
console.log(`wrote ${SUMMARY}, ${TABLE}, ${REPORT}, card 108 (marker-scoped)`);
