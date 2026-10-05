//! [`HeapRegistry`] — the global self-hosting heap slot table (§2.1 of
//! `ALLOC_PLAN_PHASE12-13.md`): claim/recycle over the process-global
//! [`Registry`](super::bootstrap::Registry) slot array.
//!
//! This is the lock-free fundament of Phase 12: every thread's heap is a SLOT
//! in this registry, not a TLS-owned `Box`. A thread claims a slot on first
//! use, caches a raw `*mut HeapCore` to it in TLS (12.3), and on thread exit
//! recycles the slot (whole-heap reuse — Phase 12.5: the `HeapCore` and its
//! segments stay with the slot, and the next claimer reuses them in full).
//! FREE discovery scans materialised chunks. No intrusive free list remains;
//! only a successful slot-state CAS grants owner or maintenance authority.

// R1-07 (src review round 1): this file is `mod.rs` — decls and re-exports
// only, per the "mod.rs — reexports only, no code" rule — so it carries NO
// `#![allow(unsafe_code)]` of its own; a module-level allow here previously
// duplicated the allow each child below already carries independently.
// [`claim`], [`counters`], and [`stack`] each have their OWN tier-1
// `#![allow(unsafe_code)]` seam (`claim.rs` documents the pointer handoff
// `*mut HeapCore` out of a slot's `UnsafeCell`; every `unsafe` block across
// the three files carries its own `// SAFETY:` proof).
mod claim;
mod counters;
mod maintenance;
mod stack;
#[cfg(feature = "internals")]
#[doc(hidden)]
pub use super::bootstrap::saturation::SaturationHint;
#[cfg(feature = "internals")]
#[doc(hidden)]
pub use stack::pick_with_saturation;

pub use claim::{HeapRegistry, MaintenanceLease};
// Ph4a (task #2091): test-only exposure of the claim lease (`internals`
// builds only; the struct itself is `pub(crate)` without `internals`).
#[cfg(feature = "internals")]
#[doc(hidden)]
pub use claim::HeapLease;
// R1-10 (src review round 1): the fallback heap's own process-static
// magazine/large-cache hit counters, bound by `global::fallback` at init
// (there is no registry slot to bind for it) — see `counters::
// FALLBACK_TCACHE_HITS`'s doc comment.
#[cfg(feature = "alloc-decommit")]
pub use counters::large_cache_hits_total;
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit"
))]
pub use counters::tcache_and_large_cache_hits_total;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub use counters::tcache_hits_total;
#[cfg(feature = "alloc-decommit")]
pub(crate) use counters::FALLBACK_LARGE_CACHE_HITS;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub(crate) use counters::FALLBACK_TCACHE_HITS;
pub use counters::{config_conflicts_total, heaps_claimed_high_water};
pub use counters::{dbg_claim_then_simulate_oom, dbg_slot_initialised};
// R2-11 (task #2013): test-only hook to deterministically reproduce the
// `bump_count`-then-`slot()` window `walk_initialised_slots` used to race
// into — see `counters::dbg_bump_count_without_materialising`'s doc comment.
#[cfg(feature = "internals")]
pub use counters::dbg_bump_count_without_materialising;
