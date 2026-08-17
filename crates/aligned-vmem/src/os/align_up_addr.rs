/// Round `addr` up to the next multiple of `align` (a power of two).
/// Returns `None` on overflow instead of wrapping.
#[cfg(not(miri))]
pub(crate) fn align_up_addr(addr: usize, align: usize) -> Option<usize> {
    debug_assert!(align.is_power_of_two());
    let mask = align - 1;
    addr.checked_add(mask).map(|sum| sum & !mask)
}
