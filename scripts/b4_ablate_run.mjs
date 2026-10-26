#!/usr/bin/env node
// B4-ablate — ONE-SHOT phase-2 runner (used by the orchestrator; review item 5).
// Modeled on scripts/b4_interleave_run.mjs: same run()/build mechanics, style
// and timeout handling. EVERY child command runs with a hard timeout and a
// checked exit code (non-zero or timeout → throw with the captured output).
// CARGO_TARGET_DIR must be set in the environment or the runner aborts.
//
// Does, strictly in order, for EVERY variant (including null) ON ITS OWN
// build (review items 3 and 4):
//   generation → build → judge `aa_sefer` → judge `aa_variant` → judge
//   `ab_variant` (three raw `<variant>_aa_sefer.jsonl`,
//   `<variant>_aa_variant.jsonl`, `<variant>_ab_variant.jsonl`) → the table
//   generator for THIS variant (all three raws of the SAME build — the
//   generator requires that) with `--expect-admitted`.
//   The NULL variant is the "copy == original" calibration: its ab_variant
//   MUST pass the pre-registered admission (the generator's
//   `--expect-admitted` gate, including the null-specific A/B check); on a
//   failure the runner STOPS with a non-zero exit code.
//   At the end: null is generated again (restore).
//
// Identity: source_diff_sha256 is sha256 over the EXACT bytes of `git diff`
// and is STABLE across variant generation: the generated copy lives
// OUTSIDE the repo tree (.rush/variant/), so `git diff` does not include it —
// one computation up front is
// valid for all judge runs of the phase. Same for head_sha (HEAD does not
// move) and rustc version. All identity flags are REQUIRED (a raw without
// them is invalid).
//
// Usage:
//   node scripts/b4_ablate_run.mjs --out-dir <dir> [--blocks 10000]
//        [--kernels churn16,churn64,pair16,larson] [--aa-sefer-only-null]
//        [--dry-run] [--profile release] [--features production,ablate]
//        [--seed 0xb4110001] [--order-seed 0x0b4d5eed] [--cargo cargo]
//        [--build-timeout 900] [--run-timeout 900] [--gen-timeout 120]
//        [--summary docs/perf/B4_ABLATE_CHURN_summary.csv]
//        [--no-null-gate] [--variants ab01_x,ab02_y] [--reps R]
// --reps R (J1′ v2, R >= 3): every judge mode runs in R SEPARATE processes
//            (order seed = --order-seed + i, raws <variant>_<mode>_r<i>.jsonl)
//            and the table comes from scripts/b4_ablate_rep_table.mjs
//            (median over processes, bootstrap over processes). Reason: the
//            null copy == original A/B moves between processes far beyond a
//            single process's CI (address-placement state is per process).
// --variants: csv filter over the variant list (names validated against the
//            patch catalog; unknown name → error). Default: ALL variants
//            (previous behavior). Does NOT affect the null variant — null
//            always runs.
// --dry-run is equivalent to --blocks 24. --blocks must be EVEN.
// --kernels: csv filter passed to EVERY judge run and to EVERY generator run
//            (default: churn16,churn64,pair16,larson — the pre-registered
//            ablation set; pass all 11 names for a full sweep).
// --aa-sefer-only-null (default OFF): run the aa_sefer judge mode ONLY for
//            the null variant and reuse its raw for every variant's table.
//            DRY-RUN ECONOMY ONLY: the three raws of a variant then come from
//            DIFFERENT builds (exe_sha256 identity), so the per-variant
//            generator run would fail its identity check — in this mode the
//            runner SKIPS the per-variant table generation entirely (and the
//            null gate still runs). Default OFF = full phase 2 (three raws
//            per variant, one build), as required by review item 3. Tables
//            are generated ONLY for variants with all three raws of the SAME
//            build — under --aa-sefer-only-null that is ONLY null.
// --no-null-gate (default OFF, DRY-RUN ONLY): the table for the null variant
//            is generated WITHOUT --expect-admitted, so the calibration gate
//            cannot stop the runner. On 24 blocks (dry-run) the wide CI set
//            almost surely fails admission — use this to smoke-test the
//            pipeline. PHASE 2 MUST KEEP THE GATE (default OFF). Non-null
//            variants keep --expect-admitted regardless.

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, mkdirSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

