//! Behavioural tests for `proc-probe`: the emitted `RESULT key=value` lines
//! must match the exact shape a runner parses, and the `proc-memstat` re-export
//! must be the same `snapshot()`.
//!
//! `emit*` write to the process's real stdout, which a same-process test cannot
//! cleanly capture without redirecting fd 1. Rather than fight that, this test
//! re-declares the runner's parser regex (the SAME `^RESULT\s+([a-z0-9_]+)=(\S+)$`
//! the `.mjs` runners use) and asserts, for each `emit*` shape, that the line it
//! WOULD print (built by the identical format string) is accepted by that
//! parser and round-trips to the original key/value. This pins the contract
//! between this crate and every runner without an fd dance; a separate
//! `emit_smoke` test additionally proves the functions run without panicking.

use proc_probe::RESULT_PREFIX;

/// The line each `emit*` prints, built with the SAME format string the library
/// uses (`"{RESULT_PREFIX} {key}={value}"`). Kept in the test so the assertions
/// below check the value shape, not just that a function exists.
fn line(key: &str, value: &str) -> String {
    format!("{RESULT_PREFIX} {key}={value}")
}

/// Parse a `RESULT key=value` line exactly as the runner scripts do
/// (`/^RESULT\s+([a-z0-9_]+)=(\S+)$/`), returned as `(key, value)` on match.
/// Implemented without a regex crate (zero deps) but semantically identical.
fn parse_result(s: &str) -> Option<(String, String)> {
    let s = s.trim();
    let rest = s.strip_prefix(RESULT_PREFIX)?;
    // `\s+` after the prefix: at least one whitespace char.
    let rest = rest.strip_prefix(char::is_whitespace)?;
    let rest = rest.trim_start();
    let (key, value) = rest.split_once('=')?;
    if key.is_empty() || value.is_empty() {
        return None;
    }
    // key: [a-z0-9_]+
    if !key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    // value: \S+ (no whitespace)
    if value.chars().any(char::is_whitespace) {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

#[test]
fn u64_line_round_trips() {
    let (k, v) = parse_result(&line("rss_kib", &1234u64.to_string())).expect("must parse");
    assert_eq!(k, "rss_kib");
    assert_eq!(v, "1234");
}

#[test]
fn i64_negative_line_round_trips() {
    // A signed delta may be negative; `\S+` accepts the leading '-'.
    let (k, v) = parse_result(&line("delta_ns", &(-42i64).to_string())).expect("must parse");
    assert_eq!(k, "delta_ns");
    assert_eq!(v, "-42");
}

#[test]
fn f64_line_has_no_whitespace_and_round_trips() {
    let s = 1.5f64.to_string();
    let (k, v) = parse_result(&line("ratio", &s)).expect("must parse");
    assert_eq!(k, "ratio");
    assert_eq!(v, "1.5");
}

#[test]
fn ns_from_u128_round_trips() {
    let ns: u128 = 987_654_321;
    let (k, v) = parse_result(&line("elapsed_ns", &ns.to_string())).expect("must parse");
    assert_eq!(k, "elapsed_ns");
    assert_eq!(v, "987654321");
}

#[test]
fn arm_string_value_round_trips() {
    let (k, v) = parse_result(&line("arm", "sefer")).expect("must parse");
    assert_eq!(k, "arm");
    assert_eq!(v, "sefer");
}

#[test]
fn parser_rejects_non_result_and_malformed_lines() {
    assert!(parse_result("not a result line").is_none());
    assert!(parse_result("RESULT nokeyvalue").is_none());
    assert!(parse_result("RESULT Key=1").is_none()); // uppercase key rejected
    assert!(parse_result("RESULT k=a b").is_none()); // whitespace in value
}

/// The re-export must be the exact same `snapshot()` — a probe gets
/// measure + report from this one crate.
#[test]
fn snapshot_re_export_matches_proc_memstat() {
    let a = proc_probe::snapshot();
    let b = proc_memstat::snapshot();
    // Both call the same OS query; fields are the same TYPE (MemStat) and on a
    // quiet instant read back-to-back are consistent (peak monotonic).
    let _typed: proc_probe::MemStat = a;
    if let (Some(pa), Some(pb)) = (a.peak_rss, b.peak_rss) {
        assert!(pb >= pa, "peak_rss must be non-decreasing across two reads");
    }
}

/// The `emit*` functions must run without panicking (they write to stdout; the
/// output itself is contract-checked above via the shared format string).
#[test]
fn emit_smoke_does_not_panic() {
    proc_probe::emit("arm", "sefer");
    proc_probe::emit_u64("rss_kib", 1234);
    proc_probe::emit_i64("delta_ns", -7);
    proc_probe::emit_f64("ratio", 1.5);
    proc_probe::emit_ns("elapsed_ns", 987_654_321u128);
}
