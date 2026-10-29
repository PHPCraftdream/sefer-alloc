# G1-SC-4 — growth time against mimalloc: pre-registration

Written and committed before any SC-4 data. Measurement only (`bench`), no
production change. Follows `SC_PREREG.md`: SC-2 confirmed the quadratic
(`full_scan_slots` ×3.77–3.94 per doubling of S), which is that file's
condition for SC-4.

## Question

Does the quadratic discovery scan cost wall-clock time against mimalloc on
a growing all-live heap, and from which heap size?

## Probe and builds

`examples/g1_rhythm_probe.rs` `growth <size> <n> [mi]`: one thread
allocates `n` blocks of `size` bytes into a static allocator (SeferAlloc,
the routed `#[global_allocator]` path, or `mimalloc::MiMalloc`) and keeps
them all live; `phase_ns` times the allocation loop only. Timing build
`--features production` (no diagnostic counters), aligned code
(`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`),
release; the same binary runs both arms. Source: this branch's commit
carrying this file and `run_sc4.sh` / `scripts/g1_sc4_report.mjs`; binary
sha256 and `rustc -vV` go to `run_identity_sc4.txt` before the run.

## Points and protocol

| id | size | S (segments at the end) | n |
|---|---:|---|---|
| G64 | 64 B | 8, 16, 32, 64, 128, 256 | S · 65 536 |
| G1K | 1 KiB | 8, 16, 32, 64, 128, 256 | S · 4 096 |

One process per run. Per point, 10 quads in the order sefer, mimalloc,
mimalloc, sefer (ABBA) — 20 runs per arm; points in the table order, the
quads of one point back to back. Raw: one JSON line per run in
`raw/sc4_growth.jsonl`.

## Statistics and verdict (all derived by `scripts/g1_sc4_report.mjs`)

Per point and arm: median and minimum `phase_ns / n` (ns per allocation).
Per point: `r = median_sefer / median_mi` with a 95 % bootstrap interval
(10 000 resamples of the 20 + 20 runs, fixed seed).

- **Loss against mimalloc at S**: the interval of `r` lies entirely above
  1.05.
- **Quadratic visible in time**: SeferAlloc's per-allocation median grows
  by at least 1.5× from S = 64 to S = 256 while mimalloc's grows by at most
  1.2× (a scan cost linear in S per refill shows as growing per-allocation
  time; constant-cost allocation does not).
- **Onset**: the smallest S with a loss.

Each statement is reported per size (G64, G1K); none decides a production
change by itself — it decides whether SC-6 (the fix) has a wall-clock target
and at which heap sizes it must be judged.

## Mechanism share (Ir, WSL callgrind)

G64 at S = 8 and S = 64, sefer and mimalloc, the same probe built for Linux
with the same features: total Ir per allocation and the inclusive share of
`linear_scan` (sefer). Published, no threshold.
