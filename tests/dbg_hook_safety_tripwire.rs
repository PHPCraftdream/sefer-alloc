//! Structural tripwire against the R25-1 / R29-7 / R29-8 bug class:
//! a **safe** `pub fn dbg_*` hook whose signature involves a raw pointer
//! (`*mut` / `*const`) and that touches allocator state through that pointer
//! WITHOUT being `pub unsafe fn` and WITHOUT being gated behind `bench-internals`.
//!
//! ## Why this test exists
//!
//! Three separate instances of the same soundness hole survived across four
//! rounds because no prior audit searched for **safe** `fn`s of this shape —
//! they searched for hooks *already* marked `unsafe fn` with a
//! production-satisfied gate (see the CLAUDE.md "Benchmark-only `dbg_*`
//! hooks..." rule). This test converts that rule into a compile-time-checked
//! invariant: it enumerates the at-risk set (safe, pointer-touching,
//! non-`bench-internals`-gated `dbg_*` hooks) and asserts it EXACTLY matches
//! a hardcoded, human-reviewed allowlist. A fourth instance cannot silently
//! land — it would appear as an un-accounted-for extra and fail this test.
//!
//! - **R25-1** (task #395): `HeapCore::dbg_overflow_bitmap_clear_pass` — a
//!   safe `pub fn` that derived a segment base from an arbitrary caller
//!   pointer and wrote allocator metadata (`clear_magazine`) at the derived
//!   offset, gated only on `alloc-global + fastbin` (both in `production`).
//! - **R29-7** (task #438, commit `3deb244`): `dbg_restore_local_for_test` —
//!   a safe `pub fn` that installed a caller-supplied raw pointer as this
//!   thread's live TLS `LOCAL` binding with zero validation.
//! - **R29-8** (task #439, commit `1bfa1d2`): `dbg_force_decommit_retain_for`
//!   — a safe `pub fn` that decommitted a segment's payload pages through a
//!   caller pointer whose segment base was containment-checked but whose
//!   `live_count == 0` precondition was not.
//!
//! ## What "safe by construction" means here (classification A)
//!
//! The vast majority of the allowlist follow this pattern: the caller-supplied
//! raw pointer is used ONLY to compute a segment base via address masking
//! (`os::segment_base_of_ptr` — pure arithmetic, no dereference); that base is
//! then verified against the allocator's own live-segment registry
//! (`contains_base_ro` / `segment_bases().any(..)` — value comparison, no
//! dereference of the base); and ONLY IF that check passes does the function
//! read a field from the segment's OWN header. Because the containment check
//! proves the base is a genuinely live, mapped, owned segment, the read target
//! is not attacker/caller-controlled — only the question "is this address one
//! of mine" is. Functions that return `None`/`false` on a failed containment
//! check and never touch memory otherwise belong here.
//!
//! ## Doc-only guard
//!
//! Reads source text, never links the crate, so it runs in every feature
//! configuration (mirrors `tests/no_stale_doc_references.rs`'s technique).

use std::fs;
use std::path::{Path, PathBuf};

/// Collect every `*.rs` file under `dir` recursively (mirrors the helper in
/// `tests/no_stale_doc_references.rs`).
fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Normalise an absolute source path to a forward-slashed relative path from
/// the crate root, e.g. `src/alloc_core/alloc_core_core_diag.rs`.
fn rel_id(abs: &Path) -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rel = abs.strip_prefix(manifest).unwrap_or(abs);
    rel.to_string_lossy().replace('\\', "/")
}

// ── Classification lists ──────────────────────────────────────────────────
//
// Every safe, non-`bench-internals`-gated `pub fn dbg_*` whose signature text
// contains `*mut` or `*const` MUST appear in exactly ONE of the three lists
// below. The test enumerates the live set from source and asserts it equals
// the union of all three (IDs only — the reason strings are for the human
// reviewer).
//
// ID format: `"relative/path.rs::fn_name"`.