// Usage errors exit non-zero WITHOUT a stack trace.
process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

// ── pre-registered phase-2 constants (docs/perf/B4_ABLATE_CHURN.md) ─────────
const DEFAULT_BLOCKS = 10000; // final runs; dry-run = 24
const DRY_RUN_BLOCKS = 24;
const PATCHES_DIR = path.join("docs", "perf", "b4_ablate", "patches");
const NULL_VARIANT = "null";
const MAKE_VARIANT = "scripts/b4_ablate_make_variant.mjs";
const TABLE_SCRIPT = "scripts/b4_interleave_table.mjs";
const REP_TABLE_SCRIPT = "scripts/b4_ablate_rep_table.mjs";
const DEFAULT_KERNELS = "churn16,churn64,pair16,larson";
const DEFAULT_SUMMARY = path.join("docs", "perf", "B4_ABLATE_CHURN_summary.csv");

// Strict parser: explicit list of flags vs options-with-a-value.
const FLAGS = new Set(["--dry-run", "--aa-sefer-only-null", "--no-null-gate"]);
const OPTS = new Set([
  "--out-dir", "--blocks", "--profile", "--features", "--seed", "--order-seed",
  "--cargo", "--build-timeout", "--run-timeout", "--gen-timeout", "--kernels",
  "--summary", "--variants", "--reps",
]);
const argv = process.argv.slice(2);
const opt = {};
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  if (FLAGS.has(a)) {
    opt[a] = true;
  } else if (OPTS.has(a)) {
    const v = argv[++i];
    if (v === undefined || v.startsWith("--")) throw new Error(`${a} needs a value`);
    if (opt[a] !== undefined) throw new Error(`${a} given twice`);
    opt[a] = v;
  } else {
    throw new Error(`unknown argument ${a} (known flags: ${[...FLAGS, ...OPTS].join(" ")})`);
  }
}
const outDir = opt["--out-dir"];
if (!outDir) throw new Error("--out-dir <dir> is REQUIRED (raw jsonl output directory; use a path OUTSIDE the repo)");
const blocksRaw = opt["--blocks"] ?? (opt["--dry-run"] ? String(DRY_RUN_BLOCKS) : String(DEFAULT_BLOCKS));
if (!/^\d+$/.test(blocksRaw))
  throw new Error(`--blocks must be a positive integer, got ${blocksRaw}`);
if (Number(blocksRaw) % 2 !== 0)
  throw new Error(`--blocks must be EVEN (strict ABBA=BAAB=N/2 balance), got ${blocksRaw}`);
const blocks = blocksRaw;
const profile = opt["--profile"] ?? "release";
if (profile !== "release" && profile !== "bench-user")
  throw new Error(`--profile must be release|bench-user, got ${profile}`);
const features = opt["--features"] ?? "production,ablate";
const seed = opt["--seed"] ?? "0xb4110001";
const orderSeed = opt["--order-seed"] ?? "0x0b4d5eed";
const cargo = opt["--cargo"] ?? "cargo";
const kernels = opt["--kernels"] ?? DEFAULT_KERNELS;
// Sanity: validate the kernel names against the judge registry EARLY.
const KNOWN_KERNELS = new Set([
  "pair16", "pair64", "pair_rt", "bulk64", "bulk16k",
  "churn16", "churn64", "churn256", "churn1024", "vec512", "larson",
]);
{
  const names = kernels.split(",").map((s) => s.trim()).filter(Boolean);
  if (names.length === 0) throw new Error("--kernels must be a non-empty csv");
  for (const n of names) if (!KNOWN_KERNELS.has(n)) throw new Error(`--kernels: unknown kernel ${n}`);
  if (new Set(names).size !== names.length) throw new Error("--kernels: duplicate kernel");
}
const summaryPath = opt["--summary"] ?? DEFAULT_SUMMARY;
const reps = opt["--reps"] === undefined ? 0 : Number(opt["--reps"]);
if (opt["--reps"] !== undefined && (!Number.isInteger(reps) || reps < 3))
  throw new Error(`--reps must be an integer >= 3, got ${opt["--reps"]}`);
