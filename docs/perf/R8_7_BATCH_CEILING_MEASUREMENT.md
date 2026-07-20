# R8-7 — public batch alloc/dealloc API: measured ceiling, GO/NO-GO verdict

**Task:** #220 (R8-7, P3-measure) — from the external perf review covering
commit range `ffd3215..f0dd9a9`.
**Measurement-only:** no `src/` changes. `git status --short` after this task's
work shows exactly one modified file, `benches/global_alloc.rs` — no new
public API, no new `src/` symbol, no new `#[doc(hidden)]` forwarder (the
primitives needed were already visible enough; see §2).
**Date:** 2026-07-20
**Base revision:** `main` @ `f0dd9a9`
**Platform:** Windows 10 Pro x86-64.
**Harness:** `benches/global_alloc.rs`, new `bench_batch_ceiling` /
`batch_ceiling` criterion group (added by this task).

---

## 1. Question this measures

An external perf review speculated that a public batch/scoped alloc API
(`alloc_batch`/`dealloc_batch`, or similar) could give **1.5–3× on bulk
small-object patterns (16–256 B)** by amortising per-call TLS lookup /
size-class classification / routing over many blocks per call.

Contemplative analysis of task #220's own description already flagged the
trap: **no consumer of such an API exists anywhere in this repo today**, so a
bench purpose-built around a not-yet-existing API signature would only prove
the batching *mechanism* works — it would not demonstrate that the API is
worth designing and shipping. That is circular justification (design the
bench to fit the API, then cite the bench as evidence for the API).

The correct move, and this task's sole deliverable, is to **measure the
ceiling** such an API could deliver by calling the **already-existing internal
batch primitives directly**, with zero new public surface. If the measured
ceiling is below the reviewer's own 1.5× floor, the API idea is dead on
arrival regardless of what signature it might have had. If it clears 1.5× at
even one size, the signature-design conversation becomes worth having (a
separate, NOT-yet-done task).

---

## 2. Method — existing primitives, existing bench precedent, zero new public API

### 2.1 The primitives measured

- **`AllocCore::refill_class_bump(class_idx, out: &mut [*mut u8]) -> usize`**
  (`src/alloc_core/alloc_core_small_magazine.rs:117`, `#[doc(hidden)] pub`,
  gated only on `alloc-core`) — bump-direct batched carve. Fills `out` with up
  to `out.len()` live, bitmap-allocated blocks of `class_idx` in ONE call:
  drains existing free blocks first, then bump-carves the remainder directly
  into `out`, skipping the per-block `alloc_small`/BinTable round-trip for
  freshly-carved blocks. End-state is byte-identical to `out.len()` separate
  `alloc_small` calls (see the function's own doc comment for the full D1/M2
  equivalence proof). Used in production today by
  `src/registry/heap_core_alloc.rs:437` (`refill_class_bump_checked`, the
  `alloc-xthread`+`fastbin` variant with a magazine-residency predicate) on
  every tcache magazine miss.
- **`unsafe AllocCore::flush_class(class_idx, blocks: &[*mut u8])`**
  (`src/alloc_core/alloc_core_small_magazine.rs:370`, `#[doc(hidden)] pub`, no
  extra feature gate beyond `alloc-core`) — pushes a batch of blocks back onto
  their owning segments' `BinTable`s in ONE call, grouping same-segment runs
  (Э8) so metadata (`bin_table`, `alloc_bitmap`, `bump`) is read/written once
  per run instead of once per block. End-state is byte-identical to
  `blocks.len()` separate `dealloc_small` calls. Used in production today by
  `src/registry/heap_core_free.rs:376` on tcache magazine overflow and
  `src/registry/heap_core_tcache.rs:101` on thread-heap teardown.
- **`AllocCore::dbg_layout_class_for(layout) -> Option<usize>`**
  (`src/alloc_core/alloc_core_core_diag.rs:391`, `#[doc(hidden)] pub`,
  test-only classification hook) — used only to resolve `class_idx` for the
  batch arm; not part of the timed region.

