//! Pure capacity arithmetic for `LockFreeRegion::with_pages`, kept apart so
//! tests can probe the boundary without allocating a page table.

/// Total slot count for `page_count` pages of `page_len` slots, or `None` if
/// it does not fit in `u32`.
///
/// The bound is `<= u32::MAX`: the free-list threading only compares
/// `global + 1` against the total, so the total itself must fit `u32`.
pub(crate) fn checked_total_slots(page_count: usize, page_len: usize) -> Option<u32> {
    let total = u64::try_from(page_count)
        .ok()?
        .checked_mul(u64::try_from(page_len).ok()?)?;
    u32::try_from(total).ok()
}
