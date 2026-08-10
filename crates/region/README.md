# sefer-region

[![Crates.io](https://img.shields.io/crates/v/sefer-region.svg)](https://crates.io/crates/sefer-region)
[![Documentation](https://docs.rs/sefer-region/badge.svg)](https://docs.rs/sefer-region)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**100 % Rust typed handle-addressed store — no C / C++ libraries.**

A thin typed membrane over [`slotmap`](https://crates.io/crates/slotmap):
values live in slotmap's slot array (resolved by a single indirection),
but removed entries leave tombstone holes — iteration skips them, so the
backing store is NOT always-compact. Every operation exposes only typed
`Handle<T>` values — raw `DefaultKey`s never escape as usable values through the API (Debug output renders the underlying key for diagnostics only — it cannot be turned back into a functioning handle through this crate's public surface). The original single-threaded face of
[`sefer-alloc`](https://crates.io/crates/sefer-alloc), extracted as a
standalone crate.

## Why?

`slotmap`'s `DefaultKey` is untyped: a key from one map compiles against another
map of a different value type without error. `sefer-region` wraps it in
`Handle<T>` — a `PhantomData<fn() -> T>`-branded key plus a `region_id` — so the
compiler rejects cross-**type** handle confusion at the type level (a
`Handle<Foo>` cannot be used where a `Handle<Bar>` is expected), and the
runtime `region_id` check rejects cross-**instance** handle confusion at the
value level: a `Handle<T>` minted by one `Region<T>` is rejected — treated
exactly like a stale handle (`None`/`false`, never a panic) — by every other
`Region<T>` of the same type, even when its raw `DefaultKey` collides with a
live key in that other instance (as it commonly does — the first insert into
any fresh `Region` tends to produce the same key). This doubles `Handle<T>`'s
size from 8 to 16 bytes versus the pre-0.2.0 layout.

The differentiator for the pure-Rust audience: **zero own unsafe** —
`#![forbid(unsafe_code)]` at the top of this crate. The internal `unsafe` lives
upstream, in the mature, widely-used `slotmap` crate, not in this one. No C / C++
libraries are pulled in. With `default-features = false` the crate builds under
`no_std + alloc`.

For users who want a typed slotmap-like handle store **without** pulling a full
allocator stack.

## Quick start

```toml
[dependencies]
sefer-region = "0.1"
```

```rust
use sefer_region::{Region, Handle};

let mut region = Region::new();
let h: Handle<String> = region.insert("hello".to_string());

// I1: fresh handle resolves to the inserted value.
assert_eq!(region.get(h).map(String::as_str), Some("hello"));

let v = region.remove(h).unwrap();
assert_eq!(v, "hello");

// I2 + I3: stale handle resolves to None.
assert!(region.get(h).is_none());
```

## Invariants

- **I1 — resolution:** a fresh handle resolves via `get` to the inserted value
  until it is `remove`d.
- **I2 — tombstone:** after `remove(h)`, `get(h)` returns `None` for
  roughly `2^31` reuse cycles of that slot (a stale handle that has
  survived that many insert/remove cycles may wrap and spuriously
  resolve to a later value). A second `remove(h)` is a no-op `None`.
- **I3 — no ABA:** a stale handle — one whose slot has since been reused for a
  new value — does not resolve to the new value for roughly `2^31` reuse cycles of
  that slot. `slotmap`'s `DefaultKey` carries a generation counter bumped on
  removal, so the old handle fails the version check and yields `None`. After
  ~2^31 cycles the generation wraps and a very old handle may alias a later value.
  Memory safety is never affected.
- **I4 — accounting:** `len()` equals the number of live entries; `is_empty()`
  agrees.
- **I5 — drop-once:** every live value is dropped exactly once — on `remove`
  (returned to the caller) or on `Region` drop — never twice, never leaked.
- **I6 — instance isolation:** a `Handle<T>` resolves only through the
  `Region<T>` instance that minted it. Every accessor (`get`, `get_mut`,
  `remove`, `contains`) stamps its `region_id` at construction and checks it
  before touching the backing slotmap; a handle from a *different* `Region<T>`
  is rejected exactly like a stale handle (`None`/`false`), even when its raw
  `DefaultKey` collides with a live key in this region.

## SyncRegion (std feature, default-on)

`SyncRegion<T>` wraps `Region<T>` in a `std::sync::RwLock` for safe concurrent
access. It recovers from lock poison rather than propagating it (a panicked op
leaves the slotmap structurally intact). Poison recovery guarantees container
integrity only — an interrupted operation may have left partial effects visible
(e.g., a panicking `T::Drop` during `clear()` leaves later values live).

```rust
use sefer_region::SyncRegion;
use std::sync::Arc;

let sr: Arc<SyncRegion<u32>> = Arc::new(SyncRegion::new());
let sr2 = Arc::clone(&sr);

// One-shot convenience: no guard needed for single operations.
let h = sr.insert(42u32);
assert_eq!(sr.get_cloned(h), Some(42u32));
assert_eq!(sr.len(), 1);

// Multi-op transaction: hold the write guard for interleaving isolation
// (serializes the critical section). Panics mid-transaction do NOT roll
// back already-applied changes — all-or-nothing rollback is not provided.
{
    let mut w = sr.write();
    w.insert(1u32);
    w.insert(2u32);
} // guard dropped, lock released

assert_eq!(sr.len(), 3);
```

## Feature flags

| Feature | Default | Effect |
|---------|---------|--------|
| `std` | yes | Enables `SyncRegion<T>` and `slotmap/std` |

Disable default features for `no_std + alloc` (`Region<T>` + `Handle<T>` only):

```toml
sefer-region = { version = "0.1", default-features = false }
```

## Performance

Measured with [`bench-scale-tool`](https://crates.io/crates/bench-scale-tool)
(a fixed-iteration harness — the iteration count is calibrated once and
pinned, so run time is a direct speed signal, not a statistical estimate).
Single noisy Windows dev host, 3 runs each; the table below shows the
median, with the observed min–max range in parentheses so the numbers
aren't read as more precise than they are:

| Workload | ns/op (median, range) |
|---|---|
| `Region::insert` (cold: fresh map, allocation + full teardown inside the timed window) | 290 (242–327) |
| `Region::get` (hit) | 5.0 (4.3–6.5) |
| `Region::get` (stale handle) | 5.0 (4.7–5.1) |
| `Region::remove` (cold: fresh map with one entry, teardown included) | 97 (96–111) |
| `Region::iter` (1,000 live values, sum, zero holes — best case) | 1,319 (1,292–1,546) |
| `Region::iter` (1,000 live values, sum, 50% holes — post-churn cost) | 2,476 (2,228–2,510) |
| `Region::iter` (1,000 live values, sum, 90% holes — post-churn cost) | 11,482 (10,955–11,845) |
| `Region` steady-state churn (remove + reinsert, map size constant) | 3.6 (3.3–4.2) |
| `SyncRegion::insert` (uncontended, cold: fresh map + teardown) | 281 (269–324) |
| `SyncRegion::get_cloned` (hit) | 34.5 (34.2–36.0) |
| `SyncRegion::remove` (uncontended, cold: fresh map with one entry) | 124 (123–130) |
| `SyncRegion` steady-state churn (remove + reinsert, map size constant) | 76.0 (72.1–84.2) |

**Note:** The `insert` and `remove` rows above are measured with `bench_batched`, which
means the fixture (a fresh `Region`/`SyncRegion`) is dropped inside the timed window —
these numbers include allocation, teardown, and cold-path overhead, not just the
steady-state operation cost. See the `steady-state churn` rows for the warm-path
performance.

### Contended reads

Under multi-threaded read contention, `SyncRegion`'s one-shot convenience methods
(`get_cloned`, `contains`, `len`, `is_empty`) anti-scale: each call pays a
shared-cache-line lock acquisition that dominates the nanosecond-scale lookup.
Batching reads under one `read()` guard restores flat scaling. Single noisy
Windows dev host, 3 runs each (median reported):

| Workload | ns/op (median, range) |
|---|---|
| 8 readers, one-shot `get_cloned` | 1,221 (1,187–1,227) |
| 8 readers, 64 gets per `read()` guard | 38.7 (37.6–40.2) |

Reproduce: `cargo run --release --example contended_reads -p sefer-region`.

**Note on `SyncRegion` steady-state churn's range:** the landing commit's own message cites
a wider spread (~69.6-84.2 ns/op) than the table's published 3-run median-of-3 (72.1-84.2)
for the same workload — a broader across-multiple-runs sample was taken while iterating on
the fix, of which the table publishes only the specific 3 runs the median is drawn from.
Re-measured independently while auditing this note: three fresh runs came back 83.60-86.17
ns/op, a third distinct range again — this workload's absolute number visibly drifts on this
single noisy dev host from one measurement session to the next, more than most other rows in
this table. Treat the published range as an order-of-magnitude anchor for this specific
workload, not a tight bound.

**Iteration cost scales with high-water mark, not live count:** the three `Region::iter`
rows measure the same 1,000 live values, but with different hole percentages (0%, 50%, 90%).
Since the underlying `slotmap` provides no shrink operation, iteration cost is proportional
to the slot-array length, which stays at the historical high-water mark of live entries.
The 50%-holes case (2,000 high-water mark) is ~1.9× the zero-holes baseline; the 90%-holes
case (10,000 high-water mark) is ~8.7×. The only way to reclaim that cost is to build a
fresh `Region` and re-insert (which invalidates every outstanding handle from the old one).

`get`'s single-indirection lookup is roughly 30–60x cheaper than `insert`,
consistent with `slotmap::SlotMap`'s own documented lookup/churn tradeoff
(see [`docs/BENCHMARKS.md`](https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/BENCHMARKS.md)
in the parent workspace for the container-choice comparison this crate's
backing store was picked from). Reproduce: `cargo bench -p sefer-region
--bench region_bench` (add `-- --calibrate <secs>` to recalibrate the
pinned iteration counts in `bench-iters.txt` first, or `-- <substring>` to
run/time one workload — e.g. `-- st/insert` runs only `Region::insert`,
never `SyncRegion::insert`, since the two prefixes are chosen to never be
substrings of one another).

### Wrapper overhead: measured, not assumed

`Region<T>` is documented as "a thin typed membrane" that "delegates every
operation to slotmap" — but that was a design claim, never actually
measured against the type it wraps. `benches/region_bench.rs` also runs
`raw/insert`/`raw/get_hit`/`raw/remove` directly against a bare
`slotmap::SlotMap<DefaultKey, u64>` (`Region`'s own backing type, no
`Handle<T>` involved) as an A/B baseline. Median-of-3 result (measured in a
separate session from the table above; see that table's ranges):
`st/insert` 281 ns/op vs `raw/insert` 305 ns/op; `st/get_hit` 5.07 vs
`raw/get_hit` 4.76; `st/remove` 99.3 vs `raw/remove` 106.4 — the wrapped and
raw numbers interleave with no consistent direction, fully inside the
~15–25% run-to-run noise this dev host already shows elsewhere in this table.
**No measurable wrapper overhead was found.** None of `Region`'s methods
carry an explicit `#[inline]` hint; since every method is generic over `T`
(so its MIR is available for cross-crate monomorphization regardless) and
each is a single-line delegation, LLVM's own size-based inlining heuristic
already inlines them at the release optimization level this bench (and any
real consumer's release build) uses. Investigated so this stays a checked
fact rather than an assumption — no code change was made, because none was
supported by the measurement.

### Capacity growth (verified, not assumed)

Measured with [`captrack`](https://crates.io/crates/captrack)
(`tests/captrack_probe.rs`, run manually — `slotmap::SlotMap` isn't one of
captrack's natively-wrapped collection types, so this drives its public
registry API directly rather than the usual macros):

- **Organic growth** (`Region::new()`, 1,000 sequential inserts):
  capacity grows geometrically — 3 → 127 → 255 → 511 → 1023 — landing at
  **1,023 for 1,000 live values (~2.3% overhead)**, not the tight-fit
  guess a caller might otherwise assume.
- **Churn is genuinely free-list-bounded, not just documented as such:**
  inserting 1,000, removing every other one, then inserting 500 more
  measured capacity staying flat at 1,023 through the whole cycle — the
  refill reuses freed slots rather than growing past the prior high-water
  mark. (`Region::reserve`'s own doc comment already claimed this; this is
  the first time it was actually measured rather than trusted.)
- **`Region::with_capacity(n)` reserves exactly `n` up front for realistic `n`** — no
  intermediate reallocation for a workload that stays within it. If you
  know your workload's peak live count ahead of time, pre-sizing avoids
  the ~2.3% organic-growth overhead entirely.

Reproduce: `cargo test -p sefer-region --test captrack_probe -- --ignored
--nocapture`.

## Safety

`#![forbid(unsafe_code)]` at the top of this crate. The internal `unsafe` lives
upstream, in the `slotmap` dependency, not in this crate -- this crate
contributes zero `unsafe` blocks and pulls in no C / C++ libraries. No
version-scoped audit record for `slotmap` is tracked by this project; `slotmap`
is a mature, widely-used dependency, and this crate's own `cargo-deny` CI check
covers advisories/licenses/sources of the current lockfile (not a substitute
for a code audit).

## License

Licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  https://www.apache.org/licenses/LICENSE-2.0)

at your option.
