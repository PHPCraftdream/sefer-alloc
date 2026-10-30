# J1-CH — why the J1 judge shows losses that the paired bench:table does not (measurement only)

**Runtime changes: none.** This report attributes the J1 losses and records
the free-path candidates measured against them: FG-1/A, A-2, F-1 and T-1. FG-1/B1
has its own report, `docs/perf/FG1_B1_COLD_BIND_BYREF_PREREG.md`; F-1 and T-1
have their pre-registrations with verdicts, `docs/perf/F1_LEAN_FREE_GUARD_PREREG.md`
and `docs/perf/T1_TINY_CLASS_INDEX_PREREG.md`.

## 1. The J1 churn kernels are a cross-class load

The J1 judge (`examples/b4_interleave_judge/`, kernels in
`examples/_shared/b4_interleave_kernels.rs`) keeps one slot ring for churn16,
churn64 and churn256. A churn16 portion therefore frees blocks of 64 or 256 B
that the other kernels left there and allocates 16 B. After K7
(`docs/perf/b4_ablate/a6/a6_judge.csv`) J1 reports t_mi/t_sefer 0.655 for
churn16, 0.785 for churn256, 0.824 for pair16, 0.831 for bulk16k and 0.838 for
churn64. The paired bench:table churn cells are single-class, and there sefer is
at par or faster.

## 2. Ticks per operation by regime (probe `b4_pmu_probe inter`, WSL, i7-11800H)

`inter` runs both allocators in one process in ABBA portions of 128 operations
(512 for pair), timed with rdtsc. Scenarios: x1664 = 16 B portions alternating
with 64 B over one ring of 256 slots; x16256 and x64256 are the same with 256 B;
c16 and c64 are single-class; pair16 and pair64 use live = 1.
Derived by `node scripts/j1ch_tables.mjs docs/perf/t1/probe_inter_base_vs_t1.csv a2base docs/perf/j1ch docs/perf/j1ch/fg1a_base.csv docs/perf/j1ch/fg1a_a1.csv`
(base arm, base = `origin/main` `dfb8c143`):

| Scenario | runs | sefer ticks/op | mimalloc ticks/op | sefer/mi |
|---|---:|---:|---:|---:|
| x1664 | 20 | 23.17 | 17.08 | 1.353 |
| x16256 | 20 | 24.41 | 17.55 | 1.399 |
| x64256 | 20 | 24.28 | 17.44 | 1.397 |
| c16 | 20 | 21.88 | 36.53 | 0.594 |
| c64 | 20 | 21.65 | 35.86 | 0.597 |
| pair16 | 20 | 21.33 | 17.19 | 1.243 |
| pair64 | 20 | 21.65 | 17.80 | 1.207 |

Median over runs of the base arm; sefer/mi = median of the per-run in-process ratio.


## 3. Instructions per operation (callgrind, `docs/perf/j1ch/cg_incl.*.txt`)

| Scenario | ops per arm | sefer free | sefer alloc | sefer pair | mimalloc free | mimalloc alloc | mimalloc pair |
|---|---:|---:|---:|---:|---:|---:|---:|
| x1664 | 612000 | 70.1 | 41.7 | 111.9 | 23.0 | 73.1 | 96.1 |
| c16 | 612000 | 68.0 | 40.1 | 108.2 | 23.0 | 211.4 | 234.4 |
| pair16 | 2148000 | 68.0 | 40.0 | 108.0 | 25.2 | 44.7 | 69.9 |

Inclusive Ir of the `#[inline(never)]` shims divided by the arm's ops (callgrind, one run per scenario).

Sefer's cost per operation is nearly constant (≈108–112 Ir, ≈21–24 ticks). The
mimalloc cost depends strongly on the regime: it is cheap when frees and allocations
hit different classes, and expensive on single-class churn, where sefer is 1.7×
faster. The callgrind cache and branch models (`docs/perf/j1ch/br_incl.*.txt`:
52 571 conditional mispredicts over the whole x1664 run) show no miss or mispredict source of the gap; WSL perf event sampling
(`ld_blocks_partial.address_alias`, `machine_clears.memory_ordering`, dTLB;
screening only, raw not kept) agrees. The lever is the instruction count
of free: 68–70 Ir against 23–25.

## 4. Free-path candidates measured against it

- **FG-1/A — the free class from the state byte** (tag `archive/fg1-a-state-class-measured`).
  This is a K2 variant: one bench:table run per arm, below (`docs/perf/j1ch/fg1a_*`).
  Every churn cell is 17–26 % slower, because the byte that the previous alloc
  just wrote becomes the index of the magazine count load. **NO-GO.**
