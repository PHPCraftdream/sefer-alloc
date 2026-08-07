# `sefer-region` — performance review (read-only)

**Date:** 2026-08-07
**Scope:** `crates/region` (crate `sefer-region`) — optimization/speedup opportunities in the
crate's own code and its new bench harness, ahead of the crates.io republish (task #656).
**Mode:** read-only investigation. No repository file was modified; this report is the only
artifact. No commit was made.
**Explicitly out of scope (already investigated this session, results accepted):** the
wrapper-overhead A/B question (README §"Wrapper overhead: measured, not assumed" — negative
result, no change warranted) and "add benchmarks" in the generic sense
(`benches/region_bench.rs` exists and was run for this review).
**Evidence base:** fresh full run of `cargo bench -p sefer-region --bench region_bench`
(numbers below); `bench-scale-tool 0.1.0` and `slotmap 1.1.1` sources read from the cargo
registry cache (`D:\system_artefact\cargo\registry\src\index.crates.io-1949cf8c6b5b557f\`);
`docs/BENCHMARKS.md`; `crates/region/tests/captrack_probe.rs` findings as published in
README §"Capacity growth".

Fresh run used throughout (one run, single noisy Windows dev host — consistent with the
3-run medians already in `crates/region/README.md:122-132`):

```
st/insert    246.63 ns/op      sync/insert          282.59 ns/op    raw/insert  286.40 ns/op
st/get_hit     4.39 ns/op      sync/get_cloned_hit   35.46 ns/op    raw/get_hit   5.16 ns/op
st/get_stale   4.22 ns/op      sync/remove          124.91 ns/op    raw/remove  105.40 ns/op
st/remove     97.86 ns/op
st/iterate  1369.34 ns/op (1,000 live values → 1.37 ns/value)
```

---

## Verdict

**No change to `src/` is supported by measurement.** The crate's own code — four files, every
method a single-line delegation — has no findable hot-path defect: `Handle<T>` is layout-optimal
(§4), the capacity growth policy is standard Vec doubling with a measured ~2.3% overshoot (§6),
and the coarse `RwLock` in `SyncRegion` is the right tradeoff for the crate's stated design
goals (§3). That is the honest core conclusion, and it matches the 2026-08-06
publish-readiness review's §7 ("no performance work owed").

**One real finding does exist, and it is in the bench harness / README table, not in `src/`:**
the four `bench_batched` workloads time the fixture's **teardown and cold-map first allocation
inside the measured window**, so the README's `insert`/`remove` rows report a cold-lifecycle
number roughly **10× higher than steady-state** — without saying so (§1). Since the README
performance table is new this session and would ship to crates.io with the next publish, this
is exactly the mislabeled-metric class this repository's own conventions treat as
top-severity (the R14-3 "sub-window figure presented as the headline" precedent in CLAUDE.md).
It does **not** invalidate the wrapper-overhead A/B conclusion (both arms carry the identical
extra cost), but the absolute numbers need either a steady-state companion workload or an
explicit cold-path label before they are published as "`Region::insert` costs 290 ns".

Findings ranked by expected real-world impact:

| # | Finding | Kind | Impact |
|---|---|---|---|
| 1 | Batched bench rows time teardown + cold first-alloc; README labels them as plain op cost | harness/doc fidelity | **High** (publishes ~10×-inflated numbers) |
| 2 | Iteration is O(high-water slot count), capacity never shrinks — cost unmeasured, permanence undocumented | missing measurement + doc | Medium |
| 3 | `SyncRegion` coarse `RwLock`: keep; contended behavior unmeasured | verdict + missing measurement | Medium-low |
| 4 | `Handle<T>` layout: optimal, 8 bytes with niche; optionally pin with a static assert; optional `Ord` | no-change confirmation | Low |
| 5 | `iter()`/`iter_mut()` erase `ExactSizeIterator`/`FusedIterator` slotmap provides | API capability, ~zero runtime cost | Low |
| 6 | Capacity growth, `#[inline]`, compile-time/binary-size: all verified fine | no-change confirmations | — |

---

## 1. HIGH — the `bench_batched` rows time fixture teardown and cold first-allocation; the README presents them as plain per-op cost

### Mechanism (verified in `bench-scale-tool` source, not inferred)

`bench-scale-tool 0.1.0`'s `bench_batched` builds this call
(`bench-scale-tool-0.1.0/src/lib.rs:282-287` in the registry cache):

```text
let call = move || -> Duration {
    let state = setup();          // untimed — correct
    let start = Instant::now();
    routine(state);               // routine takes state BY VALUE
    start.elapsed()
};
```

`routine` receives `state` by value, so `state` is **dropped at the end of the routine body —
inside the timed window**, before `start.elapsed()` is read. Every `bench_batched` workload in
`crates/region/benches/region_bench.rs` therefore times, in addition to the op under test:

- **`st/insert`** (`region_bench.rs:29-31`): setup is `Region::<u64>::new` — which allocates a
  1-slot Vec holding slotmap's sentinel (`slotmap-1.1.1/src/basic.rs:200-206`: sentinel push at
  construction). The timed closure then pays (a) the Vec **realloc 1 → 4 slots** for the first
  real insert (RawVec's 4-element minimum growth for 16-byte elements — this is exactly why the
  captrack probe saw `capacity() == 3` after one insert), (b) the insert itself, and (c) the
  **drop of the entire `Region`** (freeing the slots Vec). Reported: 246.63 ns/op this run.
