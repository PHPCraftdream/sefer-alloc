# sefer-alloc

> A safe, handle-addressed region store — generational handles instead of
> pointers, a dense cache-friendly core with **zero `unsafe` of our own**, and a
> verification-first build.

`sefer-alloc` hands out small, copyable [`Handle`] values instead of raw
pointers. The data lives in a dense backing store the region is free to move; a
stale handle never dereferences freed memory — it returns `None`, never
undefined behaviour.

```rust
use sefer_alloc::Region;

let mut region = Region::new();
let a = region.insert("alpha");
let b = region.insert("beta");

assert_eq!(region.get(a), Some(&"alpha"));

region.remove(a);
assert_eq!(region.get(a), None);        // stale handle → None, never UB
assert_eq!(region.get(b), Some(&"beta")); // others stay valid
```

## What's in the box

| Tier | Type | Feature | `unsafe` of our own | Status |
| --- | --- | --- | --- | --- |
| **Core** | `Region<T>` / `Handle<T>` | *default* (`std`) or `--no-default-features` (`no_std` + `alloc`) | **none** (`#![forbid(unsafe_code)]`) | shippable |
| **Sync** | `SyncRegion<T>` (an `RwLock<Region<T>>`) | `std` (default) | none | shippable |
| **Lock-free** | `LockFreeRegion` / `EpochRegion` (RCU + epoch reclamation) | `experimental` | one confined module (`concurrent::hand`) | experimental |
| **Byte / allocator** | `ByteRegion` + `ByteAllocator` (`unsafe impl GlobalAlloc`) | `byte` | one confined module (`byte::*`) | research |

The single-threaded core is `#![forbid(unsafe_code)]` with the default features
*off*: `slotmap`'s audited `unsafe` owns the dense generational layout (the free
list, the generation bump on remove, version-saturation retirement), and this
crate is a thin typed membrane over it. With the `experimental` and/or `byte`
features on, the crate is `#![deny(unsafe_code)]` (any `unsafe` outside an
allowed module is a hard compile error) and the confined `unsafe` lifts itself
with `#![allow(unsafe_code)]` **only** inside:

- `src/concurrent/hand.rs` (under `experimental`), and
- `src/byte/byte_region.rs` / `src/byte/byte_allocator.rs` (under `byte`).

So *"the `unsafe` lives in named modules"* is enforced by the compiler in
**every** configuration — it is structural, not asserted in prose.

## `no_std` support

The core (`Region<T>` / `Handle<T>`) needs only `alloc` and builds under
`no_std`. Disable default features to drop `SyncRegion` and the concurrent/byte
tiers:

```toml
[dependencies]
sefer-alloc = { version = "0.1", default-features = false }
```

`SyncRegion` (it wraps `std::sync::RwLock`) and the `experimental` / `byte`
tiers require `std`; they stay behind the default-on `std` feature.

## Why

Building self-referential structures in Rust — linked lists, graphs, trees,
slabs — fights the borrow checker because raw pointers dangle. The established
answer is to store references **as indices** into a backing array, and to make
those indices safe against reuse with a **generation** counter. That is what
this crate is: a clean, verified, well-documented vessel for that pattern.

Prior art that does the single-threaded job today: [`slotmap`],
[`thunderdome`], [`generational-arena`]. `sefer-alloc` builds its core on
`slotmap` and then aims past them at the parts that have **no safe, ready-made
answer**: a concurrent (epoch-reclamation) tier and an experimental byte /
global-allocator mode.

## Design in one breath

Three organs:

- **Cartographer** (safe) — all placement and free-list logic; pure integer
  arithmetic over indices, never touches memory. (In the single-threaded core
  this is `slotmap`'s job; in the byte tier it is ours.)
- **Membrane** (safe) — the typed `Handle` API and generation checks; *total*,
  cannot express UB.
- **Hand** (unsafe) — a single audited organ that touches raw memory, present
  **only** in the lower tiers (concurrent epoch reclamation; byte/allocator
  mode). The core you use today has no Hand.

See [`docs/DESIGN.md`](docs/DESIGN.md) and [`docs/INVARIANTS.md`](docs/INVARIANTS.md).

## Scope (honest)

This is an **application-level** store, not a drop-in global allocator.
`GlobalAlloc`'s contract is "give me a raw `*mut u8`", which is the one place a
handle cannot survive — so a process-wide allocator built on these principles
keeps a single, irreducible raw aperture at the very bottom. That descent is
the research-flagged `byte` tier (see [`docs/PLAN.md`](docs/PLAN.md)); it may
never beat `mimalloc`, and it exists to learn, not to replace it. For a
process-wide allocator, reach for `mimalloc`.

## Verification

The build is verification-first. Invariants I1–I6 ([`docs/INVARIANTS.md`](docs/INVARIANTS.md))
are covered by unit and a `proptest` differential-vs-reference-model harness;
the core is miri-clean; the concurrent publication protocol is loom-modelled;
Phase 5 adds a `cargo-fuzz` target and multi-arch CI. The everyday dev loop
keeps these *fast* (short proptest, tiny miri scope); the heavy CPU-hour
campaigns live in CI / nightly fuzz, not the tight cycle. See
[`docs/PLAN.md`](docs/PLAN.md) and the fuzz target at [`fuzz/`](fuzz/).

## MSRV

**1.88.** The core is plain safe Rust and will build on much older toolchains,
but we pin a known-good floor from day one (matches the resocks5 ecosystem).

## License

Dual-licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE),
at your option.

[`Handle`]: https://docs.rs/sefer-alloc
[`slotmap`]: https://crates.io/crates/slotmap
[`thunderdome`]: https://crates.io/crates/thunderdome
[`generational-arena`]: https://crates.io/crates/generational-arena
