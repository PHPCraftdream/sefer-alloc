//! Bounded, owner-only residency observation for fresh THP controls.

use aligned_vmem::ResidencyRegion;

use super::sample_owned_large_residency::sample_owned_large_residency;
use crate::alloc_core::config::policy::large_thp::LargeThpRequestGroup;
use crate::alloc_core::config::policy::large_thp_observation::large_thp_residency_plan::LargeThpResidencyPlan;
use crate::alloc_core::config::policy::large_thp_observation::large_thp_span_geometry::LargeThpSpanGeometry;
use crate::alloc_core::config::policy::large_thp_runtime::ThpRuntime;
use crate::alloc_core::segment_header::SegmentHeader;

const MAX_WINDOWS: usize = 16;
const OBSERVATION_SEED: u64 = 0x8a5c_1d37_e249_b60f;

/// Consume and, for a valid clean-control token, sample its owned Large
/// mapping. The caller must have consumed the owner's lifetime credit and
/// retain exclusive ownership before cache deposit or unmapping; this must
/// never be called for a foreign live mapping.
pub(crate) fn observe_large_control(base: *mut u8) {
    let token = SegmentHeader::thp_observation_at(base);
    if token == 0 {
        return;
    }
    // Consume before any validation or possible sampling failure: errors are
    // terminal for this first-life observation, never retried.
    SegmentHeader::set_thp_observation_at(base, 0);

    let header = SegmentHeader::read_at(base);
    let request_size = header.large_size;
    let Ok(request_bytes) = u64::try_from(request_size) else {
        ThpRuntime::note_observation_error(request_size);
        return;
    };
    let group: u64 = match LargeThpRequestGroup::of_bytes(request_bytes) {
        LargeThpRequestGroup::Under1MiB => 0,
        LargeThpRequestGroup::MiB1To8 => 1,
        LargeThpRequestGroup::MiB8To32 => 2,
        LargeThpRequestGroup::MiB32To128 => 3,
        LargeThpRequestGroup::MiB128To512 => 4,
        LargeThpRequestGroup::Over512MiB => 5,
    };
    if token == group + 9 {
        ThpRuntime::note_premise_failure(request_size);
        return;
    }
    if token != group + 1 {
        ThpRuntime::note_observation_error(request_size);
        return;
    }

    let bad_geometry = || ThpRuntime::note_observation_error(request_size);
    // Owner geometry: root/base pointer equality is NOT required. The
    // ordinary Unix backend legitimately over-reserves and aligns, so the
    // reservation root legitimately sits strictly below the SEGMENT-aligned
    // usable base (alignment slack). Bounded slack geometry plus root
    // page-alignment is the contract; the root/len pair only identifies the
    // underlying OS reservation — the sampling geometry and every region
    // offset below are usable-base-relative. The bounded owner-geometry
    // check below enforces bounding and alignment, fail closed.
    if header.large_align >= crate::alloc_core::os::SEGMENT || header.large_align == 0 {
        bad_geometry();
        return;
    }
    let page = aligned_vmem::page_size();

    if page == 0 || !page.is_power_of_two() {
        bad_geometry();
        return;
    }

    let (
        Ok(root_u64),
        Ok(root_len_u64),
        Ok(base_u64),
        Ok(span_u64),
        Ok(page_u64v),
        Ok(segment_u64),
    ) = (
        u64::try_from(header.reservation.addr()),
        u64::try_from(header.reservation_len),
        u64::try_from(base.addr()),
        u64::try_from(header.span_usable),
        u64::try_from(page),
        u64::try_from(crate::alloc_core::os::SEGMENT),
    )
    else {
        bad_geometry();
        return;
    };
    if LargeThpSpanGeometry::validate_owner_geometry(
        root_u64,
        root_len_u64,
        base_u64,
        span_u64,
        page_u64v,
        segment_u64,
    )
    .is_err()
    {
        bad_geometry();
        return;
    }

    // The trusted header records the actual payload offset computed by the
    // allocation path; use it rather than reconstructing the alignment math.
    let base_address = base.addr();
    let Some(payload_address) = base_address.checked_add(header.payload_offset) else {
        bad_geometry();
        return;
    };
    let Some(payload_end) = payload_address.checked_add(request_size) else {
        bad_geometry();
        return;
    };
    let Some(usable_end) = base_address.checked_add(header.span_usable) else {
        bad_geometry();
        return;
    };
    let Some(payload_end_offset) = header.payload_offset.checked_add(request_size) else {
        bad_geometry();
        return;
    };
    let reservation_address = header.reservation.addr();
    let Some(reservation_end) = reservation_address.checked_add(header.reservation_len) else {
        bad_geometry();
        return;
    };
    if payload_end_offset > header.span_usable
        || payload_end > usable_end
        || base_address < reservation_address
        || usable_end > reservation_end
    {
        bad_geometry();
        return;
    }

    // Only complete base pages within the requested payload are sampleable;
    // exclude both the leading and trailing partial pages.
    let Some(payload_ceil_input) = payload_address.checked_add(page - 1) else {
        bad_geometry();
        return;
    };
    let sample_start = payload_ceil_input & !(page - 1);
    let sample_end = payload_end - payload_end % page;
    if sample_start >= sample_end {
        bad_geometry();
        return;
    }
    let sample_bytes = sample_end - sample_start;
    let Ok(sample_start_u64) = u64::try_from(sample_start) else {
        bad_geometry();
        return;
    };
    let Ok(sample_bytes_u64) = u64::try_from(sample_bytes) else {
        bad_geometry();
        return;
    };
    let Ok(page_u64) = u64::try_from(page) else {
        bad_geometry();
        return;
    };
    let geometry = match LargeThpSpanGeometry::new(sample_start_u64, sample_bytes_u64, page_u64) {
        Ok(geometry) => geometry,
        Err(_) => {
            bad_geometry();
            return;
        }
    };
    if geometry.full_page_count() == 0 {
        bad_geometry();
        return;
    }

    let Ok(max_window_bytes) = usize::try_from(LargeThpResidencyPlan::MAX_WINDOW_BYTES) else {
        bad_geometry();
        return;
    };
    let window_bytes = sample_bytes.min(max_window_bytes);
    let window_bytes = window_bytes - window_bytes % page;
    let Ok(window_bytes_u64) = u64::try_from(window_bytes) else {
        bad_geometry();
        return;
    };
    if window_bytes == 0 {
        bad_geometry();
        return;
    }
    let plan = match LargeThpResidencyPlan::plan(
        &geometry,
        MAX_WINDOWS as u8,
        window_bytes_u64,
        token ^ OBSERVATION_SEED,
    ) {
        Ok(plan) if plan.placed() != 0 => plan,
        _ => {
            bad_geometry();
            return;
        }
    };

    let mut regions = [ResidencyRegion::new(0, 0); MAX_WINDOWS];
    let placed = usize::from(plan.placed());
    for (region, window) in regions.iter_mut().zip(plan.windows()) {
        let Ok(window_offset) = usize::try_from(window.byte_offset) else {
            bad_geometry();
            return;
        };
        let Some(window_address) = sample_start.checked_add(window_offset) else {
            bad_geometry();
            return;
        };
        let Ok(window_pages) = usize::try_from(window.page_count) else {
            bad_geometry();
            return;
        };
        let Some(window_len) = window_pages.checked_mul(page) else {
            bad_geometry();
            return;
        };
        let Some(window_end) = window_address.checked_add(window_len) else {
            bad_geometry();
            return;
        };
        // Region offsets are usable-base-relative: the sampler reconstructs
        // the reservation contract-faithfully from the usable start/span, so
        // offsets must be measured from the SEGMENT-aligned usable base.
        let Some(region_offset) = window_address.checked_sub(base_address) else {
            bad_geometry();
            return;
        };
        if window_end > sample_end || window_end > reservation_end {
            bad_geometry();
            return;
        }
        *region = ResidencyRegion::new(region_offset, window_len);
    }

    // Pass the untouched root pair plus the SEGMENT-aligned usable start/span;
    // the sampler reconstructs the reservation contract-faithfully, and the
    // region offsets above are usable-base-relative.
    let sample = match sample_owned_large_residency(
        header.reservation,
        header.reservation_len,
        base,
        header.span_usable,
        &regions[..placed],
    ) {
        Ok(sample) => sample,
        Err(_) => {
            bad_geometry();
            return;
        }
    };
    if sample.sampled_pages == 0 || sample.resident_pages > sample.sampled_pages {
        bad_geometry();
        return;
    }
    let Ok(resident_pages) = u64::try_from(sample.resident_pages) else {
        bad_geometry();
        return;
    };
    let Ok(sampled_pages) = u64::try_from(sample.sampled_pages) else {
        bad_geometry();
        return;
    };
    ThpRuntime::note_valid_observation(request_size, resident_pages, sampled_pages);
}
