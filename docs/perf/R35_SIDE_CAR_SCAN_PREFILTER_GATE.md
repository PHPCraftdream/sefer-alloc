# R35 sidecar scan prefilter measurement

Direct SeferAlloc GlobalAlloc alloc/dealloc -> HeapCore; SeferAlloc is NOT installed as the process global allocator. Same-binary baseline-vs-candidate runtime modes, not two source revisions. Arithmetic means of uncounted runs; runtime-enabled oracle controls collected separately. Ratios are candidate mean / base mean. Primitive counts are absent (not zero) in uncounted samples. No verdict.

## Production disposition

The candidate branch, TLS selector and counters require both `bench-internals` and `r18_sidecar_scan_bench`. Ordinary production and ordinary `perf_gate_iai` builds do not set that cfg and compile the original unconditional `swap(0, AcqRel)` with no R18 TLS read. This is a measurement-only candidate; no production promotion is claimed and item 82 remains OPEN.

Raw IAI metric lines use `current|baseline`; the generated table and CSV use only the first, current-run count. The second column is IAI's stored reference for the same benchmark name, not the opposite runtime-mode arm.

| Arm | Base SHA | Source SHA256 | Features | Build cfg | Rustflags | Workspace wrapper | CPU | OS | Samples |
|---|---|---|---|---|---|---|---|---|---:|
| base | 8d08825a9a92192763bdc12e1bfb0f9c25af6003 | 6eff394cf2985b307e9f9713a532ed0b195797e7fc785dab3ed95189ccdc5e14 | production bench-internals internals | r18_sidecar_scan_bench | --cfg r18_sidecar_scan_bench | RUSTC_WORKSPACE_WRAPPER='' | 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz | Linux 6.18.33.2-microsoft-standard-WSL2 x86_64 GNU/Linux | 3 |
| candidate | 8d08825a9a92192763bdc12e1bfb0f9c25af6003 | 6eff394cf2985b307e9f9713a532ed0b195797e7fc785dab3ed95189ccdc5e14 | production bench-internals internals | r18_sidecar_scan_bench | --cfg r18_sidecar_scan_bench | RUSTC_WORKSPACE_WRAPPER='' | 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz | Linux 6.18.33.2-microsoft-standard-WSL2 x86_64 GNU/Linux | 3 |

## Publication counted boundary

IAI setup: owner allocates 34 blocks, one joined foreign producer GlobalAlloc-deallocates all 34; counted wrapper: two owner GlobalAlloc alloc/dealloc rounds with routed discovery and prepublished sidecar drain; producer creation/join/deallocation are excluded. The native concurrent-publication regression runs the baseline mode; the candidate-specific native test exercises actual empty/nonempty cuts but not a concurrent race. Candidate interleavings appear only in the reduced Loom shadow, not an actual-type lifecycle proof. Stable uncounted Ir is required for every scenario.

| Scenario | Arithmetic mean base Ir | Arithmetic mean candidate Ir | Candidate/base Ir | Arithmetic mean base cycles | Arithmetic mean candidate cycles | Candidate/base cycles |
|---|---:|---:|---:|---:|---:|---:|
| r18_scan_growth | 6262375 | 6262407 | 1.0000051098824327 | 7919311 | 7600320 | 0.9597198544166279 |
| r18_scan_cycles | 19406959 | 19406991 | 1.0000016488930594 | 24134915 | 23130484 | 0.9583826584846062 |
| r18_scan_publication | 6821830 | 6821862 | 1.0000046908234301 | 8425351 | 8071065.333333333 | 0.9579500406966229 |

