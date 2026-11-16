# G1 decision report — STOP at A1

**Decision (2026-11-16): STOP.** The available inputs do not establish a clear, incremental benefit from P2 over the A1 layering lint. Stop at A1; do not start P2. This applies the pre-written default in `docs/perf/G1_DECISION_CRITERION.md`, “Default rule,” and plan §“G1 — decision on P2.” It is a decision on the evidence presently available, not a claim that P2 can never be beneficial.

## Scope and implementation disposition

- P2.0–P2.5 are **not started**. P1.3, P1.4 and P1.6 are **NOT MERGING**; branch `p14-sefer-platform-20261115` remains a record only, with no new PR. P1.5’s two-level selector will not proceed under this STOP. The parked PR #249 disclosure is historical evidence, not a new measurement. These dispositions match the plan’s §4 G1 decision and §5 statuses.
- Already-landed P1.1 and P1.2 remain unchanged. A1 remains the **sole layering enforcement** for this plan decision; this does not claim that A1 replaces or exhausts the project’s other safety/correctness mechanisms.
- No gate policy is changed. `docs/perf/G1_DECISION_CRITERION.md`, “Default rule,” cites `scripts/ghost_diff.mjs` BUILD_GRAPH at line 30 and the fail-closed returns at lines 49–50: any changed non-root manifest, including a `crates/**/Cargo.toml`, requires the proof. Only the root `Cargo.toml` has the narrow production-composition/comment exception (criterion lines 61–68); a new crate manifest does not. A future crate split therefore still requires the gate; passing requires production `.text` identity or a separate, explicit owner decision to change that policy. This is a cost/constraint, not evidence that identity is impossible.
- No CI run was dispatched or awaited. The P0.2 pilot’s ordinary-CI duration criterion remains pending. No actual P1.5 or P2 wall-clock time was measured.

## G1 inputs and directions

Evidence labels: **OBSERVED** means directly reported by the current log, generator, or lint checks; **INFERRED** means the default-rule decision and cost/benefit assessment, not a measured outcome.

### Input 1 — CI duration and selected-test counts

**Direction:** representative peripheral changes should select fewer tests, with measurable ordinary-CI time reduction; a P0.2 binary-count reduction is a separate lever, not incremental P2 benefit.

The existing selection baseline is a segment-directory scan selecting **369/484 tests**; the first map run recorded **374/484** mapped under `--all-features` (`docs/CI_TIERS.md`, “Test selection,” and `docs/perf/G1_DECISION_CRITERION.md`, Input 1; these are current-selection source values, not after-P1.5 measurements). The P1.5 history model analyzes **120** first-parent merge commits (**Window**); **12** are eligible and all twelve are docs-only, with **0 seconds** modeled savings under both PR and push policy and **0/12** eligible commits positive (“Aggregates,” “Per-commit table”; `docs/perf/P1_5_SELECTION_POTENTIAL.md`). This is modeled time, not actual post-P1.5 CI time. The remaining **105** are `ineligible-paths` and **3** are `p15-not-subset` (“Non-eligible commits”). The three flagged models say P1.5 would add `crate_aligned_vmem,crate_numa_shim,os`, `crate_numa_shim,os`, and `crate_proc,os`; each is explicitly routed to `p15-not-subset` with savings **0** (“Flags raised by the model”). They are not credited as P1.5 savings. The model’s disclosed limitations (“Honest limitations”) include one representative job-weight run applied to every commit, an upper-bound/perfect-selection assumption that ignores the root job still needed for dependent changes, manifest approximation and conservative dependency fallbacks, exclusion of the always group, and no queue/wait time. Thus the measured sample establishes zero modeled savings for its eligible docs-only window; it does not measure actual P1.5 or P2 time or establish that no source-affecting history could show benefit.

