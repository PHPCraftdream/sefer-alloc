//! Exact-value oracles for the `/proc/self/status` byte parser (review P2-3).
//!
//! The parser is pulled in with `#[path]` rather than through the crate's
//! public API: the review explicitly ruled out widening the public surface
//! just to let tests reach a private parser, and including the file directly
//! also means these run on EVERY host — Windows and macOS included, where
//! there is no `/proc` to read at all.
//!
//! Every case here uses byte literals with DIFFERENT numbers per field, so a
//! field mix-up cannot pass by coincidence.

#[path = "../src/status_parse.rs"]
mod status_parse;

use status_parse::read_kib_field;

/// A realistic `/proc/self/status` prefix whose `Name` is NOT valid UTF-8.
///
/// This is the case the whole task exists for: `PR_SET_NAME` bounds the task
/// name in bytes and can cut a multi-byte character in half, so byte `0xFF`
/// here is legitimate procfs content, not a forged file. Before the fix the
/// enclosing `read_to_string` failed on it and the caller's
/// `unwrap_or_default()` turned the entire snapshot into `{0, 0, None}`,
/// discarding three fields that are plain ASCII digits.
const NON_UTF8_NAME: &[u8] = b"Name:\txx\xFFyy\nUmask:\t0022\nState:\tR (running)\n\
VmSize:\t 4096 kB\nVmRSS:\t  256 kB\nVmHWM:\t  512 kB\n";

#[test]
fn non_utf8_name_does_not_hide_the_ascii_memory_fields() {
    assert_eq!(read_kib_field(NON_UTF8_NAME, b"VmRSS:"), Some(256));
    assert_eq!(read_kib_field(NON_UTF8_NAME, b"VmSize:"), Some(4096));
    assert_eq!(read_kib_field(NON_UTF8_NAME, b"VmHWM:"), Some(512));
}

#[test]
fn the_fixture_is_genuinely_not_utf8() {
    // Guards the test itself: if someone "tidies" the fixture into valid
    // UTF-8, the case above silently stops testing anything.
    //
    // Stated as "contains 0xFF" rather than `from_utf8(..).is_err()` because
    // the latter is a compile-time-known answer for a const, which rustc's
    // `invalid_from_utf8` lint rejects — and 0xFF is the stronger claim
    // anyway: it can never appear in well-formed UTF-8 at any position.
    assert!(
        NON_UTF8_NAME.contains(&0xFF),
        "fixture must contain a byte that cannot occur in UTF-8, or it does \
         not exercise the bug this parser exists to fix"
    );
}

#[test]
fn the_utf8_route_this_parser_replaced_would_still_lose_every_field() {
    // Pins the MECHANISM, not just the outcome, so a future "simplification"
    // back to `read_to_string` cannot pass this suite. Building the Vec at
    // runtime (rather than checking the const directly) keeps rustc's
    // const-eval `invalid_from_utf8` lint out of it.
    let bytes: Vec<u8> = NON_UTF8_NAME.to_vec();
    let as_string = String::from_utf8(bytes);
    assert!(
        as_string.is_err(),
        "the whole-file UTF-8 conversion must fail here — that failure, plus \
         the caller's unwrap_or_default(), is what used to blank the snapshot"
    );

    // And the byte parser reads the very same content without difficulty.
    assert_eq!(read_kib_field(NON_UTF8_NAME, b"VmRSS:"), Some(256));
}

#[test]
fn each_field_reads_its_own_value() {
    // Distinct numbers: a prefix mix-up cannot coincide with a right answer.
    let status = b"VmPeak:\t 9999 kB\nVmSize:\t 1111 kB\nVmRSS:\t 2222 kB\nVmHWM:\t 3333 kB\n";
    assert_eq!(read_kib_field(status, b"VmSize:"), Some(1111));
    assert_eq!(read_kib_field(status, b"VmRSS:"), Some(2222));
    assert_eq!(read_kib_field(status, b"VmHWM:"), Some(3333));
    assert_eq!(read_kib_field(status, b"VmPeak:"), Some(9999));
}

