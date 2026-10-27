//! Ghost checks (docs/GHOST_CHECKS_PLAN.md §3): assertions and event counts that
//! exist only in checking builds — `cfg(any(loom, miri, kani, sefer_ghost))` — and
//! expand to nothing in production. No `unsafe`, no allocation, no fields in
//! production structs; the side table uses `core` atomics, never loom's, so a
//! ghost check adds no happens-before edge a model checker could see.

mod macros;
#[cfg(all(feature = "internals", any(loom, miri, sefer_ghost)))]
mod probe;
#[cfg(any(loom, miri, sefer_ghost))]
mod shadow;

// First call sites in production code land with G2 (route pins).
#[allow(unused_imports)]
pub(crate) use macros::{ghost_assert, ghost_event};
#[cfg(all(feature = "internals", any(loom, miri, sefer_ghost)))]
pub use probe::probe;
#[cfg(any(loom, miri, sefer_ghost))]
#[allow(unused_imports)]
pub use shadow::{count, fail, record, reset};