P0.2 is not a G1 verdict. The historical criterion quotes **35 of 78** targets merged and **34 fewer binaries** (`docs/perf/G1_DECISION_CRITERION.md`, Input 1; see also plan §1, revision 3 fact 5). That is the earlier pool, before P1.2. The current pilot report’s “Merged group / Final composition” gives **32 merged modules / 43 of 75 not merged**; its “Binaries per full-target row” gives **32 flat targets → 1, saved 31** (`docs/perf/P0_2_TEST_MERGE_PILOT.md`). Its “Test inventory” reports **1,515 tests before and after**, zero lost/added/duplicated tests and zero cfg mismatches. The separate ordinary-CI duration condition requires at least three runs per side, a median reduction of at least **5% in every affected test job**, and no growth in any affected job; “Measurements” remains explicitly pending (`docs/perf/P0_2_TEST_MERGE_PILOT.md`, “Decision criterion” (b), “Measurements”). Therefore the P0.2 GO/NO-GO verdict is **PENDING**, not inferred from the module count. The P0.2 verdict will be recorded separately; it cannot create an incremental P2 benefit, whether it is GO or NO-GO. P0.2 is an independent test-target lever; it does not show a P2 benefit incremental to P0.

### Input 2 — no-LTO Ir

**Direction:** the criterion requires per-bench **delta zero after P1.6’s inline audit**; the CI job’s `Ir=10` threshold is a different gate threshold, not that criterion.

The derived comparison below reads the raw `Instructions: head|base` columns, verifies each embedded base against the canonical P1.0 CSV, and compares equal bench sets via `parseIaiLog`, `parseCsv`, `compareRuns`, `buildCompareCsv` and `buildMarkdownSection` in the existing `scripts/iai-nolto-summary.mjs`. The input is canonical baseline commit `90d8793a52a2cc4f5712b1259c13cb6ce1e41f48` versus head `ded4029003a9e3805b7ecba3e4f20dd9fb3c1d55`, run `43795691348`, job `133063872469`, features `production bench-internals internals`, no-LTO / codegen-units-16 profile, Ubuntu 26.04.1 LTS, runner image `ubuntu-26.04 20261111.196.1`; CPU is unknown from the job log. These are the logged features and runner identity, **not** a claim of canonical plain-production binary identity. A single run also does not assign the observed change to any one extraction or establish causality.

The generated comparison has **105 equal Ir rows, 1 increased and 0 decreased** among **106** benches; aggregate Ir is **58,092,915 → 58,092,980, delta +65**. `realloc_grow` is **549,348 → 549,413 Ir**, i.e. **+65 / 549,348 = +0.01%** under the summary’s two-decimal percentage rule. The criterion’s post-P1.6 delta-zero condition is **not met on the available evidence**: the post-inline run remains unmeasured, so zero is not demonstrated. This current result does not establish that P1.2 alone, or any particular change, caused the +65.

The criterion’s pre-written Disclosure separately records the parked PR #249 stack: **76/106** rows grew; `decomp_os_roundtrip_8x` moved **1,740 → 1,996 Ir**, **+256 / 1,740 = +14.7%** (`docs/perf/G1_DECISION_CRITERION.md`, “Disclosure”). That is the criterion’s disclosure of data seen before its thresholds were written, not new raw proof and not the present run’s result. Its exact post-P1.6 status remains UNMEASURED (criterion Input 2). The CI no-LTO row’s `IAI_CALLGRIND_REGRESSION: Ir=10` threshold is documented in `docs/perf/IAI_NOLTO_BASELINE.md`, “How the CI row is built”; it must not be substituted for the criterion’s delta-zero expectation. The criterion’s Disclosure also records ghost-zero-cost red for the parked P1.3/P1.4 production `.text` changes; post-P1.6 Ir remains UNMEASURED. These are historical disclosed facts, not newly rerun results.

#### Reproduction and generated comparison

Run from the repository root with Node. This is a cargo-free one-off; it writes only `docs/perf/G1_DECISION_REPORT_summary.csv`, leaves the gzip raw log untouched, asserts the bench set, embedded baseline columns, headers, totals, ratios/counts and A1 baseline, then prints the headline and generated table. It deliberately reuses the existing parser/builders; no new parser, committed script or CI registration is needed.

