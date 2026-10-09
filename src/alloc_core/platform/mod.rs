//! OS/platform shims and confined raw-memory seams.
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
/// reservation. Named unsafe seam for reservation initialization and
/// owner-tied dereference; see its module-level safety rationale.
/// Foreign-free route sidecars use separate pinned storage.
pub(crate) mod sidecar;
/// R2-12: process-wide reservation/release accounting for the owner-only
/// sidecars (`sidecar.rs`'s `AccountedSidecar` token) — the leak-fix
/// acceptance oracle. Pure counters + one `dbg_*` snapshot accessor; see the
/// module doc for the three counter breakouts and the `internals` gating.
pub(crate) mod sidecar_stats;
pub(crate) mod size_classes;
