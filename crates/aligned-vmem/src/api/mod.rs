mod internal;

mod advise_huge_pages;
mod decommit;
mod leak_zeroed_pages;
mod recommit;
mod release;
mod reserve;
#[cfg(feature = "huge-pages")]
mod reserve_aligned_huge;
mod residency;

mod lazy_commit;

pub(crate) use decommit::dispatch_try_decommit;
pub use decommit::{decommit, try_decommit};
pub use leak_zeroed_pages::leak_zeroed_pages;
pub use recommit::{recommit, try_recommit};
pub use release::{release, release_parts};
pub use reserve::{reserve_aligned, try_reserve_aligned};
pub use residency::{ResidencyRegion, ResidencySample};

pub use lazy_commit::decommit_lazy;

#[cfg(feature = "lazy-commit")]
pub use lazy_commit::{
    commit_range, reserve_aligned_lazy, try_commit_range, try_reserve_aligned_lazy,
};

#[cfg(feature = "huge-pages")]
pub use reserve_aligned_huge::{reserve_aligned_huge, try_reserve_aligned_huge};
