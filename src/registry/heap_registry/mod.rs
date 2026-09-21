//! [`HeapRegistry`] — the global self-hosting heap slot table (§2.1 of
//! `ALLOC_PLAN_PHASE12-13.md`): claim/recycle over the process-global
//! [`Registry`](super::bootstrap::Registry) slot array.
//!
//! This is the lock-free fundament of Phase 12: every thread's heap is a SLOT
//! in this registry, not a TLS-owned `Box`. A thread claims a slot on first
//! use, caches a raw `*mut HeapCore` to it in TLS (12.3), and on thread exit
//! recycles the slot (whole-heap reuse — Phase 12.5: the `HeapCore` and its
//! segments stay with the slot, and the next claimer reuses them in full).
//! (The abandoned-segments / adoption substrate that previously lived here was
//! removed — task #97 / R4-5; see the "ABA defence" note below.)
//!
//! ## Phase 12.2 scope
//!
//! This file ships the structure + the claim/recycle API, exercised
//! single-threaded by `tests/registry_basic.rs`. The orderings are written
//! CORRECT for the lock-free concurrent case from day one (loom verification
//! is Phase 12.4); each atomic op carries a `// why:` comment.
//!
//! ## ABA defence
//!
//! The [`Registry::free_slots`] Treiber stack carries a STRICTLY MONOTONIC
//! tag in the high bits of its `AtomicU64` head (48 bits — the low 16 hold
//! the slot index, task W7a), bumped on every push and never wrapped: a push
//! that would bump the tag past `TaggedIndex::TAG_MAX` is refused
//! (`Err(TagExhausted)`) instead, sealing that head. This ELIMINATES the
//! classic ABA (pop-X, re-push-X while a racer is parked with head=X), not
//! merely mitigates it — the re-push bumps the tag, so the racer's CAS on
//! `(X, old_tag)` fails, and because the tag can never recur, no amount of
//! further churn can ever reinstate `(X, old_tag)` (closes P1-1: a full tag
//! wrap used to be able to reproduce a stale head word exactly; it no longer
//! can, by construction). The stack itself — the tagged head, the seal, the
//! H-2 empty-transition tag preservation, the RAD-1 lazy links, and the
//! tag-width-vs-churn budget analysis (a `2^48`-push lifetime takes ~89
//! years at 100k pushes/s, but only ~16 days at the crate's documented
//! `2 × 10^8` RMW/s working ceiling — and ~3.3 days at the `10^9` top of the
//! hardware range, after which `free_slots` seals rather than corrupts) —
//! lives in the `tagged-index-stack` crate (CRATE-P7); `free_slots` is a
//! `tagged_index_stack::StackHead<16>` and `pop_free_slot` /
//! `push_free_slot` below delegate to it through `Registry`'s own
//! `tagged_index_stack::StackStorage<16>` impl (the slot-resident links).
//! `push_free_slot`'s handling of the (astronomically rare, but no longer
//! merely "improbable") sealed case is documented at its own definition
//! below.
//!
//! (The abandoned-segments intrusive Treiber stack that previously also lived
//! here was removed — task #97 / R4-5. It was unreachable on the production
//! whole-slot-reuse path and internally inconsistent; git history preserves
//! it. The `deferred_next` header field it shared with the
//! `deferred_large` cross-thread-free stack REMAINS — that stack is a
//! separate, live feature and is untouched by this removal.)
//!
//! [`Registry::free_slots`]: crate::registry::bootstrap::Registry::free_slots

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see
// `src/lib.rs`); this is the documented registry seam (the pointer handoff
// `*mut HeapCore` out of a slot's `UnsafeCell`). R6-OPT-P0-2 (round 1): the
// former `get_unchecked` on a `'static` inline slot array is gone — every
// slot-array access now goes through `Registry::slot(idx)`
// (`bootstrap`), the single chunk-resolving accessor, which is safe
// (range-checked via `debug_assert!` and array-index, not `get_unchecked`).
// `allow` lifts the crate-level `deny` for this module only — `unsafe`
// anywhere else in the crate is a hard error. Every remaining `unsafe` block
// carries a `// SAFETY:` proof.
#![allow(unsafe_code)]

mod claim;
mod counters;
mod stack;

pub use claim::HeapRegistry;
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
pub use counters::{config_conflicts_total, heaps_claimed_high_water};
pub use counters::{dbg_claim_then_simulate_oom, dbg_slot_initialised};
