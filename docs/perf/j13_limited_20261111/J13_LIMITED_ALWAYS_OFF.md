# J13 current range Always/Off — measured limited study

**Measurement only. No product GO, shipping default change or Small-Ir result.**

## Identity and execution

- Registered attempt: [43014496362](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/43014496362), preparation job `130515411731`. The measurement step and evidence upload completed SUCCESS.
- Measurement source: `573cb7c213eb72fe070776efb3a8e44548633ba6`; tree: `43c67faea6cc0b83929cf6ddd88e5bb199596110`.
- Measured binary SHA-256: `44d527cfa37f3ad38832c81af326f8c0899d31f58d08e3084969d41e442135b6`.
- Exactly 960 children: ten cells, 12 ABBA blocks plus 12 A/A blocks per cell. One registered attempt, no cell replacement, repeat or additional measurement warmup.
- Children started at `2026-11-11T12:16:03.449Z`; report generation completed at `2026-11-11T12:18:57.264Z`. Build and component checks are outside this interval and the native 30-minute measurement-step timeout.
- Actual preflight: Linux x86_64, 4096-byte base pages, 2097152-byte THP, top-level `madvise`, MemAvailable `15353139200` bytes. Raw preflight, actual compiler/resolved features/flags and source/hash receipts accompany the report.
- Smoke `43013696396` numbers were discarded for performance claims. They were used only for pre-dispatch runtime planning and are not included in any registered sample or interval.

## Evidence and verification

The original per-comparison JSON and full generated CSV/Markdown are preserved losslessly as gzip in the cell subdirectories. Original build/measurement-host receipts, attempt identity, timestamps and raw preflight are stored alongside them. `MANIFEST.json` records compressed and uncompressed hashes and sizes. Every registered raw comparison JSON is in the 200 KiB–2 MiB gzip storage tier, below 2 MiB uncompressed. The two kernel cpuinfo snapshots are also stored losslessly as gzip to retain their original trailing blank line without text normalization; their uncompressed hashes remain in the manifest.

All ten original CSV/Markdown outputs were recomputed from the saved raw documents through the existing pure `scripts/j13_limited_report.mjs` and compared byte-for-byte before packaging. The tables below are copied from those checked generated outputs, not hand-transcribed. `J13_LIMITED_ALWAYS_OFF_summary.csv` concatenates a column projection of their 91 metric rows, adding actual shared provenance columns; full input contexts remain in the named JSON receipts and original generated outputs. No second statistics pipeline is used.

The native binary was not executed locally. Downloaded artifacts were isolated and read as data. The original CI artifact remains the source for complete build/job logs.

## Interpretation and decision boundary

- A/A readability is satisfied in **10/10 cells**: each A/A 95% interval contains 1. This is a readability check, not proof that quotas, host pressure or other systematic effects are absent.
- Dense eligible cells favor Always. The 1 MiB and 8 MiB-minus-1-byte controls do not establish a speed difference. The exact 8 MiB boundary activates advice.
- The sparse `edge` cell is slower. Its relative cost must be presented with its small absolute time, not hidden by the dense results. Bring this finding to the owner before any default decision. Do not combine sparse and dense regimes into a Pareto claim.
- The second-life CACHE payload is slightly slower under Always. Its much faster whole-child observation includes first-life cold work and must not be presented as a cache-hit-only speedup.
- Report faults, approximate RSS and live AnonHugePages separately. The accepted per-region memory cost is reported, not a renewed veto. These observations do not establish exact physical peaks or exact advice attribution.
- Observer capture remains included. In CACHE, whole-child/OS observations span cold plus cache plus trim; first-life cold timing is separately descriptive. Null means unavailable, never zero substitution.
- Correctness item 264 remains OPEN. No standalone Small-Ir comparison was performed. Existing changed-default Small-path gates belong to #107.
- The measured source predates the independent fuzz-adapter correction `eef21ab2`. The old-source manual workflow has a known false `DoubleFreeOk` fuzz failure; it is **not** described as overall SUCCESS. The allocator and Large measurement sources were not changed by that test-only correction. Its exact captured-input replay passed in job130532120647 on `639b4ba1`, and CI43023190555 completed SUCCESS on that same source. This regression verification is separate from the measurement-step outcome and is not a random-fuzz campaign.
- The owner must review this report and decide the default before #107. Historical whole-mapping results are not combined with these current range-advice results. Auto/budget remains later; general builder and scalar research remain paused.

