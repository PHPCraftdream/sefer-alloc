//! Behavioural tests for `proc-probe`: the emitted `RESULT key=value` lines
//! must match the exact shape a runner parses, and the `proc-memstat` re-export
//! must be the same `snapshot()`.
//!
//! The contract is checked at THREE independent layers:
//!
//! 1. **Parser-side model** (`line()` / `parse_result()` + the round-trip
//!    tests): a zero-dependency model of the runner's
//!    `line.trim()` + `/^RESULT\s+([a-z0-9_]+)=(\S+)$/` contract, exercised
//!    for self-consistent shape checks (key/value round-trips, malformed-line
//!    rejection). `line()` rebuilds the expected line from the library's own
//!    `RESULT_PREFIX`, which is fine HERE — these tests only pin the model,
//!    not the library's real bytes.
//!
//! 2. **Real emitted bytes** (`real_stdout_emit_family_exact_bytes`): the
//!    REAL `emit*` functions run in a freshly re-exec'd child process and
//!    their stdout bytes are compared against INDEPENDENTLY HARDCODED
//!    expectations (the literal word `RESULT`, not `RESULT_PREFIX`). This
//!    closes **P2-2** (Sol-codex round-1 review): the previous suite never
//!    inspected real `emit*` output — emptying every `emit*` body or renaming
//!    `RESULT_PREFIX` to `"RESULTX"` passed every test. The child runs with
//!    `--format terse`, so libtest's own framing can never share a physical
//!    line with the emitted bytes (P2-1, round-2 review);
//!    `real_stdout_emit_family_exact_bytes_serial_child` pins the
//!    forced-serial (`RUST_TEST_THREADS=1` on the child) case.
//!
//! 3. **Model vs the real ECMAScript parser**
//!    (`parser_contract_matches_node_ecmascript_regex`): the Rust model is
//!    compared line-by-line against the ACTUAL regex from
//!    `scripts/paired-ab-runner.mjs:253`, executed by real Node.js. This
//!    closes **P3-2** (Sol-codex round-1 review): the model was previously
//!    claimed "semantically identical" to the ECMAScript parser, but
//!    Rust `char::is_whitespace` (Unicode `White_Space`) and ECMAScript
//!    `\s`/`trim()` disagree on the U+FEFF/U+0085 boundary — those six
//!    divergences are now explicitly documented, pinned, and node-verified
//!    rather than claimed away.
//!
//! The `emit*`-family and re-export tests additionally require the default
//! `std` feature (they do not exist without it).

use proc_probe::RESULT_PREFIX;
use std::io::Write;

/// The line each `emit*` prints, built with the SAME format string the library
/// uses (`"{RESULT_PREFIX} {key}={value}"`).
///
/// This helper exists ONLY for the parser-model round-trip tests below. It is
/// deliberately NOT used by [`real_stdout_emit_family_exact_bytes`], which
/// hardcodes its expected bytes independently (the literal word `RESULT`),
/// because an expectation rebuilt from the library's own prefix/format would
/// pass even if the library emitted garbage — exactly the P2-2 finding.
fn line(key: &str, value: &str) -> String {
    format!("{RESULT_PREFIX} {key}={value}")
}

/// Parse a `RESULT key=value` line as the runner scripts do
/// (`line.trim()` + `/^RESULT\s+([a-z0-9_]+)=(\S+)$/`), returned as
/// `(key, value)` on match. Implemented without a regex crate (zero deps).
///
/// This models the ECMAScript contract EXACTLY on the ASCII alphabet, but it
/// is NOT identical to ECMAScript outside it: the whitespace used here
/// (`str::trim`, `char::is_whitespace`) is Unicode `White_Space`, while
/// ECMAScript `\s`/`trim()` use the WhiteSpace+LineTerminator sets —
/// U+0085 (NEL) is whitespace to Rust but not to ECMAScript; U+FEFF (BOM) is
/// whitespace to ECMAScript but not to Rust. The six boundary divergences are
/// pinned and node-verified by
/// [`parser_contract_matches_node_ecmascript_regex`] (P3-2).
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

    // u128::MAX closes exactly the coverage gap the Sol-codex round-1 review
    // (P2-2) found: the previous fixture (`987_654_321`) fits u32, so an
    // accidental u128→u64/u32 narrowing in `emit_ns` was unverifiable here.
    // The current `emit_ns` does NOT narrow, so this guards a guarantee, it
    // does not fix a present bug. Mirrors the real-stdout test's fixture.
    let max: u128 = u128::MAX;
    let (k, v) = parse_result(&line("elapsed_ns_max", &max.to_string())).expect("must parse");
    assert_eq!(k, "elapsed_ns_max");
    assert_eq!(v, "340282366920938463463374607431768211455");
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
///
/// Requires the `std` feature: the `snapshot`/`MemStat` re-export is
/// std-only (`proc-memstat` is an std crate).
#[cfg(feature = "std")]
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

