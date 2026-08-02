# R32-12 — large-cache occupancy bitmask (F8 sub-change (2)), task #503

**Finding under test:** F8 in
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` — the large-cache's
four scans (best-fit lookup, free-slot search, FIFO-oldest search, and the
eviction retry loop that re-runs the last two) all walk the same
56-byte-per-slot `[Option<CachedLarge>; N]` array-of-structs to read ONE
field each. The survey splits its proposed fix into three sub-changes with
different risk profiles and explicitly recommends measuring the low-risk
one (an occupancy bitmask replacing the free-slot search) SEPARATELY before
committing to the higher-risk ones (parallel `usable_size`/`seq` sidecars).

**Verdict: shipped the bitmask ALONE (sub-change (2)), per the survey's own
"may be the whole shippable subset" framing.** The sidecars (sub-changes
(1)/(3)) were NOT built — see §6 for why, and what would need to change for
that decision to be revisited.

**Allocator layer under test (CLAUDE.md's R30-8/R31-0 rule):** §4's native
wall-clock microjudge and §5/§6's iai-callgrind benches both drive `AllocCore`
(§4: bare, via `AllocCore::new()`, mirroring R31-8's own microjudge-layer
choice) or `HeapCore` via `HeapRegistry::claim()` (§5/§6's new
`large_cache_free_slot_search_{prefill,cycle}_only` benches, mirroring
`large_cache_prefill_only_4mib`/`large_cache_hit_only_4mib`'s R31-0
corrected-layer choice — the real `#[global_allocator]` dispatch chain, not
bare `AllocCore`). `large_cache_find_free_slot` itself is a single,
non-duplicated function reached identically from both entry points (it has
no separate "substrate" vs "global-allocator" implementation to diverge
between), so a scan-cost result measured through either layer generalizes to
the other — the layer choice here is about harness convenience (bare
`AllocCore` needs no registry bootstrap for the pure-scan wall-clock probe),
not a substrate-vs-shipped-chain distinction the way R31-0's original
finding was.

---

## 0. What shipped

`AllocCore::large_cache_occupied: u64` (`src/alloc_core/alloc_core.rs`) — an
occupancy bitmask over the COMBINED base+extension index space (bit `i` set
⟺ combined slot `i` holds `Some(CachedLarge)`). Replaces
`large_cache_find_free_slot`'s base-array scan
(`self.large_cache.iter().position(|s| s.is_none())`,
`src/alloc_core/alloc_core_large_cache.rs`) with
`large_cache_occupied.trailing_ones() as usize` — the index of the lowest
CLEAR bit, found without touching the `large_cache` array at all. The
extension-sidecar fallback path (when the base is full and
`large-cache-extended` is on) is unchanged (still a linear `.position()`
scan over the 32-slot extension array — that scan was never sub-change
(2)'s target; see §6 for why it was left alone).

Best-fit lookup (`alloc_core_large.rs`) and FIFO-oldest search
(`oldest_occupied_slot`, `alloc_core_large_cache.rs`) — sub-changes (1) and
(3) — are **unchanged**. They still walk the array reading
`slot.usable_size`/`c.seq` respectively.

## 1. Correctness argument — lockstep-maintenance site enumeration

Per CLAUDE.md's site-enumeration discipline (task #494's unregister-site
enumeration is the established precedent for this rigor level): every site
that sets/clears an `Option<CachedLarge>` slot to `Some`/`None` must also
set/clear the corresponding bit. Verified by `grep -rn "large_cache\[.*\]\s*=\|large_cache\[.*\]\.take()\|ext\.slots\[.*\]\s*=\|ext\.slots\[.*\]\.take()" src/` —
**exactly two functions** in the whole crate ever write a slot:

1. **`large_cache_slot_set`** (`alloc_core_large_cache.rs`) — sets bit `idx`
   in BOTH the base (`idx < LARGE_CACHE_SLOTS`) and extension arms,
   unconditionally (a slot transitions None → Some on every call; callers
   only ever call this on a slot just proven empty by
   `large_cache_find_free_slot`).
