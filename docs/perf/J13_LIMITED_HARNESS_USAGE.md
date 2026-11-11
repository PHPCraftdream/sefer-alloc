# J13 limited harness usage

The dated 2026-11-11 owner amendment in `J13_LIMITED_STUDY_PREREG.md` authorizes manual GitHub CI smoke and the registered Large study, not local native execution. No new executable entrypoint, workflow, controller or benchmark is introduced. Read that amendment for the 20-minute feasibility gate and single interrupted-run repeat policy.

## Existing judge and paired runner

The study environment is `G1_BIG_THP=off|always` and `G1_BIG_CACHE=off|on`; both are required together. Optional `G1_BIG_BYTES` is an exact positive decimal byte count that must ceil to the CLI MiB argument. CLI: `g1_big_judge <cycle|edge|grow|many> sefer <size_mib> <reps> [k]`.

- Boundary cycle: `cycle sefer 8 1`, `G1_BIG_BYTES=8388607`, cache off.
- Cache cycle: `cycle sefer 16 2 1`, cache on; second life must be a real hit.
- Other cells use exactly the registered scenario/size/reps/k. No additional measurement cells or warmups.

Fixed templates in `docs/perf/j13_limited_cells/*.json` use `result_format: "json"`, metric `cold_ns`, no build/sanity fields, identical command/arguments for both arms, and explicit environment values. CI replaces only the executable token from the actual Cargo build receipt into temporary configurations. A=Off, B=Always. Existing invocation: `node scripts/paired-ab-runner.mjs --config <config.json> --arms A,B --pairs 12`; the control substitutes `--arms A,A`. Each comparison has its own `PAIRED_AB_OUT_DIR` outside the checkout.

Payload calls use the configured SeferAlloc facade; process-global System handles bookkeeping. `dbg_current_large_cache_budget()` binds the configured heap before tuple/stats readback, outside payload timing. THP budget 0 is unlimited advice bytes, not cache capacity. Cache-on uses 32 MiB identically in both arms. Preserve requested/resolved mode/minimum/budget/gradient/cache and both conflict counters, fresh mapping/requested-size evidence, accepted-prefix advice and cache-hit evidence. Advice capture is observer-inclusive: do not subtract its cost/storage or invent an uninstrumented timing.

## Build, observer and measurement-host identity

The existing preparation action performs a locked release build with the explicit rustc host target and features `production alloc-stats internals bench-internals`. It captures clean source SHA/tree/status, verbose Rust/Cargo versions, exact Cargo argv, actual resolved compiler-artifact features/target/path, explicit set/unset `RUSTFLAGS` and `CARGO_ENCODED_RUSTFLAGS`, binary hash and copied-artifact rehash. Use the recorded artifact path, not a guessed target path. PR merge SHA and branch head SHA are distinct.

Runner `source_identity` is observer checkout commit/tree/dirty state, not binary provenance. `features_note` is only a label. Configurations/output in RUNNER_TEMP preserve clean checkout identity. Each launch carries executable path, before/after hashes, actual environment overrides, status/signal/error, raw stdout/stderr, parsed producer and whole-child time. Failed comparisons retain earlier successful samples and the failed launch receipt.

Build-host observations are not admission of a later measurement host. The measurement step records current CPU/OS, pages, THP and raw MemAvailable before children, requiring Linux x86_64/base4096/THP2097152/madvise and>=8 GiB MemAvailable. Missing facts fail; unavailable never becomes0/false. MemAvailable is not a guarantee of cgroup headroom. No host tuning or continuous swap/PSI/cgroup observation is claimed. A/A readability replaces the old swap/pressure veto, not a proof that host/quota bias is absent.

## Timing and observation scopes

| Fact | Actual field | Scope |
|---|---|---|
| Cold payload | `row.cold_ns`, also `row.parsed_row.cold_ns` | allocation+touch+free |
| Cache payload | `row.parsed_row.cache_ns` | second-life actual cache hit |
| Whole child | `row.launch.child_wall_ns` | setup+payload+probes+trim+cleanup |
| OS evidence | `row.parsed_row.os.faults`, `peak_rss`, `end_rss`, `anon_huge_peak` | process observations; approximate RSS, live AnonHugePages |
| Capture storage | `capture_slot_storage_bytes`, `capture_total_storage_bytes`, `capture_tls_context_bytes_per_thread` in parsed producer | observer storage, not removed from cost |

