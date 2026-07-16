# Cross-version wall-clock comparison — 0.2.1 → pre-round6 → current

**Measured 2026-07-16**, single Windows dev host, quick criterion profile
(`sample_size(10)`, short warm-up), `SeferAlloc` called directly through its
`GlobalAlloc` impl vs `mimalloc 0.1` vs `System`. This is an honest
**comparative** run, not a rigorous statistical suite — the host is noisy
(±15–20 %; the README and `docs/ALLOC_BENCH.md` both document that "every
column moves between runs"). The deterministic, noise-free gate remains the
instruction-count `perf_gate_iai` bench on Linux CI (`npm run iai`,
`docs/perf/IAI_BASELINE.md`).

## What this answers

"How does the current tree compare to the last published crate (0.2.1) and to
the tree immediately before the round6 optimization wave?" — i.e. it separates
**pre-round6 gains** from **the round6 wave's own effect** on wall-clock
throughput.

## Anchors

| Label | Ref | Commit | What it is |
|---|---|---|---|
| **0.2.1** | tag `sefer-alloc-v0.2.1` | `5d75bf3` | The version published on crates.io |
| **before-wave** | — | `57bf118` | `345fa9b~1` — the commit immediately before the first round6-wave commit |
| **now** | `main` | `f27d060` | After the whole round6 wave (P0-1..P0-4, medium classes) + its two regression fixes + the review-residuals commit |

## Methodology — same measurement code against each version's allocator

The benchmark **harness** evolved across these versions, so running "each
version's own harness" would conflate harness changes with allocator changes.
Instead, HEAD's canonical measurement driver (`scripts/bench-table.mjs` +
`benches/global_alloc.rs` + `benches/large_realloc.rs`, i.e. `npm run
bench:table`, with the R5-R3 methodology fixes: per-group TLS-heap reset +
arm-order rotation) was run against each anchor's allocator **source**:

- **now** and **before-wave** already carry a byte-identical harness (verified
  `git diff --stat` empty for the two bench files + the driver).
- **0.2.1** predates `bench:table` entirely, so the current harness was ported
  onto it — preserved as the reusable **`bench/0.2.1`** branch (commit
  `5edb3d9`, on top of the 0.2.1 tag). Three glue-only adaptations were needed
  for the old API (see "0.2.1 caveats" below); the measured workload/timing
  code is byte-identical.

Each anchor ran in its own git worktree with its own `CARGO_TARGET_DIR`, one
bench process at a time (16-thread host — overlapping runs would
cross-contaminate timings). Two interleaved rounds (A,B,C,A,B,C). Cells below
show **run1 / run2**.

**Read the vs-mimalloc ratio, not the raw ns across columns.** Within a single
run, `mimalloc`/`System` are measured in the same session as `SeferAlloc`, so
the ratio cancels per-run host drift; absolute ns from three *separate* runs
sit in three different host-load windows and are not directly comparable.
**Lower ns is better** (latency); for the large-alloc and MT families a higher
"×faster" is better.

## Results

### Cold direct (1 alloc + 1 free per op, no reuse — the "first touch" path)

SeferAlloc ns/op (vs mimalloc):

| Size | 0.2.1 | before-wave | now |
|---|---|---|---|
| 16 B | 45.2 / 36.7 (2.24× / 2.22× slower) | 49.9 / 34.6 (2.54× / 2.37× slower) | 48.2 / 45.4 (2.36× / 2.84× slower) |
| 64 B | 47.6 / 39.1 (1.99× / 2.03× slower) | 46.9 / 42.5 (1.86× / 2.35× slower) | 50.5 / 46.9 (2.04× / 1.97× slower) |
| 256 B | 47.9 / 41.2 (1.43× / 1.52× slower) | 49.4 / 38.5 (1.47× / 1.76× slower) | 54.4 / 50.5 (1.69× / 1.64× slower) |
| 1024 B | 46.2 / 42.0 (1.42× / 1.33× faster) | 52.0 / 38.3 (1.32× / 1.14× faster) | 50.1 / 47.4 (1.32× / 1.35× faster) |

### Churn, non-writing (1 free + 1 alloc per op, working-set reuse)

