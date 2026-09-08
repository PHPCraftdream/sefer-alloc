//! Byte-level field extraction for `/proc/self/status`.
//!
//! # Why bytes and not `&str`
//!
//! Sol-codex proc-memstat review round 1, P2-3. The whole file used to be read
//! with `read_to_string`, which fails if ANY byte in it is not valid UTF-8 —
//! and the `Name:` line is a byte string, not a guaranteed-UTF-8 one:
//! `PR_SET_NAME` bounds the task name in BYTES and can truncate a name in the
//! middle of a multi-byte character, and `proc_task_name` escapes special
//! characters without promising valid UTF-8. A main thread whose name carries
//! byte `0xFF` therefore made the whole read fail, and the caller's
//! `unwrap_or_default()` turned that into an EMPTY file — so `snapshot()`
//! silently reported `{0, 0, None}` even though `VmRSS`/`VmSize`/`VmHWM` on
//! their own lines were ordinary ASCII digits all along.
//!
//! That is a legitimate process with legitimate procfs data. Whether a
//! measurement is available must not depend on the encoding of a field the
//! measurement never reads, so this parser looks at bytes and touches only the
//! ASCII numeric fields it was asked for.
//!
//! This module is deliberately a standalone file with no crate dependencies:
//! `tests/status_parse.rs` pulls it in with `#[path]` and exercises it
//! directly, so the parser is covered on every host — including ones with no
//! `/proc` — without widening this crate's public API, which the review
//! explicitly ruled out.

/// Read a `<prefix>\t   1234 kB` line out of `/proc/self/status` and return
/// the numeric field in kB, or `None` if the field is absent or its value is
/// not a plain ASCII integer.
///
/// kB is what procfs reports here regardless of the kernel's base page size —
/// unlike `/proc/self/statm`, which is expressed in pages and would need a
/// `sysconf(_SC_PAGESIZE)` query to convert correctly on kernels using 16 KiB
/// or 64 KiB pages (aarch64, ppc64).
///
/// Lines whose bytes are not valid UTF-8 are simply not matched; they cannot
/// abort the scan, which is the entire point of this module.
pub(crate) fn read_kib_field(status: &[u8], prefix: &[u8]) -> Option<u64> {
    for line in status.split(|&b| b == b'\n') {
        let Some(rest) = line.strip_prefix(prefix) else {
            continue;
        };
        // "VmRSS:\t   1234 kB" — skip the run of ASCII whitespace between the
        // prefix and the number, then take the digits that follow.
        let digits_start = rest.iter().position(|b| b.is_ascii_digit())?;
        // Anything non-whitespace before the first digit means this is not the
        // simple `<prefix><ws><digits>` shape, so refuse rather than guess.
        if rest[..digits_start]
            .iter()
            .any(|b| !b.is_ascii_whitespace())
        {
            return None;
        }
        let digits_end = digits_start
            + rest[digits_start..]
                .iter()
                .position(|b| !b.is_ascii_digit())
                .unwrap_or(rest.len() - digits_start);
        return parse_ascii_u64(&rest[digits_start..digits_end]);
    }
    None
}

/// Parse a non-empty run of ASCII digits, returning `None` on overflow.
///
/// Hand-rolled rather than `str::parse` so the input never has to be valid
/// UTF-8 — the caller has already established these bytes are ASCII digits,
/// and going through `from_utf8` would reintroduce, in miniature, exactly the
/// encoding dependency this module exists to remove.
fn parse_ascii_u64(digits: &[u8]) -> Option<u64> {
    if digits.is_empty() {
        return None;
    }
    let mut acc: u64 = 0;
    for &b in digits {
        debug_assert!(b.is_ascii_digit());
        acc = acc.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(acc)
}