Acquire the complete Actions job log with `gh run view 43795691348 --log --job 133063872469`. Store the full, untruncated log gzip-compressed as the cited raw evidence per `CLAUDE.md`; do not truncate it.

```sh
node --input-type=module <<'NODE'
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { gunzipSync } from 'node:zlib';
import { pathToFileURL } from 'node:url';
const root = process.cwd();
const { buildCompareCsv, buildMarkdownSection, compareRuns, parseCsv, parseIaiLog } =
  await import(pathToFileURL(resolve(root, 'scripts/iai-nolto-summary.mjs')).href);
const baselinePath = resolve(root, 'docs/perf/IAI_NOLTO_BASELINE_summary.csv');
const rawPath = resolve(root, 'docs/perf/_raw_g1_iai_nolto_main_ded40290.log.gz');
const outputPath = resolve(root, 'docs/perf/G1_DECISION_REPORT_summary.csv');
const baseCommit = '90d8793a52a2cc4f5712b1259c13cb6ce1e41f48';
const headCommit = 'ded4029003a9e3805b7ecba3e4f20dd9fb3c1d55';
const runId = '43795691348';
const jobId = '133063872469';
const baselineCsv = readFileSync(baselinePath, 'utf8');
const rawBytes = readFileSync(rawPath);
const rawText = gunzipSync(rawBytes).toString('utf8');
const canonical = parseCsv(baselineCsv);
const expectedHeader = ['bench', 'ir', 'l1', 'l2', 'ram', 'cycles', 'commit', 'features', 'profile', 'note'];
assert.deepEqual(canonical[0], expectedHeader, 'canonical P1.0 baseline header');
const baseRows = canonical.slice(1).map((cells, i) => {
  assert.equal(cells.length, expectedHeader.length, `canonical baseline row ${i + 2} field count`);
  assert.equal(cells[6], baseCommit, `canonical baseline row ${i + 2} commit`);
  return {
    name: cells[0], ir: Number(cells[1]), l1: Number(cells[2]), l2: Number(cells[3]),
    ram: Number(cells[4]), cycles: Number(cells[5]),
  };
});
assert.equal(new Set(baseRows.map(row => row.name)).size, baseRows.length, 'canonical bench names unique');
assert.equal(new Set(canonical.slice(1).map(cells => cells[7])).size, 1, 'canonical feature identity consistent');
assert.equal(new Set(canonical.slice(1).map(cells => cells[8])).size, 1, 'canonical profile identity consistent');

const headRows = parseIaiLog(rawText);
assert.equal(headRows.length, baseRows.length, 'head and canonical baseline bench counts');
const comparison = compareRuns(baseRows, headRows);
const baseByName = new Map(baseRows.map(row => [row.name, row]));

// Independently check the raw Instructions: head|base embedded baseline column.
const ansi = /\x1b\[[0-9;:?]*[A-Za-z]/g;
const timestamp = /^\d{4}-\d{2}-\d{2}T[\d:.]+Z?\s+/;
const headerPattern = /^([A-Za-z_][\w]*::)+([A-Za-z_]\w*)\b/;
const embedded = new Map();
let activeBench = null;
for (const original of rawText.split(/\r?\n/)) {
  let line = original.replace(ansi, '');
  while (/^[^\t]*\t/.test(line)) line = line.slice(line.indexOf('\t') + 1);
  line = line.replace(timestamp, '');
  const header = headerPattern.exec(line);
  if (header) {
    activeBench = header[2];
    assert(!embedded.has(activeBench), `raw log duplicate bench header: ${activeBench}`);
    continue;
  }
  const metric = /^\s*Instructions:\s*([\d,]+)\|([\d,]+|N\/A)/.exec(line);
  if (metric && activeBench) {
    assert.notEqual(metric[2], 'N/A', `${activeBench}: embedded Instructions baseline missing`);
    const pair = {
      head: Number(metric[1].replace(/,/g, '')),
      base: Number(metric[2].replace(/,/g, '')),
    };
    assert.equal(pair.head, headRows.find(row => row.name === activeBench)?.ir, `${activeBench}: raw head matches parseIaiLog`);
    assert.equal(pair.base, baseByName.get(activeBench)?.ir, `${activeBench}: embedded base matches canonical P1.0 CSV`);
    embedded.set(activeBench, pair);
  }
}
assert.deepEqual(new Set(embedded.keys()), new Set(baseRows.map(row => row.name)), 'all benches have embedded head|base Instructions');
assert.equal(embedded.size, baseRows.length, 'one embedded Instructions pair per bench');

const headers = [
  'bench', 'ir_base', 'ir_head', 'delta_ir', 'delta_pct',
  'l1_base', 'l2_base', 'ram_base', 'cycles_base',
  'l1_head', 'l2_head', 'ram_head', 'cycles_head',
  'commit_base', 'commit_head', 'features', 'profile', 'note',
];
assert.equal(new Set(headers).size, headers.length, 'summary CSV headers unique');
const rawSha256 = createHash('sha256').update(rawBytes).digest('hex');
const features = canonical[1][7];
const profile = canonical[1][8];
const os = 'Ubuntu 26.04.1 LTS; runner image ubuntu-26.04 20261111.196.1';
const cpu = 'unknown (not present in Actions job log)';
const note = `run ${runId}/job ${jobId}; event workflow_dispatch; sample_count=${baseRows.length}; OS=${os}; CPU=${cpu}; raw_gzip_sha256=${rawSha256}`;
const meta = { baseCommit, commit: headCommit, features, profile, note };
const csv = buildCompareCsv(comparison, meta);
const parsedCsv = parseCsv(csv);
assert.deepEqual(parsedCsv[0], headers, 'generated summary header');
assert.equal(new Set(parsedCsv[0]).size, parsedCsv[0].length, 'generated summary headers unique');
assert.equal(parsedCsv.length, baseRows.length + 2, 'generated CSV contains header, every bench and TOTAL');
writeFileSync(outputPath, csv);

const equal = comparison.rows.filter(row => row.deltaIr === 0).length;
const increased = comparison.rows.filter(row => row.deltaIr > 0).length;
const decreased = comparison.rows.filter(row => row.deltaIr < 0).length;
assert.equal(equal + increased + decreased, comparison.rows.length, 'derived Ir counts partition bench set');
assert.equal(equal, 105, 'derived unchanged Ir rows');
assert.equal(increased, 1, 'derived increased Ir rows');
assert.equal(decreased, 0, 'derived decreased Ir rows');
const realloc = comparison.rows.find(row => row.name === 'realloc_grow');
assert.ok(realloc, 'realloc_grow bench exists');
assert.equal(realloc.irBase, 549348);
assert.equal(realloc.irHead, 549413);
assert.equal(realloc.deltaIr, realloc.irHead - realloc.irBase);
assert.equal(realloc.deltaPct, Math.round((realloc.deltaIr / realloc.irBase) * 10000) / 100);
const { sumBase, sumHead, sumDelta } = comparison.totals;
assert.equal(sumDelta, comparison.rows.reduce((sum, row) => sum + row.deltaIr, 0));
assert.equal(sumHead - sumBase, sumDelta);
assert.deepEqual([sumBase, sumHead, sumDelta], [58092915, 58092980, 65]);
const totalDeltaPct = Math.round((sumDelta / sumBase) * 10000) / 100;
assert.equal(totalDeltaPct, Math.round(((sumHead - sumBase) / sumBase) * 10000) / 100);
assert.equal(totalDeltaPct, 0, 'aggregate percent uses the summary CSV two-decimal rounding rule');
const increasedShare = Math.round((increased / comparison.rows.length) * 10000) / 100;
assert.equal(increasedShare, 0.94, 'share of benches with increased Ir');
assert.equal(realloc.deltaPct, 0.01, 'realloc_grow percent uses the summary CSV two-decimal rounding rule');

const layerRows = parseCsv(readFileSync(resolve(root, 'docs/layering_baseline.csv'), 'utf8'))
  .filter(row => row[0] === 'backedge');
const backedgeRefs = layerRows.reduce((sum, row) => sum + Number(row[2]), 0);
assert.equal(layerRows.length, 12, 'A1 baseline back-edge count');
assert.equal(backedgeRefs, 48, 'A1 baseline back-edge reference count');

console.log(`G1 no-LTO derived: ${comparison.rows.length} benches; ${equal} equal, ${increased} increased, ${decreased} decreased; aggregate Ir ${sumBase} -> ${sumHead} (delta ${sumDelta}); realloc_grow ${realloc.irBase} -> ${realloc.irHead} (delta ${realloc.deltaIr}, ${realloc.deltaPct}%); ${rawSha256}`);
console.log(`A1 baseline derived: ${layerRows.length} back-edges / ${backedgeRefs} refs.`);
console.log(buildMarkdownSection({ compare: comparison, meta, title: 'G1 no-LTO comparison (P1.0 baseline vs main)' }));
NODE
```

