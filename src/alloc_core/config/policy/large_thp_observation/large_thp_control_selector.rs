//! `LargeThpControlSelector` — the stateful deterministic control
//! selector of the J13b observation family: exactly one control per
//! complete, aligned cohort of 8 eligible fresh requests, with seeded
//! bounded decorrelation (`docs/perf/J13B_BOUNDED_THP_PREREG.md` §3/§4;
//! the 2026-11-06 amendment takes precedence over every rolling-window
//! or free-window implication of the older text).
//!
//! ## Contract
//!
//! - Eligible FRESH requests are fed one at a time, in arrival order,
//!   via [`classify_eligible_request`](LargeThpControlSelector::classify_eligible_request).
//!   Consecutive cohorts of [`COHORT_LEN`](LargeThpControlSelector::COHORT_LEN)
//!   (= 8) requests are aligned to the selector's own stream: its first
//!   eligible request opens cohort 0. Alignment is relative to selector
//!   construction, not wall-clock.
//! - For every complete, aligned cohort the selector draws ONE bounded
//!   decorrelated control index: `bounded_mix(effective_seed, cohort) %
//!   COHORT_LEN`. Exactly one request of the cohort — the one whose
//!   cohort-local index equals the drawn position — classifies as the
//!   control (`true`); every other request classifies `false`.
//! - HARD deterministic bound: at most one control per complete cohort,
//!   expected rate 1/8. NOT claimed: any hard cap over a sliding window
//!   of 8 requests — a window straddling a cohort boundary can contain
//!   2 controls (one from each adjacent cohort); the amendment makes
//!   this legal. Likewise there is NO cap over any 8 frees: frees are
//!   not an input of this type at all.
//! - Decisions happen AT REQUEST TIME and are immutable afterwards: the
//!   returned `bool` is a plain value the caller records. Free
//!   reordering — and any later processing — cannot change a
//!   classification, because there is no free input on this type.
//! - Share-agnostic: there is no share input and no learner. At share 0
//!   the same selector keeps drawing one control per cohort (the
//!   observation stream keeps flowing), and the draw is a pure function
//!   of `(seed, cohort)` — no self-confirmation feedback exists by
//!   construction.
//! - Identity-free: state is `(seed, pending_seed, requests_seen)`. No
//!   thread id, task id, or business-process identity exists here (the
//!   prereg: two workloads of one size bucket are indistinguishable).
//! - Reseed: [`schedule_reseed`](LargeThpControlSelector::schedule_reseed)
//!   arms a new seed that is consumed at the START of the next
//!   [`classify_eligible_request`](LargeThpControlSelector::classify_eligible_request)
//!   call (which opens the next cohort). The armed seed is still pending
//!   after the request that completes the in-flight cohort, and that
//!   cohort always completes under the seed it opened with, so a
//!   scheduled reseed can never break the one-control-per-cohort
//!   property.
//! - Totality: a zero seed is defined; all arithmetic wraps. The
//!   no-reseed stream repeats with period `2^64` REQUESTS (`2^61`
//!   cohorts) — a defined, deterministic repeat, pinned by the pure
//!   [`decision_at`](LargeThpControlSelector::decision_at) function,
//!   which is total over the whole `u64` ordinal domain including the
//!   wrap boundary (testable directly, without any loop); the type never
//!   panics for any input sequence.
//!
//! ## Caller obligations and limitations
//!
//! - The caller decides eligibility and freshness ("first life"). The
//!   selector does not know blocks, caches, or frees; feeding a request
//!   the caller considers non-fresh is a caller-side contract violation
//!   this pure type cannot detect.
//! - Bounded decorrelation is NOT a representativeness guarantee (prereg
//!   §3: stratified systematic selection is a bounded-cost +
//!   bounded-decorrelation mechanism, and the prereg's own words: "a
//!   bounded PRNG alone gives no per-window hard cap; this does").
//!
//! This module is a standalone pure component; runtime integration into
//! the allocator is a separate future task.

use super::bounded_mix::bounded_mix;

/// The stateful deterministic control selector.
///
/// The state is exactly `(seed, pending_seed, requests_seen)`; every
/// accessor is a pure read. Copy/Clone give snapshot semantics, not a
/// second live stream: two copies classify identically because the
/// decision is the pure [`decision_at`](Self::decision_at) function of
/// `(seed, request ordinal)`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LargeThpControlSelector {
    seed: u64,
    pending_seed: Option<u64>,
    requests_seen: u64,
}

impl LargeThpControlSelector {
    /// Cohort length: one control per 8 eligible fresh requests is the
    /// preregistered hard deterministic bound (expected rate 1/8).
    pub const COHORT_LEN: u64 = 8;

    /// A fresh selector whose first eligible fresh request opens cohort
    /// 0. A zero seed is a defined input.
    #[must_use]
    #[allow(dead_code)] // stateful surface is exercised only by the #[path]-included tests/j13b_observation_control_selector.rs; the runtime consumes only decision_at
    pub const fn new(seed: u64) -> Self {
        Self {
            seed,
            pending_seed: None,
            requests_seen: 0,
        }
    }

