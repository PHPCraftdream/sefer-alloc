pub(crate) fn release_large_mapping(charge: usize, reservation: *mut u8, reservation_len: usize) {
    let _ = os::try_release_segment(reservation, reservation_len);
    #[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
    if charge != 0 {
        ThpRuntime::note_returned(charge);
        ThpRuntime::ledger().give_back(charge);
    }
    #[cfg(not(all(feature = "alloc-decommit", not(feature = "numa-aware"))))]
    let _ = charge;
}