## Reproduction

Retrieve the named artifact from run `43014496362` and verify manifest hashes; do not execute its downloaded binary. Recompute each saved AB/AA pair with `j13LimitedReport({evidence_class, build_receipt, measurement_host, ab:{document,facts:{cell,regime}}, control:{document,facts:{cell,regime}}})`, as documented in `../J13_LIMITED_HARNESS_USAGE.md`. CACHE uses regime `CACHE`; the other cells use `COLD`.

Historical dispatch command: `gh workflow run ci.yml --ref j13-limited-study-20261110 -f j13_study=true -f j13_smoke=false`, with that ref verified at the immutable measurement source immediately before dispatch. The mutable branch is not a permanent reproduction pin: code reproduction must use the recorded commit and the actual receipt's compiler, resolved features and flags. This recipe does not authorize repeating the completed study.

## Registered generated tables

Estimator: minimum of exactly 2 raw launches per slot/block; exp(mean of 12 paired log ratios); two-sided 95% paired t CI, df=11, t*=2.201

### cycle-1MiB

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| cycle-1MiB | COLD | selected (runner_root.cold_ns) | allocation+touch+free | 0.9882 (0.9745–1.0021) Always/Off | 1.0081 (0.9737–1.0437); A/A interval contains 1; readability only, not speed evidence | 387634 → 386924.5 |
| cycle-1MiB | COLD | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 1.0081 (0.9737–1.0437); A/A interval contains 1; readability only, not speed evidence | 2801396.5 → 2908158 |
| cycle-1MiB | COLD | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 1.0081 (0.9737–1.0437); A/A interval contains 1; readability only, not speed evidence | 451 → 451 |
| cycle-1MiB | COLD | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 1.0081 (0.9737–1.0437); A/A interval contains 1; readability only, not speed evidence | 3715072 → 3702784 |
| cycle-1MiB | COLD | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 1.0081 (0.9737–1.0437); A/A interval contains 1; readability only, not speed evidence | 2889728 → 2861056 |
| cycle-1MiB | COLD | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 1.0081 (0.9737–1.0437); A/A interval contains 1; readability only, not speed evidence | 0 → 0 |
| cycle-1MiB | COLD | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0081 (0.9737–1.0437); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| cycle-1MiB | COLD | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0081 (0.9737–1.0437); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| cycle-1MiB | COLD | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 1.0081 (0.9737–1.0437); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |

### cycle-8MiB-minus-1

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| cycle-8MiB-minus-1 | COLD | selected (runner_root.cold_ns) | allocation+touch+free | 0.9940 (0.9820–1.0062) Always/Off | 1.0048 (0.9899–1.0200); A/A interval contains 1; readability only, not speed evidence | 2973892.5 → 2963466 |
| cycle-8MiB-minus-1 | COLD | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 1.0048 (0.9899–1.0200); A/A interval contains 1; readability only, not speed evidence | 5395510.5 → 5496272.5 |
| cycle-8MiB-minus-1 | COLD | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 1.0048 (0.9899–1.0200); A/A interval contains 1; readability only, not speed evidence | 2244 → 2244 |
| cycle-8MiB-minus-1 | COLD | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 1.0048 (0.9899–1.0200); A/A interval contains 1; readability only, not speed evidence | 11042816 → 11042816 |
| cycle-8MiB-minus-1 | COLD | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 1.0048 (0.9899–1.0200); A/A interval contains 1; readability only, not speed evidence | 2879488 → 2885632 |
| cycle-8MiB-minus-1 | COLD | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 1.0048 (0.9899–1.0200); A/A interval contains 1; readability only, not speed evidence | 0 → 0 |
| cycle-8MiB-minus-1 | COLD | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0048 (0.9899–1.0200); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| cycle-8MiB-minus-1 | COLD | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0048 (0.9899–1.0200); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| cycle-8MiB-minus-1 | COLD | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 1.0048 (0.9899–1.0200); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |

