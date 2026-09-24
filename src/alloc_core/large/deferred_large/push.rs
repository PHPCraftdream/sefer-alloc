//! Multi-producer publication of a deferred Large reservation.

use core::sync::atomic::{AtomicPtr, Ordering};

use super::publishing::DEFERRED_LARGE_PUBLISHING;
use super::tail::DEFERRED_LARGE_TAIL;
use crate::alloc_core::segment_header::{SegmentMeta, ABANDONED_TAIL};

/// Queue a live Large segment on its owner's single-consumer deferred stack.
///
/// The caller supplies the live segment base and the stable head stamped in
/// its header. A successful claim owns this base's link word until publication.
/// A duplicate push before reclaim fails the claim and leaves the stack alone.
///
/// `swap` returns the actual predecessor pointer, including its allocation
/// provenance. An earlier load followed by an address-only CAS is insufficient:
/// the owner may pop and release that reservation, then a different reservation
/// may acquire the same virtual address before the CAS. The publishing marker
/// keeps the consumer off this node until its actual predecessor is recorded.
pub(crate) fn push_large_deferred_free(head: &AtomicPtr<u8>, base: *mut u8) {
    let next = SegmentMeta::new(base).deferred_next_atomic();
    if next
        .compare_exchange(
            ABANDONED_TAIL,
            DEFERRED_LARGE_PUBLISHING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return;
    }

    let predecessor = head.swap(base, Ordering::AcqRel);
    let link = if predecessor.is_null() {
        DEFERRED_LARGE_TAIL
    } else {
        // The returned pointer, not a pre-swap sample, is the reservation
        // still linked below `base`. Expose that allocation's provenance.
        predecessor.expose_provenance() as u64
    };
    next.store(link, Ordering::Release);
}
