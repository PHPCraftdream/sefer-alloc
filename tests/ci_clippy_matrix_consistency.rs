//! Structural consistency guard: `.github/workflows/ci.yml`'s `clippy` job's
//! hand-listed steps must match `scripts/check-matrix.mjs`'s `PER_PR_ROWS`
//! clippy rows exactly — same rows, same order, same feature strings.
//!
//! R31-11 (task #475), closing a gap two independent reviews flagged:
//! `docs/reviews/2026-07-30-r30-post-response-readonly-review.md` ("P2 —
//! generated CI matrix is not yet the only source of truth") and
//! `docs/reviews/2026-07-30-r30-full-review.md` P2-7 (`docs/perf/OPEN_ITEMS.md`
//! item 31). R30-5 (task #454, commit `3c3ad7d`) made
//! `scripts/check-matrix.mjs`'s `PER_PR_ROWS` the single source of truth for
//! the `check`/`test` rows — `scripts/run-check-matrix.mjs` (invoked by both
//! `scripts/check-all.mjs`'s local `npm run check` and ci.yml's
//! `check-matrix` job) actually re-runs those rows from the manifest, so a
//! drift there is mechanically caught. The `clippy` job's 5 rows were
//! deliberately left as hand-listed named Actions-UI steps (so a red step
//! names its failing feature combination at a glance) — but ci.yml's own
//! `--kind check --kind test` invocation EXCLUDES every `clippy`-kind row by
//! construction (see `run-check-matrix.mjs`'s `--kind` filter), so nothing
//! previously verified the hand-listed clippy steps still matched the
//! manifest. ci.yml's own `clippy` job header comment claimed this was
//! covered ("the `check-matrix` job runs the manifest's OWN version of each
//! row independently and would still catch a regression") — a guarantee the
//! repo did not actually provide for clippy specifically. This test is that
//! missing guarantee, implemented directly (not by shelling out to Node) so
//! it runs as part of the ordinary `cargo test` surface.
//!
//! Deliberately a lightweight text scan over both files (matching this
//! repo's established `tests/no_stale_doc_references.rs` pattern), not a
//! real JS/YAML parser: `scripts/check-matrix.mjs`'s `PER_PR_ROWS` array and
//! `.github/workflows/ci.yml`'s `clippy` job steps both have a small, fixed,
//! human-authored shape that a targeted scan handles exactly, and a full
//! parser would be more machinery than the two files it is reading.
//!
//! Doc/config-only guard: reads source text, never links the crate or
//! invokes `node`/`cargo clippy`, so it runs in every feature configuration
//! and costs zero extra `cargo` invocations (see CLAUDE.md's cargo-hack
//! per-PR-invocation-count discipline).

use std::fs;
use std::path::Path;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// One `clippy`-kind row's `features` value, extracted from
/// `scripts/check-matrix.mjs`'s `PER_PR_ROWS` array, in declaration order.
/// `None` inside the `Vec` slot is never produced — a row with `features: ''`
/// yields `Some(String::new())`, matching `rowToCargoArgs`'s "empty string
/// means no `--features` flag" contract.
struct ManifestClippyRow {
    /// `id` field, for diagnostics.
    id: String,
    /// Exact `features` field value; `"__all__"` is the `--all-features`
    /// sentinel per `check-matrix.mjs`'s `ALL_FEATURES_SENTINEL`.
    features: String,
}