No new `pub` symbol was added to `src/`. All three items above already
existed with sufficient visibility for a `benches/` binary to call them
directly (the established "`#[doc(hidden)]` test-only forwarder" pattern —
`CLAUDE.md` §"File and module structure", category 1). The task's own
contingency plan ("add a minimal forwarder if visibility is insufficient") was
not needed.

### 2.2 Bench-construction precedent

`pool_cap_sweep_spread_and_drain` (`benches/global_alloc.rs:1065`, pre-existing,
task RAD-3) already constructs `AllocCore` directly via
`AllocCore::new_with_config(...)` / `AllocCore::new()` and calls its methods
(`alloc`, `dealloc`, `dbg_*`) without going through `SeferAlloc`/`GlobalAlloc`
at all — specifically to avoid TLS/registry plumbing and get a bare, isolated
`AllocCore` per bench iteration. `bench_batch_ceiling` (added by this task,
`benches/global_alloc.rs:1230`) follows the exact same construction pattern
(`AllocCore::new().expect("primordial reservation")`) for consistency with the
file's established style.

### 2.3 The two arms — same 1024-block cold-bulk workload

Both arms process **`BATCH_CEILING_OPS = OPS = 1024`** blocks per iteration
(same constant `bench_direct_alloc` — the cold bulk pattern the review
critiques, `global_alloc.rs:192` — already uses), against a **fresh
`AllocCore` per iteration** (`iter_batched`, untimed setup) so cross-iteration
state never leaks and neither arm benefits from a warm tcache/magazine the
other lacks:

- **(a) Scalar** — `AllocCore::alloc`/`AllocCore::dealloc` called once per
  block: 1024 separate `alloc` calls into a `Vec<*mut u8>`, then 1024 separate
  `dealloc` calls.
- **(b) Batch** — ONE `refill_class_bump` call fills a
  `[*mut u8; 1024]`-sized buffer, then ONE `unsafe flush_class` call frees the
  whole buffer.

