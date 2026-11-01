# G1 WJ-1: wild judge, and one-call reserve+commit of aligned Windows spans — `perf(runtime)`

Two parts:

1. A new measurement judge, `examples/g1_wild_judge.rs` with the driver
   `scripts/g1_wild_judge.mjs`: GiB-scale heaps and seeded random mixed
   workloads, one allocator arm per process, bounded to 1 GiB live (256 MiB
   for `wild`/`phase`) and a hard 4 GiB peak RSS per run (the machine is
   shared). It found losses to mimalloc in every scenario on Windows.
2. A runtime change in `aligned-vmem` (`crates/aligned-vmem/src/os/windows.rs`):
   a fully committed span with `align > 64 KiB` is reserved and committed by
   one `VirtualAlloc(MEM_RESERVE | MEM_COMMIT)` at an aligned address found by
   an over-reserve probe (up to 4 attempts, then the previous two-call path).

Entry point under test: SeferAlloc's `GlobalAlloc` methods on a static
instance through `#[inline(never)]` shims (the `__rust_alloc` boundary); the
change sits below that, in every segment and Large-span reservation of the
real `#[global_allocator]`.

## 1. Findings of the wild judge (before the change)

Windows 10, i7-11800H, scale 1, two ABBA rounds, min ns/op per arm
(`wj1/wild/before_scale1_table.md`, from `before_scale1_raw.jsonl`):
grow64 ×1.82, grow1k ×1.54, growmix ×1.42, regrow ×1.87, **large ×6.86**,
wild ×1.84, phase ×1.34 (sefer / mimalloc time). On Linux (WSL, scale 0.5,
`wj1/wild/linux_scale05_times.jsonl`) large and wild are sefer wins
(0.11 and 0.47), so those two losses are Windows-only.

A split timer (`WJ_SPLIT=1`) put the large loss on frees of blocks
> 256 KiB: 17.5 µs each against 0.19 µs for mimalloc. The segment counters
(`internals` build) showed ~320 reservations and ~309 releases for ~2 600
large allocations: each Large-cache miss later evicts a span, and the
eviction is a synchronous `VirtualFree(MEM_RELEASE)`.

## 2. Mechanism: the separate commit, not the release

`examples/g1_vfree_probe.rs` times the return of a span to the OS by
reservation shape (direct calls, 200 cycles, median;
`wj1/vfree_probe.jsonl`, table by `node scripts/wj1_report.mjs`):

