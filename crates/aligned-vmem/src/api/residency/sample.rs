//! `Reservation::try_sample_residency` — bounded, passive base-page
//! residency sampling (task #53, J13b VM-part).

use super::region::ResidencyRegion;
use crate::error::VmemError;
use crate::page_size::{page_size_or_poison, PAGE_SIZE_QUERY_FAILED};
use crate::Reservation;

// Upper bound of the mincore vector for ONE region, in entries: a sampled
// region is at most `RESIDENCY_SAMPLE_MAX_REGION_BYTES` (2 MiB) and the
// validated runtime page size is always `>= PAGE` (4 KiB) and a power of
// two (`page_size`'s validation contract), so one region's vector never
// exceeds `2 MiB / 4 KiB = 512` entries. The buffer is stack-allocated;
// the sampler performs no heap allocation.
#[cfg_attr(
    not(all(
        unix,
        not(miri),
        not(aligned_vmem_mock),
        any(target_os = "linux", target_os = "android")
    )),
    allow(dead_code)
)]
const RESIDENCY_SAMPLE_VEC_ENTRIES: usize = 512;

impl Reservation {
    /// Protocol bound: at most this many regions per
    /// [`try_sample_residency`](Self::try_sample_residency) call.
    pub const RESIDENCY_SAMPLE_MAX_REGIONS: usize = 16;
    /// Protocol bound: at most this many bytes per single sampled region;
    /// a longer [`ResidencyRegion`] is [`VmemError::invalid_argument`].
    pub const RESIDENCY_SAMPLE_MAX_REGION_BYTES: usize = 2 * 1024 * 1024;

