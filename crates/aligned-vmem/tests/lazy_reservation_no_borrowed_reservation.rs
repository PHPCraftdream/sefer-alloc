//! Mechanical pin on the `LazyReservation` watermark: NO safe borrowed path
//! from a `LazyReservation` to a `Reservation` (task #1104, finding H1 of
//! `docs/reviews/2026-08-18-aligned-vmem-publication-readiness-audit.md`).
//!
//! H1 was a HIGH release blocker. `LazyReservation::as_reservation(&self)
//! -> &Reservation` looked like a read-only convenience, but every
//! OS-state mutator on `Reservation` — `decommit`, `try_decommit`,
//! `decommit_lazy`, `recommit`, `try_recommit`, `commit_range`,
//! `try_commit_range` — takes `&self`, so 100% safe code could decommit
//! pages under the watermark while `committed_len()` kept promising them
//! writable (a Windows write in that range is `STATUS_ACCESS_VIOLATION`),
//! or commit past it without the watermark ever learning. The accessor was
//! DELETED, and this test turns that deletion into a STATED invariant:
//! any re-added borrowed path fails HERE, with a message pointing at the
//! card that must be re-derived in the same change.
//!
//! A runtime test cannot express "this does not compile", so — same
//! pattern as `tests/granted_huge_reader_enumeration.rs` (itself
//! following the root crate's `tests/ci_clippy_matrix_consistency.rs`) —
//! this is a deliberately lightweight text scan over the crate's `src/`,
//! not a parser and not a `trybuild` dev-dependency. Doc lines (`///`,
//! `//!`) and `//` comment lines are stripped before scanning, so
//! documentation mentions never count.
//!
//! Checked patterns, and why exactly these:
//! - the token `as_reservation` on any code line anywhere in `src/` —
//!   resurrection under the old name, in any impl, at any visibility.
//! - `&Reservation` or `&mut Reservation` on any code line in `src/` —
//!   the leak class itself: a shared or exclusive reference to a
//!   `Reservation`, from any function, in return position or otherwise.
//!   When this guard was written `src/` contained ZERO such lines (the
//!   accessor was the only one), so any hit is new code. A helper that
//!   legitimately wants `&Reservation` as a PARAMETER also fires —
//!   accepted: same risk class, failing is the right default (the same
//!   posture as the `granted_huge` guard's shadowing note), and the
//!   failure message says what to do.
//! - `AsRef<Reservation>`, `Borrow<Reservation>`, or a `type Target`
//!   line naming `Reservation` — the trait-shaped resurrection: a view
//!   type (or `LazyReservation` itself) gaining `Deref`/`AsRef`/
//!   `Borrow` to `Reservation` re-exposes every `&self` mutator through
//!   auto-deref without a single `&Reservation` token in a fn signature.
//! - The exact set of `pub fn` / `pub const fn` names inside every
//!   `impl LazyReservation` block is pinned to: `align`, `as_ptr`,
//!   `committed_len`, `ensure_committed`, `is_empty`, `into_reservation`,
//!   `len`, `shrink_committed`. Any addition or removal fails here, so a
//!   future accessor joins this expectation table consciously instead of
//!   shipping silently.
//!
//! Known limitations, accepted: `/* */` block comments are not stripped
//! (none in `src/` mention `Reservation` today — if one appears it trips
//! this test and joins the table consciously); a reference type SPLIT
//! across lines (`&` on one line, `Reservation` on the next) evades the
//! line-local scan — rustfmt does not produce that shape; a NEW wrapper
//! type re-implementing the mutators directly over a raw pointer, rather
//! than delegating through `&Reservation`, is a semantic duplication no
//! text scan can recognize — only its `Deref`/`AsRef`/`Borrow` forms are
//! caught here; and `Reservation` gaining `Clone` would reopen an
//! OWNED-handle leak this guard does not model — structurally impossible
//! today (bare pointer + `Drop`: a `Clone` would double-release).

use std::fs;
use std::path::{Path, PathBuf};

const REDERIVE_MSG: &str = "the LazyReservation watermark-sealing surface changed: \
                            re-derive docs/CORRECTNESS_OPEN_ITEMS.md item 66's card and \
                            this test's expectation table in the SAME change \
                            (task #1104 / finding H1)";

