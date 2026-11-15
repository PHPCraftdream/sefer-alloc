# sefer-concurrent

**The legacy/research concurrent region tiers of `sefer-alloc`, extracted into
a standalone crate: epoch-based reclamation, RCU page snapshots, sharded
regions, and a thread-per-core runner.**

## What this is

`sefer-concurrent` holds the concurrent region family that `sefer-alloc` grew
during its Phase 3b / 7a / 7b / 7c research phases:

- `EpochRegion<T>` / `EpochHandle<T>` — epoch-based reclamation over
  `crossbeam-epoch`, with the confined `unsafe` seam `src/epoch/hand.rs`
  (the `AtomicSlot<T>` membrane).
- `LockFreeRegion<T>` / `LockFreeHandle<T>` — RCU page-granularity
  copy-on-write snapshots over `arc-swap`.
- `ShardedRegion<T>` / `ShardedHandle<T>` — sharded regions routing handles
  to per-shard `EpochRegion`s.
- `PinnedRunner` (feature `pinning`) — a thin safe wrapper over
  `core_affinity` for thread-per-core dispatch.

The region family is a **legacy research-tier**: it is superseded by the
production `alloc-xthread` cross-thread free path in `sefer-alloc` and no new
development is planned on it. `PinnedRunner` is **not** deprecated and remains
useful on its own.

## Relationship to `sefer-alloc`

The `sefer-alloc` root crate enables this crate with its `experimental`
feature (`pinning` additionally forwards to `sefer-concurrent/pinning`) and
re-exports all seven public types unchanged through its `src/concurrent`
shim module, so `sefer_alloc::{EpochRegion, LockFreeRegion, ShardedRegion, ...}`
and `sefer_alloc::PinnedRunner` keep resolving. Using the types through
`sefer-alloc` or depending on this crate directly is equivalent.

## Quick start

```toml
[dependencies]
sefer-concurrent = "0.1"
# or, for PinnedRunner:
sefer-concurrent = { version = "0.1", features = ["pinning"] }
```

```rust
use sefer_concurrent::{EpochHandle, EpochRegion};

let region = EpochRegion::new();
let handle = region.insert(42_u64);
assert_eq!(region.get(handle), Some(&42));
```

## Where unsafe lives

The crate is `#![deny(unsafe_code)]` at its root. The ONE confined seam that
lifts that deny is `src/epoch/hand.rs` (module-level `#![allow(unsafe_code)]`):
the `AtomicSlot<T>` membrane over `crossbeam-epoch`, where every `unsafe`
block carries a `// SAFETY:` comment. Everything else in the crate is
compiler-checked safe code.

The root crate `sefer-alloc` carries the full workspace unsafe inventory in
its README/lib.rs, verifiable with:

```sh
grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/
```

## License

MIT OR Apache-2.0.
