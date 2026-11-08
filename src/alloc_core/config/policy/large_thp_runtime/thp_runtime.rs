//! `ThpRuntime` — namespace over the process-wide large-THP statics: the
//! publication cell and the always-compiled conflict counter.

use crate::alloc_core::config::policy::large_thp_runtime::thp_budget_ledger::ThpBudgetLedger;
use crate::alloc_core::config::policy::large_thp_runtime::thp_tuple_cell::{ThpTupleCell, UNSET};
use crate::alloc_core::config::policy::large_thp_runtime::ThpTuple;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

// Direct initializers (not `new()`): statics need a const value and the
// modelable atomic alias has no const constructor.
static TUPLE: ThpTupleCell = ThpTupleCell {
    state: AtomicU8::new(UNSET),
    w: [const { AtomicU64::new(0) }; 5],
};

static LEDGER: ThpBudgetLedger = ThpBudgetLedger {
    used: AtomicU64::new(0),
};

/// Budget bytes charged to the ledger over the process lifetime.
static THP_CHARGED_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Budget bytes given back to the ledger over the process lifetime.
static THP_RETURNED_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Bytes the kernel accepted advice for, in every mode.
static THP_ADVISED_BYTES_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Publications that found a different tuple already published (or one still
/// being published). Backs `AllocStats::thp_tuple_conflicts`.
static THP_TUPLE_CONFLICTS: AtomicU64 = AtomicU64::new(0);

/// Process-wide large-THP tuple state; first publication wins.
pub(crate) struct ThpRuntime;

impl ThpRuntime {
    /// Publish `t`; returns `true` when the process tuple equals `t`
    /// afterwards, otherwise counts one conflict and returns `false`.
    pub(crate) fn publish(t: &ThpTuple) -> bool {
        let same = TUPLE.publish(t);
        if !same {
            THP_TUPLE_CONFLICTS.fetch_add(1, Ordering::Relaxed);
        }
        same
    }

    /// The tuple in force: the published one, else [`ThpTuple::DEFAULT`].
    pub(crate) fn current() -> ThpTuple {
        TUPLE.read()
    }

    // Callers: the AllocStats surface (alloc-global) and the bench-internals
    // dbg hook, both NUMA-out; the module itself only compiles under the
    // mechanism gate, so the attribute covers the remaining rows.
    #[cfg_attr(
        not(any(
            feature = "alloc-global",
            feature = "bench-internals",
            feature = "numa-aware"
        )),
        allow(dead_code)
    )]
    /// Conflicting publications so far.
    pub(crate) fn conflicts() -> u64 {
        THP_TUPLE_CONFLICTS.load(Ordering::Relaxed)
    }

    /// The process-wide budget ledger.
    pub(crate) fn ledger() -> &'static ThpBudgetLedger {
        &LEDGER
    }

    /// Record `bytes` charged to the ledger.
    pub(crate) fn note_charged(bytes: usize) {
        THP_CHARGED_TOTAL.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Record `bytes` given back to the ledger.
    pub(crate) fn note_returned(bytes: usize) {
        THP_RETURNED_TOTAL.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Record `bytes` the kernel accepted advice for.
    pub(crate) fn note_advised(bytes: usize) {
        THP_ADVISED_BYTES_TOTAL.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    // Production readers are only the bench-internals dbg total hooks; the
    // write side is live under the mechanism gate.
    #[cfg_attr(not(feature = "bench-internals"), allow(dead_code))]
    /// Total bytes charged.
    pub(crate) fn charged_total() -> u64 {
        THP_CHARGED_TOTAL.load(Ordering::Relaxed)
    }

    #[cfg_attr(not(feature = "bench-internals"), allow(dead_code))]
    /// Total bytes returned.
    pub(crate) fn returned_total() -> u64 {
        THP_RETURNED_TOTAL.load(Ordering::Relaxed)
    }

    #[cfg_attr(not(feature = "bench-internals"), allow(dead_code))]
    /// Total bytes the kernel accepted advice for.
    pub(crate) fn advised_bytes_total() -> u64 {
        THP_ADVISED_BYTES_TOTAL.load(Ordering::Relaxed)
    }
}
