//! Cross-thread free routing for [`HeapCore`](crate::registry::HeapCore):
//! terminal sidecar-ingress drain and the foreign-thread dealloc routing protocol.
//! All methods are `#[cfg(feature = "alloc-xthread")]`.
//! Mechanical split of `heap_core.rs` (task R4-10), extended with the
//! per-thread route pin cache used by `routing.rs` (task b4-r1).

pub(crate) mod heap_route_pin_cache;
pub(crate) mod route_pin_cache;
mod routing;
#[cfg(feature = "alloc-global")]
mod sidecar_drain;
