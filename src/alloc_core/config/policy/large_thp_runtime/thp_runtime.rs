//! `ThpRuntime` — namespace over the process-wide large-THP statics: the
//! publication cell and the always-compiled conflict counter.

use crate::alloc_core::config::policy::large_thp::large_thp_request_group::LargeThpRequestGroup;
use crate::alloc_core::config::policy::large_thp::{LargeThpGradient, LargeThpShare};
use crate::alloc_core::config::policy::large_thp_observation::large_thp_control_selector::LargeThpControlSelector;
use crate::alloc_core::config::policy::large_thp_runtime::thp_budget_ledger::ThpBudgetLedger;
use crate::alloc_core::config::policy::large_thp_runtime::thp_learning_cell::ThpLearningCell;
use crate::alloc_core::config::policy::large_thp_runtime::thp_tuple_cell::{ThpTupleCell, UNSET};
use crate::alloc_core::config::policy::large_thp_runtime::ThpTuple;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

const CONTROL_GROUP_COUNT: usize = 6;
const CONTROL_SEED: u64 = 0x4a13_b7d2_96c5_e80f;

// Direct initializers (not `new()`): statics need a const value and the
// modelable atomic alias has no const constructor.
static TUPLE: ThpTupleCell = ThpTupleCell {
    state: AtomicU8::new(UNSET),
    w: [const { AtomicU64::new(0) }; 5],
};

static LEDGER: ThpBudgetLedger = ThpBudgetLedger {
    used: AtomicU64::new(0),
};

// Per-request-size-group state; each snapshot/update covers both components.
static LEARNERS: [ThpLearningCell; CONTROL_GROUP_COUNT] = [const {
    ThpLearningCell {
        state: AtomicU64::new(ThpLearningCell::INITIAL),
    }
}; CONTROL_GROUP_COUNT];
// Independent wrapping diagnostics, never estimator inputs or admission gates.
static LEARNING_APPLIED: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];
static LEARNING_DROPPED: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];

/// Ordinal for process-wide deterministic control selection.
static CONTROL_ORDINAL: AtomicU64 = AtomicU64::new(0);

// Diagnostic counters only: fetch_add wraps modulo 2^64 and never gates
// allocator behavior. Group order follows LargeThpRequestGroup's declaration.
static CONTROL_SELECTIONS: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];
static CONTROL_PREMISE_FAILURES: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];
static CONTROL_VALID_OBSERVATIONS: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];
static CONTROL_INVALIDATED_OBSERVATIONS: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];
static CONTROL_INVALIDATED_REALLOC: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];
static CONTROL_OBSERVATION_ERRORS: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];
static CONTROL_RESIDENT_PAGES: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];
static CONTROL_SAMPLED_PAGES: [AtomicU64; CONTROL_GROUP_COUNT] =
    [const { AtomicU64::new(0) }; CONTROL_GROUP_COUNT];

