//! Unconnected process-wide route directory and sidecar substrate.
//!
//! Blocking progress: lookup takes one shard mutex and binary-searches a
//! sorted pointer array. Registration alone grows arrays using `System`;
//! insertion/removal shift O(routes-in-shard) pointers under the mutex.
//! Removal and pin release never allocate. Pointer-array capacity is retained
//! at its high-water mark, while unlinked entries and sidecars are reclaimed
//! on the last pin. Pins do not keep reservation memory alive: an independent
//! owner ledger must do that until every unpublished free is accounted for.

mod directory;
mod error;
mod kind;
mod large_publisher;
mod large_state;
mod pin;
mod registration;
mod small_publisher;
mod small_sidecar;

pub use directory::RouteDirectory;
pub use error::RouteError;
pub use kind::RouteKind;
pub use large_publisher::LargePublisher;
pub use large_state::LargeState;
pub use pin::RoutePin;
pub use registration::RouteRegistration;
pub use small_publisher::SmallPublisher;
pub use small_sidecar::SmallSidecar;
