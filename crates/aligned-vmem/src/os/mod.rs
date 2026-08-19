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
#[cfg(all(miri, feature = "lazy-commit", not(aligned_vmem_mock)))]
pub(crate) use miri::{commit_range_impl, reserve_aligned_lazy_raw};
#[cfg(all(miri, not(aligned_vmem_mock)))]
pub(crate) use miri::{decommit_pages_impl, recommit_pages_impl};
#[cfg(miri)]
pub(crate) use miri::{release_reservation, reserve_aligned_raw};

#[cfg(all(
    unix,
    not(miri),
    not(aligned_vmem_mock),
    any(target_os = "linux", target_os = "android"),
    feature = "huge-pages"
))]
pub(crate) use unix::linux_huge_range_is_madvise_eligible;
#[cfg(all(unix, not(miri), feature = "huge-pages"))]
pub(crate) use unix::reserve_aligned_huge_raw;
#[cfg(all(unix, not(miri), feature = "lazy-commit", not(aligned_vmem_mock)))]
pub(crate) use unix::{commit_range_impl, reserve_aligned_lazy_raw};
#[cfg(all(unix, not(miri), not(aligned_vmem_mock)))]
pub(crate) use unix::{decommit_pages_impl, recommit_pages_impl};
#[cfg(all(unix, not(miri)))]
pub(crate) use unix::{release_reservation, reserve_aligned_raw, sysconf, _SC_PAGESIZE};

#[cfg(all(windows, not(miri), feature = "huge-pages"))]
pub(crate) use windows::reserve_aligned_huge_raw;
#[cfg(all(windows, not(miri), feature = "lazy-commit", not(aligned_vmem_mock)))]
pub(crate) use windows::{commit_range_impl, reserve_aligned_lazy_raw};
#[cfg(all(windows, not(miri), not(aligned_vmem_mock)))]
pub(crate) use windows::{decommit_pages_impl, recommit_pages_impl};
#[cfg(all(windows, not(miri)))]
pub(crate) use windows::{
    release_reservation, reserve_aligned_raw, GetSystemInfo, SystemInfo, WIN_ALLOCATION_GRANULARITY,
};
