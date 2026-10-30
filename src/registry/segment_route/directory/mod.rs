//! Process-wide route directory (see `segment_route`): `route_directory` holds
//! entries, pins and shards; `blocks` the sorted pointer storage.

mod blocks;
mod ingress;
mod route_directory;

#[cfg_attr(loom, allow(unused_imports))]
pub(crate) use ingress::{HeapIngress, INGRESS_CELLS, INGRESS_WORDS};
pub(super) use route_directory::EntryHandle;
pub use route_directory::RouteDirectory;
// Names the children take from `segment_route` (the Loom harnesses supply
// their own under the same names).
pub(super) use super::{
    fence, shard_lock, AtomicBool, AtomicU64, AtomicU8, AtomicUsize, LargeState, Ordering,
    RouteError, RouteKind, RoutePin, RouteRegistration, SmallSidecar,
};
