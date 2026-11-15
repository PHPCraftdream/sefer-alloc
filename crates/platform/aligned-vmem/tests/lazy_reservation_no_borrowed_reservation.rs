//! Structural pin on the `LazyReservation` watermark seal (task #1104,
//! task #1113, and docs/reviews/2026-08-18-aligned-vmem-publication-readiness-audit.md
//! finding H1).
//!
//! # PRIMARY defense is STRUCTURAL (task #1113)
//!
//! The seven OS-state mutators on `Reservation` — `decommit`, `try_decommit`,
//! `decommit_lazy`, `recommit`, `try_recommit`, `commit_range`,
//! `try_commit_range` — now take `&mut self` instead of `&self`. This is the
//! primary seal: even if safe code leaks a `&Reservation`, that reference
//! cannot mutate OS state. The watermark is structurally read-only from any
//! borrowed path.
//!
//! This test is belt-over-suspenders: it catches what the structural seal
//! does NOT — exclusive-access leaks — and it makes a re-opened borrow fail
//! loudly with a pointer to the card instead of silently.
//!
//! # What this scan catches
//!
//! All checks are line-local text scans over `src/` (doc lines `///` and `//!`,
//! and `//` comments are stripped before scanning):
//!
//! - **Class 1: Path-qualified shared/exclusive references** — `fn peek(&self)
//!   -> &crate::Reservation`, `fn view(&'a self) -> &'a Reservation`, etc. A
//!   hand-rolled scanner recognizes `&` followed by optional lifetime and `mut`,
//!   then zero or more path segments (`ident::`), then `Reservation` at a word
//!   boundary. This catches all spellings: `&Reservation`, `&mut Reservation`,
//!   `&'a Reservation`, `&crate::Reservation`, `&self::Reservation`, etc.
//!
//! - **Class 2: Trait-shaped resurrection** — `AsRef<Reservation>`,
//!   `Borrow<Reservation>`, or any `type Target = ... Reservation`. A view type
//!   (or `LazyReservation` itself) gaining `Deref`/`AsRef`/`Borrow` to
//!   `Reservation` is a trait-shaped resurrection of the H1 borrow class (hands
//!   out `&Reservation` through auto-coercion without a named method). Today that
//!   reference is structurally read-only because the mutators require `&mut
//!   self`, but this pin exists because any such impl is one refactor away from
//!   re-opening H1 and because the borrow class must not come back silently.
//!
//! - **Class 3: New trait impls on `LazyReservation`** — The ONLY trait
//!   implementation allowed for `LazyReservation` is `core::fmt::Debug`. Any new
//!   trait impl (e.g., `Deref`, `AsRef`, a custom `PeekInner` trait) fails here,
//!   naming the offending trait path.
//!
//! - **Class 4: Public fields on `LazyReservation`** — A `pub inner:
//!   Reservation` field hands safe code `&mut Reservation` through `&mut
//!   LazyReservation`, which CAN still desync the watermark. The `&mut self`
//!   structural seal (task #1113) does NOT cover exclusive-access leaks, so this
//!   guard must.
//!
//! - **Class 5: Token resurrection** — The literal token `as_reservation` on
//!   any code line anywhere in `src/`. This catches a revival under the old
//!   name, in any impl, at any visibility.
//!
//! - **Class 6: Intrinsic method-set pin** — The exact set of `pub fn` /
//!   `pub const fn` names inside every `impl LazyReservation` block is pinned
//!   to: `align`, `as_ptr`, `committed_len`, `ensure_committed`,
//!   `into_reservation`, `is_empty`, `len`, `shrink_committed`. Any addition
//!   or removal fails here, so a future accessor joins this expectation table
//!   consciously instead of shipping silently.
//!
//! # Known limitations
//!
//! Accepted, stated plainly:
//!
//! - `/* */` block comments are NOT stripped (none in `src/` mention
//!   `Reservation` today — if one appears it trips this test and joins the table
//!   consciously).
//!
//! - A reference type SPLIT across lines (`&` on one line, `Reservation` on the
//!   next) evades this line-local scan — rustfmt does not produce that shape.
//!
//! - A NEW wrapper type re-implementing the mutators directly over a raw
//!   pointer, rather than delegating through `&Reservation`, is a semantic
//!   duplication no text scan can recognize — only its `Deref`/`AsRef`/`Borrow`
//!   forms are caught here.
//!
//! - Interior mutability on `LazyReservation` (e.g., a `RefCell<Reservation>` +
//!   a `&self` accessor returning `RefMut`) would hand out `&mut Reservation`
//!   from a shared reference and is NOT caught by this scan — only the trait
//!   impl pin (Class 3) and the method-set pin (Class 6) would slow it down.
//!
//! - `Reservation` gaining `Clone` would reopen an OWNED-handle leak this guard
//!   does NOT model — structurally impossible today (bare pointer + `Drop`: a
//!   `Clone` would double-release).
//!
//! - This scan catches references WRITTEN ON ONE CODE LINE, in any path-qualified
//!   or lifetime-qualified spelling. It does NOT claim to catch "a shared or
//!   exclusive reference from any function in return position or otherwise" as a
//!   blanket — that qualification requires spanning multiple lines or function
//!   bodies, which this line-local scan does NOT attempt.
//!
//! # Re-derivation pointers
//!
//! If this test fires, the correct response is to re-derive the argument in
//! docs/CORRECTNESS_OPEN_ITEMS.md item 66's card, referencing both tasks
//! #1104 and #1113, and update this test's expectation table in the SAME
//! change.

