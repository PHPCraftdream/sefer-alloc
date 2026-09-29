#![allow(unsafe_code)]

use super::*;
use std::alloc::{alloc, dealloc, Layout};
use std::sync::atomic::AtomicPtr;
use std::sync::{mpsc, Arc};

struct Reservation {
    base: *mut u8,
    layout: Layout,
}

impl Reservation {
    fn new() -> Self {
        let layout = Layout::from_size_align(SegmentLayout::SEGMENT, SegmentLayout::SEGMENT)
            .expect("segment geometry is a valid allocation layout");
        // SAFETY: valid nonzero Layout; the resulting reservation remains
        // alive until all scoped producer threads and owner reads complete.
        let base = unsafe { alloc(layout) };
        assert!(
            !base.is_null(),
            "segment test reservation allocation failed"
        );
        SegmentMeta::new(base).init_small_terminal();
        Self { base, layout }
    }

    fn block(&self, offset: usize) -> *mut u8 {
        self.base.with_addr(self.base.addr() + offset)
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // SAFETY: this is the original allocation pointer and exact layout;
        // tests join all producers and consume cursors before fixture drop.
        unsafe { dealloc(self.base, self.layout) };
    }
}

#[test]
fn minimum_block_and_last_block() {
    let reservation = Reservation::new();
    let first = SegmentLayout::SMALL_META_END;
    let last = SegmentLayout::SEGMENT - MIN_BLOCK;
    // SAFETY: both offsets designate distinct live physical 16-byte blocks
    // within this canonical reservation; the test grants exclusive frees.
    unsafe {
        RemoteInbox::prepare(reservation.base, reservation.block(first), 0)
            .expect("first payload block")
            .publish();
        RemoteInbox::prepare(reservation.base, reservation.block(last), 1)
            .expect("last payload block")
            .publish();
        let mut cut = RemoteInbox::detach(reservation.base);
        assert_eq!(
            cut.pop(),
            Some(InboxRecord {
                offset: last as u32,
                class: 1
            })
        );
        assert_eq!(
            cut.pop(),
            Some(InboxRecord {
                offset: first as u32,
                class: 0
            })
        );
        assert!(cut.is_empty());
        assert_eq!(cut.pop(), None);
    }
}

#[test]
fn paused_before_cas_is_not_in_cut_and_after_cas_is_reusable() {
    let reservation = Reservation::new();
    let offset = SegmentLayout::SMALL_META_END;
    let shared_base = Arc::new(AtomicPtr::new(reservation.base));
    let (ready_tx, ready_rx) = mpsc::channel();
    let (go_tx, go_rx) = mpsc::channel();
    let (published_tx, published_rx) = mpsc::channel();
    let (return_tx, return_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let worker_base = Arc::clone(&shared_base);
        let worker = scope.spawn(move || {
            let base = worker_base.load(Ordering::Relaxed);
            let block = base.with_addr(base.addr() + offset);
            // SAFETY: the fixture owns one issued physical block and pins its
            // reservation across the entire scoped producer operation.
            let private =
                unsafe { RemoteInbox::prepare(base, block, 2) }.expect("valid private block");
            ready_tx.send(()).expect("owner is receiving");
            go_rx.recv().expect("owner releases producer");
            private.publish();
            published_tx.send(()).expect("owner is receiving");
            return_rx.recv().expect("owner releases producer return");
        });
        ready_rx.recv().expect("producer prepared");
        // SAFETY: the scoped owner has the only consumer lease and the
        // reservation remains live. The unpublished block is not in this cut.
        unsafe { assert!(RemoteInbox::detach(reservation.base).is_empty()) };
        go_tx.send(()).expect("producer is waiting");
        published_rx.recv().expect("producer published");
        // SAFETY: same exclusive owner lease and live reservation. The CAS
        // has completed, so owner may consume and reissue before producer
        // returns; no producer access follows that CAS.
        unsafe {
            let mut cut = RemoteInbox::detach(reservation.base);
            assert_eq!(cut.pop().expect("published block").offset, offset as u32);
            assert!(cut.is_empty());
            RemoteInbox::prepare(reservation.base, reservation.block(offset), 3)
                .expect("same offset, new allocation instance")
                .publish();
            let mut reused = RemoteInbox::detach(reservation.base);
            assert_eq!(reused.pop().expect("reused block").class, 3);
        }
        return_tx.send(()).expect("producer is waiting");
        worker.join().expect("producer completed");
    });
}
