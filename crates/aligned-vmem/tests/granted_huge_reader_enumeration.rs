//! Mechanical pin on the `granted_huge` / `is_huge()` READER enumeration
//! (task #1098, condition 3; strengthened and re-honestly-documented by
//! task #1103 after an independent review confirmed three defects: a
//! UFCS-form false negative, a CRLF hard failure, and a trailing-comment
//! false positive — all three reproduced by execution before this
//! rewrite).
//!
//! `tests/reservation_decommit_contract.rs`'s SAFETY comment on
//! `method_try_decommit_reports_malformed_range_on_huge_flagged_reservation`
//! synthesizes `granted_huge: true` over an ordinary-page reservation and
//! justifies the synthesis with an enumeration: within
//! `crates/aligned-vmem/src/`, the flag's only behavioral readers are
//! `is_huge()` itself, the pure query `can_decommit_reclaim_and_zero()`,
//! and the three decommit-family huge-skips — so a wrong flag can change
//! query results and suppress OS calls, but never touch memory safety,
//! release behavior, or an unsafe precondition. Prose enumerations rot
//! silently; this test turns that one into a STATED invariant.
//!
//! Same pattern as the root crate's `tests/ci_clippy_matrix_consistency.rs`
//! (itself following `tests/no_stale_doc_references.rs`): a deliberately
//! lightweight text scan rather than a parser.
//!
//! ## What this guard ACTUALLY catches (task #1103, honest scope)
//!
//! Every behavioral read of the flag requires one of exactly two tokens:
//! `granted_huge` (a field read, in any spelling — dotted access,
//! destructuring, `matches!`, struct-literal round-trips) or `is_huge`
//! (the accessor, in any spelling — method call, UFCS
//! `Reservation::is_huge(&r)`, `Self::is_huge(self)`, spaced `is_huge ()`).
//! This guard pins, PER FILE and in total, the count of BOTH bare tokens on
//! code lines. Therefore:
//!
//! - an ADDED reader fails here, in any spelling, in any file;
//! - a REMOVED reader fails here;
//! - a reader MOVED BETWEEN FILES fails here (both files' counts change).
//!
//! What it does NOT catch: a reader moved between methods WITHIN one file
//! with the file's token counts unchanged (e.g. relocating a call from
//! `decommit` to `recommit`). That is not a safety regression — the SAFETY
//! argument depends on the SET of readers, not their addresses — but it is
//! a doc-drift risk, and the per-method detail in the SAFETY comment is
//! still only as current as the last human re-derivation.
//!
//! ## Comment and line-ending handling
//!
//! Doc lines (`///`, `//!`), full `//` comment lines, AND trailing
//! `// …` comments on code lines are all stripped before counting, so
//! documentation mentions never count (the trailing-comment strip is the
//! task #1103 fix: `pub const fn is_huge(&self) -> bool { // see docs`
//! previously counted and failed the guard as a phantom reader). All file
//! slicing goes through `str::lines()`, which tolerates both LF and CRLF
//! line endings, so a checkout with `core.autocrlf=true` behaves
//! identically (the previous raw `"\n}\n"` block-end search panicked on a
//! CRLF copy).
//!
//! ## Additional pinned regions
//!
//! - The `impl Drop for Reservation` block must contain NEITHER token —
//!   the release path stays flag-blind.
//! - Between `pub unsafe fn from_raw_parts` and `impl Drop for
//!   Reservation`: exactly 2 bare `granted_huge` tokens (the parameter
//!   and the field init). More would mean the constructor started
//!   VALIDATING on the flag, contradicting the SAFETY comment's "assert
//!   block does not read it".
//!
//! Known limitations, accepted: `/* */` block comments are not stripped
//! (none in `src/` mention the flag today — if one appears it trips this
//! test and joins the expectation table consciously); a `//` INSIDE a
//! string literal would be mis-stripped as a comment boundary (no such
//! literal in `src/` today); a local variable named `granted_huge`
//! shadowing the field counts the same as a field read (same risk class,
//! so failing is the right default).

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Counts {
    is_huge_calls: usize,
    granted_huge_member_reads: usize,
    bare_is_huge: usize,
    bare_granted_huge: usize,
}

/// Strip a trailing `//` line comment from a code line. Full comment lines
/// become empty. A `//` inside a string literal would be mis-stripped —
/// accepted limitation (none in `src/` today).
fn strip_line_comment(line: &str) -> &str {
    match line.split_once("//") {
        Some((code, _)) => code,
        None => line,
    }
}

/// Code-line predicate over the comment-stripped text: a line whose
/// non-comment remainder has no code is skipped.
fn code_part(line: &str) -> Option<&str> {
    let code = strip_line_comment(line);
    let t = code.trim_start();
    if t.is_empty() || t.starts_with("///") || t.starts_with("//!") || t.starts_with("//") {
        None
    } else {
        Some(code)
    }
}

fn count_code_line_patterns(src: &str) -> Counts {
    let mut c = Counts::default();
    for line in src.lines() {
        let Some(code) = code_part(line) else {
            continue;
        };
        c.is_huge_calls += code.matches("is_huge()").count();
        c.granted_huge_member_reads += code.matches(".granted_huge").count();
        c.bare_is_huge += code.matches("is_huge").count();
        c.bare_granted_huge += code.matches("granted_huge").count();
    }
    c
}

fn count_bare_granted_huge(src: &str) -> usize {
    src.lines()
        .filter_map(code_part)
        .map(|code| code.matches("granted_huge").count())
        .sum()
}