2. **`large_cache_slot_take`** (`alloc_core_large_cache.rs`) — clears bit
   `idx` in BOTH arms, but only AFTER the `.expect()`/extension-sidecar path
   proves an entry really was there (so the bitmask is never cleared for an
   already-empty slot — a defensive ordering choice: mutate the array first,
   then the bitmask, so a panic on an empty slot cannot leave the bitmask
   claiming occupancy the array doesn't have).

Every other mutation path in the crate — deposit (`alloc_core.rs`'s
`dealloc`'s Large branch, `alloc_core_large.rs`'s
`reclaim_large_segment`), cache-hit take (`alloc_core_large.rs`'s
`alloc_large`), and eviction (`evict_at_least`, `evict_one_oldest`,
`alloc_core_large_cache.rs`) — funnels through one of these two functions
(confirmed by `grep -n "large_cache_slot_set\|large_cache_slot_take" src/`
— 4 call sites total, all in `alloc_core.rs`/`alloc_core_large.rs`,
none bypassing the two maintenance functions). The bitmask is therefore
correct **by construction**, not by convention: there is no third call
site that could drift out of sync.

The bit is a **derived value, not a duplicated field** — `Option::is_some()`
already carries no independent state that could disagree with itself, so
this is NOT the X5-shaped replication hazard the survey warns sub-changes
(1)/(3) carry (see §6). It is state that must be *maintained*, but nothing
about it can independently diverge from ground truth the way a copied
`usable_size` could.

### 1.1 Falsification-first invariant test

`tests/large_cache_occupancy_bitmask_invariant.rs` (4 tests, run under both
`alloc-core,alloc-decommit` and `alloc-core,alloc-decommit,large-cache-extended`):
compares `dbg_large_cache_occupied_bits()` bit-for-bit against the actual
per-slot occupancy (`dbg_large_cache_slot_sizes()` /
`dbg_large_cache_extended_slot_sizes()`, independently read, not re-derived
from the bitmask) at every observable step of a real alloc/dealloc/evict
sequence:

- `fresh_cache_bitmask_is_zero` — a freshly constructed `AllocCore` starts
  with `large_cache_occupied == 0`.
- `single_deposit_and_hit_bitmask` — one deposit sets exactly one bit; the
  following cache-hit re-alloc clears it.
- `base_slots_fill_sets_all_bits` — 8 distinct-size deposits set all 8 base
  bits (`0xFF`), nothing above.
- `eviction_clears_bitmask_bit` — filling the base then forcing FIFO
  eviction (via a budget clamp — see the false-start note below) nets to
  the same occupied count, bitmask still bit-for-bit correct.

**Two genuine false starts, both self-caught by the invariant checker
before either was mistaken for a bug in the bitmask itself:**

1. `eviction_clears_bitmask_bit`'s first draft (no explicit budget
   override) failed under `large-cache-extended` with `count_ones() == 29`
   instead of the expected 8 — traced to `large-cache-extended`'s resolved
   DEFAULT budget being finite (`DEFAULT_EXTENDED_BUDGET_BYTES`,
   `large_cache_config.rs`), which triggered budget-driven eviction DURING
   the fill loop itself, leaving fewer than 8 base slots occupied before
   the test's own eviction-triggering step ran. Fixed by explicitly setting
   an unbounded budget (`dbg_set_large_cache_budget(None)`) for the fill
   phase, matching `base_slots_fill_sets_all_bits`'s existing convention.
2. After that fix, the same test failed again with `count_ones() == 9`
   instead of 8 — under `large-cache-extended` with an unbounded budget, a
   full base cache does NOT force eviction on the 9th deposit; it
   materialises the extension sidecar and admits there instead (a free slot
   via extension is preferred over eviction when the budget permits it —
   correct pre-existing admission-loop behaviour, `large_cache_slot_set`'s
   own call site in `alloc_core_large.rs`). The test's assumption
   ("base full ⇒ next deposit must evict") was wrong under that feature
   combination. Fixed by clamping the budget to the exact used-bytes total
   after the fill, which forces the admission loop's
   `if !self.evict_one_oldest() { break; }` branch to run regardless of
   extension availability — the property the test actually wants to
   observe, independent of whether the extension happens to be available
   as an alternative to eviction.

