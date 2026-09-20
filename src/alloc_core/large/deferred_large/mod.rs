//! Shared A1 primitives (0.3.x task #132): the cross-thread deferred-free
//! Treiber stack for Large/huge segments, extracted so the
//! [`crate::registry::heap_core::HeapCore`] allocator face (the `SeferAlloc`/
//! `GlobalAlloc` path) reuses a single push/drain logic (including the
//! double-push guard hardening) instead of maintaining copies that could
//! drift apart.
//!
//! Both faces' `thread_free`/identity `AtomicPtr<u8>` field plays a dual
//! role: its ADDRESS is a stable per-heap identity (stamped into segment
//! headers as `owner_thread_free`, compared by pointer to recognise
//! ownership), and its VALUE is the head of this Treiber stack of Large
//! segment bases deferred for cross-thread reclaim. See `push_large_deferred_free`
//! and `drain_large_deferred_free` for the mechanism.
//!
//! ## Provenance model (task #140)
//!
//! This is a cross-allocation intrusive Treiber stack: segment `A`'s
//! `deferred_next` header field (this stack's link word) holds the address of
//! segment `B`, a DIFFERENT OS reservation with unrelated provenance — no
//! single `u64` link word can carry both an address and a provenance token
//! for a foreign allocation. Full strict-provenance conformance is therefore
//! unreachable for this stack by a structural argument that holds for any
//! cross-allocation intrusive link. `push_large_deferred_free` calls
//! `expose_provenance` on every real head pointer before packing its address
//! into the link word; `drain_large_deferred_free` reconstructs via
//! `with_exposed_provenance_mut` on load — this crate's sanctioned
//! exposed-provenance pairing. Plain `cargo +nightly miri test` (not
//! `-Zmiri-strict-provenance`) is the validation mode for this module.

#[cfg(feature = "alloc-xthread")]
mod drain;
#[cfg(feature = "alloc-xthread")]
mod layout_consistent;
#[cfg(feature = "alloc-xthread")]
mod push;
#[cfg(feature = "alloc-xthread")]
mod tail;

#[cfg(feature = "alloc-xthread")]
pub(crate) use drain::drain_large_deferred_free;
#[cfg(feature = "alloc-xthread")]
#[doc(hidden)]
pub use drain::DBG_LARGE_XTHREAD_RECLAIMED;
#[cfg(feature = "alloc-xthread")]
pub(crate) use layout_consistent::large_layout_consistent;
#[cfg(feature = "alloc-xthread")]
pub(crate) use push::push_large_deferred_free;
