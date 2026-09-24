//! Single-consumer pop and reclaim for deferred Large reservations.

use core::sync::atomic::{AtomicPtr, Ordering};

use super::publishing::DEFERRED_LARGE_PUBLISHING;
use super::tail::DEFERRED_LARGE_TAIL;
use crate::alloc_core::segment_header::SegmentMeta;
use crate::alloc_core::AllocCore;

/// Number of Large segments reclaimed through this path (diagnostic only).
#[doc(hidden)]
pub static DBG_LARGE_XTHREAD_RECLAIMED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// Drain the owner's deferred Large stack through its matching `AllocCore`.
///
/// Only the owner consumes this head. It must not read a predecessor while a
/// producer has swapped the node into the head but has not published its link.
/// In that case the owner leaves the node queued for a later drain rather than
/// blocking allocation on a paused remote producer.
/// On a successful pop, no producer can still refer to the popped reservation:
/// a push that displaced it has already published its link, while a push that
/// follows the pop receives the new head from its swap. The owner can then
/// release the OS mapping, including with a zero-byte Large cache budget.
pub(crate) fn drain_large_deferred_free(head: &AtomicPtr<u8>, core: &mut AllocCore) {
    loop {
        let cur = head.load(Ordering::Acquire);
        if cur.is_null() {
            return;
        }
        let meta = SegmentMeta::new(cur);
        let next_atomic = meta.deferred_next_atomic();
        // The Acquire head load observes a producer's Release swap (possibly
        // through later AcqRel swaps). Its preceding claim is therefore
        // visible: this cannot still be ABANDONED_TAIL. The link may remain
        // PUBLISHING until that producer writes the actual predecessor.
        let next_link = next_atomic.load(Ordering::Acquire);
        if next_link == DEFERRED_LARGE_PUBLISHING {
            return;
        }
        let next = if next_link == DEFERRED_LARGE_TAIL {
            core::ptr::null_mut()
        } else {
            // The producer exposed the pointer returned by the successful
            // swap, and the Acquire load above observes its Release store.
            core::ptr::with_exposed_provenance_mut::<u8>(next_link as usize)
        };
        if head
            .compare_exchange(cur, next, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            core.reclaim_large_segment(cur);
            DBG_LARGE_XTHREAD_RECLAIMED.fetch_add(1, Ordering::Relaxed);
        }
    }
}
