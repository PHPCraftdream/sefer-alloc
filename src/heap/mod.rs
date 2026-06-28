//! Phase 9/10/12.1 -- per-thread heap over the segment substrate.
//!
//! With `alloc` only: the Phase 9 single-thread-owner allocator.
//! With `alloc-xthread`: adds cross-thread free via the Treiber stack protocol.
//!
//! Phase 12.1: the per-class free-list state lives in each segment's `BinTable`
//! (self-hosted in segment memory), NOT in a heap-local array. The heap is
//! thin (current-segment pointer + cross-thread stack).
//!
//! Re-exports only -- no logic lives here (per the one-export-per-file rule).

#[allow(clippy::module_inception)]
mod heap;
#[cfg(feature = "alloc-xthread")]
pub(crate) mod thread_free;
mod tls;

pub use heap::Heap;
pub use tls::with_heap;
#[cfg(feature = "alloc-global")]
#[allow(unused_imports)]
pub(crate) use tls::with_heap_try;