/// Bench-internals diagnostic ticket log: records the `(ordinal, decision)`
/// pair actually consumed by each `next_control` call so tests can observe
/// real tickets instead of inferring them from counter deltas. Bounded and
/// fail-closed: entries at or past the cap are dropped, but the snapshot's
/// raw cursor still counts them, and a top-truncated ticket is stored as
/// the explicit invalid marker 0, so unsupported capture is
/// machine-detectable and rejected by the diagnostic hook instead of
/// masquerading as usable evidence. Diagnostic only — it never gates
/// allocator behavior, publishes nothing, and allocates nothing.
#[cfg(feature = "bench-internals")]
const TICKET_LOG_CAP: usize = 128;
#[cfg(feature = "bench-internals")]
static CONTROL_TICKET_CURSOR: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
static CONTROL_TICKET_LOG: [AtomicU64; TICKET_LOG_CAP] =
    [const { AtomicU64::new(0) }; TICKET_LOG_CAP];

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

    /// Select a deterministic process-wide control for this request size.
    /// Exactly one relaxed ordinal increment occurs per call; wrapping is
    /// defined by the atomic counter and the selector's total `u64` domain.
    #[allow(dead_code)]
    pub(crate) fn next_control(request_size: usize) -> (bool, u64) {
        let ordinal = CONTROL_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let selected = LargeThpControlSelector::decision_at(CONTROL_SEED, ordinal);
        if selected {
            note_group_counter(&CONTROL_SELECTIONS, group_index(request_size), 1);
        }
        #[cfg(feature = "bench-internals")]
        record_control_ticket(ordinal, selected);
        (selected, ordinal)
    }

    /// Record that an observation's preconditions were not met.
    #[allow(dead_code)]
    pub(crate) fn note_premise_failure(request_size: usize) {
        note_group_counter(&CONTROL_PREMISE_FAILURES, group_index(request_size), 1);
    }

    /// Record a valid observation and its resident/sample page totals.
    #[allow(dead_code)]
    pub(crate) fn note_valid_observation(
        request_size: usize,
        resident_pages: u64,
        sampled_pages: u64,
    ) {
        let Some(observation) = ThpLearningCell::observation(resident_pages, sampled_pages) else {
            Self::note_observation_error(request_size);
            return;
        };
        let group = group_index(request_size);
        note_group_counter(&CONTROL_VALID_OBSERVATIONS, group, 1);
        note_group_counter(&CONTROL_RESIDENT_PAGES, group, resident_pages);
        note_group_counter(&CONTROL_SAMPLED_PAGES, group, sampled_pages);
        let applied = LEARNERS[group].try_observe(&Self::current().gradient, observation);
        if applied {
            note_group_counter(&LEARNING_APPLIED, group, 1);
        } else {
            note_group_counter(&LEARNING_DROPPED, group, 1);
        }
    }

    /// Current share for one request group, from a coherent estimator pair.
    /// The caller supplies the same resolved gradient as its advice tuple.
    pub(crate) fn learned_share(request_size: usize, gradient: &LargeThpGradient) -> LargeThpShare {
        LEARNERS[group_index(request_size)]
            .snapshot()
            .bounded_share(gradient)
    }

    /// Per-group e/m/share from one coherent pair, followed by independent
    /// applied/dropped diagnostic loads. The whole array is not a snapshot;
    /// closure checks require quiescence after joining workers.
    #[cfg(feature = "bench-internals")]
    pub(crate) fn learning_telemetry() -> [[u64; 5]; CONTROL_GROUP_COUNT] {
        let gradient = Self::current().gradient;
        let mut result = [[0; 5]; CONTROL_GROUP_COUNT];
        for group in 0..CONTROL_GROUP_COUNT {
            let estimator = LEARNERS[group].snapshot();
            result[group] = [
                u64::from(estimator.e_raw()),
                u64::from(estimator.m_raw()),
                u64::from(estimator.bounded_share(&gradient).raw()),
                LEARNING_APPLIED[group].load(Ordering::Relaxed),
                LEARNING_DROPPED[group].load(Ordering::Relaxed),
            ];
        }
        result
    }

    /// Record an observation invalidated after its preconditions were met and
    /// then consumed by a remote reclaim (the REMOTE-RECLAIM consumption).
    #[allow(dead_code)]
    pub(crate) fn note_invalidated_observation(request_size: usize) {
        note_group_counter(
            &CONTROL_INVALIDATED_OBSERVATIONS,
            group_index(request_size),
            1,
        );
    }

    /// Record that a reallocation consumed an invalidated observation.
    ///
    /// Token-derived attribution is separate from requested-byte
    /// classification: `1..=6` maps directly to the invalidated-realloc
    /// group at `token - 1`; `9..=14` maps a premise-failed control consumed
    /// by a realloc before any free to the premise-failures group at
    /// `token - 9`, accounted as a premise failure exactly once. An unknown
    /// token carries no trustworthy group and fails closed: the anomaly is
    /// attributed via [`Self::note_observation_error`] to the recorded
    /// request size of the reallocated mapping, never to a token-derived
    /// group and never out of bounds.
    #[allow(dead_code)]
    pub(crate) fn note_realloc_invalidated_by_token(token: u64, request_size: usize) {
        if (1..=6).contains(&token) {
            // The token encodes group + 1 (success) or group + 9 (failed premise);
            // subtracting the offset yields the original group index directly.
            note_group_counter(&CONTROL_INVALIDATED_REALLOC, (token - 1) as usize, 1);
        } else if (9..=14).contains(&token) {
            note_group_counter(&CONTROL_PREMISE_FAILURES, (token - 9) as usize, 1);
        } else {
            Self::note_observation_error(request_size);
        }
    }

    /// Record an observation error.
    #[allow(dead_code)]
    pub(crate) fn note_observation_error(request_size: usize) {
        note_group_counter(&CONTROL_OBSERVATION_ERRORS, group_index(request_size), 1);
    }

    /// Per-group diagnostics in `[group][field]` order: selections, premise
    /// failures, valid observations, invalidated remote-reclaim, invalidated
    /// realloc, errors, resident pages, sampled pages.
    #[cfg(feature = "bench-internals")]
    const CONTROL_TELEMETRY_FIELD_COUNT: usize = 8;

    /// Per-group diagnostics in the field order above.
    #[cfg(feature = "bench-internals")]
    pub(crate) fn control_telemetry(
    ) -> [[u64; Self::CONTROL_TELEMETRY_FIELD_COUNT]; CONTROL_GROUP_COUNT] {
        let mut result = [[0; Self::CONTROL_TELEMETRY_FIELD_COUNT]; CONTROL_GROUP_COUNT];
        for group in 0..CONTROL_GROUP_COUNT {
            result[group] = [
                CONTROL_SELECTIONS[group].load(Ordering::Relaxed),
                CONTROL_PREMISE_FAILURES[group].load(Ordering::Relaxed),
                CONTROL_VALID_OBSERVATIONS[group].load(Ordering::Relaxed),
                CONTROL_INVALIDATED_OBSERVATIONS[group].load(Ordering::Relaxed),
                CONTROL_INVALIDATED_REALLOC[group].load(Ordering::Relaxed),
                CONTROL_OBSERVATION_ERRORS[group].load(Ordering::Relaxed),
                CONTROL_RESIDENT_PAGES[group].load(Ordering::Relaxed),
                CONTROL_SAMPLED_PAGES[group].load(Ordering::Relaxed),
            ];
        }
        result
    }

    /// Bench-internals diagnostic probe: the current process-global control
    /// ordinal (one Relaxed load, no mutation). Boundary/totality check
    /// only: ticket uniqueness and per-cohort placement are established
    /// from the ticket log's actual `(ordinal, decision)` records, never
    /// from the difference of two probe reads.
    #[cfg(feature = "bench-internals")]
    pub(crate) fn control_ordinal_probe() -> u64 {
        CONTROL_ORDINAL.load(Ordering::Relaxed)
    }

    /// Pure diagnostic passthrough: the runtime selector's decision for a raw
    /// ordinal. No atomics, no state; lets tests census the per-cohort
    /// decisions of an observed ordinal window.
    #[cfg(feature = "bench-internals")]
    pub(crate) fn control_decision_for_ordinal(ordinal: u64) -> bool {
        LargeThpControlSelector::decision_at(CONTROL_SEED, ordinal)
    }

    /// Reset the bench-internals diagnostic ticket-log cursor (one Relaxed
    /// store, NOT synchronized with in-flight writers): call it only while
    /// no control decisions are in flight, e.g. before the scenario's
    /// threads spawn. Entries are not cleared; the new cursor simply
    /// orphans the older records below it.
    #[cfg(feature = "bench-internals")]
    pub(crate) fn ticket_log_reset() {
        CONTROL_TICKET_CURSOR.store(0, Ordering::Relaxed);
    }

    /// Snapshot the bench-internals diagnostic ticket log: the raw packed
    /// entries (`ordinal << 2 | selected << 1 | valid`; 0 = explicit invalid
    /// marker for a top-truncated ticket) plus the RAW write cursor,
    /// deliberately uncapped — a cursor above the log length means records
    /// were dropped past the cap. The loads are unsynchronized Relaxed
    /// reads, so a coherent per-scenario window requires reset before the
    /// scenario's threads spawn and snapshot after they join. Reject
    /// unsupported capture with `ticket_log_capture_supported` (the
    /// diagnostic hook does) before treating the window as usable evidence.
    #[cfg(feature = "bench-internals")]
    pub(crate) fn ticket_log_snapshot() -> ([u64; TICKET_LOG_CAP], u64) {
        let mut entries = [0u64; TICKET_LOG_CAP];
        for (slot, entry) in CONTROL_TICKET_LOG.iter().zip(entries.iter_mut()) {
            *entry = slot.load(Ordering::Relaxed);
        }
        (entries, CONTROL_TICKET_CURSOR.load(Ordering::Relaxed))
    }

    /// Whether a snapshot is fully supported capture: the raw cursor is
    /// within the cap (no dropped records) and every consumed entry carries
    /// the valid bit (no top-truncated ticket). Unsupported capture is
    /// rejected at the diagnostic boundary instead of masquerading as
    /// complete usable evidence.
    #[cfg(feature = "bench-internals")]
    pub(crate) fn ticket_log_capture_supported(
        entries: &[u64; TICKET_LOG_CAP],
        cursor: u64,
    ) -> bool {
        if cursor > TICKET_LOG_CAP as u64 {
            return false;
        }
        entries[..cursor as usize]
            .iter()
            .all(|entry| entry & 1 == 1)
    }

    /// Pack one diagnostic ticket record exactly as `record_control_ticket`
    /// stores it: `ordinal << 2 | selected << 1 | valid` for ordinals below
    /// 2^62 (lossless, valid bit set); any other ordinal is the explicit
    /// invalid marker 0. Pure value function — the diagnostic hooks expose
    /// it so tests bind to the production packing instead of duplicating it.
    #[cfg(feature = "bench-internals")]
    pub(crate) fn ticket_log_pack(ordinal: u64, selected: bool) -> u64 {
        if ordinal < (1u64 << 62) {
            (ordinal << 2) | (u64::from(selected) << 1) | 1
        } else {
            0
        }
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

/// Bump `counters[group]`. The helper takes a VALIDATED group index; byte-
/// size classification happens at the callers via [`group_index`]. Fail
/// closed: diagnostics never panic and never index out of bounds.
fn note_group_counter(counters: &[AtomicU64; CONTROL_GROUP_COUNT], group: usize, amount: u64) {
    debug_assert!(group < CONTROL_GROUP_COUNT);
    // Fail closed: diagnostics never panic and never index out of bounds.
    if group < CONTROL_GROUP_COUNT {
        counters[group].fetch_add(amount, Ordering::Relaxed);
    }
}

fn group_index(request_size: usize) -> usize {
    match LargeThpRequestGroup::of_bytes(request_size as u64) {
        LargeThpRequestGroup::Under1MiB => 0,
        LargeThpRequestGroup::MiB1To8 => 1,
        LargeThpRequestGroup::MiB8To32 => 2,
        LargeThpRequestGroup::MiB32To128 => 3,
        LargeThpRequestGroup::MiB128To512 => 4,
        LargeThpRequestGroup::Over512MiB => 5,
    }
}

/// Record one consumed control ticket into the bench-internals diagnostic
/// log. Exactly one Relaxed cursor fetch_add and one Relaxed slot store per
/// call; the stored value is [`ThpRuntime::ticket_log_pack`] — the
/// fail-closed packing whose valid bit makes a top-truncated ticket
/// machine-detectable and rejected by the diagnostic hook instead of
/// masquerading as usable evidence. Never gates allocator behavior.
#[cfg(feature = "bench-internals")]
fn record_control_ticket(ordinal: u64, selected: bool) {
    let slot = CONTROL_TICKET_CURSOR.fetch_add(1, Ordering::Relaxed);
    if slot < TICKET_LOG_CAP as u64 {
        CONTROL_TICKET_LOG[slot as usize].store(
            ThpRuntime::ticket_log_pack(ordinal, selected),
            Ordering::Relaxed,
        );
    }
}