Neither false start was a defect in the bitmask maintenance itself — both
were the test's own assumptions about admission-loop behavior under
`large-cache-extended`'s different default budget/materialisation
semantics. Both are exactly the kind of thing a falsification-first
invariant check is supposed to catch before a wrong assumption ships as a
passing (vacuous) test.

## 2. Path-activation oracle (CLAUDE.md R30-8)

`examples/r32_12_large_cache_free_slot_search_isolation.rs` asserts
`dbg_large_cache_hits()`'s before/after delta equals `ROUNDS` (200,000) —
every timed `alloc(cycle_size)` call is a genuine cache HIT against the
entry the prior iteration's `dealloc` deposited, never a miss (fresh OS
reservation), which would silently measure something else. Confirmed:
`oracle_hit_delta=200000` in every run (see raw output below).

## 3. Same-workload-regime discipline (CLAUDE.md R30-6/R31-1 rule)

Cost and benefit are measured in the SAME workload regime: the cache is
genuinely near-full (7 of 8 base slots permanently occupied by decoys,
never touched again after population) for both the native wall-clock probe
and the Ir-level decomposition below — not an empty-or-near-empty cache,
which is the trivial case the survey's own text explicitly warns against
("the single most likely way to accidentally publish a null result here").
The one free slot sits at the LAST scanned index (worst case for a "find
first `None`" linear scan), matching R31-8's own worst-case-position
convention for the sibling best-fit-scan microjudge.

## 4. Native wall-clock measurement (paired A/B, N=8, production's actual base cache size)

**Harness:** `examples/r32_12_large_cache_free_slot_search_isolation.rs` +
`examples/_shared/r32_12_large_cache_free_slot_search_workload.rs`, driven
through bare `AllocCore` (same microjudge-layer choice R31-8 documents and
justifies — this isolates the scan itself, not the full
`#[global_allocator]` dispatch chain). BEFORE/AFTER measured via
`git worktree add` at the base commit (this scan's implementation changed
in-place, not behind a Cargo feature — same non-feature-gated before/after
pattern `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md` established), run
through `scripts/paired-ab-runner.mjs --config scripts/_r32_12_free_slot_search_ab.json`
(A/B/B/A protocol, 20 pairs).

| comparison | mean Δ (before − after) | t | crit (p<0.05) | sign test | verdict |
|---|---:|---:|---:|---|---|
| before vs after | +0.80 ns | 0.492 | 2.101 | before-faster 8/20, after-faster 12/20 | **NOISE** (not significant) |
| after vs after (same-vs-same control) | −0.40 ns | −0.394 | 2.101 | 11/20 vs 9/20 | NOISE, as expected (harness sanity) |

**Both arms produced a 200,000/200,000 cache-hit oracle** in every one of
the 20+20 process launches (checked per-launch by the microjudge's own
`assert_eq!` before it ever emits a RESULT line — a launch that failed the
oracle would abort before printing, not silently pass through).

**Reading this: the survey's own prediction is confirmed, not falsified.**
At `scan_bound = 8` (production's actual base cache size without
`large-cache-extended`), a linear "find first `None`" scan over an
already-tiny array is cheap enough that its cost sits below this host's
wall-clock noise floor (~0.4–2 ns/pair standard error on this hardware).
This is the SAME conclusion R31-8's own best-fit-scan microjudge would
reach at N=8 in isolation — the wall-clock win from removing an 8-element
scan is real in instruction-count terms (§5) but too small to clear the
noise floor of a process-level timer on this host.

## 5. Ir-level decomposition (iai-callgrind via WSL/Valgrind — same host, `--features "production bench-internals"`)

Wall-clock at N=8 is noise-dominated (§4); instruction-count (`Ir`) has a
far lower noise floor and is where a change this small should actually show
up. Added a dedicated bench pair,
`benches/perf_gate_iai.rs::large_cache_free_slot_search_{prefill,cycle}_only`,
mirroring the pre-existing `large_cache_prefill_only_4mib`/
`large_cache_hit_only_4mib` shared-prefix design (R23-3 subtraction
pattern): `prefill_only` deposits 7 permanent decoys (occupying combined
slots 0–6) then primes the one cycling slot once; `cycle_only` is the
BYTE-IDENTICAL prefix plus 8 more alloc/dealloc admission rounds against
the cycling slot, each dealloc paying the worst-case free-slot-search cost
(walk all 7 occupied decoys before finding the one free slot).

Measured via `git worktree add` at the base commit (same technique as §4),
`--target-dir` isolated per tree, both built with
`CARGO_BUILD_RUSTC_WRAPPER=""` (sccache/WSL toolchain-wrapper conflict,
same fix R32-10 documents), both run through the SAME iai-callgrind
harness file (copied verbatim into the BEFORE tree — the harness itself did
not change, only the `large_cache_find_free_slot` implementation it calls
transitively):

| bench | before (Ir) | after (Ir) | Δ |
|---|---:|---:|---:|
| `large_cache_free_slot_search_prefill_only` (shared prefix) | 7,924 | 7,924 | **0** |
| `large_cache_free_slot_search_cycle_only` (prefix + 8 rounds) | 11,045 | 11,005 | −40 |

**Prefill arm is byte-identical (Δ=0), confirming the shared prefix is
truly shared** — the bitmask change does not touch the decoy-deposit path
at all (population goes through `large_cache_slot_set`, which does one
extra `|=` per deposit — invisible at this granularity because the OS
round-trip / header-write cost dominates a fresh deposit).

**Isolated marginal cost (shared-prefix subtraction, R23-3 pattern):**
`cycle − prefill` = 3,121 Ir (before) vs 3,081 Ir (after) for 8 admission
rounds → **−40 Ir / 8 rounds = −5.0 Ir per admission**, a small, real,
directionally-correct win: replacing an up-to-7-element linear scan with a
`trailing_ones()` bitmask lookup costs fewer instructions, exactly as
predicted, just not enough to matter in wall-clock terms at this scan
width.

## 6. Standing ±10 raw-Ir churn kill gate — stays flat

Per CLAUDE.md's kill-gate requirement, same WSL/Valgrind run as §5 (both
BEFORE and AFTER trees, same 5 standing small-object benches):

