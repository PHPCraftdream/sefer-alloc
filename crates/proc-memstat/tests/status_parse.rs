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
