//! Reduced-geometry ordering model of SidecarBitmap's exact atomic operations.
#![cfg(loom)]

use loom::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use loom::sync::Arc;
use loom::thread;

struct Word {
    bits: AtomicU64,
    classes: [AtomicU8; 2],
}

impl Word {
    fn new() -> Self {
        Self {
            bits: AtomicU64::new(0),
            classes: [AtomicU8::new(0), AtomicU8::new(0)],
        }
    }

    fn issue(&self, slot: usize, class: u8) {
        self.classes[slot].store(class + 1, Ordering::Release);
    }

    fn publish(&self, slot: usize) {
        self.bits.fetch_or(1 << slot, Ordering::AcqRel);
    }

    fn cut(&self) -> u64 {
        if self.bits.load(Ordering::Acquire) == 0 {
            0
        } else {
            self.bits.swap(0, Ordering::AcqRel)
        }
    }

    fn class(&self, slot: usize) -> u8 {
        self.classes[slot].load(Ordering::Relaxed) - 1
    }
}

#[test]
fn loom_sidecar_class_publication_and_two_producers() {
    loom::model(|| {
        let word = Arc::new(Word::new());
        word.issue(0, 3);
        word.issue(1, 4);
        let a = Arc::clone(&word);
        let b = Arc::clone(&word);
        let pa = thread::spawn(move || a.publish(0));
        let pb = thread::spawn(move || b.publish(1));
        let first = word.cut();
        if first & 1 != 0 {
            assert_eq!(word.class(0), 3);
        }
        if first & 2 != 0 {
            assert_eq!(word.class(1), 4);
        }
        pa.join().unwrap();
        pb.join().unwrap();
        let second = word.cut();
        assert_eq!(first | second, 3);
        assert_eq!(first & second, 0);
        if second & 1 != 0 {
            assert_eq!(word.class(0), 3);
        }
        if second & 2 != 0 {
            assert_eq!(word.class(1), 4);
        }
    });
}

#[test]
fn loom_sidecar_paused_producer_and_reused_bit() {
    loom::model(|| {
        let word = Arc::new(Word::new());
        word.issue(0, 2);
        let producer_word = Arc::clone(&word);
        let producer = thread::spawn(move || {
            thread::yield_now();
            producer_word.publish(0);
            thread::yield_now();
        });
        let first = word.cut();
        if first != 0 {
            assert_eq!(word.class(0), 2);
        }
        producer.join().unwrap();
        let second = word.cut();
        assert_eq!(first | second, 1);
        assert_eq!(first & second, 0);
        if second != 0 {
            assert_eq!(word.class(0), 2);
        }
        word.issue(0, 5);
        let next = Arc::clone(&word);
        let reused = thread::spawn(move || next.publish(0));
        reused.join().unwrap();
        assert_eq!(word.cut(), 1);
        assert_eq!(word.class(0), 5);
    });
}

#[test]
#[should_panic(expected = "class published before bit")]
fn loom_sidecar_negative_class_after_terminal_bit() {
    loom::model(|| {
        let word = Arc::new(Word::new());
        let producer_word = Arc::clone(&word);
        let producer = thread::spawn(move || {
            producer_word.publish(0);
            thread::yield_now();
            producer_word.issue(0, 2);
        });
        if word.cut() != 0 {
            assert_ne!(
                word.classes[0].load(Ordering::Relaxed),
                0,
                "class published before bit"
            );
        }
        producer.join().unwrap();
    });
}