    /// TEST-ONLY bounded stream-position constructor, compiled solely
    /// under `cfg(test)`.
    ///
    /// This module is now declared from `config/policy/mod.rs` (the
    /// runtime consumes the pure `decision_at` surface), while this
    /// `cfg(test)` constructor and the `#[path]`-included integration
    /// tests still exercise the stateful surface. This constructor never
    /// exists in any runtime or library build. It exists to place the stream at an arbitrary
    /// ordinal (e.g. `u64::MAX - 7`) so the real `u64` counter wrap can
    /// be driven through
    /// [`classify_eligible_request`](Self::classify_eligible_request)
    /// with a handful of calls instead of a `2^64`-request loop. Not
    /// part of any stable runtime API.
    #[cfg(test)]
    #[must_use]
    pub const fn with_requests_seen(seed: u64, requests_seen: u64) -> Self {
        Self {
            seed,
            pending_seed: None,
            requests_seen,
        }
    }

    /// Classify the next eligible FRESH request and advance the stream.
    ///
    /// Returns `true` iff the request is its complete, aligned cohort's
    /// single control. A seed armed by
    /// [`schedule_reseed`](Self::schedule_reseed) is consumed at the
    /// START of this call when the call opens a new cohort, so the
    /// in-flight cohort always completes under the seed it opened with.
    /// The returned decision is a plain value — final and immutable once
    /// returned; there is no input on this type that could revoke it.
    /// Total: never panics; the request counter wraps at `2^64` (defined,
    /// deterministic repeat of the no-reseed stream).
    #[must_use]
    #[allow(dead_code)] // stateful surface is exercised only by the #[path]-included tests/j13b_observation_control_selector.rs; the runtime consumes only decision_at
    pub fn classify_eligible_request(&mut self) -> bool {
        let ordinal = self.requests_seen;
        if ordinal.is_multiple_of(Self::COHORT_LEN) {
            if let Some(seed) = self.pending_seed.take() {
                self.seed = seed;
            }
        }
        self.requests_seen = self.requests_seen.wrapping_add(1);
        Self::decision_at(self.seed, ordinal)
    }

    /// The pure no-reseed decision function: `true` iff the request at
    /// ordinal `requests_seen` is its cohort's single control under
    /// `seed`.
    ///
    /// The no-reseed stream repeats with period `2^64` requests (`2^61`
    /// cohorts); this function IS that stream's definition, so the wrap
    /// boundary is evaluated here directly — total over the whole `u64`
    /// ordinal domain, no loop needed to reach it.
    /// [`classify_eligible_request`](Self::classify_eligible_request)
    /// advances the counter and delegates to this function with the
    /// effective seed (a scheduled reseed changes the effective seed, so
    /// reseeded streams are not modeled by one `decision_at` call).
    #[must_use]
    pub const fn decision_at(seed: u64, requests_seen: u64) -> bool {
        let cohort = requests_seen / Self::COHORT_LEN;
        let local = requests_seen % Self::COHORT_LEN;
        local == bounded_mix(seed, cohort) % Self::COHORT_LEN
    }

    /// Arm `seed` to be consumed at the START of the next
    /// [`classify_eligible_request`](Self::classify_eligible_request)
    /// call that opens a new cohort. The in-flight cohort always
    /// completes under the seed it opened with, so a scheduled reseed can
    /// never break the one-control-per-cohort property. Calling this
    /// again before it is consumed overwrites the armed value.
    #[allow(dead_code)] // stateful surface is exercised only by the #[path]-included tests/j13b_observation_control_selector.rs; the runtime consumes only decision_at
    pub fn schedule_reseed(&mut self, seed: u64) {
        self.pending_seed = Some(seed);
    }

    /// Whether a reseed is armed but not yet consumed. The armed seed
    /// SURVIVES the request that completes its cohort and is consumed at
    /// the start of the next
    /// [`classify_eligible_request`](Self::classify_eligible_request)
    /// call (which opens the next cohort), so this reads `true` between
    /// the two calls even though the cohort boundary has been crossed.
    #[must_use]
    #[allow(dead_code)] // stateful surface is exercised only by the #[path]-included tests/j13b_observation_control_selector.rs; the runtime consumes only decision_at
    pub const fn has_pending_reseed(&self) -> bool {
        self.pending_seed.is_some()
    }

    /// The seed governing the CURRENT cohort; a scheduled reseed swaps
    /// this value only at a cohort boundary.
    #[must_use]
    #[allow(dead_code)] // stateful surface is exercised only by the #[path]-included tests/j13b_observation_control_selector.rs; the runtime consumes only decision_at
    pub const fn effective_seed(&self) -> u64 {
        self.seed
    }

    /// The cohort index the NEXT classified request lands in
    /// (`requests_seen / COHORT_LEN`).
    #[must_use]
    #[allow(dead_code)] // stateful surface is exercised only by the #[path]-included tests/j13b_observation_control_selector.rs; the runtime consumes only decision_at
    pub const fn cohort_index(&self) -> u64 {
        self.requests_seen / Self::COHORT_LEN
    }

    /// Eligible fresh requests classified so far.
    #[must_use]
    #[allow(dead_code)] // stateful surface is exercised only by the #[path]-included tests/j13b_observation_control_selector.rs; the runtime consumes only decision_at
    pub const fn requests_seen(&self) -> u64 {
        self.requests_seen
    }
}
