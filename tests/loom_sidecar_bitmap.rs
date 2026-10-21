//! Reduced shadow of pending-bit publication/cuts, NOT the real sidecar.
//! D1′ phase-1b: the shadow class map is gone — the only gate a producer
//! crosses is the owner's "start was issued" bit (the production
//! `SidecarBitmap::publish` start-bit Acquire load). Positive cases are
//! ordering-insensitive: one thread issues (sets the start bit) and the
//! publishes drain exactly once. Production orderings are pinned by
//! r11_ph5b_c5_sidecar_ordering_pinned; the actual-source runs live in
//! tests/loom_sidecar_bitmap_actual.rs.
#![cfg(loom)]

use loom::sync::atomic::{AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

struct Word {
    bits: AtomicU64,
    /// Shadow of the owner-written "start was issued" map (one word).
    starts: AtomicU64,
}

impl Word {
    fn new() -> Self {
        Self {
            bits: AtomicU64::new(0),
            starts: AtomicU64::new(0),
        }
    }

    /// Shadow of the owner issue: set the start bit (single-writer
    /// load → or → Release store; monotone).
    fn issue(&self, slot: usize) {
        let old = self.starts.load(Ordering::Acquire);
        self.starts.store(old | (1 << slot), Ordering::Release);
    }

    /// Shadow of the producer publication: the start-bit gate, then the
    /// terminal pending RMW — the same shape as
    /// `SidecarBitmap::publish`.
    fn publish(&self, slot: usize) -> bool {
        if self.starts.load(Ordering::Acquire) & (1 << slot) == 0 {
            return false;
        }
        self.bits.fetch_or(1 << slot, Ordering::AcqRel);
        true
    }

    /// Ungated mutant publication (skips the start-bit gate): what a
    /// producer WITHOUT the payload/start filter would do.
    fn ungated_publish(&self, slot: usize) {
        self.bits.fetch_or(1 << slot, Ordering::AcqRel);
    }

    fn start_bit(&self, slot: usize) -> bool {
        self.starts.load(Ordering::Acquire) & (1 << slot) != 0
    }

    fn cut(&self) -> u64 {
        if self.bits.load(Ordering::Acquire) == 0 {
            0
        } else {
            self.bits.swap(0, Ordering::AcqRel)
        }
    }
}

#[test]
fn loom_sidecar_publication_and_two_producers() {
    loom::model(|| {
        let word = Arc::new(Word::new());
        word.issue(0);
        word.issue(1);
        let a = Arc::clone(&word);
        let b = Arc::clone(&word);
        let pa = thread::spawn(move || a.publish(0));
        let pb = thread::spawn(move || b.publish(1));
        let first = word.cut();
        pa.join().unwrap();
        pb.join().unwrap();
        let second = word.cut();
        assert_eq!(first | second, 3);
        assert_eq!(first & second, 0);
    });
}

#[test]
fn loom_sidecar_paused_producer_and_reused_bit() {
    loom::model(|| {
        let word = Arc::new(Word::new());
        word.issue(0);
        let producer_word = Arc::clone(&word);
        let producer = thread::spawn(move || {
            thread::yield_now();
            producer_word.publish(0);
            thread::yield_now();
        });
        let first = word.cut();
        producer.join().unwrap();
        let second = word.cut();
        assert_eq!(first | second, 1);
        assert_eq!(first & second, 0);
        // Reissue from the free list: the start bit is monotone — the same
        // bit is stored back and the granule publishes exactly once more.
        word.issue(0);
        let next = Arc::clone(&word);
        let reused = thread::spawn(move || next.publish(0));
        assert!(reused.join().unwrap());
        assert_eq!(word.cut(), 1);
    });
}

/// NEGATIVE control (D1′ phase-1b): an ungated publication (a producer
/// without the start-bit filter) can plant a pending bit whose start bit is
/// zero — exactly the corruption the production start-bit gate makes
/// impossible. The owner's reclaim-side check (pending bit ⇒ start bit)
/// detects it.
#[test]
#[should_panic(expected = "pending bit without start bit")]
fn loom_sidecar_negative_ungated_publication_detected_by_start_check() {
    loom::model(|| {
        let word = Arc::new(Word::new());
        let producer_word = Arc::clone(&word);
        let producer = thread::spawn(move || {
            producer_word.ungated_publish(0);
        });
        producer.join().unwrap();
        let bits = word.cut();
        if bits != 0 && !word.start_bit(0) {
            panic!("pending bit without start bit");
        }
    });
}
