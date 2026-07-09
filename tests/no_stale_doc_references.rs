//! Structural regression guard against doc-drift about REMOVED entities.
//!
//! The public `Heap` / `with_heap` alloc-face type was removed in 0.3.x
//! (task #204); `HeapCore` (registry-resident, magazine-backed) is the sole
//! surviving allocator face. Historically ~14 doc lines across 8 files still
//! referred to `Heap` in the present tense, including a BROKEN intra-doc link
//! `crate::heap::Heap::...` (task #17 cleaned them up). This test fails if any
//! such stale reference is reintroduced.
//!
//! Two independent checks over `src/**/*.rs`:
//!
//!   1. NO `crate::heap::` path anywhere — the `heap` module no longer exists,
//!      so any such intra-doc link is broken. Zero exceptions.
//!
//!   2. NO `` `Heap` `` doc-comment mention (the removed TYPE) outside the
//!      single allowed site: `registry/heap_core.rs`, whose module doc
//!      legitimately describes the removal in the PAST tense. Note this is
//!      distinct from the LIVE, unrelated `fallback::with_heap` internal
//!      function (a similar name, NOT the removed type) — that file contains
//!      no `` `Heap` `` type mention and so needs no exception.
//!
//! Doc-only guard: it reads source text, never links against the crate, so it
//! runs in every feature configuration.

use std::fs;
use std::path::{Path, PathBuf};

/// Collect every `*.rs` file under `dir` recursively.
fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("dir entry").path();
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

#[test]
fn no_crate_heap_module_path() {
    let mut files = Vec::new();
    rs_files(&src_dir(), &mut files);
    assert!(!files.is_empty(), "no source files found");

    let mut offenders = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read source");
        for (i, line) in text.lines().enumerate() {
            if line.contains("crate::heap::") {
                offenders.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the `crate::heap` module was removed in 0.3.x (task #204); \
         these are broken intra-doc links to the removed `Heap` type:\n{}",
        offenders.join("\n"),
    );
}

#[test]
fn no_removed_heap_type_doc_mentions() {
    // The ONLY file allowed to mention the removed `Heap` type — its module
    // doc records the removal in the past tense.
    let allowed = src_dir().join("registry").join("heap_core.rs");

    let mut files = Vec::new();
    rs_files(&src_dir(), &mut files);

    let mut offenders = Vec::new();
    for file in &files {
        if file == &allowed {
            continue;
        }
        let text = fs::read_to_string(file).expect("read source");
        for (i, line) in text.lines().enumerate() {
            // Backtick-quoted `Heap` doc mention of the removed TYPE. This does
            // NOT match `HeapCore`, `HeapSlot`, `HeapRegistry`, `with_heap`,
            // etc. — the trailing "`" requires an exact `` `Heap` `` token.
            if line.contains("`Heap`") {
                offenders.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the `Heap`/`with_heap` public alloc face was removed in 0.3.x \
         (task #204); `HeapCore` is the sole surviving face. These doc \
         comments still reference the removed `Heap` type:\n{}",
        offenders.join("\n"),
    );
}
