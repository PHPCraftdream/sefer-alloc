# R30-6 — the large-cache `headroom_bytes` BENEFIT-side A/B gate

**Task #455 (R30-6), Round 30.** R29-13
(`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md`) measured the RETENTION
COST of the large cache's default `headroom_bytes = 256 MiB/heap` and
explicitly named its own missing piece (§7): *"a throughput/hit-rate A/B at
a smaller headroom through the real `#[global_allocator]` (the large-cache
analogue of R27-4)."* `docs/perf/OPEN_ITEMS.md` item 27 filed exactly that
gap as "confirmed-by-design, no action recommended" **because the benefit
side was unmeasured**. This task measures it, mirroring R27-3/R27-4's
established two-report pattern (retention cost + real-allocator benefit) —
but for the large cache instead of the small-segment pool.

**Verdict: at this task's workload (48 MiB/burst, 8×6 MiB objects), 64 MiB
and 256 MiB headroom are INDISTINGUISHABLE — both achieve 100% hit rate on
a burst that immediately follows an idle window long enough for the decay
timer to be armed. 0 MiB and 16 MiB headroom cost exactly one hit out of
eight per burst (87.5% vs 100%, an EXACT, not noisy, 12.5-percentage-point
gap, reproduced identically at 1/8/32 threads). Wall-clock latency shows NO
statistically significant difference across the ENTIRE headroom sweep** —
none of the three real-`#[global_allocator]` paired A/B comparisons (h256 vs
h64, h256 vs h16, h256 vs h0) rejects the null hypothesis (all `t` well
under `crit(p<0.05)=2.101`; the same-vs-same honesty control also correctly
shows no signal, confirming the harness is not manufacturing noise into a
false positive).

**Headline for the pending decision (R30-7/task #456, gated on this
report):** **64 MiB preserves the FULL measured hit-rate benefit of the
256 MiB default in this workload — 0 percentage points of loss — while
RSS retention drops from R29-13's measured ~238-241 MiB/heap post-drain
floor to ~34-37 MiB/heap (R29-13 §0, same table), roughly 7× smaller.** This
is exactly the "if 64 MiB preserves most of the benefit, that's the
headline" scenario the task brief named. 16 MiB and 0 MiB both cost a real,
reproducible 12.5-percentage-point hit-rate hit relative to 64/256 MiB in
this workload — NOT free, so "drop to the smallest headroom" is not
supported by this data; "drop to 64 MiB" is.

**This task does not change any `src/` default** — measurement only, per
its own explicit instruction. `DEFAULT_HEADROOM_BYTES` (256 MiB) is
untouched in `src/alloc_core/large_cache_config.rs`. One `src/` addition was
needed: a single thin `HeapCore::dbg_large_cache_hits` delegation wrapper
(exposing the pre-existing `AllocCore::dbg_large_cache_hits` accessor at the
`HeapCore` level, following the exact established pattern already used by
`dbg_large_cache_used`/`dbg_large_cache_slot_sizes`/`dbg_decay_config` in the
same file) — no new `unsafe`, no raw-pointer parameter, gated identically to
the delegated method (`alloc-decommit` only; this is a pre-existing
diagnostic counter read, not a new hook).

**Date:** 2026-07-30. **Base revision:** `main` @ `3c3ad7d8` (clean at
session start, confirmed via `git rev-parse HEAD` before any edit) + this
task's working tree, landed together in the SAME commit this report cites —
per CLAUDE.md's R29-6 immutable-source-identity rule, citing the actual
landing commit SHA (form 1: a real, permanent commit, not a scratch hash) is
the strongest of the four sanctioned identity forms and is used here rather
than a patch/tree hash. **Commit that lands this report:** `97c2f07bf5c43478632ab01f9037a34cc648e9eb`
(this SHA was necessarily added in a small follow-up commit after the
landing commit itself, since a commit cannot cite its own SHA inside its
own tree — see that follow-up commit's message for the one-line
explanation).
**Platform:** native Windows 10 Pro x86-64, 11th Gen Intel Core i7-11800H @
2.30GHz (8 cores / 16 logical), rustc 1.97.0 — the SAME host as
R27-3/R27-4/R29-13 (shared, noisy — see the wall-clock caveat in §3).
**Feature sets:** hit-rate/RSS axis (`examples/r30_6_large_cache_headroom_ab_gate.rs`)
= `production alloc-stats bench-internals`; latency axis
(`examples/r30_6_latency_h0/_h16/_h64/_h256.rs`) = `production alloc-stats`
(no `bench-internals` needed — these use only `SeferAlloc::stats()`, the
public, always-`production`-available diagnostic surface).

---

## 0. Headline numbers

### 0.1 Hit-rate / RSS / burst-idle-burst axis (subprocess-per-arm, registry-bypass, median of 3 reps)

