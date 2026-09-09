//! The Linux acquisition seam: the task-status read with the
//! kernel-version fallback.
//!
//! # Why this file exists
//!
//! Sol-codex proc-memstat review round 3, P3-1. The NotFound→fallback branch
//! of the Linux backend's acquisition step previously lived inline in
//! `mod platform`'s `read_status()`, reachable only through a LIVE
//! `/proc/thread-self` — so the branch that keeps pre-3.17 kernels working
//! could not be exercised anywhere a modern kernel is mounted. Extracted
//! here, it is `#[path]`-included by `tests/status_read.rs` on EVERY host
//! (exactly like `src/status_parse.rs` and `src/status_convert.rs`), so the
//! fallback is directly testable without an actual pre-3.17 kernel, without
//! widening the crate's public API.
//!
//! The fallback fires ONLY on [`std::io::ErrorKind::NotFound`] — the exact
//! shape of a pre-3.17 kernel, where `/proc/thread-self` does not exist at
//! all. Any OTHER error from the primary read propagates untouched: an
//! arbitrary I/O failure must never be masked as "must be an old kernel".
//!
//! Pure std, no `cfg` — host-independent, so the `#[path]` compilation is
//! the sanctioned pattern that keeps the crate's public API untouched.

/// The production acquisition: read the primary path — the calling
/// THREAD's own task status (`/proc/thread-self/status`) — and fall back to
/// the fallback path — the leader-named file (`/proc/self/status`) — only
/// when the primary is `NotFound`.
///
/// `NotFound` is the ONLY error that triggers the fallback: kernels older
/// than 3.17 have no `/proc/thread-self` at all, so their primary read
/// fails with exactly that kind. Any other error from the primary read is
/// returned untouched — an arbitrary I/O failure must never be masked as
/// "must be an old kernel". The fallback file is correct whenever the
/// leader is alive, which on such a kernel is the only situation this read
/// can be made in.
pub(crate) fn read_status_from(
    thread_status: &str,
    process_status: &str,
) -> Result<Vec<u8>, std::io::Error> {
    match std::fs::read(thread_status) {
        Ok(bytes) => Ok(bytes),
        // Linux < 3.17 has no `/proc/thread-self` at all; fall back to
        // the leader-named file so old kernels keep their previous
        // (correct-while-leader-alive) behaviour instead of losing the
        // reading to a missing path.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::read(process_status),
        Err(e) => Err(e),
    }
}
