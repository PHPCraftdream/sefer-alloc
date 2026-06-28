# sefer-region

[![Crates.io](https://img.shields.io/crates/v/sefer-region.svg)](https://crates.io/crates/sefer-region)
[![Documentation](https://docs.rs/sefer-region/badge.svg)](https://docs.rs/sefer-region)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**100 % Rust typed handle-addressed store — no C / C++ libraries.**

A thin typed membrane over [`slotmap`](https://crates.io/crates/slotmap):
values live in slotmap's dense, cache-friendly, always-compact backing store,
and every operation exposes only typed `Handle<T>` values — raw `DefaultKey`s
never escape the crate boundary. The original single-threaded face of
[`sefer-alloc`](https://crates.io/crates/sefer-alloc), extracted as a
standalone crate.

## Why?

`slotmap`'s `DefaultKey` is untyped: a key from one map compiles against another
map of a different value type without error. `sefer-region` wraps it in
`Handle<T>` — a `PhantomData<fn() -> T>`-branded key — so the compiler rejects
cross-region handle confusion at the type level.

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
- **I2 — tombstone:** after `remove(h)`, `get(h)` is `None` forever; a second
  `remove(h)` is a no-op `None`.
- **I3 — no ABA:** a stale handle — one whose slot has since been reused for a
  new value — never resolves to the new value. `slotmap`'s `DefaultKey` carries
  a generation counter bumped on removal, so the old handle fails the version
  check and yields `None`.
- **I4 — accounting:** `len()` equals the number of live entries; `is_empty()`
  agrees.
- **I5 — drop-once:** every live value is dropped exactly once — on `remove`
  (returned to the caller) or on `Region` drop — never twice, never leaked.

## SyncRegion (std feature, default-on)

`SyncRegion<T>` wraps `Region<T>` in a `std::sync::RwLock` for safe concurrent
access. It recovers from lock poison rather than propagating it (a panicked op
leaves the slotmap structurally intact).

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