if (reps && opt["--aa-sefer-only-null"]) throw new Error("--reps and --aa-sefer-only-null are exclusive");
const aaSeferOnlyNull = opt["--aa-sefer-only-null"] === true;
const noNullGate = opt["--no-null-gate"] === true;
// Timeouts in SECONDS (positive integers), converted to ms for spawnSync.
function secondsOpt(name, def) {
  const raw = opt[name] ?? String(def);
  const v = Number(raw);
  if (!Number.isInteger(v) || !Number.isFinite(v) || v <= 0)
    throw new Error(`${name} must be a positive integer number of SECONDS, got ${raw}`);
  return v * 1000;
}
const buildTimeout = secondsOpt("--build-timeout", 900);
const runTimeout = secondsOpt("--run-timeout", 900);
const genTimeout = secondsOpt("--gen-timeout", 120); // variant generator + table generator

const targetDir = process.env.CARGO_TARGET_DIR;
if (!targetDir) throw new Error("CARGO_TARGET_DIR must be set (shared target dir, NOT in the repo)");
mkdirSync(outDir, { recursive: true });

// run(cmd, args, {timeout, label}): spawnSync with a hard timeout; throws
// with stdout+stderr on a non-zero exit, a signal or a timeout.
function run(cmd, cmdArgs, { timeout, label }) {
  const r = spawnSync(cmd, cmdArgs, { encoding: "buffer", timeout });
  const buf = Buffer.concat([r.stdout ?? Buffer.alloc(0), r.stderr ?? Buffer.alloc(0)]).toString("utf8");
  if (r.error) throw new Error(`[${label}] ${cmd} failed to start / TIMED OUT (${timeout} ms): ${r.error.message}\n${buf}`);
  if (r.status !== 0 || r.signal)
    throw new Error(`[${label}] ${cmd} ${cmdArgs.join(" ")} exited with status=${r.status} signal=${r.signal}\n${buf}`);
  return Buffer.concat([r.stdout ?? Buffer.alloc(0), r.stderr ?? Buffer.alloc(0)]);
}

// runAndEcho: like run, but ECHOES the child's captured stdout+stderr so the
// orchestrator log contains the full child output verbatim.
function runAndEcho(cmd, cmdArgs, opts) {
  console.log(`[run] ${cmd} ${cmdArgs.join(" ")}`);
  const out = run(cmd, cmdArgs, opts);
  process.stdout.write(out.toString("utf8"));
  return out;
}

// Variant list, DYNAMIC: every *.patch in the patches dir except null.patch.
const variantsFilterRaw = opt["--variants"];
let variantsFilter = null;
if (variantsFilterRaw !== undefined) {
  variantsFilter = variantsFilterRaw.split(",").map((s) => s.trim()).filter(Boolean);
  if (variantsFilter.length === 0) throw new Error("--variants must be a non-empty csv");
  if (variantsFilter.includes(NULL_VARIANT))
    throw new Error(`--variants: ${NULL_VARIANT} is implicit (always runs), do not list it`);
  if (new Set(variantsFilter).size !== variantsFilter.length)
    throw new Error("--variants: duplicate variant");
}
const variants = readdirSync(PATCHES_DIR)
  .filter((f) => f.endsWith(".patch"))
  .map((f) => f.replace(/\.patch$/, ""))
  .filter((v) => v !== NULL_VARIANT)
  .sort();
if (variants.length === 0) throw new Error(`no variant patches found in ${PATCHES_DIR}`);
if (variantsFilter) {
  const known = new Set(variants);
  for (const v of variantsFilter)
    if (!known.has(v)) throw new Error(`--variants: unknown variant ${v} (patch catalog: ${[NULL_VARIANT, ...variants].join(", ")})`);
}
const selectedVariants = variantsFilter ? variants.filter((v) => variantsFilter.includes(v)) : variants;
if (selectedVariants.length === 0) throw new Error("--variants: empty selection after filtering");
console.log(`[run] variants (${selectedVariants.length}): ${selectedVariants.join(", ")}`);