#[test]
fn absent_field_is_none_not_zero() {
    let status = b"VmSize:\t 1111 kB\n";
    assert_eq!(read_kib_field(status, b"VmHWM:"), None);
}

#[test]
fn a_prefix_is_matched_at_line_start_only() {
    // "VmRSS:" appears, but only inside another line's value text — matching
    // it would invent a number out of unrelated content.
    let status = b"SomeOther:\t VmRSS: 4242 kB\nVmRSS:\t 7 kB\n";
    assert_eq!(read_kib_field(status, b"VmRSS:"), Some(7));
}

#[test]
fn malformed_values_are_none_rather_than_a_guess() {
    assert_eq!(read_kib_field(b"VmRSS:\t kB\n", b"VmRSS:"), None);
    assert_eq!(read_kib_field(b"VmRSS:\n", b"VmRSS:"), None);
    // Non-whitespace junk before the digits: refuse instead of skipping it.
    assert_eq!(read_kib_field(b"VmRSS:\tx 12 kB\n", b"VmRSS:"), None);
}

#[test]
fn a_trailing_nondigit_before_the_unit_rejects_the_line() {
    // The numeric token must END where the unit begins: `12oops` must not
    // quietly become `12`.
    assert_eq!(read_kib_field(b"VmRSS:\t 12oops kB\n", b"VmRSS:"), None);
}

#[test]
fn a_decimal_point_is_not_an_integer() {
    // `12.5` truncated to `12` would under-report by half a unit with no sign.
    assert_eq!(read_kib_field(b"VmRSS:\t 12.5 kB\n", b"VmRSS:"), None);
}

#[test]
fn scientific_notation_is_not_an_integer() {
    // `1e6` truncated to `1` would misreport by six orders of magnitude.
    assert_eq!(read_kib_field(b"VmRSS:\t 1e6 kB\n", b"VmRSS:"), None);
}

#[test]
fn a_wrong_or_missing_unit_is_rejected_not_read_as_kib() {
    // procfs prints these lines as `%5lu kB`; treating any other spelling as
    // kB is exactly the silent unit-substitution misread (12 MB read as 12).
    assert_eq!(read_kib_field(b"VmRSS:\t 12 MB\n", b"VmRSS:"), None);
    assert_eq!(read_kib_field(b"VmRSS:\t 12 KB\n", b"VmRSS:"), None);
    assert_eq!(read_kib_field(b"VmRSS:\t 12 KiB\n", b"VmRSS:"), None);
    // Line ends right after the digits: no unit at all.
    assert_eq!(read_kib_field(b"VmRSS:\t 12\n", b"VmRSS:"), None);
    // Whitespace between digits and unit is required.
    assert_eq!(read_kib_field(b"VmRSS:\t 12kB\n", b"VmRSS:"), None);
}

#[test]
fn junk_after_the_unit_rejects_the_line() {
    assert_eq!(read_kib_field(b"VmRSS:\t 12 kB extra\n", b"VmRSS:"), None);
}

#[test]
fn plain_kib_values_still_parse() {
    // Positive control: the strict grammar must not reject what real kernels
    // actually print.
    assert_eq!(read_kib_field(b"VmRSS:\t 1234 kB\n", b"VmRSS:"), Some(1234));
    assert_eq!(read_kib_field(b"VmRSS:\t 0 kB\n", b"VmRSS:"), Some(0));
    assert_eq!(
        read_kib_field(b"VmRSS:\t 12345678 kB\n", b"VmRSS:"),
        Some(12345678)
    );
}

#[test]
fn trailing_whitespace_after_the_unit_is_accepted() {
    // No trailing newline: only ASCII whitespace may follow the unit.
    assert_eq!(read_kib_field(b"VmRSS:\t 12 kB ", b"VmRSS:"), Some(12));
}

