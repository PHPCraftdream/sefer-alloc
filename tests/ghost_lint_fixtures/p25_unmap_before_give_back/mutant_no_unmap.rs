// expect: is not preceded
pub(crate) fn release_large_mapping(charge: usize, reservation: *mut u8, reservation_len: usize) {
    #[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
    if charge != 0 {
        ThpRuntime::note_returned(charge);
        ThpRuntime::ledger().give_back(charge);
    }
    #[cfg(not(all(feature = "alloc-decommit", not(feature = "numa-aware"))))]
    let _ = charge;
}