/// Parse every `{ id: '...', kind: 'clippy', features: '...', ... }` object
/// out of `PER_PR_ROWS`'s source array literal, in the order they appear.
///
/// This scans object-by-object rather than attempting a general JS parser:
/// each `PER_PR_ROWS` entry is a `{ ... }` block on its own lines (verified
/// by inspection of `scripts/check-matrix.mjs` at the time this test was
/// written), so splitting on top-level `{` / `}` boundaries within the
/// `export const PER_PR_ROWS = [ ... ];` slice is exact for this file's
/// actual shape without needing a JSON/JS grammar.
fn parse_manifest_clippy_rows(check_matrix_src: &str) -> Vec<ManifestClippyRow> {
    let start_marker = "export const PER_PR_ROWS";
    let start = check_matrix_src
        .find(start_marker)
        .expect("scripts/check-matrix.mjs: `export const PER_PR_ROWS` not found");
    let after_start = &check_matrix_src[start..];
    // The array literal ends at the first top-level `];` after the opening
    // `[` that follows `PER_PR_ROWS =`.
    let array_open = after_start
        .find('[')
        .expect("scripts/check-matrix.mjs: no `[` found after PER_PR_ROWS declaration");
    let end_marker = "\n];";
    let array_close_rel = after_start[array_open..]
        .find(end_marker)
        .expect("scripts/check-matrix.mjs: no closing `\\n];` found for PER_PR_ROWS array");
    let array_body = &after_start[array_open + 1..array_open + array_close_rel];

    let mut rows = Vec::new();
    let mut rest = array_body;
    while let Some(obj_start) = rest.find('{') {
        let obj_end = rest[obj_start..]
            .find('}')
            .expect("scripts/check-matrix.mjs: unmatched `{` in PER_PR_ROWS entry")
            + obj_start;
        let obj_text = &rest[obj_start..=obj_end];

        let kind = extract_single_quoted_field(obj_text, "kind")
            .unwrap_or_else(|| panic!("PER_PR_ROWS entry missing `kind` field: {obj_text}"));
        if kind == "clippy" {
            let id = extract_single_quoted_field(obj_text, "id").unwrap_or_else(|| {
                panic!("PER_PR_ROWS clippy entry missing `id` field: {obj_text}")
            });
            let features = extract_single_quoted_field(obj_text, "features").unwrap_or_else(|| {
                panic!("PER_PR_ROWS clippy entry {id:?} missing `features` field: {obj_text}")
            });
            rows.push(ManifestClippyRow { id, features });
        }

        rest = &rest[obj_end + 1..];
    }
    rows
}

/// Extract the single-quoted string value of `field: '...'` from a JS object
/// literal's source text (e.g. `features: 'production'` -> `"production"`).
/// Handles the field appearing anywhere in the object body, on any line.
fn extract_single_quoted_field(obj_text: &str, field: &str) -> Option<String> {
    let needle = format!("{field}:");
    let pos = obj_text.find(&needle)?;
    let after = &obj_text[pos + needle.len()..];
    let after = after.trim_start();
    let after = after.strip_prefix('\'')?;
    let end = after.find('\'')?;
    Some(after[..end].to_string())
}

/// Build the canonical `cargo clippy ...` invocation for a clippy-kind row
/// with no `target` (every `PER_PR_ROWS` clippy row is whole-crate today),
/// mirroring `check-matrix.mjs`'s `rowToCargoArgs`:
/// `clippy --all-targets [--features <value> | --all-features] -- -D
/// warnings`. A single-token `--features` value is left UNQUOTED — shell
/// quoting (`--features experimental` vs `--features "experimental"`) is a
/// pure presentation choice there, not a semantic difference `cargo` sees.
/// A multi-token value (e.g. `"hardened medium-classes"`) MUST stay quoted
/// — the quotes are load-bearing so the shell passes one argument.
/// [`normalize_features_quoting`] strips matching quotes from the ACTUAL
/// ci.yml line ONLY in the single-token case, so both sides are compared in
/// this same "quoted only when the value has multiple tokens" form.
fn expected_clippy_invocation(features: &str) -> String {
    const ALL_FEATURES_SENTINEL: &str = "__all__";
    let mut s = String::from("cargo clippy --all-targets");
    if features == ALL_FEATURES_SENTINEL {
        s.push_str(" --all-features");
    } else if !features.is_empty() {
        if features.contains(' ') {
            s.push_str(&format!(" --features \"{features}\""));
        } else {
            s.push_str(&format!(" --features {features}"));
        }
    }
    s.push_str(" -- -D warnings");
    s
}

/// Strip a matching pair of double quotes around the `--features <...>`
/// argument in a `cargo clippy ...` line, if present, so
/// `--features "production"` and `--features production` compare equal —
/// only the multi-token case (`--features "hardened medium-classes"`) truly
/// needs the quotes; the single-token case is quoted or not per ci.yml's own
/// existing style without semantic meaning either way.
fn normalize_features_quoting(line: &str) -> String {
    let Some(pos) = line.find("--features \"") else {
        return line.to_string();
    };
    let after_open = pos + "--features \"".len();
    let Some(close_rel) = line[after_open..].find('"') else {
        return line.to_string();
    };
    let close = after_open + close_rel;
    let value = &line[after_open..close];
    if value.contains(' ') {
        // Multi-token value: quotes are load-bearing, keep them.
        return line.to_string();
    }
    format!("{}--features {value}{}", &line[..pos], &line[close + 1..])
}

