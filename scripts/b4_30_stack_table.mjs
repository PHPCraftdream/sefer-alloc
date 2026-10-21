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
import {
  EVIDENCE_PATH,
  MASKED_TEST_SHA_PINS,
} from "./b4_30_fix7_evidence.mjs";

// The evidence path is the producer's own constant: importing the producer
// ALWAYS regenerates the default-path CSV, so a `B4_30_EVIDENCE` override
// could only point the generator at a file the just-ran producer did not
// write (the old override was dead/misleading and is gone).
const EVIDENCE = EVIDENCE_PATH;
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
check(MASKED_TEST_SHA_PINS.accepts(maskedSha),
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
  check(r.test_sha === MASKED_TEST_SHA_PINS.pin, `evidence: row ${r.os}/${r.profile}/${r.scenario} test_sha != live masked test logic sha`);
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
// full ENFORCE=1 run of the test on ONE cell against ONE mutant. P2-1 (fix8):
// RED means the BUDGET ASSERT fired — parsed from the assert's own panic
// message `need <N> B exceeds the declared budget <B> B (os=… profile=…
// scenario=<s>)` — not "any test result: FAILED". Every logged budget value
// is asserted against the COMMITTED `BUDGET_*` constant for the cell, with
// one honest exception: the 8 WSL mutant logs were captured before the
// linux/debug budget was rebased 84992 -> 86016, so 84992 is accepted there
// (and the verdict is RECOMPUTED against the committed 86016 — see
// `legacyBudgetBudget` and the table note). Δneed for a RED cell comes from
// the PANICKING scenario's need (mutant need − evidence need, worst of
// small/large), NOT from a passing foreign-free scenario; for non-RED cells
// it is the worst Δ over small/large details lines (foreign-free excluded).
const DELTA_SCENARIOS = ["small", "large"];
// Captured before the linux/debug rebase; verdicts recomputed vs 86016.
const LEGACY_BUDGETS = { "linux/debug": [84992] };
function parseMutant(cell, version) {
  const p = `${PROBES}/b4_30_fix7_mut_v${version}_${cell.tag}.log`;
  check(existsSync(p), `mutant log missing: ${p}`);
  const text = readFileSync(p, "utf8");
  const key = `${cell.os}/${cell.profile}`;
  const panics = [
    ...text.matchAll(
      /B4-30 first-alloc differential stack need (\d+) B exceeds the declared budget (\d+) B \(os=(\S+) profile=(\S+) scenario=(\S+)\)/g,
    ),
  ].map((m) => ({
    need: Number(m[1]),
    loggedBudget: Number(m[2]),
    os: m[3],
    profile: m[4],
    scenario: m[5],
  }));
  const failedLines = [
    ...text.matchAll(/test result: FAILED\. \d+ passed; (\d+) failed;/g),
  ].map((m) => Number(m[1]));
  const failedTotal = failedLines.reduce((a, b) => a + b, 0);
  check(
    failedTotal === panics.length && failedTotal <= 3,
    `${p}: ${failedTotal} failed tests but ${panics.length} budget-assert panics parsed`,
  );
  const red = panics.length > 0;
  const enforced = [];
  for (const m of text.matchAll(/\[b4-30\] enforced scenario=(small|large|foreign-free)/g)) enforced.push(m[1]);
  if (red) {
    // libtest keeps running the remaining scenarios after a budget-assert
    // failure, so a RED run still prints enforced markers for the scenarios
    // that passed: require at most 3 markers.
    check(enforced.length <= 3, `${p}: more enforced markers than scenarios`);
  } else {
    check(failedTotal === 0, `${p}: FAILED result line without a budget-assert panic`);
    check(enforced.length === 3, `${p}: expected 3 enforced markers, got ${enforced.length}`);
  }
  // Every budget value printed anywhere in the log (details lines and
  // assert panics) must equal the committed constant for this cell.
  const loggedBudgets = new Set();
  for (const m of text.matchAll(/budget=(\d+)/g)) loggedBudgets.add(Number(m[1]));
  let legacyBudget = false;
  for (const b of loggedBudgets) {
    if (b === budgetsDeclared[key]) continue;
    check(
      (LEGACY_BUDGETS[key] ?? []).includes(b),
      `${p}: logged budget ${b} != committed ${budgetsDeclared[key]} for ${key}`,
    );
    legacyBudget = true;
  }
  for (const pc of panics) {
    check(pc.os === cell.os && pc.profile === cell.profile,
      `${p}: budget assert for ${pc.os}/${pc.profile} in a ${key} log`);
  }
  // Verdict recomputed against the COMMITTED budget: a failed scenario is
  // RED only if its measured need exceeds the committed constant (for the
  // legacy 84992 WSL logs this is the honest 86016-based recompute).
  const redScenarios = panics
    .filter((pc) => pc.need > budgetsDeclared[key])
    .map((pc) => pc.scenario);
  check(redScenarios.length === 0 || red, `${p}: recomputed RED without assert panics`);
  const det = {};
  for (const m of text.matchAll(/\[b4-30\] scenario=(\S+) details need=(\d+) budget=\d+ margin=(-?\d+) fs=(\d+) fc=(\d+) stride=(\d+).*enforce=1/g)) {
    check(!det[m[1]], `${p}: duplicate details for ${m[1]}`);
    det[m[1]] = { need: Number(m[2]), margin: Number(m[3]), fs: Number(m[4]), fc: Number(m[5]) };
  }
  if (!red) check(Object.keys(det).length === 3, `${p}: expected 3 details lines`);
  // Δ: RED cells — worst over the PANICKING scenarios' need vs evidence;
  // non-RED cells — worst over small/large details (not foreign-free).
  let worstDelta = null, worstScenario = null, worstSource = null;
  if (red) {
    for (const pc of panics) {
      if (pc.need <= budgetsDeclared[key]) continue; // legacy-budget non-RED
      const d = pc.need - findRow(cell, pc.scenario).need;
      if (worstDelta === null || d > worstDelta) {
        worstDelta = d; worstScenario = pc.scenario; worstSource = "budget-assert panic";
      }
    }
  } else {
    for (const s of DELTA_SCENARIOS) {
      const base = findRow(cell, s).need;
      const d = det[s].need - base;
      if (worstDelta === null || d > worstDelta) { worstDelta = d; worstScenario = s; worstSource = "details (small/large worst)"; }
    }
  }
  return { path: p, red, det, worstDelta, worstScenario, worstSource, enforced, legacyBudget };
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
const summaryHeader = "os,profile,features,scenario,need,fs,fc,stride,boundary,control,rustc,test_sha,budget,budget_base,worst_os_profile,margin,cpu,os_build,runs";
const summaryLines = [
  `# identity: generator=scripts/b4_30_stack_table.mjs sha256=${GEN_SHA}`,
  `# measured_base=${MEASURED_BASE} (worktree base commit of the measurement; the generator does NOT read HEAD — outputs stay reproducible after the commit)`,
  `# measured test logic sha256 (BUDGET_* values masked)=${MASKED_TEST_SHA_PINS.pin}; live masked=${maskedSha}; current test file sha256=${TEST_SHA}`,
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
  "v1 — пад `size_of::<HeapCore>()` в `GlobalAlloc::alloc` (оболочка inline(always)-цепочки; патч `b4_30_fix7_mut_v1.patch`); v4 — лишняя копия `HeapCore` ПО ЗНАЧЕНИЮ в `HeapRegistry::claim_impl` перед `heap_ptr.write(hc)` (реальный сайт by-value конструирования, путь `bind_slow_tagged_with_config` → `claim_impl`, общий для всех ячеек; патч `b4_30_fix7_mut_v4.patch`). Каждый мутант: временная правка `src/`, побайтный откат (sha `src/` до=после), полный прогон ячейки при `SEFER_B4_30_ENFORCE=1`. Итог ячейки выводится генератором из лога: RED — ТОЛЬКО срабатывание assert'а бюджета (парсится его паника `need … B exceeds the declared budget … B (… scenario=<s>)`); каждый напечатанный в логе `budget=` сверяется с ЗАКОММИЧЕННОЙ константой `BUDGET_*` этой ячейки, вердикт пересчитывается против закоммиченного бюджета (need > budget). Δneed в RED-ячейках — из УПАВШЕГО сценария (need паники − need evidence, худший из small/large); в не-RED — худший из small/large по details (foreign-free не используется). ЧЕСТНАЯ ПОМЕТКА: 8 WSL-логов сняты при старом `BUDGET_LINUX_DEBUG = 84992` (до перебазирования на 86016); для linux/debug это допущено явно, вердикт пересчитан против 86016 (упавшие need ≥ 86208 — итоги не меняются).",
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
// P3 (fix8): the rev2 excerpt is OPTIONAL input — if the file is absent, the
// prose degrades to a placeholder instead of throwing (the excerpt is a
// rev2-era artifact, not part of the fix7 evidence chain).
const asmSizes = (() => {
  if (!existsSync(ASM)) return null;
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
  "Метрика — ДИФФЕРЕНЦИАЛЬНАЯ: `need = fs − fc + stride`, где fs — худшая (max за 3 ПОЛНЫЕ независимые bracket+bisect поиска) граница сценария, fc — граница контроля eat-only (те же 3 независимых поиска), на том же worker-потоке той же сборки. Собственные кадры контроля в need НЕ входят. Оценка `fs/fc = max` по `REPEATS = 3` независимым поискам занижает need на разброс fc между поисками (на Windows до 272 Б, на Linux 0 — детерминирован); эта погрешность поглощается гранулярностью `ceil_1024` базы бюджета.",
  "",
  "Изоляция кадров (fix7, починка дефекта стенда fix6): каждый сценарий исполняется в СВОЁМ `#[inline(never)]`-кадре (`sc_small` / `sc_large` / `sc_foreign_free`), контроль — в отдельном пустом кадре; тело сценария передаётся в едока через `black_box(&mut body)`, что запрещает его девиртуализацию/инлайнинг в `eat`. Проверено по ассемблеру (`docs/perf/b4_30_probes/b4_30_fix7_inline_check.s` — УСЕЧЁННЫЙ дамп ОДНОЙ ячейки win release production internals; полная сборка воспроизводима: `RUSTFLAGS='--emit asm' cargo test --release --test b4_30_first_alloc_min_stack --features 'production internals' --no-run`, windows x86_64, rustc 1.97.0; 4 вывода проверки, перенесённые из лога раунда: (1) `eat` содержит только саморекурсию и один косвенный вызов black_box-нутого тела; (2) цепочка входа аллокатора встраивается только в кадры `sc_*` (`alloc_via_fallback`, `bind_slow_tagged_with_config`, `refill_magazine_slow`, `exit_escape_alloc`); (3) `sc_eat_only` пуст и LLVM удалил его целиком — контроль не содержит кода аллокатора; (4) `scenario` — диспетчер с хвостовыми переходами в `sc_*`): тело аллокатора (`#[inline(always)]`-цепочка `GlobalAlloc::alloc`/`dealloc` → `current_heap` → `HeapCore::alloc`) встраивается ТОЛЬКО в кадры `sc_*`; кадр `eat` содержит только рекурсию и виртуальный вызов тела, контроль не содержит кода аллокатора. Поэтому стековая проба пролога сценарного кадра больше не списывается на контроль — дефект fix6 (0/8 мутантов из-за общего кадра) устранён. ОГОВОРКИ: asm покрывает одну ячейку (win release production internals) — для остальных изоляция держится структурно, на атрибутах `noinline`/`black_box`; в release `sc_eat_only` удалён LLVM и сценарии входят в `sc_*` хвостовым вызовом, поэтому самый глубокий кадр контроля — замыкание-диспетчер (~48 Б), и need занижен на ≤ 48 Б — постоянная, на гейт и мутанты не влияет.",
  "",
  "Вырожденные границы (ветки `floor`): если bracket СЦЕНАРИЯ дошёл до дна free-координаты, это валидный результат `boundary=floor` (need = верхняя оценка fs + stride); если дно достигает КОНТРОЛЬ, дифференциал неизмеряем и тест консервативно подставляет абсолютную верхнюю оценку `need = fs + stride` (маркер `control=floor`). ФАКТ fix7: во ВСЕХ 24 строках evidence (и во всех известных прогонах) `boundary=bisect` и `control=bisect`; linux release fc = 63 Б, fs = 159 Б — ни сценарий, ни контроль дна не достигали. Обе floor-ветки непокрыты живым прогоном: их консервативность аргументирована модельно, но НЕ продемонстрирована (см. «Что НЕ доказано»). На Windows fc ≈ 29 КиБ — контроль останавливается на std-гарантии; на Linux fc мал (std-гарантии минимального стека spawned-потока нет). Негативные пробы (логи раунда, в репозиторий не закоммичены): бюджетная проба — RED при ENFORCE=1 (assert бюджета на `tests/…:654`), GREEN без ENFORCE (margin −4640); проба STACK=16 КиБ упала НЕ на overflow/якоре, а на оракуле геометрии `tests/b4_30_first_alloc_min_stack.rs:486` (`worker stack size != 2 MiB`: left 65536, right 16384 — Windows округляет резерв потока до 64 КиБ) — громкий отказ оракула, а не доказательство срабатывания overflow-ветки; STACK=64 КиБ — GREEN. Путь `Step::Overflow` при штатном STACK=2 МиБ исполняется в каждом прогоне (каждый bracket кончается overflow-ребёнком), но отдельная негативная проба «настоящий overflow → FAIL» на 64 КиБ НЕ воспроизведена.",
  "",
  "## Бюджет",
  "",
  "Правило (зафиксировано): `budget = ceil_1024(worst_differential) + max(4096, ceil_1024(0.25 × worst_differential))`, где worst_differential — максимум need по сценариям и обеим feature-конфигурациям ячейки (os, profile) из evidence. Генератор вычисляет правило из evidence и assert'ит равенство константам `BUDGET_*`, распарсенным из теста (таблица выше, секция «Budget rule»). Assert теста ОДНОСТОРОННИЙ верхний (`need <= budget`, только при `SEFER_B4_30_ENFORCE=1`); нижней границы нет — сценарии легитимно измеряются много ниже worst-бюджета. При ENFORCE=1 тест печатает отдельные статические литералы `[b4-30] enforced scenario=<s>` (только после прохождения assert'а) — CI-шаги grep'ают ИМЕННО их, так что срыв принуждения (опечатка в имени env, переименование константы) даёт RED, а не меру без assert'а. Без `SEFER_B4_30_ENFORCE=1` тест измеряет и печатает маркеры `scenario=<s> need=` без assert'а.",
  "",
  "Процедура перебазирования при дрейфе stable-тулчейна (в репозитории, выполняется одним PR): (1) перемерить все 8 ячеек текущей ревизией логики теста без `SEFER_B4_30_ENFORCE`, ≥2 полных прогона на ячейку, сырые логи — `docs/perf/_raw_b4_30_fix7_<cell>.run<k>.log` + строка в `b4_30_fix7_run_manifest.csv`; (2) `node scripts/b4_30_fix7_evidence.mjs` — пересобрать evidence CSV; (3) обновить константы `BUDGET_*` в тесте по выводу `node scripts/b4_30_stack_table.mjs` (пин замаскированного sha НЕ меняется — менялись только значения); (4) прогон всех ячеек с `SEFER_B4_30_ENFORCE=1` (GREEN) и регенерация производных файлов тем же PR.",
  "",
  "Предыдущие метрики — superseded: fix4-raw (абсолютная метрика required_free_from_low, водители `scripts/b4_30_stack_pilot.mjs` / `b4_30_stack_pilot_linux.py`) и fix6-raw удалены из дерева в fix7 как не потребляемые генераторами и не воспроизводимые (probe-тест fix4 не был закоммичен); бюджеты fix7 выводятся ТОЛЬКО из fix7-логов. " + (asmSizes ? `Сохранённый ASM excerpt Windows release-production показывает прологи ${asmSizes.join(" и ")} Б через __chkstk и вызов конструктора AllocCore (sret) — контекст, не вход расчёта.` : "(asm excerpt not present)"),
  "",
  "Точка входа измеренного пути: `#[global_allocator] SeferAlloc` через `GlobalAlloc::alloc` (`src/global/sefer_alloc/global_alloc.rs:96–119`); сайт by-value конструирования `HeapCore` — `HeapRegistry::claim_impl` (`src/registry/heap_registry/claim.rs:285`, запись `:410`), путь первого bind `bind_slow_tagged_with_config` (`src/global/tls_heap.rs:489`, при alloc-decommit; виден в asm; `claim_impl` общий для всех конфигураций, поэтому вывод v4 от выбора ветки bind не зависит). Linux-среда измерений — WSL2.",
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
  out.push(`- **Мутант v1 (пад \`size_of::<HeapCore>()\` в \`GlobalAlloc::alloc\`, оболочка inline(always)-цепочки): RED на ${v1Reds} из 8 ячеек.** ${m1.join("; ")}. Числа выведены генератором из закоммиченных логов (таблица выше); все 4 не-RED ячейки v1 — debug (margin там ≫ размера пада): ожидаемая недостаточность резерва правила, а не дефект стенда. Вывод вердикта: RED = именно assert бюджета (генератор парсит его панику), каждый \`budget=\` лога сверен с закоммиченной константой \`BUDGET_*\`, Δ RED-ячеек взята из УПАВШЕГО сценария.`);
  out.push(`- **Мутант v4 (лишняя копия HeapCore по значению в \`claim_impl\` перед \`write\`): RED на ${v4Reds} из 8 ячеек.** ${m4.join("; ")}. Это САМ сайт by-value конструирования (предмет карточки 108); там, где копия НЕ даёт RED (margin больше размера копии), гейт не ловит рост — ловит прокси \`size_of::<HeapCore>() <= 9216\` размер, но не ЧИСЛО by-value копий; несколько копий в одном кадре этим классом не покрыты. Два debug-RED v4 ПОГРАНИЧНЫ: win debug all-features +256/+272 Б при разбросе debug на Windows до 448 Б — не гарантированно воспроизводимы; linux debug all-features +192 Б против закоммиченных 86016 — детерминирован. ЧЕСТНАЯ ПОМЕТКА: WSL-мутантные логи (8 шт.) сняты при старом \`BUDGET_LINUX_DEBUG = 84992\` (до перебазирования); вердикты пересчитаны генератором против закоммиченных 86016.`);
  out.push("- **ASM-проверка изоляции кадров покрывает ОДНУ ячейку** (win release production internals, `docs/perf/b4_30_probes/b4_30_fix7_inline_check.s`, усечён до цитируемых функций `eat`, диспетчера, `sc_*`); в release `sc_eat_only` удалён LLVM — самый глубокий кадр контроля, замыкание-диспетчер, занижает need на ≤ 48 Б (постоянная, гейт и мутанты не затрагивает); для остальных 7 ячеек изоляция держится структурно, на `#[inline(never)]`/`black_box`.");
  out.push("- **Удвоение кадра первого alloc мутантом не проверялось** (только v1-пад и v4-копия); statement «удвоение ловится, где margin < worst» — модельное, мутантом не подтверждено.");
  out.push("- **CI-разрывы покрытия:** Windows-ячейки исполняются только в nightly/full-ci (per-PR Windows отсутствует для этого теста, `--all-features` на Windows нет нигде); прогон на CI-stable (1.99) не выполнялся; ~20 whole-suite строк CI компилируют тест в режиме измерения-без-assert (без `SEFER_B4_30_ENFORCE=1`). Разрывы заведены в индекс: `docs/CORRECTNESS_OPEN_ITEMS.md` → `docs/correctness-open-items/TRACKED_ci_gate_coverage.md` (карточка 253). До fix8 masked-пин логики измерения не принуждался ни одним CI-гейтом; с fix8 его проверяет cargo-free `scripts/verify-b4-30-pin.mjs` (шаг в job `docs-consistency`).");
  out.push("- **Обе ветки floor (сценарий и контроль) в fix7 НЕ наблюдались:** во всех 24 строках evidence `boundary=bisect` и `control=bisect` (linux release fc = 63 Б, fs = 159 Б). Их консервативность — модельная аргументация, не продемонстрированная живым прогоном; прежняя формулировка «control=floor наблюдается на linux release» (закрытый P3 fix6) была ЛОЖНОЙ для fix7 — переоткрыта по факту и закрыта этой правкой. На Linux fc мал (std-гарантии минимального стека spawned-потока нет), на Windows fc ≈ 29 КиБ — контроль останавливается на std-гарантии; сдвиг std-гарантии виден в need только на Windows, на Linux гейт защищён дифференциалом напрямую.");
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
    "   > **Current state**",
    "   > - **Status:** OPEN `[L]` — fix7: the contract is a fixed per-(OS, profile) DIFFERENTIAL budget (`need = fs − fc + stride`, control = eat-only boundary in its own allocator-free frame, floor-tolerant), self-measured by every run on the current toolchain; enforcement gated by `SEFER_B4_30_ENFORCE=1` and observable via the enforce-only markers `[b4-30] enforced scenario=<s>`. Phase 2 trigger remains open.",
    "   > - **Current verdict:** Budgets (rule: `ceil_1024(worst differential) + max(4096, ceil1024(0.25 × worst))`, asserted by the generator against the test's `BUDGET_*` constants):",
    `   >   - windows debug = ${bytes(key("windows")("debug"))}, windows release = ${bytes(key("windows")("release"))}, linux debug = ${bytes(key("linux")("debug"))}, linux release = ${bytes(key("linux")("release"))}.`,
    `   > - Mutants (table DERIVED from the committed logs \`docs/perf/b4_30_probes/b4_30_fix7_mut_v*.log\`, patches \`b4_30_fix7_mut_v1.patch\`/\`v4.patch\`, byte-exact src rollback per run): v1 — pad \`size_of::<HeapCore>()\` in \`GlobalAlloc::alloc\` (inline(always) envelope): RED on ${v1Reds} of 8 cells; v4 — an extra by-value \`HeapCore\` copy in \`claim_impl\` before the slot write (the real construction site): RED on ${v4Reds} of 8 cells. Cells without RED have margin = budget − need larger than the copy size — the gate does not claim detection there; the compile-time proxy pins SIZE only, not the NUMBER of by-value copies (proxy margin is 8 bytes: HeapCore 9208 vs threshold 9216 — any growth >= 16 bytes reds the r34_18 doc check in someone else's PR; until fix8 the measurement-logic pin was enforced by no CI gate, now by scripts/verify-b4-30-pin.mjs in docs-consistency).`,
    "   > - **Rebase procedure on stable-toolchain drift (one PR, in-repo):** (1) re-measure all 8 cells with ENFORCE unset, ≥2 full runs per cell, raw logs committed; (2) rebuild the evidence CSV with `scripts/b4_30_fix7_evidence.mjs`; (3) update the `BUDGET_*` constants per the generator output (the masked-logic pin is unchanged by value-only edits); (4) run all cells with `SEFER_B4_30_ENFORCE=1` green and regenerate the derived files.",
    "   > - **Next trigger:** a target platform with a small guaranteed stack, or any change to the HeapCore construction path.",
    "   > - **Evidence:** docs/perf/B4_30_STACK_CONTRACT.md, B4_30_STACK_CONTRACT_summary.csv, B4_30_STACK_CONTRACT_table.generated.md; docs/perf/b4_30_probes/b4_30_fix7_budget_evidence.csv, b4_30_fix7_run_manifest.csv, b4_30_fix7_mut_v*.log/.patch, b4_30_fix7_inline_check.s; docs/perf/_raw_b4_30_fix7_*.log; tests/b4_30_first_alloc_min_stack.rs; CI-gap card: docs/correctness-open-items/TRACKED_ci_gate_coverage.md (253).",
    "   > <!-- B4-30-CARD-NUMBERS-START -->",
    `   > - Headline: budgets win-debug=${bytes(key("windows")("debug"))}, win-release=${bytes(key("windows")("release"))}, linux-debug=${bytes(key("linux")("debug"))}, linux-release=${bytes(key("linux")("release"))}; worst differentials ${Object.entries(budgets).map(([k, b]) => `${k}=${b.worst}`).join(", ")} bytes.`,
    "   > <!-- B4-30-CARD-NUMBERS-END -->",
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
