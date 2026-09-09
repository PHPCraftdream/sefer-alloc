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
/// not a plain ASCII integer followed by the `kB` unit.
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
        // "VmRSS:	   1234 kB" — the shared tail below skips the ASCII
        // whitespace between the prefix and the number, then takes the digits.
        return field_value(rest);
    }
    None
}

/// Read SEVERAL fields in ONE pass over `status`, in the order `prefixes`
/// gives them.
///
/// Same per-field semantics as [`read_kib_field`] — this only changes how
/// many times the buffer is walked, from once per field to once in total.
///
/// # Precondition: distinct, non-overlapping prefixes
///
/// The equivalence with N independent [`read_kib_field`] calls holds only
/// while a status line can match at most ONE of `prefixes`. That is a
/// REQUIREMENT on the caller, not something the signature enforces: given
/// `"VmRSS: 7 kB"` and the duplicate prefixes `[b"VmRSS:", b"VmRSS:"]`, the
/// two per-field calls both return `Some(7)`, but after the first prefix
/// matches a line this function stops comparing that LINE (the `break`
/// below) and moves on, so the second, duplicate prefix records `None`. The
/// three `FIELDS` of `examples/status_scan_cost.rs` — the only prefix set
/// this function is measured with — satisfy the requirement, and production
/// code never calls this function at all (see the NO-GO note below). The
/// behavior with duplicate prefixes is pinned as a KNOWN, documented
/// limitation by a fixture in `tests/status_parse.rs`, not left
/// undiscoverable.
///
/// Exists to answer review P4-2 with a measurement rather than an assumption:
/// the finding notes that three lookups rescan the buffer from the start each
/// time, which is O(L) done three times, NOT O(L²), and that the gain from
/// fusing them is unmeasured. `examples/status_scan_cost.rs` measures both
/// against each other; see that file for what the numbers actually said.
///
/// **The answer was NO-GO, and this function is deliberately NOT wired into
/// the backend.** It is 2.62x faster than three separate scans on a real
/// `/proc/self/status`, and that saving is 2.0% of one whole `snapshot()`
/// call, which the open/read/close round trip dominates. It stays in the tree
/// as the measurement's reproduction subject — the comparison cannot be
/// re-run without both arms — not as a pending optimization someone should
/// finish wiring up.
///
/// Deliberately still a full scan: the review rules out reading only the
/// first 4/8 KiB, because a long field such as `Groups` can precede the
/// memory fields, and a truncated read would silently lose them.
// Unconditionally allowed, not `cfg_attr(not(test), ...)`: the only users are
// `examples/status_scan_cost.rs` and `tests/status_parse.rs`, both of which
// pull this file in with `#[path]` and are therefore invisible to the library
// AND to the other `#[path]` includer (`tests/platform_contract.rs`), which
// compiles the same module without touching this function. A `test`-scoped
// allow leaves those two compilations red.
#[allow(dead_code)]
pub(crate) fn read_kib_fields<const N: usize>(
    status: &[u8],
    prefixes: [&[u8]; N],
) -> [Option<u64>; N] {
    let mut out = [None; N];
    // Tracked separately from `out`: a field can be SEEN yet parse to `None`
    // (a malformed value), and that must count as resolved — otherwise a
    // second line with the same prefix would be consulted, which
    // `read_kib_field` never does, and the two readers would disagree.
    // Relying on procfs keys being unique instead would make correctness here
    // depend on an external file's shape.
    let mut seen = [false; N];
    let mut remaining = N;
    for line in status.split(|&b| b == b'\n') {
        if remaining == 0 {
            break;
        }
        for i in 0..N {
            if seen[i] {
                continue;
            }
            let Some(rest) = line.strip_prefix(prefixes[i]) else {
                continue;
            };
            out[i] = field_value(rest);
            seen[i] = true;
            remaining -= 1;
            // REQUIRES distinct, non-overlapping prefixes (see this
            // function's doc): given that, a line matching prefix `i` cannot
            // match any other, so stop comparing this line. With duplicate
            // or overlapping prefixes the early stop is observable — prefix
            // `i + 1` never sees the line — which is exactly the documented,
            // fixture-pinned limitation, not an internal detail.
            break;
        }
    }
    out
}

