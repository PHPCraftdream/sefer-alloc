mod align_up_addr;
mod decommit_kind;
#[cfg(miri)]
mod miri;
#[cfg(all(unix, not(miri)))]
mod unix;
#[cfg(all(not(windows), not(unix), not(miri)))]
mod unsupported;
#[cfg(all(windows, not(miri)))]
mod windows;

#[cfg(not(miri))]
pub(crate) use align_up_addr::align_up_addr;
pub(crate) use decommit_kind::DecommitKind;

#[cfg(all(miri, feature = "huge-pages"))]
pub(crate) use miri::reserve_aligned_huge_raw;
#[cfg(all(miri, feature = "lazy-commit"))]
pub(crate) use miri::{commit_range_impl, reserve_aligned_lazy_raw};
#[cfg(miri)]
pub(crate) use miri::{
    decommit_pages_impl, recommit_pages_impl, release_reservation, reserve_aligned_raw,
};

#[cfg(all(unix, not(miri), feature = "huge-pages"))]
pub(crate) use unix::reserve_aligned_huge_raw;
#[cfg(all(unix, not(miri), feature = "lazy-commit"))]
pub(crate) use unix::{commit_range_impl, reserve_aligned_lazy_raw};
#[cfg(all(unix, not(miri)))]
pub(crate) use unix::{
    decommit_pages_impl, recommit_pages_impl, release_reservation, reserve_aligned_raw, sysconf,
    _SC_PAGESIZE,
};

#[cfg(all(windows, not(miri), feature = "huge-pages"))]
pub(crate) use windows::reserve_aligned_huge_raw;
#[cfg(all(windows, not(miri), feature = "lazy-commit"))]
pub(crate) use windows::{commit_range_impl, reserve_aligned_lazy_raw};
#[cfg(all(windows, not(miri)))]
pub(crate) use windows::{
    decommit_pages_impl, recommit_pages_impl, release_reservation, reserve_aligned_raw,
    GetSystemInfo, SystemInfo, WIN_ALLOCATION_GRANULARITY,
};
