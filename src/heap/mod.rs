//! Phase 9/10 -- per-thread heap + intrusive free lists.
//!
//! With `alloc` only: the Phase 9 single-thread-owner allocator.
//! With `alloc-xthread`: adds cross-thread free via the Treiber stack protocol.
//!
//! Re-exports only -- no logic lives here (per the one-export-per-file rule).

mod free_list;
#[allow(clippy::module_inception)]
mod heap;
#[cfg(feature = "alloc-xthread")]
pub(crate) mod thread_free;
mod tls;

pub use heap::Heap;
#[cfg(feature = "alloc-global")]
pub(crate) use tls::with_heap_try;
pub use tls::with_heap;
