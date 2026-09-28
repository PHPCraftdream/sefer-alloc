use core::sync::atomic::{AtomicU32, Ordering};

use crate::alloc_core::{node::Node, os};

use super::HeapOverflow;

// One pending free owns its block until the consumer reclaims it. The node
// fits even the smallest class and needs no allocator/OS allocation.
#[repr(C)]
#[derive(Clone, Copy)]
struct SpillNode {
    next: *mut u8,
    packed: u32,
    ready: u32,
}

const _: () =
    assert!(core::mem::size_of::<SpillNode>() <= crate::alloc_core::size_classes::MIN_BLOCK);
const _: () =
    assert!(core::mem::align_of::<SpillNode>() <= crate::alloc_core::size_classes::MIN_BLOCK);
const SPILL_READY_OFF: usize = core::mem::offset_of!(SpillNode, ready);
const _: () = assert!(SPILL_READY_OFF.is_multiple_of(core::mem::align_of::<AtomicU32>()));

/// Abort rather than let an unwinding reclaim leave a popped note lost in a
/// still-running allocator. Reclaim may have already unmapped its node, so
/// requeueing after a panic cannot be made safe.
struct SpillReclaimGuard(bool);

impl Drop for SpillReclaimGuard {
    fn drop(&mut self) {
        if self.0 {
            std::process::abort();
        }
    }
}

impl HeapOverflow {
    /// Last-resort lossless publication. `block` is a valid small allocation
    /// being freed exactly once, with at least `MIN_BLOCK` writable bytes; it
    /// stays mapped until its note is reclaimed. A second publication of the
    /// same block violates this precondition; mappedness alone cannot make it
    /// safe. No allocation or blocking is required, even when the owner has
    /// exited or the OS is out of memory.
    ///
    /// `swap` returns the actual old pointer, including its provenance. The
    /// consumer cannot pop this node until its ready word is Release-stored,
    /// so the producer may safely finish `next` after the swap. No operation
    /// between swap and ready can unwind; an indefinitely descheduled
    /// producer can delay this stack's drain just like an unpublished ring
    /// reservation, but an exiting Rust thread cannot abandon this gap under
    /// the allocator's legal, non-panicking call contract.
    fn spill_ready_atomic(block: *mut u8) -> &'static AtomicU32 {
        let base = os::segment_base_of_ptr(block);
        let off = block.addr() - base.addr() + SPILL_READY_OFF;
        // The block starts at a MIN_BLOCK-aligned offset in a live segment;
        // the ready word is 4-aligned and in its first MIN_BLOCK bytes. A
        // pending note keeps that segment live through this atomic access.
        Node::atomic_u32_at(base, off)
    }

    pub(crate) fn spill_push(&self, block: *mut u8, packed: u32) {
        // `SizeClasses::class_for` selects a block >= MIN_BLOCK; `carve_block`
        // aligns each start to that block size, and recycled blocks retain
        // that alignment. A legal free owns it exclusively outside the owner
        // free lists until this note is consumed.
        Node::write_struct(
            block.cast::<SpillNode>(),
            SpillNode {
                next: core::ptr::null_mut(),
                packed,
                ready: 0,
            },
        );
        let previous = self.spill_head.swap(block, Ordering::AcqRel);
        // The swap's return is the actual preceding head, not a stale
        // address-equal CAS input. While ready is zero, the sole consumer
        // cannot pass this node and therefore cannot reclaim `previous`.
        Node::write_ptr_mut(block.cast::<*mut u8>(), previous);
        Self::spill_ready_atomic(block).store(1, Ordering::Release);
        #[cfg(feature = "internals")]
        self.spill_pushed.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(feature = "internals")]
    pub(crate) fn spill_pending_for_test(&self) -> bool {
        !self.spill_head.load(Ordering::Acquire).is_null()
    }

    #[cfg(feature = "internals")]
    pub(crate) fn spill_ledger_for_test(&self) -> (usize, usize) {
        (
            self.spill_pushed.load(Ordering::Relaxed),
            self.spill_popped.load(Ordering::Relaxed),
        )
    }

    /// Consume at most `budget` intrusive notes under the same exclusive
    /// token as the ring drain. A failed/reentrant token acquisition leaves
    /// every note pending. The consumer alone removes heads, so its Acquire
    /// load and node read remain valid until its own successful pop CAS;
    /// producers only prepend. An unready head stops this pass without
    /// changing the stack; its infallible producer completion unblocks a
    /// later pass. Each successful pop transfers one note to the
    /// callback, and the node is never touched after that callback, which may
    /// recycle the segment. A callback panic aborts the process: unlike ring
    /// slots, an intrusive node may already be unmapped, so unwinding and
    /// requeueing could continue with a lost or double-reclaimed note.
    pub(crate) fn try_drain_spill<F: FnMut(*mut u8, u32)>(
        &self,
        budget: usize,
        mut reclaim: F,
    ) -> Option<()> {
        let mut guard = self.begin_drain()?;
        // `begin_drain` seeds `h = 0`; preserve the ring cursor here so this
        // guard's Drop cannot roll a prior ring drain back to zero.
        guard.h = self.head.load(Ordering::Relaxed);
        for _ in 0..budget {
            let mut head = self.spill_head.load(Ordering::Acquire);
            loop {
                if head.is_null() {
                    return Some(());
                }
                // Only this token-holder pops. A producer may prepend, but
                // cannot mutate or reclaim the current head node.
                if Self::spill_ready_atomic(head).load(Ordering::Acquire) == 0 {
                    return Some(());
                }
                let node = Node::read_struct(head.cast::<SpillNode>());
                match self.spill_head.compare_exchange_weak(
                    head,
                    node.next,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        let mut reclaim_guard = SpillReclaimGuard(true);
                        #[cfg(feature = "internals")]
                        self.spill_popped.fetch_add(1, Ordering::Relaxed);
                        let base = os::segment_base_of_ptr(head);
                        reclaim(base, node.packed);
                        reclaim_guard.0 = false;
                        break;
                    }
                    Err(observed) => head = observed,
                }
            }
        }
        Some(())
    }
}
