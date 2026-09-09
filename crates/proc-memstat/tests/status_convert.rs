//! Exact-value and injected-result tests for the Linux status→`MemStat`
//! conversion seam (review round 2, P3-2 and P3-3b).
//!
//! What this file pins and why a live test cannot:
//!
//! - **The scale.** `tests/platform_contract.rs`'s scale oracle is a ratio
//!   band between two LIVE reads of a moving process, so a ×2 or ×1000
//!   multiplier bug passes it. Here the production
//!   `status_convert::memstat_from_status` is fed an immutable fixture byte
//!   buffer and the resulting `MemStat` is compared BYTE-EXACTLY, so a
//!   dropped, doubled, or decimalized multiplier fails loudly.
//! - **The error contract.** `SnapshotError::Os`/`Malformed` used to be
//!   constructed only as bare values in tests — the real failure paths never
//!   ran through real code, and `tests/fallible_api.rs`'s two-live-calls
//!   comparison proves nothing causal. Here ONE tested path — "acquire bytes
//!   → parse → produce `MemStat` or `SnapshotError`" — is fed one
//!   pre-arranged injected result per case (`Ok(well-formed)`, `Ok(malformed)`,
//!   `Err(io)`) and the exact error variant out is asserted.
//!
//! The real source files are wired in with `#[path]` — the sanctioned
//! pattern (`status_parse` did it first) that keeps the crate's public API
//! untouched while testing the production code on every host, /proc or not.

#[path = "../src/status_convert.rs"]
mod status_convert;
#[path = "../src/status_parse.rs"]
mod status_parse;

// Re-exported at the test crate root so the `#[path]`-included seam module's
// `use crate::{MemStat, SnapshotError}` resolves to the REAL crate types.
use proc_memstat::{MemStat, SnapshotError};

/// A realistic task-status shape, hand-built and immutable.
///
/// Deliberate details:
///
/// - the `Name:` line contains byte `0xFF` — invalid UTF-8, the review
///   P2-3 case — to show the seam is byte-oriented and does not care;
/// - several irrelevant lines surround the memory ones (procfs is a big
///   file; the parser must find its fields, not assume position);
/// - the three memory numbers are all DIFFERENT (`1234`, `654321`, `2000`)
///   so a field mix-up (`VmSize` into `rss`, say) cannot coincide with the
///   expected value.
const FIXTURE: &[u8] = b"Name:\tworker thr\xffead\n\
    Umask:\t0022\n\
    State:\tS (sleeping)\n\
    Tgid:\t4242\n\
    Pid:\t4243\n\
    Threads:\t7\n\
    VmPeak:\t 700000 kB\n\
    VmRSS:\t     1234 kB\n\
    VmSize:\t   654321 kB\n\
    VmHWM:\t     2000 kB\n\
    HugetlbPages:\t       0 kB\n";

/// The fixture WITHOUT the VmSize/VmHWM lines: absent optional fields.
const FIXTURE_RSS_ONLY: &[u8] = b"Name:\tworker thr\xffead\n\
    State:\tS (sleeping)\n\
    VmRSS:\t     1234 kB\n";

/// The full conversion produces EXACT bytes — the scale oracle the live
/// ratio band could not be.
///
/// A dropped multiplier, a ×1000, or a ×2048 all produce different values
/// here; the ratio-band test could not tell any of them from ×1024.
#[test]
fn the_full_conversion_produces_exact_bytes() {
    assert_eq!(
        status_convert::memstat_from_status(FIXTURE),
        Ok(MemStat {
            rss: 1234 * 1024,
            virtual_size: Some(654321 * 1024),
            commit_charge: None,
            peak_rss: Some(2000 * 1024),
        }),
    );
}

/// The read-result spine agrees with the byte seam: the two compose into the
/// ONE tested path, "acquire bytes → parse → `MemStat` or `SnapshotError`".
#[test]
fn the_read_result_spine_agrees_with_the_byte_seam() {
    assert_eq!(
        status_convert::snapshot_from_read(Ok(FIXTURE.to_vec())),
        Ok(MemStat {
            rss: 1234 * 1024,
            virtual_size: Some(654321 * 1024),
            commit_charge: None,
            peak_rss: Some(2000 * 1024),
        }),
    );
}

/// A content defect is `Malformed`, not `Os`: the source was read fine, its
/// shape is the problem.
///
/// `12 MB` is rejected by the strict kB grammar (the unit must be exactly
/// `kB`), so `VmRSS` is unparseable and — being the one field with no
/// `Option` to express absence — the whole conversion fails.
#[test]
fn an_ok_read_of_a_malformed_status_is_malformed() {
    const FIXTURE_MB: &[u8] = b"Name:\tworker\nVmRSS:\t     12 MB\nVmSize:\t   654321 kB\n";
    assert_eq!(
        status_convert::snapshot_from_read(Ok(FIXTURE_MB.to_vec())),
        Err(SnapshotError::Malformed),
    );
}

/// A task status without `VmRSS` at all is `Malformed` through BOTH seams —
/// the missing-field classification does not depend on which entry point was
/// taken.
#[test]
fn a_status_without_vmrss_is_malformed() {
    const FIXTURE_NO_RSS: &[u8] = b"Name:\tworker\nVmSize:\t   654321 kB\nVmHWM:\t     2000 kB\n";
    assert_eq!(
        status_convert::memstat_from_status(FIXTURE_NO_RSS),
        Err(SnapshotError::Malformed),
    );
    assert_eq!(
        status_convert::snapshot_from_read(Ok(FIXTURE_NO_RSS.to_vec())),
        Err(SnapshotError::Malformed),
    );
}

/// A failed read maps to `Os`, not `Malformed` — and is not lost.
///
/// `InvalidData` is the exact `io::ErrorKind` `read_to_string` produced on a
/// non-UTF-8 status (the review P2-3 regression class), so the injected
/// error is the real historical shape of an acquisition failure. Mapping it
/// to `Malformed` (claiming a content defect where the ACCESS failed) or
/// losing it to a default is what this test forecloses.
#[test]
fn a_failed_read_maps_to_os_not_malformed() {
    assert_eq!(
        status_convert::snapshot_from_read(Err(std::io::Error::from(
            std::io::ErrorKind::InvalidData
        ))),
        Err(SnapshotError::Os),
    );
}

/// Absent optional fields convert to `None` while `rss` still converts
/// exactly — absence must not drag the required field down with it.
#[test]
fn optional_absent_fields_convert_to_none_with_an_exact_rss() {
    assert_eq!(
        status_convert::memstat_from_status(FIXTURE_RSS_ONLY),
        Ok(MemStat {
            rss: 1234 * 1024,
            virtual_size: None,
            commit_charge: None,
            peak_rss: None,
        }),
    );
}

/// The LAST KiB figure whose ×1024 product fits in a `u64` converts exactly:
/// `18_014_398_509_481_983 = u64::MAX / 1024`.
///
/// Behavior ABOVE this boundary — a KiB figure whose product overflows the
/// `u64` byte value — is review round 2's P4-1, a separate follow-up
/// deliberately NOT asserted here.
#[test]
fn the_boundary_kib_value_converts_exactly() {
    const FIXTURE_BOUNDARY: &[u8] = b"VmRSS:\t 18014398509481983 kB\n";
    assert_eq!(
        status_convert::memstat_from_status(FIXTURE_BOUNDARY),
        Ok(MemStat {
            rss: 18_014_398_509_481_983 * 1024,
            virtual_size: None,
            commit_charge: None,
            peak_rss: None,
        }),
    );
}
