// expect: outside a branch matching
pub(crate) fn release_large_mapping(charge: usize, reservation: *mut u8, reservation_len: usize) {
    if charge == 0 {
        os::release_segment(reservation, reservation_len);
        return;
    }
    #[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
    if os::try_release_segment(reservation, reservation_len).is_err() {
        ThpRuntime::note_returned(charge);
        ThpRuntime::ledger().give_back(charge);
    }
    #[cfg(not(all(feature = "alloc-decommit", not(feature = "numa-aware"))))]
    os::release_segment(reservation, reservation_len);
}
