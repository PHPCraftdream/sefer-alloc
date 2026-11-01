
## 3. Reading the map

1. **Growth family (grow64, grow1k, regrow, growmix).** This is the baseline of `main` with WG-LEAF
   and without AO-1 (PR #153, held by its preregistration) and without any release policy. What is
   known about it: WG-M0 attributed most of the growth gap (not that of growmix) to release inside
   `free` (opt-in `Retain` is the owner's decision), AO-1 targets the address order of magazine refills, WG-LEAF is merged.
   The end-RSS columns of the judge table show the trade that a release policy touches: sefer returns
   memory after `free`, mimalloc keeps it.
2. **`wild`, `large`, `phase` — the part without an attribution.** The sefer / mi ratios are above 1 in
   both campaigns of this report (the judge table and the attribution table are separate campaigns;
   their intervals overlap). The `sefer_pool` arm moves the ratio by no measurable amount
   (`sefer_pool / sefer` is close to 1 with intervals that include 1 in all three scenarios), **and**
   the oracle column shows why: `seg_released` of the pool arm is as large as the plain arm's (equal
   for `wild` and `large`, 1004 against 1022 for `phase`). These scenarios allocate sizes up to
   4 MiB (`wild`) and 64 KiB..32 MiB (`large`), so the releases are most likely Large spans, which the
   WG-M0 pool (empty *small* segments only) does not retain; the counter does not tell the kinds apart. Consequently this arm cannot separate "release" from "everything else" here, and the
   12 % in the `phase` row is a share of a gap that the arm barely touched — it is not evidence for or
   against release. **Open:** a lever that reaches Large spans (the large-cache policy, a
   Large-span retention arm) or a per-phase timer is needed before a cause is named.
3. **`bench:table`.** Only the warm bulk burst of 16 B is a loss in both sets; churn + write 16 B is
   split (one set below the loss line, one above), so it is not a finding on its own; a third paired
   set would be needed.
4. **MT.** The T=1 gaps of larson and mstress and the T=4 gap of mstress are what the map adds to
   MT1. The table reports medians of 6 runs without a spread; a loss here is a priority for a
   dedicated paired campaign, not a verdict.
5. **What the map does not cover:** Linux (no run in this report), the RSS of the MT workloads, and
   any mechanism below the cell level.

## 4. Order of work this map suggests

| rank | item | why | needs |
|---:|---|---|---|
| 1 | growth family | largest ratios and the widest breadth (4 of 7 judge scenarios) | owner decisions: AO-1 (#153), `Retain` budget |
| 2 | `wild` / `large` / `phase` attribution (new task) | second-largest ratios, no mechanism yet; the pool arm cannot reach it | a Large-span lever or timers, consultation (@oh, rush reviewer) |
| 3 | MT T=1 (mstress, larson) and mstress T=4 | losses on the most common production shape (single thread) | paired MT campaign, then profile |
| 4 | bulk 16 B (D16-R) | the only `bench:table` loss; ratio smaller than 1 and 2 | profile of the 16 B bulk path |

## 5. Reproduction

- Build at `c495edc9`: `cargo build --release --example g1_wild_judge --features "production internals"` and
  `cargo build --release --example malloc_macro --features production`; compare the binaries'
  SHA-256 with `docs/perf/lm1/ident.txt` (same toolchain assumed).
- Judge: `node docs/perf/lm1/run.mjs --exe <judge> --out judge_raw.jsonl --blocks 12 --repeats 3`;
  attribution: the same with `--scenarios wild,phase,large --arms sefer,sefer_pool,mi` into `pool_raw.jsonl`.
- `bench:table`: `node scripts/bench-table-paired.mjs --rounds 200 --processes 3 --features production --raw <prefix>`
  twice (prefixes `bt_s1`, `bt_s2`); the raw files are committed gzipped, the manifests as written.
- MT: six consecutive runs of `malloc_macro`, concatenated with `== run N` separators into `mt_raw.txt`.
- Tables and the summary CSV: `node scripts/lm1_report.mjs` (reads `docs/perf/lm1/`, writes
  `docs/perf/lm1/tables.md`, `docs/perf/LM1_LOSS_MAP_summary.csv` and this file from `doc_head.md`,
  `doc_tail.md` and the generated tables).
- Raw files are stored gzipped (`*.jsonl.gz`); the `raw_sha256` values in the `bench:table` manifests are
  those of the uncompressed files.