/// The fallible re-export must be reachable through `proc-probe` alone
/// (P3-4): a one-dependency probe can distinguish a failed measurement from a
/// genuinely near-zero one without naming `proc-memstat`, and the error type
/// is nameable and `Display`-able through the re-export.
///
/// Requires the `std` feature: the `try_snapshot`/`SnapshotError` re-export
/// is std-only, exactly like `snapshot`/`MemStat`.
#[cfg(feature = "std")]
#[test]
fn try_snapshot_re_export_reachable() {
    match proc_probe::try_snapshot() {
        Ok(m) => {
            let _typed: proc_probe::MemStat = m;
        }
        // `SnapshotError` is `#[non_exhaustive]`, so a wildcard arm is
        // mandatory from outside `proc-memstat`; binding it here proves the
        // error type itself crossed the re-export.
        Err(e) => {
            let _msg: String = e.to_string();
        }
    }
}

/// The `emit*` functions must run without panicking. This smoke test does NOT
/// inspect their output — that contract (real stdout bytes vs independently
/// hardcoded expectations) is checked by
/// [`real_stdout_emit_family_exact_bytes`].
///
/// Requires the `std` feature: every `emit*` function is std-only.
#[cfg(feature = "std")]
#[test]
fn emit_smoke_does_not_panic() {
    proc_probe::emit("arm", "sefer");
    proc_probe::emit_u64("rss_kib", 1234);
    proc_probe::emit_i64("delta_ns", -7);
    proc_probe::emit_f64("ratio", 1.5);
    proc_probe::emit_ns("elapsed_ns", 987_654_321u128);
}

// ---------------------------------------------------------------------------
// Layer 2: the REAL `emit*` bytes, checked in a fresh process.
// ---------------------------------------------------------------------------

#[cfg(feature = "std")]
mod real_emit {
    /// Marker env var, set ONLY on the freshly-spawned child: the marked
    /// process runs the emit body directly instead of spawning yet another
    /// copy of itself (the runner/child split from
    /// `crates/proc-memstat/tests/non_utf8_name.rs`).
    const CHILD_MARKER: &str = "PROC_PROBE_PROTOCOL_EMIT_CHILD";

    /// The `--exact` filter selecting this test inside the re-exec'd binary.
    /// Hardcoded rather than `concat!(module_path!(), ...)`: in an integration
    /// test `module_path!()` includes the test-crate name (`protocol::...`),
    /// while libtest's own test paths omit it — a `module_path!()`-built
    /// filter matches nothing and the child silently runs 0 tests.
    ///
    /// BOTH runner tests below re-exec with this exact filter, so the child
    /// branch always runs inside `real_stdout_emit_family_exact_bytes`;
    /// `real_stdout_emit_family_exact_bytes_serial_child` is runner-only by
    /// construction.
    const TEST_NAME: &str = "real_emit::real_stdout_emit_family_exact_bytes";

    /// Independently HARDCODED expectations — the literal word `RESULT`, NOT
    /// built from `RESULT_PREFIX` or the library format string. These are the
    /// exact decimal spellings of `u64::MAX`, `i64::MIN`, `1.5f64` Display,
    /// and `u128::MAX`; a narrowing regression such as `ns as u64` would emit
    /// u64::MAX's digits and mismatch. A `RESULT_PREFIX` rename to `"RESULTX"`
    /// or an empty `emit*` body makes exactly this test (and only this test)
    /// fail — the P2-2 finding.
    const EXPECTED_BLOCK: &str = concat!(
        "RESULT arm=sefer\n",
        "RESULT rss_kib_max=18446744073709551615\n",
        "RESULT delta_neg=-42\n",
        "RESULT i64_min=-9223372036854775808\n",
        "RESULT ratio=1.5\n",
        "RESULT elapsed_ns_max=340282366920938463463374607431768211455\n",
    );

    /// The child branch: the REAL `emit*` calls, in this exact order. Shared
    /// by every runner scenario below (the `--exact` filter above selects
    /// this body in the re-exec'd child regardless of which runner test
    /// spawned it).
    fn emit_family() {
        proc_probe::emit("arm", "sefer");
        proc_probe::emit_u64("rss_kib_max", u64::MAX);
        proc_probe::emit_i64("delta_neg", -42);
        proc_probe::emit_i64("i64_min", i64::MIN);
        proc_probe::emit_f64("ratio", 1.5);
        // elapsed_ns_max = u128::MAX closes exactly the coverage gap the
        // Sol-codex round-1 review (P2-2) found — the previous test only
        // used `987_654_321` (fits u32), so an accidental u128→u64/u32
        // narrowing in `emit_ns` was unverifiable; the current `emit_ns`
        // does NOT narrow, so this guards a guarantee, it does not fix a
        // present bug.
        proc_probe::emit_ns("elapsed_ns_max", u128::MAX);
    }

