# sefer-alloc-core

`sefer-alloc-core` owns the sefer allocator's core allocation paths: the
small/medium cluster (alloc/dealloc/carve, magazine batch ops, cross-thread
reclaim, the decommit/empty-segment pool), the large/huge path (alloc_large +
slow path + reclaim, the per-shard large-cache decay/eviction cluster, the
Large phase-transition state), the `AllocCore` hub itself, and its
diagnostics (`dbg_*` seams, counters).

Plan step P2.2 moved `src/alloc_core` here wholesale: module tree, file names
and internal `crate::alloc_core::…` paths are unchanged, so Miri and evidence
pins on exact relative paths (`platform/node.rs`,
`mem/advise_fresh_large.rs`) keep resolving. The root `sefer-alloc` crate re-exports this module tree at
`sefer_alloc::alloc_core` under the `alloc-core` feature; the registry and
global-allocator consumers normalize their cross-crate import paths as part
of P2.2.

The crate is independently publishable (`0.1.0`) and carries the root
allocator feature names as default-off toggles; the root manifest forwards
each feature weakly (`sefer-alloc-core?/<feature>`) without altering its
production bundle. Segment metadata/routing comes from `sefer-segment`,
policy/config types from `sefer-policy`, VM/platform seams from
`sefer-platform` and `aligned-vmem`, NUMA binding from `numa-shim`.

## Safety

`#![deny(unsafe_code)]` applies crate-wide. The moved pointer-manipulation
seams (`platform::{os,node,sidecar}`, `large::large_cache_extended`) retain
their narrowly scoped `#![allow(unsafe_code)]` attributes and existing
documented invariants; the audited tier-1 seam inventory is unchanged (see
the module doc of `alloc_core`). No crate depends back on `sefer-alloc`.

## Target support

Allocator features require a 64-bit target (R2-17); the crate root carries
the same `compile_error!` gate (with the identical message) as the root
crate, so a 32-bit build of this crate alone fails with the same diagnostic.

## License

Licensed under either MIT or Apache-2.0, at your option. See `LICENSE-MIT`
and `LICENSE-APACHE`.