Both arms allocate one `Vec<*mut u8>` of the same size inside the timed
closure (via the bench binary's own default/System allocator, since
`SeferAlloc` is not installed as `#[global_allocator]` in this bench binary —
see the file's module doc, confound 1/2 discussion). This cost is identical
and additive in both arms, so it does not bias the ratio between them.

### 2.4 Command run

```text
cargo bench --bench global_alloc --features production -- batch_ceiling
```

Criterion fast profile per `CLAUDE.md`: `sample_size(10)`,
`warm_up_time(150ms)`, `measurement_time(600ms)` — same profile
`bench_working_set_cycle` uses in this file.

Verified clean under the full feature matrix before measuring:
- `cargo fmt --check` — clean.
- `cargo clippy --release --features production --all-targets -- -D warnings` — clean.
- `cargo clippy --release --all-features --all-targets -- -D warnings` — clean.
- `cargo clippy --release --features experimental --all-targets -- -D warnings` — clean.
- `cargo clippy --release --all-targets -- -D warnings` (no features) — clean.
- `cargo bench --bench global_alloc --features production --no-run` — compiles.

---

## 3. Raw numbers — three independent runs, `--features production`

Criterion reports `[low high]` ns/µs bounds per sample; the middle figure
below is criterion's own point estimate. All times are µs per 1024-block
iteration (alloc-then-free, or one batch-fill-then-one-batch-flush).

| Size | Run | scalar (µs/1024) | batch (µs/1024) | ratio (scalar/batch) |
|---|---|---:|---:|---:|
| 16 B | 1 | 61.312 | 19.753 | 3.10× |
| 16 B | 2 | 57.348 | 24.426 | 2.35× |
| 16 B | 3 | 58.150 | 20.686 | 2.81× |
| 64 B | 1 | 86.149 | 52.235 | 1.65× |
| 64 B | 2 | 86.609 | 50.246 | 1.72× |
| 64 B | 3 | 82.973 | 47.397 | 1.75× |
| 256 B | 1 | 209.58 | 161.64 | 1.30× |
| 256 B | 2 | 208.00 | 180.40 | 1.15× |
| 256 B | 3 | 188.47 | 162.06 | 1.16× |

### 3-run average, and ns/op (per-block) figures

| Size | avg scalar (µs/1024) | avg batch (µs/1024) | **avg ratio** | scalar ns/op | batch ns/op |
|---|---:|---:|---:|---:|---:|
| **16 B** | 58.94 | 21.62 | **2.73×** | 57.6 | 21.1 |
| **64 B** | 85.24 | 49.96 | **1.71×** | 83.2 | 48.8 |
| **256 B** | 202.02 | 168.03 | **1.20×** | 197.3 | 164.1 |

Run-to-run spread is modest (16 B: 2.35–3.10×; 64 B: 1.65–1.75×; 256 B:
1.15–1.30×) — the ranking and the "crosses 1.5×" call is stable across all
three runs at 16 B and 64 B, and stable BELOW 1.5× across all three runs at
256 B. No run flips any size across the threshold.

---

## 4. Verdict

| Size | Measured ceiling (avg) | >= 1.5×? |
|---|---:|---|
| 16 B | 2.73× | **YES** |
| 64 B | 1.71× | **YES** |
| 256 B | 1.20× | no |

**GO** — per task #220's own decision rule, the batch ceiling is **>= 1.5× at
two of the three sizes** (16 B and 64 B), so this does NOT close the API idea.
The reviewer's speculated 1.5–3× range is empirically real at the small end
(16 B lands almost exactly at the top of that range; 64 B lands mid-range) and
tapers toward the top of the range's low end at 256 B (1.20×, below the 1.5×
floor but not negligible — a genuine ~17% steady-state win, just below the
review's threshold).

**Mechanism read:** the win shrinks monotonically as block size grows (2.73×
→ 1.71× → 1.20× at 16/64/256 B). This tracks the review's own amortisation
theory: per-call fixed overhead (classification, routing, the
free-drain-then-carve bookkeeping inside `refill_class_bump`, the per-run
metadata hoist inside `flush_class`) is a CONSTANT cost independent of block
size, so it dominates more heavily relative to the (also constant, since these
are all small-class carve/free operations) per-block work at smaller sizes.
At 256 B the fixed per-call savings are the same absolute magnitude but a
smaller fraction of a larger per-block cost, so the ratio compresses toward
1×.

**What this does NOT settle (explicitly out of scope for this task):**
- The exact public signature (`alloc_batch(layout, out: &mut [*mut u8]) ->
  usize` + `unsafe dealloc_batch`, per #220's own sketch) — a real public API
  adds argument-validation/layout-consistency overhead this measurement's raw
  internal-primitive call does NOT pay, so the measured ratio here is a strict
  ceiling, not what a shipped API would deliver. A shipped API's real number
  would be somewhat lower than the figures in §3.
- Whether any real call site in this repo (or a plausible downstream
  consumer) would actually batch 1024 same-size, same-class allocations at
  once — the original circularity concern this task exists to avoid. This
  measurement establishes the mechanism has headroom; it does not establish
  demand.
- Where to draw the size cutoff for a public API (e.g. "batch API applies only
  below N bytes") — that is a signature-design question for the next phase,
  informed by the monotonic-decay shape observed here.

**Recommendation:** since the ceiling clears 1.5× at 16 B and 64 B, task #220
graduates to the signature-design phase per its own description (the
candidate `alloc_batch`/`unsafe dealloc_batch` shape already sketched in the
task). That design work is explicitly NOT part of this task and was not
started here.

---

## 5. Caveats

- **Single host, three runs, no statistical treatment beyond a plain
  average.** The ranking (16 B > 64 B > 256 B, 256 B below 1.5×) is stable
  across all three runs; the exact ratio values carry normal criterion-sample
  noise (see the `[low high]` bounds criterion itself reports, roughly ±5–10%
  around the point estimate quoted above).
- **This is a same-thread, no-contention measurement.** `AllocCore` is used
  directly with no TLS/registry/cross-thread-free path involved — the
  `refill_class_bump`/`flush_class` calls measured are the exact primitives a
  real `alloc-xthread`+`fastbin` production build uses on the magazine-miss/
  overflow paths, but this bench does not exercise concurrent access or the
  ring-drain branches.
- **Ceiling, not a shippable-API forecast** (see §4) — a real public API
  pays extra argument validation the raw internal call does not.
- **No `src/` was modified.** `git status --short` after this task's work
  shows only `benches/global_alloc.rs` changed.
