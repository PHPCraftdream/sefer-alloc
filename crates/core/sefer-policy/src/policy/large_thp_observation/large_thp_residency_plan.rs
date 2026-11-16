//! `LargeThpResidencyPlan` — the pure bounded residency sampling planner:
//! takes validated live-span geometry plus a bounded sample request and
//! produces up to [`MAX_WINDOWS`](LargeThpResidencyPlan::MAX_WINDOWS)
//! (= 16) disjoint, page-aligned window locations, each at most
//! [`MAX_WINDOW_BYTES`](LargeThpResidencyPlan::MAX_WINDOW_BYTES)
//! (= 2 MiB), for the FUTURE owning `mincore` walk.
//!
//! ## Stratified placement contract (no rejection loops)
//!
//! The span's full page range `[0, full_page_count)` is divided into
//! `placed` contiguous strata of equal page count (floor-division
//! boundaries; strata lengths differ by at most one page). Each stratum
//! hosts exactly ONE window: its start page is
//! `stratum_start + bounded_mix(seed, stratum) % (stratum_pages -
//! window_pages + 1)`. Because `placed <= full_page_count /
//! window_pages`, every stratum is at least `window_pages` long, so the
//! window always fits inside its own stratum: every window is in-bounds
//! and pairwise page-disjoint BY CONSTRUCTION, offsets are strictly
//! increasing, and the planner does O(placed) bounded work with NO PRNG
//! rejection loop. When `full_page_count` is an exact multiple of
//! `window_pages` and `placed == full_page_count / window_pages`
//! (maximal packing), every stratum is exactly `window_pages` long and
//! the windows tile the WHOLE full-page range exactly, independent of
//! the seed. When `full_page_count` is NOT a multiple of
//! `window_pages`, the remainder pages are distributed ACROSS the
//! strata (floor-division boundaries `floor(full_page_count * i /
//! placed) .. floor(full_page_count * (i + 1) / placed)`), not
//! appended to the last one alone; the windows stay disjoint and
//! in-bounds, but the unsampled gaps are NOT obliged to sit at the
//! span tail — exactly `placed * window_pages` pages are sampled and
//! exact whole-span tiling remains impossible.
//!
//! ## Explicit rejections and shrink-to-fit
//!
//! Invalid requests are rejected with an explicit
//! [`LargeThpObservationError`] variant — never silently clamped:
//! `EmptySample` (requested 0), `WindowCapExceeded` (requested >
//! `MAX_WINDOWS`), `ZeroWindow`, `WindowTooLarge` (>
//! `MAX_WINDOW_BYTES`), `WindowNotPageAligned`, `WindowExceedsSpan`. The
//! span's trailing partial page never participates: sampling is bounded
//! by the geometry's `full_page_count`. When the span cannot hold as
//! many disjoint windows as requested, `placed` reports the smaller
//! count explicitly (shrink-to-fit is observable, never out-of-range).
//! Huge `u64`/`usize` inputs are handled in `u128` intermediates with
//! checked narrowing; the defensive `InternalRange` arm is unreachable
//! by construction and exists so the planner fails closed instead of
//! silently wrapping.
//!
//! ## The future owning mincore contract (NOT implemented here)
//!
//! A future integration walks the plan's windows with a bounded
//! `mincore` primitive and reports the metric as RESIDENT BASE pages
//! divided by SAMPLED BASE pages, summed over the plan's windows — NOT
//! a region-level "regions with any resident page" fraction, which
//! reads ~1 even on unhinted controls under one-byte-per-region fills
//! (prereg §4). This module performs NO sampling: no pinning, no
//! proc/sysfs reads, no live or foreign-mapping access, no syscall —
//! it computes locations only.
//!
//! ## Limitations (named, not solved)
//!
//! - Stratified decorrelation is NOT a representativeness guarantee:
//!   contiguous windows correlate neighboring pages, so clustered fill
//!   or reclaim patterns are observed as clustered.
//! - A plan is a single spatial snapshot; there is no time dimension
//!   and no repeated sampling.
//! - Kernel reclaim/swap bias has UNKNOWN direction (prereg §4 defect
//!   3): residency observed here is an estimate with bias, never an
//!   exact record of program behavior.
//! - Bounded sparsity: at most 16 windows of at most 2 MiB sample a
//!   possibly much larger span.
//! - Offsets are span-relative; the geometry VALIDATES a page-aligned
//!   base address and a representable absolute range end at
//!   construction, so every window is absolutely page-aligned by
//!   construction — see
//!   [`LargeThpSpanGeometry::checked_absolute_address`].
//!
//! This module is a standalone pure component; runtime integration into
//! the allocator is a separate future task.

use super::bounded_mix::bounded_mix;
use super::large_thp_observation_error::LargeThpObservationError;
use super::large_thp_residency_window::LargeThpResidencyWindow;
use super::large_thp_span_geometry::LargeThpSpanGeometry;