| bench | before (Ir) | after (Ir) | Δ | within ±10? |
|---|---:|---:|---:|---|
| `small_churn_16b` | 9,038 | 9,039 | +1 | yes |
| `aligned_churn_640b_a128` | 8,974 | 8,975 | +1 | yes |
| `churn_256b` | 9,038 | 9,039 | +1 | yes |
| `cold_alloc_free_256x16b` | 51,548 | 51,549 | +1 | yes |
| `recycle_alloc_free_256x16b` | 100,764 | 100,758 | −6 | yes |

All 5 within ±10 — this change is scoped to Large-object cache management
and does not touch the small-object hot path, confirmed rather than
assumed.

## 7. Why sub-changes (1)/(3) (the sidecars) were NOT built

Per the survey's own explicit steer ("The bitmask alone may be the whole
shippable subset; a gate should measure it separately from the sidecars
rather than bundling all three") and per this task's own instructions: only
proceed to the sidecars if the bitmask alone does NOT already capture most
of the measurable win, or if there is a clear, separately-justified case
for the higher-risk change. Both apply against building the sidecars now:

1. **The measured win at production's actual scan width (8) is honest and
   small — real in Ir (−5.0/admission), invisible in wall-clock.** Adding a
   REPLICATED-field hazard (the survey's own words: `usable_size`/`seq`
   would live in BOTH `CachedLarge` and a new sidecar array, needing
   lockstep maintenance at every `large_cache_slot_set`/`_take`/`evict_*`
   site — this task's own §1 enumeration shows exactly 2 sites for the
   bitmask; the sidecars would add the SAME 2 sites but with real duplicate
   DATA, not a derived bit, so a maintenance bug becomes possible in a way
   it structurally cannot be for the bitmask) is not justified by a −5
   Ir/admission win that doesn't clear the wall-clock noise floor.