    /// Bounded, passive residency sampling over this reservation's usable
    /// span: one `mincore(2)` call per sampled region occurrence (at most
    /// [`RESIDENCY_SAMPLE_MAX_REGIONS`](Self::RESIDENCY_SAMPLE_MAX_REGIONS)
    /// per call, at most
    /// [`RESIDENCY_SAMPLE_MAX_REGION_BYTES`](Self::RESIDENCY_SAMPLE_MAX_REGION_BYTES)
    /// per region), returning BASE-page counts for an estimator ratio.
    ///
    /// The counts are PAGES, not regions-with-any-resident-page;
    /// `resident_pages <= sampled_pages` always. Duplicate/overlapping
    /// regions are NOT deduplicated — each occurrence contributes to both
    /// counters (a caller wanting the union ratio passes disjoint regions).
    /// The effective window of a region is the whole pages fully inside it
    /// (start rounds up, end rounds down): a region smaller than one page
    /// contributes zero to both counts. An EMPTY region slice is `Ok` with
    /// a zero sample on EVERY platform — a no-op that consults no backend.
    ///
    /// Pure passive observation of an owned, live mapping: no heap
    /// allocation (static 512-entry stack vector — bound proof above), no
    /// `/proc` or `/sysfs` reads, no fault-in/touch of the sampled pages,
    /// the mapping is never mutated. `&self` of an owning `Reservation`
    /// guarantees the mapping is live for the call; no raw-pointer API is
    /// exposed, so the sampler cannot be pointed at a foreign or unmapped
    /// range.
    ///
    /// What a success means: the kernel answered for exactly these base
    /// pages. Observed semantics, honestly stated: `MADV_DONTNEED`-
    /// decommitted pages read as NOT resident; `MADV_FREE` pages read as
    /// resident until the kernel actually reclaims them; under THP/mTHP a
    /// single faulted byte can make a WHOLE huge page resident — that is
    /// kernel truth, and guaranteed base-page granularity requires a
    /// caller-side control mapping (`MADV_NOHUGEPAGE`); this API applies
    /// no hint of its own. On `MAP_HUGETLB` reservations residency is
    /// huge-page granular and the base-page reading does not apply. Only
    /// the `mincore` LOW bit is treated as the residency bit.
    ///
    /// # Errors
    ///
    /// - [`VmemError::invalid_argument`]: more regions than
    ///   [`RESIDENCY_SAMPLE_MAX_REGIONS`](Self::RESIDENCY_SAMPLE_MAX_REGIONS),
    ///   or any region with `offset + len` overflow, an end past
    ///   [`len`](Reservation::len), or `len` over
    ///   [`RESIDENCY_SAMPLE_MAX_REGION_BYTES`](Self::RESIDENCY_SAMPLE_MAX_REGION_BYTES);
    /// - [`VmemError::os_refusal_unknown_code`]: the poisoned page-size
    ///   query, a platform without the `mincore` backend (non-Linux/
    ///   Android, miri, `aligned_vmem_mock`), or an armed `fault-injection`
    ///   hook — all PRE-syscall, all sharing the one no-code sentinel and
    ///   deliberately NOT distinguishable from each other. NEVER a forged
    ///   zero sample: no caller can mistake "no backend" for "nothing
    ///   resident";
    /// - [`VmemError::last_os_error`]: the kernel refused a `mincore`
    ///   call (captured immediately). The WHOLE call fails and the
    ///   partially accumulated counters are discarded — no half-observation
    ///   leaks out.
    pub fn try_sample_residency(
        &self,
        regions: &[ResidencyRegion],
    ) -> Result<ResidencySample, VmemError> {
        if regions.len() > Self::RESIDENCY_SAMPLE_MAX_REGIONS {
            return Err(VmemError::invalid_argument());
        }
        let ps = page_size_or_poison();
        if ps == PAGE_SIZE_QUERY_FAILED {
            return Err(VmemError::os_refusal_unknown_code());
        }
        for region in regions {
            let Some(end) = region.offset.checked_add(region.len) else {
                return Err(VmemError::invalid_argument());
            };
            if end > self.len() || region.len > Self::RESIDENCY_SAMPLE_MAX_REGION_BYTES {
                return Err(VmemError::invalid_argument());
            }
        }
        if regions.is_empty() {
            return Ok(ResidencySample {
                resident_pages: 0,
                sampled_pages: 0,
            });
        }
        #[cfg(all(
            unix,
            not(miri),
            not(aligned_vmem_mock),
            any(target_os = "linux", target_os = "android")
        ))]
        {
            #[cfg(feature = "fault-injection")]
            if crate::fault_injection::should_fail_residency_sample() {
                return Err(VmemError::os_refusal_unknown_code());
            }
            let mut vec = [0u8; RESIDENCY_SAMPLE_VEC_ENTRIES];
            let mut resident: usize = 0;
            let mut sampled: usize = 0;
            for region in regions {
                let Some(start_abs) = self.as_ptr().addr().checked_add(region.offset) else {
                    return Err(VmemError::invalid_argument());
                };
                let Some(end_abs) = start_abs.checked_add(region.len) else {
                    return Err(VmemError::invalid_argument());
                };
                let Some(start_pg) = crate::os::align_up_addr(start_abs, ps) else {
                    return Err(VmemError::invalid_argument());
                };
                let end_pg = end_abs - end_abs % ps;
                if start_pg >= end_pg {
                    continue;
                }
                let delta_start = start_pg - self.as_ptr().addr();
                let span = end_pg - start_pg;
                let pages = span / ps; // <= RESIDENCY_SAMPLE_VEC_ENTRIES (bound proof above).
                let base = self.as_ptr();
                // SAFETY: `delta_start <= self.len()` (validated above), so
                // `base.add(delta_start)` stays inside this reservation's
                // live, exclusively owned usable span.
                let start = unsafe { base.add(delta_start) };
                // SAFETY: `span` is a multiple of the runtime page size and
                // `pages <= RESIDENCY_SAMPLE_VEC_ENTRIES` fits `vec`;
                // `start` is a live mapping address per the SAFETY comment
                // above; `mincore` only reads the kernel's page tables and
                // writes into our stack vector.
                let ret = unsafe {
                    super::mincore::mincore(
                        start.cast::<core::ffi::c_void>(),
                        span,
                        vec.as_mut_ptr(),
                    )
                };
                if ret != 0 {
                    return Err(VmemError::last_os_error());
                }
                for &entry in &vec[..pages] {
                    if entry & 1 == 1 {
                        resident += 1;
                    }
                }
                // Bound proof: sampled <= 16 * 512 and resident <= sampled;
                // plain `+=` cannot overflow.
                sampled += pages;
            }
            Ok(ResidencySample {
                resident_pages: resident,
                sampled_pages: sampled,
            })
        }
        #[cfg(not(all(
            unix,
            not(miri),
            not(aligned_vmem_mock),
            any(target_os = "linux", target_os = "android")
        )))]
        {
            Err(VmemError::os_refusal_unknown_code())
        }
    }
}

/// The outcome of
/// [`Reservation::try_sample_residency`](crate::Reservation::try_sample_residency):
/// base-page residency counts summed over every sampled region occurrence.
/// The estimator ratio is `resident_pages / sampled_pages` (a caller guards
/// the zero-denominator case; an all-empty sample yields
/// `sampled_pages == 0` legitimately).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidencySample {
    /// Base pages found resident (`mincore(2)` low bit set).
    pub resident_pages: usize,
    /// Base pages actually queried — the estimator's denominator.
    pub sampled_pages: usize,
}
