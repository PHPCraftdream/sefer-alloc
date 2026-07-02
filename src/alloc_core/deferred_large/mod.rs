//! Shared A1 primitives (0.3.x task #132): the cross-thread deferred-free
//! Treiber stack for Large/huge segments, extracted so BOTH public allocator
//! faces — [`crate::registry::heap_core::HeapCore`] (the `SeferAlloc`/
//! `GlobalAlloc` face) and [`crate::heap::Heap`] (the explicit `Heap`/
//! `with_heap` face) — reuse the identical push/drain logic (including the
//! double-push guard hardening) instead of maintaining two copies that could
//! drift apart.
//!
//! Both faces' `thread_free`/identity `AtomicPtr<u8>` field plays a dual
//! role: its ADDRESS is a stable per-heap identity (stamped into segment
//! headers as `owner_thread_free`, compared by pointer to recognise
//! ownership), and its VALUE is the head of this Treiber stack of Large
//! segment bases deferred for cross-thread reclaim. See `push_large_deferred_free`
//! and `drain_large_deferred_free` for the mechanism.

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