/// Extract the `clippy:` job's step block from `ci.yml`'s raw text: from the
/// `  clippy:` job header (two-space indent, top-level job key) up to the
/// next top-level job key (a line starting with exactly two spaces then a
/// bareword followed by `:`, at the same indent level) or end of file.
fn extract_clippy_job_block(ci_yml: &str) -> &str {
    let lines: Vec<&str> = ci_yml.lines().collect();
    let start = lines
        .iter()
        .position(|l| *l == "  clippy:")
        .expect("ci.yml: no top-level `  clippy:` job found");

    let mut end = lines.len();
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        // A top-level job key: exactly two leading spaces, then a
        // non-space/non-`#` char, ending in `:` (ignoring trailing comment).
        // Blank lines and deeper-indented lines (steps, comments) don't match.
        if line.starts_with("  ")
            && !line.starts_with("   ")
            && !line.trim().is_empty()
            && !line.trim_start().starts_with('#')
            && line.trim_end().ends_with(':')
        {
            end = i;
            break;
        }
    }

    // Recompute the byte offset of `start`/`end` in the original text via
    // line-join, since we only need this block's own text for substring
    // scans below (line numbers are not needed past this point).
    let block_start_byte: usize = lines[..start].iter().map(|l| l.len() + 1).sum();
    let block_end_byte: usize = lines[..end].iter().map(|l| l.len() + 1).sum();
    &ci_yml[block_start_byte..block_end_byte]
}

#[test]
fn ci_clippy_steps_match_manifest_clippy_rows_exactly() {
    let manifest = manifest_dir();
    // Normalize CRLF -> LF right after reading: `extract_clippy_job_block`
    // reconstructs byte offsets from `lines()` output as `l.len() + 1` per
    // line (a bare `\n` terminator). On a Windows checkout (e.g. this repo's
    // `test windows (production)` CI job), `ci.yml` is read back with `\r\n`
    // line endings, so that formula undercounts by 1 byte per line and the
    // reconstructed job-block slice ends short of the real end, silently
    // dropping trailing steps (caught with `clippy (--features "production")`
    // missing — R32-1). Normalizing first keeps the offset math correct
    // regardless of the checkout platform's line-ending conversion.
    let check_matrix_src = fs::read_to_string(manifest.join("scripts").join("check-matrix.mjs"))
        .expect("read scripts/check-matrix.mjs")
        .replace("\r\n", "\n");
    let ci_yml = fs::read_to_string(manifest.join(".github").join("workflows").join("ci.yml"))
        .expect("read .github/workflows/ci.yml")
        .replace("\r\n", "\n");

    let manifest_rows = parse_manifest_clippy_rows(&check_matrix_src);
    assert!(
        !manifest_rows.is_empty(),
        "parse_manifest_clippy_rows found zero clippy-kind rows in \
         scripts/check-matrix.mjs's PER_PR_ROWS — either the manifest lost \
         all its clippy rows (a real regression) or this test's parser no \
         longer matches the file's shape (update the parser, don't just \
         let this pass vacuously)."
    );

    let clippy_job = extract_clippy_job_block(&ci_yml);

    // Every `run:` line in the clippy job that invokes `cargo clippy`, in
    // the order they appear — this is ci.yml's actual, current enumeration
    // of clippy invocations for this job.
    let ci_invocations: Vec<&str> = clippy_job
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("run: cargo clippy"))
        .map(|l| l.trim_start_matches("run: ").trim())
        .collect();

    assert_eq!(
        ci_invocations.len(),
        manifest_rows.len(),
        "ci.yml's `clippy` job has {} `cargo clippy` step(s) but \
         scripts/check-matrix.mjs's PER_PR_ROWS has {} clippy-kind row(s) — \
         these must match 1:1 (R31-11/task #475; see this test's module doc \
         and docs/perf/OPEN_ITEMS.md item 31 / P2-7). ci.yml steps:\n{}\n\n\
         manifest row ids: {:?}",
        ci_invocations.len(),
        manifest_rows.len(),
        ci_invocations.join("\n"),
        manifest_rows.iter().map(|r| &r.id).collect::<Vec<_>>(),
    );

    let mut mismatches = Vec::new();
    for (i, (row, ci_line)) in manifest_rows.iter().zip(ci_invocations.iter()).enumerate() {
        let expected = expected_clippy_invocation(&row.features);
        let actual = normalize_features_quoting(ci_line);
        if actual != expected {
            mismatches.push(format!(
                "position {i}: manifest row `{}` (features={:?}) expects\n    {expected}\n  \
                 but ci.yml's clippy job has\n    {ci_line}\n  (normalized: {actual})\n  \
                 at the same position",
                row.id, row.features,
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "ci.yml's `clippy` job steps have drifted from \
         scripts/check-matrix.mjs's PER_PR_ROWS clippy rows (R31-11/task #475). \
         Edit the feature combinations in check-matrix.mjs, then keep ci.yml's \
         hand-listed steps (kept hand-listed so each combination gets its own \
         named Actions-UI step) transcribed identically:\n{}",
        mismatches.join("\n\n"),
    );
}
