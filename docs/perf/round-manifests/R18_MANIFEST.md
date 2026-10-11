# Round 18 manifest — source review remediation and P2 measurement

**Scope:** Round 18 starts at `ff3d60bb571f706cd8f034665fac8c3f4b6f0d23` (the R17 synthesis target) and ends with the R18 evidence-closeout commit that adds this manifest. The range command below derives the prior commit rows. The final row is this file's own commit; recover its self-referential SHA with `git log -1 --format=%H -- docs/perf/round-manifests/R18_MANIFEST.md`, following the established manifest convention.

**Commit table reproduction:**

```sh
git log --reverse --format="%H|%cI|%s" ff3d60bb571f706cd8f034665fac8c3f4b6f0d23..HEAD
```

The 17 rows before the self-row are transcribed from the exact `git log` output at the R18 closeout base. No SHA is inferred from a subject or worktree branch.

## §1. Commit classification

| # | SHA | Date | Subject | Category and R18 effect |
|---:|---|---|---|---|
| 1 | `7b4aedd05676c4b7d010f0575e3746fb05e4bd6d` | 2026-10-09 01:09:50 +02:00 | `fix(alloc): use canonical provenance for decommit diagnostics` | **fix — soundness**; metadata reads resolve through the table's canonical stored pointer. |
| 2 | `54fc2a21b706d71b137a73d2dd17391a878435a0` | 2026-10-09 01:57:57 +02:00 | `fix(perf): validate hardened freelists and remove dead generation table` | **fix(perf) — correctness/consistency**; no speed or RSS claim. |
| 3 | `36ba37584c73fc5495c4a2e1248411a2dc2f34d3` | 2026-10-09 02:26:04 +02:00 | `fix(perf): guard magazine frees against invalid bounds` | **fix(perf) — invalid-input hardening**; no speed claim. |
| 4 | `ccd26f54bf7527a4cb14ddd8087d5a125bf2e8f5` | 2026-10-09 02:48:04 +02:00 | `fix(perf): count only successful large-cache hits` | **fix(perf) — diagnostic correctness**; `alloc-stats`-gated. |
| 5 | `ecf8e898b38b8d8ed7450031d3b9ebdc36bd710a` | 2026-10-09 03:19:34 +02:00 | `fix(perf): bound fallback spin counters` | **fix(perf) — bounded wait bookkeeping**; no latency claim. |
| 6 | `fc40ab96e234bc37b6d9281b7b9a7cb3273dfd16` | 2026-10-09 03:36:21 +02:00 | `fix(api): support NUMA-only large allocation build` | **fix — supported feature configuration**; no production algorithm change. |
| 7 | `44ca9541f2b654ab8f5db7dd087fa797b7f0ea6a` | 2026-10-09 03:55:31 +02:00 | `fix(perf): remove unused realloc bound branch` | **fix(perf) — dead-shape cleanup**; no speed claim. |
| 8 | `f153c54b03cc958813d2a639aee115ae2ff2a318` | 2026-10-09 04:28:09 +02:00 | `build: restore edition-2021 import ordering` | **build/formatting**. |
| 9 | `1cb85d3348705367b24bd188aea0f08f522c7d8e` | 2026-10-09 05:30:38 +02:00 | `docs: reconcile allocator contracts and diagnostics` | **docs-only**; source contracts/counters/inventories reconciled. |
| 10 | `f384eb0f3d165950da87bc51de6fc4bb5871cee4` | 2026-10-09 07:17:12 +02:00 | `fix(api): gate hidden test hooks behind internals` | **fix(api) — test-surface isolation**; production callers use internal helpers. |
| 11 | `a7d3ff099a71ee4059bf492f4961ec81bcb85b48` | 2026-10-09 08:27:37 +02:00 | `docs(api): clarify GlobalAlloc no-unwind contract` | **docs/API contract**; no unwind behavior change. |
| 12 | `1552d97742b54fb076ce9c1f97af881ef90a73a7` | 2026-10-09 08:46:09 +02:00 | `docs: clarify bounded decay and gated layout helpers` | **docs-only**; no runtime change. |
| 13 | `8ac3f93d502208f2d5f4170a8f765da081459f26` | 2026-10-09 09:01:35 +02:00 | `docs(api): reconcile feature and pinning contracts` | **docs/API contract**; no feature composition change. |
| 14 | `22285f5683e269e0395b75ab6013e2f4e7c5fff0` | 2026-10-09 09:17:19 +02:00 | `fix(build): gate experimental on 64-bit atomics` | **fix(build) — unsupported-target diagnostic**; production feature composition unchanged. |
| 15 | `689755101a393f794496ff5c5c29c9ea39870d07` | 2026-10-09 09:50:58 +02:00 | `test(epoch): align Loom shadow eviction order` | **test-only model correction**; remains a shadow model. |
| 16 | `8d08825a9a92192763bdc12e1bfb0f9c25af6003` | 2026-10-09 11:04:36 +02:00 | `docs(review): index P4 verification residuals` | **docs/process**; open verification items indexed, not closed. |
| 17 | `07a3652b74ea535977cb7c5c487a66b416c4205c` | 2026-10-10 23:56:44 +02:00 | `fix(perf): restore realloc committed-span guard` | **fix(perf)**; restores the defensive promotion read bound. The closeout also records R35 measurement artifacts, closes the R15/R16 follow-up findings, refreshes the README bench census, adjusts the Windows small-stack regression setup, and documents the R30-12 inline-attribute false-positive exception; no speedup is claimed and no history is rewritten. |
| 18 | *(this commit; recover with the command above)* | 2026-10-11 | `docs(evidence): refresh R18 verification receipts` | **docs**; refreshes pinned Loom, Miri, Windows and WSL receipts and records final R18 verification. |