- **`st/remove`** (`region_bench.rs:52-62`): setup builds the map + inserts untimed (correct),
  but the timed closure pays remove **plus the drop/free of the whole `Region`**. Reported:
  97.86 ns/op.
- **`sync/insert`**, **`sync/remove`** (`region_bench.rs:77-100`) and **`raw/insert`**,
  **`raw/remove`** (`region_bench.rs:113-136`): identical shape, identical contamination.
- On top of that, every batched iteration pays one `Instant::now()` pair — a cost the harness's
  own module doc explicitly warns is "a real, visible fraction of the reported ns/op" for
  nanosecond-scale routines (`bench-scale-tool-0.1.0/src/lib.rs:94-99`). On Windows that is a
  QPC read at each edge, order 10-40 ns per window.

### Why this is a ~10× distortion, with repository-internal evidence

This workspace already measured the same operations steady-state:
`docs/BENCHMARKS.md:29` — churn ("one steady-state `remove` + `insert`" on a warm 10,000-entry
map, 32-byte payload, criterion) = **10.7 ns/op for the whole remove+insert cycle** through the
`Region` wrapper, **8.9 ns** raw (`docs/BENCHMARKS.md:31-33`). Against that, the README table
(`crates/region/README.md:122-132`) publishes `Region::insert` = 290 ns and `Region::remove` =
97 ns with no cold-path qualifier. The gap is not wrapper overhead and not noise — it is
malloc/realloc + free + timer-window cost of a from-scratch map lifecycle, an order of
magnitude above the steady-state op the row name implies.

### What is NOT wrong

- The **A/B wrapper-overhead conclusion stands untouched**: `st/*` and `raw/*` batched arms
  carry byte-identical extra cost (same setup shape, same in-window drop), so their
  interleaving-within-noise comparison remains a valid negative result.
- The cold numbers are not meaningless — "create region, insert, tear down" is a real workload
  shape (many short-lived regions). The defect is the **label**, not the measurement.
- The `bench`-registered (plan-1, bulk-timed) workloads — `st/get_hit`, `st/get_stale`,
  `st/iterate`, `sync/get_cloned_hit`, `raw/get_hit` — are clean: fixture built once at
  registration, no drop or timer pair in the per-op cost.

### Recommended fix (concrete, small, and implementable in the existing harness)

