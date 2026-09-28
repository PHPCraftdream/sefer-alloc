//! R1-07 tripwire (src review round 1,
//! `docs/reviews/2026-09-28-005939-src-review-oxx-round-1.md`): a
//! module-level `#![allow(unsafe_code)]` in a `mod.rs` silently widens the
//! lifted seam to every descendant module — including ones neither
//! `README.md`'s nor `src/lib.rs`'s unsafe inventory ever names.
//! `src/registry/bootstrap/mod.rs`'s blanket allow covered
//! `bootstrap::loom_shim` (a `#[cfg(loom)]`-only module with three `unsafe
//! impl`s and three `NonNull::new_unchecked` call sites) with no seam entry
//! of its own in either doc; `src/registry/heap_registry/mod.rs` carried a
//! second blanket allow that was a pure duplicate of its three children's
//! own tier-1 allows. Both fixed in the same commit as this test.
//!
//! Two structural guards, both doc/source-only (read source text, never
//! link the crate, so this test runs in every feature configuration):
//!
//! (a) [`no_mod_rs_carries_module_level_allow`] — no `mod.rs` under `src/`
//!     may carry a module-level `#![allow(unsafe_code)]`. `mod.rs` is
//!     reexports-only by this project's own convention (`CLAUDE.md`'s
//!     "mod.rs — reexports only, no code"), so it never legitimately needs
//!     to lift the crate-level `#![deny(unsafe_code)]` itself; the seam
//!     belongs to whichever child file actually contains the `unsafe` it
//!     protects.
//! (b) [`every_file_with_unsafe_carries_its_own_allow`] — every
//!     `src/**/*.rs` file that contains an `unsafe` token (outside line
//!     comments) must carry its OWN tier-1 (`#![allow(unsafe_code)]`) or
//!     tier-2 (`#[allow(unsafe_code)]`) allow somewhere in the file. A file
//!     relying on an ancestor `mod.rs`'s blanket allow would otherwise
//!     compile silently today and stop compiling the moment that ancestor's
//!     allow is removed (exactly what happened to `loom_shim.rs` here) —
//!     undetected by the existing doc-drift tests
//!     (`readme_unsafe_inventory_counts_match_reality` /
//!     `lib_rs_seam_inventory_matches_canonical_grep` in
//!     `tests/no_stale_doc_references.rs`), which recompute their expected
//!     sets FROM the grep of existing allows and so cannot notice a file
//!     that has an `unsafe` token but NO allow at all.
//!
//! ## Scanner limits (documented, not a soundness claim)
//!
//! The comment-stripping pass is LINE-ORIENTED and handles only `//` line
//! comments (which also strips `//!`/`///` doc comments, since they share the
//! `//` prefix); it does NOT parse `/* */` block comments or track
//! string-literal boundaries. Verified by inspection (and by this file's own
//! `find`-based sweep before writing it) that `src/` contains no `/* */`
//! block comment and no string literal containing the standalone word
//! `unsafe` — so this limitation is real but not currently load-bearing.
//! Tier-1/tier-2 coverage is checked at FILE granularity, not per-site: it
//! confirms the file contains at least one qualifying allow line somewhere,
//! not that a tier-2 allow sits immediately above the specific `unsafe`
//! block it is meant to cover. Per-site correlation (matching CLAUDE.md's
//! "a single documented reason to hold `unsafe`" rule for tier-2) is a
//! stronger property enforced by human zero-trust review at each phase
//! boundary, not mechanically by this test.

use std::fs;
use std::path::{Path, PathBuf};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Strip `//` line comments (naive — see module doc "Scanner limits").
fn strip_line_comments(text: &str) -> String {
    text.lines()
        .map(|line| match line.find("//") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// True iff `text` contains the standalone word `unsafe`, word-boundary
/// checked so it does not match inside a longer identifier (e.g.
/// `unsafe_code`, `is_unsafe_thing`).
fn contains_unsafe_token(text: &str) -> bool {
    let bytes = text.as_bytes();
    text.match_indices("unsafe").any(|(start, matched)| {
        let end = start + matched.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_ident_byte(bytes[end]);
        before_ok && after_ok
    })
}

fn has_tier1_allow(text: &str) -> bool {
    text.lines()
        .any(|l| l.trim_start().starts_with("#![allow(unsafe_code)]"))
}

fn has_tier2_allow(text: &str) -> bool {
    text.lines()
        .any(|l| l.trim_start().starts_with("#[allow(unsafe_code)]"))
}

#[test]
fn no_mod_rs_carries_module_level_allow() {
    let src = src_dir();
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    assert!(!files.is_empty(), "no source files found under src/");

    let offenders: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some("mod.rs"))
        .filter(|p| {
            let text = fs::read_to_string(p).expect("read source");
            has_tier1_allow(&text)
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "the following `mod.rs` files carry a module-level \
         `#![allow(unsafe_code)]`, silently widening the seam to every \
         descendant module (R1-07): {offenders:#?}\nMove the allow to the \
         child file(s) that actually contain `unsafe`; `mod.rs` is \
         reexports-only and never needs one.",
    );
}

#[test]
fn every_file_with_unsafe_carries_its_own_allow() {
    let src = src_dir();
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    assert!(!files.is_empty(), "no source files found under src/");

    let mut offenders = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path).expect("read source");
        let stripped = strip_line_comments(&text);
        if !contains_unsafe_token(&stripped) {
            continue;
        }
        if !(has_tier1_allow(&text) || has_tier2_allow(&text)) {
            offenders.push(path);
        }
    }

    assert!(
        offenders.is_empty(),
        "the following files contain an `unsafe` token (outside line \
         comments) but carry NEITHER a tier-1 (`#![allow(unsafe_code)]`) \
         NOR a tier-2 (`#[allow(unsafe_code)]`) allow anywhere in the file \
         — either the `unsafe` is relying on an ancestor `mod.rs`'s blanket \
         allow (which this project no longer permits — see \
         `no_mod_rs_carries_module_level_allow`), or it is missing an allow \
         entirely and would fail to compile as a hard `deny(unsafe_code)]` \
         error under any feature combination that reaches this file: \
         {offenders:#?}",
    );
}
