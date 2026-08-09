//! Behavioral-oracle tests for `numa_shim::cpumap` (task #721).
//!
//! Before this task, the Linux sysfs cpumap parser (most-significant-word-
//! first hex bitmask decoding) lived entirely inside a `#[cfg(target_os =
//! "linux")]`-gated module and had zero direct tests anywhere -- the only
//! spec vectors existed as prose in doc comments, never as executable
//! assertions (per the original rust-intel audit's own §D1a finding). The
//! parsing functions are pure byte-slice logic with no OS dependency, so
//! `src/lib.rs` (task #721) extracted them into a target-independent,
//! `#[doc(hidden)] pub` module (`numa_shim::cpumap`) specifically so this
//! file can exercise them on ANY host, including this session's Windows one
//! -- not just a real Linux machine this project's CI does not currently run
//! these tests on either.

use numa_shim::cpumap::{
    format_sysfs_path, nth_token, parse_contains_cpu, parse_hex_u32, trim_end,
};

// ── parse_contains_cpu: the doc example, verbatim ───────────────────────────

/// The exact vector from `parse_contains_cpu`'s own doc comment:
/// `"00000000,00000003\n"` means CPUs 0 and 1 are in this node.
#[test]
fn doc_example_cpus_0_and_1_set() {
    let data = b"00000000,00000003\n";
    assert!(parse_contains_cpu(data, 0), "CPU 0 must be set");
    assert!(parse_contains_cpu(data, 1), "CPU 1 must be set");
    assert!(!parse_contains_cpu(data, 2), "CPU 2 must NOT be set");
    assert!(!parse_contains_cpu(data, 31), "CPU 31 must NOT be set");
}

// ── Word-order boundary: the exact shape task #720's fix protects ──────────

/// CPU 32 is the first bit of the SECOND word (0-indexed) -- but the format
/// is most-significant-word-first, so it must be read from the FIRST
/// (leftmost) token, not the second. This is the exact word-ordering logic
/// task #720's truncation fix depends on staying correct.
#[test]
fn cpu_32_is_bit_0_of_the_leftmost_word() {
    // word 0 (leftmost, covers CPUs 32-63) = 0x00000001 -> bit 0 = CPU 32 set.
    // word 1 (rightmost, covers CPUs 0-31)  = 0x00000000 -> no CPUs 0-31 set.
    let data = b"00000001,00000000\n";
    assert!(parse_contains_cpu(data, 32), "CPU 32 must be set");
    assert!(!parse_contains_cpu(data, 0), "CPU 0 must NOT be set");
    assert!(!parse_contains_cpu(data, 33), "CPU 33 must NOT be set");
}

/// The highest bit (31) of the leftmost word is the highest representable
/// CPU (63) for a 2-word mask.
#[test]
fn cpu_63_is_bit_31_of_the_leftmost_word() {
    let data = b"80000000,00000000\n";
    assert!(parse_contains_cpu(data, 63), "CPU 63 must be set");
    assert!(!parse_contains_cpu(data, 62), "CPU 62 must NOT be set");
}

/// A single-word mask (CPUs 0-31 only): CPU 0 is the LOWEST addressable
/// index, and CPU 31 is the highest within that one word.
#[test]
fn single_word_mask_cpu_0_and_cpu_31() {
    let data = b"00000001\n"; // no trailing comma: word_count == 1
    assert!(parse_contains_cpu(data, 0), "CPU 0 must be set");
    assert!(!parse_contains_cpu(data, 31), "CPU 31 must NOT be set");

    let data = b"80000000\n";
    assert!(parse_contains_cpu(data, 31), "CPU 31 must be set");
    assert!(!parse_contains_cpu(data, 0), "CPU 0 must NOT be set");
}

/// A CPU index beyond the mask's total word coverage (`target_word >=
/// word_count`) must return `false`, not panic or wrap.
#[test]
fn cpu_beyond_mask_coverage_returns_false() {
    let data = b"00000001\n"; // 1 word == CPUs 0..32 only
    assert!(!parse_contains_cpu(data, 32), "CPU 32 is out of range");
    assert!(!parse_contains_cpu(data, 1000), "CPU 1000 is out of range");
}

// ── Negative controls: malformed input must fail closed (false), never panic ─

#[test]
fn empty_token_is_rejected() {
    // CPU 0 reads from the LAST (rightmost) token -- a trailing comma with
    // nothing after it makes that specific token empty.
    let data = b"00000001,\n";
    assert!(
        !parse_contains_cpu(data, 0),
        "an empty hex token must not parse as a set bit"
    );
}