| Size | 0.2.1 | before-wave | now |
|---|---|---|---|
| 16 B | 27.3 / 22.8 (1.05× slower / 1.17× faster) | 30.1 / 20.0 (1.10× slower / 1.01× faster) | 27.2 / 23.6 (1.02× faster / 1.01× slower) |
| 64 B | 29.8 / 29.2 (1.04× faster / 1.03× slower) | 26.7 / 18.9 (1.18× / 1.17× faster) | 26.6 / 23.7 (1.24× / 1.29× faster) |
| 256 B | 49.9 / 52.2 (1.26× / 1.25× **slower**) | 26.0 / 18.6 (1.57× / 1.94× **faster**) | 27.4 / 22.3 (1.51× / 1.75× **faster**) |
| 1024 B | 54.8 / 51.2 (5.06× / 6.71× faster) | 27.9 / 20.7 (10.09× / 10.09× faster) | 29.0 / 25.5 (10.15× / 9.35× faster) |

### Churn + write (writes 16 B after each alloc — the realistic pattern, headline)

| Size | 0.2.1 | before-wave | now |
|---|---|---|---|
| 16 B | 22.5 / 26.0 (1.08× / 1.04× faster) | 24.2 / 19.6 (1.01× / 1.06× faster) | 25.3 / 27.2 (1.11× slower / 1.02× faster) |
| 64 B | 22.4 / 28.4 (1.33× / 1.01× faster) | 26.1 / 24.3 (1.08× / 1.29× faster) | 24.6 / 25.9 (1.18× / 1.13× faster) |
| 256 B | 39.8 / 27.1 (1.05× slower / 1.54× faster) | 25.4 / 23.3 (1.56× / 1.55× faster) | 25.5 / 27.1 (1.56× / 1.44× faster) |
| 1024 B | 35.9 / 45.5 (6.81× / 5.41× faster) | 27.9 / 28.5 (9.32× / 9.99× faster) | 27.8 / 29.1 (9.23× / 8.93× faster) |

### Churn + teardown (diagnostic — decommit/release/re-reserve inside the timed region)

| Size | 0.2.1 | before-wave | now |
|---|---|---|---|
| 16 B | 41.1 / 42.3 (1.46× / 1.49× slower) | 35.2 / 21.5 (1.11× / 1.20× slower) | 36.4 / 29.2 (1.18× / 1.17× slower) |
| 64 B | 42.8 / 46.5 (1.48× / 1.63× slower) | 35.9 / 23.1 (1.25× slower / 1.11× faster) | 36.3 / 32.6 (1.12× / 1.22× slower) |
| 256 B | 60.5 / 50.2 (1.76× / 1.27× slower) | 33.0 / 24.4 (1.10× / 1.02× faster) | 36.3 / 31.2 (1.11× slower / 1.19× faster) |
| 1024 B | 102.5 / 117.0 (1.76× / 1.75× slower) | 102.9 / 100.2 (1.65× / 1.86× slower) | 95.4 / 78.5 (1.57× / 1.20× slower) |

### Vec_push (geometric `Vec` growth, unscaled ns per closure)

| | 0.2.1 | before-wave | now |
|---|---|---|---|
| Vec_push | 1456 / 1246 (1.10× / 1.17× faster) | 1380 / 904 (1.11× / 1.14× faster) | 1476 / 1197 (1.00× / 1.28× faster) |

### Large alloc + free (`large_alloc_free`; SeferAlloc ns vs mimalloc µs — Sefer is ns-scale at every anchor)

| Size | 0.2.1 | before-wave | now |
|---|---|---|---|
| 4 MiB | 60 / 84 ns (21× / 18× faster) | 78 / 85 ns (18× / 15× faster) | **53 / 56 ns (17× / 23× faster)** |
| 16 MiB | 88 / 89 ns (19× / 19× faster) | 85 / 76 ns (19× / 20× faster) | 73 / 72 ns (20× / 23× faster) |
| 64 MiB | 110 / 86 ns (33× / 43× faster) | 68 / 77 ns (45× / 44× faster) | 77 / 77 ns (44× / 49× faster) |

### Realloc grow (unscaled per closure: `grow_geometric` = 16 doublings 64 B→4 MiB; `neighbour_pressure` = 8×256 KiB steps)

| Group | 0.2.1 | before-wave | now |
|---|---|---|---|
| grow_geometric | **3.05 / 2.74 ms (6.5× / 6.8× SLOWER than mimalloc)** | 13.0 / 12.2 µs (32× / 35× faster) | 13.3 / 14.3 µs (34× / 33× faster) |
| grow_neighbour_pressure | **3.11 / 2.54 ms (1.62× / 1.73× slower)** | 1.21 / 1.48 µs (~1200× / ~1080× faster) | 1.73 / 1.45 µs (~910× / ~1130× faster) |

## Per-family read — did the round6 wave (before-wave → now) move it beyond noise?