**Command output, pasted verbatim (including the generated comparison block):**

G1 no-LTO derived: 106 benches; 105 equal, 1 increased, 0 decreased; aggregate Ir 58092915 -> 58092980 (delta 65); realloc_grow 549348 -> 549413 (delta 65, 0.01%); 526131eb1d19cc16a1dae565840ae5007ba05fc08428c100448585abaabb46b7
A1 baseline derived: 12 back-edges / 48 refs.
```
### G1 no-LTO comparison (P1.0 baseline vs main)

- commit: ded4029003a9e3805b7ecba3e4f20dd9fb3c1d55
- base commit: 90d8793a52a2cc4f5712b1259c13cb6ce1e41f48
- features: production bench-internals internals
- profile: bench profile + CARGO_PROFILE_BENCH_LTO=false + CARGO_PROFILE_BENCH_CODEGEN_UNITS=16 (cargo env overrides of [profile.bench]; user-default codegen)
- note: run 43795691348/job 133063872469; event workflow_dispatch; sample_count=106; OS=Ubuntu 26.04.1 LTS; runner image ubuntu-26.04 20261111.196.1; CPU=unknown (not present in Actions job log); raw_gzip_sha256=526131eb1d19cc16a1dae565840ae5007ba05fc08428c100448585abaabb46b7

| bench | Ir base | Ir head | Δ Ir | Δ % |
| --- | --- | --- | --- | --- |
| small_churn_16b | 53,313 | 53,313 | 0 | 0% |
| small_churn_16b_2n | 58,113 | 58,113 | 0 | 0% |
| dealloc_prealloc_only_16b | 53,605 | 53,605 | 0 | 0% |
| dealloc_free_only_16b | 55,968 | 55,968 | 0 | 0% |
| dealloc_contains_base_probe_only_16b | 54,723 | 54,723 | 0 | 0% |
| dealloc_segment_base_of_ptr_probe_only_16b | 54,159 | 54,159 | 0 | 0% |
| alloc_magazine_prefill_only_16b | 52,403 | 52,403 | 0 | 0% |
| alloc_magazine_hit_only_16b | 53,027 | 53,027 | 0 | 0% |
| alloc_zeroed_magazine_prefill_only_16b | 52,427 | 52,427 | 0 | 0% |
| alloc_zeroed_magazine_hit_only_16b | 53,077 | 53,077 | 0 | 0% |
| dealloc_hash_contains_only_probe_16b | 56,144 | 56,144 | 0 | 0% |
| dealloc_own_thread_body_only_16b | 57,847 | 57,847 | 0 | 0% |
| dealloc_free_only_16b_n1 | 53,720 | 53,720 | 0 | 0% |
| dealloc_free_only_16b_n8 | 53,984 | 53,984 | 0 | 0% |
| dealloc_free_only_16b_n9 | 54,016 | 54,016 | 0 | 0% |
| dealloc_free_only_16b_n16 | 54,240 | 54,240 | 0 | 0% |
| dealloc_free_only_16b_n17 | 54,336 | 54,336 | 0 | 0% |
| dealloc_free_only_16b_n32 | 54,816 | 54,816 | 0 | 0% |
| dealloc_flush_class_only_16b_prefix | 48,749 | 48,749 | 0 | 0% |
| dealloc_flush_class_only_16b | 48,939 | 48,939 | 0 | 0% |
| dealloc_flush_all_tcache_16b_prefix | 49,088 | 49,088 | 0 | 0% |
| dealloc_flush_all_tcache_16b | 51,362 | 51,362 | 0 | 0% |
| alloc_clear_magazine_only_16b_prefix | 49,062 | 49,062 | 0 | 0% |
| alloc_clear_magazine_only_16b | 49,042 | 49,042 | 0 | 0% |
| alloc_zeroed_calloc_virgin_64k_prefix | 3 | 3 | 0 | 0% |
| alloc_zeroed_calloc_virgin_64k | 3 | 3 | 0 | 0% |
| alloc_zeroed_calloc_recycled_64k_prefix | 3 | 3 | 0 | 0% |
| alloc_zeroed_calloc_recycled_64k | 3 | 3 | 0 | 0% |
| dealloc_prealloc_only_1088_16b | 172,098 | 172,098 | 0 | 0% |
| dealloc_free_only_1088_16b_n17 | 172,829 | 172,829 | 0 | 0% |
| dealloc_free_only_1088_16b_n32 | 173,309 | 173,309 | 0 | 0% |
| dealloc_free_only_1088_16b_n64 | 174,461 | 174,461 | 0 | 0% |
| dealloc_free_only_1088_16b_n256 | 183,005 | 183,005 | 0 | 0% |
| dealloc_free_only_1088_16b_n1024 | 212,661 | 212,661 | 0 | 0% |
| dealloc_realloc_burst_1088_16b_n17 | 173,524 | 173,524 | 0 | 0% |
| oscillating_live_set_16b | 73,821 | 73,821 | 0 | 0% |
| carve_batch_only_16b | 2,921 | 2,921 | 0 | 0% |
| carve_batch_only_16b_2n | 8,120 | 8,120 | 0 | 0% |
| dealloc_batch_fresh_16_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_64_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_80_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_81_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_128_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_200_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_512_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_1024_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_0_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_1_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_8_16b | 3 | 3 | 0 | 0% |
| dealloc_batch_fresh_17_16b | 3 | 3 | 0 | 0% |
| medium_class_dealloc_churn_16b | 53,301 | 53,301 | 0 | 0% |
| aligned_churn_640b_a128 | 56,378 | 56,378 | 0 | 0% |
| large_alloc_free_cycle | 49,399 | 49,399 | 0 | 0% |
| large_cache_prefill_only_4mib | 58,549 | 58,549 | 0 | 0% |
| large_cache_hit_only_4mib | 59,939 | 59,939 | 0 | 0% |
| large_cache_eviction_prefill_only_41x4mib | 173,910 | 173,910 | 0 | 0% |
| large_cache_eviction_hit_only_41x4mib | 273,305 | 273,305 | 0 | 0% |
| large_cache_payload_prefill_only_4mib | 58,623 | 58,623 | 0 | 0% |
| large_cache_payload_hit_only_4mib | 58,629 | 58,629 | 0 | 0% |
| large_cache_free_slot_search_prefill_only | 66,284 | 66,284 | 0 | 0% |
| large_cache_free_slot_search_cycle_only | 78,152 | 78,152 | 0 | 0% |
| realloc_grow | 549,348 | 549,413 | 65 | 0.01% |
| realloc_large_shrink_8_to_6mib | 1,546,704 | 1,546,704 | 0 | 0% |
| realloc_large_shrink_8_to_6mib_prefix | 507,796 | 507,796 | 0 | 0% |
| realloc_large_shrink_8_to_4p5mib | 1,287,696 | 1,287,696 | 0 | 0% |
| realloc_large_shrink_8_to_4p5mib_prefix | 507,796 | 507,796 | 0 | 0% |
| realloc_large_shrink_8_to_3mib | 1,028,688 | 1,028,688 | 0 | 0% |
| realloc_large_shrink_8_to_3mib_prefix | 507,796 | 507,796 | 0 | 0% |
| realloc_large_grow_6_to_8mib | 1,546,713 | 1,546,713 | 0 | 0% |
| realloc_large_grow_6_to_8mib_prefix | 392,910 | 392,910 | 0 | 0% |
| realloc_large_equal_8mib | 966,831 | 966,831 | 0 | 0% |
| realloc_large_equal_8mib_prefix | 507,820 | 507,820 | 0 | 0% |
| cold_alloc_free_256x16b | 87,951 | 87,951 | 0 | 0% |
| cold_alloc_free_256x16b_2n | 131,452 | 131,452 | 0 | 0% |
| cold_alloc_free_256x16b_4n | 214,818 | 214,818 | 0 | 0% |
| cold_alloc_only_256x16b | 76,042 | 76,042 | 0 | 0% |
| cold_alloc_only_256x16b_2n | 108,727 | 108,727 | 0 | 0% |
| cold_alloc_only_256x16b_4n | 170,229 | 170,229 | 0 | 0% |
| cold_alloc_free_256x64b | 88,266 | 88,266 | 0 | 0% |
| recycle_alloc_free_256x16b | 110,204 | 110,204 | 0 | 0% |
| recycle_alloc_only_256x16b | 99,657 | 99,657 | 0 | 0% |
| recycle_alloc_free_256x64b | 110,519 | 110,519 | 0 | 0% |
| churn_256b | 53,313 | 53,313 | 0 | 0% |
| churn_write_256b | 53,503 | 53,503 | 0 | 0% |
| multiseg_cold_256k | 180,143 | 180,143 | 0 | 0% |
| seg_cycle_decommit_256k | 256,106 | 256,106 | 0 | 0% |
| growth16_n | 5,720,219 | 5,720,219 | 0 | 0% |
| growth16_2n | 11,391,258 | 11,391,258 | 0 | 0% |
| growth16_4n | 24,468,055 | 24,468,055 | 0 | 0% |
| r19_prf01_joined_producer_same_word | 545,928 | 545,928 | 0 | 0% |
| r19_prf01_publication | 125,253 | 125,253 | 0 | 0% |
| mimalloc_small_churn_16b | 15,169 | 15,169 | 0 | 0% |
| mimalloc_small_churn_16b_2n | 19,905 | 19,905 | 0 | 0% |
| mimalloc_churn_256b | 14,767 | 14,767 | 0 | 0% |
| mimalloc_cold_alloc_free_256x16b | 30,289 | 30,289 | 0 | 0% |
| mimalloc_cold_alloc_free_256x16b_2n | 55,608 | 55,608 | 0 | 0% |
| mimalloc_cold_alloc_free_256x16b_4n | 102,378 | 102,378 | 0 | 0% |
| mimalloc_cold_alloc_only_256x16b | 22,806 | 22,806 | 0 | 0% |
| mimalloc_cold_alloc_only_256x16b_2n | 40,701 | 40,701 | 0 | 0% |
| mimalloc_cold_alloc_only_256x16b_4n | 72,623 | 72,623 | 0 | 0% |
| mimalloc_cold_alloc_free_256x64b | 31,344 | 31,344 | 0 | 0% |
| mimalloc_recycle_alloc_free_256x16b | 50,238 | 50,238 | 0 | 0% |
| mimalloc_recycle_alloc_free_256x64b | 51,293 | 51,293 | 0 | 0% |
| mimalloc_bootstrap_proxy | 11,391 | 11,391 | 0 | 0% |
| decomp_full_cycle_8x | 621,496 | 621,496 | 0 | 0% |
| decomp_os_roundtrip_8x | 1,740 | 1,740 | 0 | 0% |
Total Ir base/head/delta: 58,092,915 / 58,092,980 / 65
```

