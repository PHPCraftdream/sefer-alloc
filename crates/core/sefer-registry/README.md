# sefer-registry

P2.3 moves the sefer allocator's heap core, heap slots, registry bootstrap,
heap ownership, cross-thread routing, and fallback hook into this crate.
The registry keeps its `src/registry/` subtree; the process-global fallback
heap moves separately to `src/fallback.rs`. Internal `crate::alloc_core`
paths are retained through crate-local re-exports.

The crate has no default features. `alloc-global` enables the registry and
fallback and selects the required allocator substrate features for standalone
use. The root `sefer-alloc` crate enables this dependency only with
`alloc-global` and forwards registry features weakly. For a standalone
production-equivalent row, select `alloc-global`, `alloc-decommit`, `fastbin`,
`alloc-segment-directory`, and `primordial-lazy-commit` (`alloc-global`
already implies `alloc-xthread`).

The root crate keeps a registry re-export facade and owns TLS and
`GlobalAlloc` dispatch.

## License

Licensed under either MIT or Apache-2.0, at your option. See `LICENSE-MIT`
and `LICENSE-APACHE`.
