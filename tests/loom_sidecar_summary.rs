//! PRF-01 C-prime reduced shadow (Loom 0.7.2): two pending words, one summary.
//! Not the actual SidecarBitmap/ClassLeaves or a lifetime/provenance proof.
//! Arc keeps the shadow sidecar alive; production must retain RoutePin and
//! obtain the summary address before the pending RMW. No reservation is modeled.
//! Oracles follow owner decision Question 2: discovery must recover without
//! strict assistance; strict must include a completed pending publication even
//! while its producer is paused before the discovery-only notification.
#![cfg(loom)]
#![forbid(unsafe_code)]

use loom::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use loom::sync::{mpsc, Arc};
use loom::thread;

#[derive(Clone, Copy)]
enum Mode {
    Conditional,
    SummaryBeforePending,
    ClearAfterCut,
    NeverSetCleanSummary,
}

struct Sidecar {
    pending: [AtomicU64; 2],
    summary: AtomicU64,
    classes: [AtomicU8; 2],
}

impl Sidecar {
    fn new() -> Self {
        Self {
            pending: [AtomicU64::new(0), AtomicU64::new(0)],
            summary: AtomicU64::new(0),
            classes: [AtomicU8::new(0), AtomicU8::new(0)],
        }
    }

    fn notify(&self, word: usize, mode: Mode) {
        let bit = 1 << word;
        if self.summary.load(Ordering::Relaxed) & bit == 0
            && !matches!(mode, Mode::NeverSetCleanSummary)
        {
            self.summary.fetch_or(bit, Ordering::Release);
        }
    }

    fn publish(&self, id: usize, word: usize, mode: Mode) {
        // Shadow of owner issue + synchronized allocation handoff to producer.
        // Write inside the producer so the owner's pre-join cut must acquire
        // publication to see the class; issue on the owner would mask this.
        self.classes[id].store((id + 3) as u8, Ordering::Release);
        if matches!(mode, Mode::SummaryBeforePending) {
            self.notify(word, mode);
        }
        self.pending[word].fetch_or(1 << id, Ordering::AcqRel);
        if !matches!(mode, Mode::SummaryBeforePending) {
            self.notify(word, mode);
        }
    }

    fn cut_word(&self, word: usize) -> u64 {
        let bits = self.pending[word].swap(0, Ordering::AcqRel);
        for id in 0..2 {
            if bits & (1 << id) != 0 {
                assert_eq!(
                    self.classes[id].load(Ordering::Relaxed),
                    (id + 3) as u8,
                    "class visibility after pending cut"
                );
            }
        }
        bits
    }

    fn discovery(&self, mode: Mode) -> u64 {
        let selected = if matches!(mode, Mode::ClearAfterCut) {
            self.summary.load(Ordering::Acquire)
        } else {
            self.summary.swap(0, Ordering::AcqRel)
        };
        let mut bits = 0;
        for word in 0..2 {
            if selected & (1 << word) != 0 {
                bits |= self.cut_word(word);
            }
        }
        if matches!(mode, Mode::ClearAfterCut) {
            self.summary.store(0, Ordering::Release);
        }
        bits
    }

    fn strict(&self) -> u64 {
        self.cut_word(0) | self.cut_word(1)
    }
}

fn two_producers(mode: Mode, same_word: bool, loss: &'static str) {
    loom::model(move || {
        let sidecar = Arc::new(Sidecar::new());
        if matches!(mode, Mode::ClearAfterCut) {
            // Benign stale hints force actual word cuts before the bad clear.
            // A producer publishing after its W cut must survive in next S.
            sidecar.summary.store(3, Ordering::Release);
        }
        let a = Arc::clone(&sidecar);
        let b = Arc::clone(&sidecar);
        let pa = thread::spawn(move || a.publish(0, 0, mode));
        let pb = thread::spawn(move || b.publish(1, usize::from(!same_word), mode));
        let first = sidecar.discovery(mode);
        pa.join().unwrap();
        pb.join().unwrap();
        let second = sidecar.discovery(mode);
        // Must precede strict: a final full scan cannot conceal lost discovery.
        assert_eq!(first | second, 3, "{loss}");
        assert_eq!(first & second, 0, "publication consumed twice");
        assert_eq!(sidecar.strict(), 0, "strict recovered discovery loss");
    });
}

#[test]
fn loom_summary_conditional_two_words_no_loss() {
    two_producers(Mode::Conditional, false, "lost publication");
}

#[test]
fn loom_summary_conditional_same_word_no_loss() {
    // Exercises the conditional-load fast path and shared W release sequence.
    two_producers(Mode::Conditional, true, "lost publication");
}

fn paused_pending(strict_uses_summary: bool) {
    loom::model(move || {
        let sidecar = Arc::new(Sidecar::new());
        let producer_sidecar = Arc::clone(&sidecar);
        let (published_tx, published_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let producer = thread::spawn(move || {
            producer_sidecar.classes[0].store(3, Ordering::Release);
            producer_sidecar.pending[0].fetch_or(1, Ordering::AcqRel);
            published_tx.send(()).unwrap();
            resume_rx.recv().unwrap();
            producer_sidecar.notify(0, Mode::Conditional);
        });
        // Intentional HB: pending publication completed BEFORE strict entry.
        // Producer cannot notify until trim has returned; no sleeps or polling.
        published_rx.recv().unwrap();
        assert_eq!(sidecar.discovery(Mode::Conditional), 0);
        let strict = if strict_uses_summary {
            sidecar.discovery(Mode::Conditional)
        } else {
            sidecar.strict()
        };
        // Resume/join before the oracle so a failing control leaves no waiter.
        resume_tx.send(()).unwrap();
        producer.join().unwrap();
        assert_eq!(strict, 1, "strict missed completed pending publication");
        assert_eq!(sidecar.discovery(Mode::Conditional), 0);
        assert_eq!(sidecar.strict(), 0, "publication consumed twice");
    });
}

#[test]
fn loom_summary_pending_pause_strict_inclusion() {
    paused_pending(false);
}

#[test]
fn loom_summary_spurious_bit_benign() {
    loom::model(|| {
        let sidecar = Sidecar::new();
        sidecar.classes[0].store(3, Ordering::Release);
        sidecar.pending[0].fetch_or(1, Ordering::AcqRel);
        assert_eq!(sidecar.strict(), 1);
        // Late notification after strict consumed the bit is only a false hint.
        sidecar.notify(0, Mode::Conditional);
        assert_eq!(sidecar.summary.load(Ordering::Relaxed), 1);
        assert_eq!(sidecar.discovery(Mode::Conditional), 0);
        assert_eq!(sidecar.discovery(Mode::Conditional), 0);
        assert_eq!(sidecar.strict(), 0);
    });
}

#[test]
#[should_panic(expected = "summary before pending lost publication")]
fn loom_summary_negative_summary_before_pending() {
    two_producers(
        Mode::SummaryBeforePending,
        false,
        "summary before pending lost publication",
    );
}

#[test]
#[should_panic(expected = "clear after word cut lost publication")]
fn loom_summary_negative_clear_after_word_cut() {
    two_producers(
        Mode::ClearAfterCut,
        true,
        "clear after word cut lost publication",
    );
}

#[test]
#[should_panic(expected = "strict missed completed pending publication")]
fn loom_summary_negative_strict_uses_summary() {
    paused_pending(true);
}

#[test]
#[should_panic(expected = "producer never sets clean summary lost publication")]
fn loom_summary_negative_producer_never_sets_clean_summary() {
    two_producers(
        Mode::NeverSetCleanSummary,
        false,
        "producer never sets clean summary lost publication",
    );
}
