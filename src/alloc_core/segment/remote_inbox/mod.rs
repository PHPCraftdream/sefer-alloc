//! Stage 2A small/primordial remote-free inbox. Not yet a production route.
//!
//! The caller retains one outstanding credit per block from issue through
//! owner reclaim. In particular a paused unpublished producer pins the live
//! segment; a published or detached block also pins it until reclaimed.
//! Duplicate free/publication of one allocation instance is forbidden.

#![allow(unsafe_code)]

use core::mem::{align_of, size_of};
use core::ptr;
use core::sync::atomic::Ordering;

use crate::alloc_core::segment_header::{SegmentMeta, REMOTE_HEAD_EMPTY};
use crate::alloc_core::size_classes::{MIN_BLOCK, SMALL_CLASS_COUNT};

use super::segment_layout::SegmentLayout;

/// Offset zero belongs to metadata, not a block. The sentinel is outside
/// every segment-relative offset, including on a 32-bit target.
pub(crate) const EMPTY: u32 = REMOTE_HEAD_EMPTY;

#[repr(C)]
struct InboxNode {
    next_offset: u32,
    class: u32,
}

const _: () = {
    assert!(size_of::<InboxNode>() == 8);
    assert!(MIN_BLOCK >= size_of::<InboxNode>());
    assert!(MIN_BLOCK.is_power_of_two());
    assert!(SegmentLayout::SEGMENT < u32::MAX as usize);
    assert!(SegmentLayout::SEGMENT <= isize::MAX as usize);
    assert!(SegmentLayout::SMALL_META_END < SegmentLayout::SEGMENT);
    assert!(SegmentLayout::PRIMORDIAL_META_END < SegmentLayout::SEGMENT);
};

/// A private, not-yet-published block. Contains no reference into the segment.
/// It must be published exactly once; abandoning it strands its outstanding
/// lifetime credit and prevents safe segment release.
#[must_use = "publish the private node exactly once before releasing its segment"]
pub(crate) struct Prepared {
    base: *mut u8,
    node: *mut InboxNode,
    own_offset: u32,
}

/// A whole-chain cut. The owner must persist it outside the reservation if
/// consumption spans maintenance passes. No node is reclaimed by dropping it.
#[must_use = "consume or persist the detached chain before releasing its segment"]
pub(crate) struct Detached {
    base: *mut u8,
    next: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InboxRecord {
    pub offset: u32,
    pub class: u32,
}

pub(crate) struct RemoteInbox;

impl RemoteInbox {
    /// Prepare one block without publishing it. `class` is derived by the
    /// caller from the original `Layout`, never the mixed-class page map.
    ///
    /// # Safety
    /// `base` is the canonical base of a live Small/Primordial reservation,
    /// with provenance covering its full committed physical block. `block`
    /// names the start of a currently issued, uniquely freed allocation in
    /// that same reservation. Its full physical block is at least `MIN_BLOCK`
    /// writable bytes, even if requested size is smaller; the caller's
    /// pointer/provenance contract permits allocator writes to those bytes.
    /// No active shared or exclusive Rust borrow covers the node bytes.
    /// The owner cannot reclaim this block or release/decommit its page until
    /// it has been published and then consumed. The initialized remote head
    /// belongs to this reservation. Duplicate free is invalid.
    pub(crate) unsafe fn prepare(base: *mut u8, block: *mut u8, class: u32) -> Option<Prepared> {
        let base_addr = base.addr();
        let offset = block.addr().checked_sub(base_addr)?;
        if base.is_null()
            || offset < SegmentLayout::SMALL_META_END
            || offset > SegmentLayout::SEGMENT - MIN_BLOCK
            || offset % MIN_BLOCK != 0
            || class as usize >= SMALL_CLASS_COUNT
        {
            return None;
        }
        let node = base.with_addr(block.addr()).cast::<InboxNode>();
        if node.addr() % align_of::<InboxNode>() != 0 {
            return None;
        }
        // SAFETY: caller grants the full physical block and exclusive private
        // node bytes. `base.with_addr` preserves the canonical reservation's
        // provenance; checked offset/size/alignment keep this 8-byte write in
        // that block. This is before publication.
        unsafe { ptr::addr_of_mut!((*node).class).write(class) };
        Some(Prepared {
            base,
            node,
            own_offset: offset as u32,
        })
    }

    /// One AcqRel exchange cuts the entire prior chain. Requires the owner's
    /// external exclusive heap lease; it must retain the segment until the
    /// returned cursor is exhausted and all records are reclaimed.
    ///
    /// # Safety
    /// `base` is the canonical, live Small/Primordial reservation base with
    /// full provenance and initialized terminal word. The lease excludes a
    /// second consumer, but producers may prepend concurrently. All detached
    /// blocks remain committed until their records are consumed.
    pub(crate) unsafe fn detach(base: *mut u8) -> Detached {
        let meta = SegmentMeta::new(base);
        let head = meta.remote_head_atomic();
        let next = head.swap(EMPTY, Ordering::AcqRel);
        Detached { base, next }
    }
}

impl Prepared {
    /// Publish this private node. The successful strong CAS is the final
    /// producer access to the entire reservation, including its head word.
    /// No callback, owner wait, allocation, or post-CAS cleanup occurs here.
    pub(crate) fn publish(self) {
        let Prepared {
            base,
            node,
            own_offset,
        } = self;
        let meta = SegmentMeta::new(base);
        let head = meta.remote_head_atomic();
        let mut expected = head.load(Ordering::Acquire);
        loop {
            // SAFETY: `prepare` gave this producer exclusive ownership of
            // the private node. Every failed CAS retains that ownership;
            // the old head is a numeric offset, never dereferenced here.
            unsafe { ptr::addr_of_mut!((*node).next_offset).write(expected) };
            match head.compare_exchange(expected, own_offset, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return,
                Err(current) => expected = current,
            }
        }
    }
}

impl Detached {
    pub(crate) const fn is_empty(&self) -> bool {
        self.next == EMPTY
    }

    /// Copy successor and class to a value record before caller reclaim.
    /// At most the finite detached chain is traversed; concurrent producers
    /// only join the new head. The cursor has no callback or `Drop` work.
    ///
    /// # Safety
    /// The caller still holds the exclusive lease. `base` is live with full
    /// provenance; every pending node remains committed and has not been
    /// reclaimed/reused. After this returns, only the returned block may be
    /// reclaimed, not any future cursor node. No duplicate free occurred.
    pub(crate) unsafe fn pop(&mut self) -> Option<InboxRecord> {
        let offset = self.next;
        if offset == EMPTY {
            return None;
        }
        let addr = self.base.addr() + offset as usize;
        let node = self.base.with_addr(addr).cast::<InboxNode>();
        // SAFETY: the detached chain consists only of prepared/published
        // nodes in this still-live segment. Acquire swap observes their
        // class/next writes through the head's RMW release sequence. The
        // owner reads both fields before any reclaim of this node.
        let record = unsafe {
            InboxRecord {
                offset,
                class: ptr::addr_of!((*node).class).read(),
            }
        };
        // SAFETY: same valid, initialized node as the class read above.
        self.next = unsafe { ptr::addr_of!((*node).next_offset).read() };
        Some(record)
    }
}

#[cfg(test)]
mod tests;
