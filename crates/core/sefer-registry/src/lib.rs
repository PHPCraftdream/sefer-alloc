//! Heap ownership, slot recycling, and the process-global fallback allocator.

// R2-17: allocator metadata and provenance are audited only for 64-bit targets.
#[cfg(all(feature = "alloc-core", not(target_pointer_width = "64")))]
compile_error!("sefer-registry: allocator features require a 64-bit target (R2-17)");

#[cfg(feature = "alloc-global")]
mod alloc_core;
#[cfg(feature = "alloc-global")]
#[doc(hidden)]
pub mod fallback;
#[cfg(feature = "alloc-global")]
#[doc(hidden)]
pub mod registry;

// The actual-source Loom harness supplies these same parent bindings.
#[cfg(feature = "alloc-global")]
use core::hint::spin_loop;
#[cfg(feature = "alloc-global")]
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
#[cfg(feature = "alloc-global")]
use sefer_ghost::ghost;
#[cfg(feature = "alloc-global")]
use sefer_platform::process_exit::process_exit_in_progress;
#[cfg(feature = "alloc-global")]
use std::thread::yield_now;