### Input 3 — A1 back-edges

**Direction:** P2.0’s stated comparison target is zero back-edges, edge by edge; the current A1 lint must continue to pass. The one-off comparison asserts the committed `docs/layering_baseline.csv` contains **12** back-edge rows and **48** total references. `node scripts/module-graph.mjs --check-backedges` passes on this tree (parent’s independent verification). “Pass” means the ratchet holds; it does **not** mean zero. `segment_route` remains a single same-layer segment/route layer, so its mutual references are not back-edges (`scripts/module-graph/layers.json`, header note and `segment_route` layer; `docs/plans/2026-11-15-workspace-restructure-plan.md`, §2). The zero-edge value is the P2.0 target in plan §5, not a measured state.

### Input 4 — visibility widening

**Direction:** count the visibility/safety-surface cost; the criterion sets no numeric GO cutoff and treats signalled items as owner-facing audit cost.

The committed widening report’s **split** variant lists **182 items / 19 unsafe-or-pointer-signalled**; its **merged `sefer-segment-route`** variant lists **180 / 19** (`docs/perf/G1_P2_WIDENING_COUNT.md`, “Variant A — split” and “Variant B — merged”). These are alternative modeled boundaries, not additive totals. The report’s “Method” section explicitly says private declarations and unresolved path references are not counted, making the total a conservative undercount. A signal is not proof of a vulnerability; it indicates review obligations if a boundary is exposed. The criterion’s Input 4 and the widening report are inputs, not a P2 verdict.

