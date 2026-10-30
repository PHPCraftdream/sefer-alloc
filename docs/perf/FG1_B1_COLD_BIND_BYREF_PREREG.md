# FG-1 / B1 — cold TLS bind takes the config by reference: pre-registration

Written and committed before the confirmatory data. The screening run
(`.rush`-local, not evidence) motivated it; its numbers are not used here.

## Change

`tls_heap::bind_slow_tagged_with_config` takes `&LargeCacheConfig` and
copies it inside (`#[cold] #[inline(never)]`). The call site used to pass the
0x68-byte config by value, so every `alloc` — hot path included — reserved a
0x70-byte stack frame for that copy (`sub $0x70,%rsp` / `add $0x70,%rsp`,
seen in WSL `objdump` of `examples/b4_pmu_probe` `sefer_alloc`). After the
change the hot path keeps only `push %rbx` / `pop %rbx`. No behaviour
change: the cold bind receives the same value.

## Measurement

`node scripts/bench-table-paired.mjs --features production`, aligned code
(`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`),
one build per arm, order base, b1, b1, base (two pairs). Base = `origin/main`
production (worktree `g1-dz2`, production bits identical to `c4b1ceeb`),
b1 = this branch. Per cell: ratio r = SeferAlloc / mimalloc from the same run.

## Rule (derived by `scripts/fg1_b1_verdict.mjs`)

- PASS requires the warm bulk 16 B ratio to be lower in BOTH pairs.
- FAIL if any cell's ratio is higher by more than 3 % in both pairs.
- Otherwise PASS.

Mechanism check: WSL callgrind Ir per pair of the bulk kernel must not rise
(screen: 157.0 → 154.8).

## Verdict (derived by `node scripts/fg1_b1_verdict.mjs docs/perf/fg1/b1 docs/perf/FG1_B1_COLD_BIND_BYREF_summary.csv`)

| Cell | ratio base→b1, pair 1 | Δ | ratio base→b1, pair 2 | Δ | SeferAlloc ns (base1, b1, b1, base2) | Rule |
|---|---:|---:|---:|---:|---|---|
| direct 16 B | 1.562 → 1.526 | -2.3 % | 1.531 → 1.490 | -2.7 % | 7.32, 7.23, 7.13, 7.32 | ok |
| direct 64 B | 1.061 → 1.061 | -0.0 % | 1.538 → 1.077 | -30.0 % | 6.79, 6.84, 6.84, 9.91 | ok |
| direct 256 B | 0.975 → 0.994 | 1.9 % | 1.000 → 0.975 | -2.5 % | 7.76, 7.76, 7.62, 8.01 | ok |
| direct 1024 B | 0.596 → 0.598 | 0.3 % | 0.592 → 0.599 | 1.2 % | 10.16, 10.30, 10.06, 10.40 | ok |
| churn 16 B | 0.983 → 1.000 | 1.7 % | 0.983 → 0.983 | -0.0 % | 5.76, 5.81, 5.71, 5.76 | ok |
| churn 64 B | 0.886 → 0.879 | -0.8 % | 0.848 → 0.879 | 3.6 % | 5.66, 5.66, 5.66, 5.71 | ok |
| churn 256 B | 0.628 → 0.641 | 2.2 % | 0.488 → 0.585 | 19.9 % | 5.76, 5.76, 5.71, 5.81 | ok |
| churn 1024 B | 0.343 → 0.352 | 2.6 % | 0.321 → 0.341 | 6.3 % | 5.86, 5.86, 5.86, 6.25 | ok |
| churn_write 16 B | 1.025 → 1.017 | -0.8 % | 1.008 → 1.017 | 0.9 % | 5.96, 5.86, 5.81, 5.86 | ok |
| churn_write 64 B | 0.968 → 0.984 | 1.7 % | 0.976 → 0.959 | -1.7 % | 5.81, 5.91, 5.66, 5.86 | ok |
| churn_write 256 B | 0.655 → 0.684 | 4.4 % | 0.676 → 0.697 | 3.2 % | 6.10, 5.91, 6.30, 6.01 | FAIL: ratio higher by > 3 % in both pairs |
| churn_write 1024 B | 0.317 → 0.320 | 1.0 % | 0.325 → 0.346 | 6.5 % | 6.35, 6.30, 6.74, 6.64 | ok |

Verdict: FAIL

**NO-GO as registered.** The bulk 16 B gain repeats in both pairs
(−2.3 % and −2.7 %, SeferAlloc 7.32 → 7.23 / 7.13 ns/pair). But churn+write
256 B moves by more than 3 % in both pairs. In that cell SeferAlloc's own
time does not get worse (base 6.10 / 6.01, b1 5.91 / 6.30 ns/pair); its
ratio rose because mimalloc ran faster in the b1 runs. The base of pair 2
also has an outlier in another cell (direct 64 B, 9.91 ns). The rule is not
re-read after the data. The code stays under tag
`archive/fg1-b1-cold-bind-byref-measured`.

Raw data: `docs/perf/fg1/b1/` (`.jsonl.gz`, CSVs, manifests). The base
runs were built in worktree `g1-dz2`, whose production bits equal `origin/main`
(its only source change is `bench-internals`-gated). The b1 runs were built
from this branch's commit `3e33a53b` (`src/` tree identical to `4ab95520`).
