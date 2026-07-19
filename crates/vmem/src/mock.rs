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

use core::cell::RefCell;

use crate::error::VmemError;

/// One recorded invocation of a public `aligned-vmem` function under the mock.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    /// [`crate::try_reserve_aligned`] / [`crate::reserve_aligned`].
    Reserve {
        /// Requested reservation size in bytes.
        size: usize,
        /// Requested alignment in bytes.
        align: usize,
    },
    /// [`crate::reserve_aligned_lazy`] (feature `lazy-commit`).
    ReserveLazy {
        /// Requested reservation size in bytes.
        size: usize,
        /// Requested alignment in bytes.
        align: usize,
        /// Bytes committed up front.
        initial_commit: usize,
    },
    /// [`crate::reserve_aligned_huge`] (feature `huge-pages`).
    ReserveHuge {
        /// Requested reservation size in bytes.
        size: usize,
        /// Requested alignment in bytes.
        align: usize,
    },
    /// [`crate::release`] (from `into_parts` + manual release).
    Release {
        /// Reservation base address, as `usize`.
        reservation: usize,
        /// Reservation length in bytes.
        reservation_len: usize,
    },
    /// [`crate::decommit`].
    Decommit {
        /// Span base address, as `usize`.
        base: usize,
        /// Start offset in bytes.
        start: usize,
        /// End offset in bytes.
        end: usize,
    },
    /// [`crate::decommit_lazy`].
    DecommitLazy {
        /// Span base address, as `usize`.
        base: usize,
        /// Start offset in bytes.
        start: usize,
        /// End offset in bytes.
        end: usize,
    },
    /// [`crate::recommit`].
    Recommit {
        /// Span base address, as `usize`.
        base: usize,
        /// Start offset in bytes.
        start: usize,
        /// End offset in bytes.
        end: usize,
    },
    /// [`crate::commit_range`] (feature `lazy-commit`).
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
