//! Path-injected tests for the Linux acquisition seam's NotFound→fallback
//! branch (Sol-codex proc-memstat review round 3, P3-1).
//!
//! What this file pins and a live test cannot:
//!
//! The production `read_status()` runs against real kernel paths, so its
//! fallback is unreachable wherever `/proc/thread-self` exists — i.e. on
//! every kernel this repo is ever compiled on. Here the seam's two path
//! arguments are INJECTED (missing paths, temp files, a directory), so all
//! four shapes of the branch matrix are exercised directly:
//!
//! - the fallback DOES fire on a `NotFound` primary read, and reads the
//!   fallback file's exact bytes;
//! - the fallback does NOT fire when the primary read succeeds (a
//!   regression to an unconditional fallback returns `Err` here);
//! - a NON-NotFound primary error is NOT masked — the fallback is not even
//!   attempted, so an arbitrary I/O failure can never masquerade as "must
//!   be an old kernel";
//! - a `NotFound` from the fallback read itself propagates (upstream,
//!   `status_convert::snapshot_from_read` classifies any acquisition error
//!   as `SnapshotError::Os`).
//!
//! The production source is wired in with `#[path]` — the sanctioned
//! pattern (`status_parse` did it first) that keeps the crate's public API
//! untouched while testing the production code on every host, /proc or not.

#![cfg(not(miri))] // the tests do real filesystem I/O

#[path = "../src/status_read.rs"]
mod status_read;

use std::time::{SystemTime, UNIX_EPOCH};

/// A unique per-run temp path: pid plus nanos, so two runs — or two tests
/// in one run — can never collide. A dangling temp file after a failure is
/// acceptable; a collision is not (the pattern `tests/thread_leader_exit.rs`
/// uses).
fn temp_path(tag: &str) -> std::path::PathBuf {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("proc_memstat_status_read_{pid}_{nanos}_{tag}.txt"))
}

/// Write known bytes to a fresh temp file, returning (path, bytes).
fn write_temp(tag: &str, bytes: &[u8]) -> (std::path::PathBuf, Vec<u8>) {
    let path = temp_path(tag);
    std::fs::write(&path, bytes).expect("write temp file");
    (path, bytes.to_vec())
}

/// The fallback branch, no old kernel needed: a primary path that does not
/// exist — the exact shape of a pre-3.17 kernel's missing
/// `/proc/thread-self` — must make the seam read the fallback path, and the
/// result must be the fallback file's EXACT bytes.
#[test]
fn fallback_fires_on_not_found_and_returns_fallback_bytes() {
    let (fallback_path, bytes) = write_temp("fallback", b"VmRSS:\t  123456 kB\n");
    let missing = temp_path("missing_thread");

    let result = status_read::read_status_from(
        missing.to_str().expect("utf-8 temp path"),
        fallback_path.to_str().expect("utf-8 temp path"),
    );

    assert_eq!(result.expect("fallback read must succeed"), bytes);

    let _ = std::fs::remove_file(&fallback_path);
}

/// No fallback when the primary read succeeds: a readable primary file and
/// a NONEXISTENT fallback path must yield the PRIMARY file's exact bytes.
/// If a regression made the fallback unconditional, the second read hits
/// the missing path and this returns `Err` — the test fails loudly.
#[test]
fn no_fallback_when_primary_read_succeeds() {
    let (primary_path, bytes) = write_temp("primary", b"VmHWM:\t    4321 kB\n");
    let missing = temp_path("missing_process");

    let result = status_read::read_status_from(
        primary_path.to_str().expect("utf-8 temp path"),
        missing.to_str().expect("utf-8 temp path"),
    );

    assert_eq!(result.expect("primary read must succeed"), bytes);

    let _ = std::fs::remove_file(&primary_path);
}

/// A non-NotFound error is NOT masked: the primary path carries an interior
/// NUL byte, which `std` rejects with `ErrorKind::InvalidInput` BEFORE any
/// syscall, on every host — deterministic and platform-independent. (A
/// directory was considered, but on Windows `std::fs::read` of a directory
/// surfaces as `NotFound` — ERROR_PATH_NOT_FOUND — which would wrongly fire
/// the fallback here.) The result must be `Err` with kind NOT `NotFound`,
/// and the fallback file must remain unread: if the code fell back — the
/// only way `Ok` could appear, or `NotFound` itself — this test fails, so
/// it also proves the fallback is NotFound-only.
#[test]
fn non_not_found_error_is_not_masked() {
    let (fallback_path, _bytes) = write_temp("nul_case", b"VmRSS:\t       0 kB\n");

    let result = status_read::read_status_from(
        "proc_memstat_bad\0path",
        fallback_path.to_str().expect("utf-8 temp path"),
    );

    let err = result.expect_err("an interior-NUL path must fail");
    assert_ne!(
        err.kind(),
        std::io::ErrorKind::NotFound,
        "an interior-NUL path must not surface as NotFound — that would mask a real failure as an old kernel"
    );

    let _ = std::fs::remove_file(&fallback_path);
}

/// Both paths missing: the `NotFound` from the FALLBACK read propagates
/// untouched. Upstream, `status_convert::snapshot_from_read` classifies any
/// acquisition error as `SnapshotError::Os` — this test pins that the
/// fallback's own failure reaches that classifier as a real `Err`, not as a
/// silently-swallowed condition.
#[test]
fn not_found_from_fallback_read_propagates() {
    let missing_thread = temp_path("missing_thread_both");
    let missing_process = temp_path("missing_process_both");

    let result = status_read::read_status_from(
        missing_thread.to_str().expect("utf-8 temp path"),
        missing_process.to_str().expect("utf-8 temp path"),
    );

    let err = result.expect_err("both paths missing must fail");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}