/// (A) Reviewed safe by construction — each entry's reason is a terse summary
/// of the mask→base→guard→read-own-header invariant (or equivalent) that makes
/// the caller pointer non-load-bearing for soundness.
const REVIEWED_SAFE: &[(&str, &str)] = &[
    // ── src/alloc_core/alloc_core_core_diag.rs ──
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_node_id_for",
        "mask→base→contains_base_ro guard→read own SegmentMeta.node_id_of()",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_page_map_class_for",
        "mask→base→contains_base_ro guard→kind check→read own PageMap",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_contains_base",
        "mask→base→contains_base_ro (the guard IS the operation; no deref of base)",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_hash_contains_only",
        "value-based hash probe of caller base; hash_contains never derefs the key",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_segment_id_of",
        "mask→base→contains_base_ro assert→read own SegmentHeader.segment_id_at",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_kind_byte_of",
        "mask→base→contains_base_ro assert→read own header kind byte",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_kind_at_tag",
        "mask→base→contains_base_ro assert→read own header kind_at",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_large_size_of",
        "mask→base→contains_base_ro assert→read own header large_size field",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_span_usable_of",
        "mask→base→contains_base_ro assert→read own header span_usable field",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_reserved_capacity_of",
        "mask→base→contains_base_ro assert→read own header reserved_capacity field",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_payload_virgin_for",
        "mask→base→contains_base_ro guard→kind check→read own SegmentMeta.payload_virgin_of()",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_find_segment_with_free",
        "no pointer INPUT; returns own segment base from internal scan",
    ),
    (
        "src/alloc_core/alloc_core_core_diag.rs::dbg_directory_rescue_scan",
        "no pointer INPUT; returns own segment base from internal scan",
    ),
    // ── src/alloc_core/alloc_core_small_diag.rs ──
    (
        "src/alloc_core/alloc_core_small_diag.rs::dbg_carve_batch",
        "output buffer only (&mut [*mut u8]); writes freshly-carved ptrs, never reads caller values",
    ),
    (
        "src/alloc_core/alloc_core_small_diag.rs::dbg_freelist_head_for",
        "mask→base→contains_base_ro guard→class bounds→read own BinTable.head()",
    ),
    (
        "src/alloc_core/alloc_core_small_diag.rs::dbg_is_free_for",
        "mask→base→contains_base_ro guard→read own alloc_bitmap",
    ),
    (
        "src/alloc_core/alloc_core_small_diag.rs::dbg_committed_payload_end_for",
        "mask→base→contains_base_ro guard→kind check→read own SegmentMeta.committed_payload_end_of()",
    ),
    // ── src/alloc_core/alloc_core_small_pool.rs ──
    (
        "src/alloc_core/alloc_core_small_pool.rs::dbg_live_count_for",
        "mask→base→contains_base_ro guard→kind check→read own SegmentMeta.live_count_of()",
    ),
    (
        "src/alloc_core/alloc_core_small_pool.rs::dbg_is_decommitted_for",
        "mask→base→contains_base_ro guard→kind check→read own SegmentMeta.is_decommitted()",
    ),
    // ── src/alloc_core/alloc_core_small_reclaim.rs ──
    (
        "src/alloc_core/alloc_core_small_reclaim.rs::dbg_force_coarse_dirty_bit_for",
        "mask→base→contains_base_ro guard→read own segment_id→bounds-checked fetch_or into own dirty_segments[]",
    ),
    // ── src/registry/heap_core_diag.rs ──
    (
        "src/registry/heap_core_diag.rs::dbg_owner_id_for",
        "mask→base→segment_bases() containment guard→read own owner_state_atomic",
    ),
    (
        "src/registry/heap_core_diag.rs::dbg_last_stamped_segment",
        "no pointer param; returns stored field (pure accessor, no deref)",
    ),
    (
        "src/registry/heap_core_diag.rs::dbg_kind_at_tag",
        "delegates to (A)-classified AllocCore::dbg_kind_at_tag",
    ),
    (
        "src/registry/heap_core_diag.rs::dbg_segment_base_of_ptr",
        "pure address masking (os::segment_base_of_ptr); no deref",
    ),
    (
        "src/registry/heap_core_diag.rs::dbg_live_count_for",
        "delegates to (A)-classified AllocCore::dbg_live_count_for",
    ),
    (
        "src/registry/heap_core_diag.rs::dbg_contains_base",
        "delegates to contains_base (the guard itself; no deref of base)",
    ),
    (
        "src/registry/heap_core_diag.rs::dbg_hash_contains_only",
        "delegates to (A)-classified value-based hash probe; no deref of base",
    ),
];