1. **Add steady-state churn workloads using plan 1 (`Harness::bench`), which is possible
   because churn is self-neutralizing:** hold a warm pre-populated `Region` + one live handle
   in the closure's captured state; each iteration does `let v = r.remove(h).unwrap();
   h = r.insert(v);`. Map size stays constant, so a fixed 5M-iteration run neither grows the
   map nor re-allocates, and bulk timing applies (no in-window drop, no per-iteration timer
   pair). Three arms: `st/churn`, `sync/churn`, `raw/churn` — directly comparable to
   `docs/BENCHMARKS.md`'s churn axis, and expected to land near 10-30 ns/cycle. (Workload ids
   chosen to satisfy `tests/bench_ids_isolatable.rs`'s no-substring rule.)
2. **Relabel the existing batched rows** in `crates/region/README.md:122-132` as what they
   are — e.g. `Region::insert (cold map: first alloc + teardown incl.)` — or add one sentence
   under the table stating that the insert/remove rows measure a from-scratch map lifecycle
   (allocation and drop inside the timed window, per `bench_batched`'s semantics) and pointing
   at the churn rows for steady-state cost. Keeping both framings is more informative than
   deleting either.
3. Commit prefix under the R30-12 taxonomy: `bench` (harness/report change only, no shipping
   code touched).

Priority: do this **before** the README performance table first ships to crates.io — after
that, the inflated absolute numbers become the crate's public face.

---

## 2. MEDIUM — iteration cost is O(high-water slot count), the backing never shrinks, and neither fact is measured or fully documented in this crate

### Mechanism

`Region::iter`/`iter_mut` (`crates/region/src/region.rs:127-135`) walk slotmap's slot array
skipping tombstones, so per-sweep cost is proportional to the **slot-array length (the
high-water mark of live entries), not the current live count**. Two verified aggravators:

- **slotmap 1.1.1 has no `shrink_to_fit` and no capacity-reducing operation of any kind**
  (grep over `slotmap-1.1.1/src/basic.rs`: the only "shrink" hits are two safety comments,
  lines 580 and 1115, both *relying* on slots never shrinking). `clear()` keeps capacity;
  freed slots only ever return via the free list. Consequently a `Region` that once held
  100,000 values and now holds 1,000 pays a ~100,000-slot walk on every `iter()` **forever** —
  the only reclaim path is building a fresh `Region` and re-inserting, which invalidates every
  outstanding handle.
- The one number this workspace has on the iteration penalty — `docs/BENCHMARKS.md:27`'s ~30%
  SlotMap-vs-DenseSlotMap gap — was measured at **equal live counts with zero holes** (dense
  layout comparison), which is a *different* axis from hole-skipping cost. The hole-skipping
  cost of this crate's own store has never been measured at all. Current `st/iterate`
  (1,000 live, 0 holes → 1.37 ns/value this run) is the best case.

### Recommended actions

1. **Missing measurement, named precisely:** add two plan-1 workloads —
   - `st/iterate_holey`: fixture inserts 2,000, removes every other handle (1,000 live in a
     ≥2,000-slot array), then iterate-sum. Report ns **per live value** against `st/iterate`'s
     1.37; expectation is ~1.5-2.5× (double the slots walked per live value, plus branchier
     skipping).
   - `st/iterate_sparse`: fixture inserts 10,000, removes 9,000 (1,000 live in a ~10,000-slot
     array — a 90%-holes post-churn steady state). Expectation is ~5-10× ns/live-value. This
     arm is the one that turns the README's qualitative "NOT always-compact" caveat
     (`README.md:11-13`) into a number a consumer can budget against.
2. **Doc addition (one sentence each)** on `Region::iter` (`region.rs:123-126`) and
   `Region::capacity`/`clear` (`region.rs:75-79`, `region.rs:137-141`): capacity — and
   therefore per-sweep iteration cost — is permanently bounded below by the high-water mark;
   `slotmap` provides no shrink operation, so the only reclaim is rebuilding into a fresh
   `Region` (invalidating all handles). The existing `reserve` doc (`region.rs:83-88`) states
   the free-list-bounded *growth* half of this truth; the never-*shrinks* half is currently
   nowhere in the crate.
3. On the standing "expose `DenseSlotMap` behind a type alias for iteration-bound consumers"
   idea (`docs/BENCHMARKS.md:52-55`): still correctly trigger-gated. No iteration-bound
   consumer exists; adding a second backing type today would double the API and test surface
   for nobody. The two workloads above are the cheap prerequisite that would let a future
   consumer decide with numbers instead of the current single 0-hole data point. No change
   recommended now.

---

## 3. MEDIUM-LOW — `SyncRegion`'s coarse `RwLock`: genuinely the right tradeoff; keep it; the missing piece is a contended measurement, not a different lock

### Verdict: keep, with reasons

- **It matches the crate's stated design contract.** `sync_region.rs:9-14` positions
  `SyncRegion` as "the *always-shippable* concurrent answer" with lock-free tiers explicitly
  deferred to a later opt-in "until those land and clear loom/TSan". Replacing the coarse lock
  would be delivering Phase 3b out of order, without the verification budget the crate's own
  docs say that step requires.
- **The uncontended premium is small and now measured:** `sync/get_cloned_hit` 35.46 ns vs
  `st/get_hit` 4.39 ns → **~31 ns per read-locked op** (Windows SRWLock acquire/release +
  `Option<u64>` clone). For the target "shared handle table" use, that is not a bottleneck.
- **`parking_lot` — not recommended.** It would add a second dependency (with its own
  `unsafe`) to a crate whose crates.io pitch is "the only dependency is slotmap, zero own
  unsafe" (`Cargo.toml:7`, `README.md:28-31`), to chase single-digit-ns uncontended deltas
  against a modern std `RwLock` (futex-based on Linux since 1.62, SRWLock on Windows) — with
  zero contended-workload evidence that this crate's consumers would ever see the difference.
- **Sharding (N inner `RwLock<Region<T>>` shards) — not recommended, honestly costed.** It
  stays `#![forbid(unsafe_code)]`-compatible, but it is not cheap: a handle's shard must be
  recoverable from the handle alone, and slot indices are per-shard, so `Handle<T>` would need
  shard bits embedded — a breaking representation change; `len`/`is_empty`/`clear` lose
  single-lock atomicity; I1-I5 need re-proving per shard plus cross-shard; and a loom/TSan
  suite becomes mandatory. That is a real engineering project justified only by a measured
  contended consumer, which does not exist. The same reasoning already rejected finer-grained
  designs at the parent-crate level.