/// A computed, bounded residency sampling plan.
///
/// Stack-only by construction: a fixed-size window array plus a placed
/// count; no allocation, no `Drop`, total to compare (`PartialEq`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LargeThpResidencyPlan {
    windows: [LargeThpResidencyWindow; Self::MAX_WINDOWS],
    placed: u8,
}

impl LargeThpResidencyPlan {
    /// Hard cap on windows per plan: the preregistered `K = 16` sampled
    /// regions.
    pub const MAX_WINDOWS: usize = 16;
    /// Hard cap on one window's byte length (the 2 MiB huge-region
    /// size).
    pub const MAX_WINDOW_BYTES: u64 = 2 * 1024 * 1024;

    /// Plan the bounded residency sampling locations for `geometry`.
    ///
    /// `requested_windows` (1..=16), `window_bytes` (a positive
    /// page-aligned multiple of the geometry's base page size, at most
    /// [`MAX_WINDOW_BYTES`](Self::MAX_WINDOW_BYTES)), and `seed` (any
    /// `u64`, including 0) fully determine the plan: same inputs, same
    /// plan. Total: never panics; every invalid request is an explicit
    /// `LargeThpObservationError`.
    pub fn plan(
        geometry: &LargeThpSpanGeometry,
        requested_windows: u8,
        window_bytes: u64,
        seed: u64,
    ) -> Result<Self, LargeThpObservationError> {
        if requested_windows == 0 {
            return Err(LargeThpObservationError::EmptySample);
        }
        if usize::from(requested_windows) > Self::MAX_WINDOWS {
            return Err(LargeThpObservationError::WindowCapExceeded);
        }
        if window_bytes == 0 {
            return Err(LargeThpObservationError::ZeroWindow);
        }
        if window_bytes > Self::MAX_WINDOW_BYTES {
            return Err(LargeThpObservationError::WindowTooLarge);
        }
        let page = geometry.base_page_size();
        if !window_bytes.is_multiple_of(page) {
            return Err(LargeThpObservationError::WindowNotPageAligned);
        }
        let pages = geometry.full_page_count();
        let window_pages = window_bytes / page;
        if window_pages > pages {
            return Err(LargeThpObservationError::WindowExceedsSpan);
        }
        // Explicit shrink-to-fit: place no more disjoint windows than
        // the span can hold. The result is <= 16 by the cap check, so
        // the narrowing below is safe.
        let fit = pages / window_pages;
        let placed_u64 = u64::from(requested_windows).min(fit);
        let placed =
            usize::try_from(placed_u64).map_err(|_| LargeThpObservationError::InternalRange)?;
        let pages_u = u128::from(pages);
        let k = placed as u128;
        let mut windows = [LargeThpResidencyWindow {
            byte_offset: 0,
            start_page_index: 0,
            page_count: 0,
        }; Self::MAX_WINDOWS];
        for (i, window) in windows.iter_mut().enumerate().take(placed) {
            // Stratum [lo, hi) in whole pages. `floor(pages / k) >=
            // window_pages` holds because `k <= pages / window_pages`,
            // so the window always fits inside its own stratum.
            let lo = u64::try_from(pages_u * i as u128 / k)
                .map_err(|_| LargeThpObservationError::InternalRange)?;
            let hi = u64::try_from(pages_u * (i as u128 + 1) / k)
                .map_err(|_| LargeThpObservationError::InternalRange)?;
            let stratum_pages = hi - lo;
            let slack = stratum_pages - window_pages + 1;
            let draw = bounded_mix(seed, i as u64) % slack;
            let start_page_index = lo + draw;
            let offset_u = u128::from(start_page_index) * u128::from(page);
            let byte_offset =
                u64::try_from(offset_u).map_err(|_| LargeThpObservationError::InternalRange)?;
            let page_count =
                u32::try_from(window_pages).map_err(|_| LargeThpObservationError::InternalRange)?;
            *window = LargeThpResidencyWindow {
                byte_offset,
                start_page_index,
                page_count,
            };
        }
        let placed_u8 =
            u8::try_from(placed).map_err(|_| LargeThpObservationError::InternalRange)?;
        Ok(Self {
            windows,
            placed: placed_u8,
        })
    }

    /// The number of windows actually placed (may be below the
    /// requested count — explicit shrink-to-fit; never above
    /// [`MAX_WINDOWS`](Self::MAX_WINDOWS)).
    #[must_use]
    pub const fn placed(&self) -> u8 {
        self.placed
    }

    /// The placed windows, in strictly increasing `byte_offset` order.
    /// The slice length always equals
    /// `usize::from(self.placed())`.
    #[must_use]
    pub fn windows(&self) -> &[LargeThpResidencyWindow] {
        &self.windows[..usize::from(self.placed)]
    }
}