2. **This is exactly the X5 (`OPEN_ITEMS.md [L]` item 20) failure shape the
   survey itself names as the risk**: "at n=3 the maintenance RMW on every
   transition is a net cost." The best-fit scan and FIFO-oldest scan both
   run on the SAME frequency class as the free-slot search (every large
   alloc/dealloc), so the sidecars' maintenance overhead (one extra array
   write per `large_cache_slot_set`/`_take` call, times two more arrays)
   would need to beat out a scan that is ALREADY this cheap at N=8 — a much
   higher bar than the bitmask cleared, on a mechanism the survey already
   flags as previously netting NEGATIVE at small N.
3. **No workload regime in production makes N large enough for the
   sidecars' win to plausibly clear their own maintenance cost.** The base
   cache is a fixed 8 slots; the extension (40 total) is `large-cache-extended`-gated,
   EXPERIMENTAL, and — per R31-3's own finding
   (`docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md`) — the
   scan-bound-alone contribution at 40 slots is dominated by eviction cost,
   not the scan width itself.

**This is not a rejected/failed measurement — it is the correctly-scoped
stopping point the survey's own risk stratification calls for.** If a
future workload profile or `large-cache-extended` policy change makes the
free-slot search (or best-fit/FIFO-oldest) a measured hot spot at a wider N
— i.e. if `docs/perf/OPEN_ITEMS.md`'s large-cache items ever motivate
raising `LARGE_CACHE_SLOTS` itself, or a future macro-benchmark
(`benches/macro_multiseg_steady_state.rs`-style) shows material scan cost
at the CURRENT N under real admission churn — that would be the trigger to
revisit sub-changes (1)/(3), armed with this task's own lockstep-site
enumeration (§1) as the template for the sidecars' correctness argument.

## 8. Test suite

- `tests/large_cache_occupancy_bitmask_invariant.rs` (new, 4 tests) — green
  under both `alloc-core,alloc-decommit` and
  `alloc-core,alloc-decommit,large-cache-extended`.
- `cargo test --release --features production` — green (235 test binaries;
  the one pre-existing flaky test,
  `xthread_large_free_tiny_size_huge_align_is_reclaimed`
  [`docs/CORRECTNESS_OPEN_ITEMS.md` item 14], is unrelated to this change
  and reproduces identically on `HEAD` before this task's edits).
- `cargo test --release --features "production large-cache-extended"` —
  green (same pre-existing flaky test, same unrelated cause).
- All pre-existing `large_cache_*`/`large_cache_extended_*`/
  `regression_large_cache_*` test files — green, unmodified.
- `tests/dbg_hook_safety_tripwire.rs` — green; the new
  `dbg_large_cache_occupied_bits()` test-only accessor added to
  `PURE_OBSERVERS` (zero-argument `&self` read of an already-plain `u64`
  field, no pointer, no mutation — same category as the pre-existing
  `dbg_large_cache_used`/`dbg_large_cache_slot_sizes` siblings in the same
  file).
- `docs/ARCHITECTURE.md`'s test-file-count line updated (234 → 235, per
  `tests/no_stale_doc_references.rs::architecture_test_file_count_matches_reality`).
- `cargo fmt --check` — clean.
- `cargo clippy -- -D warnings` (no features, `experimental`,
  `--all-features`) — clean, the three official CI feature-matrix rows.
- `cargo clippy --features "production large-cache-extended bench-internals" -- -D warnings` —
  clean.

## 9. Files changed

- `src/alloc_core/alloc_core.rs` — `large_cache_occupied: u64` field +
  compile-time width assertions (`LARGE_CACHE_SLOTS [+ LARGE_CACHE_EXTENDED_SLOTS] <= u64::BITS`).