| Cell | sefer ns base | sefer ns FG-1/A | Δ sefer | sefer/mi base | sefer/mi FG-1/A |
|---|---:|---:|---:|---:|---:|
| direct 16 B | 7.28 | 7.32 | +0.7 % | 1.536 | 1.531 |
| direct 64 B | 6.98 | 6.93 | -0.7 % | 1.100 | 1.076 |
| direct 256 B | 7.81 | 7.81 | +0.0 % | 0.982 | 0.976 |
| direct 1024 B | 10.25 | 10.16 | -1.0 % | 0.607 | 0.594 |
| churn 16 B | 5.76 | 7.03 | +22.0 % | 0.992 | 1.200 |
| churn 64 B | 5.62 | 6.93 | +23.5 % | 0.871 | 1.084 |
| churn 256 B | 5.86 | 6.98 | +19.2 % | 0.500 | 0.753 |
| churn 1024 B | 6.15 | 7.18 | +16.7 % | 0.301 | 0.435 |
| churn_write 16 B | 5.81 | 7.32 | +26.0 % | 1.009 | 1.271 |
| churn_write 64 B | 5.96 | 7.08 | +18.9 % | 0.984 | 1.208 |
| churn_write 256 B | 6.01 | 7.42 | +23.6 % | 0.631 | 0.859 |
| churn_write 1024 B | 6.64 | 7.81 | +17.6 % | 0.331 | 0.404 |

One run per arm (screening, not a paired verdict).

- **A-2 — a plain `encode(c, LIVE)` store instead of the state-byte RMW at magazine
  issue** (tag `archive/a2-issue-store-measured`). Probe pairs
  (`docs/perf/j1ch/a2_probe_inter.csv`, 6 ABBA repeats), derived by
  `VBASE=a2base VX=a2 node scripts/g1_probe_pair_table.mjs docs/perf/j1ch/a2_probe_inter.csv`:

| Scenario | sefer before | sefer after | Δ sefer (median of pairs) | pairs a2 faster | sefer/mi before | sefer/mi after | class before → after |
|---|---:|---:|---:|---:|---:|---:|---|
| x1664 | 28.88 | 26.62 | -6.1 % | 10/12 | 1.348 | 1.331 | LOSS → LOSS |
| x16256 | 31.31 | 29.35 | -2.3 % | 8/12 | 1.387 | 1.376 | LOSS → LOSS |
| x64256 | 29.70 | 29.36 | +2.6 % | 5/12 | 1.381 | 1.379 | LOSS → LOSS |
| c16 | 27.16 | 26.73 | +4.5 % | 3/12 | 0.584 | 0.584 | win → win |
| c64 | 26.37 | 24.63 | -2.5 % | 7/12 | 0.591 | 0.590 | win → win |
| pair16 | 25.40 | 25.33 | +0.6 % | 6/12 | 1.249 | 1.231 | LOSS → LOSS |
| pair64 | 25.09 | 24.98 | +1.4 % | 3/12 | 1.190 | 1.196 | LOSS → LOSS |

sefer = median ticks/op (TSC) over runs; Δ = median over ABBA pairs of a2/base − 1 (negative = A-2 faster); sefer/mi = median of the per-run in-process ratio; class: < 0.97 win, > 1.03 LOSS, else par.

  The in-process ratio sefer/mi does not move beyond noise (x1664 1.348 → 1.331),
  so the variant was not taken to a paired bench:table. **NULL.**
- **F-1 — lean free guard: the metadata-offset reject moves to the mismatch branch.**
  **NO-GO** by its pre-registered rule: 2 WIN cells, 4 LOSS cells, direct 16 B
  +2.4 % in both pairs (`docs/perf/F1_LEAN_FREE_GUARD_PREREG.md`).
- **T-1 — linear class index up to 80 B.** **NO-GO** by its pre-registered rule:
  4 WIN cells (churn 16/64 B −3…−6.5 %) and 3 LOSS cells (warm batch 16/64 B +5…+9 %).
  On mixed sizes across 80 B the size branch mispredicts (+35 %)
  (`docs/perf/T1_TINY_CLASS_INDEX_PREREG.md`).

## 5. Reading

Micro-edits of the free fast path move single cells by 1–6 %, against a layout
noise floor of 1.5–5 %. The J1 cross-class gap needs about −25 % of the
alloc+free instructions. Next trigger: a free-path design that removes
≥ 20 Ir without weakening any check, worked out with @ox and the rush reviewer
before any prototype.
