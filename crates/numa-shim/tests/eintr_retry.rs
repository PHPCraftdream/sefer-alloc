//! Behavioral-oracle tests for `numa_shim::eintr` (task #1327, seventeenth
//! review P3-1).
//!
//! Before this task, the bounded-EINTR retry decision for the Linux sysfs
//! topology scan (`should_retry_eintr`, added by task #1319) lived inside
//! the `#[cfg(all(target_os = "linux", not(miri)))] mod platform` module
//! as a private fn matching a locally-defined `EINTR: i32 = 4` errno
//! constant — ZERO test coverage on ANY host: that module does not even
//! compile on this session's Windows machine, and no Linux test reached
//! the private fn either. The predicate is pure `std::io::Error` logic
//! with no OS dependency (the raw errno comparison was replaced by
//! `err.kind() == std::io::ErrorKind::Interrupted`, the kind std's own
//! `decode_error_kind` maps `EINTR` to on every Unix), so `src/lib.rs`
//! (task #1327) relocated it into the target-independent,
//! `#[doc(hidden)] pub` module `numa_shim::eintr` specifically so this
//! file can exercise it on ANY host — the same extraction pattern as
//! `numa_shim::cpumap` (`tests/cpumap_parser.rs`) before it.

use numa_shim::eintr::{should_retry_eintr, EINTR_RETRY_LIMIT};

/// An `Interrupted`-kind error with a fresh streak is retried: one stray
/// signal during the process's first topology scan must not permanently
/// disable NUMA detection (the original task #1319 bug the retry exists
/// to fix). Counterfactual oracle: under the pre-#1327 raw-errno check
/// (`raw_os_error() == Some(4)`), an `Error::from(ErrorKind::Interrupted)`
/// has `raw_os_error() == None`, so this assert would FAIL — proving the
/// test exercises the ErrorKind-based check, not a tautology.
#[test]
fn interrupted_error_with_fresh_streak_is_retried() {
    let err = std::io::Error::from(std::io::ErrorKind::Interrupted);
    assert!(should_retry_eintr(&err, 0));
}

/// The bound is a strict `<`: the last permitted retry is at
/// `EINTR_RETRY_LIMIT - 1` consecutive interruptions, and the limit itself
/// fails closed — a pathological signal storm must not spin the `OnceLock`
/// topology initializer forever (the availability-vs-hang tradeoff the
/// bound exists for).
#[test]
fn retry_limit_exhaustion_fails_closed() {
    let err = std::io::Error::from(std::io::ErrorKind::Interrupted);
    assert!(should_retry_eintr(&err, EINTR_RETRY_LIMIT - 1));
    assert!(!should_retry_eintr(&err, EINTR_RETRY_LIMIT));
}

/// Any non-`Interrupted` error fails closed at every streak count.
/// Errno 13 is `EACCES` on Unix (a real permission refusal, never an
/// interruption); on the Windows host the same bits decode through the
/// Windows table to a kind that is never `Interrupted` either. This is a
/// pure function over the decoded kind, so the specific errno needs no
/// real OS meaning on the host running the test.
#[test]
fn non_interrupted_error_never_retries() {
    let err = std::io::Error::from_raw_os_error(13);
    assert!(!should_retry_eintr(&err, 0));
    assert!(!should_retry_eintr(&err, EINTR_RETRY_LIMIT - 1));
}
