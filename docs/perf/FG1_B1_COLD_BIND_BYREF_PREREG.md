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