| Arm | Scenario | Sample | Routed scans | Visited | Empty | Exchanges | Nonempty exchanges | Ir | Estimated cycles |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| base | r18_scan_growth | 1 | 99 | 319063 | 319063 | 319063 | 0 | 9137939 | 12394093 |
| base | r18_scan_cycles | 1 | 275 | 1004503 | 1004503 | 1004503 | 0 | 28451855 | 38202603 |
| base | r18_scan_publication | 1 | 92 | 355593 | 355559 | 355593 | 34 | 10027448 | 13416665 |
| base | r18_scan_growth | 2 | 99 | 319063 | 319063 | 319063 | 0 | 9137939 | 12394093 |
| base | r18_scan_cycles | 2 | 275 | 1004503 | 1004503 | 1004503 | 0 | 28451855 | 38202603 |
| base | r18_scan_publication | 2 | 92 | 355593 | 355559 | 355593 | 34 | 10027448 | 13416665 |
| base | r18_scan_growth | 3 | 99 | 319063 | 319063 | 319063 | 0 | 9137939 | 12394093 |
| base | r18_scan_cycles | 3 | 275 | 1004503 | 1004503 | 1004503 | 0 | 28451855 | 38202603 |
| base | r18_scan_publication | 3 | 92 | 355593 | 355559 | 355593 | 34 | 10027448 | 13416665 |
| candidate | r18_scan_growth | 1 | 99 | 319063 | 319063 | 0 | 0 | 9137866 | 12074941 |
| candidate | r18_scan_cycles | 1 | 275 | 1004503 | 1004503 | 0 | 0 | 28451777 | 37198004 |
| candidate | r18_scan_publication | 1 | 92 | 355593 | 355559 | 34 | 34 | 10027392 | 13060056 |
| candidate | r18_scan_growth | 2 | 99 | 319063 | 319063 | 0 | 0 | 9137866 | 12074941 |
| candidate | r18_scan_cycles | 2 | 275 | 1004503 | 1004503 | 0 | 0 | 28451777 | 37198004 |
| candidate | r18_scan_publication | 2 | 92 | 355593 | 355559 | 34 | 34 | 10027392 | 13060056 |
| candidate | r18_scan_growth | 3 | 99 | 319063 | 319063 | 0 | 0 | 9137866 | 12074941 |
| candidate | r18_scan_cycles | 3 | 275 | 1004503 | 1004503 | 0 | 0 | 28451777 | 37198004 |
| candidate | r18_scan_publication | 3 | 92 | 355593 | 355559 | 34 | 34 | 10027392 | 13060056 |

## Reproduction (repository root)

```sh
env RUSTC_WORKSPACE_WRAPPER='' 'node' 'scripts/r18-perf-gates.mjs' 'collect' 'docs/perf/r35_sidecar_scan_evidence_test_oracle_final' 'base'
env RUSTC_WORKSPACE_WRAPPER='' 'node' 'scripts/r18-perf-gates.mjs' 'collect' 'docs/perf/r35_sidecar_scan_evidence_test_oracle_final' 'candidate'
'node' 'scripts/r18-perf-gates.mjs' 'generate' 'docs/perf/r35_sidecar_scan_evidence_test_oracle_final'
```

Features: `production bench-internals internals`. Dedicated target: `r18_sidecar_scan_iai`. Build cfg: `r18_sidecar_scan_bench`. RUSTFLAGS: `--cfg r18_sidecar_scan_bench`. The collector explicitly sets `RUSTC_WORKSPACE_WRAPPER=''` for both compilation and measurement, disabling the workspace wrapper (not unsetting it). The collector unsets inherited CARGO_ENCODED_RUSTFLAGS for both compilation and measurement so the explicit RUSTFLAGS apply. On Windows, Windows Node launches Linux commands through WSL; Linux Node is not needed. For recollection, select a fresh evidence destination and use it consistently for both collect commands and generate; existing arm directories are never overwritten. Collection first uses cargo bench --no-run --message-format=json to locate the Linux executable, hashes it with sha256sum, requires the same executable SHA256 for both modes, and checks it before and after every measurement.

- base executable SHA256: bba8abef518c230096ce8d98e0001d25488b59457a75f47e1af356cfccf40dfc
- candidate executable SHA256: bba8abef518c230096ce8d98e0001d25488b59457a75f47e1af356cfccf40dfc

## Saved raw logs

- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_base_oracle_1.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_base_oracle_2.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_base_oracle_3.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_base_timing_1.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_base_timing_2.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_base_timing_3.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_candidate_oracle_1.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_candidate_oracle_2.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_candidate_oracle_3.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_candidate_timing_1.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_candidate_timing_2.log
- docs/perf/_raw_r35_sidecar_scan_test_oracle_final_candidate_timing_3.log
