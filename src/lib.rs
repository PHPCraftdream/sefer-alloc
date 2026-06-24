//! # sefer-alloc
//!
//! A safe, *handle-addressed* region store. Instead of handing out raw
//! pointers, a [`Region<T>`] hands out small generational [`Handle<T>`]
//! values; the bytes live in a dense, cache-friendly backing store that the
//! region is free to move. A stale handle never resolves to a live value — it
//! returns `None`, never undefined behaviour.
//!
//! ## The engine
//!
//! The single-threaded core is a thin **typed membrane** over
//! [`slotmap`](https://crates.io/crates/slotmap): [`Region<T>`] wraps a
//! `slotmap::SlotMap<DefaultKey, T>` and [`Handle<T>`] is a newtype over a
//! `DefaultKey` plus `PhantomData<fn() -> T>`, so handles stay generic-over-`T`
//! and typed (which raw slotmap keys are not). `slotmap`'s audited `unsafe`
//! owns the dense generational layout — the free list, generation bump on
//! remove, and version-saturation retirement; this crate adds the typed
//! boundary and stays `#![forbid(unsafe_code)]`.
//!
//! ## Scope (honest)
//!
//! This is an *application-level* store, not a drop-in global allocator. The
//! global-allocator descent (`ByteRegion` + `GlobalAlloc`) is a later,
//! research-flagged phase; see `docs/PLAN.md`. For a process-wide allocator,
//! reach for `mimalloc`.
//!
//! See `docs/INVARIANTS.md` for the safety invariants this crate upholds and
//! `docs/DESIGN.md` for the architecture.
//!
//! ## Example
//!
//! ```
//! use sefer_alloc::Region;
//!
//! let mut region = Region::new();
//! let a = region.insert("alpha");
//! let b = region.insert("beta");
//!
//! assert_eq!(region.get(a), Some(&"alpha"));
//!
//! region.remove(a);
//! assert_eq!(region.get(a), None); // stale handle → None, never UB
//! assert_eq!(region.get(b), Some(&"beta")); // others stay valid
//! ```

#![forbid(unsafe_code)]

mod handle;
mod region;
mod sync_region;

#[cfg(feature = "experimental")]
mod concurrent;

pub use handle::Handle;
pub use region::Region;
pub use sync_region::SyncRegion;

#[cfg(feature = "experimental")]
pub use concurrent::{LockFreeHandle, LockFreeRegion};