## Reopening conditions

Reconsider STOP only if new evidence materially changes the comparison, for example:

- a representative history with source-affecting eligible changes shows positive selector savings, rather than the current docs-only eligible sample;
- at least three matched ordinary-CI runs per side show a measurable **incremental P2** benefit beyond P0, rather than a P0.2 binary-count change alone;
- the post-P1.6 no-LTO comparison demonstrates delta zero per bench;
- production `.text` identity is established for the proposed split, or the owner separately and explicitly changes the ghost-zero-cost gate policy; or
- audited widening/back-edge counts are reduced and show a verification benefit that is otherwise unavailable.

These are reopening triggers, not promises that P2 will be feasible or pass. Missing evidence is not evidence of impossibility. The raw gzip and baseline remain immutable; the companion CSV and this report are the only new perf artifacts. No G1/P2 waiting card was identified by the separate read-only review of both open-item indexes and all correctness tier files; no index/card was added or changed.

## Local verification and handoff

**OBSERVED locally:** the required `cargo check` rows—default, no-default-features, production, production + internals, hardened, all-features, workspace all-targets—all returned rc0 under `wnice -p idle -c50 -t4`. Clippy returned rc0 for all `PER_PR_ROWS` from `scripts/check-matrix.mjs`, using each row's correct lib/all-targets scope and `-Dwarnings`. Existing Node checks: 14 passed; report exact reproduction and CSV passed; A1 passed; structure checks OK; YAML validation reported 0 errors.

The strict registry command returned rc0, but its verdict is **INCOMPLETE(strict)**, not PASS: unpublished spike rows are **R-PG3, R-Ph1b-native, R-Ph1b-sb, R-Ph1b-tb, R-PG2-sb, R-PG2-tb, R5b-09-sb, R5b-09-tb**; separately, required cells await final status. The docs-consistency suite passed locally (all listed Node steps PASS; 0 failing steps; rc0) using `<J13B>/.rush/stdin/docs-consistency-check.sh <root>`; landing CI must still confirm it.

This is a docs-only edit: no evidence paths were touched, no changed file was cited by the registry, and no EV-1-triggering edit was made; rules/scopes are unchanged. Derived inventories and digests remain unchanged and are not stale; lockfiles/dependencies are unchanged. Rust tests and benchmarks were not executed, and CI diff-selected checks remain unverified on the new landing.
