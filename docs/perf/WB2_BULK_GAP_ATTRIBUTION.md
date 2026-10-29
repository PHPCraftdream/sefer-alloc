# WB-2: where the warm-bulk 16 B gap to mimalloc goes (measurement only)

No runtime code changed. Two measurements after the WB-1 A-1 and B-1a
NO-GOs (`WB1_A1_CLASS_FOR_LAYOUT_VERDICT.md`,
`WB1_B1_DEPOT_INLINE_PREREG.md`), both chosen so that the allocator is not
rebuilt between the arms being compared.

## M1 — n-sweep on one binary

`examples/b4_pmu_probe.rs` `bulk <sefer|mi> <n> <size> <passes>`: allocate
`n` blocks, write 8 bytes into each, free them in allocation order; one
binary for both allocators (`main` `61b994fa`, `production`, aligned code;
identity in `wb2/m1_identity.txt`). `passes = 2^20 / n`, so every run does
about 2^20 pairs; 21 fresh processes per (arm, n), arm order alternating;
driven by `scripts/wb2_m1_bulk_sweep.mjs`, raw lines in
`wb2/sweep{16,64}.jsonl`, tables below are its output (ns per alloc+free
pair, minimum over the 21 processes, median in parentheses).

16 B ([sweep16.md](wb2/sweep16.md)):

| n | passes | sefer ns/pair min (median) | mimalloc ns/pair min (median) | sefer − mi (min) |
|---:|---:|---:|---:|---:|
| 16 | 65536 | 6.66 (7.08) | 5.31 (5.65) | 1.35 |
| 32 | 32768 | 6.94 (7.28) | 5.58 (5.81) | 1.36 |
| 64 | 16384 | 7.24 (7.88) | 5.53 (6.25) | 1.71 |
| 256 | 4096 | 7.83 (8.38) | 5.15 (5.56) | 2.68 |
| 1024 | 1024 | 7.83 (8.09) | 5.21 (5.38) | 2.62 |

64 B ([sweep64.md](wb2/sweep64.md)):

| n | passes | sefer ns/pair min (median) | mimalloc ns/pair min (median) | sefer − mi (min) |
|---:|---:|---:|---:|---:|
| 16 | 65536 | 6.52 (6.82) | 5.86 (6.10) | 0.66 |
| 32 | 32768 | 6.79 (7.13) | 5.80 (6.10) | 0.98 |
| 64 | 16384 | 7.21 (7.38) | 5.57 (5.84) | 1.64 |
| 256 | 4096 | 7.74 (7.96) | 5.21 (5.44) | 2.53 |
| 1024 | 1024 | 7.89 (8.17) | 6.47 (6.71) | 1.42 |

Reading (16 B): with `n = 16` the burst fits one magazine
(`TCACHE_CAP = 16`), so no depot park or reload runs; the gap there,
1.35 ns/pair, is the fast path. From `n = 16` to `n = 1024` SeferAlloc rises
by 1.17 ns/pair (6.66 → 7.83) while mimalloc stays within 5.15–5.58: the
depot traffic of a burst larger than the magazine (one park per 16 frees,
one reload per 16 allocs) costs about 1.2 ns per pair, a little under half
of the 2.62 ns gap at `n = 1024`. That is more than the ≈ 19 Ir/pair the
same traffic costs in callgrind would suggest at the fast path's rate (the
consultation's model, ≈ 0.05 ns per Ir, gives ≈ 0.95 ns) — consistent with
pipeline stalls at the magazine boundaries, not measured directly here.

## M2 — why A-1 lost 1.3 ns on churn+write 1024 B

Hypothesis (WB-1 consultation): 4K aliasing between the hot class-table
load and the benchmark's 16-byte writes, which for 1024 B blocks land only
at offsets `[0, 16)` modulo 1024 (blocks are carved at multiples of the
block size from a 4 MiB-aligned segment). Check without running anything:
disassemble the four measured `bench_table_paired.exe` (sha256 equal to the
`wb1_a1/check3*/` manifests) with `dumpbin -disasm`, resolve the
RIP-relative `lea` targets of the hot tables
(`scripts/wb2_m2_table_alias.mjs`, outputs `wb2/m2_*.syms.txt`), and take
the element the 1024 B class reads modulo 1024:

| Build | Table element read for 1024 B | (RVA + index) mod 1024 |
|---|---|---:|
| base (check 3 and 3b) | `SIZE2CLASS[63]` | 47 |
| a1 (check 3 and 3b) | `SIZE2CLASS_CEIL[64]` | **13** |
| base | `FREE_PARK_CAP[17]` | 814 |
| a1 | `FREE_PARK_CAP[17]` | 779 |

Only A-1's new load falls inside the written window `[0, 16)`, in both the
plain and the aligned build (code alignment moves `.text`, not `.rdata`
offsets) — consistent with the hypothesis and with churn non-write 1024 B
not moving. This is a static check; the run that would confirm it (moving
the benchmark's write within the cache line) was not made.

Consequence for later work: any hot load from a static table on the
alloc/free path can fall into such a window for power-of-two classes with
user writes at block start, and where it falls is decided by the final
binary's `.rdata` layout, including the user's code. A table change must
pin the hot tables' placement (e.g. an aligned container with a fixed
prefix) before its timing is meaningful.

## Not concluded

- Which of mispredicts, store-forwarding or plain instruction count makes
  the depot traffic cost 1.2 ns/pair (WSL hardware counters not tried).
- No GO/NO-GO: measurement only.