use std::fs;
use std::path::{Path, PathBuf};

const REDERIVE_MSG: &str = "the LazyReservation watermark-sealing surface changed: \
                            re-derive docs/CORRECTNESS_OPEN_ITEMS.md item 66's card and \
                            this test's expectation table in the SAME change \
                            (task #1104 / task #1113 / finding H1)";

fn is_code_line(line: &str) -> bool {
    let t = line.trim_start();
    !(t.starts_with("///") || t.starts_with("//!") || t.starts_with("//"))
}

/// Blank out doc/comment lines while preserving line structure, so block
/// markers (`impl LazyReservation`, `pub struct LazyReservation`) are only
/// searched in real code.
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

/// Recognizes a reference to `Reservation` in ANY spelling: `&` followed
/// (with optional whitespace) by an optional lifetime (`'ident`), then optional
/// `mut`, then zero or more path segments (`ident::`, allowing whitespace),
/// then the identifier `Reservation` NOT followed by an identifier char (word
/// boundary).
///
/// Matches:
/// - `&Reservation`, `&mut Reservation`
/// - `&'a Reservation`, `&'a mut Reservation`
/// - `&crate::Reservation`, `&mut crate::Reservation`
/// - `&self::Reservation`, `&mut self::Reservation`
/// - `& super::my_mod::Reservation`, etc.
///
/// Does NOT match:
/// - `ReservationFullParts`, `ReservationFoo` (word boundary)
/// - `&SomethingElse` (no `Reservation` at word boundary)
/// - `Reservation` without `&` (not a reference type)
///
/// Implementation: simple left-to-right scan, 30-40 lines.
fn contains_reservation_reference(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for '&'
        if bytes[i] == b'&' {
            let mut pos = i + 1;

            // Skip optional whitespace
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }

            // Skip optional lifetime ('ident)
            if pos < bytes.len() && bytes[pos] == b'\'' {
                pos += 1;
                // Skip lifetime identifier
                while pos < bytes.len()
                    && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_')
                {
                    pos += 1;
                }
                // Skip optional whitespace
                while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                    pos += 1;
                }
            }

            // Skip optional 'mut'
            if pos + 2 < bytes.len()
                && bytes[pos] == b'm'
                && bytes[pos + 1] == b'u'
                && bytes[pos + 2] == b't'
            {
                pos += 3;
                // Skip optional whitespace
                while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                    pos += 1;
                }
            }

            // Skip zero or more path segments (ident::, allowing whitespace)
            loop {
                // Skip optional whitespace before identifier
                while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                    pos += 1;
                }

                // Check if we're at 'Reservation' (case-sensitive)
                if pos + 11 <= bytes.len() && &bytes[pos..pos + 11] == b"Reservation" {
                    // Check word boundary: either end of line, or next char is not
                    // alphanumeric/underscore
                    let next_pos = pos + 11;
                    if next_pos >= bytes.len()
                        || !(bytes[next_pos].is_ascii_alphanumeric() || bytes[next_pos] == b'_')
                    {
                        // Found it!
                        return true;
                    }
                }

                // Look for path segment end '::' (allowing whitespace around it)
                let mut path_end = pos;
                while path_end < bytes.len()
                    && (bytes[path_end].is_ascii_alphanumeric() || bytes[path_end] == b'_')
                {
                    path_end += 1;
                }
                if path_end == pos {
                    // No identifier, stop looking for path segments
                    break;
                }

                // Skip whitespace after identifier
                while path_end < bytes.len() && bytes[path_end].is_ascii_whitespace() {
                    path_end += 1;
                }

                // Check for '::'
                if path_end + 1 < bytes.len()
                    && bytes[path_end] == b':'
                    && bytes[path_end + 1] == b':'
                {
                    pos = path_end + 2;
                    // Continue to look for more path segments or 'Reservation'
                } else {
                    // Not a path segment, stop
                    break;
                }
            }
        }
        i += 1;
    }

    false
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

