//! SC-6 A-exact remote-free ingress (`docs/perf/g1_scan/SC6_ORACLE.md`):
//! the per-route ARMED/NOTIFIED state a producer checks after its terminal
//! publication, and the per-heap cell it notifies. Atomics come from `super`
//! so the loom test runs this exact source.
//!
//! Handshake (store-buffering): producer `pending.fetch_or(SeqCst)` then
//! `state.load(SeqCst)`; owner `state.store(ARMED, Release)`, `fence(SeqCst)`,
//! then a full drain of `pending`. Either the drain sees the producer's bit,
//! or the producer sees ARMED and notifies the heap cell.

use super::{fence, AtomicU64, AtomicU8, Ordering};

const ARMED: u8 = 0;
const NOTIFIED: u8 = 1;

/// Words of one heap cell's slot mask: one bit per owner segment-table slot
/// (`MAX_SEGMENTS / 64`, asserted owner-side).
pub(crate) const INGRESS_WORDS: usize = 64;
/// Heap cells: one per registry slot (`MAX_HEAPS`, asserted owner-side).
pub(crate) const INGRESS_CELLS: usize = 4096;

/// Per-route state, owned by the route entry. A route without a heap cell is
/// born NOTIFIED and never notifies.
pub(crate) struct SegmentIngress {
    state: AtomicU8,
    slot: u32,
}

impl SegmentIngress {
    pub(crate) fn new(slot: u32, armed: bool) -> Self {
        Self {
            state: AtomicU8::new(if armed { ARMED } else { NOTIFIED }),
            slot,
        }
    }

    /// Producer half, after a successful terminal publication: notify the
    /// heap once per arming. `heap` is resolved only on the cold path.
    #[inline(always)]
    pub(crate) fn after_publish<'h>(&self, heap: impl FnOnce() -> Option<&'h HeapIngress>) {
        // loom models SeqCst accesses as AcqRel; the twin fence restores the
        // store-buffering order the C++ model gives the two SeqCst accesses.
        #[cfg(loom)]
        fence(Ordering::SeqCst);
        if self.state.load(Ordering::SeqCst) == ARMED {
            self.notify(heap());
        }
    }

    #[cold]
    #[inline(never)]
    fn notify(&self, heap: Option<&HeapIngress>) {
        if self
            .state
            .compare_exchange(ARMED, NOTIFIED, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        crate::ghost::ghost_event!("ingress/notified", self.slot as usize, 1);
        heap.unwrap_or_else(|| {
            crate::alloc_core::fatal::fatal(b"ingress: armed route without heap\n")
        })
        .notify(self.slot);
    }

    /// Owner half: re-arm before a full drain of this route's pending words.
    pub(crate) fn arm(&self) {
        self.state.store(ARMED, Ordering::Release);
        fence(Ordering::SeqCst);
    }

    /// Test oracle: whether a notification is outstanding for this route.
    #[cfg_attr(not(loom), allow(dead_code))]
    pub(crate) fn is_notified(&self) -> bool {
        self.state.load(Ordering::Relaxed) == NOTIFIED
    }
}

/// Per-heap notification cell: `mask` bit `s` = owner slot `s` was notified;
/// `summary` bit `w` = `mask[w]` may be non-zero.
#[repr(C, align(64))]
pub(crate) struct HeapIngress {
    summary: AtomicU64,
    mask: [AtomicU64; INGRESS_WORDS],
}

impl HeapIngress {
    #[cfg(not(loom))]
    pub(crate) const fn new() -> Self {
        Self {
            summary: AtomicU64::new(0),
            mask: [const { AtomicU64::new(0) }; INGRESS_WORDS],
        }
    }

    #[cfg(loom)]
    pub(crate) fn new() -> Self {
        Self {
            summary: AtomicU64::new(0),
            mask: core::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// The cell of registry slot `owner`; the fallback heap and unbound
    /// owners (ids >= `INGRESS_CELLS`) have none.
    #[cfg(not(loom))]
    pub(crate) fn of(owner: usize) -> Option<&'static Self> {
        static CELLS: [HeapIngress; INGRESS_CELLS] = [const { HeapIngress::new() }; INGRESS_CELLS];
        CELLS.get(owner)
    }

    /// Loom harnesses register heapless routes (born NOTIFIED).
    #[cfg(loom)]
    pub(crate) fn of(_owner: usize) -> Option<&'static Self> {
        None
    }

    fn notify(&self, slot: u32) {
        let s = slot as usize;
        self.mask[s / 64].fetch_or(1 << (s % 64), Ordering::Relaxed);
        self.summary.fetch_or(1 << (s / 64), Ordering::Release);
    }

    /// Owner only: take the notified slots, word by word.
    #[cfg_attr(feature = "numa-aware", allow(dead_code))]
    pub(crate) fn take(&self, mut each: impl FnMut(usize)) {
        if self.summary.load(Ordering::Relaxed) == 0 {
            return;
        }
        let mut words = self.summary.swap(0, Ordering::Acquire);
        while words != 0 {
            let w = words.trailing_zeros() as usize;
            words &= words - 1;
            let mut bits = self.mask[w].swap(0, Ordering::Acquire);
            while bits != 0 {
                let j = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                crate::ghost::ghost_event!("ingress/taken", w * 64 + j, 1);
                each(w * 64 + j);
            }
        }
    }
}