    /// Re-exec this test binary as a libtest child running ONLY
    /// [`TEST_NAME`], and return its captured stdout.
    ///
    /// **Why `--format terse` is REQUIRED on the child** (Sol-codex round-2
    /// review P2-1): without it the child's libtest selects its PRETTY
    /// formatter, and when the child is SINGLE-threaded that formatter prints
    /// `test <name> ... ` with NO trailing newline BEFORE the test body —
    /// verified against the Rust 1.88 sources
    /// (`PrettyFormatter::write_test_start` in
    /// `library/test/src/formatters/pretty.rs`; the selector
    /// `is_multithreaded = opts.test_threads.unwrap_or_else(get_concurrency)
    /// \> 1` in `library/test/src/console.rs`; `get_concurrency` reads
    /// `RUST_TEST_THREADS` first in
    /// `library/test/src/helpers/concurrency.rs`). The child inherits the
    /// outer environment, so an outer `RUST_TEST_THREADS=1` (a common CI/dev
    /// setting for deterministic sequential runs) — or
    /// `available_parallelism() == 1` — triggers exactly this; the first
    /// emitted line then physically reads
    /// `test real_emit::... ... RESULT arm=sefer`, the
    /// `starts_with("RESULT ")` filter silently drops it, and the test fails
    /// although every `emit*` printed exactly correct bytes. The TERSE
    /// formatter writes the pre-body name only for padded BENCHMARKS
    /// (`write_test_start` guards on `NamePadding::PadOnRight`,
    /// `library/test/src/formatters/terse.rs`): for a normal test in EITHER
    /// thread mode nothing is written between the LF-terminated
    /// `running N tests` header and the body, and the post-body result is a
    /// bare `.` — so every emitted line starts at column 0 regardless of the
    /// child's thread count. `--format terse` is stable libtest CLI and only
    /// changes the harness framing, never the emitted bytes.
    ///
    /// `extra_child_env` is a single `(key, value)` pair set ON TOP of the
    /// inherited environment; the serial-child regression test below uses it
    /// to force `RUST_TEST_THREADS=1` on the child deterministically.
    fn emit_child_stdout(extra_child_env: Option<(&str, &str)>) -> String {
        let exe = std::env::current_exe().expect("current_exe resolves this test binary");
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(["--exact", TEST_NAME, "--nocapture", "--format", "terse"])
            .env(CHILD_MARKER, "1");
        if let Some((key, value)) = extra_child_env {
            cmd.env(key, value);
        }
        let out = cmd
            .output()
            .unwrap_or_else(|e| panic!("failed to re-spawn this binary for {TEST_NAME}: {e}"));
        assert!(
            out.status.success(),
            "emit child failed ({status}):\n\
             --- child stdout ---\n{stdout}\n\
             --- child stderr ---\n{stderr}",
            status = out.status,
            stdout = String::from_utf8_lossy(&out.stdout),
            stderr = String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout)
            .expect("emit* write UTF-8; the harness adds no non-UTF-8 framing")
    }

    /// The full two-part oracle over the child's real stdout. UNCHANGED by
    /// the P2-1 fix — the fix changed only the child's libtest formatter,
    /// never the strictness of these assertions.
    fn assert_expected_result_lines(stdout: &str) {
        // 1. Contiguity, order, and the exact LF-terminated line shape: a
        //    `\r\n` or a missing trailing `\n` breaks this contains match.
        assert!(
            stdout.contains(EXPECTED_BLOCK),
            "real stdout does not contain the expected emit block:\n{stdout:?}"
        );

        // 2. No EXTRA `RESULT `-prefixed line anywhere in the child's real
        //    stdout, and no expected line missing. Both the `"RESULT "`
        //    filter and `EXPECTED_BLOCK` are hardcoded literals independent
        //    of `RESULT_PREFIX`, so a prefix rename or an empty emit body
        //    makes exactly this test (and only this test) fail.
        let emitted: Vec<&str> = stdout
            .lines()
            .filter(|l| l.starts_with("RESULT "))
            .collect();
        let expected: Vec<&str> = EXPECTED_BLOCK.lines().collect();
        assert_eq!(
            emitted, expected,
            "RESULT lines in the child's real stdout diverge from the hardcoded block"
        );
    }

    /// The real `emit*` functions must land on the child's real stdout as exact
    /// bytes matching independently hardcoded expectations (P2-2).
    ///
    /// **Why a re-exec** (the `crates/proc-memstat/tests/non_utf8_name.rs`
    /// pattern): `emit*` write to the process's real stdout, which a
    /// same-process libtest cannot cleanly observe — the harness captures
    /// test output through a pipe it owns.
    ///
    /// **Why `--nocapture` is REQUIRED on the child**: without it the child's
    /// libtest harness swallows the `emit*` writes into its capture buffer
    /// and DISCARDS them on success, so the pipe we read would never see
    /// them. With `--nocapture` the harness forwards writes straight to the
    /// inherited stdout — which, because we spawned the child with
    /// `.output()`, is the pipe we then read.
    ///
    /// **Why `--format terse` is REQUIRED on the child**: see
    /// [`emit_child_stdout`] (P2-1 — the serial-child pretty formatter would
    /// merge libtest's own `test ... ... ` framing into the first metric
    /// line).
    #[test]
    fn real_stdout_emit_family_exact_bytes() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            // Child branch: call the REAL functions in this exact order.
            emit_family();
            return;
        }

        // Runner branch: re-exec this test binary with the marker set.
        let stdout = emit_child_stdout(None);
        assert_expected_result_lines(&stdout);
    }

    /// P2-1 regression (Sol-codex round-2 review): the emitted bytes must
    /// survive a FORCED single-threaded child. `RUST_TEST_THREADS=1` is set
    /// via `.env(...)` on the child's `Command` — deterministic, independent
    /// of the outer run's environment — which is exactly the condition under
    /// which the pre-fix child's PRETTY formatter printed `test <name> ... `
    /// (no trailing newline) before the body and silently dropped
    /// `RESULT arm=sefer` from the strict line-filter assertion. Verified
    /// counterfactually before the fix: the pre-fix child under this exact
    /// scenario fails with the first expected line missing.
    ///
    /// Runner-only by construction: the child re-exec's `--exact` filter
    /// selects [`TEST_NAME`], so this test's own body always takes the
    /// runner branch. Together with [`real_stdout_emit_family_exact_bytes`]
    /// this covers the terse formatter in BOTH child thread modes
    /// (multithreaded default and forced serial).
    #[test]
    fn real_stdout_emit_family_exact_bytes_serial_child() {
        let stdout = emit_child_stdout(Some(("RUST_TEST_THREADS", "1")));
        assert_expected_result_lines(&stdout);
    }
}
// ---------------------------------------------------------------------------
// Layer 3: the parser model vs the real ECMAScript regex, via real Node.js.
// ---------------------------------------------------------------------------