/// Read a unit-less integer line (`Tgid:`, `Pid:`) from `/proc` status.
///
/// Grammar: `ASCII-whitespace* digits ASCII-whitespace* end-of-line` — a bare
/// non-negative ASCII integer with NO unit, which is what the identity fields
/// carry (the `Tgid:`/`Pid:` checks in `tests/thread_leader_exit.rs`).
/// Deliberately a SEPARATE grammar from [`read_kib_field`]'s strict kB one:
/// a kB line read here is rejected, because after the digits comes `" kB"`,
/// which is not whitespace-only — a unit-bearing value must not silently pass
/// through the unit-less reader. Trailing non-whitespace junk after the
/// digits (`1234x`) is likewise rejected rather than truncated to `1234`.
// Unconditionally allowed, not `cfg_attr(not(test), ...)`: the only users are
// `#[path]` includers, invisible to the library AND to the other `#[path]`
// includers, which compile the same module without touching this function. A
// `test`-scoped allow leaves those compilations red.
#[allow(dead_code)]
pub(crate) fn read_int_field(status: &[u8], prefix: &[u8]) -> Option<u64> {
    for line in status.split(|&b| b == b'\n') {
        let Some(rest) = line.strip_prefix(prefix) else {
            continue;
        };
        let (start, end) = integer_span(rest)?;
        // Unit-less grammar: after the digits only whitespace may follow. A
        // kB line read here is rejected — " kB" is not whitespace-only.
        if !rest[end..].iter().all(|b| b.is_ascii_whitespace()) {
            return None;
        }
        return parse_ascii_u64(&rest[start..end]);
    }
    None
}

/// Shared tail of the kB readers: given everything after the prefix on a
/// matching line, return its numeric value, or `None` if the line does not
/// match the strict kB grammar.
///
/// Accepted grammar:
///
/// ```text
/// line = ASCII-whitespace+ , ASCII-digit+ , ASCII-whitespace+ , "kB" , ASCII-whitespace*
/// ```
///
/// Deliberate decisions:
///
/// - the unit is REQUIRED and must be exactly `kB` (case-sensitive: lowercase
///   k, capital B). Mainline kernels print these memory lines as `%5lu kB`
///   (fs/proc/array.c), so requiring it rejects nothing a real kernel prints;
///   treating `MB`/`KB`/a missing unit as kB, by contrast, is exactly the
///   silent unit-substitution misread (a megabyte line read as KiB) that must
///   never happen here;
/// - whitespace between the digits and the unit is required (`12kB` is
///   rejected);
/// - the numeric token must END at the unit: a trailing non-digit before the
///   whitespace/unit (`12oops`, `12.5`, `1e6`) rejects the whole line instead
///   of quietly becoming `12`/`12`/`1`.
fn field_value(rest: &[u8]) -> Option<u64> {
    let (start, end) = integer_span(rest)?;
    let tail = &rest[end..];
    let ws = tail.iter().position(|b| !b.is_ascii_whitespace())?;
    if ws == 0 {
        // Whitespace between integer and unit is required (`12kB` is not
        // accepted).
        return None;
    }
    let unit = tail[ws..].strip_prefix(b"kB")?;
    if !unit.iter().all(|b| b.is_ascii_whitespace()) {
        return None;
    }
    parse_ascii_u64(&rest[start..end])
}

/// The ASCII-digit run after ASCII-whitespace-only leading bytes; `None` if
/// there is no digit or non-whitespace junk precedes it.
fn integer_span(rest: &[u8]) -> Option<(usize, usize)> {
    let start = rest.iter().position(|b| b.is_ascii_digit())?;
    if rest[..start].iter().any(|b| !b.is_ascii_whitespace()) {
        return None;
    }
    let end = start
        + rest[start..]
            .iter()
            .position(|b| !b.is_ascii_digit())
            .unwrap_or(rest.len() - start);
    Some((start, end))
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
