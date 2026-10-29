# G1-SC-4: growth time against mimalloc — the quadratic discovery scan costs time from ~512 MiB (measurement only)

No production change. Pre-registration: `docs/perf/g1_scan/SC4_PREREG.md`
(committed `db63643f` before any data; the run's identity — commit, clean
tree, binary sha256, `rustc -vV`, features, `RUSTFLAGS` — is in
`docs/perf/g1_scan/run_identity_sc4.txt`). Entry point: the static
`SeferAlloc` called through `GlobalAlloc` on one thread (the routed path the
`#[global_allocator]` uses) against `mimalloc::MiMalloc`, one binary, one
process per run, 10 ABBA quads per point (20 runs per arm).

## Wall clock (Windows, i7-11800H; `scripts/g1_sc4_report.mjs`, `sc4_report.log`)

| Size | S | n | sefer ns/alloc median (min) | mimalloc ns/alloc median (min) | r = sefer / mi | 95 % interval | runs |
|---:|---:|---:|---:|---:|---:|---|---:|
| 64 B | 8 | 524288 | 16.06 (13.50) | 43.57 (27.55) | 0.369 | [0.333, 0.576] | 20 + 20 |
| 64 B | 16 | 1048576 | 20.15 (16.00) | 39.62 (27.04) | 0.509 | [0.390, 0.622] | 20 + 20 |
| 64 B | 32 | 2097152 | 24.49 (20.19) | 35.82 (28.60) | 0.684 | [0.580, 0.857] | 20 + 20 |
| 64 B | 64 | 4194304 | 33.49 (27.61) | 42.61 (29.21) | 0.786 | [0.697, 0.956] | 20 + 20 |
| 64 B | 128 | 8388608 | 53.87 (43.91) | 38.54 (28.69) | 1.398 | [1.215, 1.623] | 20 + 20 |
| 64 B | 256 | 16777216 | 179.36 (99.63) | 38.86 (30.48) | 4.615 | [3.944, 5.082] | 20 + 20 |
| 1024 B | 8 | 32768 | 100.28 (71.87) | 272.35 (233.56) | 0.368 | [0.303, 0.416] | 20 + 20 |
| 1024 B | 16 | 65536 | 143.19 (94.29) | 388.34 (246.94) | 0.369 | [0.288, 0.478] | 20 + 20 |
| 1024 B | 32 | 131072 | 141.40 (103.39) | 398.50 (254.70) | 0.355 | [0.311, 0.434] | 20 + 20 |
| 1024 B | 64 | 262144 | 179.70 (140.31) | 445.76 (268.70) | 0.403 | [0.347, 0.518] | 20 + 20 |
| 1024 B | 128 | 524288 | 268.20 (203.70) | 439.34 (302.40) | 0.610 | [0.530, 0.720] | 20 + 20 |
| 1024 B | 256 | 1048576 | 536.44 (361.63) | 345.40 (262.56) | 1.553 | [1.382, 1.771] | 20 + 20 |

- 64 B: SeferAlloc per-allocation growth S 64→256 ×5.36, mimalloc ×0.91 → quadratic VISIBLE in time; loss against mimalloc from S = 128
- 1024 B: SeferAlloc per-allocation growth S 64→256 ×2.99, mimalloc ×0.77 → quadratic VISIBLE in time; loss against mimalloc from S = 256

ns per allocation of the growth phase (all blocks live); `r` = SeferAlloc
median / mimalloc median with a 95 % bootstrap interval; summary CSV
`docs/perf/G1_SC4_GROWTH_TIME_summary.csv`, raw lines
`docs/perf/g1_scan/raw/sc4_growth.jsonl`.

Up to 64 segments (256 MiB) SeferAlloc is 1.3–2.8× faster than mimalloc on
this shape (mimalloc's first-touch cost dominates both small heaps). From
there SeferAlloc's per-allocation time grows with the heap — ×5.4 (64 B) and
×3.0 (1 KiB) from 64 to 256 segments, while mimalloc's stays flat or drops —
and it loses: 64 B from 128 segments (512 MiB, r = 1.40) and 256 segments
(1 GiB, r = 4.62, 179 against 39 ns per allocation); 1 KiB at 256 segments
(1 GiB, r = 1.55).

## Mechanism share (WSL callgrind, growth 64 B; `scripts/g1_sc4_cg_share.mjs`)

| Arm | S | n | Ir/alloc (excl. probe count) | discovery-scan self Ir/alloc | share |
|---|---:|---:|---:|---:|---:|
| sefer | 8 | 524288 | 156.9 | 23.9 | 15.2 % |
| sefer | 64 | 4194304 | 312.3 | 117.3 | 37.6 % |
| mi | 8 | 524288 | 74.3 | 0.0 | 0.0 % |
| mi | 64 | 4194304 | 73.4 | 0.0 | 0.0 % |

Self Ir of the discovery-scan files (`find_segment/scan.rs`,
`remote_bitmap/bitmap_scan.rs`, `segment_table/active_kind_index.rs`; the
scan is inlined, so it is attributed by file, not by a `linear_scan`
symbol) per allocation, excluding the probe's own post-phase segment count.
From 8 to 64 segments SeferAlloc's Ir per allocation doubles (156.9 →
312.3) and the scan's share grows from 15 % to 38 %; mimalloc stays at
≈ 74 Ir. Listings and the build script: `docs/perf/g1_scan/sc4_cg/`.

## Consequence

SC-6 (the fix of the routed scan) has a wall-clock target: growth of an
all-live heap past ~128 segments, where SeferAlloc now loses up to 4.6× to
mimalloc. The fix must be judged on these points (and on churn/pair/MT,
where it must not lose).