### cycle-8MiB

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| cycle-8MiB | COLD | selected (runner_root.cold_ns) | allocation+touch+free | 0.1336 (0.1177–0.1518) Always/Off | 0.9908 (0.9802–1.0016); A/A interval contains 1; readability only, not speed evidence | 3036332 → 477341.5 |
| cycle-8MiB | COLD | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 0.9908 (0.9802–1.0016); A/A interval contains 1; readability only, not speed evidence | 5505875 → 2853511 |
| cycle-8MiB | COLD | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 0.9908 (0.9802–1.0016); A/A interval contains 1; readability only, not speed evidence | 2242 → 199 |
| cycle-8MiB | COLD | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 0.9908 (0.9802–1.0016); A/A interval contains 1; readability only, not speed evidence | 11046912 → 13176832 |
| cycle-8MiB | COLD | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 0.9908 (0.9802–1.0016); A/A interval contains 1; readability only, not speed evidence | 2859008 → 2867200 |
| cycle-8MiB | COLD | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 0.9908 (0.9802–1.0016); A/A interval contains 1; readability only, not speed evidence | 0 → 10485760 |
| cycle-8MiB | COLD | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 0.9908 (0.9802–1.0016); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| cycle-8MiB | COLD | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 0.9908 (0.9802–1.0016); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| cycle-8MiB | COLD | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 0.9908 (0.9802–1.0016); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |

### cycle-16MiB

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| cycle-16MiB | COLD | selected (runner_root.cold_ns) | allocation+touch+free | 0.1421 (0.1393–0.1451) Always/Off | 0.9976 (0.9877–1.0077); A/A interval contains 1; readability only, not speed evidence | 6149720.5 → 884646 |
| cycle-16MiB | COLD | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 0.9976 (0.9877–1.0077); A/A interval contains 1; readability only, not speed evidence | 9033921 → 3503092.5 |
| cycle-16MiB | COLD | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 0.9976 (0.9877–1.0077); A/A interval contains 1; readability only, not speed evidence | 4292 → 204 |
| cycle-16MiB | COLD | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 0.9976 (0.9877–1.0077); A/A interval contains 1; readability only, not speed evidence | 19437568 → 21565440 |
| cycle-16MiB | COLD | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 0.9976 (0.9877–1.0077); A/A interval contains 1; readability only, not speed evidence | 2883584 → 2881536 |
| cycle-16MiB | COLD | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 0.9976 (0.9877–1.0077); A/A interval contains 1; readability only, not speed evidence | 0 → 18874368 |
| cycle-16MiB | COLD | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 0.9976 (0.9877–1.0077); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| cycle-16MiB | COLD | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 0.9976 (0.9877–1.0077); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| cycle-16MiB | COLD | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 0.9976 (0.9877–1.0077); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |

### cycle-128MiB

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| cycle-128MiB | COLD | selected (runner_root.cold_ns) | allocation+touch+free | 0.1307 (0.1266–0.1350) Always/Off | 1.0107 (0.9908–1.0311); A/A interval contains 1; readability only, not speed evidence | 50361941.5 → 6390493.5 |
| cycle-128MiB | COLD | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 1.0107 (0.9908–1.0311); A/A interval contains 1; readability only, not speed evidence | 55764617.5 → 9417961.5 |
| cycle-128MiB | COLD | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 1.0107 (0.9908–1.0311); A/A interval contains 1; readability only, not speed evidence | 32964 → 260 |
| cycle-128MiB | COLD | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 1.0107 (0.9908–1.0311); A/A interval contains 1; readability only, not speed evidence | 136888320 → 139012096 |
| cycle-128MiB | COLD | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 1.0107 (0.9908–1.0311); A/A interval contains 1; readability only, not speed evidence | 2875392 → 2883584 |
| cycle-128MiB | COLD | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 1.0107 (0.9908–1.0311); A/A interval contains 1; readability only, not speed evidence | 0 → 136314880 |
| cycle-128MiB | COLD | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0107 (0.9908–1.0311); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| cycle-128MiB | COLD | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0107 (0.9908–1.0311); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| cycle-128MiB | COLD | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 1.0107 (0.9908–1.0311); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |

