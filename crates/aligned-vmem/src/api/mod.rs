mod internal;

#[cfg(feature = "lazy-commit")]
mod commit_range;
mod decommit;
mod decommit_lazy;
mod leak_zeroed_pages;
mod recommit;
mod release;
mod reserve;
#[cfg(feature = "huge-pages")]
mod reserve_aligned_huge;
#[cfg(feature = "lazy-commit")]
mod reserve_aligned_lazy;

pub(crate) use decommit::dispatch_try_decommit;
pub use decommit::{decommit, try_decommit};
pub use decommit_lazy::decommit_lazy;
pub use leak_zeroed_pages::leak_zeroed_pages;
pub use recommit::{recommit, try_recommit};
pub use release::{release, release_parts};
pub use reserve::{reserve_aligned, try_reserve_aligned};

#[cfg(feature = "lazy-commit")]
pub use commit_range::{commit_range, try_commit_range};
#[cfg(feature = "lazy-commit")]
pub use reserve_aligned_lazy::{reserve_aligned_lazy, try_reserve_aligned_lazy};

#[cfg(feature = "huge-pages")]
pub use reserve_aligned_huge::{reserve_aligned_huge, try_reserve_aligned_huge};
