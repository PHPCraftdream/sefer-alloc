//! Unconnected process-wide route-identity directory substrate.
//!
//! Blocking progress: lookup and removal take one short shard mutex and
//! lookup scans at most `SLOTS_PER_SHARD` entries. Fixed inline slots avoid
//! allocator recursion. This layer stores allocator-origin roots and pins
//! route identity; it has no sidecar allocation or publication API yet.
//! Adding a new unsafe System-allocation seam needs the README/lib.rs unsafe
//! inventory changes forbidden by this task's write set. It must not be used
//! as a completed remote-free ingress.

mod directory;
mod error;
mod kind;
mod pin;
mod registration;

pub use directory::RouteDirectory;
pub use error::RouteError;
pub use kind::RouteKind;
pub use pin::RoutePin;
pub use registration::RouteRegistration;