/// Lines on which the Rust model and the real ECMAScript parser MUST agree
/// (`parse_result(line).is_some()` == (`re.exec(line.trim()) !== null`)).
///
/// Corpus lines deliberately contain NO `\n`/`\r`: the runner's `/\r?\n/`
/// framing split (`scripts/paired-ab-runner.mjs:252`) is NOT reproduced here —
/// only the per-line contract `line.trim()` + regex (line 253) is. The
/// exotic-space accept cases document WHICH characters both whitespace models
/// share; U+180E is in neither modern set (Unicode 6.3 moved it out of
/// Zs/White_Space).
const NODE_AGREEMENT_LINES: &[&str] = &[
    // accepts
    "RESULT rss_kib=1234",
    "RESULT arm=sefer",
    "RESULT k_1=a-b.c",
    "RESULT k=1=2", // '=' is legal inside the value token
    "  RESULT k=1", // leading spaces: both trims remove
    "RESULT k=1\t", // trailing tab
    "\t RESULT k=1 \t",
    "RESULT\tk=1", // \s+ separator, ASCII tab
    "RESULT \t k=1",
    "RESULT\u{00A0}k=1",   // NBSP: whitespace in BOTH models
    "RESULT\u{1680}k=1",   // OGHAM SPACE MARK
    "RESULT\u{2003}k=1",   // EN SPACE
    "RESULT\u{2028}k=1",   // LINE SEPARATOR
    "RESULT\u{2029}k=1",   // PARAGRAPH SEPARATOR
    "RESULT\u{202F}k=1",   // NARROW NO-BREAK SPACE
    "RESULT\u{205F}k=1",   // MEDIUM MATHEMATICAL SPACE
    "RESULT\u{3000}k=1",   // IDEOGRAPHIC SPACE
    "RESULT\u{000B}k=1",   // vertical tab
    "RESULT\u{000C}k=1",   // form feed
    "RESULT k=a\u{00A0}b", // NBSP inside the value: rejected by BOTH
    "RESULT k=\u{1F680}", // astral value: accepted by BOTH (surrogate pair is neither \s nor White_Space)
    // rejects
    "not a result line",
    "RESULT nokeyvalue",
    "RESULT Key=1", // uppercase key
    "result k=1",   // lowercase prefix
    "RESULTX k=1",  // renamed-prefix shape: 'X' breaks BOTH
    "RESULTX=1",
    "RESULT k=a b",      // space inside value
    "RESULT k=",         // empty value
    "RESULT =1",         // empty key
    "RESULT\u{180E}k=1", // MONGOLIAN VOWEL SEPARATOR: whitespace in NEITHER modern engine (Unicode 6.3 moved it out of Zs/White_Space)
];

/// (line, what Rust's `parse_result` must return) — the EXPLICITLY DOCUMENTED
/// divergences between the two whitespace models (P3-2): ECMAScript
/// `\s`/`trim` accept U+FEFF and reject U+0085; Unicode `White_Space` — what
/// `char::is_whitespace` implements — is the exact reverse. The `bool` pins
/// the Rust side too, so a future "fix" of `parse_result` flips these loudly
/// and forces the divergence docs to be updated.
const NODE_DIVERGENCE_LINES: &[(&str, bool)] = &[
    ("\u{FEFF}RESULT k=1", false), // JS trims the BOM -> accept; Rust trim keeps it -> reject
    ("RESULT\u{FEFF}k=1", false), // JS \s+ matches BOM -> accept; Rust is_whitespace(BOM)=false -> reject
    ("RESULT k=a\u{FEFF}b", true), // JS \S+ rejects (BOM is \s); Rust accepts (BOM is not Rust-whitespace)
    ("\u{0085}RESULT k=1", true),  // Rust trim removes NEL -> accept; JS trim keeps it -> reject
    ("RESULT\u{0085}k=1", true),   // Rust \s-model matches NEL -> accept; JS \s does not -> reject
    ("RESULT k=a\u{0085}b", false), // Rust: NEL is Rust-whitespace -> reject; JS: NEL is \S -> accept
];