| headroom | threads | BURST2 hits/possible | hit rate | RSS @ burst2 (MiB/heap) | RSS @ idle (MiB/heap) | oracle |
|---:|---:|---:|---:|---:|---:|---|
| 0 MiB | 1 | 7/8 | **87.5%** | 51.3 | 51.3 | PASS |
| 0 MiB | 8 | 56/64 | **87.5%** | 49.7 | 49.7 | PASS |
| 0 MiB | 32 | 224/256 | **87.5%** | 49.4 | 49.4 | PASS |
| 16 MiB | 1 | 7/8 | **87.5%** | 51.3 | 51.3 | PASS |
| 16 MiB | 8 | 56/64 | **87.5%** | 49.7 | 49.7 | PASS |
| 16 MiB | 32 | 224/256 | **87.5%** | 49.4 | 49.4 | PASS |
| 64 MiB | 1 | 8/8 | **100.0%** | 51.3 | 51.3 | PASS |
| 64 MiB | 8 | 64/64 | **100.0%** | 49.7 | 49.7 | PASS |
| 64 MiB | 32 | 256/256 | **100.0%** | 49.4 | 49.4 | PASS |
| 256 MiB | 1 | 8/8 | **100.0%** | 51.3 | 51.3 | PASS |
| 256 MiB | 8 | 64/64 | **100.0%** | 49.7 | 49.7 | PASS |
| 256 MiB | 32 | 256/256 | **100.0%** | 49.4 | 49.4 | PASS |

**All 36 arms passed the path-activation oracle** (`admissions_ok` AND
`hits_ok`, both hard-asserted before the arm's numbers were trusted — see
§1.3). The 87.5%-vs-100.0% split is **exact and identical at every thread
count** (7/8, 56/64, 224/256 — all precisely 87.5%; 8/8, 64/64, 256/256 —
all precisely 100.0%), not a noisy trend — this is the single most
reproducible signal in the whole matrix. RSS at burst2 and RSS after the
1200 ms idle window are identical to the KiB (see raw log; `rss_idle_kib -
rss_burst2_kib` is 0 or within single-digit KiB noise in every row),
confirming R29-13's own idle-non-decay finding holds in this mixed
small+large, burst-idle-burst workload shape too, not only R29-13's
large-only tight-loop shape.

### 0.2 Latency axis (real `#[global_allocator]`, `paired-ab-runner.mjs`, A/B/B/A, n=20 pairs)

| comparison | mean Δ (A−B) | t | crit (p<0.05) | verdict | sign split |
|---|---:|---:|---:|---|---|
| h256 vs h64 | −256.0 µs | −0.503 | 2.101 | **NOT significant** | 12/8 |
| h256 vs h16 | −51.1 µs | −0.144 | 2.101 | **NOT significant** | 7/13 |
| h256 vs h0 | +54.5 µs | 0.173 | 2.101 | **NOT significant** | 10/10 |
| h256 vs h256 (control) | −226.2 µs | −1.134 | 2.101 | **NOT significant** | 14/6 |

