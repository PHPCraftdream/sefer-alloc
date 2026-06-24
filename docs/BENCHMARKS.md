# Benchmarks — container choice (Phase 2)

`sefer-alloc`'s single-threaded `Region<T>` is a typed membrane over a backing
container. Phase 2 measures four candidates to **justify the default backing**
empirically rather than by assertion. The bench is `benches/locality.rs`
(criterion); reproduce with `cargo bench`.

## What was measured

Four containers holding a 32-byte `Copy` payload (`[u64; 4]`, wide enough that
cache locality matters) at **N = 10 000** live entries, across three axes:

- **iterate** — sum a field across all live entries (favours dense layouts).
- **lookup** — resolve 10 000 pre-saved handles/keys in a fixed random order
  (favours single-indirection access).
- **churn** — one steady-state `remove` + `insert` (insert/remove throughput).

## Results

Median of the criterion estimate; **lower is better**. Numbers come from the
**quick profile** (`sample_size(10)`, short warm-up/measurement — see
`benches/locality.rs::quick`), so they are rough (±a few %), single-machine,
and meant to show the **relative ordering**, which is stable and matches theory.

| Axis | `Region<T>` (SlotMap) | `DenseSlotMap` | `HashMap<u32,_>` | `Vec<Box<T>>` |
| --- | --- | --- | --- | --- |
| **iterate** (µs, all 10k) | 14.1 | **10.8** | 18.1 | 12.6 |
| **lookup** (µs, 10k random) | **46.8** | 54.4 | 244 | — |
| **churn** (ns / op) | 10.7 | 30.3 | 63.9 | 83.0 |

(Raw `slotmap::SlotMap` without our wrapper: iterate 14.4 µs, lookup 47.0 µs,
churn 8.9 ns — the typed-`Handle` membrane costs essentially nothing on lookup
and ~1.8 ns on churn.)

## Verdict

**Standard `slotmap::SlotMap` stays the default backing for `Region<T>`.**

- It **wins lookup** (46.8 µs vs DenseSlotMap's 54.4 µs and HashMap's 244 µs).
  Lookup is the **hotter path** for the target read-mostly workloads
  (connection tables, caches: per-entry resolution dominates), so this is the
  axis that matters most. `DenseSlotMap` pays a double indirection here
  (key → slot → dense index); `SlotMap` resolves in a single indirection.
- It **wins churn** decisively (10.7 ns vs 30.3 ns): `DenseSlotMap` must move
  the last live value into the hole on every remove to stay compact.
- It **loses iteration** to `DenseSlotMap` (~30 % slower), because `SlotMap`'s
  values live in a slot array with holes while `DenseSlotMap` packs them. This
  is the **colder path** for the target workloads, so the trade is worth it.
- The typed-`Handle` membrane adds **negligible** overhead — lookup is
  identical to raw `slotmap`, churn within ~2 ns.

`DenseSlotMap` remains a documented option for **iteration-bound** consumers
(frequent full sweeps, rare lookups); the wrapper could expose it behind a type
alias if such a consumer appears. Until then, the lookup-and-churn-optimal
`SlotMap` is the honest default.

`HashMap` and `Vec<Box<T>>` are the baselines: `HashMap` is ~5× slower on
lookup (hashing + probing) and `Vec<Box<T>>` is the cache-hostile pointer chase
(and offers no generational safety at all).
