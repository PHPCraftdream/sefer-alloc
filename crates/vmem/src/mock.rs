//! Recording mock + fault injection for `aligned-vmem` (feature `mock`).
//!
//! Mirrors [`numa-shim`](https://crates.io/crates/numa-shim)'s proven
//! recording-mock pattern: a thread-local call log plus scripted failures, so
//! any consumer can deterministically test its OOM-handling on any target
//! (including macOS and miri) WITHOUT exhausting real commit charge.
//!
//! When the `mock` feature is on:
//! - reservation entry points still chain to the real `std::alloc`/OS backend
//!   (so the returned [`crate::Reservation`] is genuinely usable), but record a
//!   [`Call`] and honour a scripted [`fail_next_reserve`] first;
//! - decommit / recommit / commit_range record a [`Call`] and honour
//!   [`fail_next_commit`] WITHOUT touching the OS.
//!
//! ```text
//! aligned_vmem::mock::fail_next_commit(1);
//! // SAFETY: `base` is a live reservation.
//! let ok = unsafe { aligned_vmem::recommit(base, 0, PAGE) };
//! assert!(!ok);
//! assert_eq!(aligned_vmem::mock::drain().len(), 1);
//! ```
//!
//! Runnable form: `tests/mock.rs`.
//!
//! # Cargo feature-unification hazard (task #715)
//!
//! `mock` is a NON-ADDITIVE, backend-REPLACING feature, and Cargo unifies
//! features across a build's WHOLE dependency graph, not per edge. If ANY
//! crate anywhere downstream of your build enables `aligned-vmem/mock`
//! (including a sibling workspace member's own `[dev-dependencies]`), every
//! OTHER consumer in that SAME build silently gets the mock backend too —
//! with no compile error and no warning. **Enable this feature only in a
//! leaf test/dev target — never in a library's own `[dependencies]`, and
//! never in a shared `[dev-dependencies]` entry reachable from more than one
//! build target in the same workspace.** See the `mock` feature's own doc
//! comment in `Cargo.toml` for the full reasoning and the stronger
//! alternative (a `--cfg` flag instead of a Cargo feature) considered and
//! deferred for this crate's first publish.

use core::cell::RefCell;

use crate::error::VmemError;

/// One recorded invocation of a public `aligned-vmem` function under the mock.
///
/// task #715 (rust-intel audit MEDIUM §C1a): every struct-like variant below
/// ALSO carries its own `#[non_exhaustive]` (the enum-level one above only
/// reserves the right to add whole VARIANTS — adding a FIELD to an existing
/// variant is still semver-major for every downstream `Call::Reserve { size,
/// align }` match without the variant-level marker too; `ReserveLazy` already
/// grew `initial_commit` after `Reserve`/`ReserveHuge` were designed, so this
/// is not a hypothetical). Decided now, before this crate's first publish
/// (task #659) — adding it retroactively after publish would itself be the
/// breaking change this is meant to prevent.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// [`crate::try_reserve_aligned`] / [`crate::reserve_aligned`].
    #[non_exhaustive]
    Reserve {
        /// Requested reservation size in bytes.
        size: usize,
        /// Requested alignment in bytes.
        align: usize,
    },
    /// [`crate::reserve_aligned_lazy`] (feature `lazy-commit`).
    #[non_exhaustive]
    ReserveLazy {
        /// Requested reservation size in bytes.
        size: usize,
        /// Requested alignment in bytes.
        align: usize,
        /// Bytes committed up front.
        initial_commit: usize,
    },
    /// [`crate::reserve_aligned_huge`] (feature `huge-pages`).
    #[non_exhaustive]
    ReserveHuge {
        /// Requested reservation size in bytes.
        size: usize,
        /// Requested alignment in bytes.
        align: usize,
    },
    /// [`crate::release`] (from `into_parts` + manual release).
    #[non_exhaustive]
    Release {
        /// Reservation base address, as `usize`.
        reservation: usize,
        /// Reservation length in bytes.
        reservation_len: usize,
    },
    /// [`crate::decommit`].
    #[non_exhaustive]
    Decommit {
        /// Span base address, as `usize`.
        base: usize,
        /// Start offset in bytes.
        start: usize,
        /// End offset in bytes.
        end: usize,
    },
    /// [`crate::decommit_lazy`].
    #[non_exhaustive]
    DecommitLazy {
        /// Span base address, as `usize`.
        base: usize,
        /// Start offset in bytes.
        start: usize,
        /// End offset in bytes.
        end: usize,
    },
    /// [`crate::recommit`].
    #[non_exhaustive]
    Recommit {
        /// Span base address, as `usize`.
        base: usize,
        /// Start offset in bytes.
        start: usize,
        /// End offset in bytes.
        end: usize,
    },
    /// [`crate::commit_range`] (feature `lazy-commit`).
    #[non_exhaustive]
    CommitRange {
        /// Span base address, as `usize`.
        base: usize,
        /// Start offset in bytes.
        start: usize,
        /// End offset in bytes.
        end: usize,
    },
}

std::thread_local! {
    /// Calls recorded since the last [`drain`].
    static CALLS: RefCell<Vec<Call>> = const { RefCell::new(Vec::new()) };
    /// Remaining scripted reserve failures ([`fail_next_reserve`]).
    static RESERVE_FAILS: RefCell<u32> = const { RefCell::new(0) };
    /// Remaining scripted commit failures ([`fail_next_commit`]).
    static COMMIT_FAILS: RefCell<u32> = const { RefCell::new(0) };
}

/// Drain and return every recorded [`Call`] since the last drain (or test
/// start). Clears the log.
#[must_use]
pub fn drain() -> Vec<Call> {
    CALLS.with(|c| c.borrow_mut().drain(..).collect())
}

/// Clear the recorded call log AND both fault counters — call at the start of a
/// test to isolate it from any residue on the current thread.
pub fn reset() {
    CALLS.with(|c| c.borrow_mut().clear());
    RESERVE_FAILS.with(|c| *c.borrow_mut() = 0);
    COMMIT_FAILS.with(|c| *c.borrow_mut() = 0);
}

/// Arm the reserve fault injector: the next `n` reservation attempts
/// ([`crate::try_reserve_aligned`] and its `lazy`/`huge` variants) return
/// `Err(VmemError::last_os_error())` without allocating. `n == 0` disarms.
pub fn fail_next_reserve(n: u32) {
    RESERVE_FAILS.with(|c| *c.borrow_mut() = n);
}

/// Arm the commit fault injector: the next `n` commit attempts
/// ([`crate::recommit`] / [`crate::commit_range`]) return failure without
/// touching the OS, simulating commit-charge exhaustion. `n == 0` disarms.
pub fn fail_next_commit(n: u32) {
    COMMIT_FAILS.with(|c| *c.borrow_mut() = n);
}

/// Internal: record a call into the thread-local log.
pub(crate) fn record(call: Call) {
    CALLS.with(|c| c.borrow_mut().push(call));
}

/// Internal: consume one armed reserve fault, returning the error to raise.
pub(crate) fn take_reserve_fault() -> Option<VmemError> {
    RESERVE_FAILS.with(|c| {
        let mut n = c.borrow_mut();
        if *n > 0 {
            *n -= 1;
            Some(VmemError::last_os_error())
        } else {
            None
        }
    })
}

/// Internal: consume one armed commit fault, returning the error to raise.
pub(crate) fn take_commit_fault() -> Option<VmemError> {
    COMMIT_FAILS.with(|c| {
        let mut n = c.borrow_mut();
        if *n > 0 {
            *n -= 1;
            Some(VmemError::last_os_error())
        } else {
            None
        }
    })
}