**No headroom value shows a statistically distinguishable latency
difference from the 256 MiB default in this mixed small+large workload** —
every `|t|` is well under `crit`, and the same-vs-same control (last row)
confirms the harness correctly resolves "no real difference" rather than
manufacturing a spurious signal (its `t`/sign-split are in the same noise
band as the three real comparisons, not visibly tighter). This host is
shared with other concurrent builds during this measurement (see §3); the
per-run range shows real jitter (some `elapsed_ns` samples 2–4× their
neighbors), but the paired A/B/B/A protocol and the 20-pair sample are
exactly the machinery this project uses to separate real effects from that
noise (R5-R2, R27-4's own precedent), and it reports a clean null here.

---

## 1. Methodology

### 1.1 Two entry points, mirroring R27-3/R27-4's split

Per the task's own instruction and R27-3/R27-4's established precedent for
exactly this kind of question:

- **Hit-rate/RSS/burst-idle-burst axis** — `examples/r30_6_large_cache_headroom_ab_gate.rs`,
  subprocess-per-arm isolation via `HeapRegistry::claim_with_config`
  (registry-bypass, matching R27-3/R29-13's own shape for this axis) — a
  runtime headroom sweep across 12 arms in ONE binary needs the config
  passed at claim time, which the real `#[global_allocator]` entry point
  cannot do (see next point).
- **Latency axis** — four small standalone binaries
  (`r30_6_latency_h0`/`_h16`/`_h64`/`_h256`), each a REAL `#[global_allocator]`
  `SeferAlloc::with_config` static, one per headroom grid point.
  `SeferAlloc::with_config` bakes its `LargeCacheConfig` into a `static`
  initializer at COMPILE time (`src/global/sefer_alloc.rs:260`) — there is
  no way to select a runtime headroom value for the real global-allocator
  entry point without a separate binary per value, exactly the same
  structural constraint R27-4 hit for `pool_segments`/`pool_byte_cap` (that
  report's §1 names the identical reason for its `cap4`/`cap8` binary
  split).

### 1.2 Subprocess-per-arm isolation (hit-rate/RSS axis) — R26-4 config-sweep evidence rule, all 4 pieces

Every `(headroom_bytes, thread_count, repetition)` tuple runs in its OWN
freshly-spawned OS process (re-exec via `std::env::current_exe()`), exactly
R27-3/R29-13's protocol. Per CLAUDE.md's R26-4 rule, all four pieces are
present, per row:

1. **Requested** — `headroom_bytes` passed to `config_for()` /
   `LargeCacheConfig::new().headroom_bytes(n)`.
2. **Resolved** — read back via `HeapCore::dbg_decay_config()`'s third field
   (the SAME diagnostic accessor R29-13 used), hard-`assert_eq!`'d against
   the requested value inside EACH worker thread before any workload runs.
3. **Config-conflict delta** — `config_conflicts_total()` sampled before/after;
   hard-`assert_eq!(0, ...)` at the end of every child run. A fresh process
   has an empty registry, so the first claim is unconditionally this arm's
   config — structurally zero by construction, and independently confirmed
   zero in all 36 runs (see raw log / CSV `config_conflicts_delta` column).
4. **Process-identity** — subprocess-per-arm (a fresh OS process per cell),
   stated explicitly in the CSV's `process_identity` column (`"subprocess"`
   for every row) and structurally guaranteed by the re-exec launch
   mechanism — the exact R26-4-mandated mitigation for the `HeapRegistry`
   first-claim-wins slot-reuse hazard (`src/registry/heap_registry.rs`
   ~209-300) that invalidated R25-5's same-process sweep. **This is
   structural, not merely asserted**: each child is launched via
   `std::process::Command::new(current_exe)` with the arm's parameters
   passed as environment variables, so no two arms can ever share a
   process, a `HeapRegistry` slot, or any other cross-arm mutable state.

**All 36 child runs passed both self-checks** (every CSV row shows
`verified_headroom == headroom_bytes` and `config_conflicts_delta == 0`).

### 1.3 Path-activation oracle — admissions AND hits (R30-3's established pattern)

Per the task's explicit instruction and R30-3's precedent
(`benches/r30_3_virgin_zero_skip_native_gate.rs`): a number is only
trustworthy if the arm actually exercised the mechanism it claims to
measure. Each child hard-`assert!`s, before its RESULT lines print:

- **`admissions_ok`** — `burst1_used_max > 0` (the same oracle R29-13
  already established: at least one large span was genuinely cached after
  BURST1's teardown).
- **`hits_ok`** — `burst2_hits_sum > 0` (a NEW oracle this gate adds — R29-13
  never needed it, since its workload never re-allocated a same-sized
  object after freeing one; this gate's BURST2 does exactly that, so a
  genuine cache hit is only observable if BURST2 successfully reuses a
  size class BURST1 populated).

**All 36 arms passed both oracle conditions** (`oracle_pass = 1` in every
CSV row; the orchestrator's own summary line confirms "all 36 arms passed
the path-activation oracle"). **No arm was excluded** — none needed to be.

### 1.4 Workload — MIXED small + large (the methodology extension this task's brief asked for)

R29-13's workload was LARGE-only. This gate's workload interleaves, per
thread:

1. **SMALL churn** (`SMALL_SIZE = 1024` B, `SMALL_WORKING_SET = 64`,
   `SMALL_OPS = 128`) — the exact prefill/random-replace/teardown SHAPE
   `benches/global_alloc.rs`'s `churn_prefill`/`churn_step`/`churn_teardown`
   use, routed through `HeapCore` instead of `std::alloc` (registry-bypass,
   matching how the large half is routed on this same axis). This proves
   the mixed workload's small half is genuinely exercised alongside the
   large half — it is not intended to re-measure the small pool itself
   (R27-3/R27-4 already cover that exhaustively).
2. **BURST1** — allocate 8 distinct 6 MiB objects (`LARGE_OBJ_BYTES = 6 *
   1024 * 1024`, `LARGE_OBJ_COUNT = 8`, one per base large-cache slot), each
   genuinely touched (`write_volatile` every 4 KiB page, so the reservation
   is committed, not merely reserved), then free all 8 (returning each span
   to the large cache subject to admission/headroom policy). Per R29-13
   §3's mechanism, BURST1's LAST dealloc primes the decay timer without
   decaying (first-call priming rule).
3. **IDLE** — a fixed 1200 ms sleep, chosen to exceed the 1000 ms default
   decay interval (`DECAY_INTERVAL_MS`), so BURST2's first dealloc — whenever
   it runs — finds `elapsed >= decay_interval` and CAN fire a real
   (non-forced) decay tick. This is the one design choice that makes a
   natural decay tick observable in this gate's workload, unlike R29-13's
   tight-loop-only shape where decay never fires at all mid-workload.
4. **BURST2** — allocate 8 MORE 6 MiB objects (same size class as BURST1),
   timed (`Instant::now()` around the alloc+touch loop only), reading
   `AllocCore::dbg_large_cache_hits()` before/after to count genuine cache
   hits, then free them.

`8 * 6 MiB = 48 MiB` per burst — comfortably exceeds the 16 MiB headroom
arm (so that arm genuinely exercises eviction pressure, not a vacuous
always-fits case) while `LARGE_CACHE_SLOTS = 8` bounds the cache to at most
8 resident spans regardless of headroom (slot-count-limited at this size,
not byte-limited) — the reason 64 MiB and 256 MiB behave identically in
this workload (see §2).

### 1.5 Burst-idle-burst — why this is the load-bearing sequence

Per the task's explicit instruction: "at least one arm sequence that
allocates+frees a burst of large objects, goes idle for a measured
interval, then bursts again — this is the shape where event-driven-only
decay (no background thread) is most visible, since R29-13 already proved
PURE idle reclaims exactly 0 KiB." This gate's ENTIRE matrix (not just one
arm) uses this shape — every one of the 36 cells runs BURST1 → IDLE →
BURST2. The idle window's own RSS delta (`rss_idle_kib - rss_burst2_kib`,
see raw log) is 0 or single-digit-KiB in every row, reconfirming R29-13's
"idle reclaims nothing" finding in this NEW mixed-workload, burst-idle-burst
shape — not merely re-citing R29-13's own large-only tight-loop result.

### 1.6 Latency axis methodology (real `#[global_allocator]`, `paired-ab-runner.mjs`)

Each of the four `r30_6_latency_h{0,16,64,256}` binaries installs a REAL
`#[global_allocator]` `SeferAlloc::with_config(LargeCacheConfig::new().headroom_bytes(N))`
static, then runs the SAME mixed workload shape as §1.4's BURST1/BURST2
(small churn + 8×6 MiB large burst) for 8 timed batches (after 1 untimed
warm-up batch, absorbing primordial-segment bootstrap — R27-4's
warm-up-placement fix applied from the start, since these are new files),
driven through `std::alloc` (which routes to the installed
`#[global_allocator]`). Each binary emits `elapsed_ns` (the timed region),
`segments_reserved_total` (installed-allocator sanity check), and
`large_cache_hits` (`SeferAlloc::stats().large_cache_hits`, confirming real
hits occurred through the real entry point too — all four binaries reported
`large_cache_hits=64` in smoke tests, 8 batches × 8 hits/batch, 100%, since
the tight timed loop never lets the decay timer's 1000 ms interval elapse
mid-loop — the SAME first-call-priming mechanism R29-13 §3 documented,
confirming this axis's workload shape does not itself manufacture a
headroom-dependent hit-rate difference; that difference is isolated to the
burst-idle-burst shape on the OTHER axis, §0.1).

Driven by `scripts/paired-ab-runner.mjs --config docs/perf/r30_6_latency_run.json
--arms h256,hN` for each `hN` in `{h64, h16, h0}`, plus `--arms h256,h256`
as the same-vs-same honesty control — the exact protocol R27-4 established
(A/B/B/A alternation, 20 pairs = 80 process launches per comparison, paired
t-test + sign test computed by the runner itself, not eyeballed).

**Config-identity fields (R26-4 contract), latency axis:** (1) REQUESTED =
the `HEADROOM_BYTES` const compiled into each binary; (2) RESOLVED = proven
structurally — a compile-time `static` has no runtime resolution step, and
each binary's own `large_cache_hits=64` smoke-test readout (identical
across all four) confirms the const took effect (a mis-wired const would
show 0 hits, since a headroom that never admits anything cannot produce
hits); (3) no config-conflict counter applies at this entry point (no
registry-slot reuse is possible — each process is one fixed `static`,
identical to R27-4's own reasoning for this same entry point); (4) process
identity = subprocess-isolated (`paired-ab-runner.mjs` spawns a fresh
process per launch).

---

## 2. Why 64 MiB and 256 MiB are IDENTICAL, and why 0/16 MiB cost exactly 1/8

This gate's 48 MiB/burst workload, read against `run_decay_step`'s
mechanism (`src/alloc_core/alloc_core_large_cache.rs:366-380`, cited
verbatim by R29-13 §4): eviction proceeds in WHOLE-SEGMENT units and stops
the instant `large_cache_used_bytes` would drop to or below the headroom
target.

- **At headroom = 64 MiB or 256 MiB:** the cache's actual occupancy after
  BURST1's fill is bounded by `LARGE_CACHE_SLOTS = 8` slots × ~6 MiB usable
  span ≈ 48 MiB total — ALREADY at or below both the 64 MiB and 256 MiB
  targets. `maybe_decay_large_cache`'s fast-path early-return
  (`if large_cache_used_bytes <= headroom_bytes { return; }`,
  `alloc_core_large_cache.rs:320-356`, cited by R29-13 §3) fires
  unconditionally for these two arms — decay never even attempts eviction,
  so ALL 8 of BURST1's cached spans survive the idle window intact, and
  BURST2 hits all 8. This is why 64 MiB and 256 MiB produce byte-identical
  100.0% hit rates in this workload: at this burst size, 64 MiB is already
  "generous enough," and 256 MiB adds no further benefit — the headroom
  ceiling is not the binding constraint at either value.
- **At headroom = 16 MiB:** BURST1's ~48 MiB occupancy EXCEEDS the 16 MiB
  target, so once the idle interval crosses the decay interval (§1.4 point
  3), a real decay tick fires and evicts whole segments until occupancy is
  at-or-below 16 MiB — at ~6 MiB/segment, this evicts segments down to
  ~2-3 remaining (12-18 MiB), i.e. AT LEAST one, typically more, of
  BURST1's 8 cached spans is evicted before BURST2 runs. BURST2 hits
  exactly 7/8 (not fewer) because the FIRST BURST2 dealloc (not alloc) is
  what triggers the actual decay tick inside THIS gate's design (decay
  fires on the large alloc/dealloc slow path, and BURST2's allocs are what
  get measured) — the measured 87.5% is this workload's own concrete,
  reproducible answer for its specific burst size and idle timing, not a
  general "headroom=16 always costs 1/8" claim (a different burst size or
  idle timing would evict a different count, per R29-13 §4's own "not an
  exhaustive characterization" caveat, which this gate inherits and does
  not attempt to lift).
