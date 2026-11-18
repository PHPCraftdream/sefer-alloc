//! Process-wide route directory connects HeapCore segment registration to
//! foreign-free terminal publication through descriptor pins and sidecars.
//!
//! Blocking progress: lookup takes one shard mutex and binary-searches a
//! sorted block index, then one fixed-capacity sorted block. Registration
//! alone allocates (blocks, block index) from `System`, outside the lock;
//! insertion/removal shift at most one block (plus the short block index on
//! split/merge) under the mutex, never the whole shard. Removal and pin
//! release never allocate. Block-index capacity is retained at its high-water
//! mark, plus one spare block, while unlinked entries and sidecars are
//! reclaimed on the last pin. Pins do not keep reservation memory alive: an independent
//! owner ledger must do that until every unpublished free is accounted for.

// Atomic seam for source-including Loom harnesses. The protocol files resolve
// their atomics through `super`, so the harness can re-export loom atomics and
// execute the same production sources.
pub(crate) use core::sync::atomic::{
    fence, AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};

mod directory;
#[path = "route_protocol/error.rs"]
mod error;
#[path = "route_protocol/kind.rs"]
mod kind;
#[path = "route_protocol/large_state.rs"]
mod large_state;
#[path = "route_state/pin.rs"]
mod pin;
#[path = "route_state/registration.rs"]
mod registration;
#[path = "route_protocol/route_cut.rs"]
mod route_cut;
#[path = "route_state/route_pin_cache.rs"]
pub mod route_pin_cache;
#[path = "route_protocol/route_record.rs"]
mod route_record;
#[path = "route_protocol/route_scan.rs"]
mod route_scan;
#[path = "route_state/shard_lock.rs"]
mod shard_lock;
#[path = "route_state/small_sidecar.rs"]
mod small_sidecar;
#[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
#[path = "route_protocol/terminal_publication_gate.rs"]
mod terminal_publication_gate;

pub use directory::RouteDirectory;
#[cfg_attr(feature = "numa-aware", allow(unused_imports))]
#[doc(hidden)]
pub use directory::{HeapIngress, INGRESS_CELLS, INGRESS_WORDS};
pub use error::RouteError;
pub use kind::RouteKind;
pub use large_state::LargeState;
pub use pin::RoutePin;
pub use registration::RouteRegistration;
pub use route_cut::RouteCut;
pub use route_record::RouteRecord;
pub use route_scan::RouteScan;
pub use small_sidecar::SmallSidecar;
#[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
pub use terminal_publication_gate::TerminalPublicationGate;
