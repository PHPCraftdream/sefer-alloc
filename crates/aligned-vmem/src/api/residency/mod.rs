//! Bounded residency sampling over an owning [`Reservation`](crate::Reservation)
//! (task #53, J13b VM-part): the region type, protocol bounds, the sampling
//! method and the Linux/Android `mincore(2)` FFI seam. In the same pure-move
//! split, the lazy-commit free functions moved to `api/lazy_commit/` —
//! public paths unchanged.

mod mincore;
mod region;
mod sample;

pub use region::ResidencyRegion;
pub use sample::ResidencySample;