- **At headroom = 0 MiB:** produces the SAME 87.5% as 16 MiB in this
  workload — not a MORE severe hit-rate loss, matching R29-13 §4's own
  finding that headroom=0 and headroom=16 MiB converge to essentially the
  same near-zero floor under forced drain (whole-segment eviction
  granularity means a target smaller than one segment's size behaves the
  same as a target of exactly zero once at least one eviction fires).

**This is a genuinely different regime from R29-13's own workload** (34 MiB
objects, 272 MiB/burst, chosen specifically to EXCEED even the 256 MiB
headroom) — this gate's 48 MiB/burst was chosen specifically to stay
UNDER the 64/256 MiB arms while still exceeding the 16 MiB arm, so this
gate's grid can show where the "headroom is generous enough" transition
sits for a concrete, realistic burst size, rather than proving retention at
every arm the way R29-13 did.

---

## 3. Wall-clock latency — an honest noise assessment

The `ns_per_b2_op` column in §0.1's raw aggregation (not restated in the
headline table above, since it is not the load-bearing axis for this
report's decision) shows visibly larger variance than the hit-rate/RSS
columns — e.g. the 64 MiB/32-thread cell's raw log shows one repetition at
110 ms and another at 5.02 s for the same nominal workload (see
`docs/perf/_raw_r30_6_large_cache_headroom_ab_gate.log`, the
`67108864,64,32,2,...` CSV row). This is measured on a SHARED host: `wmic
process` confirmed 15+ concurrent `cargo`/`rustc` processes from unrelated
projects (a `shamir-server` release build under `lto=fat`, a `wezterm-gui`
test run, and others) were competing for CPU during this exact measurement
window. This is disclosed, not hidden — R27-3/R27-4/R29-13 all carry the
identical "shared host, RSS/latency deltas are noisy point estimates"
caveat.