/// CRLF-safe offset of the first line at or after `from` that is exactly
/// `}` (a column-0 closing brace, modulo the line ending). Returns the
/// offset just past that line's `}` character.
fn find_column_zero_block_end(src: &str, from: usize) -> Option<usize> {
    let mut offset = from;
    for line in src[from..].lines() {
        let line_start = offset;
        offset += line.len();
        // Advance past this line's terminator (LF or CRLF).
        if src[offset..].starts_with('\r') {
            offset += 1;
        }
        if src[offset..].starts_with('\n') {
            offset += 1;
        }
        if line == "}" {
            return Some(line_start + line.len());
        }
    }
    None
}

fn walk_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("granted_huge guard: cannot read {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("granted_huge guard: read_dir entry");
        let path = entry.path();
        if path.is_dir() {
            walk_rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

const REDERIVE_MSG: &str = "the granted_huge/is_huge reader enumeration changed: \
                            re-derive the SAFETY comment on \
                            method_try_decommit_reports_malformed_range_on_huge_flagged_reservation \
                            (tests/reservation_decommit_contract.rs) and update this test's \
                            expectation table in the SAME change";

/// Per-file expectation table. `is_huge_calls` counts `is_huge()` method
/// call sites (readers through the accessor); `granted_huge_member_reads`
/// counts `.granted_huge` dotted field reads; `bare_is_huge` /
/// `bare_granted_huge` count EVERY occurrence of the token on a code line
/// — the task #1103 strengthening: any read of the flag, in any spelling
/// (UFCS, destructuring, `matches!`, spaced), carries one of these tokens,
/// so the bare counts catch readers the two call-shaped patterns miss.
/// Producer positions (struct-literal keys, shorthand inits, parameters,
/// local variables) also count in the bare totals — writing the flag
/// cannot branch on it, but a change there still updates this table
/// consciously, which is the point of the pin.
fn expected_for(rel: &str) -> (usize, usize, usize, usize) {
    // (is_huge_calls, granted_huge_member_reads, bare_is_huge, bare_granted_huge)
    match rel {
        "src/reservation.rs" => (4, 3, 5, 8),
        "src/reservation_full_parts.rs" => (0, 1, 0, 4),
        "src/api/internal.rs" => (0, 1, 0, 5),
        "src/api/reserve.rs" => (0, 0, 0, 1),
        "src/api/reserve_aligned_lazy.rs" => (0, 0, 0, 1),
        "src/os/unix.rs" => (0, 0, 0, 7),
        "src/os/windows.rs" => (0, 0, 0, 2),
        _ => (0, 0, 0, 0),
    }
}

#[test]
fn granted_huge_reader_enumeration_is_pinned() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files(&src_dir, &mut files);
    assert!(
        !files.is_empty(),
        "granted_huge guard: no .rs files found under {} — layout changed; {REDERIVE_MSG}",
        src_dir.display()
    );

    let mut total = Counts::default();
    for file in &files {
        let src = fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("granted_huge guard: cannot read {}: {e}", file.display()));
        let c = count_code_line_patterns(&src);
        let rel = file
            .strip_prefix(src_dir.parent().unwrap())
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let (e_calls, e_member, e_bare_is_huge, e_bare_granted) = expected_for(&rel);
        assert_eq!(
            (
                c.is_huge_calls,
                c.granted_huge_member_reads,
                c.bare_is_huge,
                c.bare_granted_huge
            ),
            (e_calls, e_member, e_bare_is_huge, e_bare_granted),
            "{rel}: granted_huge/is_huge token counts: {REDERIVE_MSG}"
        );
        total.is_huge_calls += c.is_huge_calls;
        total.granted_huge_member_reads += c.granted_huge_member_reads;
        total.bare_is_huge += c.bare_is_huge;
        total.bare_granted_huge += c.bare_granted_huge;
    }
    assert_eq!(total.is_huge_calls, 4, "{REDERIVE_MSG}");
    assert_eq!(total.granted_huge_member_reads, 5, "{REDERIVE_MSG}");
    assert_eq!(total.bare_is_huge, 5, "{REDERIVE_MSG}");
    assert_eq!(total.bare_granted_huge, 28, "{REDERIVE_MSG}");

    let reservation_rs =
        fs::read_to_string(src_dir.join("reservation.rs")).expect("read src/reservation.rs");
    let drop_marker = "impl Drop for Reservation";
    let drop_start = reservation_rs
        .find(drop_marker)
        .unwrap_or_else(|| panic!("granted_huge guard: `{drop_marker}` not found"));
    // CRLF-safe column-0 `}` scan (task #1103 fix (b): the previous raw
    // `"\n}\n"` search panicked with a misleading "impl not found" on any
    // CRLF checkout).
    let drop_end = find_column_zero_block_end(&reservation_rs, drop_start)
        .unwrap_or_else(|| panic!("granted_huge guard: end of `{drop_marker}` impl not found"));
    let drop_block = &reservation_rs[drop_start..drop_end];
    let drop_counts = count_code_line_patterns(drop_block);
    assert_eq!(
        (
            drop_counts.is_huge_calls,
            drop_counts.granted_huge_member_reads,
            drop_counts.bare_is_huge,
            drop_counts.bare_granted_huge
        ),
        (0, 0, 0, 0),
        "impl Drop for Reservation reads the huge flag: {REDERIVE_MSG}"
    );

    let frp_marker = "pub unsafe fn from_raw_parts";
    let frp_start = reservation_rs
        .find(frp_marker)
        .unwrap_or_else(|| panic!("granted_huge guard: `{frp_marker}` not found"));
    let frp_body = &reservation_rs[frp_start..drop_start];
    assert_eq!(
        count_bare_granted_huge(frp_body),
        2,
        "from_raw_parts gained a third `granted_huge` token (beyond the \
         parameter and the field init) — it started VALIDATING on the flag: \
         {REDERIVE_MSG}"
    );
}