// ── identity (computed ONCE; stable across variant generation, see header) ──
const exe = (process.platform === "win32"
  ? `${targetDir}/${profile}/examples/b4_interleave_judge.exe`
  : `${targetDir}/${profile}/examples/b4_interleave_judge`);
if (!existsSync(PATCHES_DIR)) throw new Error(`patches dir not found: ${PATCHES_DIR}`);

const diffBytes = run("git", ["diff"], { timeout: 60000, label: "git-diff" });
const diffSha = createHash("sha256").update(diffBytes).digest("hex");
const headSha = run("git", ["rev-parse", "HEAD"], { timeout: 10000, label: "git-rev-parse" }).toString("utf8").trim();
if (!/^[0-9a-f]{40}$/.test(headSha))
  throw new Error(`git rev-parse HEAD returned a non-40-hex value: ${headSha}`);
const rustc = run("rustc", ["--version"], { timeout: 30000, label: "rustc-version" }).toString("utf8").trim();

// OS version: on Windows `cmd /c ver` parsed into major.minor.build; on
// non-Windows the literal "unknown" (documented choice, same as the classic
// runner).
function osVersion() {
  if (process.platform !== "win32") return "unknown";
  const out = run("cmd", ["/c", "ver"], { timeout: 15000, label: "win-ver" }).toString("utf8");
  const m2 = out.match(/Version\s+(\d+)\.(\d+)\.(\d+)/);
  if (!m2) throw new Error(`cannot parse Windows version from "cmd /c ver" output: ${JSON.stringify(out)}`);
  return `${m2[1]}.${m2[2]}.${m2[3]}`;
}
const osVersionStr = osVersion();
console.log(`[run] head_sha=${headSha}`);
console.log(`[run] source_diff_sha256=${diffSha} (stable: the variant copy lives outside the repo tree, git diff does not include it)`);
console.log(`[run] ${rustc}`);
console.log(`[run] os_version=${osVersionStr}`);

function buildJudge(label) {
  runAndEcho(cargo, ["build", "--profile", profile, "--example", "b4_interleave_judge", "--features", features],
    { timeout: buildTimeout, label });
  run("node", ["-e", `require("node:fs").accessSync(${JSON.stringify(exe)})`], { timeout: 10000, label: `check-exe-${label}` });
}

function judgeRun(mode, outPath, ord = orderSeed) {
  runAndEcho(exe,
    ["--mode", mode, "--blocks", blocks, "--seed", seed, "--order-seed", ord,
     "--kernels", kernels,
     "--out", outPath, "--head-sha", headSha,
     "--source-diff-sha", diffSha, "--features", features, "--profile", profile, "--rustc", rustc,
     "--os-version", osVersionStr],
    { timeout: runTimeout, label: `judge-${mode}-${path.basename(outPath, ".jsonl")}` });
}

// Table generation for ONE variant over the three raws OF ITS OWN BUILD;
// --expect-admitted gates BOTH A/A arms, and for the null variant also the
// ab_variant itself ("copy == original") — non-zero generator exit stops the
// runner (the run() helper throws).
function tableFor(variant, { expectAdmitted = true } = {}) {
  const csvPath = path.join(outDir, "b4_ablate", `summary_${variant}.csv`);
  mkdirSync(path.dirname(csvPath), { recursive: true });
  const args = [
    TABLE_SCRIPT,
    "--aa-sefer", path.join(outDir, `${variant}_aa_sefer.jsonl`),
    "--aa-variant", path.join(outDir, `${variant}_aa_variant.jsonl`),
    "--ab-variant", path.join(outDir, `${variant}_ab_variant.jsonl`),
    "--kernels", kernels,
    "--csv", csvPath,
    "--summary-append", summaryPath,
  ];
  // --expect-admitted is the ADMISSION FILTER + the null calibration gate:
  // required for EVERY table in phase 2; dropped only under --no-null-gate
  // (dry-run only) so the null gate cannot abort a dry run.
  if (expectAdmitted) args.push("--expect-admitted");
  runAndEcho(process.execPath, args,
    { timeout: genTimeout, label: `table-${variant}` });
  console.log(`[run] table ${variant}: ${csvPath} (+ appended to ${summaryPath})`);
}

