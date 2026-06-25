//! Phase 9 -- per-thread heap + intrusive free lists (the hot path).
//!
//! Re-exports only -- no logic lives here (per the one-export-per-file rule).

mod free_list;
#[allow(clippy::module_inception)]
mod heap;
mod tls;

pub use heap::Heap;
pub use tls::with_heap;
