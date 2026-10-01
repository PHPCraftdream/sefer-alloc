//! Opt-in prototype (`exact-object-proto`, NOT in `production`): narrow
//! requests are served by one exact `System` allocation each, registered in an
//! out-of-object descriptor table. Not a production backend.

mod exact_fatal;
mod exact_shard;
mod exact_table;
mod insert_outcome;
mod narrow;

pub use narrow::ExactNarrow;
