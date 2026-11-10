// expect: may do this
pub(crate) fn release_large_mapping(charge: usize, reservation: *mut u8, reservation_len: usize) {
    os::release_segment(reservation, reservation_len);
    #[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
    if charge != 0 {
        ThpRuntime::note_returned(charge);
        ThpRuntime::ledger().give_back(charge);
    }
    #[cfg(not(all(feature = "alloc-decommit", not(feature = "numa-aware"))))]
    let _ = charge;
}

fn evict(r: *mut u8, l: usize) {
    crate::alloc_core::os::release_segment(r, l);
}

impl AllocCore {
    fn free_large(&mut self, charge: usize, r: *mut u8, l: usize) {
        release_large_mapping(charge, r, l);
    }
}
