//! Process-wide route directory (see `segment_route`): `route_directory` holds
//! entries, pins and shards; `blocks` the sorted pointer storage.

mod blocks;
mod route_directory;

pub(super) use route_directory::EntryHandle;
pub(crate) use route_directory::HeapIngress;
pub use route_directory::RouteDirectory;
// Names the children take from `segment_route` (the Loom harnesses supply
// their own under the same names).
pub(super) use super::{
    shard_lock, AtomicBool, AtomicUsize, LargeState, RouteError, RouteKind, RoutePin,
    RouteRegistration, SmallSidecar,
};