fn is_code_line(line: &str) -> bool {
    let t = line.trim_start();
    !(t.starts_with("///") || t.starts_with("//!") || t.starts_with("//"))
}

/// Blank out doc/comment lines while preserving line structure, so block
/// markers (`impl LazyReservation`) are only searched in real code.
fn mask_non_code(src: &str) -> String {
    let is_final_newline = src.ends_with('\n');
    let masked = src
        .lines()
        .map(|line| if is_code_line(line) { line } else { "" })
        .collect::<Vec<_>>()
        .join("\n");
    // Preserve the final newline if the original had one (lines() strips it).
    if is_final_newline {
        masked + "\n"
    } else {
        masked
    }
}

fn walk_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("lazy_reservation guard: cannot read {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("lazy_reservation guard: read_dir entry");
        let path = entry.path();
        if path.is_dir() {
            walk_rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_borrowed_reservation_escapes_lazy_reservation() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk_rs_files(&src_dir, &mut files);
    assert!(
        !files.is_empty(),
        "lazy_reservation guard: no .rs files found under {} — layout changed; {REDERIVE_MSG}",
        src_dir.display()
    );

    let mut impl_blocks = 0usize;
    let mut pub_fn_names: Vec<String> = Vec::new();

    for file in &files {
        let src = fs::read_to_string(file).unwrap_or_else(|e| {
            panic!(
                "lazy_reservation guard: cannot read {}: {e}",
                file.display()
            )
        });
        let rel = file
            .strip_prefix(src_dir.parent().unwrap())
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");

        for line in src.lines().filter(|l| is_code_line(l)) {
            assert!(
                !line.contains("as_reservation"),
                "{rel}: `as_reservation` resurrected — the H1 bypass is back: {REDERIVE_MSG}"
            );
            assert!(
                !(line.contains("&Reservation") || line.contains("&mut Reservation")),
                "{rel}: a `&Reservation`/`&mut Reservation` reference exists in code — the \
                 leak class H1 closed (any such borrow reaches the `&self` OS-state \
                 mutators): {REDERIVE_MSG}"
            );
            assert!(
                !(line.contains("AsRef<Reservation>") || line.contains("Borrow<Reservation>")),
                "{rel}: an `AsRef`/`Borrow` impl targeting `Reservation` re-exposes its \
                 `&self` mutators: {REDERIVE_MSG}"
            );
            if line.trim_start().starts_with("type Target") && line.contains("Reservation") {
                panic!(
                    "{rel}: a `type Target = ... Reservation` deref target re-exposes \
                        the `&self` mutators: {REDERIVE_MSG}"
                );
            }
        }

        // Enumerate every `impl LazyReservation { ... }` block and pin its
        // public method set. Block end = the first column-0 `}` after the
        // marker (inner items are indented), same heuristic as the
        // `granted_huge` guard's `impl Drop for Reservation` scan.
        let masked = mask_non_code(&src);
        let mut rest = masked.as_str();
        while let Some(pos) = rest.find("impl LazyReservation") {
            let after_marker = &rest[pos..];
            let block_len = after_marker
                .find("\n}\n")
                .unwrap_or_else(|| panic!("lazy_reservation guard: `impl LazyReservation` block in {rel} has no column-0 closing brace"));
            let block = &after_marker[..block_len];
            impl_blocks += 1;
            for line in block.lines().filter(|l| is_code_line(l)) {
                let t = line.trim_start();
                for prefix in ["pub const fn ", "pub fn "] {
                    if let Some(after) = t.strip_prefix(prefix) {
                        let name = after.split('(').next().unwrap_or(after).trim();
                        if !name.is_empty() {
                            pub_fn_names.push(name.to_string());
                        }
                    }
                }
            }
            rest = &rest[pos + block_len..];
        }
    }

    assert!(
        impl_blocks >= 1,
        "lazy_reservation guard: no `impl LazyReservation` block found under {} — \
         layout changed; {REDERIVE_MSG}",
        src_dir.display()
    );
    pub_fn_names.sort();
    pub_fn_names.dedup();
    let expected = [
        "align",
        "as_ptr",
        "committed_len",
        "ensure_committed",
        "into_reservation",
        "is_empty",
        "len",
        "shrink_committed",
    ];
    assert_eq!(
        pub_fn_names, expected,
        "LazyReservation's public method set changed: {REDERIVE_MSG}"
    );
}