/// The documented runner contract this test's corpus and divergence fixtures
/// were built against: the regex literal BODY at
/// `scripts/paired-ab-runner.mjs:253` (inside `parseResult`, lines 250–257),
/// no flags, and `line.trim()` applied immediately before `.exec()`.
/// Byte-for-byte confirmed 2026-09-09. Identical regex copies also exist at
/// `scripts/r10_5_large_cache_gate.mjs:114`,
/// `scripts/r34_7_causal_harness.mjs:128` and
/// `scripts/r34_23_vec_harness.mjs:117` — only the paired-ab-runner file is
/// drift-guarded below.
const EXPECTED_RUNNER_REGEX_BODY: &str = "^RESULT\\s+([a-z0-9_]+)=(\\S+)$";

/// The ACTIVE runner parsing contract, extracted mechanically from the
/// runner's source text (Sol-codex round-2 review P3-1): the regex literal
/// body, any flags after the closing slash, and whether `.exec()` is called
/// on `line.trim()` — the three things the old substring guard could NOT see.
#[derive(Debug, PartialEq, Eq)]
struct RunnerContract {
    /// The literal's body between the slashes.
    regex_body: String,
    /// JS regex flags after the closing slash ("" today).
    flags: String,
    /// Whether `.exec()` is called on `line.trim()`.
    trims: bool,
}

/// Extract the ACTIVE contract from runner source text: find the regex
/// literal anchored on `/^RESULT` whose flags are immediately followed by
/// `.exec(` (the production call shape); read the body up to the first
/// UNESCAPED closing `/`, then ASCII-alphabetic flags, then require exactly
/// `.exec(` and read the paren-balanced argument; `trims` is whether that
/// argument ends with `.trim()`.
///
/// Returns `None` when the anchor+call shape is absent (the runner
/// restructured its parser — a drift the caller must report loudly). Uses
/// `str::get` (checked slicing) throughout so exotic future literal content
/// can never panic mid-char. Escaped slashes (`\/`) inside the body do not
/// terminate it. Each occurrence of the anchor is attempted independently;
/// an occurrence whose flags are not immediately followed by `.exec(` (e.g.
/// the literal used with `.test(`) is skipped in favor of the next one.
fn extract_runner_contract(runner_src: &str) -> Option<RunnerContract> {
    const ANCHOR: &str = "/^RESULT";
    let mut search_from = 0;
    while let Some(rel) = runner_src[search_from..].find(ANCHOR) {
        let lit_start = search_from + rel;
        search_from = lit_start + ANCHOR.len();
        // Body: from after the anchor's opening slash to the first UNESCAPED
        // closing '/'. Walk bytewise with checked slicing (str::get).
        let mut i = lit_start + ANCHOR.len();
        let mut body_end = None;
        while i < runner_src.len() {
            let b = runner_src.as_bytes()[i];
            if b == b'\\' {
                i += 2; // skip the escaped char wholesale (never panics: the
                        // loop bound re-checks `i < len`)
                continue;
            }
            if b == b'/' {
                body_end = Some(i);
                break;
            }
            i += 1;
        }
        let body_end = body_end?;
        let regex_body = runner_src.get(lit_start + 1..body_end)?.to_string();
        // Flags: ASCII-alphabetic chars immediately after the closing slash.
        let mut j = body_end + 1;
        while runner_src
            .get(j..j + 1)
            .and_then(|c| c.bytes().next())
            .is_some_and(|b| b.is_ascii_alphabetic())
        {
            j += 1;
        }
        let flags = runner_src.get(body_end + 1..j)?.to_string();
        // The production call shape: `.exec(` immediately after the flags.
        if runner_src.get(j..j + 6) != Some(".exec(") {
            continue; // not the active callable (e.g. used with `.test(`)
        }
        // Read the paren-balanced argument.
        let arg_start = j + 6;
        let mut depth = 1usize;
        let mut k = arg_start;
        while k < runner_src.len() {
            match runner_src.as_bytes()[k] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        if depth != 0 {
            return None;
        }
        let arg = runner_src.get(arg_start..k)?;
        return Some(RunnerContract {
            regex_body,
            flags,
            trims: arg.ends_with(".trim()"),
        });
    }
    None
}

/// The single drift-guard entry point used by the interop test AND both
/// negative-control tests: extract the active contract and compare it to the
/// documented one, returning a drift message naming the moved field
/// (Sol-codex round-2 review P3-1).
fn runner_contract_drift(runner_src: &str) -> Result<RunnerContract, String> {
    let contract = extract_runner_contract(runner_src).ok_or_else(|| {
        "cannot locate the `/^RESULT.../...exec(...)` parse contract in \
         scripts/paired-ab-runner.mjs; the runner's parseResult restructured — \
         re-sync EXPECTED_RUNNER_REGEX_BODY and the extraction in this test \
         (Sol-codex round-2 P3-1)"
            .to_string()
    })?;
    if contract.regex_body != EXPECTED_RUNNER_REGEX_BODY {
        return Err(format!(
            "runner regex body drifted: documented {EXPECTED_RUNNER_REGEX_BODY:?}, \
             found {found:?} (Sol-codex round-2 P3-1)",
            found = contract.regex_body
        ));
    }
    if !contract.flags.is_empty() {
        return Err(format!(
            "runner regex flags drifted: documented no flags, found {:?} — \
             e.g. an added 'i' flag would silently make the parser \
             case-insensitive, exactly the drift the old substring guard \
             could not see (Sol-codex round-2 P3-1)",
            contract.flags
        ));
    }
    if !contract.trims {
        return Err(
            "runner preprocessing drifted: documented `line.trim()` immediately \
             before `.exec()`, found no trim immediately before .exec( — \
             whitespace handling would have silently drifted under the old \
             substring guard (Sol-codex round-2 P3-1)"
                .to_string(),
        );
    }
    Ok(contract)
}

/// Minimal verbatim copy of the runner's parseResult
/// (scripts/paired-ab-runner.mjs:250–257) used to build synthetic variants
/// for the drift-guard negative controls. NEVER the real file: the controls
/// must not modify or even require it (they run in every configuration,
/// including --no-default-features and published-crate checkouts).
const SYNTHETIC_PARSE_RESULT_SRC: &str = r#"function parseResult(out) {
  const r = {};
  for (const line of out.split(/\r?\n/)) {
    const m = /^RESULT\s+([a-z0-9_]+)=(\S+)$/.exec(line.trim());
    if (m) r[m[1]] = /^-?\d+$/.test(m[2]) ? Number(m[2]) : m[2];
  }
  return r;
}"#;