- **Churn (writing and non-writing), 16–1024 B:** flat before-wave → now — both
  runs at both anchors land in the same band (ratios overlap run-to-run). The
  big story is **0.2.1 → before-wave**: 256 B churn flipped from ~1.25× *slower*
  to ~1.6–1.9× *faster*, and 1024 B churn went from ~5–7× to a stable ~9–10×
  faster. Those gains predate the wave (Э6 / P6 — the M2 double-free oracle
  moved out of the block body) and are preserved by it.
- **Cold direct:** flat before-wave → now within noise; a hair worse at 16/256 B
  in one round but the two-run spread at each anchor covers the delta — not a
  confident regression. Cold tiny (16–64 B) remains the one place mimalloc leads
  (~2×), unchanged by round6 (which does not touch the cold carve path).
- **Churn + teardown 1024 B (the decommit-cost diagnostic):** the one family
  where before-wave → now moves consistently in both rounds: 103/100 ns →
  95/79 ns, ratio 1.65×/1.86× slower → 1.57×/1.20× slower. Direction
  (improvement) is consistent; magnitude sits inside this host's noise band — a
  *probable* modest win.
- **Large alloc/free 4 MiB:** before-wave → now improvement consistent in both
  rounds (78/85 ns → 53/56 ns, ~30–35 % faster); 16 MiB slightly better at now
  in both rounds; 64 MiB flat. (0.2.1's 4 MiB was too noisy to rank vs now.)
- **Realloc grow (both groups):** flat before-wave → now. The ~30×/~1000×
  **0.2.1 → before-wave** jump is the OPT-G in-place Large-realloc work that
  landed before the wave — at 0.2.1 realloc-grow was copy-and-free (ms-scale,
  *slower* than mimalloc); by before-wave it is in-place (µs-scale).
- **Vec_push:** flat everywhere within noise.

## Verdict

**All of the large wall-clock wins landed between 0.2.1 and the pre-round6 tree**
(OPT-G in-place realloc → ms-scale to µs-scale; Э6 churn → 256 B/1024 B
throughput), not in the round6 wave itself. **The round6 wave (before-wave →
now) is flat-to-slightly-better on wall-clock throughput and regresses no family
beyond host noise**, with probable modest wins on the 4 MiB large-alloc/free path
and the 1024 B teardown/decommit diagnostic.

This is expected and consistent with the wave's stated targets: round6 P0 work
went after **OS commit charge** (≈7.4× lower for the first heap — 33.3 MiB →
4.5 MiB), **cross-thread-free tail latency**, and **the SMALL_MAX fragmentation
cliff** (the opt-in `medium-classes` feature) — axes that `bench:table` does not
measure. Those wins are captured by the R6-OPT-A judges
(`examples/first_alloc_process.rs`, `benches/heap_fanin_persistent.rs`,
`benches/medium_size_sweep.rs`), not here. This cross-version run confirms the
wave delivered its targeted improvements **without costing throughput**.

## 0.2.1 caveats (not apples-to-apples — anchor A only)

before-wave and now use a byte-identical harness; 0.2.1's port needed three
glue-only adaptations (workload/timing untouched):

1. **All groups after the first:** `SeferAlloc::dbg_trim_current_thread` does not
   exist at 0.2.1, so the per-group TLS-heap reset (R5-R3 confound-1 fix) was
   stubbed to a no-op. 0.2.1's later groups (churn / churn-write / teardown)
   therefore see leftover TLS-heap state from earlier groups — the pre-R5-R3
   confound. This makes 0.2.1's churn columns partly a **methodology** artifact,
   not a pure allocator signal; read them as a loose lower bound, and lean on the
   0.2.1 → before-wave *direction* rather than the exact 0.2.1 ratio. The
   arm-order rotation (confound-2 fix) IS active at 0.2.1.
2. **`working_set_cycle`:** a `stats()` diagnostic `eprintln!` was removed (no
   `stats()` at 0.2.1) — timing-neutral, and the group is not in the tables above.
3. **`pool_cap_sweep`:** compiled out at 0.2.1 (needs `SmallSegmentPoolConfig` /
   `AllocCore` dbg seams absent then) — diagnostic-only, never in the comparison.

## Reproducing

- **now** / **before-wave:** `git worktree add ../sa-x <commit> && cd ../sa-x &&
  npm run bench:table`.
- **0.2.1:** `git worktree add ../sa-021 bench/0.2.1 && cd ../sa-021 && npm run
  bench:table` — the `bench/0.2.1` branch (commit `5edb3d9`) carries the current
  harness ported onto the 0.2.1 tag, so 0.2.1 stays re-measurable without
  redoing the port. The branch is local-only (not pushed).