### cycle-1024MiB

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| cycle-1024MiB | COLD | selected (runner_root.cold_ns) | allocation+touch+free | 0.1273 (0.1253–0.1292) Always/Off | 1.0018 (0.9847–1.0193); A/A interval contains 1; readability only, not speed evidence | 403793011.5 → 51069253 |
| cycle-1024MiB | COLD | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 1.0018 (0.9847–1.0193); A/A interval contains 1; readability only, not speed evidence | 424478179 → 54739998.5 |
| cycle-1024MiB | COLD | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 1.0018 (0.9847–1.0193); A/A interval contains 1; readability only, not speed evidence | 262339.5 → 707 |
| cycle-1024MiB | COLD | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 1.0018 (0.9847–1.0193); A/A interval contains 1; readability only, not speed evidence | 1076920320 → 1079072768 |
| cycle-1024MiB | COLD | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 1.0018 (0.9847–1.0193); A/A interval contains 1; readability only, not speed evidence | 3434496 → 3420160 |
| cycle-1024MiB | COLD | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 1.0018 (0.9847–1.0193); A/A interval contains 1; readability only, not speed evidence | 0 → 1075838976 |
| cycle-1024MiB | COLD | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0018 (0.9847–1.0193); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| cycle-1024MiB | COLD | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0018 (0.9847–1.0193); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| cycle-1024MiB | COLD | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 1.0018 (0.9847–1.0193); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |

### grow-1GiB

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| grow-1GiB | COLD | selected (runner_root.cold_ns) | allocation+touch+free | 0.1597 (0.1566–0.1628) Always/Off | 0.9998 (0.9880–1.0118); A/A interval contains 1; readability only, not speed evidence | 896388132 → 143366975 |
| grow-1GiB | COLD | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 0.9998 (0.9880–1.0118); A/A interval contains 1; readability only, not speed evidence | 915574447.5 → 147220769 |
| grow-1GiB | COLD | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 0.9998 (0.9880–1.0118); A/A interval contains 1; readability only, not speed evidence | 523982 → 2765 |
| grow-1GiB | COLD | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 0.9998 (0.9880–1.0118); A/A interval contains 1; readability only, not speed evidence | 1076953088 → 1081159680 |
| grow-1GiB | COLD | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 0.9998 (0.9880–1.0118); A/A interval contains 1; readability only, not speed evidence | 3426304 → 3457024 |
| grow-1GiB | COLD | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 0.9998 (0.9880–1.0118); A/A interval contains 1; readability only, not speed evidence | 0 → 1075838976 |
| grow-1GiB | COLD | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 0.9998 (0.9880–1.0118); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| grow-1GiB | COLD | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 0.9998 (0.9880–1.0118); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| grow-1GiB | COLD | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 0.9998 (0.9880–1.0118); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |

### many-128x16MiB

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| many-128x16MiB | COLD | selected (runner_root.cold_ns) | allocation+touch+free | 0.1462 (0.1439–0.1485) Always/Off | 1.0024 (0.9913–1.0137); A/A interval contains 1; readability only, not speed evidence | 783398866.5 → 113130919.5 |
| many-128x16MiB | COLD | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 1.0024 (0.9913–1.0137); A/A interval contains 1; readability only, not speed evidence | 816139763.5 → 116950016 |
| many-128x16MiB | COLD | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 1.0024 (0.9913–1.0137); A/A interval contains 1; readability only, not speed evidence | 524628 → 1415 |
| many-128x16MiB | COLD | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 1.0024 (0.9913–1.0137); A/A interval contains 1; readability only, not speed evidence | 2150660096 → 2418669568 |
| many-128x16MiB | COLD | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 1.0024 (0.9913–1.0137); A/A interval contains 1; readability only, not speed evidence | 2945024 → 3158016 |
| many-128x16MiB | COLD | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 1.0024 (0.9913–1.0137); A/A interval contains 1; readability only, not speed evidence | 0 → 2415919104 |
| many-128x16MiB | COLD | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0024 (0.9913–1.0137); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| many-128x16MiB | COLD | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0024 (0.9913–1.0137); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| many-128x16MiB | COLD | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 1.0024 (0.9913–1.0137); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |

