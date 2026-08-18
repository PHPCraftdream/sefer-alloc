//! Mechanical pin on the `granted_huge` / `is_huge()` READER enumeration
//! (task #1098, condition 3).
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
//! silently; this test turns that one into a STATED invariant: any added,
//! removed, or moved reader fails HERE, with a message pointing at the
//! SAFETY comment that must be re-derived in the same change.
//!
//! Same pattern as the root crate's `tests/ci_clippy_matrix_consistency.rs`
//! (itself following `tests/no_stale_doc_references.rs`): a deliberately
//! lightweight text scan rather than a parser. Doc lines (`///`, `//!`) and
//! comment lines (`//`) are stripped before counting, so documentation
//! mentions never count.
//!
//! Counted patterns, and why exactly these:
//! - `is_huge()` call sites on code lines — behavioral readers through the
//!   accessor (the accessor's own definition, `is_huge(&self)`, deliberately
//!   does not match the pattern — it is not a reader). Expected: exactly 4,
//!   all in `src/reservation.rs` (`can_decommit_reclaim_and_zero`,
//!   `decommit`, `try_decommit`, `decommit_lazy`).
//! - `.granted_huge` member-access reads (`self.granted_huge`,
//!   `r.granted_huge`) — direct field reads. Expected: exactly 5 —
//!   `src/reservation.rs` x3 (`Debug`, `is_huge`, `into_full_parts`),
//!   `src/reservation_full_parts.rs` x1 (`into_reservation`),
//!   `src/api/internal.rs` x1 (`finish_reservation`). Producers
//!   (struct-literal keys, shorthand inits, tuple elements, parameters)
//!   are deliberately NOT counted: writing the flag cannot branch on it.
//! - The `impl Drop for Reservation` block must contain NEITHER pattern —
//!   the release path stays flag-blind.
//! - Between `pub unsafe fn from_raw_parts` and `impl Drop for
//!   Reservation`: exactly 2 bare `granted_huge` tokens (the parameter
//!   and the field init). More would mean the constructor started
//!   VALIDATING on the flag, contradicting the SAFETY comment's "assert
//!   block does not read it".
//!
//! Known limitations, accepted: `/* */` block comments are not stripped
//! (none in `src/` mention the flag today — if one appears it trips this
//! test and joins the expectation table consciously); a local variable
//! named `granted_huge` shadowing the field counts the same as a field
//! read (same risk class, so failing is the right default).

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Counts {
    is_huge_calls: usize,
    granted_huge_member_reads: usize,
}

fn count_code_line_patterns(src: &str) -> Counts {
    let mut c = Counts::default();
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("///") || t.starts_with("//!") || t.starts_with("//") {
            continue;
        }
        c.is_huge_calls += line.matches("is_huge()").count();
        c.granted_huge_member_reads += line.matches(".granted_huge").count();
    }
    c
}

fn count_bare_granted_huge(src: &str) -> usize {
    src.lines()
        .filter(|line| {
            let t = line.trim_start();
            !(t.starts_with("///") || t.starts_with("//!") || t.starts_with("//"))
        })
        .map(|line| line.matches("granted_huge").count())
        .sum()
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

const REDERIVE_MSG: &str = "the granted_huge/is_huge() reader enumeration changed: \
                            re-derive the SAFETY comment on \
                            method_try_decommit_reports_malformed_range_on_huge_flagged_reservation \
                            (tests/reservation_decommit_contract.rs) and update this test's \
                            expectation table in the SAME change";

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
        let in_reservation_rs = rel == "src/reservation.rs";
        assert_eq!(
            c.is_huge_calls,
            usize::from(in_reservation_rs) * 4,
            "{rel}: is_huge() call sites: {REDERIVE_MSG}"
        );
        let expected_member_reads = match rel.as_str() {
            "src/reservation.rs" => 3,
            "src/reservation_full_parts.rs" => 1,
            "src/api/internal.rs" => 1,
            _ => 0,
        };
        assert_eq!(
            c.granted_huge_member_reads, expected_member_reads,
            "{rel}: .granted_huge member reads: {REDERIVE_MSG}"
        );
        total.is_huge_calls += c.is_huge_calls;
        total.granted_huge_member_reads += c.granted_huge_member_reads;
    }
    assert_eq!(total.is_huge_calls, 4, "{REDERIVE_MSG}");
    assert_eq!(total.granted_huge_member_reads, 5, "{REDERIVE_MSG}");

    let reservation_rs =
        fs::read_to_string(src_dir.join("reservation.rs")).expect("read src/reservation.rs");
    let drop_marker = "impl Drop for Reservation";
    let drop_start = reservation_rs
        .find(drop_marker)
        .unwrap_or_else(|| panic!("granted_huge guard: `{drop_marker}` not found"));
    let drop_end = reservation_rs[drop_start..]
        .find("\n}\n")
        .map(|i| drop_start + i + 3)
        .unwrap_or_else(|| panic!("granted_huge guard: end of `{drop_marker}` impl not found"));
    let drop_block = &reservation_rs[drop_start..drop_end];
    let drop_counts = count_code_line_patterns(drop_block);
    assert_eq!(
        (
            drop_counts.is_huge_calls,
            drop_counts.granted_huge_member_reads
        ),
        (0, 0),
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
