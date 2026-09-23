//! OS & platform shims (`os`, `numa`, `size_classes` — the three external-crate
//! adapters) plus the confined raw-memory `unsafe` seams (`node`, `sidecar`,
//! `dirty_by_class`).
//
// `numa` and `dirty_by_class` keep their feature gates HERE (not only on the
// root re-export in `alloc_core/mod.rs`) because the FILES themselves are not
// feature-independent: `numa.rs` unconditionally names `numa_shim` (an
// optional dependency pulled in only by `numa-aware`) and
// `dirty_by_class.rs` unconditionally imports `segment_directory`
// (itself gated on `alloc-segment-directory`, which `class-aware-dirty`
// implies). Gating the declarations keeps their compiled-file set identical
// to when they lived directly in `alloc_core`.

/// R12-7 stage 2 (`class-aware-dirty`, EXPERIMENTAL): the lazily-materialised
/// per-(segment, class) dirty-bit sidecar (`PerClassDirty`) — see the module
/// doc for the full design. A named `unsafe` seam (single documented reason:
/// dereferencing the `OncePtrCell`-published sidecar pointer).
#[cfg(feature = "class-aware-dirty")]
pub(crate) mod dirty_by_class;
pub(crate) mod node;
/// NUMA OS-seam: NUMA-node detection and segment binding.
/// `pub` (not `pub(crate)`) only because `alloc_core` itself is
/// `#[doc(hidden)]` (see `lib.rs`): the public surface is test-only (the
/// `#[doc(hidden)]` re-export), reachable by the isolated NUMA unit test.
/// Nothing here is stable public API.
#[cfg(feature = "numa-aware")]
pub mod numa;
pub(crate) mod os;
/// R14-9 (task #294): the owner-only lazily-materialised sidecar primitive
/// (`reserve`/`deref`/`deref_mut`) shared by `os.rs`'s `SegmentDirectory`
/// reservation and `large_cache_extended.rs`'s `LargeCacheExtension`
/// reservation. A named `unsafe` seam (two documented reasons: typed
/// `ptr::write` init, and the `&'static [mut] T` deref boundary). See the
/// module doc for why `PerClassDirty` (cross-thread-published via
/// `OncePtrCell`) is NOT migrated onto this type.
pub(crate) mod sidecar;
/// R2-12: process-wide reservation/release accounting for the owner-only
/// sidecars (`sidecar.rs`'s `AccountedSidecar` token) — the leak-fix
/// acceptance oracle. Pure counters + one `dbg_*` snapshot accessor; see the
/// module doc for the three counter breakouts and the `internals` gating.
pub(crate) mod sidecar_stats;
pub(crate) mod size_classes;