- `src/alloc_core/alloc_core_large_cache.rs` — `large_cache_slot_set`/
  `large_cache_slot_take` maintain the bitmask; `large_cache_find_free_slot`
  uses `trailing_ones()` for the base-slot lookup; new
  `dbg_large_cache_occupied_bits()` test accessor.
- `tests/large_cache_occupancy_bitmask_invariant.rs` (new) — falsification-first invariant tests.
- `tests/dbg_hook_safety_tripwire.rs` — allowlist the new accessor.
- `benches/perf_gate_iai.rs` — new `large_cache_free_slot_search_{prefill,cycle}_only`
  bench pair (+ `not(alloc-decommit)` no-op stubs), registered in
  `library_benchmark_group!`.
- `examples/r32_12_large_cache_free_slot_search_isolation.rs` (new),
  `examples/_shared/r32_12_large_cache_free_slot_search_workload.rs` (new) —
  native wall-clock microjudge.
- `Cargo.toml` — `[[example]]` registration for the new microjudge.
- `scripts/_r32_12_free_slot_search_ab.json` (new) — `paired-ab-runner.mjs` config.
- `scripts/r32_12_derive_report_data.mjs` (new) — checked derive script (this report's tables).
- `docs/perf/R32_12_LARGE_CACHE_OCCUPANCY_BITMASK_GATE_summary.csv` (new,
  derived) — this report's own machine-readable summary.
- `docs/perf/_raw_r32_12_before_killgate.log`,
  `docs/perf/_raw_r32_12_after_killgate.log` (new, truncated per the
  R14-10 convention) — the Ir raw evidence §5/§6 cite.
- `docs/perf/paired_ab_runs/2026-08-02T21-17-17-448Z.json`,
  `docs/perf/paired_ab_runs/2026-08-02T21-17-29-807Z.json` — raw per-sample
  provenance for §4's wall-clock A/B and same-vs-same control.
- `docs/ARCHITECTURE.md` — test-file count 234 → 235.
- `docs/perf/OPEN_ITEMS.md` — close F8's item (this task).

## 10. Provenance / immutable source identity (CLAUDE.md R29-6 rule)

- **Base commit:** `e784dbc537752c4b4537a043130fc8da2b2573b1` (the tip this
  task started from — `docs(perf): fill R32-11's landing-commit SHA
  placeholder`).
- **Immutable tree SHA (this task's changed/added files staged into a
  scoped temporary index over the base commit, `git write-tree`):**
  `705cb3487c556e1bd3897644c1bae9ac1f3b1bd2`.
- **Landing commit:** `e88390bc88c863c8861d8bdda26fb49269cf9a89` (filled in
  this same-round follow-up commit, matching this session's own established
  `docs(perf): fill R3x-y's landing-commit SHA placeholder` pattern).
- **CPU/OS:** Intel Core i7-11800H @ 2.30GHz, Windows 10 Pro 10.0.19045
  (native, for the wall-clock A/B in §4); WSL2 Ubuntu 24.04 with Valgrind
  (for the Ir axis in §5/§6) — same physical host.
- **rustc:** 1.97.0 (both native Windows and WSL toolchains).

## 11. Reproduce

```text
# Native wall-clock A/B (§4):
cargo build --release --example r32_12_large_cache_free_slot_search_isolation --features "alloc-core alloc-decommit alloc-stats"
# BEFORE tree via git worktree add <path> e784dbc537752c4b4537a043130fc8da2b2573b1,
# same build command there, then:
node scripts/paired-ab-runner.mjs --config scripts/_r32_12_free_slot_search_ab.json
node scripts/paired-ab-runner.mjs --config scripts/_r32_12_free_slot_search_ab.json --arms after,after

# Ir decomposition + kill gate (§5/§6, WSL/Valgrind):
CARGO_BUILD_RUSTC_WRAPPER="" cargo bench --bench perf_gate_iai --features "production bench-internals"
# (run in both the current tree and a git worktree at the base commit with
# benches/perf_gate_iai.rs copied over, since the harness itself is
# unmodified between trees)

# Derive this report's tables + summary CSV from the raw numbers:
node scripts/r32_12_derive_report_data.mjs <landing_commit_sha>
```
