//! The Linux status→`MemStat` conversion seam.
//!
//! # Why this file exists
//!
//! Sol-codex proc-memstat review round 2, P3-2 and P3-3(b). Until this seam
//! was extracted, the `* 1024` scale and the `MemStat` field mapping lived
//! inline in the Linux backend's `try_snapshot()`, reachable only through a
//! LIVE `/proc` read — so the ratio-band test in `tests/platform_contract.rs`
//! could not prove the ×1024 multiplier (a 2× or ×1000 bug passes a ratio
//! band between two live reads), and the `SnapshotError::Os`/`Malformed`
//! variants were constructed only as bare values in tests: the real failure
//! paths never ran through real code.
//!
//! This file is therefore the ONLY place the `* 1024` scale and the
//! `MemStat` field mapping live. A wrong or missing multiplier, or a misrouted
//! error variant, is caught by the exact-value and injected-result tests in
//! `tests/status_convert.rs` — not by a live ratio oracle or a two-live-calls
//! comparison, neither of which can pin exact bytes.
//!
//! The code is pure: no I/O, no `cfg` — host-independent, so
//! `tests/status_convert.rs` can `#[path]`-include it on EVERY platform,
//! exactly like `src/status_parse.rs`. The `use crate::...` paths resolve to
//! the real crate types in the library compilation, and to the same real
//! types re-exported at the test crate root in the `#[path]` compilation —
//! the sanctioned pattern that keeps the crate's public API untouched.

use crate::status_parse::read_kib_field;
use crate::{MemStat, SnapshotError};

/// The production conversion: a task-status buffer as read from
/// `/proc/thread-self/status` into a [`MemStat`], in BYTES.
///
/// procfs reports the `Vm*` figures in kB; this is where that kB figure
/// becomes the bytes every other layer of this crate deals in. The exact
/// multiplier is pinned by `tests/status_convert.rs` against an immutable
/// fixture, which is the only kind of test that can distinguish ×1024 from
/// ×1000 or ×2048 — a live ratio band cannot. A KiB figure whose ×1024
/// product overflows `u64` is `Malformed` too — a content defect, not a
/// panic or a wrap.
pub(crate) fn memstat_from_status(status: &[u8]) -> Result<MemStat, SnapshotError> {
    // VmRSS is the one field with no `Option` to express absence, so a
    // procfs without it is Malformed rather than a silent zero. `Malformed`
    // also covers the ×1024 overflow: the strict parser already rejects a
    // KiB figure that overflows `u64`, but even a parseable one overflows
    // when SCALED (`u64::MAX / 1024 + 1` KiB × 1024 > `u64::MAX`), and an
    // unchecked `* 1024` there would panic under overflow-checks or wrap to
    // a fabricated near-zero byte count without them. Checked on ALL THREE
    // scaled fields (review round 2, P4-1; defensive — no real Linux
    // process reaches the boundary).
    let rss = read_kib_field(status, b"VmRSS:")
        .and_then(|kib| kib.checked_mul(1024))
        .ok_or(SnapshotError::Malformed)?;
    Ok(MemStat {
        rss,
        virtual_size: read_kib_field(status, b"VmSize:")
            .map(|kib| kib.checked_mul(1024).ok_or(SnapshotError::Malformed))
            .transpose()?,
        // The task status exposes no commit-charge counter; `VmSize` above
        // is address space, a different quantity (see `MemStat`).
        commit_charge: None,
        peak_rss: read_kib_field(status, b"VmHWM:")
            .map(|kib| kib.checked_mul(1024).ok_or(SnapshotError::Malformed))
            .transpose()?,
    })
}

/// The whole non-I/O spine of the backend: the RESULT of the task-status read
/// in, [`MemStat`] or the documented [`SnapshotError`] out.
///
/// This is the single tested path "acquire bytes → parse → produce `MemStat`
/// or `SnapshotError`", with the acquire step injectable: `tests/status_convert.rs`
/// feeds it one pre-arranged `Ok`/`Err` per case and asserts which error
/// variant comes out — the per-injected-result coverage P3-3 asked for, which
/// comparing two live OS calls cannot give.
///
/// Error classification lives here and only here: an `io::Error` from the
/// acquisition is `SnapshotError::Os` (the ACCESS failed), while a content
/// defect found by the conversion is `SnapshotError::Malformed` (the source
/// was read, its shape was wrong). `std::fs::read` produces NO
/// `InvalidData` error for non-UTF-8 content — unlike `read_to_string` —
/// but the injected test still pins this mapping against the historical
/// `read_to_string` failure shape, so a regression to `Malformed` (or to
/// silently dropping the error) is caught.
pub(crate) fn snapshot_from_read(
    read: Result<Vec<u8>, std::io::Error>,
) -> Result<MemStat, SnapshotError> {
    memstat_from_status(&read.map_err(|_| SnapshotError::Os)?)
}