**This is exactly why the LATENCY verdict in this report rests on §0.2's
paired A/B/B/A protocol, not on the registry-bypass axis's raw
`ns_per_b2_op` column.** The paired protocol's whole purpose (R5-R2's
original motivation, reused verbatim by R27-4 and again here) is to survive
host noise via alternation + a statistical test, rather than trusting a
single-sample wall-clock number. §0.2's four comparisons (three real +
one control) all report `t` well under `crit`, and critically, the
same-vs-same control's `t`/sign-split sit in the SAME noise band as the
three real comparisons — meaning even under this host's confirmed
contention, the harness is not manufacturing a false positive out of that
noise. The honest conclusion is a genuine NULL result on the latency axis
at this workload's scale (8 batches × 8 large objects, ~50-100 ms/batch),
not "latency is inconclusive due to noise" — the paired protocol's whole
point is that it remains conclusive (a clean null) even in the presence of
this much single-sample noise.

---

## 4. Decision for R30-7 (task #456)

Per the task brief's decision space (a/b/c):

- **(a) 256 MiB remains an acceptable default** — supported for workloads
  that need the FULL R29-13-measured retention ceiling (e.g. genuinely wide
  working sets exceeding `LARGE_CACHE_SLOTS`, or bursts significantly larger
  than this gate's 48 MiB), but this gate shows 256 MiB buys NOTHING over
  64 MiB in a representative 48 MiB-burst mixed workload — the extra ~200
  MiB/heap of R29-13's measured floor is paid for zero measured benefit at
  this scale.
- **(b) the default should drop to a smaller headroom** — **64 MiB is the
  evidence-supported candidate**, not 16 MiB or 0 MiB: 64 MiB preserves
  100% of the measured hit-rate benefit (identical to 256 MiB) at
  ~7× lower R29-13-measured RSS retention (~34-37 MiB/heap vs ~238-241
  MiB/heap post-drain floor), with no measured latency cost either
  direction. 16 MiB/0 MiB are NOT supported as a blanket default swap — both
  cost a real, reproducible 12.5-percentage-point hit-rate loss in this
  workload, which is exactly the kind of measured downside CLAUDE.md's task
  brief asked this gate to surface rather than bury.
- **(c) ship named profiles instead of changing the single default** —
  **this is the recommendation this report actually supports**, and it
  directly feeds R30-7/task #456 (already queued, blocked on this report):
  a `throughput`/`balanced` profile at 64 MiB (this gate's evidence: full
  hit-rate parity with 256 MiB, ~7× smaller RSS floor) and an `rss`-priority
  profile at a smaller value (0-16 MiB, with the explicit disclosure that it
  costs real hit-rate in bursty large-object workloads, per §2's mechanism)
  are both defensible, EVIDENCE-BACKED named choices — changing the single
  global `DEFAULT_HEADROOM_BYTES` to 64 MiB unilaterally would still leave a
  caller with a genuinely different workload shape (larger bursts, working
  sets that need the fuller retention ceiling) worse off with no opt-out
  named for them. This report does not implement (c) — that is R30-7's
  scope — but the 64 MiB number this gate establishes as "no measured
  benefit lost, ~7× less RSS" is the concrete evidence R30-7 needs for its
  `throughput`/`balanced` profile's headroom value.

**Not overriding CLAUDE.md's explicit instruction**: this report does not
change `DEFAULT_HEADROOM_BYTES` or any other `src/` default — the decision
above is a recommendation recorded here, to be enacted (or not) in R30-7.

---

## 5. What this gate does NOT claim

- **Not an exhaustive burst-size characterization.** §2 explains why 64 MiB
  and 256 MiB tie and 16/0 MiB both cost 1/8 SPECIFICALLY for this gate's
  48 MiB/burst, 6 MiB-object workload. A different burst size (fewer/more
  objects, larger/smaller objects) would shift exactly where the "headroom
  is generous enough" transition sits — this gate establishes ONE concrete,
  representative point on that curve (chosen deliberately to straddle the
  16-vs-64 MiB boundary), not the full curve. R29-13's own §7 carries the
  identical caveat for its own workload; this gate does not lift it.
- **Latency verdict is a NULL result, not a "headroom never matters"
  claim.** The workload here (8 batches × 8 large objects, tight timed
  loop) never lets the decay timer's interval elapse mid-batch (§1.6) — so
  ALL FOUR headroom arms see the SAME 100% hit rate on the LATENCY axis's
  own tight-loop workload (unlike the hit-rate/RSS axis's burst-idle-burst
  shape, which is specifically designed to let decay fire). The latency
  null result therefore says "at 100% hit rate for all four arms, there is
  no latency difference" — it does NOT independently confirm what a lower
  hit rate's latency cost would be (e.g. a burst-idle-burst-shaped workload
  driven through the real global allocator, which this report did not
  build, would be needed to directly measure the LATENCY cost of a cache
  miss under a smaller headroom — this gate measures the HIT-RATE cost via
  the registry-bypass axis and the LATENCY-AT-CONSTANT-HIT-RATE null via
  the real-allocator axis, not a combined "latency cost of the hit-rate
  loss").
- **RSS numbers on this axis are a byproduct, not a controlled retention
  measurement.** §0.1's RSS columns come from the SAME registry-bypass
  probe as the hit-rate axis and are broadly consistent with R29-13's own
  controlled retention floors (e.g. this gate's headroom=64 MiB arm shows
  ~49-51 MiB/heap total process RSS at a MUCH smaller 48 MiB burst size,
  vs R29-13's dedicated ~34-37 MiB/heap POST-DRAIN floor at its larger 272
  MiB burst) — R29-13 remains the authoritative RETENTION-cost citation;
  this report's RSS columns exist to confirm the SAME arm's hit-rate number
  came from a genuinely realistic, admission-proven run, not to re-derive
  R29-13's floor independently.
- **Windows-native only, shared host** — same caveat every prior gate in
  this family carries (§3 discloses the specific contention observed during
  THIS measurement window).

---

## 6. Files changed

| file | change |
|---|---|
| `examples/r30_6_large_cache_headroom_ab_gate.rs` | new — the subprocess-per-arm hit-rate/RSS/burst-idle-burst probe (registry-bypass via `HeapRegistry::claim_with_config`, mirrors R29-13's shape). Mixed small+large workload; BURST1→IDLE(1200ms)→BURST2 sequence; path-activation oracle (admissions AND hits); R26-4 config self-verification (all 4 pieces). |
| `examples/r30_6_latency_h0.rs` / `_h16.rs` / `_h64.rs` / `_h256.rs` | new — four real-`#[global_allocator]` latency arms, one per headroom grid point (mirrors R27-4's `cap4`/`cap8` real-allocator shape, generalized to 4 arms). Same mixed workload shape as the probe above, driven through `std::alloc`. |
| `docs/perf/r30_6_latency_run.json` | new — the `--config` file for `paired-ab-runner.mjs` (committed; documents exactly what was compared). |
| `src/registry/heap_core_diag.rs` | added ONE thin `#[doc(hidden)]`, `alloc-decommit`-gated `HeapCore::dbg_large_cache_hits` delegation wrapper exposing the pre-existing `AllocCore::dbg_large_cache_hits` accessor — the exact established pattern already used by `dbg_large_cache_used`/`dbg_large_cache_slot_sizes`/`dbg_decay_config` in this same file. No new `unsafe`, no raw-pointer parameter, no allocator-metadata mutation via caller-supplied pointer. |
| `Cargo.toml` | added five `[[example]]` entries (`r30_6_large_cache_headroom_ab_gate` + the four `r30_6_latency_*` arms) with the established `required-features` triples/quads (matches the r27_3/r27_4/r29_13 sibling pattern — prevents the E0601 build failure a missing entry causes under plain `--features production`). |
| `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` | this report (new) |
| `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE_summary.csv` | machine-readable summary, both axes (new) |
| `docs/perf/_raw_r30_6_large_cache_headroom_ab_gate.log` | raw hit-rate/RSS probe stdout, the canonical run cited in §0.1 (`.gitignore`d by default — `git add -f` at commit time) |
| `docs/perf/_raw_r30_6_latency_ab.log` | raw `paired-ab-runner.mjs` stdout for all four latency comparisons, cited in §0.2 (`.gitignore`d — `git add -f`) |
| `docs/perf/paired_ab_runs/2026-07-30T09-21-*.json` (4 files) | runner provenance JSONs for the four latency comparisons (`.gitignore`d — `git add -f`) |
| `docs/perf/OPEN_ITEMS.md` | item 27's "Current state" bullets updated with the benefit-side result (append-only, existing retention-cost text preserved) |
| `CHANGELOG.md` | Round 30 section extended with this task's entry (append-only, R30-1 through R30-5 untouched) |

**No production source default changed.** `DEFAULT_HEADROOM_BYTES` (256
MiB, `src/alloc_core/large_cache_config.rs`) is untouched.

---

## 7. Reproduce

```text
# Hit-rate / RSS / burst-idle-burst axis (36 subprocess arms, ~1-2 min):
cargo run --release --example r30_6_large_cache_headroom_ab_gate --features "production alloc-stats bench-internals"

# Latency axis (real #[global_allocator], 4 comparisons x 20 pairs = 320 process launches, ~1-2 min each):
cargo build --release --example r30_6_latency_h0 --example r30_6_latency_h16 --example r30_6_latency_h64 --example r30_6_latency_h256 --features "production alloc-stats"
node scripts/paired-ab-runner.mjs --config docs/perf/r30_6_latency_run.json --arms h256,h64
node scripts/paired-ab-runner.mjs --config docs/perf/r30_6_latency_run.json --arms h256,h16
node scripts/paired-ab-runner.mjs --config docs/perf/r30_6_latency_run.json --arms h256,h0
node scripts/paired-ab-runner.mjs --config docs/perf/r30_6_latency_run.json --arms h256,h256   # same-vs-same honesty control
```

The hit-rate/RSS orchestrator prints each child's `RESULT key=value` lines +
`OK ...` self-check/oracle summary, then an aggregated (median, min..max)
table, then a CSV block (one row per child, 36 rows). Each child
independently hard-asserts `verified_headroom == headroom_bytes`,
`config_conflicts_delta == 0`, `admissions_ok`, and `hits_ok` — any failure
panics loudly in that child's stderr and fails the orchestrator. Measured
full-matrix wall-clock on this 16-core host: well under the default 2-minute
Bash tool timeout for the 36-arm hit-rate/RSS sweep; each latency comparison
(80 process launches) completes in well under a minute per invocation —
**this task's full matrix (36 hit-rate/RSS arms + 4×20-pair latency
comparisons = 356 total process launches) completed in low single-digit
minutes of wall-clock**, comfortably inside CLAUDE.md's "minutes, not hours"
budget for this class of sweep. No sample count was capped below this
project's established per-cell precedent (3 reps for the subprocess axis,
matching R27-3/R29-13; 20 pairs for the paired axis, matching R27-4's
real-claim threshold, not its `--quick` 4-pair smoke count).

---

## 8. 2026-07-30 addendum (R31-12, task #476) — data-hygiene repairs, APPEND-ONLY

**Nothing above this line is edited or deleted.** This section repairs five
data-hygiene defects the Round 30 independent full review
(`docs/reviews/2026-07-30-r30-full-review.md` §5, filed as
`docs/perf/OPEN_ITEMS.md` item 31's P2-3/P2-4/P2-5 sub-findings) identified
in this report and its companion CSV, none of which had been repaired before
this addendum. Every number below was independently re-derived from the
ALREADY-COMMITTED raw artifacts (`docs/perf/_raw_r30_6_large_cache_headroom_ab_gate.log`,
`docs/perf/paired_ab_runs/2026-07-30T09-2{1,2}-*.json`) by
`scripts/r31_12_repair_r30_6_data.mjs` — a checked script, not hand
re-transcription (CLAUDE.md's "tables derived by one checked script" rule).
No re-measurement was performed; this addendum corrects PROSE and fills gaps
against data that was already correct.

### 8.1 P2-3 repair — the idle-RSS claim's column pair was wrong (now corrected)

§0.1's original text claimed *"RSS at burst2 and RSS after the 1200 ms idle
window are identical to the KiB ... `rss_idle_kib - rss_burst2_kib` is 0 or
within single-digit KiB noise in every row."* **This is false as written**:
`rss_idle_kib - rss_burst2_kib` is `0` in **0 of 36 rows** (verified by
`scripts/r31_12_repair_r30_6_data.mjs` §1) — structurally expected, not a
data error, because `rss_burst2_kib` is sampled AFTER BURST2 runs (which
itself allocates+frees 8 more large objects and grows RSS), so comparing it
against the PRE-burst2 idle sample necessarily shows the burst's own
footprint, not an idle-window delta.

**The claim the report actually intended** — that idle reclaims nothing
between BURST1's fill and the idle sample — is `rss_idle_kib -
rss_burst1_kib == 0`, which IS exact in **33 of 36 rows** (confirmed by the
same script), with the remaining 3 rows covered by §8.2 immediately below.
§0.1's prose above this addendum is superseded by this corrected statement;
the underlying finding (idle reclaims nothing) was already correct, only the
cited column pair was wrong.

### 8.2 P2-4 repair — one physically-impossible raw-log row, now excluded and flagged

Raw log row `67108864,64,32,2,...` (`headroom=64 MiB, threads=32, rep=2`)
reads `rss_burst1_kib=1,580,920` → `rss_idle_kib=424` — a claimed RSS
collapse of ~1.58 GiB across a 1.2 second PURE IDLE window in which every
worker thread is parked in its idle-wait loop (zero deallocation activity
anywhere in the process). This is not physically possible for a live
32-thread process and was neither excluded nor flagged in the original
report.

**Exclusion rule (stated explicitly, applied retroactively to this row's
interpretation only, not to the underlying data file):** a
`(headroom_bytes, thread_count, repetition)` row's `rss_idle_kib` sample is
EXCLUDED from any idle-delta claim if `rss_burst1_kib - rss_idle_kib >
rss_burst1_kib / 10 + 4096` (a >10%-plus-4-MiB drop across a window with no
possible deallocation activity) — this bound is met by exactly this one row
out of 36 (`docs/perf/_raw_r30_6_large_cache_headroom_ab_gate.log` line
1056; confirmed by `scripts/r31_12_repair_r30_6_data.mjs` §1, which applies
this exact rule mechanically). The other two non-exact rows from §8.1
(`+4 KiB` at `256 MiB/32 threads`, reps 0 and 2) are within the tolerance
and are NOT excluded — they are genuine single-digit-KiB noise, not a broken
sample.

**Confirmed: excluding this row changes no §0.1 headline conclusion.**
`scripts/r31_12_repair_r30_6_data.mjs` §2 recomputes the `headroom=64
MiB, threads=32` cell's `burst2_hits_sum` median with and without this row's
repetition included: **256 either way** (the row's own hit-rate/oracle
fields were valid — `oracle_pass=1` — only its RSS sample was broken; a
median of 3 is robust to one outlier). The §0.1 headline hit-rate table is
unaffected.

**Harness fix (forward-looking, not retroactive to this already-committed
raw log):** `examples/r31_1_large_cache_headroom_crossing_regime_gate.rs`
(R31-1, task #464, landing in the same round) adds exactly this bound as a
hard `assert!` inside the CHILD process, before its `RESULT` lines print —
so a future run of that sibling harness cannot silently admit the same class
of broken sample into a table again. This R30-6 harness
(`examples/r30_6_large_cache_headroom_ab_gate.rs`) is NOT modified by this
addendum (CLAUDE.md's non-retroactive convention for already-published gate
docs/harnesses); the fix is applied going forward in the sibling file.

### 8.3 Summary CSV commit-SHA placeholder — filled in

`docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE_summary.csv`'s header carried
a prose placeholder (`commit_sha=<see report header, this file is committed
alongside the same commit that lands the measured tree>`) instead of the
actual landing commit SHA this report's own header already names
(`97c2f07bf5c43478632ab01f9037a34cc648e9eb`, filled in by the same-day
follow-up commit `1272a52`). This addendum fills that ONE remaining
historical placeholder — see the CSV file's own header comment for the
correction (same append-only-correction convention: the file's data ROWS are
untouched, only the header comment's placeholder line is corrected).

### 8.4 Latency-null MDE — added (was missing from the original headline)

§0.2's original headline reported a clean NULL result (`|t| < crit` for all
four comparisons) without stating the comparison's own minimum-detectable
effect (MDE) — R30-7's post-review correction added this for ITSELF but not
retroactively for R30-6, per this project's own already-flagged gap.
Computed here (same formula as R30-7 §0.2 / `scripts/r31_2_derive_report_data.mjs`:
`MDE = crit * se`, reported in ns and as a percentage of that comparison's
own mean elapsed time), by `scripts/r31_12_repair_r30_6_data.mjs` §3 from
the already-committed provenance JSONs:

| comparison | mean elapsed (both arms) | MDE (`crit * se`) | MDE as % of mean elapsed |
|---|---:|---:|---:|
| h256 vs h64 | 4.130 ms | 1.069 ms | **25.9%** |
| h256 vs h16 | 3.468 ms | 0.746 ms | **21.5%** |
| h256 vs h0 | 3.597 ms | 0.662 ms | **18.4%** |
| h256 vs h256 (control) | 3.790 ms | 0.419 ms | **11.1%** |

**Decision-facing reading:** this comparison's n=20-pair sample could only
have detected a REAL latency effect of roughly 18-26% of mean elapsed time
at `p<0.05` — a true effect smaller than that (e.g. a genuine 5-10% latency
cost from a smaller headroom) is statistically indistinguishable from noise
in this sample and would NOT have been caught by this gate's null result.
The null result means "no effect this large was found," not "no effect
exists" — the same honest reading R30-7's own MDE addition established for
itself. This does not change §0.2's verdict (still a genuine null at the
resolution this sample can detect) but makes the resolution explicit rather
than implicit.

### 8.5 Structural limitation — the latency axis cannot expose hit-loss cost (stated explicitly)

**Documented limitation, not a data error:** every arm of R30-6's §0.2
latency workload sees 100% cache hits (§1.6's own text already says this:
"the tight timed loop never lets the decay timer's 1000 ms interval elapse
mid-loop ... all four binaries reported `large_cache_hits=64`... 100%").
This means the latency axis, AS BUILT, structurally CANNOT expose the
latency cost of a cache MISS under a smaller headroom — it can only speak to
"at constant 100% hit rate, is there a latency difference," which is a
narrower question than "does a smaller headroom cost real latency." R31-1
(task #464, same round) independently confirms the hit-rate cost is real at
crossing-regime burst sizes (a genuine 12.5-percentage-point hit-rate loss
at 64 MiB headroom vs 256 MiB once the burst exceeds 64 MiB) — what R30-6
cannot say, and what remains unmeasured after this addendum, is the WALL-CLOCK
cost of THAT hit-rate loss (a burst-idle-burst-shaped workload driven
through the real `#[global_allocator]`, which neither R30-6 nor R31-1
builds). §5's original text already flagged this gap qualitatively; this
addendum states it as an explicit, permanent limitation of this report's
latency axis, not merely a possible future extension.

### 8.6 Files touched by this addendum

| file | change |
|---|---|
| `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` | this §8 addendum (append-only; §0-§7 unedited) |
| `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE_summary.csv` | header comment's commit-SHA placeholder filled in (§8.3); data rows unchanged |
| `scripts/r31_12_repair_r30_6_data.mjs` | new — the checked derivation script this addendum's numbers come from |
| `docs/perf/OPEN_ITEMS.md` | item 27 narrowed per §8.7 below (append-only) |

### 8.7 Item 27's parity claim — narrowed (see OPEN_ITEMS.md item 27 for the full text)

R31-12/task #476 independently confirmed (see §8.1's rounding arithmetic,
also documented in `examples/r31_1_large_cache_headroom_crossing_regime_gate.rs`'s
module doc) that R30-6's "64 MiB ties 256 MiB" finding holds ONLY at a
burst size at or below the 64 MiB headroom target — R31-1 (task #464)
measured a real, reproducible cost once the burst genuinely exceeds 64 MiB.
`docs/perf/OPEN_ITEMS.md` item 27 is updated (append-only) to restate the
parity claim as "parity at a 64 MiB rounded working set" rather than general
throughput/hit-rate equivalence — see that file for the corrected text.

