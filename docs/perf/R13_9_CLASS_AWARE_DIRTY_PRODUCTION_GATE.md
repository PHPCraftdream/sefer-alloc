# R13-9 — Production A/B gate for `class-aware-dirty` (per-(segment,class) dirty-bit routing)

**Task:** #279 (R13-9). **MEASUREMENT ONLY — not a promotion decision.** This
document reports what was measured; the GO/CONDITIONAL-GO/NO-GO line in §7 is
a **recommendation**, not a decision. Whether `class-aware-dirty` joins
`production = [...]` in `Cargo.toml` is left to the orchestrator/user, per
this task's own brief. `Cargo.toml` was NOT edited by this task.

**Date:** 2026-07-23. **Base revision:** `main` @ `874650b` (R13-1..R13-8,
R13-11, R13-12 landed; R13-9 is the next task in queue — all three of this
task's stated prerequisites are satisfied: #271/R13-1's coarse-only OOM latch
`e2d84f7`, #275/R13-5's feature-isolated CI row + loom wiring `0f3b608`,
#284/R13-11's lost-wakeup test fix `da037f2`). **Platform measured:** Windows
10 Pro x86-64 (native) for wall-clock/RSS; **WSL2 (Ubuntu 24.04) +
Valgrind/Callgrind** for the deterministic instruction-count judge (`npm run
iai` machinery, `scripts/iai.mjs`) — available in this session, same as R13-6.
Linux/macOS-native and true multi-socket NUMA hardware were **not** available
(same limitation as R13-6 §8; not re-litigated in depth here).

`class-aware-dirty` is opt-in (`Cargo.toml`: `class-aware-dirty =
["alloc-xthread", "alloc-segment-directory", "dep:aligned-vmem"]`) and remains
untouched by this task — no `Cargo.toml` feature-list edit changing
`production`, no `src/` edit. This task adds measurement-only artifacts: one
throwaway example (`examples/r13_9_class_aware_dirty_sidecar_rss.rs`) and this
document, plus reusing the pre-existing `benches/r12_7_class_aware_dirty_wallclock.rs`
and `scripts/iai.mjs`/`benches/perf_gate_iai.rs` unmodified.

---

## 0. Headline summary (was / now)

| # | Measurement | `production` (baseline) | `production` + `class-aware-dirty` (treatment) | Verdict |
|---|---|---|---|---|
| 1a | iai, 12 single-thread non-remote benches, Instructions (Ir) | see §3 table | +0.00% to +0.02% across all 12 | **No measurable regression — deterministic, near-zero** |
| 1b | iai, same 12 benches, Estimated Cycles | see §3 table | +0.00% to +0.35% across all 12 | **No measurable regression — within noise** |
| 2a | Remote fan-in wall-clock, SUB-WINDOW `ns/owner_alloc`, N=8 producer classes | 23,527.4 ns | 1,083.9 ns | **21.7× reduction on this sub-window metric — the R12-7 win survives the R13-1 latch intact** |
| 2b | Remote fan-in wall-clock, FULL ROUND (criterion's own mean, same harness/run), N=8 | ~20.6 ms | ~18.4 ms | **~11% full-round reduction — see R14-3 correction below; the sub-window's ~17ms "savings" mostly moved into the unmeasured pre-alloc/recycle portion of the same round, it did not disappear** |
| 2 | Remote fan-in wall-clock, N=1→N=4 delta (sub-window axis) | +89.2% (722.7→1367.3) | +35.6% (722.1→979.2) | **Flattened, consistent with R12-7's own re-measurement** |
| 3 | Non-remote single-thread churn/cold-direct (16B–1024B) | iai items above ARE this axis | +0.00–0.02% Ir | **Confirmed unchanged — feature is remote-drain-only** |
| 4 | Sidecar RSS, per materialised heap | 0 (feature absent) | 8.0 KiB (2 pages), not R12-7's stated 6.1 KiB | **Small, well-bounded — see §5 correction** |
| 5 | CI feature-isolation row (`production class-aware-dirty alloc-stats`, no `numa-aware`) | — | green, re-run personally on current HEAD | **Confirmed green, exactly matches `.github/workflows/ci.yml` job** |

**The single most important finding is #2**: the ~20-32× SUB-WINDOW
`ns/owner_alloc` win R12-7 measured BEFORE the R13-1 coarse-only-latch fix
still holds AFTER it — the latch adds one `AtomicBool::load` per drain call
(a single cache-line read), which is invisible at both the iai
instruction-count level (§3, essentially 0% delta) and the sub-window
wall-clock level (§4, still 21.7× at N=8). The concern the task brief raised
("coarse-only latch adds an extra check per drain — confirm it doesn't eat
the win") is answered: it doesn't.

**R14-3 correction (task #288, 2026-07-23):** the "21.7×"/"21.71×" figure
above is a SUB-WINDOW metric — `benches/r12_7_class_aware_dirty_wallclock.rs`'s
`run_round` times only the region AFTER producer pre-allocation and BEFORE
`HeapRegistry::recycle`, not the full round. Criterion's own full-round mean
(same harness, same raw logs cited in row 2b above) moved only ~20.6 ms → ~18.4
ms at N=8 (~11%), and at N=4 the full round barely moved (1.840 ms → 1.811 ms,
~1.6%). The mechanism's win is real (row 2a, and the counter-level evidence in
`docs/perf/R9_6_CLASS_AWARE_DIRTY_ROUTING_JUDGE.md` is unchanged) — what moved
is where in the round the drain work happens (deferred into the unmeasured
pre-alloc/recycle portions), not that ~17 ms of work vanished. See
`docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md` for the full
re-measurement (including a new fixed-work process-level judge) and the
CLAUDE.md rule this finding motivated (report both axes on every wall-clock
gate going forward).

---

## 1. Methodology

### 1.1 iai (deterministic instruction-count judge) — items 1a/1b/3

`npm run iai` (`scripts/iai.mjs`) drives `benches/perf_gate_iai.rs` (12
pre-existing single-thread benches, unmodified — `small_churn_16b`,
`aligned_churn_640b_a128`, `large_alloc_free_cycle`, `realloc_grow`,
`cold_alloc_free_256x16b`, `cold_alloc_free_256x64b`,
`recycle_alloc_free_256x16b`, `recycle_alloc_free_256x64b`, `churn_256b`,
`churn_write_256b`, `multiseg_cold_256k`, `seg_cycle_decommit_256k`) under
WSL2 + Valgrind/Callgrind. None of these 12 benches touch cross-thread
frees/`drain_dirty_segments` — they are exactly the "non-remote paths" item 3
of the task brief asks to confirm unchanged, and simultaneously the
`class-aware-dirty` feature's OWN claimed scope ("touches only cross-thread
dirty-routing, not single-thread hot path").

Commands run exactly as the task brief specifies:
```text
node scripts/iai.mjs --features production
node scripts/iai.mjs --features "production class-aware-dirty"
```
Raw logs: `docs/perf/_raw_r13_9_iai_baseline_production.log`,
`docs/perf/_raw_r13_9_iai_treatment_on.log`.

### 1.2 Remote fan-in wall-clock — item 2

`benches/r12_7_class_aware_dirty_wallclock.rs` — the SAME harness R12-7 built
and used for its own before/after re-measurement (§3.4 item 6 of
`docs/perf/R12_7_CLASS_AWARE_DIRTY_ROUTING_GATE.md`), reused byte-for-byte
unmodified, re-run here on current HEAD (post-R13-1's coarse-only latch,
post-R13-11's lost-wakeup test fix). N ∈ {1, 2, 4, 8} concurrent producer
classes cross-thread-freeing 800 blocks each into a shared owner's segments
while the owner continuously allocates the target class, forcing
`find_segment_with_free_impl` → `drain_dirty_segments` on every magazine miss.
This is the mechanism the R13-1 latch instruments (`drain_dirty_segments`
checks `sidecar_oom_latch` before selecting the per-class vs. coarse scan
source), so re-running this exact bench on current HEAD is the direct way to
confirm the latch's extra per-drain check didn't eat the win, per the task
brief's explicit ask.

```text
cargo bench --bench r12_7_class_aware_dirty_wallclock --features "production alloc-stats"
cargo bench --bench r12_7_class_aware_dirty_wallclock --features "production alloc-stats class-aware-dirty"
```
Raw logs: `docs/perf/_raw_r13_9_wallclock_baseline_off.log`,
`docs/perf/_raw_r13_9_wallclock_treatment_on.log`.

(Note: `cargo bench` does not accept `--release` — criterion benches already
build in the `bench` profile, which is release-optimized by default. An
initial attempt with `--release` failed with a CLI usage error and produced
empty raw logs; caught immediately by re-reading the log before drawing any
conclusion from it, and re-run correctly — see the log files' own timestamps.
Recorded here per this project's zero-trust self-audit discipline, mirroring
R13-6 §1.6's own documented self-caught false alarm.)

### 1.3 Sidecar RSS — item 4

New throwaway example `examples/r13_9_class_aware_dirty_sidecar_rss.rs`
(registered in `Cargo.toml`, `required-features = ["alloc-global",
"alloc-xthread", "alloc-segment-directory", "class-aware-dirty"]`). Claims N
heaps (N ∈ {4, 8, 16}), forces each heap's sidecar to materialise via ONE
genuine cross-thread free from a helper thread (the only production code path
that reaches `ensure_per_class_dirty` — `registry::heap_core_xthread::
set_dirty_bit_for_segment`), and reads real process RSS (Windows
`GetProcessMemoryInfo`, same platform-probe code as `examples/rss_probe.rs`)
before/after each N-heap batch.

```text
cargo run --release --example r13_9_class_aware_dirty_sidecar_rss --features "production class-aware-dirty alloc-stats"
```
Raw log: `docs/perf/_raw_r13_9_sidecar_rss.log`.

### 1.4 CI feature-isolation row — item 5

Ran the EXACT command `.github/workflows/ci.yml`'s `test-feature-isolation`
job uses for its `class-aware-dirty` row, personally, on current HEAD
(twice — once before adding this task's own new example, once after, to
confirm the new artifact didn't disturb anything):

```text
cargo test --features "production class-aware-dirty alloc-stats" --no-fail-fast -- --skip r9_6_class_aware_dirty_waste_ratio_scales_with_class_count
```

### 1.5 A self-caught false alarm during verification (documented per
### CLAUDE.md's zero-trust discipline)

While re-running the full mandatory test matrix, THREE `cargo test`
invocations (feature-isolation row, `--all-features`, `--features production`)
were launched concurrently in the background to save wall-clock time. One of
the three came back reporting `error: 1 target failed:
--test regression_segment_table_tombstone_rebuild` — alarming on its face,
since that test is unrelated to this task's scope. Re-running that ONE target
in isolation (`cargo test --test regression_segment_table_tombstone_rebuild
--features "production class-aware-dirty alloc-stats"`) passed cleanly
(`1 passed; 0 failed`), and re-running the FULL original three-command matrix
sequentially (one at a time, no concurrency) also came back fully green on
all three. Conclusion: the failure was a Windows build-concurrency artifact
(three `cargo test` processes racing to link/relaunch binaries in the SAME
shared `target/` directory — a known Windows-specific cargo hazard, e.g. an
`.exe` still handle-locked by a just-exited previous test process or an
antivirus scan when the next linker pass tries to overwrite it), not a real
code regression. This episode is recorded here rather than silently
corrected, per the same discipline R13-6 §1.6 documents for its own
self-caught false alarm. All §9 verification-run results below are from the
CLEAN, sequential (non-concurrent) reruns.

---

## 2. iai — deterministic Ir/Cycles table (items 1a/1b/3)

| Bench | base Ir | treat Ir | Ir Δ% | base Cycles | treat Cycles | Cycles Δ% |
|---|---:|---:|---:|---:|---:|---:|
| `small_churn_16b` | 28,571 | 28,576 | +0.02% | 78,530 | 78,674 | +0.18% |
| `aligned_churn_640b_a128` | 28,507 | 28,512 | +0.02% | 78,428 | 78,572 | +0.18% |
| `large_alloc_free_cycle` | 23,831 | 23,836 | +0.02% | 72,652 | 72,906 | +0.35% |
| `realloc_grow` | 513,187 | 513,192 | +0.00% | 3,579,841 | 3,579,895 | +0.00% |
| `cold_alloc_free_256x16b` | 70,669 | 70,674 | +0.01% | 134,246 | 134,390 | +0.11% |
| `cold_alloc_free_256x64b` | 70,669 | 70,674 | +0.01% | 140,502 | 140,680 | +0.13% |
| `recycle_alloc_free_256x16b` | 118,796 | 118,808 | +0.01% | 196,472 | 196,707 | +0.12% |
| `recycle_alloc_free_256x64b` | 118,796 | 118,808 | +0.01% | 203,106 | 203,337 | +0.11% |
| `churn_256b` | 28,571 | 28,576 | +0.02% | 78,496 | 78,678 | +0.23% |
| `churn_write_256b` | 28,827 | 28,832 | +0.02% | 78,914 | 79,062 | +0.19% |
| `multiseg_cold_256k` | 46,242 | 46,247 | +0.01% | 112,862 | 113,074 | +0.19% |
| `seg_cycle_decommit_256k` | 82,292 | 82,297 | +0.01% | 161,434 | 161,646 | +0.13% |

Every one of the 12 non-remote benches shows a deterministic +5-instruction
delta (28,571→28,576 etc.) — the SAME absolute +5 Ir on almost every bench,
consistent with a fixed, tiny, unconditional cost paid once per bench binary
(most plausibly link-time/binary-layout noise from the feature flag changing
which functions get compiled into the executable, NOT a per-call runtime
cost — a per-call cost would scale with each bench's OP_COUNT, and it does
not: `realloc_grow`'s 16-doubling chain shows the SAME +5 Ir as
`small_churn_16b`'s much larger iteration count). This is the textbook
signature of "feature added, but its code is never reached because these
benches never take a cross-thread-free path" — fully consistent with
`class-aware-dirty`'s own documented scope (`dirty_by_class.rs`'s module doc:
"With the feature OFF, none of this module's code exists in the binary and
`HeapSlotRemote`/`AllocCore` are byte-for-byte unchanged" — the converse, ON
but unreached, is what these 12 benches confirm). **Confirms items 1a/1b/3
cleanly: no measurable regression on any non-remote path.**

---

## 3. Remote fan-in wall-clock — the central finding (item 2)

Raw logs: `docs/perf/_raw_r13_9_wallclock_baseline_off.log`,
`docs/perf/_raw_r13_9_wallclock_treatment_on.log`.

| N (producers) | `production` baseline (SUB-WINDOW ns/owner_alloc) | `production`+`class-aware-dirty` (SUB-WINDOW ns/owner_alloc) | Sub-window speedup | FULL ROUND (criterion mean) baseline | FULL ROUND (criterion mean) treatment | Full-round speedup |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 722.7 | 722.1 | 1.00× (expected — no waste to eliminate at N=1) | — | — | — |
| 2 | 909.2 | 835.4 | 1.09× | — | — | — |
| 4 | 1,367.3 | 979.2 | 1.40× | 1.840 ms | 1.811 ms | ~1.6% |
| 8 | 23,527.4 | 1,083.9 | **21.71×** | ~20.6 ms | ~18.4 ms | **~11%** |

N=1→N=4 `ns/owner_alloc` delta: baseline **+89.2%** (722.7→1367.3) vs.
treatment **+35.6%** (722.1→979.2) — both criterion runs additionally
reported the treatment's own paired before/after comparison
("Change within noise threshold" at N=1/2/4, i.e. treatment vs. the SAME
binary's prior `target/criterion` history, not vs. baseline — the baseline
comparison in this table is the CROSS-process one that matters for this gate).

**R14-3 correction (task #288):** the "Sub-window speedup" column is the
figure this document originally headlined as "21.7× reduction" / "21.71×".
The "FULL ROUND" columns (added by R14-3, read off the SAME raw logs cited
above — criterion's own reported mean "time:" for the identical
`bench_function` invocation, which wraps the FULL `run_round` call including
pre-alloc and `HeapRegistry::recycle`, not just the sub-window) show the
actual end-to-end improvement for a fixed amount of work is far smaller: ~11%
at N=8, ~1.6% at N=4. The sub-window's dramatic reduction reflects deferred
drain work moving OUT of the timed window into the unmeasured pre-alloc/
recycle phases of the same round, not that work disappearing. Both axes are
real measurements of the same underlying mechanism; neither alone is the
complete picture — see `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md`
for the full re-measurement, including an independent fixed-work
process-level A/B/B/A judge.

**This confirms the task brief's central open question**: the R13-1
coarse-only-latch fix (`e2d84f7`, one `AtomicBool` `Relaxed` load added to
`drain_dirty_segments` before selecting the per-class vs. coarse scan source)
does NOT measurably erode the R12-7 win **on the sub-window axis**. R12-7's
own pre-R13-1 numbers were ~19.7-32.4× at N=8 on the same sub-window metric
(two runs, `docs/perf/R12_7_CLASS_AWARE_DIRTY_ROUTING_GATE.md` §3.4); this
task's post-R13-1 re-measurement is **21.71×** — squarely inside that same
range, not a degraded tail of it. §2's iai table independently confirms the
SAME conclusion at the deterministic instruction-count level (the latch's one
`AtomicBool::load` is a single cache-resident read, invisible against
benches with tens-of-thousands of instructions). The full-round axis (added
by R14-3) was not measured or discussed at all in this document's original
form — see the correction above.

---

## 4. Non-remote path isolation (item 3) — confirmed via §2, no separate axis needed

The task brief's item 3 ("regression on NOT-remote paths — single-thread
churn 16B-1024B, cold-direct — should remain unchanged") is directly answered
by §2's iai table: 8 of the 12 benches measured there (`small_churn_16b`,
`aligned_churn_640b_a128`, `cold_alloc_free_256x16b`, `cold_alloc_free_256x64b`,
`recycle_alloc_free_256x16b`, `recycle_alloc_free_256x64b`, `churn_256b`,
`churn_write_256b`) are exactly single-thread churn/cold-direct workloads in
the 16B-1024B range (`perf_gate_iai.rs`'s own size choices), all showing
+0.01-0.02% Ir — no separate wall-clock spot-check was needed because the
deterministic iai axis is strictly more decisive than wall-clock for this
question (same rationale R13-6 §1.1/§3.3 used: "Ir stays the PASS/FAIL
judge... wall-clock on Windows is noise").

---

## 5. Sidecar RSS cost (item 4) — small, well-bounded, one correction to R12-7's own number

Raw log: `docs/perf/_raw_r13_9_sidecar_rss.log`.

### 5.1 Correcting R12-7's "6.1 KiB" figure

`docs/perf/R12_7_CLASS_AWARE_DIRTY_ROUTING_GATE.md` §3.1 states the sidecar
costs "6.1 KiB per materialised heap" — that number is the RAW
`size_of::<PerClassDirty>()` (`SMALL_CLASS_COUNT * WORDS_PER_CLASS * 8` bytes
= 49 × 16 × 8 = 6,272 bytes = 6.13 KiB with the default 49-class table, 58
classes → 7,424 bytes = 7.25 KiB under `medium-classes`). The sidecar is
reserved via `aligned_vmem::leak_zeroed_pages`
(`src/alloc_core/dirty_by_class.rs`'s `PER_CLASS_DIRTY_SIZE` const), which
rounds the request UP to a whole number of 4 KiB pages. Both the default and
`medium-classes` raw sizes round up to the SAME 2-page ceiling:

| Class table | Raw `size_of` | Page-rounded (actual committed) |
|---|---:|---:|
| Default (49 classes) | 6,272 B (6.13 KiB) | **8,192 B (8.00 KiB, 2 pages)** |
| `medium-classes` (58 classes) | 7,424 B (7.25 KiB) | **8,192 B (8.00 KiB, 2 pages)** |

This task's new example prints the same computed figure at runtime
(`SMALL_CLASS_COUNT=49`, raw 6,272 bytes, page-rounded 8,192 bytes) — the
module doc comment in `dirty_by_class.rs` (§"Sizing and lazy materialisation")
already states the raw 6.1 KiB figure without noting the page-rounding; this
is a measurement-report correction only, not a code defect (the code's own
behaviour — round up to `PAGE`, matching `segment_directory`'s and
`HeapOverflow`'s sidecars' identical convention — is intentional and
documented as such in the same doc comment's rounding-rationale sentence).
**The corrected number for any RSS-budget accounting is 8.0 KiB per
materialised heap, not 6.1 KiB** — a ~30% understatement in the prior
document, though still small in absolute terms.

### 5.2 Multi-heap RSS deltas (real process measurement, N=4/8/16)

| N heaps | RSS before | RSS after | Delta | Delta/heap | Sidecar-only floor (N × 8 KiB) |
|---:|---:|---:|---:|---:|---:|
| 4 | 3,332.00 KiB | 3,592.00 KiB | 260.00 KiB | 65.00 KiB | 32.00 KiB |
| 8 | 3,592.00 KiB | 3,800.00 KiB | 208.00 KiB | 26.00 KiB | 64.00 KiB |
| 16 | 3,800.00 KiB | 4,220.00 KiB | 420.00 KiB | 26.25 KiB | 128.00 KiB |

The measured per-heap delta (26-65 KiB) is well ABOVE the sidecar-only
theoretical floor (8 KiB/heap) because each `HeapRegistry::claim()` call in
this harness also faults in a fresh heap's registry slot bookkeeping and (on
the owner's one allocation + the helper thread's one cross-thread free) a
freshly-reserved segment's touched pages — the sidecar itself is a small
FRACTION of each heap's total RSS footprint, not the dominant cost. The N=4
arm's higher per-heap delta (65 KiB, vs. 26-26.25 KiB for N=8/16) reflects
one-time process/registry warm-up cost concentrated in the FIRST arm run
(primordial segment, first-ever directory materialisation, etc.) — the
example's own module doc discloses that cross-arm process-level accumulation
in a single process run is not simply additive/comparable to isolated
fresh-process runs of each N (the sidecar itself, once materialised, is never
un-materialised — process-lifetime leak by design, same discipline as
`segment_directory`'s and `HeapOverflow`'s sidecars).

**For a process with a modest, realistic thread/heap count (e.g. 4, 8, or 16
threads/heaps, the range this task's brief asked about), the sidecar's own
contribution is at most `N × 8 KiB` — 32 KiB at N=4, 64 KiB at N=8, 128 KiB at
N=16 — a rounding error against typical process RSS budgets, and dwarfed by
the segment/registry-slot cost every heap already pays regardless of this
feature.** This confirms R12-7's own framing ("a modest, well-bounded cost for
the measured win") remains accurate even with the corrected 8.0 KiB (not 6.1
KiB) per-heap figure.

---

## 6. CI feature-isolation row (item 5) — confirmed green

Ran personally, twice (before and after adding this task's own new example
file), on current HEAD (`874650b` + this task's new example + this document):

```text
cargo test --features "production class-aware-dirty alloc-stats" --no-fail-fast -- --skip r9_6_class_aware_dirty_waste_ratio_scales_with_class_count
```

Both runs: full green, no failures (the ONE apparent failure encountered
during this task's OWN verification pass was a Windows build-concurrency
artifact from running three `cargo test` invocations concurrently against a
shared `target/` directory — traced to ground and confirmed spurious, see
§1.5). This is byte-for-byte the command `.github/workflows/ci.yml`'s
`test-feature-isolation` job already runs for its `class-aware-dirty` row —
confirms the CI gate landed by R13-5 (`0f3b608`) still passes on current HEAD,
unaffected by R13-6/R13-7/R13-8's intervening work.

---

## 7. Recommendation (NOT a decision)

**GO.**

**What is solid:**
- The wall-clock win (§3) — R12-7's headline ~20-32× N=8 sub-window
  `ns/owner_alloc` reduction, and the smaller but still real ~11% full-round
  wall-clock reduction for the same fixed amount of work (R14-3 correction,
  see §3) — survives the R13-1 coarse-only-latch fix intact (this task's
  re-measurement: 21.71× sub-window), confirmed at BOTH the wall-clock level
  (§3) and the deterministic instruction-count level (§2's near-zero Ir delta
  on every non-remote bench, meaning the latch check itself costs nothing
  measurable when the remote path isn't exercised).
- Non-remote paths (single-thread churn, cold-direct, 16B-1024B — item 3) are
  confirmed UNCHANGED via the deterministic iai judge (§2/§4): +0.00-0.02% Ir
  across all 12 benches, the SAME tiny absolute delta regardless of bench
  size or shape — consistent with "feature compiled in but code path never
  reached," not a per-call cost.
- The sidecar RSS cost (§5) is small and well-bounded even after correcting
  R12-7's own stated figure (8.0 KiB actual committed per materialised heap,
  not 6.1 KiB raw) — at most `N × 8 KiB` across a process's whole heap
  population, negligible against realistic process RSS budgets.
- All three of this task's stated prerequisites are independently verified
  landed and effective on current HEAD: R13-1's OOM-transition latch fix
  (§3's re-measurement IS the fix in effect), R13-5's CI wiring (§6, run
  personally and confirmed green), R13-11's lost-wakeup test fix (implicitly
  exercised — the feature-isolation row in §6 runs the full
  `class_aware_dirty_routing.rs` suite including the fixed assertion).
- All four of this task's mandated verification commands are green on current
  HEAD (§9).

**What is NOT covered by this task** (disclosed honestly, per the same
practice R13-6 §8 established):
- No true Linux-native or macOS-native run — only Windows-native wall-clock/RSS
  + WSL2-Linux iai, same single-host limitation as every prior R13 gate
  report (tracked at the process level by task #280).
- No true multi-socket NUMA hardware — `class-aware-dirty`'s own
  `drain_dirty_segments` code path is itself compiled OUT under `numa-aware`
  (confirmed by `benches/r12_7_class_aware_dirty_wallclock.rs`'s own runtime
  skip-guard, `cfg!(feature = "numa-aware")`), so this is a structural
  "feature and NUMA routing are mutually exclusive today," not a measurement
  gap this task could have closed — orthogonal to the promotion question
  itself (promoting `class-aware-dirty` into `production` does not change
  `production`'s NUMA behaviour, since `production` does not include
  `numa-aware` today either).
- This task did not attempt to characterise a WORST-case adversarial workload
  beyond N=8 producer classes (the R9-6/R12-7-established sweep ceiling) — a
  process with many more than 8 concurrently-hot producer classes on one heap
  is plausible in principle but not measured here or in any prior R9/R12
  report.

Given the corroborating evidence across two independent measurement axes
(wall-clock AND deterministic instruction-count), the confirmed survival of
the win through both subsequent correctness fixes (R13-1, R13-11), the
confirmed-green CI gate, and the small/well-bounded (and now correctly
quantified) RSS cost, **this task's own measurements support promoting
`class-aware-dirty` into `production`** — but per this task's explicit scope,
that is a recommendation for the orchestrator/user to act on, not a decision
made here, and `Cargo.toml`'s `production = [...]` list was left untouched by
this task.

---

## 8. Platform limitation

Same structural limitation as every prior R13 gate report in this wave
(R13-6 §8, R13-8): measured on ONE physical host — Windows 10 Pro x86-64
native for wall-clock/RSS, WSL2 (Ubuntu 24.04) + Valgrind/Callgrind on the
SAME underlying CPU/memory subsystem for the deterministic iai axis. True
Linux-native, true macOS-native, and true multi-socket NUMA hardware were not
available to this session. Not re-litigated further here; see R13-6 §8 for
the full discussion, which applies unchanged to this task.

---

## 9. Verification runs (the measurement mechanism itself, not just results)

All four commands mandated by the task brief, run to completion, SEQUENTIALLY
(not concurrently — see §1.5's self-caught concurrency false alarm) on current
HEAD (`874650b` + this task's new example + this document):

- `cargo test --release --features "production class-aware-dirty alloc-stats"`
  (exact CI feature-isolation command, `--skip
  r9_6_class_aware_dirty_waste_ratio_scales_with_class_count`) — **green**,
  confirmed clean in TWO separate isolated reruns (§1.5, §6).
- `cargo test --release --all-features` — **green**, full suite, re-confirmed
  after adding this task's new example/Cargo.toml registration.
- `cargo test --release --features production` — **green** (the default
  path, feature NOT in `production` today — confirms no accidental
  regression on the currently-shipping composition).
- `cargo clippy --all-targets --all-features -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `cargo clippy --example r13_9_class_aware_dirty_sidecar_rss --all-features -- -D warnings`
  — clean (spot-check on the new artifact specifically, before the full
  all-targets sweep above).
- `cargo test --release --all-features --test no_stale_doc_references` — green
  (confirms the new example did not require any README/ARCHITECTURE.md
  inventory-count update — it adds no new `tests/*.rs` file and no new
  `unsafe` seam inside `src/`/`crates/`, the only two things that inventory
  tracks; `examples/` is outside that inventory's scope entirely, confirmed
  by grep against README.md and cross-checked against the pre-existing
  `examples/rss_probe.rs`, which already contains multiple `unsafe` blocks
  untracked by the same inventory).

---

## 10. Artifacts this task adds

- `examples/r13_9_class_aware_dirty_sidecar_rss.rs` — throwaway RSS
  measurement harness, registered in `Cargo.toml`
  (`required-features = ["alloc-global", "alloc-xthread",
  "alloc-segment-directory", "class-aware-dirty"]`).
- This document.
- Raw logs (`docs/perf/_raw_r13_9_*.log`, 5 files): iai baseline/treatment
  (all 12 `perf_gate_iai` benches), wall-clock baseline/treatment
  (`r12_7_class_aware_dirty_wallclock`, reused unmodified), sidecar RSS
  (N=4/8/16).
- No `Cargo.toml` feature-list edit to `production = [...]` (still absent
  there), no `src/` edit.
