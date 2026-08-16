//! Recording mock + fault injection for `aligned-vmem` (cfg `aligned_vmem_mock`).
//!
//! Mirrors [`numa-shim`](https://crates.io/crates/numa-shim)'s proven
//! recording-mock pattern: a thread-local call log plus scripted failures, so
//! any consumer can deterministically test its OOM-handling on any target
//! (including macOS and miri) WITHOUT exhausting real commit charge.
//!
//! When the `aligned_vmem_mock` cfg is set:
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
//! # Cross-thread drops split the Reserve/Release pair (task #959)
//!
//! The log behind [`drain`] is a `thread_local!`: a [`Call`] lands in
//! the log of the thread the call runs ON, not the thread the
//! reservation was created on. [`crate::Reservation`] is `Send` (see
//! the `unsafe impl Send for Reservation` and its `SAFETY` comment in
//! `src/lib.rs` — a reservation owns its bytes exclusively, with no
//! thread affinity), so a test can create a reservation on thread A,
//! move it to thread B, and drop it there. `Reservation`'s `Drop` then
//! records `Call::Release` in thread B's log while the paired
//! `Call::Reserve` stays in thread A's log; neither thread's `drain()`
//! ever sees both halves, and a naive "every Reserve has a Release"
//! leak check on thread A would misread the reservation as leaked.
//! This is the thread-local log working as designed — not unsoundness,
//! not a leak. Practical rule: [`drain`] on the thread where the drop
//! happened; a test that moves a `Reservation` across threads must not
//! expect one `drain()` to contain the Reserve/Release pair.
//!
//! # Build-time cfg flag (task #962)
//!
//! This backend is enabled via the `aligned_vmem_mock` cfg flag
//! (`RUSTFLAGS="--cfg aligned_vmem_mock"`), following the same pattern as
//! this repo's `cfg(loom)`/`cfg(kani)` flags. A `--cfg` flag cannot be
//! silently unified into a build by another crate downstream — that was the
//! whole point of the conversion (task #715, task #658).

use core::cell::{Cell, RefCell};

use crate::error::VmemError;

/// One recorded invocation of a public `aligned-vmem` function under the mock.
///
/// task #715 (rust-intel audit MEDIUM §C1a): every struct-like variant below
/// ALSO carries its own `#[non_exhaustive]` (the enum-level one above only
/// reserves the right to add whole VARIANTS — adding a FIELD to an existing
/// variant is still semver-major for every downstream `Call::Reserve { size,
/// align }` match without the variant-level marker too; `ReserveLazy` already
/// grew `initial_commit` after `Reserve`/`ReserveHuge` were designed, so this
/// is not a hypothetical). `Call` is new in 0.2.0 (0.1.0 had no mock
/// backend at all) and 0.2.0 has not shipped yet (task #658), so this is
/// decided now, before its own first publish — adding the marker
/// retroactively after 0.2.0 ships would itself be the breaking change this
/// is meant to prevent.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(docsrs, doc(cfg(aligned_vmem_mock)))]
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
    /// [`crate::release`] (from `into_parts` + manual release) AND RAII drop
    /// (via `Reservation`'s `Drop` implementation). Both sources record this
    /// variant when a reservation is released.
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

// Constructors for external crates to build expected call vectors.
impl Call {
    /// Create a [`Call::Reserve`] variant.
    #[must_use]
    pub fn reserve(size: usize, align: usize) -> Self {
        Call::Reserve { size, align }
    }

    /// Create a [`Call::ReserveLazy`] variant.
    #[must_use]
    pub fn reserve_lazy(size: usize, align: usize, initial_commit: usize) -> Self {
        Call::ReserveLazy {
            size,
            align,
            initial_commit,
        }
    }

    /// Create a [`Call::ReserveHuge`] variant.
    #[must_use]
    pub fn reserve_huge(size: usize, align: usize) -> Self {
        Call::ReserveHuge { size, align }
    }

    /// Create a [`Call::Release`] variant.
    #[must_use]
    pub fn release(reservation: usize, reservation_len: usize) -> Self {
        Call::Release {
            reservation,
            reservation_len,
        }
    }

    /// Create a [`Call::Decommit`] variant.
    #[must_use]
    pub fn decommit(base: usize, start: usize, end: usize) -> Self {
        Call::Decommit { base, start, end }
    }

    /// Create a [`Call::DecommitLazy`] variant.
    #[must_use]
    pub fn decommit_lazy(base: usize, start: usize, end: usize) -> Self {
        Call::DecommitLazy { base, start, end }
    }

    /// Create a [`Call::Recommit`] variant.
    #[must_use]
    pub fn recommit(base: usize, start: usize, end: usize) -> Self {
        Call::Recommit { base, start, end }
    }

    /// Create a [`Call::CommitRange`] variant.
    #[must_use]
    pub fn commit_range(base: usize, start: usize, end: usize) -> Self {
        Call::CommitRange { base, start, end }
    }
}

std::thread_local! {
    /// Calls recorded since the last [`drain`].
    static CALLS: RefCell<Vec<Call>> = const { RefCell::new(Vec::new()) };
    /// Remaining scripted reserve failures ([`fail_next_reserve`]).
    static RESERVE_FAILS: RefCell<u32> = const { RefCell::new(0) };
    /// Remaining scripted commit failures ([`fail_next_commit`]).
    static COMMIT_FAILS: RefCell<u32> = const { RefCell::new(0) };
    /// Reentrancy guard for [`record`]. Set to `true` while we're inside
    /// `record` to detect and silently drop reentrant calls rather than
    /// panicking. This prevents a panic when the consumer's global allocator
    /// calls back into this crate during `Vec::push`'s allocation (task #945/M-1).
    static RECORDING: Cell<bool> = const { Cell::new(false) };
}