- One micro-observation, checked and dismissed: the poison-recovery path
  (`sync_region.rs:57,65` — `unwrap_or_else(PoisonError::into_inner)`) costs nothing on the
  happy path (a branch on the already-loaded `Result` discriminant).

### The missing measurement, named precisely

All three `sync/*` workloads are **uncontended single-thread** — they measure lock overhead,
not lock behavior. Missing: one workload with a warm `SyncRegion` where **4 reader threads
each perform a fixed count of `get_cloned` on distinct pre-inserted handles while 1 writer
thread runs steady insert/remove churn**, reporting aggregate reader ns/op at 1 vs 4 readers.
Why it matters: it is the only number that can ever justify (or permanently bury) the
`parking_lot`/sharding conversation, and it exercises the writer-starvation/reader-scaling
behavior of the platform `RwLock` that the crate currently ships blind. Implementable in
`bench-scale-tool` as a plan-1 closure that scoped-spawns the threads and amortizes spawn cost
by giving each thread a large fixed op count per iteration (report ns per *op*, not per
iteration). Until a concurrent consumer appears this is nice-to-have, which is why it ranks
below the two findings above.

---

## 4. LOW — `Handle<T>` layout: optimal; nothing to change; two optional cheap hardenings

Verified against `slotmap-1.1.1/src/lib.rs:245-248`: `KeyData { idx: u32, version: NonZeroU32 }`.

- **Size:** `Handle<T>` = `DefaultKey` (8 bytes) + `PhantomData<fn() -> T>` (ZST, align 1) =
  **8 bytes, no padding, no wasted bytes**. The `NonZeroU32` version field provides a niche, so
  `Option<Handle<T>>` is also 8 bytes. `fn() -> T` branding is the correct variance/auto-trait
  choice (covariant, unconditionally `Send + Sync + Copy`), exactly as `handle.rs:9-14`
  documents. There is no smaller or cheaper representation available without abandoning
  slotmap's key type.
- **Optional hardening (test-only, ~6 lines in `tests/`):** a static assertion pinning
  `size_of::<Handle<u8>>() == 8` and `size_of::<Option<Handle<u8>>>() == 8`, so the niche
  guarantee this review verified by construction becomes a checked fact that survives a future
  slotmap major bump. Matches this repo's "checked fact rather than assumption" register.
- **Optional API addition with a mild perf angle:** `KeyData` derives `Ord`/`PartialOrd`
  (`slotmap-1.1.1/src/lib.rs:245`, with a doc note that this exists precisely for `BTreeMap`
  keys), but `Handle<T>`'s hand-written unconditional impls (`handle.rs:36-57`) stop at
  `Eq`/`Hash`. Adding unconditional `PartialOrd`/`Ord` (delegating to `key`, same pattern)
  would let consumers sort a batch of handles before resolving them — sorted-by-key resolution
  approximates slot order, which is the cache-friendly access pattern for bulk lookups — and
  use handles as `BTreeMap` keys. Two ~5-line impls, semver-additive. Not a measured need;
  listed because it is the only capability gap between `Handle<T>` and the key it wraps.

## 5. LOW — `iter()`/`iter_mut()` erase iterator capabilities slotmap provides; runtime cost ≈ zero, so this is API polish, not perf