/// (B) Known-unfixed: safe `pub fn dbg_*` hooks that dereference or mutate
/// allocator state through a caller pointer WITHOUT an adequate containment /
/// liveness guard. Listed separately so the test still PASSES — the tripwire
/// is for catching NEW instances, not for retroactively fixing (B) findings.
/// Each entry here must be resolved by its own dedicated soundness task (same
/// rigor as R29-7 / R29-8), then removed from this list.
const KNOWN_UNFIXED: &[(&str, &str)] = &[(
    "src/registry/heap_core_diag.rs::dbg_directory_bit_for_ptr",
    "(B) dereferences base via SegmentHeader::segment_id_at() WITHOUT a \
         contains_base / segment_bases guard — a null/foreign/arbitrary ptr \
         whose segment-aligned base is unmapped causes a read of unmapped \
         memory (crash), or of arbitrary mapped memory (garbage sid). The \
         sibling dbg_segment_id_of in alloc_core_core_diag.rs does the SAME \
         read but WITH an assert guard it explicitly calls 'the soundness fix \
         here'. Filed as a follow-up soundness task.",
)];

/// Heuristic false positives: the signature text contains `*mut`/`*const` but
/// the pointer is NOT a parameter or return type — it appears only in a
/// generic bound or similar non-load-bearing position. Enumerated by the scan
/// but not genuinely "pointer-touching"; listed here so the set-equality
/// assertion accounts for them.
const HEURISTIC_FALSE_POSITIVES: &[(&str, &str)] = &[(
    "src/alloc_core/alloc_core_small_reclaim.rs::dbg_drain_all_rings_checked",
    "*mut u8 appears only in the generic bound Fn(*mut u8, usize) -> bool, \
         not in any parameter or return type; the function takes a closure &F",
)];

// ── Source-scanning helpers ───────────────────────────────────────────────

/// Extract the full (possibly multi-line) `#[...]` attribute starting at
/// `lines[start]`. Returns the attribute text and the index of its last line.
/// Bracket-depth tracking: `#[cfg(all(` ... `))]` spans 4 lines.
fn extract_full_attribute(lines: &[&str], start: usize) -> (String, usize) {
    let mut text = String::new();
    let mut depth = 0i32;
    let mut end = start;
    for (i, line) in lines.iter().enumerate().skip(start) {
        for ch in line.chars() {
            match ch {
                '[' | '(' => depth += 1,
                ']' | ')' => depth -= 1,
                _ => {}
            }
        }
        text.push_str(line.trim());
        text.push('\n');
        end = i;
        if depth <= 0 {
            break;
        }
    }
    (text, end)
}