/// Drain and return every recorded [`Call`] since the last drain (or test
/// start). Clears the log.
///
/// Returns THIS thread's log only: a [`Call::Release`] recorded by a
/// `Reservation` dropped on another thread is not visible here — see
/// the module-level "Cross-thread drops" section for the mechanism.
#[must_use]
#[cfg_attr(docsrs, doc(cfg(aligned_vmem_mock)))]
pub fn drain() -> Vec<Call> {
    CALLS.with(|c| c.borrow_mut().drain(..).collect())
}

/// Clear the recorded call log AND both fault counters — call at the start of a
/// test to isolate it from any residue on the current thread.
#[cfg_attr(docsrs, doc(cfg(aligned_vmem_mock)))]
pub fn reset() {
    CALLS.with(|c| c.borrow_mut().clear());
    RESERVE_FAILS.with(|c| *c.borrow_mut() = 0);
    COMMIT_FAILS.with(|c| *c.borrow_mut() = 0);
}

/// Arm the reserve fault injector: the next `n` reservation attempts
/// ([`crate::try_reserve_aligned`] and its `lazy`/`huge` variants) return
/// `Err(VmemError::os_refusal_unknown_code())` without allocating. `n == 0` disarms.
#[cfg_attr(docsrs, doc(cfg(aligned_vmem_mock)))]
pub fn fail_next_reserve(n: u32) {
    RESERVE_FAILS.with(|c| *c.borrow_mut() = n);
}

/// Arm the commit fault injector: the next `n` commit attempts
/// ([`crate::recommit`] / [`crate::commit_range`]) return failure without
/// touching the OS, simulating commit-charge exhaustion. `n == 0` disarms.
#[cfg_attr(docsrs, doc(cfg(aligned_vmem_mock)))]
pub fn fail_next_commit(n: u32) {
    COMMIT_FAILS.with(|c| *c.borrow_mut() = n);
}

/// Internal: record a call into the thread-local log.
///
/// # Reentrancy safety (task #945/M-1)
///
/// This function is called from within the `GlobalAlloc` implementation path.
/// When `Vec::push` needs to grow its buffer, it allocates through the global
/// allocator, which may call back into this crate again. Without a guard, this
/// would attempt to mutably borrow `CALLS` twice on the same thread, causing a
/// `BorrowMutError` panic inside an allocator — undefined behavior in that
/// context.
///
/// We guard against this with a `RECORDING` flag: if already set (indicating a
/// reentrant call), we silently drop the recording rather than corrupting state
/// or panicking. The reentrant call's own recording is lost, but the outer
/// recording remains intact and the allocator path completes safely.
///
/// This is the same hazard class already documented for the miri backend in
/// the crate-level module header (see `lib.rs`'s "A consumer that installs
/// itself as `#[global_allocator]` cannot use this crate under miri..."
/// paragraph — the mock backend has the same issue for the same reason).
///
/// # Thread-local storage teardown safety (task #945/M-2)
///
/// `Reservation::drop` calls this function (via `release` in `lib.rs`). If a
/// `Reservation` is owned by a `thread_local!` elsewhere in a consumer's code,
/// its destructor runs during TLS teardown, in unspecified order relative to
/// `CALLS`'s own destructor. `LocalKey::with` panics if the thread-local value
/// has already been destroyed on that thread — a panic during `Drop` becomes
/// an abort if anything else is unwinding.
///
/// We use `try_with` (instead of `with`) to silently become a no-op when
/// `CALLS` has already been destroyed, avoiding the teardown-order panic.
///
/// Note: no RAII guard is used to clear `RECORDING` on panic, because the only
/// way `Vec::push` can panic is allocation failure, which aborts the process
/// regardless. A non-allocation panic in `push` is virtually impossible (the
/// only path would be `Clone` impl on `Call` panicking, which cannot happen
/// here). The added complexity of a guard is not worth it for a case that
/// either never occurs or always aborts.
pub(crate) fn record(call: Call) {
    RECORDING.with(|recording| {
        if recording.get() {
            // Reentrant call: silently drop to avoid BorrowMutError panic.
            // The outer call's recording stays intact.
            return;
        }
        recording.set(true);

        // Use `try_with` to avoid panicking during TLS teardown when `CALLS`
        // has already been destroyed (task #945/M-2).
        let _ = CALLS.try_with(|c| c.borrow_mut().push(call));

        recording.set(false);
    });
}

/// Internal: consume one armed reserve fault, returning the error to raise.
pub(crate) fn take_reserve_fault() -> Option<VmemError> {
    RESERVE_FAILS.with(|c| {
        let mut n = c.borrow_mut();
        if *n > 0 {
            *n -= 1;
            // task #776 (F2): this is a SIMULATED failure -- no real syscall
            // runs, so `VmemError::last_os_error()` would read whatever
            // `errno`/`GetLastError` happens to be lying around from
            // unrelated prior code, exactly the "fabricated OS code" hazard
            // task #713 already fixed for `try_commit_range`'s real-path
            // fault-injection branch (`lib.rs`). Mirrors that fix.
            Some(VmemError::os_refusal_unknown_code())
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
            // task #776 (F2): same reasoning as `take_reserve_fault` above.
            Some(VmemError::os_refusal_unknown_code())
        } else {
            None
        }
    })
}
