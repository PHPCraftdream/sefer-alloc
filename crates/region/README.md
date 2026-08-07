# sefer-region

[![Crates.io](https://img.shields.io/crates/v/sefer-region.svg)](https://crates.io/crates/sefer-region)
[![Documentation](https://docs.rs/sefer-region/badge.svg)](https://docs.rs/sefer-region)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**100 % Rust typed handle-addressed store — no C / C++ libraries.**

A thin typed membrane over [`slotmap`](https://crates.io/crates/slotmap):
values live in slotmap's slot array (resolved by a single indirection),
but removed entries leave tombstone holes — iteration skips them, so the
backing store is NOT always-compact. Every operation exposes only typed
`Handle<T>` values — raw `DefaultKey`s never escape the crate boundary. The original single-threaded face of
[`sefer-alloc`](https://crates.io/crates/sefer-alloc), extracted as a
standalone crate.

## Why?

`slotmap`'s `DefaultKey` is untyped: a key from one map compiles against another
map of a different value type without error. `sefer-region` wraps it in
`Handle<T>` — a `PhantomData<fn() -> T>`-branded key — so the compiler rejects
cross-**type** handle confusion at the type level (a `Handle<Foo>` cannot be
used where a `Handle<Bar>` is expected). Note: branding is by value type `T`,
not by `Region` instance — a `Handle<T>` from one `Region<T>` is accepted by
a *different* `Region<T>` of the same type and could silently access or remove
a value keyed by the same `DefaultKey` in that other instance.

The differentiator for the pure-Rust audience: **zero own unsafe** —
`#![forbid(unsafe_code)]` at the top of this crate. The internal `unsafe` in
`slotmap` is its own, audited and battle-tested. No C / C++ libraries are pulled
in. With `default-features = false` the crate builds under `no_std + alloc`.

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

// Multi-op transaction: hold the write guard for atomicity.
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
| `Region::iter` (1,000 live values, sum) | 1,319 (1,292–1,546) |
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
`Handle<T>` involved) as an A/B baseline. Median-of-3 result: `st/insert`
281 ns/op vs `raw/insert` 305 ns/op; `st/get_hit` 5.07 vs `raw/get_hit`
4.76; `st/remove` 99.3 vs `raw/remove` 106.4 — the wrapped and raw numbers
interleave with no consistent direction, fully inside the ~15–25%
run-to-run noise this dev host already shows elsewhere in this table.
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

`#![forbid(unsafe_code)]` at the top of this crate. The internal `unsafe` in
the `slotmap` dependency is its own, audited and battle-tested. This crate
contributes zero `unsafe` blocks and pulls in no C / C++ libraries.

## License

Licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  https://www.apache.org/licenses/LICENSE-2.0)

at your option.