/// Minimal JSON string escaper (no serde dependency): quotes and escapes
/// `"` `\` `\n` `\r` `\t`, other chars < 0x20 as `\uXXXX`, everything else —
/// including non-ASCII — passed through as UTF-8. Valid JSON, and
/// `JSON.parse` reads it back identically.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The Rust parser model must match the REAL runner regex — executed by real
/// Node.js (`re.exec(line.trim()) !== null`, exactly the production call
/// shape at `scripts/paired-ab-runner.mjs:253`) — on a shared corpus, and the
/// six documented U+FEFF/U+0085 divergences must hold in both directions.
///
/// Not std-feature-gated: uses only the test crate's std and `RESULT_PREFIX`.
#[test]
fn parser_contract_matches_node_ecmascript_regex() {
    // Locate the runner source. A published-crate checkout has no `scripts/`:
    // SKIP (stderr notice + return), never fail, on NotFound.
    let runner_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/paired-ab-runner.mjs");
    let runner_src = match std::fs::read_to_string(&runner_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            report_skip(&format!(
                "parser_contract_matches_node_ecmascript_regex: runner not \
                 found at {}",
                runner_path.display()
            ));
            return;
        }
        Err(e) => panic!("failed to read {}: {e}", runner_path.display()),
    };

    let contract = match runner_contract_drift(&runner_src) {
        Ok(c) => c,
        Err(drift) => panic!("drift guard: {drift}"),
    };

    // Node availability: absent node is a SKIP; a failing `node --version`
    // is a real environment error and panics.
    let node_version_out = match std::process::Command::new("node").arg("--version").output() {
        Ok(o) => o,
        Err(e) => match node_spawn_skip_reason(&e) {
            Some(reason) => {
                report_skip(&format!(
                    "parser_contract_matches_node_ecmascript_regex: {reason}"
                ));
                return;
            }
            None => panic!(
                "`node --version` failed to spawn: {e} — a found-but-broken node \
                 is a real environment problem, not an absent optional tool \
                 (Sol-codex round-2 review P3-2a)"
            ),
        },
    };
    assert!(
        node_version_out.status.success(),
        "`node --version` exited non-zero: {}",
        String::from_utf8_lossy(&node_version_out.stderr)
    );
    let node_version = String::from_utf8_lossy(&node_version_out.stdout)
        .trim()
        .to_string();

    let all_lines: Vec<&str> = NODE_AGREEMENT_LINES
        .iter()
        .copied()
        .chain(NODE_DIVERGENCE_LINES.iter().map(|(l, _)| *l))
        .collect();

    // Corpus JSON in a unique temp file (proc-memstat's unique-per-run
    // pattern), written via the local escaper — no serde dependency.
    let corpus_path = std::env::temp_dir().join(format!(
        "proc_probe_p3_2_corpus_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is after the epoch")
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&corpus_path); // best-effort, pre-existing leftovers
    let mut json = String::from("{\"regex\":");
    json.push_str(&json_string(&contract.regex_body));
    json.push_str(",\"flags\":");
    json.push_str(&json_string(&contract.flags));
    json.push_str(",\"trim\":");
    json.push_str(if contract.trims { "true" } else { "false" });
    json.push_str(",\"lines\":[");
    for (i, l) in all_lines.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&json_string(l));
    }
    json.push_str("]}");
    std::fs::write(&corpus_path, json).expect("write corpus temp file");

    let verdicts = run_node_corpus_check(&corpus_path, all_lines.len());

    // Best-effort cleanup of the temp corpus.
    let _ = std::fs::remove_file(&corpus_path);

    for (i, l) in NODE_AGREEMENT_LINES.iter().enumerate() {
        assert_eq!(
            parse_result(l).is_some(),
            verdicts.as_bytes()[i] == b'A',
            "agreement mismatch at corpus line {i} ({l:?}): rust={}, js={}",
            parse_result(l).is_some(),
            verdicts.as_bytes()[i] == b'A',
        );
    }
    let off = NODE_AGREEMENT_LINES.len();
    for (i, (l, rust_accepts)) in NODE_DIVERGENCE_LINES.iter().enumerate() {
        let rust_verdict = parse_result(l).is_some();
        let js_verdict = verdicts.as_bytes()[off + i] == b'A';
        assert_eq!(
            rust_verdict, *rust_accepts,
            "divergence fixture {i} ({l:?}): the recorded Rust expectation moved; \
             update NODE_DIVERGENCE_LINES and its docs"
        );
        assert!(
            rust_verdict != js_verdict,
            "divergence fixture {i} ({l:?}): expected the models to DISAGREE \
             (U+FEFF/U+0085: ECMAScript \\s/trim accept U+FEFF and reject U+0085; \
             Unicode White_Space — what char::is_whitespace implements — is the \
             exact reverse). This gap is DOCUMENTED and node-verified, not claimed \
             away as 'identical'."
        );
    }

    // Diagnostic: proves that node actually ran (version + raw verdict
    // string) rather than the test silently skipping. Direct stderr write —
    // a captured println! from a PASSING test is discarded by libtest unless
    // --nocapture is requested, which CI never does (Sol-codex round-2
    // review P3-2b).
    writeln!(
        std::io::stderr().lock(),
        "node {node_version}: {}/{} agreement lines + {} documented divergences, verdicts={verdicts}",
        NODE_AGREEMENT_LINES.len(),
        all_lines.len(),
        NODE_DIVERGENCE_LINES.len(),
    )
    .expect("write node-verification success diagnostic");
}

