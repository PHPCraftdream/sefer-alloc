//! Path-injected tests for the Linux acquisition seam's NotFound→fallback
//! branch (Sol-codex proc-memstat review round 3, P3-1).
//!
//! What this file pins and a live test cannot:
//!
//! The production `read_status()` runs against real kernel paths, so its
//! fallback is unreachable wherever `/proc/thread-self` exists — i.e. on
//! every kernel this repo is ever compiled on. Here the seam's two path
//! arguments are INJECTED (missing paths, temp files, an interior-NUL path), so all
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

    let result = status_read::read_status_from(&missing, &fallback_path);

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

    let result = status_read::read_status_from(&primary_path, &missing);

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
        std::path::Path::new("proc_memstat_bad\0path"),
        &fallback_path,
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

    let result = status_read::read_status_from(&missing_thread, &missing_process);

    let err = result.expect_err("both paths missing must fail");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

/// Only name-encoding errors from create_new may skip the byte-read assertion.
#[cfg(unix)]
fn is_filename_encoding_error(error: &std::io::Error) -> bool {
    // ext4 strict casefold: EINVAL; macOS APFS: EILSEQ (Darwin errno 92).
    (cfg!(target_os = "linux") && error.kind() == std::io::ErrorKind::InvalidInput)
        || (cfg!(target_os = "macos") && error.raw_os_error() == Some(92))
}

/// The P3-1 regression fixture (Sol-codex review round 4): the seam takes
/// `&Path`, so a temp path that is NOT valid UTF-8 — which `std::env::
/// temp_dir()` can legitimately produce via a non-UTF-8 `TMPDIR` path
/// component on Unix — flows through the read untouched. On the pre-fix
/// `&str` signature every call site converted first, and
/// `path.to_str().expect("utf-8 temp path")` PANICKED on exactly this path
/// during test setup, before the seam — and the fallback logic under test —
/// was ever reached. A plain Cyrillic or other valid-non-ASCII name would
/// NOT exercise this: it is valid UTF-8, so the old conversion succeeded.
#[cfg(unix)]
#[test]
fn non_utf8_temp_path_is_read_without_conversion() {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;

    // Raw 0xFF is not UTF-8; some filesystems reject it in names.
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut name =
        std::ffi::OsString::from(format!("proc_memstat_status_read_{pid}_{nanos}_non_utf8_"));
    name.push(std::ffi::OsStr::from_bytes(b"\xFF.txt"));
    let path = std::env::temp_dir().join(name);

    // Premise: this is genuinely the path shape the old setup panicked on.
    assert!(
        path.to_str().is_none(),
        "fixture premise: the temp path must not be valid UTF-8"
    );

    let fallback_missing = temp_path("non_utf8_missing_fallback");
    let bytes: &[u8] = b"VmRSS:\t  4242 kB\n";

    // Confirm the same directory accepts an ordinary fixture first.
    let control_path = temp_path("non_utf8_ascii_control");
    let mut control =
        std::fs::File::create_new(&control_path).expect("create ASCII control fixture");
    let control_write = control.write_all(bytes);
    drop(control);
    let _ = std::fs::remove_file(&control_path);
    control_write.expect("write ASCII control fixture");

    let mut file = match std::fs::File::create_new(&path) {
        Ok(file) => file,
        Err(e) if is_filename_encoding_error(&e) => {
            writeln!(
                std::io::stderr().lock(),
                "[status_read] SKIP non-UTF-8 file-byte assertion: name rejected ({e}); \
                 checking only the reader's error path"
            )
            .expect("report unavailable non-UTF-8 fixture");
            status_read::read_status_from(&path, &fallback_missing)
                .expect_err("a rejected non-UTF-8 name must yield a reader error, not panic");
            return;
        }
        Err(e) => panic!("create non-UTF-8 fixture failed: {e}"),
    };
    let write_result = file.write_all(bytes);
    drop(file);
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&path);
        panic!("write non-UTF-8 fixture failed: {e}");
    }

    let result = status_read::read_status_from(&path, &fallback_missing);
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        result.expect("the seam must read the REAL non-UTF-8 path"),
        bytes,
        "the seam must return the REAL file's bytes, without lossy path conversion"
    );
}

#[cfg(unix)]
#[test]
fn fixture_setup_errors_are_not_name_encoding_skips() {
    use std::io::{Error, ErrorKind};

    for kind in [
        ErrorKind::PermissionDenied,
        ErrorKind::NotFound,
        ErrorKind::AlreadyExists,
        ErrorKind::StorageFull,
        ErrorKind::ReadOnlyFilesystem,
        ErrorKind::InvalidData,
        ErrorKind::WriteZero,
        ErrorKind::Other,
    ] {
        assert!(
            !is_filename_encoding_error(&Error::from(kind)),
            "setup failure {kind:?} must not skip the byte-read assertion"
        );
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn known_filename_encoding_rejection_can_skip() {
    let errno = if cfg!(target_os = "macos") { 92 } else { 22 };
    let error = std::io::Error::from_raw_os_error(errno);
    assert!(is_filename_encoding_error(&error), "{error:?}");
}