| Shape | 36 MiB, 2 pages touched: setup / return µs | 36 MiB, all touched: return µs |
|---|---:|---:|
| `two_call` (reserve size + 4 MiB, commit the window; SeferAlloc's shape) | 34.1 / 273.1 | 3097 |
| `rereserve_two` (exact window, reserve then commit) | 35.4 / 273.8 | 3227 |
| `one_call` (`MEM_RESERVE \| MEM_COMMIT`) | 0.7 / 3.2 | 3394 |
| `va2` (`VirtualAlloc2`, 4 MiB alignment) | 1.2 / 4.2 | 3302 |
| `rereserve` (probe, release, one-call at the aligned address) | 2.3 / 4.4 | 3380 |

A region whose pages were committed by a separate `MEM_COMMIT` call costs
~270 µs to release at 36 MiB even with two pages touched; a region reserved
and committed in one call costs ~4 µs. `rereserve_two` isolates it: the
over-reserved head and tail are not the cost, the separate commit is. Fully
touched spans cost the same in every shape (freeing the pages dominates).
`rereserve` needs no `VirtualAlloc2` (no Windows-version floor, no runtime
symbol resolution) and is within 1 µs of it.

## 3. Wild judge A/B (Windows, scale 1)

Base `main` `f05ffb67` against the change, sefer arm 2 runs each, mimalloc 2
runs, min ns/op (`wj1/wild/ab_rereserve_raw.jsonl`, table and
`G1_WJ1_WINDOWS_SPAN_RESERVE_summary.csv` by `node scripts/wj1_report.mjs`):

| Scenario | base | re-reserve | mimalloc | re-reserve / base | re-reserve / mi |
|---|---:|---:|---:|---:|---:|
| large | 7648.9 | 1513.0 | 1197.4 | 0.198 | 1.264 |
| wild | 25.1 | 21.4 | 17.7 | 0.852 | 1.205 |
| phase | 18.4 | 17.4 | 17.5 | 0.948 | 0.994 |
| growmix | 226.5 | 216.6 | 147.7 | 0.956 | 1.466 |
| regrow | 175.6 | 175.3 | 118.9 | 0.998 | 1.474 |
| grow64 | 38.0 | 38.6 | 21.7 | 1.016 | 1.781 |
| grow1k | 258.1 | 308.9 | 232.0 | 1.197 | 1.332 |

grow1k's +19.7 % did not reproduce: three further base/re-reserve pairs gave
224.7 / 221.7, 247.9 / 246.4 and 258.6 / 255.8 ns/op (re-reserve not slower);
its spans are fully touched, where every shape costs the same.

## 4. Paired bench:table (control)

Four sets, base `f05ffb67` against `ff112904`; tables and
`G1_WJ1_WINDOWS_SPAN_RESERVE_bt_summary.csv` by
`node scripts/g1_magnitude_verdict.mjs docs/perf/wj1/bt s{N}base s{N}wj s{N}`.

| Set | order | sefer time gm | verdict |
|---|---|---:|---|
| s1 | base, x, x, base | −2.2 % | GO |
| s2 | x, base, base, x | +0.9 % | NO-GO (largest loss +4.2 %) |
| s3 | base, x, x, base | −2.6 % | GO |
| s4 | x, base, base, x | +1.8 % | NO-GO (largest loss +3.1 %) |

The sign follows the order: the arm that runs first in a set loses. The
paired script builds before its first run, so the first arm starts on a CPU
still hot from the build — an artifact of the protocol, not of this change.
Over the four sets the gm is −0.5 %; the warm bench:table loops reserve no
segments, so no effect is expected there, and none is seen beyond the order
artifact. The verdict of this change rests on the wild judge (§3), where its
mechanism is active.

## 5. MT control (malloc_macro, Windows, ABBA × 6)

`G1_WJ1_WINDOWS_SPAN_RESERVE_mt_summary.csv` by
`node scripts/as1_mt_table.mjs docs/perf/wj1/mt_malloc_macro_raw.txt`:
larson T=1/2/4 −0.9 / −6.5 / −2.0 %, mstress T=1/2/4 +1.5 / +3.5 / +12.2 %
(run medians; larson T=2's run-to-run spread is of the same order).

## 6. Tests

`crates/aligned-vmem/tests/windows_aligned_single_region.rs` (Windows CI job):
4, 8 and 36 MiB spans at 4 MiB alignment are one region —
`reservation_ptr() == as_ptr()`, `reservation_len() == len()` (the old shape
reserved `size + align` at a lower base), every page writable, and under
`bench-internals` every reservation counts as a single-region call.

## 7. Identity

- Base: `main` at `f05ffb6763f058fb9d878309675abcb2d6d3b53f`.
- Measured tree: `ff112904a274b00c2dff812307e0d8fc753602c2` (branch `g1-wild`).
- Host: Windows 10 x86_64, i7-11800H, 48 GiB (all Windows numbers); WSL2
  Ubuntu (Linux control).

## 8. What is left

- Growth: grow64 ×1.78, regrow ×1.47, growmix ×1.47. @ox's reading: most of
  the free gap is the synchronous release of emptied, fully touched segments
  (~0.7 ms each), the price of returning RSS (sefer ends at 30 MiB, mimalloc
  at ~1 GiB), plus magazine turnover when the depot is latched. A deferred
  release is an RSS policy decision.
- large ×1.26 and wild ×1.21 remain: Large-cache slot turnover (8 slots) and
  route registration per cached span.