#[test]
fn a_final_line_without_a_trailing_newline_still_parses() {
    assert_eq!(read_kib_field(b"VmRSS:\t 64 kB", b"VmRSS:"), Some(64));
}

#[test]
fn an_overflowing_value_is_none_rather_than_a_wrapped_number() {
    // 21 digits — beyond u64::MAX (20 digits). A wrapped result here would be
    // a fabricated measurement, which is worse than reporting nothing.
    let status = b"VmRSS:\t 999999999999999999999 kB\n";
    assert_eq!(read_kib_field(status, b"VmRSS:"), None);
}

#[test]
fn zero_is_a_real_value_and_not_confused_with_absence() {
    assert_eq!(read_kib_field(b"VmRSS:\t 0 kB\n", b"VmRSS:"), Some(0));
}

// ---------------------------------------------------------------------------
// The fused reader must agree with the per-field one, field for field.
// ---------------------------------------------------------------------------

use status_parse::read_kib_fields;

/// `read_kib_fields` is only allowed to change HOW MANY passes are made, not
/// what is read. If the two ever disagree, the P4-2 measurement comparing
/// them is meaningless and any switch between them is a silent behaviour
/// change.
#[test]
fn the_fused_reader_agrees_with_the_per_field_reader() {
    let cases: [&[u8]; 7] = [
        NON_UTF8_NAME,
        b"VmPeak:\t 9999 kB\nVmSize:\t 1111 kB\nVmRSS:\t 2222 kB\nVmHWM:\t 3333 kB\n",
        // Absent fields.
        b"VmSize:\t 1111 kB\n",
        // Malformed value, and a later duplicate line that must NOT be used
        // as a second chance by either reader.
        b"VmRSS:\tx 12 kB\nVmRSS:\t 77 kB\n",
        // No trailing newline.
        b"VmRSS:\t 64 kB",
        // Trailing junk before the unit: both readers must reject, not read 12.
        b"VmRSS:\t 12oops kB\n",
        // Wrong unit: a megabyte line must not be read as KiB.
        b"VmRSS:\t 12 MB\n",
    ];
    let prefixes: [&[u8]; 3] = [b"VmRSS:", b"VmSize:", b"VmHWM:"];
    for (i, status) in cases.iter().enumerate() {
        let fused = read_kib_fields(status, prefixes);
        for (j, prefix) in prefixes.iter().enumerate() {
            assert_eq!(
                fused[j],
                read_kib_field(status, prefix),
                "case {i}, field {}: fused and per-field readers disagree",
                String::from_utf8_lossy(prefix)
            );
        }
    }
}

/// The documented PRECONDITION of `read_kib_fields` — distinct,
/// non-overlapping prefixes — and the EXACT behavior when it is violated,
/// pinned as a known, documented limitation rather than left undiscoverable
/// (review round 2, P4-4). With duplicate prefixes, two independent
/// `read_kib_field` calls each see the line and both return `Some(7)`, but
/// the fused reader attributes the line to its FIRST matching prefix only,
/// so the second, duplicate prefix records `None`. Deliberately pins CURRENT
/// behavior: production never calls the fused reader (NO-GO measurement
/// subject), the real `FIELDS` are distinct and non-overlapping, and
/// changing the `break` would silently alter the exact function the
/// recorded scan-cost measurement measured. If you ever change the
/// function's contract, change this fixture and the doc precondition in the
/// same commit.
#[test]
fn duplicate_prefixes_are_a_documented_precondition_violation() {
    let status: &[u8] = b"VmRSS:\t 7 kB\n";
    // Two independent per-field scans each see the line.
    assert_eq!(read_kib_field(status, b"VmRSS:"), Some(7));
    assert_eq!(read_kib_field(status, b"VmRSS:"), Some(7));
    // The fused reader: the first duplicate wins the line, the second
    // never sees it.
    assert_eq!(
        read_kib_fields(status, [b"VmRSS:", b"VmRSS:"]),
        [Some(7), None]
    );
}