// Task #1147: this is a text guard over `src/` (a `std::fs::read_dir` +
// `std::fs::read_to_string` walk of source files), not a check of anything
// miri interprets. Under miri's default filesystem isolation, `read_dir`
// aborts the whole interpreter run with "unsupported operation: `opendir`
// not available when isolation is enabled" (Linux CI) / "can't call foreign
// function `FindFirstFileExW`" (observed locally on Windows) — not a bug in
// this test or in `aligned-vmem`, just an operation miri's default sandbox
// refuses by design. Same reason and same fix as
// `tests/granted_huge_reader_enumeration.rs`'s identically-shaped
// `#[cfg_attr(miri, ignore)]`; that file discovered the abort first (it
// sorts alphabetically earlier, so under `cargo miri test`'s fail-fast this
// test's own miri failure was masked until confirmed independently). See
// `docs/CORRECTNESS_OPEN_ITEMS.md` item 84 for the recorded card.
#[cfg_attr(miri, ignore)]
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
    let mut trait_impls: Vec<String> = Vec::new();

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

        // Class 5: Token resurrection - as_reservation ban
        for line in src.lines().filter(|l| is_code_line(l)) {
            assert!(
                !line.contains("as_reservation"),
                "{rel}: `as_reservation` resurrected — the H1 bypass is back: {REDERIVE_MSG}"
            );
        }

        // Class 1: Path-qualified reference check (hand-rolled scanner)
        for line in src.lines().filter(|l| is_code_line(l)) {
            if contains_reservation_reference(line) {
                panic!(
                    "{rel}: a reference to `Reservation` exists in code — this is the H1 leak class \
                     (a borrowed `&Reservation` escape). Today such a reference is structurally read-only \
                     because the OS-state mutators take `&mut self`, but this pin exists so the borrow \
                     cannot come back silently and so any future relaxation of `&mut self` does not \
                     inherit a standing leak: {REDERIVE_MSG}"
                );
            }
        }

        // Class 2: Trait-shaped resurrection - generalized AsRef/Borrow check
        for line in src.lines().filter(|l| is_code_line(l)) {
            let has_asref = line.contains("AsRef");
            let has_borrow = line.contains("Borrow");
            let has_reservation = line.contains("Reservation");

            if (has_asref || has_borrow) && has_reservation {
                panic!(
                    "{rel}: an `AsRef`/`Borrow` impl targeting `Reservation` is a trait-shaped resurrection \
                     of the H1 borrow class (hands out `&Reservation` through auto-coercion). Today that reference \
                     is structurally read-only because the mutators require `&mut self`, but this pin exists \
                     as belt-over-suspenders (see header): {REDERIVE_MSG}"
                );
            }
            if line.trim_start().starts_with("type Target") && has_reservation {
                panic!(
                    "{rel}: a `type Target = ... Reservation` deref target is a trait-shaped resurrection \
                        of the H1 borrow class (hands out `&Reservation` through auto-deref). Today that \
                        reference is structurally read-only because the mutators require `&mut self`, but \
                        this pin exists as belt-over-suspenders (see header): {REDERIVE_MSG}"
                );
            }
        }

        // Class 3: Trait impls on LazyReservation - pin to { Debug } (per-line scan)
        // Class 6: Intrinsic method-set pin on impl LazyReservation
        // Class 4: Public fields on LazyReservation - ban
        let masked = mask_non_code(&src);

        // Class 3: Scan for trait impls on LazyReservation
        for line in masked.lines() {
            // Check if this line contains both "impl" and "for LazyReservation"
            if line.contains("impl") && line.contains("for LazyReservation") {
                // Verify "for LazyReservation" has a word boundary before "for"
                let for_pos = line.find("for LazyReservation").unwrap();
                if for_pos > 0 {
                    let char_before_for = line.chars().nth(for_pos - 1).unwrap();
                    // If the character before "for" is an identifier char (alphanumeric or _),
                    // this is a false match (e.g., somethingimplfor LazyReservation)
                    if char_before_for.is_ascii_alphanumeric() || char_before_for == '_' {
                        continue;
                    }
                }
                // Extract the text between "impl" and "for LazyReservation"
                let impl_pos = line.find("impl").unwrap();
                let trait_path = line[impl_pos + 4..for_pos].trim(); // Skip "impl"
                if !trait_path.is_empty() {
                    trait_impls.push(trait_path.to_string());
                }
            }
        }

        // Class 6: Intrinsic method-set pin on impl LazyReservation
        // Class 4: Public fields on LazyReservation - ban
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

        // Class 4: Scan for pub struct LazyReservation and check for pub fields
        rest = masked.as_str();
        while let Some(pos) = rest.find("pub struct LazyReservation") {
            let after_marker = &rest[pos..];
            let block_len = after_marker
                .find("\n}\n")
                .unwrap_or_else(|| panic!("lazy_reservation guard: `pub struct LazyReservation` block in {rel} has no column-0 closing brace"));
            let block = &after_marker[..block_len];
            for line in block.lines().filter(|l| is_code_line(l)) {
                let t = line.trim_start();
                // Skip the struct declaration line itself
                if t.starts_with("pub struct LazyReservation") {
                    continue;
                }
                // Check for `pub ` field declaration (must be followed by identifier then ':')
                // Pattern: "pub " then optional visibility spec, then identifier, then ":"
                if let Some(pub_pos) = t.find("pub ") {
                    let after_pub = &t[pub_pos + 4..];
                    let after_pub = after_pub.trim_start();
                    // Skip over optional visibility spec like "(crate)"
                    let mut field_start = after_pub;
                    if field_start.starts_with("(") {
                        if let Some(closing) = field_start.find(')') {
                            field_start = &field_start[closing + 1..];
                            field_start = field_start.trim_start();
                        }
                    }
                    // Check if this looks like a field (identifier followed by ':')
                    // A field declaration has the form: "pub name: Type"
                    if let Some(colon_pos) = field_start.find(':') {
                        let before_colon = &field_start[..colon_pos].trim();
                        // If there's something before ':' that looks like an identifier
                        if !before_colon.is_empty() {
                            // This is a field - check if it's a valid identifier
                            let is_ident = before_colon
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '_');
                            if is_ident {
                                panic!(
                                    "{rel}: `pub struct LazyReservation` contains a `pub` field — this hands \
                                     safe code `&mut Reservation` through `&mut LazyReservation`: {REDERIVE_MSG}"
                                );
                            }
                        }
                    }
                }
            }
            rest = &rest[pos + block_len..];
        }
    }

    // Class 3: Assert exactly { core::fmt::Debug } trait impls for LazyReservation
    trait_impls.sort();
    trait_impls.dedup();
    let expected_traits = ["core::fmt::Debug"];
    assert_eq!(
        trait_impls, expected_traits,
        "LazyReservation's trait impl set changed: {REDERIVE_MSG}"
    );

    // Class 6: Assert intrinsic method set is unchanged
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
