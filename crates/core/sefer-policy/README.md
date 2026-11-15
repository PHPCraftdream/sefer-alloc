# sefer-policy

[![Crates.io](https://img.shields.io/crates/v/sefer-policy.svg)](https://crates.io/crates/sefer-policy)
[![Documentation](https://docs.rs/sefer-policy/badge.svg)](https://docs.rs/sefer-policy)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**The allocation-policy layer of the [`sefer-alloc`](https://crates.io/crates/sefer-alloc)
allocator, extracted as a zero-own-unsafe, `no_std` crate with a single dependency, `aligned-vmem`.**

sefer-policy holds everything in sefer-alloc that is *policy* rather than
*mechanism*: the const-buildable configuration types, the measured
configuration-policy builders, the medium→Large promotion-reachability
predicate, and the Large-THP (transparent huge page) policy runtime. It
contains no allocator, no OS seam and no raw pointers — pure `core`
arithmetic, constants and `core::sync::atomic` state — so the whole crate
is `#![forbid(unsafe_code)]` and builds under `no_std` with **one
dependency** (`aligned-vmem`, error type only).

## Contents

| Module | Contents | Gate |
|---|---|---|
| `large_cache_config` | `LargeCacheConfig` — the config builder every configured allocator entry point consumes | `alloc-decommit` |
| `large_cache_mode` | `LargeCacheMode` — the Large-cache decommit mode enum | `alloc-decommit` |
| `small_segment_pool_config` | `SmallSegmentPoolConfig` — the small-segment pool pair | `alloc-decommit` |
| `policy` | `Profile`, `SmallPoolPolicy`, `LargeCachePolicy` — measured builder axes over the config types | `alloc-decommit` |
| `medium_promotion` | the single-source-of-truth promotion-reachability `cfg!` const + `#[cfg]`-generator macros | none |
| `policy::large_thp` | `ThpTuple` codec, share/gradient/curve estimators, request groups | `alloc-decommit` |
| `policy::large_thp_observation` | bounded mix, control selector, residency planner, span geometry | `alloc-decommit` |
| `policy::large_thp_runtime` | packed tuple publication cell, cadence, budget ledger, `ThpRuntime` statics | `alloc-decommit` (parts also `!numa-aware`) |

## Feature mesh

The sources moved from `sefer-alloc`'s `src/alloc_core/config` gate on the
root crate's feature names. sefer-policy mirrors each name 1:1 (all
default-off, none implying another): `alloc-decommit`, `alloc-global`,
`bench-internals`, `internals`, `large-cache-extended`, `medium-classes`,
`exact-span-large`, `large-reserved-capacity`, `numa-aware`. The root
`sefer-alloc` crate forwards every one of them to this crate, so each
`#[cfg(feature = ...)]` inside the policy layer evaluates exactly as it did
when the files lived in the root crate. Standalone consumers enable the
same names directly on `sefer-policy`.

## Relationship to `sefer-alloc`

The root crate re-exports this crate's modules verbatim under its historic
paths (`sefer_alloc::alloc_core::config::…`), so all existing consumers of
`Profile` / `LargeCacheConfig` / `LargeCacheMode` / `SmallPoolConfig` /
`SmallPoolPolicy` / `LargeCachePolicy` / the THP types keep resolving
unchanged. Segment geometry, the OS aperture and the allocator runtime
itself stay in the root crate (segment geometry moves on to the segment
crate in a later restructuring step).

## Safety

`#![forbid(unsafe_code)]` at the top of this crate. The policy layer
contributes zero `unsafe` blocks, pulls in no C/C++ libraries, and its
only dependency is `aligned-vmem` (error-type plumbing, default features
off).

## License

Licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  https://www.apache.org/licenses/LICENSE-2.0)

at your option.