#[test]
fn invalid_hex_digit_is_rejected() {
    let data = b"0000000g\n"; // 'g' is not a hex digit
    assert!(
        !parse_contains_cpu(data, 0),
        "an invalid hex digit must not parse as a set bit"
    );
}

#[test]
fn empty_input_is_rejected() {
    assert!(!parse_contains_cpu(b"", 0));
    assert!(!parse_contains_cpu(b"\n", 0));
}

// ── trim_end ─────────────────────────────────────────────────────────────

#[test]
fn trim_end_strips_newline_cr_and_trailing_space() {
    assert_eq!(trim_end(b"abc\n"), b"abc");
    assert_eq!(trim_end(b"abc\r\n"), b"abc");
    assert_eq!(trim_end(b"abc "), b"abc");
    assert_eq!(trim_end(b"abc"), b"abc");
    assert_eq!(trim_end(b""), b"");
}

// ── nth_token ────────────────────────────────────────────────────────────

#[test]
fn nth_token_splits_on_separator() {
    let data = b"aa,bb,cc";
    assert_eq!(nth_token(data, 0, b','), Some(&b"aa"[..]));
    assert_eq!(nth_token(data, 1, b','), Some(&b"bb"[..]));
    assert_eq!(nth_token(data, 2, b','), Some(&b"cc"[..]));
    assert_eq!(nth_token(data, 3, b','), None);
}

#[test]
fn nth_token_single_token_no_separator() {
    let data = b"aa";
    assert_eq!(nth_token(data, 0, b','), Some(&b"aa"[..]));
    assert_eq!(nth_token(data, 1, b','), None);
}

// ── parse_hex_u32 ────────────────────────────────────────────────────────

#[test]
fn parse_hex_u32_accepts_lower_and_upper_case() {
    assert_eq!(parse_hex_u32(b"ff"), Some(0xff));
    assert_eq!(parse_hex_u32(b"FF"), Some(0xff));
    assert_eq!(parse_hex_u32(b"00000000"), Some(0));
    assert_eq!(parse_hex_u32(b"ffffffff"), Some(u32::MAX));
}

#[test]
fn parse_hex_u32_rejects_empty_and_invalid() {
    assert_eq!(parse_hex_u32(b""), None);
    assert_eq!(parse_hex_u32(b"xyz"), None);
    assert_eq!(parse_hex_u32(b"12g4"), None);
}

/// task #727 (rust-intel audit §B26): before this task, a token longer than
/// 8 hex digits silently WRAPPED via `wrapping_shl` (dropping the
/// most-significant nibbles) instead of returning `None` like every other
/// malformed input this parser rejects. `"1_00000000"`-shaped inputs never
/// occur in real sysfs cpumap files (fixed 8-char words), but the parser
/// should fail closed on them like it does on an invalid digit or an empty
/// token.
#[test]
fn parse_hex_u32_rejects_tokens_longer_than_8_digits() {
    assert_eq!(
        parse_hex_u32(b"ffffffff"),
        Some(u32::MAX),
        "8 digits: valid"
    );
    assert_eq!(
        parse_hex_u32(b"1ffffffff"),
        None,
        "9 digits must be rejected, not silently wrapped"
    );
    assert_eq!(parse_hex_u32(b"00000000000"), None, "11 digits: rejected");
}

// ── format_sysfs_path ────────────────────────────────────────────────────

#[test]
fn format_sysfs_path_node_0() {
    let mut buf = [0u8; 64];
    let path = format_sysfs_path(&mut buf, 0);
    assert_eq!(path, b"/sys/devices/system/node/node0/cpumap\0");
}

#[test]
fn format_sysfs_path_multi_digit_node() {
    let mut buf = [0u8; 64];
    let path = format_sysfs_path(&mut buf, 123);
    assert_eq!(path, b"/sys/devices/system/node/node123/cpumap\0");
}

/// task #727 (rust-intel audit §B7): before this task, `format_sysfs_path`'s
/// internal digit buffer was `[u8; 4]`, which panics (`tmp[digits]` out of
/// bounds) for any `node >= 10000` -- unreachable via the crate's own 0..64
/// caller bound, but reachable through this `#[doc(hidden)] pub` function
/// directly, which is exactly how this test reaches it. Proves the fix
/// (buffer sized for the full `u32` range) with the maximum possible value.
#[test]
fn format_sysfs_path_does_not_panic_on_five_plus_digit_node() {
    let mut buf = [0u8; 64];
    let path = format_sysfs_path(&mut buf, 10000);
    assert_eq!(path, b"/sys/devices/system/node/node10000/cpumap\0");

    let mut buf = [0u8; 64];
    let path = format_sysfs_path(&mut buf, u32::MAX);
    assert_eq!(path, b"/sys/devices/system/node/node4294967295/cpumap\0");
}