### edge-128MiB

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| edge-128MiB | COLD | selected (runner_root.cold_ns) | allocation+touch+free | 10.0406 (8.7064–11.5792) Always/Off | 0.9574 (0.9001–1.0184); A/A interval contains 1; readability only, not speed evidence | 15198.5 → 176632 |
| edge-128MiB | COLD | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 0.9574 (0.9001–1.0184); A/A interval contains 1; readability only, not speed evidence | 2242137 → 2504117.5 |
| edge-128MiB | COLD | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 0.9574 (0.9001–1.0184); A/A interval contains 1; readability only, not speed evidence | 197 → 196 |
| edge-128MiB | COLD | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 0.9574 (0.9001–1.0184); A/A interval contains 1; readability only, not speed evidence | 2875392 → 6891520 |
| edge-128MiB | COLD | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 0.9574 (0.9001–1.0184); A/A interval contains 1; readability only, not speed evidence | 2875392 → 2879488 |
| edge-128MiB | COLD | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 0.9574 (0.9001–1.0184); A/A interval contains 1; readability only, not speed evidence | 0 → 4194304 |
| edge-128MiB | COLD | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 0.9574 (0.9001–1.0184); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| edge-128MiB | COLD | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 0.9574 (0.9001–1.0184); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| edge-128MiB | COLD | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 0.9574 (0.9001–1.0184); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |

### cache-16MiB

| Cell | Regime | Metric | Scope | Always/Off (95% CI) | A/A payload B/A | AB Off → Always median |
|---|---|---|---|---:|---:|---:|
| cache-16MiB | CACHE | selected (parsed_row.cache_ns) | allocation+touch+free | 1.0231 (1.0093–1.0371) Always/Off | 1.0125 (0.9929–1.0324); A/A interval contains 1; readability only, not speed evidence | 36961 → 37917.5 |
| cache-16MiB | CACHE | child (runner_launch.child_wall_ns) | full direct child | descriptive; no ratio | 1.0125 (0.9929–1.0324); A/A interval contains 1; readability only, not speed evidence | 9267249 → 3563783 |
| cache-16MiB | CACHE | faults (parsed_row.os.faults) | process fault observation | descriptive; no ratio | 1.0125 (0.9929–1.0324); A/A interval contains 1; readability only, not speed evidence | 4291 → 203 |
| cache-16MiB | CACHE | peak_rss (parsed_row.os.peak_rss) | approximate process peak RSS | descriptive; no ratio | 1.0125 (0.9929–1.0324); A/A interval contains 1; readability only, not speed evidence | 19429376 → 21557248 |
| cache-16MiB | CACHE | end_rss (parsed_row.os.end_rss) | process end RSS | descriptive; no ratio | 1.0125 (0.9929–1.0324); A/A interval contains 1; readability only, not speed evidence | 2873344 → 2889728 |
| cache-16MiB | CACHE | anon_huge_peak (parsed_row.os.anon_huge_peak) | live AnonHugePages observation | descriptive; no ratio | 1.0125 (0.9929–1.0324); A/A interval contains 1; readability only, not speed evidence | 0 → 18874368 |
| cache-16MiB | CACHE | capture_slot (parsed_row.capture_slot_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0125 (0.9929–1.0324); A/A interval contains 1; readability only, not speed evidence | 4194304 → 4194304 |
| cache-16MiB | CACHE | capture_total (parsed_row.capture_total_storage_bytes) | native advice capture storage | descriptive; no ratio | 1.0125 (0.9929–1.0324); A/A interval contains 1; readability only, not speed evidence | 4194320 → 4194320 |
| cache-16MiB | CACHE | capture_tls (parsed_row.capture_tls_context_bytes_per_thread) | native advice capture storage | descriptive; no ratio | 1.0125 (0.9929–1.0324); A/A interval contains 1; readability only, not speed evidence | 16 → 16 |
| cache-16MiB | COLD | cold_first_life (runner_root.cold_ns) | first-life allocation+touch+free; COLD | descriptive; no ratio | COLD first-life median; descriptive, no CI | 5122188 → 808230.5 |
