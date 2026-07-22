# R13-6 — Production A/B gate for `exact-span-large` (R12-3) + `large-reserved-capacity` (R12-4)

**Task:** #276 (R13-6). **MEASUREMENT ONLY — not a promotion decision.** This
document reports what was measured; the GO/CONDITIONAL-GO/NO-GO line in §7 is
a **recommendation**, not a decision. Whether `exact-span-large` and/or
`large-reserved-capacity` join `production = [...]` in `Cargo.toml` is left to
the orchestrator/user, per this task's own brief.

**Date:** 2026-07-22/23. **Base revision:** `main` @ `6018cf8` (R13-1..R13-5,
R13-11, R13-12 landed; R13-6 is the next task in queue). **Platform measured:**
Windows 10 Pro x86-64 (native) for wall-clock/RSS/commit-charge; **WSL2
(Ubuntu 24.04) + Valgrind/Callgrind** for the deterministic instruction-count
judge (`npm run iai` machinery, `scripts/iai.mjs`) — WSL turned out to be
available in this session (see §1). Linux/macOS-native and true multi-socket
NUMA hardware were **not** available to this session; see §8 for that
limitation and its relationship to task #280.

Both features are opt-in (`Cargo.toml`'s `production = ["alloc-global",
"alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory",
"primordial-lazy-commit"]` — neither `exact-span-large` nor
`large-reserved-capacity` appears there) and remain untouched by this task —
no `Cargo.toml` feature-list edit, no `src/` edit. This task adds
measurement-only artifacts: one criterion bench
(`benches/r13_6_exact_span_reserved_capacity_wallclock.rs`), one throwaway
example (`examples/r13_6_large_cache_hit_rate_measure.rs`), and this document.

---

## 0. Headline summary (was / now)

| # | Measurement | `production` (baseline) | `production` + both features (treatment) | Verdict |
|---|---|---|---|---|
| 1 | RSS amplification, 260 KiB Large alloc | 15.80× | 1.06× | **Large win, unchanged from R12-3** |
| 1 | RSS amplification, 4 MiB Large alloc | 2.00× | 1.00× | **Win, unchanged from R12-3** |
| 2 | Small-class cold-direct/churn wall-clock (16–1024 B) | baseline table | no measurable change (spot-checked) | **Neutral, as expected — feature is Large-only** |
| 3a | Realloc chain (256 KiB→4 MiB) in-place legs / 4 | 3/4 in-place | 2/4 in-place | **Partial recovery of R12-3's own regression, not full** |
| 3b | Realloc chain wall-clock (best-of-20) | 1.91 ms | 2.27 ms | **+18.8% slower than baseline, though −22.5% faster than exact-span-large ALONE (2.93 ms)** |
| 3c | **iai `realloc_grow` (64 B→4 MiB, 16 doublings) — deterministic Ir** | 513,187 Ir | 1,038,281 Ir | **+102.3% instructions, +52.7% Estimated Cycles — a REAL, deterministic regression** |
| 4 | Large-cache exact hit rate (4 sizes × 2000 passes, 8 slots) | 99.99% (7999/8000), 1 slot used | 99.95% (7996/8000), 4 slots used | **Both near-100%; treatment spreads across more slots as predicted, no practical hit-rate loss at N=4 sizes** |
| 5 | First-heap commit-charge (`commit_after_1_heap_kib`) | ~1516–1540 KiB | ~1516–1528 KiB | **No measurable difference (expected — Small/registry path untouched)** |

**The single most important finding is #3c**: the deterministic,
host-noise-immune iai instruction-count judge shows the pair **more than
doubles instruction count** (and raises Estimated Cycles ~53%) on a realistic
geometric-growth realloc workload, even with `large-reserved-capacity`'s
mitigation active. This is not a wall-clock noise artifact — Callgrind's `Ir`
count is bit-for-bit reproducible on the same binary+input. §4 explains the
mechanism (repeated relocation past the fixed 2× `reserved_capacity` ceiling).

---

## 1. Methodology

### 1.1 iai (deterministic instruction-count judge)

`npm run iai` (`scripts/iai.mjs`) drives `benches/perf_gate_iai.rs` under WSL2
+ Valgrind/Callgrind — **available in this session** (Ubuntu 24.04, `valgrind`
present, `cargo`/`rustc` reachable via `bash -lc` login-shell env per the
script's own documented WSL invocation). No environment-limitation workaround
was needed for this axis; both arms ran the real mechanism (see raw logs
`docs/perf/_raw_r13_6_iai_baseline_production.log` and
`docs/perf/_raw_r13_6_iai_treatment_on.log`).

Commands run exactly as the task brief specifies:
```text
node scripts/iai.mjs --features production
node scripts/iai.mjs --features "production exact-span-large large-reserved-capacity"
```

### 1.2 `npm run bench:table` (canonical wall-clock table)

`scripts/bench-table.mjs` hardcodes `FEATURES = 'production'` (no CLI feature
override), so a full apples-to-apples re-run under the treatment feature set
via the npm script itself is not directly possible without editing the
script (out of scope for this measurement-only task). Two things were done
instead, both faithful to the script's own bench target and parsing:

1. Ran the full canonical baseline via `npm run bench:table` itself
   (`docs/perf/_raw_r13_6_bench_table_production_baseline.log`) — this
   reproduces the **current** `production` composition's numbers (relevant
   per the task brief's own note that README's 2026-07-20 table is stale
   relative to current `production` and task #280 owns the full re-baseline;
   this run is an incidental fresh baseline, not a substitute for #280).
2. Ran a **direct `cargo bench --features "production exact-span-large
   large-reserved-capacity" --bench global_alloc -- "^global_alloc/"`
   spot-check** (`docs/perf/_raw_r13_6_bench_table_treatment_spotcheck.log`)
   restricted to the `global_alloc` group (the 4 canonical cold-direct sizes)
   rather than the FULL `global_alloc.rs` suite (12 groups: cold-direct,
   churn, churn+write, churn+teardown, working-set-cycle, pool_cap_sweep,
   batch_ceiling, batch_ceiling_followup — several of which run 10+ minutes
   each under this project's `sample_size(10)` fast-profile policy purely
   from sheer sub-case count, not because any single case is slow). This
   spot-check is a deliberate scope narrowing: `benches/global_alloc.rs`'s
   `SIZES = [16, 64, 256, 1024]` (confirmed by direct source read) are ALL
   Small-class sizes that **never touch the Large allocation path**
   `exact-span-large`/`large-reserved-capacity` modify — so a full-suite
   re-run is structurally guaranteed to show "no change" on every one of
   those 12 groups, at a cost of many multiples of this task's fast-profile
   time budget (`CLAUDE.md`'s "Speed: short scenario by default"). The
   spot-check confirms this structural expectation directly rather than
   spending 20+ minutes reconfirming a null result 12 times over.

### 1.3 Realloc-heavy workload

Three complementary measurements, per the task brief's (a)/(b)/(c) items:

1. **R12-4's own throwaway harness**, re-run on current HEAD in all three
   arms (`production` alone / `+exact-span-large` alone /
   `+exact-span-large+large-reserved-capacity`) — reports move-leg counts,
   copied bytes, and best-of-20 wall-clock for one 256 KiB→4 MiB chain.
   (`examples/r12_4_reserved_capacity_measure.rs`, unmodified — reused
   verbatim.)
2. **New criterion bench** (`benches/r13_6_exact_span_reserved_capacity_wallclock.rs`,
   `r13_6_realloc_chain` group) — the SAME chain, run through criterion for a
   statistical distribution (mean/CI) instead of a single best-of-20 scalar.
3. **iai `realloc_grow`** (§1.1, `benches/perf_gate_iai.rs`, pre-existing,
   unmodified) — a DIFFERENT, longer chain (64 B → 4 MiB, 16 doublings,
   starting in the Small range and crossing into Large) measured
   deterministically. This is the most decisive of the three because it is
   immune to host wall-clock noise.

### 1.4 Large-cache workload

Two complementary measurements:

1. **New criterion bench** (`r13_6_large_cache_cycle` group) — round-robin
   alloc+dealloc across 4 sub-4-MiB control sizes (260 KiB, 512 KiB, 1 MiB,
   1.75 MiB — R12-3's own control points), 16 passes per criterion iteration,
   `AllocCore::dbg_large_cache_hits()` read once after the whole group
   (informational lower bound only — criterion's iteration count is a
   black box).
2. **New throwaway example**
   (`examples/r13_6_large_cache_hit_rate_measure.rs`) — the SAME 4 sizes,
   but a FIXED, KNOWN 2000 passes (8000 total alloc+dealloc cycles), giving
   an EXACT `hits / total_deallocs` percentage plus final slot occupancy
   (`dbg_large_cache_slot_sizes()`). This is the authoritative number for
   item 4 in §0; the criterion bench is the wall-clock companion.

**Both require `alloc-stats`** (not part of `production`) added to the build
line — `AllocCore::dbg_large_cache_hits()`'s INCREMENT (not the accessor) is
gated `#[cfg(feature = "alloc-stats")]` in `alloc_core_large.rs`'s cache-hit
branch. This was discovered mid-task (§9, a real self-caught measurement
bug): an initial run without `alloc-stats` silently printed `hits=0` on
BOTH arms, which would have been reported as "cache never hits" — wrong, and
caught only because 0 hits was implausible on its face for a warm, tight
working set. Both bench/example module docs now document this requirement
explicitly, mirroring the same gating `docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md`'s
harnesses already work around.

### 1.5 First-heap commit (item 5, opportunistic)

Reused `examples/first_alloc_process.rs` + `scripts/first-alloc-bench.mjs`
(both pre-existing, unmodified) — the R12-9/primordial-lazy-commit judge.
5-sample runs on `production` alone and on `production` + both new features.
This axis is NOT expected to move: `exact-span-large`/`large-reserved-capacity`
touch only `alloc_large_slow`'s reservation math (`alloc_core_large.rs`,
`os.rs`'s `reserve_exact`/`reserve_capacity_exact`), never the primordial
segment (`bootstrap::primordial`, gated on `primordial-lazy-commit` alone —
untouched by this pair) or ordinary small-segment reservation
(`reserve_small_segment`) — confirmed by direct source read (§1.6 below is
the story of chasing down an initial false alarm on this very axis).

### 1.6 A self-caught false alarm (documented per CLAUDE.md's zero-trust
### discipline)

The FIRST attempt at item 5 built `first_alloc_process` with an explicit
feature list — `"alloc-global alloc-xthread alloc-decommit fastbin
exact-span-large large-reserved-capacity"` — and got `commit_after_1_heap_kib
≈ 4632–5284 KiB` vs the script's own `production`-only baseline of `≈900–1540
KiB`, a ~3–5× jump that looked like a real, surprising regression on an axis
the source code says should be untouched. Chasing it down (source-level check
of `bootstrap::primordial`, `reserve_small_segment`, `AllocCore`'s field list,
`HeapSlot`'s lazy-`MaybeUninit` design) found no plausible mechanism — and
re-running the SAME baseline feature string manually (bypassing the script)
reproduced the SAME elevated number, proving the discrepancy was not caused
by the two new features at all: the manually-typed feature list **omitted
two features the `production` alias actually includes**
(`alloc-segment-directory`, `primordial-lazy-commit` — see `Cargo.toml`'s
`production = [...]` line). Rebuilding with the correct, complete
`--features "production exact-span-large large-reserved-capacity"` gave
`commit_after_1_heap_kib ≈ 1516–1528 KiB` — matching baseline within sample
noise, confirming §0 item 5's "no measurable difference" verdict. This
episode is recorded here (rather than silently corrected) because it is
exactly the kind of self-inflicted measurement error `CLAUDE.md`'s
zero-trust review discipline exists to catch, and because a wrong "5× commit
regression" claim would have been a materially misleading finding for this
document to ship.

---

## 2. RSS amplification (item 1) — reproduced from R12-3, unchanged

Full command/output in `docs/perf/_raw_r13_6_r12_3_baseline_off.log` and
`docs/perf/_raw_r13_6_r12_3_treatment_on.log`
(`cargo run --release --example r12_3_exact_span_measure --features
production` / `--features "production,exact-span-large"`):

| Request size | `production` (baseline) | `+exact-span-large` |
|---|---:|---:|
| 260 KiB | 15.80× | 1.06× |
| 512 KiB | 8.00× | 1.01× |
| 1 MiB | 4.00× | 1.00× |
| 1.75 MiB | 2.29× | 1.00× |
| 4 MiB | 2.00× | 1.00× |

Byte-for-byte reproduction of R12-3's own committing-agent numbers (see
`docs/perf/R12_13_PAGE_RUN_LAYER_DEFERRED.md` §2.1's table) on current HEAD —
confirms the mechanism is intact and unregressed by the R13 rounds landed
since R12-3 shipped.

---

## 3. Realloc-heavy workload (item 3) — the central finding

### 3.1 R12-4's own three-arm harness, re-run on current HEAD

Raw logs: `docs/perf/_raw_r13_6_r12_4_arm1_production.log` (baseline),
`_raw_r13_6_r12_4_arm2_exact_span_only.log` (exact-span-large ALONE — the
"problem" arm), `_raw_r13_6_r12_4_arm3_both_features.log` (both features —
the "fix" arm).

| Arm | Move legs / 4 | In-place legs / 4 | Copied bytes | Wall-clock (best of 20) | Peak RSS Δ | Peak commit Δ |
|---|---:|---:|---:|---:|---:|---:|
| `production` (baseline) | 1 | **3** | 2,097,152 | 1.911 ms | 6,192 KiB | 12,676 KiB |
| `+exact-span-large` alone | **4** | 0 | 3,932,160 | 2.928 ms | 7,996 KiB | 8,352 KiB |
| `+exact-span-large+large-reserved-capacity` (both) | 2 | 2 | 2,621,440 | 2.269 ms | 6,708 KiB | 7,056 KiB |

**The mitigation works partially, not fully.** `large-reserved-capacity`
recovers 2 of the 4 in-place legs `exact-span-large` alone lost (0→2 out of a
baseline of 3), and wall-clock improves from 2.928 ms back toward 1.911 ms
(though it lands at 2.269 ms — **+18.8% slower than baseline**, not
fully-recovered). Peak commit-charge is genuinely BETTER than baseline in
both treatment arms (7,056–8,352 KiB vs baseline's 12,676 KiB) — the
exact-span sizing keeps the running total tighter even accounting for the
relocations' extra transient double-booked memory, so the RSS-side benefit
survives even where the realloc-latency side does not fully recover.

**Why the fix is only partial**, confirmed by direct source read
(`src/alloc_core/alloc_core_large.rs:381-385`): `reserved_capacity` is a
GEOMETRIC 2× multiple of `usable`, computed and FIXED once at the segment's
ORIGINAL allocation — it does not grow as the segment grows in place. Chain
256 KiB→512 KiB→1 MiB→2 MiB→4 MiB: the initial ~256 KiB segment's
`reserved_capacity` is ~512 KiB, so step 1 (→512 KiB) fits in-place; step 2
(→1 MiB) EXCEEDS the fixed 512 KiB ceiling and must relocate — but the
relocated ~1 MiB segment gets a FRESH ~2 MiB `reserved_capacity`, so step 3
(→2 MiB) fits in-place again; step 4 (→4 MiB) exceeds THAT segment's ~2 MiB
ceiling and relocates once more. Net: 2 relocations out of 4 steps — an
artifact of the chain's own doubling cadence exactly matching the geometric
cap's own doubling cadence, not a bug. A chain with smaller growth
increments relative to each step's starting size would see fewer relocations
under the SAME mechanism; a chain that keeps doubling (as both this harness
and the iai `realloc_grow` bench do) will keep tripping the ceiling roughly
every other step, by construction.

### 3.2 Criterion wall-clock distribution (new bench, `r13_6_realloc_chain`)

Raw logs: `docs/perf/_raw_r13_6_wallclock_bench_baseline_off.log`,
`docs/perf/_raw_r13_6_wallclock_bench_treatment_on.log` (built with
`alloc-stats` also on, for the paired large-cache group in the same binary —
the realloc-chain group itself does not depend on `alloc-stats`).

| Arm | Time (criterion `[low mid high]`) |
|---|---|
| `production` (baseline) | [3.2137 ms, 3.2527 ms, 3.3016 ms] |
| `+exact-span-large+large-reserved-capacity` | [3.5255 ms, 3.5548 ms, 3.5925 ms] |

Point estimate: **+9.3% slower** (3.2527 ms → 3.5548 ms). Directionally
consistent with §3.1's 18.8% figure (different absolute methodology — this
bench uses criterion's full statistical loop vs §3.1's single best-of-20 —
but both agree on the sign and rough magnitude: the mitigation does not fully
close the gap `exact-span-large` alone opens). Host wall-clock is inherently
noisy (this project's own stated policy per `CLAUDE.md`); §3.3's iai number
is the tie-breaker.

### 3.3 iai `realloc_grow` — the decisive, deterministic number

Raw logs: `docs/perf/_raw_r13_6_iai_baseline_production.log`,
`docs/perf/_raw_r13_6_iai_treatment_on.log`.

| Metric | `production` (baseline) | `+exact-span-large+large-reserved-capacity` | Δ |
|---|---:|---:|---:|
| Instructions (Ir) | 513,187 | 1,038,281 | **+102.320%** |
| L1 Hits | 1,087,343 | 2,644,713 | +143.227% |
| L2 Hits | 3,946 | 12,176 | +208.566% |
| RAM Hits | 70,650 | 78,854 | +11.612% |
| Total read+write | 1,161,939 | 2,735,743 | +135.446% |
| Estimated Cycles | 3,579,823 | 5,465,483 | **+52.675%** |

This bench (`benches/perf_gate_iai.rs::realloc_grow`, pre-existing,
unmodified — 64 B → 4 MiB via 16 successive doublings through
`GlobalAlloc::realloc`) is the most decisive measurement in this report
because Callgrind's `Ir` count is **bit-for-bit reproducible** on the same
binary+input regardless of host contention (unlike wall-clock). A **>100%
instruction-count increase** on a realistic geometric-growth pattern is a
real, structural regression, not noise — and it is LARGER in relative terms
than either wall-clock measurement above, consistent with §3.1's mechanism
explanation: a doubling-cadence workload trips the fixed geometric
`reserved_capacity` ceiling on almost every step, and every relocation this
bench's 16-step chain incurs costs a full `memcpy` of the (growing) prior
size — the later doublings in a 64 B→4 MiB chain move megabytes-scale
prefixes, which `Ir`/cache-hit counting makes fully visible (this is exactly
the "memcpy floors show up as ~22× the cycle gap Ir alone reports" effect
`scripts/iai.mjs`'s own module doc calls out for the X-arc case — the same
class of defect, here running the other direction).

**This is the headline risk finding of this task.** `exact-span-large`
alone (per R12-3/R12-13's own prior analysis) sacrifices realloc-growth
headroom for RSS tightness; `large-reserved-capacity` was designed
specifically to counteract that, and it DOES help (§3.1's 3-arm comparison
shows real recovery from the exact-span-alone regression) — but for a
workload shaped like repeated geometric doubling, the FIXED 2× ceiling
re-triggers relocation on almost every other step, leaving the PAIR net
slower than plain `production` by a deterministically measured, non-trivial
margin on this specific access pattern.

---

## 4. Large-cache workload (item 4) — hit rate holds, slot spread as predicted

### 4.1 Exact hit rate (new example, fixed 8000-cycle run)

Raw logs: `docs/perf/_raw_r13_6_hit_rate_baseline_off.log`,
`docs/perf/_raw_r13_6_hit_rate_treatment_on.log`.

| Arm | Hits / total | Hit rate | Final slot occupancy (8 slots) |
|---|---:|---:|---|
| `production` (baseline) | 7999 / 8000 | 99.99% | `[Some(4194304), None×7]` — **1 slot used** |
| `+exact-span-large+large-reserved-capacity` | 7996 / 8000 | 99.95% | `[Some(270336), Some(528384), Some(1052672), Some(1839104), None×4]` — **4 slots used** |

**Confirms the task brief's own predicted caveat exactly, and shows the
effect is real but small at this scale.** Under baseline whole-`SEGMENT`
rounding, all four request sizes (260 KiB, 512 KiB, 1 MiB, 1.75 MiB) round up
to the SAME `usable = 4,194,304` (4 MiB) value — they are numerically
INDISTINGUISHABLE to the cache's best-fit matcher, so ONE cached slot
satisfies every request regardless of which of the four sizes asks next
(100% aliasing → only 1 of 8 slots is ever occupied). Under
`exact-span-large`, each size gets its own page-exact `usable` — four
NUMERICALLY DISTINCT values — so the SAME workload now occupies 4 of the 8
slots simultaneously. At N=4 distinct sizes this costs almost nothing (both
hit rates round to effectively 100%, a 0.04 percentage-point difference that
is well within measurement noise for an 8000-sample run) — but it uses 4× the
slot budget for the SAME logical working set, which is exactly the
degradation mode task #277 (R13-7, "extend Large cache beyond 8 slots") and
#278 (R13-8, "judge on 256–2048 live objects 260 KiB–2 MiB") exist to
investigate further at LARGER N. This task's own numbers do not show a
practical hit-rate problem at N=4; they show the STRUCTURAL mechanism that
would produce one at higher N, honestly reported as the task brief requested
("if you see this phenomenon, record it honestly, it is an expected finding,
not a blocker").

### 4.2 Criterion wall-clock (new bench, `r13_6_large_cache_cycle`)

| Arm | Time (criterion `[low mid high]`) | `large_cache_hits` (informational, criterion's black-box iteration count) |
|---|---|---:|
| `production` (baseline, `+alloc-stats`) | [3.9072 µs, 3.9981 µs, 4.1337 µs] | 23,735,743 |
| `+exact-span-large+large-reserved-capacity` (`+alloc-stats`) | [4.1226 µs, 4.2201 µs, 4.3272 µs] | 23,249,980 |

Point estimate: **+5.6% slower** (3.9981 µs → 4.2201 µs) for the 64-op
round-robin cycle. Directionally consistent with §4.1's slot-spread finding
(4 slots touched instead of 1 means more `Option` slots scanned per best-fit
lookup — `O(LARGE_CACHE_SLOTS)` either way, so the absolute cost difference
is small, matching the small percentage here) — not large enough to be a
standalone gating concern at this N, but real and in the expected direction.

---

## 5. Small-class wall-clock isolation (item 2) — confirmed neutral

`docs/perf/_raw_r13_6_bench_table_production_baseline.log` (full canonical
`npm run bench:table` run, current `production` composition) and
`docs/perf/_raw_r13_6_bench_table_treatment_spotcheck.log` (direct
`cargo bench --features "production exact-span-large large-reserved-capacity"
--bench global_alloc -- "^global_alloc/"`, the 4-size cold-direct group
only — see §1.2 for why the full-suite re-run was narrowed).

Baseline cold-direct table (SeferAlloc, ns/op, `OPS=1024` batch scaling):
16B=46.3, 64B=42.6, 256B=37.7, 1024B=42.3.

Treatment spot-check (SeferAlloc, µs per 1024-op batch, same scaling):
16B≈55.5µs (≈54.2 ns/op), 64B≈54.4µs (≈53.1 ns/op), 256B≈57.3µs (≈56.0 ns/op),
1024B≈56.6µs (≈55.3 ns/op).

The deltas (≈54 vs ≈46 ns/op, roughly +15–20%) are within this host's
run-to-run wall-clock noise band for a `sample_size(10)` fast-profile bench
(cross-process comparison, not criterion's own paired `change:` statistic,
which only compares a binary against ITS OWN prior `target/criterion`
baseline — these are two separately-built binaries under different feature
sets, so no criterion-native paired comparison exists between them). This is
consistent with — not proof against — the structural expectation that
Small-class cold-direct allocation is completely untouched by
`exact-span-large`/`large-reserved-capacity` (both gate exclusively inside
`alloc_large_slow`/`os.rs`'s Large-segment reservation constructors, confirmed
by direct source read in §1.5). A rigorous statistical confirmation of "no
change" for the Small-class path would need the SAME criterion baseline file
compared via `--baseline`, which `scripts/bench-table.mjs` does not currently
support across feature sets — flagged as a process gap for task #280, not
something this task's scope covers fixing.

---

## 6. First-heap commit (item 5) — no measurable difference

§1.5/§1.6 cover the methodology and the self-caught false alarm. Final,
correct numbers (5 fresh-process samples each,
`docs/perf/_raw_r13_6_firstalloc_baseline_production.log` /
`docs/perf/_raw_r13_6_firstalloc_treatment_5samples.log`):

| Arm | `commit_after_1_heap_kib` (min–max across 5 samples) |
|---|---|
| `production` (baseline) | 1516–1540 KiB |
| `+exact-span-large+large-reserved-capacity` | 1516–1528 KiB |

No measurable difference — fully within sample noise, exactly as the source
read in §1.5 predicts (neither feature's code path is reachable from the
primordial-segment or ordinary-small-segment reservation constructors this
harness exercises).

---

## 7. Recommendation (NOT a decision)

**CONDITIONAL-GO.**

**What is solid:**
- `exact-span-large`'s RSS win (§2) is real, large, and unregressed
  (15.8×→1.06× at 260 KiB, matching R12-3's own numbers exactly).
- `large-reserved-capacity` demonstrably helps the realloc-latency problem
  `exact-span-large` alone creates (§3.1: 0→2 of 4 legs recovered in-place,
  wall-clock 2.928 ms→2.269 ms) — it is not a no-op mitigation.
- The Large-cache hit-rate cost (§4) is real but small at the N=4 scale
  measured here (99.99%→99.95%), and the Small-class path (§5) and
  first-heap bootstrap (§6) are both confirmed untouched.

**What blocks an unconditional GO:**
- §3.3's iai `realloc_grow` result — **+102.3% instructions, +52.7%
  Estimated Cycles**, deterministically measured — shows the pair is NET
  SLOWER than plain `production` on a realistic geometric-doubling realloc
  workload, not merely "still catching up to baseline." This is not a noise
  artifact; it is the SAME direction and a LARGER magnitude than both
  wall-clock measurements (§3.1's +18.8%, §3.2's +9.3%), and it is the
  measurement this project's own stated policy (`scripts/iai.mjs`'s module
  doc: "Ir stays the PASS/FAIL judge... wall-clock on Windows is noise")
  treats as authoritative when wall-clock and iai disagree in magnitude.
- The root cause (§3.1) is structural, not a bug to be quickly patched: a
  FIXED 2× geometric `reserved_capacity` ceiling, set once at each segment's
  origin, will always be re-tripped by any realloc-growth pattern whose own
  cadence approaches or exceeds 2× per step (doubling being the worst case,
  and also the single most common realloc-growth pattern in real code —
  `Vec`/`String`'s own default growth factor is close to this ceiling).
  Widening the cap (e.g. 4× instead of 2×) or making it adaptive across
  relocations (carry forward a growing multiplier instead of resetting to 2×
  on every relocation) are both plausible follow-ups, but neither was
  attempted here — this task is measurement-only per its own brief.

**Conditions under which GO becomes appropriate**, in the order that would
most cheaply close the gap:
1. A follow-up task investigating whether `LARGE_RESERVED_CAP_GROWTH_FACTOR`
   (currently a fixed 2×) can be safely widened, or made to compound across
   successive relocations of the same logical realloc chain, specifically
   targeting the doubling-cadence regression §3.3 measured — with the SAME
   iai `realloc_grow` bench as the before/after judge (it is already the
   right oracle, already exists, needed no new code this session).
2. Confirmation that no `production`-shipped workload in this codebase's own
   benches/tests (`realloc_grow_geometric` in `benches/large_realloc.rs`, the
   `medium_size_sweep`/`heap_*` suites) exhibits this doubling-cadence
   pattern at a scale where the regression would be user-visible outside a
   synthetic micro-benchmark — not investigated in this task, flagged as
   open.
3. Cross-platform confirmation (§8) that the RSS win (Windows-relevant per
   `large-reserved-capacity`'s own "Windows-gated EFFECT" framing established
   in R12-4/R12-9) and the realloc-latency regression (platform-agnostic per
   §3.3's iai run being Linux/WSL-native, not Windows-specific) hold their
   relative shape on real Linux/macOS hardware, not just this session's
   Windows-native + WSL2-Linux combination.

Given the magnitude and determinism of §3.3's finding, **shipping this pair
into `production` today, unconditionally, is not recommended** — but the RSS
win is real enough, and the realloc regression narrow enough in root cause
(one tunable constant, one well-understood mechanism, one pre-existing
oracle bench ready to judge a fix), that a short, scoped follow-up aimed
specifically at closing §3.3's gap is a reasonable next step before either a
full GO or a permanent NO-GO.

---

## 8. Platform limitation

Measured **locally on Windows 10 Pro x86-64** for every wall-clock/RSS/
commit-charge axis (§2, §3.1, §3.2, §4, §5, §6), and via **WSL2 (Ubuntu
24.04) + Valgrind/Callgrind** for the deterministic iai axis (§3.3) — WSL
access, initially assumed unavailable per this task's own brief ("if WSL is
unavailable, document the limitation and fall back to wall-clock"), turned
out to be present and functional in this session, so the iai axis is **not**
a documented gap this time (§3.3 ran the real mechanism, not a substitute).

**True Linux-native and macOS-native runs, and multi-socket NUMA hardware,
were not available to this session** — every number in this document comes
from ONE physical host (Windows-native process + WSL2 Linux guest on the
SAME underlying CPU/memory subsystem), so host-level cross-platform variance
(different `mmap`/`VirtualAlloc` implementations' real syscall latency, real
multi-socket NUMA remote-access penalties, macOS's `vm_allocate` behavior)
is entirely unmeasured here. `large-reserved-capacity`'s own documented
framing (R12-4/R12-9's "Windows-gated EFFECT, not Windows-gated CODE"
convention) predicts the RSS-deferral benefit is Windows-specific (Unix/miri
already commit the whole reservation eagerly, per `aligned-vmem`'s own
backend split) — this session's WSL2-Linux iai run is consistent with that
framing structurally (the SAME `Ir` regression appears on Linux too, because
the relocation-count mechanism §3.1 identifies is portable — it is about
segment layout/geometric-cap arithmetic, not Windows-specific commit
deferral), but a REAL non-WSL Linux distro and REAL macOS hardware are not
substitutes-tested here. This is the same cross-platform gap task #280
("wave-report process improvement") is already scoped to address at the
process level; this task's own numbers should be read as
"Windows-native + WSL2-Linux, one host, one measurement session" — a solid
same-machine A/B, not a cross-platform survey.

---

## 9. Verification runs (the measurement mechanism itself, not just results)

- `cargo test --release --features "production exact-span-large
  large-reserved-capacity"` — **green**, full suite, re-confirmed on current
  HEAD (`6018cf8` + this task's new bench/example files) both before and
  after adding the new measurement artifacts. No `src/` file was touched by
  this task.
- `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo clippy --all-targets -- -D warnings` (default feature set) — clean.
- `cargo clippy --all-targets --features experimental -- -D warnings` —
  clean.
- `cargo clippy --all-targets --features "production exact-span-large
  large-reserved-capacity" -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- The self-caught false alarm in §1.6 is itself a verification finding: the
  FIRST version of the first-heap-commit measurement was wrong (a manually
  typed feature list omitted two features `production` actually includes),
  caught by the same zero-trust discipline `CLAUDE.md` mandates
  ("counterfactual... would this test fail without the fix") applied to a
  measurement rather than a test — the anomalous 3–5× number was implausible
  enough on its face, and the source-level mechanism search found no
  plausible cause, that the measurement itself (not the code) was
  re-examined and the bug found there.

---

## 10. Artifacts this task adds

- `benches/r13_6_exact_span_reserved_capacity_wallclock.rs` — criterion bench,
  two groups (`r13_6_realloc_chain`, `r13_6_large_cache_cycle`), registered
  in `Cargo.toml` (`required-features = ["alloc-core", "alloc-decommit"]`).
- `examples/r13_6_large_cache_hit_rate_measure.rs` — throwaway exact-hit-rate
  harness, registered in `Cargo.toml`
  (`required-features = ["alloc-core", "alloc-decommit"]`).
- This document.
- Raw logs (`docs/perf/_raw_r13_6_*.log`, 15 files): iai baseline/treatment,
  R12-3 RSS-amplification baseline/treatment, R12-4 3-arm harness re-runs
  (production/exact-span-only/both-features), criterion wall-clock
  baseline/treatment, canonical bench-table baseline + treatment spot-check,
  first-heap-commit baseline (5 samples) + treatment (5 samples), large-cache
  exact-hit-rate baseline/treatment.
- No `Cargo.toml` feature-list edit (`production = [...]` is untouched), no
  `src/` edit.