**Aggregate:** 18 commits total (17 prior R18 work commits plus this closeout commit). Categories: 1 `fix(alloc)`, 6 `fix(perf)` correctness/consistency fixes, 2 `fix(api)`, 2 build/formatting fixes, 6 docs, 1 test. This matches the commit subjects in §1.

## §2. Net production/default impact

- `production`'s Cargo feature composition is **UNCHANGED**. No dependency or crate version changed.
- Valid production allocation/free behavior retains the existing algorithms; the committed-span fix restores null rejection for a false `old_layout` that would otherwise be copied beyond the committed bound.
- The R18 prefilter, TLS selector and counters require both `bench-internals` and the explicit `r18_sidecar_scan_bench` build cfg. Ordinary production and ordinary `perf_gate_iai` builds without that cfg use the original unconditional `AcqRel` swap and have no R18 TLS read. No Cargo feature was added.
- `benches/perf_gate_iai.rs` is unchanged; ordinary `npm run iai` does not compile the dedicated cfg or register the R18 scenarios.
- No `perf(runtime)` or `perf(opt-in)` commit landed. The `fix(perf)` commits above are correctness/consistency changes, not speedup claims.

## §3. Measurement and artifacts

The only new performance measurement is the dedicated R35 Callgrind gate, reported in [`R35_SIDE_CAR_SCAN_PREFILTER_GATE.md`](../R35_SIDE_CAR_SCAN_PREFILTER_GATE.md) with its companion [`summary CSV`](../R35_SIDE_CAR_SCAN_PREFILTER_GATE_summary.csv). It uses direct `SeferAlloc` `GlobalAlloc::alloc/dealloc -> HeapCore`, not an installed process-global allocator; one binary, two runtime modes; three scenarios and three samples per mode/setting; route-activation evidence per arm. The exact source/binary identities, build cfg, Rust flags, explicit-empty `RUSTC_WORKSPACE_WRAPPER`, CPU/OS, raw logs and reproduction commands are recorded in that report and [`r35_sidecar_scan_evidence_test_oracle_final/`](../r35_sidecar_scan_evidence_test_oracle_final/).

Callgrind observed the candidate skip empty-word exchanges in every activated scenario. The uncounted candidate retired 32 more Ir in each scenario and had lower **estimated** cycles. These are simulated cycle-model observations, not native latency, RSS, producer latency, cache-coherence benefit or a production speedup. The publication setup is outside the counted owner window. There is no pre-registered numeric promotion threshold.

The report cites 12 root `_raw_*.log` files totaling **25,716 bytes**; every file is below 200 KiB and is force-added because the report cites it. Per-sample JSON, source snapshots and identities are in the evidence directory. No large raw artifact is omitted.

Round-wide, the R18 range adds 16 `_raw_*.log` files totaling **44,106 bytes**: the 12 perf logs above (25,716 bytes) plus four Ph7 receipts in `docs/evidence/` (18,390 bytes: 5,638 / 3,905 / 4,135 / 4,712 bytes). Reproduce the inventory with `git diff --name-only --diff-filter=A ff3d60bb571f706cd8f034665fac8c3f4b6f0d23 HEAD -- "docs/perf/_raw_*.log" "docs/evidence/_raw_*.log" | xargs -r wc -c`. All 16 are below the 200 KiB threshold.

## §4. Final verdict by review priority

| Priority/finding group | R18 verdict |
|---|---|
| P1 — `R17-UNS-01` | **CLOSED** by canonical-pointer provenance lookup; native/strict-provenance Miri and negative-control receipts are in the P1 report and CHANGELOG. |
| P2 — `R17-PRF-01` | **OPEN / NOT PROMOTED.** Gate and path activation completed; native producer latency/coherence and an actual-type/reviewed publication-refinement proof remain absent. Correctness perf item 82 remains OPEN. |
| P3 — confirmed fixes | Implemented fixes and receipts are cross-referenced in the P3 report and CHANGELOG. `R17-PRF-02/03/04` remain OPEN; no speculative optimization/default change was made. |
| P4 — hooks/API/model/coverage | Confirmed source/API fixes and 34-finding crosswalk are in the P4 report. Correctness items 185–199 remain OPEN with their triggers; shadow Loom is not actual-type proof. |
| P5 — maintainability/docs | The specific CQ-12 dead-shape realloc parameter/unused wrapper parts were removed. CQ-10/11/13 and unrated OPT proposals remain open/unimplemented. |
| R15/R16 follow-up — R17F-01..05 plus the realloc cleanup regression | **CLOSED / corrected.** F1/F2 documentation scope corrected; F3 freshness and exact-E0514 handling hardened; F4 interpretation corrected with NO-GO unchanged; F5 same-target all-foreign skip fails closed. The empty realloc guard is restored and counterfactually tested. Focused receipts are in the R15/R16 review addendum; workflow files were not changed. |

This manifest does not close correctness items 154/160 or CI/platform/model-proof residuals. Detailed IDs and acceptance boundaries remain in the P-level reports, `docs/perf/OPEN_ITEMS.md`, and `docs/CORRECTNESS_OPEN_ITEMS.md`; their current-state cards remain authoritative.
