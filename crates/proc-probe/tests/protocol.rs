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
//!    `RESULT_PREFIX` to `"RESULTX"` passed every test.
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

    /// The real `emit*` output must land on the child's real stdout as exact
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
    #[test]
    fn real_stdout_emit_family_exact_bytes() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            // Child branch: call the REAL functions in this exact order.
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
            return;
        }

        // Runner branch: re-exec this test binary with the marker set.
        let exe = std::env::current_exe().expect("current_exe resolves this test binary");
        let out = std::process::Command::new(&exe)
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(CHILD_MARKER, "1")
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
        let stdout = String::from_utf8(out.stdout)
            .expect("emit* write UTF-8; the harness adds no non-UTF-8 framing");

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

/// Byte-for-byte the body of the regex literal at
/// `scripts/paired-ab-runner.mjs:253` (inside `parseResult`, lines 250–257;
/// re-read and confirmed 2026-09-09). Identical copies also exist at
/// `scripts/r10_5_large_cache_gate.mjs:114`, `scripts/r34_7_causal_harness.mjs:128`
/// and `scripts/r34_23_vec_harness.mjs:117` — only the paired-ab-runner file
/// is drift-guarded below.
const RUNNER_REGEX_SRC: &str = "^RESULT\\s+([a-z0-9_]+)=(\\S+)$";

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
            eprintln!(
                "SKIP parser_contract_matches_node_ecmascript_regex: runner not found at {}",
                runner_path.display()
            );
            return;
        }
        Err(e) => panic!("failed to read {}: {e}", runner_path.display()),
    };

    // Drift guard: if the runner's parse contract moved, FAIL rather than
    // silently comparing against a stale copy of the regex.
    assert!(
        runner_src.contains(RUNNER_REGEX_SRC),
        "the runner's parse contract moved; re-sync RUNNER_REGEX_SRC from \
         scripts/paired-ab-runner.mjs:253 (parseResult, lines 250-257)"
    );

    // Node availability: absent node is a SKIP; a failing `node --version`
    // is a real environment error and panics.
    let node_version_out = match std::process::Command::new("node").arg("--version").output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "SKIP parser_contract_matches_node_ecmascript_regex: node not runnable ({e})"
            );
            return;
        }
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
    json.push_str(&json_string(RUNNER_REGEX_SRC));
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

    // Diagnostic: proves under --nocapture that node actually ran (version +
    // raw verdict string) rather than the test silently skipping.
    println!(
        "node {node_version}: {}/{} agreement lines + {} documented divergences, verdicts={verdicts}",
        NODE_AGREEMENT_LINES.len(),
        all_lines.len(),
        NODE_DIVERGENCE_LINES.len(),
    );
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
/// per corpus line — mirroring the production call shape exactly
/// (`re.exec(line.trim()) !== null`, as `parseResult` does at
/// `scripts/paired-ab-runner.mjs:253`).
const NODE_CHECK_SCRIPT: &str = r"
const fs = require('fs');
const parsed = JSON.parse(fs.readFileSync(process.env.PROC_PROBE_P3_2_CORPUS, 'utf8'));
const re = new RegExp(parsed.regex);
process.stdout.write(parsed.lines.map((l) => (re.exec(l.trim()) !== null ? 'A' : 'R')).join(''));
";