/// Invoke node ONCE with the corpus path via an ENV VAR (avoids argv-encoding
/// pitfalls on Windows and `-e`/argv-index ambiguities across node versions)
/// and validate the verdict string. Extracted as a fn (not an immediately
/// invoked closure) to satisfy clippy's `redundant_closure_call`.
fn run_node_corpus_check(corpus_path: &std::path::Path, expected_len: usize) -> String {
    let out = std::process::Command::new("node")
        .arg("-e")
        .arg(NODE_CHECK_SCRIPT)
        .env("PROC_PROBE_P3_2_CORPUS", corpus_path)
        .output()
        .expect("spawn node (availability checked above)");
    assert!(
        out.status.success(),
        "node corpus check failed:
{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        v.len(),
        expected_len,
        "one verdict per corpus line expected"
    );
    assert!(
        v.chars().all(|c| c == 'A' || c == 'R'),
        "verdict string must contain only 'A'/'R', got {v:?}"
    );
    // Non-vacuity: the comparison must exercise both outcomes.
    assert!(
        v.contains('A') && v.contains('R'),
        "corpus must produce both accept and reject verdicts, got {v:?}"
    );
    v
}

/// The node one-liner: reads the corpus path from an ENV VAR (avoids
/// argv-encoding pitfalls on Windows and `-e`/argv-index ambiguities across
/// node versions), rebuilds the runner regex, and prints one `A`/`R` verdict
/// per corpus line. The regex body, FLAGS, and trim preprocessing all come
/// from the runner's extracted ACTIVE contract (Sol-codex round-2 review
/// P3-1) instead of a hand-copied literal, so the check always executes the
/// runner's CURRENT semantics: `new RegExp(body, flags)` and
/// `parsed.trim ? line.trim() : line` before `.exec()`, exactly as
/// `parseResult` does at `scripts/paired-ab-runner.mjs:253`.
const NODE_CHECK_SCRIPT: &str = r"
const fs = require('fs');
const parsed = JSON.parse(fs.readFileSync(process.env.PROC_PROBE_P3_2_CORPUS, 'utf8'));
const re = new RegExp(parsed.regex, parsed.flags);
const prep = (l) => (parsed.trim ? l.trim() : l);
process.stdout.write(parsed.lines.map((l) => (re.exec(prep(l)) !== null ? 'A' : 'R')).join(''));
";

/// The ONLY skippable node-spawn failure is "binary absent" (NotFound).
/// Anything else (e.g. PermissionDenied: found but not executable) is a real
/// environment problem and must hard-fail, not silently skip (Sol-codex
/// round-2 review P3-2a). Returns Some(skip reason) for NotFound, None for
/// fatal.
fn node_spawn_skip_reason(e: &std::io::Error) -> Option<String> {
    if e.kind() == std::io::ErrorKind::NotFound {
        Some(format!("node not runnable ({e})"))
    } else {
        None
    }
}

/// Direct stderr write — libtest captures println!/eprintln! from a PASSING
/// test and discards the buffer unless --nocapture is requested, which CI
/// never does, so a captured skip is indistinguishable from a pass in CI
/// logs (Sol-codex round-2 review P3-2b). `std::io::stderr().lock()` writes
/// bypass capture entirely. Mirrors
/// `crates/proc-memstat/tests/non_utf8_name.rs` exactly.
fn report_skip(reason: &str) {
    writeln!(std::io::stderr().lock(), "[protocol] SKIP: {reason}").expect("report skip");
}

#[test]
fn drift_guard_rejects_added_case_insensitive_flag() {
    let mutated = SYNTHETIC_PARSE_RESULT_SRC.replace(
        r#"=(\S+)$/.exec(line.trim())"#,
        r#"=(\S+)$/i.exec(line.trim())"#,
    );
    assert_ne!(
        mutated, SYNTHETIC_PARSE_RESULT_SRC,
        "premise: the mutation must actually apply to the synthetic runner copy"
    );
    // Premise of the blindness being fixed (P3-1): the OLD naive substring
    // guard (`runner_src.contains(EXPECTED_RUNNER_REGEX_BODY)`) still passes
    // on the mutated source, because the documented body remains present —
    // the added flag lives AFTER the closing slash and is invisible to a
    // body-substring check.
    assert!(
        mutated.contains(EXPECTED_RUNNER_REGEX_BODY),
        "premise: the old substring guard is blind to an added regex flag"
    );
    let err = runner_contract_drift(&mutated)
        .expect_err("the drift guard must reject an added case-insensitive flag");
    assert!(
        err.to_lowercase().contains("flag"),
        "drift message must name the flags field, got: {err}"
    );
}

#[test]
fn drift_guard_rejects_removed_trim() {
    let mutated = SYNTHETIC_PARSE_RESULT_SRC.replace(".exec(line.trim())", ".exec(line)");
    assert_ne!(
        mutated, SYNTHETIC_PARSE_RESULT_SRC,
        "premise: the mutation must actually apply to the synthetic runner copy"
    );
    // Same premise as the flag control (P3-1): the OLD substring guard is
    // blind — the regex body itself is untouched, only the preprocessing
    // around `.exec(` changed.
    assert!(
        mutated.contains(EXPECTED_RUNNER_REGEX_BODY),
        "premise: the old substring guard is blind to removed trim preprocessing"
    );
    let err =
        runner_contract_drift(&mutated).expect_err("the drift guard must reject removed trim");
    assert!(
        err.contains("trim"),
        "drift message must mention the trim drift, got: {err}"
    );
}

#[test]
fn drift_guard_accepts_untouched_synthetic_runner() {
    let contract = runner_contract_drift(SYNTHETIC_PARSE_RESULT_SRC)
        .expect("the verbatim runner copy must match the documented contract");
    assert_eq!(contract.regex_body, EXPECTED_RUNNER_REGEX_BODY);
    assert_eq!(contract.flags, "");
    assert!(contract.trims);
}

#[test]
fn node_spawn_errors_other_than_not_found_are_fatal_not_skips() {
    // A DIRECTORY, not the node binary: spawning it yields an error verified
    // empirically on this Windows machine as ErrorKind::PermissionDenied
    // (raw os error 5); on Unix EACCES maps to PermissionDenied the same way.
    let err = std::process::Command::new(std::env::current_dir().expect("current_dir"))
        .arg("--version")
        .output()
        .expect_err("spawning a directory must fail");
    assert!(
        err.kind() != std::io::ErrorKind::NotFound,
        "premise: the spawn failed with a non-NotFound kind, got {:?}",
        err.kind()
    );
    assert!(
        node_spawn_skip_reason(&err).is_none(),
        "a found-but-unspawnable target is FATAL, not a skippable absent tool \
         (Sol-codex round-2 review P3-2a); got kind {:?}",
        err.kind()
    );
    // The genuinely-absent case stays skippable.
    assert!(
        node_spawn_skip_reason(&std::io::Error::from(std::io::ErrorKind::NotFound)).is_some(),
        "NotFound must remain the single skippable spawn failure"
    );
}

/// P3-2b counterfactual (Sol-codex round-2 review): the SKIP notice must
/// reach the REAL stderr even though libtest captures (and, for a passing
/// test, DISCARDS) println!/eprintln! output — CI never passes --nocapture.
/// Mirrors `crates/proc-memstat/tests/non_utf8_name.rs`'s
/// `skip_notice_survives_both_libtest_capture_layers`: re-exec this test
/// binary with a marker env var and `.env_remove("RUST_TEST_NOCAPTURE")`, no
/// `--nocapture`, and assert the direct-stderr notice appears in the child's
/// CAPTURED stderr. Under the pre-fix eprintln! implementation this test
/// FAILS (the child's passing harness discards the buffer).
#[test]
fn skip_notice_survives_libtest_capture() {
    const TEST: &str = "skip_notice_survives_libtest_capture";
    const MARKER: &str = "PROC_PROBE_PROTOCOL_SKIP_CAPTURE_CHILD";
    const REASON: &str = "synthetic libtest-capture probe";

    if std::env::var_os(MARKER).is_some() {
        report_skip(REASON);
        return;
    }

    let out = std::process::Command::new(std::env::current_exe().expect("current_exe"))
        .args(["--exact", TEST])
        .env(MARKER, "1")
        .env_remove("RUST_TEST_NOCAPTURE")
        .output()
        .expect("spawn libtest-capture regression runner");
    assert!(
        out.status.success(),
        "libtest-capture regression runner failed: {out:?}"
    );
    let stderr = String::from_utf8(out.stderr).expect("UTF-8 skip diagnostic");
    assert!(
        stderr.contains(&format!("[protocol] SKIP: {REASON}")),
        "skip notice was hidden by libtest capture: {stderr:?}"
    );
}
