//! Terminal release of a Large mapping and its mapping-owned advice charge.

use crate::alloc_core::os;
#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
use crate::alloc_core::policy::large_thp_runtime::ThpRuntime;

/// Caller transfers the live mapping exactly once, reading `charge` before
/// this call and never accessing mapping storage afterwards. Charged mappings
/// return budget only after confirmed native release. Failure or unknown
/// outcome consumes ownership and conservatively retains the charge; no retry.
/// Uncharged mappings retain the legacy release path and touch no THP ledger.
pub(crate) fn release_large_mapping(charge: usize, reservation: *mut u8, reservation_len: usize) {
    #[cfg(feature = "bench-internals")]
    let _capture_source =
        aligned_vmem::AdviceCapture::large_release_scope(reservation.addr(), reservation_len);
    if charge == 0 {
        os::release_segment(reservation, reservation_len);
        return;
    }
    #[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
    if os::try_release_segment(reservation, reservation_len).is_ok() {
        ThpRuntime::note_returned(charge);
        ThpRuntime::ledger().give_back(charge);
    }
    #[cfg(not(all(feature = "alloc-decommit", not(feature = "numa-aware"))))]
    os::release_segment(reservation, reservation_len);
}
