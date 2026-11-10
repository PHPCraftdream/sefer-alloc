//! `ThpTupleCell` — the process-wide, write-once publication cell of the
//! resolved large-THP tuple.
//!
//! One `compare_exchange(UNSET, PUBLISHING)` elects the single writer, which
//! stores the five packed words and then releases `PUBLISHED`; the words are
//! never written again, so a reader that observes `PUBLISHED` sees the
//! winner's whole tuple and never a mix of two. Nothing here loops: a loser
//! that meets `PUBLISHING` cannot read the words yet and reports a conflict
//! instead of waiting (a same-tuple race is therefore over-counted, which is
//! the conservative direction). A winner that never finishes leaves every
//! reader on the default tuple.

use crate::alloc_core::config::policy::large_thp_runtime::ThpTuple;
// Atomics through `super` so the actual-source concurrency model can bind.
use super::{AtomicU64, AtomicU8, Ordering};

pub(crate) const UNSET: u8 = 0;
const PUBLISHING: u8 = 1;
const PUBLISHED: u8 = 2;

/// Write-once cell holding a packed [`ThpTuple`]; see the module doc.
pub(crate) struct ThpTupleCell {
    pub(crate) state: AtomicU8,
    pub(crate) w: [AtomicU64; 5],
}

impl ThpTupleCell {
    /// An unset cell; readers see [`ThpTuple::DEFAULT`] until a tuple is published.
    // Not `const`: the modelable atomic alias has no const constructor, so the
    // process statics in `thp_runtime.rs` write their initializer directly.
    // In-crate statics in `thp_runtime.rs` initialize directly (see above);
    // `new` is consumed by the root test binaries, which mount this file by
    // path and are invisible to lib-target dead-code analysis.
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self {
            state: AtomicU8::new(UNSET),
            w: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
        }
    }

    /// Try to publish `t`. Returns `true` when the cell holds exactly `t`
    /// afterwards (this call won, or an equal tuple was already published) and
    /// `false` on a conflict: a different or undecodable published tuple, or a
    /// publication still in progress.
    pub(crate) fn publish(&self, t: &ThpTuple) -> bool {
        match self
            .state
            .compare_exchange(UNSET, PUBLISHING, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {
                let packed = t.pack();
                self.w[0].store(packed[0], Ordering::Relaxed);
                self.w[1].store(packed[1], Ordering::Relaxed);
                self.w[2].store(packed[2], Ordering::Relaxed);
                self.w[3].store(packed[3], Ordering::Relaxed);
                self.w[4].store(packed[4], Ordering::Relaxed);
                self.state.store(PUBLISHED, Ordering::Release);
                true
            }
            Err(PUBLISHED) => ThpTuple::try_unpack(self.load_words()) == Some(*t),
            Err(_) => false,
        }
    }

    /// The published tuple, or [`ThpTuple::DEFAULT`] while unset, still being
    /// published, or undecodable. One Acquire load.
    pub(crate) fn read(&self) -> ThpTuple {
        if self.state.load(Ordering::Acquire) == PUBLISHED {
            ThpTuple::unpack(self.load_words())
        } else {
            ThpTuple::DEFAULT
        }
    }

    fn load_words(&self) -> [u64; 5] {
        [
            self.w[0].load(Ordering::Relaxed),
            self.w[1].load(Ordering::Relaxed),
            self.w[2].load(Ordering::Relaxed),
            self.w[3].load(Ordering::Relaxed),
            self.w[4].load(Ordering::Relaxed),
        ]
    }
}