// --reps: R processes per mode, distinct order seeds (base + i).
function judgeReps(variant, mode) {
  const base = BigInt(orderSeed);
  for (let i = 0; i < reps; i++)
    judgeRun(mode, path.join(outDir, `${variant}_${mode}_r${i}.jsonl`), "0x" + (base + BigInt(i)).toString(16));
}

function repTableFor(variant, { expectAdmitted }) {
  const csvPath = path.join(outDir, "b4_ablate", `summary_${variant}.csv`);
  const args = [REP_TABLE_SCRIPT, "--dir", outDir, "--variant", variant, "--reps", String(reps),
    "--kernels", kernels, "--csv", csvPath, "--summary-append", summaryPath];
  if (expectAdmitted) args.push("--expect-admitted");
  runAndEcho(process.execPath, args, { timeout: genTimeout, label: `rep-table-${variant}` });
}

function makeVariant(name) {
  runAndEcho(process.execPath, [MAKE_VARIANT, name], { timeout: genTimeout, label: `make-variant-${name}` });
}

// 1+2. EVERY variant (null included) on ITS OWN build: generate → build →
// aa_sefer → aa_variant → ab_variant → table (--expect-admitted). The null
// table run is the CALIBRATION GATE: any not-admitted kernel (or, for null,
// an ab_variant outside the A/A threshold) stops the runner with a non-zero
// exit code BEFORE any real variant is measured.
// Fresh summary per run: the summary is appended per variant, so a stale
// file from a previous run would duplicate variant×kernel rows.
if (existsSync(summaryPath)) rmSync(summaryPath);
try {
for (const v of [NULL_VARIANT, ...selectedVariants]) {
  console.log(`[run] === variant ${v} ===`);
  makeVariant(v);
  buildJudge(v);
  if (reps) {
    for (const mode of ["aa_sefer", "aa_variant", "ab_variant"]) judgeReps(v, mode);
    repTableFor(v, { expectAdmitted: v === NULL_VARIANT && !noNullGate });
    continue;
  }
  // --aa-sefer-only-null: aa_sefer runs ONLY for null (its raw is reused by
  // every variant's table); skipping it for non-null variants keeps the
  // dry-run judge budget at 3 + 2×N processes.
  if (!aaSeferOnlyNull || v === NULL_VARIANT)
    judgeRun("aa_sefer", path.join(outDir, `${v}_aa_sefer.jsonl`));
  judgeRun("aa_variant", path.join(outDir, `${v}_aa_variant.jsonl`));
  judgeRun("ab_variant", path.join(outDir, `${v}_ab_variant.jsonl`));
  if (v === NULL_VARIANT || !aaSeferOnlyNull) {
    // NO null between variants — the copy in .rush/variant/ participates in
    // the build, so a build after EVERY generation is MANDATORY (the
    // generator does rm+copy itself, no cleanup needed before generation).
    // --no-null-gate: null's table runs WITHOUT the admission gate (dry-run
    // only — phase 2 MUST keep the gate). Under --aa-sefer-only-null only null has all three raws of the
    // same build, so only null gets a table (see the skip branch below).
    // The admission gate is the NULL calibration ("copy == original"); a
    // non-null variant's not-admitted kernels are DATA (written to its CSV
    // and the summary as not-admitted rows), not a reason to stop the run.
    tableFor(v, { expectAdmitted: v === NULL_VARIANT && !noNullGate });
  } else {
    console.log(`[run] --aa-sefer-only-null: table generation for ${v} SKIPPED (mixed-build raws would fail the generator's exe_sha256 identity check)`);
  }
}

} finally {
  // 3. Restore null — also when a step above threw.
  makeVariant(NULL_VARIANT);
}
console.log("[run] OK");
