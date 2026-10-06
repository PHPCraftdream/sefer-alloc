# R12-05 — bounded Large ingress on `alloc_batch` misses

**Class (R30-12):** `perf(opt-in)` — the `fastbin` + `batch-api` code of
`src/registry/heap_core/alloc/batch.rs` changed (`batch-api` is `experimental`, not part of
`production`); the only change visible to `production` is the visibility of one private helper
(`refill_with_large_rescue`: `fn` → `pub(super) fn`, no codegen change). Measured on a
deterministic scan-count axis only; **no wall-clock or latency claim**.

Source: `docs/reviews/2026-10-06-063308-src-review-fxx-round-12.md`, card fxx R12-05 and §3 O-2.

## 1. The change

`HeapCore::alloc_batch` ran the full `drain_large_sidecar_ingress()` — a walk over every
active Large route — on every magazine miss, and `alloc_batch_large` ran it once per call. The
scalar path already uses the cursor-preserving bounded probe (`refill_magazine_slow`,
`drain_large_sidecar_ingress_hot_bounded`, budget `LARGE_HOT_BUDGET` = 4 routes per miss) with
one full rescue sweep only when a refill comes back empty (`refill_with_large_rescue`). Both
batch paths now use the same primitives:

- Small batch miss: bounded probe, then the refill goes through `refill_with_large_rescue`.
- Large batch: bounded probe up front; the first block is allocated through
  `refill_with_large_rescue`, so a batch that cannot make progress gets the one full rescue
  sweep and one retry (identical shape to the scalar path).
- Non-`fastbin` builds keep their existing full drain (no cursor/bounded primitive exists
  there).
- `realloc` keeps its full drain: it is a cold path, no measured cost justifies a bounded
  probe there, and the in-code comment now says so. This report makes no realloc claim.

## 2. Entry point under test

`HeapCore::alloc_batch` (and its `alloc_batch_large` branch), driven through a real heap lease
(`HeapRegistry::dbg_claim_lease`). `alloc_batch` is the layer a `batch-api` consumer calls; the
`#[global_allocator]` path does not call it, so no `GlobalAlloc` chain is relevant here.

## 3. Identity (produced before measurement)

| field | value |
|---|---|
| base | `main` @ `d74fe5e5616057267c6448e78b0bbe4634fde9cd` |
| source delta | `sha256(git diff --ignore-cr-at-eol -- src)` = `b54ec9b52f13e5bc11fff34c4cc0bc5a31edb9b6186ecffbfbd7b525394c91b1` (3 files: `batch.rs`, `hot.rs`, `realloc.rs`) |
| features | `production,batch-api,internals,bench-internals` |
| command | `node scripts/r12_05_batch_gate.mjs` (runs the test, writes the raw log + CSV) |

## 4. Results (derived by `scripts/r12_05_batch_gate.mjs` from the raw logs)

<!-- r12_05:tables:begin -->
| scenario | active Large routes L | batch misses K | budget B | inspections (patched) | inspections (full-drain mutant) | mutant/budget (num mutant / den B) | rescues |
|---|---:|---:|---:|---:|---:|---|---:|
| small_batch_miss | 64 | 1 | 4 | 4 | 64 | 16.00 (64/4) | 0 |
| large_batch | 9 | 1 | 4 | 4 | 9 | 2.25 (9/4) | 0 |

| late-publication scenario | table slots | budget B | misses to retire | bound ⌈table/B⌉+1 | first-miss inspections (full-drain mutant) |
|---|---:|---:|---:|---:|---:|
| small batch misses, one late Large publication | 11 | 4 | 2 | 4 | 9 |
<!-- r12_05:tables:end -->

Reading:

- With L = 64 active Large routes, one Small batch miss inspects 4 routes (= B) instead of 64:
  the full-drain mutant inspects 64/4 = 16× the budget. With L = 9, one Large batch inspects 4
  instead of 9 (9/4 = 2.25×). "inspections" is the integer delta of
  `HeapCore::dbg_large_sidecar_slot_inspections` over the single batch call; no mean/median is
  computed.
- A Large route published to its terminal ingress after allocation (the late-publication row:
  one victim published through its `RouteDirectory` pin before any batch miss) is retired by the
  rotating cursor within 2 batch misses, inside the bound ⌈11/4⌉ + 1 = 4; every miss stays ≤ B inspections and no full rescue
  fires.

## 5. Path activation (R30-8)

Every test first asserts `dbg_active_kind_census() == (1, L, true)` (L active Large routes
registered), asserts the inspection delta is **> 0** (the oracle actually fired) and that
`dbg_large_sidecar_full_rescues` did not move (the budgeted probe, not the rescue sweep, did
the work).

## 6. Tests and counterfactual

`tests/r12_05_batch_large_bounded.rs` (3 tests, all green). Counterfactual (raw log
`_raw_r12_05_mutant_full_drain.log`): restoring the full `drain_large_sidecar_ingress()` at the
Small-miss site makes `small_batch_miss_probes_at_most_the_hot_budget` fail (`inspected 64;
budget=4`) and the late-publication test fail on the first miss (`miss 0: inspected 9`);
restoring it at the Large-batch site makes `large_batch_probes_at_most_the_hot_budget` fail
(`inspected 9; budget=4`). The patch hash above was re-computed after each restore and is
identical. Neighbouring suites `r11_p3_large_hot_refill`, `r11_p3_large_hot_late`,
`batch_large_deferred_reclaim` stay green.

## 7. Caveats

- Not exercised by execution: the rescue branch of the two batch paths. A real empty refill
  needs a logically full route table, which has no test hook; the branch is the shared
  `refill_with_large_rescue` helper, whose one-rescue-one-retry contract is pinned by
  `r11_p3_large_hot_refill::logical_capacity_failure_gets_one_full_rescue_and_one_retry`.
- Scan counts do not prove lower latency (the work saved is table/header reads, not OS calls).
- The late-publication row uses a same-thread publish through the real `RouteDirectory` pin;
  the cross-thread publication protocol itself is covered by `r11_p3_large_hot_late`.

## 8. Reproduce

```text
node scripts/r12_05_batch_gate.mjs          # runs the patched test, rewrites the raw log + CSV
node scripts/r12_05_batch_gate.mjs --check  # fails if the CSV or this report's tables drift from the raw logs
```

Raw logs: `_raw_r12_05_batch_counter.log` (patched), `_raw_r12_05_mutant_full_drain.log`
(counterfactual; build paths replaced by `<target>`). Summary: `R12_05_BATCH_LARGE_BOUNDED_GATE_summary.csv`.