CACHE OS/whole-child observations cover cold+cache+trim together, not per-life memory attribution. Null remains unavailable. Faults/AnonHugePages corroborate activity, not exact advice attribution, physical maxima or standalone speed.

## Pure reducer

`j13LimitedReport` accepts `{ evidence_class, build_receipt, measurement_host, ab: { document, facts: { cell, regime } }, control: { document, facts: { cell, regime } } }`. Supply unchanged actual JSON runner documents, actual separate receipt and actual measurement-host facts. No fabricated legacy root provenance, flattened metric clones or resolved-feature labels.

Measurement-host facts: `scope: "measurement_host"`, `os`, `arch`, `cpu_info`, `base_page_bytes`, `thp_page_bytes`, `thp_mode`, pre-launch `captured_before`, `memory_available_bytes`. Swap/pressure booleans are no longer admission requirements; do not fabricate false. The reducer validates supplied facts, not their authenticity or collection.

Registered statistic: exactly 12 blocks, minimum of 2 observations per slot/block, exp(mean of 12 paired log Always/Off ratios), two-sided 95% paired-t CI, df 11, critical 2.201. A/A uses actual slots even when labels match. CACHE selects `cache_ns`; first-life cold is descriptive. Runner block means/difference-t summaries are descriptive only. Reducer emits CSV/Markdown without a product GO/veto; A/A interval containing 1 is readability only. Task 112 completed pure tooling; task 111 owns actual measurements/report.

## Manual CI and bounds

Use the existing `CI` workflow at a reviewed immutable commit/ref. Inputs `j13_study` and `j13_smoke` are mutually exclusive, default false; no PR, push or schedule starts measurements. Dispatch shape: `gh workflow run ci.yml --ref <reviewed-ref> -f j13_smoke=true`; registered invocation uses `-f j13_study=true`. Obtain actual run ID and immediately background `gh run watch <run-id>`.

Build and component tests are outside the 30-minute measurement step. One child at a time, <=2 actors, <=2 GiB payload, existing<=120s per-child timeout. The runner requests SIGKILL for its direct child only; the actual judge launches no children. OS scheduling/uninterruptible waits mean this is not an absolute timeout guarantee or a process-tree manager. The prior shared256 MiB evidence bound is superseded, not claimed enforced by128 KiB child buffers. Record evidence sizes and apply repository raw-storage policy. Always upload available preflight/raw/failure artifacts, never manufacture missing data.

Smoke: cycle8 MiB and cache16 MiB, one A/B and one A/A block each,16 children. Numbers discarded before dispatch. Strict reducer sample-count refusal is expected and saved, not a fabricated measured report. Full-shape synthetic fixtures verify successful generation; the registered run exercises actual measured CSV/Markdown.

Before registered dispatch, assess 960-child runtime against 20 minutes using actual smoke timings with stated assumptions and incomplete large-cell coverage. Stop and ask if it cannot fit or the estimate is unsupported. Hard timeout remains30 minutes. Registered study:10cells,12ABBA+12AA each,960 children sequentially. One full repeat allowed only after interruption before all cells complete; preserve both attempts separately, never splice cells or rerun for cleaner results. After all measurement cells complete, report/upload recovery does not authorize another payload run.

The owner accepts up to2 MiB per touched2 MiB region of an advised block; report it, do not renew the veto. Report materially new regressions for owner review. Present actual report before the owner's default decision. Auto/budget later, builder/scalar paused.

## Source budget and Small-path verification

Historical source tally900/900 becomes **899/900** after deleting only the obsolete reducer predicate:470 helper+48 judge+21 parser+101 runner+259 full-physical-reducer lines. No minification/hidden implementation. Tests, ordinary YAML/JSON invocation declarations and documentation are not places to conceal new implementation.

The original standalone 64-byte1024 alloc/free Small-Ir prerequisite is superseded. Task 107 owns changed-default Small-path cost verification through existing IAI/zero-cost gates; no unperformed standalone result or new benchmark is claimed. Correctness item264 stays OPEN.

CI-only process/jq/native tests remain CI-only. Pure synthetic fixtures are not measured evidence. CI definitions alone do not prove execution or performance.
