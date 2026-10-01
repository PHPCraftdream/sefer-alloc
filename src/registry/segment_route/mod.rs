//! Process-wide route directory: HeapCore segment registration is connected;
//! foreign-free ingress remains on the existing path.
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

mod directory;
mod error;
mod kind;
mod large_state;
mod pin;
mod registration;
mod route_cut;
mod route_record;
mod route_scan;
mod small_sidecar;
#[cfg(all(miri, feature = "internals", feature = "bench-internals"))]
mod terminal_publication_gate;

pub use directory::RouteDirectory;
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