/// Scan `text` for complete `#[cfg(...)]` attribute blocks (using bracket
/// matching to handle multi-line cfgs) and return `true` if any contains the
/// feature-string `"bench-internals"`.
fn has_bench_internals_cfg(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 4 < bytes.len() {
        if &bytes[i..i + 5] == b"#[cfg" {
            let mut depth = 0i32;
            for j in i..bytes.len() {
                match bytes[j] {
                    b'[' | b'(' => depth += 1,
                    b']' | b')' => depth -= 1,
                    _ => {}
                }
                if depth <= 0 && j > i {
                    let attr = &text[i..=j];
                    if attr.contains("\"bench-internals\"") {
                        return true;
                    }
                    i = j + 1;
                    break;
                }
            }
            // If depth never returned to 0 (unterminated), advance past `#[`.
            if depth > 0 {
                i += 5;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// Collect the function-signature text from `lines[fn_idx]` up to and
/// including the first line containing `{` (the body delimiter).
fn collect_signature(lines: &[&str], fn_idx: usize) -> String {
    let mut sig = String::new();
    for line in lines.iter().skip(fn_idx) {
        sig.push_str(line);
        if line.contains('{') {
            break;
        }
    }
    sig
}

/// Extract the bare function name from a `pub fn dbg_foo(...)` / `pub fn
/// dbg_foo<...>(...)` line.
fn extract_fn_name(trimmed_line: &str) -> Option<&str> {
    let rest = trimmed_line.strip_prefix("pub fn ")?;
    let end = rest.find(|c: char| c == '(' || c == '<' || c.is_whitespace())?;
    Some(&rest[..end])
}

/// One candidate hook found during the scan.
struct Candidate {
    id: String,
    bench_internals_gated: bool,
}

/// Forward-scan one source file, returning every safe `pub fn dbg_*` whose
/// signature text contains `*mut` or `*const`, along with whether the
/// preceding attribute block gates it behind `bench-internals`.
fn scan_file(rel: &str, text: &str) -> Vec<Candidate> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    // Accumulated attribute text for the next item; cleared by doc comments,
    // other items, and blank lines (but NOT by `//` regular comments, which
    // can sit between an attribute block and the fn — see the R29-7 pattern
    // in tls_heap.rs).
    let mut attr_block = String::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();

        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            attr_block.clear();
            i += 1;
        } else if trimmed.starts_with("#[") {
            let (attr, end) = extract_full_attribute(&lines, i);
            attr_block.push_str(&attr);
            i = end + 1;
        } else if trimmed.starts_with("//") {
            i += 1;
        } else if trimmed.starts_with("pub fn dbg_") {
            let sig = collect_signature(&lines, i);
            if sig.contains("*mut") || sig.contains("*const") {
                if let Some(name) = extract_fn_name(trimmed) {
                    out.push(Candidate {
                        id: format!("{rel}::{name}"),
                        bench_internals_gated: has_bench_internals_cfg(&attr_block),
                    });
                }
            }
            attr_block.clear();
            i += 1;
        } else {
            // Any other line — `pub unsafe fn dbg_*` (already safe-by-marking),
            // blank lines, other items, code — resets the attribute accumulator.
            attr_block.clear();
            i += 1;
        }
    }
    out
}

#[test]
fn safe_dbg_pointer_hooks_match_reviewed_allowlist() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Collect source trees (same scope as the README unsafe-inventory grep).
    let mut files = Vec::new();
    rs_files(&manifest.join("src"), &mut files);
    rs_files(&manifest.join("crates"), &mut files);
    assert!(!files.is_empty(), "no source files found");

    // Enumerate the at-risk set: safe, pointer-touching, NOT bench-internals-gated.
    let mut found: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut gated_seen: Vec<String> = Vec::new();
    for file in &files {
        let rel = rel_id(file);
        let text =
            fs::read_to_string(file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        for c in scan_file(&rel, &text) {
            if c.bench_internals_gated {
                gated_seen.push(c.id);
            } else {
                found.insert(c.id);
            }
        }
    }

    // Build the expected set from all three classification lists.
    let mut expected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (id, _reason) in REVIEWED_SAFE
        .iter()
        .chain(KNOWN_UNFIXED)
        .chain(HEURISTIC_FALSE_POSITIVES)
    {
        assert!(
            expected.insert((*id).to_string()),
            "duplicate ID in classification lists: {id}"
        );
    }

    // Diff.
    let extras: Vec<_> = found.difference(&expected).cloned().collect();
    let missing: Vec<_> = expected.difference(&found).cloned().collect();

    assert!(
        extras.is_empty() && missing.is_empty(),
        "dbg_* pointer-hook safety tripwire mismatch.\n\n\
         This test enforces CLAUDE.md's benchmark-hook rule: every safe \
         `pub fn dbg_*` whose signature involves a raw pointer and is NOT \
         `bench-internals`-gated must be individually reviewed and listed.\n\n\
         NEW unaccounted-for hooks (not in any list — review and classify):\n  {}\n\n\
         Listed hooks no longer found in source (removed, renamed, or became \
         unsafe/bench-internals-gated — update the lists):\n  {}\n\n\
         (For reference, {} pointer-touching hooks were detected as \
         bench-internals-gated and correctly excluded from the at-risk set.)",
        extras.join("\n  "),
        missing.join("\n  "),
        gated_seen.len(),
    );
}
