//! Opt-in prototype (`exact-object-proto`, NOT in `production`): narrow
//! requests are served by one exact `System` allocation each, registered in an
//! out-of-object descriptor table. Not a production backend.

mod exact_shard;
mod exact_table;
mod insert_outcome;
mod narrow;

// `exact_shard` takes its sync primitives from here (a loom model binds loom ones).
use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::yield_now;

pub use narrow::ExactNarrow;
