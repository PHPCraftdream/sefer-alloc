//! C-prime-double (C'') reduced shadow (Loom 0.7.2): the discovery summary bit
//! is set ONLY on a pending word's 0 -> non-0 transition, and the owner clears
//! the summary word BEFORE cutting any of its pending words. Not the actual
//! SidecarBitmap; no reservation or RoutePin lifetime is modeled.
//!
//! Negative controls (mutants), each expected to fail this model:
//! 1. notify on every free instead of only the 0 -> non-0 transition
//!    (detected by an exact summary-RMW == transition-count oracle);
//! 2. owner clears the summary AFTER cutting pending words (lost publication);
//! 3. missing Acquire on the pending RMW lets the summary RMW be observed
//!    above the pending linearization point (modeled literally as summary
//!    before pending, the only observable effect Loom can explore: Loom steps
//!    operations in program order, so relaxed ordering must be modeled as its
//!    permitted observation). A Relaxed summary STORE by itself cannot lose
//!    the flag against coherent owner RMWs in this bounded model; the loss
//!    requires the hoist above the LP, which is what this control exhibits.
#![cfg(loom)]
#![forbid(unsafe_code)]

use loom::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Correct C'': summary only on 0 -> non-0, cleared before cuts.
    C2,
    /// Mutant 1: summary touched on every free.
    NotifyEveryFree,
    /// Mutant 2: owner clears the summary after cutting pending words.
    ClearAfterCut,
    /// Mutant 3: summary RMW hoisted above the pending LP (the observable
    /// effect of dropping the pending RMW's Acquire half).
    SummaryHoisted,
}

struct Sidecar {
    pending: [AtomicU64; 2],
    summary: AtomicU64,
    /// Shadow of production summary RMW traffic (oracle counters only).
    summary_rmw: AtomicUsize,
    /// Number of actual 0 -> non-0 pending transitions in this execution.
    transitions: AtomicUsize,
}

impl Sidecar {
    fn new() -> Self {
        Self {
            pending: [AtomicU64::new(0), AtomicU64::new(0)],
            summary: AtomicU64::new(0),
            summary_rmw: AtomicUsize::new(0),
            transitions: AtomicUsize::new(0),
        }
    }

    fn set_summary(&self, word: usize) {
        self.summary_rmw.fetch_add(1, Ordering::Relaxed);
        self.summary.fetch_or(1 << word, Ordering::Release);
    }

    fn publish(&self, id: usize, word: usize, mode: Mode) {
        if matches!(mode, Mode::SummaryHoisted) {
            self.set_summary(word);
        }
        let prev = self.pending[word].fetch_or(1 << id, Ordering::AcqRel);
        if prev == 0 {
            self.transitions.fetch_add(1, Ordering::Relaxed);
        }
        if matches!(mode, Mode::NotifyEveryFree)
            || (prev == 0 && !matches!(mode, Mode::SummaryHoisted))
        {
            self.set_summary(word);
        }
    }

    fn cut_word(&self, word: usize) -> u64 {
        self.pending[word].swap(0, Ordering::AcqRel)
    }

    fn discovery(&self, mode: Mode) -> u64 {
        let mut bits = 0;
        if matches!(mode, Mode::ClearAfterCut) {
            // Mutant: read, cut, then clear — clears late-set bits.
            let selected = self.summary.load(Ordering::Acquire);
            for word in 0..2 {
                if selected & (1 << word) != 0 {
                    bits |= self.cut_word(word);
                }
            }
            self.summary.store(0, Ordering::Release);
        } else {
            // Correct: clear the summary word BEFORE any of its pending cuts.
            let selected = self.summary.swap(0, Ordering::AcqRel);
            for word in 0..2 {
                if selected & (1 << word) != 0 {
                    bits |= self.cut_word(word);
                }
            }
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
        // C'' exactness: exactly one summary RMW per 0 -> non-0 transition.
        assert_eq!(
            sidecar.summary_rmw.load(Ordering::Relaxed),
            sidecar.transitions.load(Ordering::Relaxed),
            "summary RMW count diverged from 0 -> non-0 transitions"
        );
    });
}

#[test]
fn loom_c2_transition_two_words_no_loss() {
    two_producers(Mode::C2, false, "lost publication");
}

#[test]
fn loom_c2_transition_same_word_no_loss() {
    // Exercises the prev != 0 fast path: the second producer touches no summary.
    two_producers(Mode::C2, true, "lost publication");
}

#[test]
#[should_panic(expected = "summary RMW count diverged from 0 -> non-0 transitions")]
fn loom_c2_negative_notify_on_every_free() {
    two_producers(Mode::NotifyEveryFree, true, "lost publication");
}

#[test]
#[should_panic(expected = "clear after word cut lost publication")]
fn loom_c2_negative_clear_after_cut() {
    two_producers(
        Mode::ClearAfterCut,
        true,
        "clear after word cut lost publication",
    );
}

#[test]
#[should_panic(expected = "summary RMW count diverged from 0 -> non-0 transitions")]
fn loom_c2_negative_summary_hoisted_above_lp() {
    // Loom fails this mutant on the exactness oracle first: a hoisted summary
    // RMW means the summary is touched without a real transition. The same
    // hoist also loses publications (checked by the shared assertions); Loom
    // stops at the first failing execution it explores.
    two_producers(
        Mode::SummaryHoisted,
        true,
        "summary hoisted above pending LP lost publication",
    );
}
