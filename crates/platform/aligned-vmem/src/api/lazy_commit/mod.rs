mod decommit_lazy;

#[cfg(feature = "lazy-commit")]
mod commit_range;
#[cfg(feature = "lazy-commit")]
mod reserve_aligned_lazy;

pub use decommit_lazy::decommit_lazy;

#[cfg(feature = "lazy-commit")]
pub use commit_range::{commit_range, try_commit_range};
#[cfg(feature = "lazy-commit")]
pub use reserve_aligned_lazy::{reserve_aligned_lazy, try_reserve_aligned_lazy};