`Region::iter` returns `impl Iterator<Item = &T>` (`region.rs:127-129`), but the concrete
`slotmap::basic::Values` implements `ExactSizeIterator` **and** `FusedIterator`
(`slotmap-1.1.1/src/basic.rs:1280,1288-1289`), with an exact `size_hint`
(`basic.rs:1162`: `(num_left, Some(num_left))`). Because `size_hint` flows through the opaque
type at runtime, `collect::<Vec<_>>()` already pre-sizes correctly — I verified the exact-hint
implementation specifically to check this — so there is **no measurable allocation penalty
today**. What is lost is capability only: `.len()` on the iterator and `ExactSizeIterator`/
`FusedIterator`-bounded generic code. Fix, if wanted: widen the return types to
`impl ExactSizeIterator<Item = &T> + FusedIterator` (one line each; strictly additive for
callers, at the cost of committing to those bounds across future backing changes — note a
`DenseSlotMap` alias per §2.3 would satisfy them too). Ranked low because no consumer is
blocked and no cycle is saved.

## 6. No-action confirmations (checked, nothing owed)

- **Capacity growth policy:** slotmap grows its slot Vec by standard RawVec doubling; the
  captrack-observed 3 → 127 → 255 → 511 → 1023 sequence is exactly `Vec` capacity 4 → 128 →
  256 → 512 → 1024 minus the reserved sentinel slot (`basic.rs:260-262`: `capacity()` =
  `slots.capacity() - 1`). Measured overshoot at n=1000 is ~2.3%; `with_capacity` is exact;
  churn is free-list-bounded (all three already measured and published in README
  §"Capacity growth"). A custom growth factor is neither accessible through slotmap's API nor
  worth wanting at 2.3% overshoot. **Nothing to do.**
- **`#[inline]`:** already investigated this session with an A/B measurement; negative result;
  correctly left alone. Re-confirmed by this review's fresh run (st/raw interleave again:
  246.63 vs 286.40 insert, 97.86 vs 105.40 remove — no consistent direction). **Nothing to do.**
- **Compile-time / binary-size:** one runtime dependency (`slotmap`, `default-features =
  false`), no build script, no macros, no trait-object machinery, four small generic types
  whose monomorphization cost is proportional to actual use; the `std` feature gates only
  `SyncRegion` + `slotmap/std` (`Cargo.toml:15-17`, `lib.rs:59-66`). The heavyweight
  dev-dependency tree (captrack → serde_derive/ctor) never reaches consumers. `get_cloned`'s
  `T: Clone` bound is method-level, not type-level — correct. **Nothing to do.**
- **`st/get_hit`'s single-hot-handle shape:** best-case by construction (L1-resident,
  branch-predicted), but adequate: `docs/BENCHMARKS.md:26`'s 10,000-random-handle lookup axis
  measured 46.8 µs / 10k = **4.68 ns/op** — within noise of `st/get_hit`'s 4.39 ns, because a
  10k×16 B slot array is L2-resident anyway. A random-order variant would add little until
  someone cares about >100k-entry regions. Recorded here so the omission is a decision, not an
  oversight.
- **Missing large-`T` workload:** all benches use `u64`. A 256-byte payload arm would shift
  `insert` (memcpy) and `get_cloned` (clone dominates the lock) but tests slotmap's memcpy,
  not this crate's code; worth adding only alongside §1's churn workloads if the README table
  is being reworked anyway. Low value on its own.

---

## Summary

One genuine, publish-relevant finding: **the README performance table's insert/remove rows are
cold-lifecycle numbers (teardown + first-alloc + timer window inside the measurement) presented
as plain per-op costs, ~10× above the steady-state figures this workspace has already measured
elsewhere** — fix by adding self-neutralizing plan-1 churn workloads and relabeling the batched
rows before the table ships to crates.io. One worthwhile missing measurement with a doc
consequence (iteration over holes + the capacity-never-shrinks permanence). One keep-as-is
verdict with its future decision-gate named (`SyncRegion` contention). Everything else in the
crate's own code checked out as already optimal or already correctly decided — consistent with
this crate being 200 lines of single-line delegations over a well-chosen backing store.

## Open questions for the maintainer

- **Q1** — Should the §1 churn workloads + README relabel land *before* the 0.1.1 republish
  (my recommendation: yes, since the table becomes the crate's public performance claim the
  moment it ships), or is annotating the existing rows without new workloads enough for 0.1.1?
- **Q2** — §2's never-shrinks doc note touches `src/region.rs` doc comments, which are part of
  the docs.rs surface being corrected in 0.1.1 anyway — fold it into the same docs pass?
- **Q3** — Any appetite for the two optional `Handle` items (§4: size static-assert test,
  `Ord` impls)? Both are additive and tiny; neither is measurement-driven.
