//! Doc-accuracy regression guard for the "No-panic" contract in
//! `src/global/sefer_alloc/mod.rs` (R34-16, release-stabilization audit F-5).
//!
//! F-5 found a divergence between the module doc's claim "Every entry point
//! here returns null on failure and NEVER panics" and the code: five
//! release-surviving (not `debug_assert!`) invariant checks are reachable
//! from the `GlobalAlloc` impl under `production`. R34-16 resolved F-5 the
//! low-risk way (option (b)): the doc was rewritten to (1) keep the accurate
//! failure-path bullets, (2) enumerate the five tripwires as "abort by
//! design" defence-in-depth, and (3) state explicitly that a panic escaping
//! `GlobalAlloc` aborts via `#[rustc_nounwind]` (not UB), independent of any
//! downstream `panic = "abort"` setting.
//!
//! #1984 (alloc-core perf review P1-2) demoted the realloc ownership
//! re-check (former site 1) to `debug_assert!`, leaving FOUR release-surviving
//! tripwires. A later canonical-root change replaced that duplicate probe
//! with fallible `canonical_base_of(key)?`, where `key` is derived from the
//! payload address. This test pins the current mechanism and the four
//! remaining large-cache tripwires:
//!
//!   * **Code side:** the four remaining distinctive panic-message strings
//!     each appear exactly once in their expected source file, AND the
//!     former realloc site resolves the table-stored canonical root with `?`,
//!     checks the supplied base against that root or the payload-derived key,
//!     reconstructs the block from the stored root, and validates a Large
//!     payload's exact header offset before resizing. No release-surviving
//!     `assert!(` appears in that file.
//!
//!   * **Doc side:** `sefer_alloc.rs`'s "No-panic" section contains the
//!     qualifying language (`rustc_nounwind`, `invariant tripwire`), states
//!     the FOUR count (not the stale five), names the fallible canonical-root
//!     replacement,
//!     and does NOT contain the old unqualified overclaim. If the section is
//!     rewritten back to "NEVER panics" without the caveat, this fails.
//!
//! R2-08 (task #2010) later found point (3) above wrong: a DIRECT trait call
//! never passes through the `#[rustc_nounwind]` std shims, and even on the
//! `#[global_allocator]` path a pre-R2-08 panic was observed to unwind
//! through `__rust_alloc` rather than abort. The doc now states the
//! normative "`GlobalAlloc` methods must not unwind — upheld at the source"
//! rule instead; `no_panic_doc_is_qualified` also pins that the old
//! "Panic-in-`GlobalAlloc` is abort, not UB" claim stays gone.
//!
//! Doc/source-text only: never links against the crate, so it runs in every
//! feature configuration (mirrors `tests/no_stale_doc_references.rs`).

use std::fs;
use std::path::PathBuf;

fn src_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(rel)
}

fn read_src(rel: &str) -> String {
    fs::read_to_string(src_path(rel)).unwrap_or_else(|e| panic!("read src/{rel}: {e}"))
}

/// Assert `needle` occurs exactly `expected` times in `haystack`.
fn assert_count(haystack: &str, needle: &str, expected: usize, ctx: &str) {
    let actual = haystack.matches(needle).count();
    assert_eq!(
        actual, expected,
        "{ctx}: expected {expected} occurrence(s) of {needle:?}, found {actual}"
    );
}

#[test]
fn four_invariant_tripwires_pinned_by_message() {
    // Former site 1 resolves the payload-derived key to the stored root.
    // Reject an inconsistent supplied base before reconstructing the block;
    // a Large resize also requires the exact header payload offset.
    let core = read_src("alloc_core/alloc_core/mem/realloc_fastpath.rs");
    let squashed: String = core.split_whitespace().collect::<Vec<_>>().join(" ");
    let known_base = squashed
        .split_once("pub(super) fn realloc_inplace_fast_path_known_base(")
        .expect("known-base realloc function")
        .1
        .split_once("fn try_grow_large_reserved_capacity(")
        .expect("end of known-base realloc function")
        .0;
    let guards = [
        "let key = os::segment_base_of_ptr(ptr);",
        "let canonical = self.table.canonical_base_of(key)?;",
        "if base.addr() != canonical.addr() && base.addr() != key.addr() { return None; }",
        "let base = canonical;",
        "let ptr = crate::alloc_core::node::Node::deref(base, ptr.addr().wrapping_sub(base.addr()));",
        "let kind = SegmentHeader::kind_at(base);",
        "let payload_off = SegmentHeader::read_at(base).payload_offset;",
        "if ptr.addr() != base.addr() + payload_off { return None; }",
        "let span_usable = SegmentHeader::span_usable_at(base);",
    ];
    let mut remainder = known_base;
    for guard in guards {
        assert_count(known_base, guard, 1, "fallible realloc root/shape guard");
        remainder = remainder
            .split_once(guard)
            .unwrap_or_else(|| panic!("realloc guard is absent or out of order: {guard}"))
            .1;
    }
    assert!(
        !squashed.contains(" assert!("),
        "mem/realloc_fastpath.rs must contain no release-surviving `assert!(` — \
         a re-promotion (or a new release assert) regresses the no-panic \
         contract this guard pins"
    );

    // Sites 1–4 (renumbered from 2–5 after #1984) — large-cache slot take/set
    // helpers (alloc-decommit-gated, in `production`; gated out of some
    // configs, but the source text is feature-independent so this guard
    // still applies).
    let cache = read_src("alloc_core/large/alloc_core_large_cache.rs");
    assert_count(
        &cache,
        "large_cache_slot_take: empty base slot",
        1,
        "large_cache.rs site 1",
    );
    assert_count(
        &cache,
        "large_cache_slot_take: empty extension slot",
        1,
        "large_cache.rs site 2",
    );
    assert_count(
        &cache,
        "large_cache_slot_take: idx out of base range with extension disabled",
        1,
        "large_cache.rs site 3",
    );
    assert_count(
        &cache,
        "large_cache_slot_set: idx out of base range with extension disabled",
        1,
        "large_cache.rs site 4",
    );
}

#[test]
fn no_panic_doc_is_qualified() {
    let doc = read_src("global/sefer_alloc/mod.rs");

    // The old unqualified overclaim must be gone.
    assert!(
        !doc.contains("returns null on failure and NEVER panics"),
        "sefer_alloc.rs still carries the unqualified 'NEVER panics' overclaim (F-5 regression)"
    );

    // The shim mechanism must still be named — as what the crate does NOT
    // rely on (R2-08), not as an abort guarantee.
    assert!(
        doc.contains("rustc_nounwind"),
        "sefer_alloc.rs 'No-panic' section must name the #[rustc_nounwind] shims"
    );
    // R2-08: the shim-based "abort, not UB" claim is false for a direct trait
    // call (and was observed false on the #[global_allocator] path too); it
    // must not come back, and the normative no-unwind rule must stay.
    assert!(
        !doc.contains("Panic-in-`GlobalAlloc` is abort, not UB"),
        "sefer_alloc.rs reintroduced the R2-08 overclaim that a panic escaping \
         GlobalAlloc is a guaranteed abort via the #[rustc_nounwind] shims"
    );
    assert!(
        doc.contains("`GlobalAlloc` methods must not unwind"),
        "sefer_alloc.rs must state the normative R2-08 rule that GlobalAlloc \
         methods must not unwind (upheld at the source, not by the std shims)"
    );
    assert!(
        doc.contains("DIRECT trait call"),
        "sefer_alloc.rs must say a direct GlobalAlloc trait call bypasses the \
         std shims (R2-08)"
    );

    // The remaining four tripwires must be acknowledged as abort-by-design.
    assert!(
        doc.contains("invariant tripwire"),
        "sefer_alloc.rs 'No-panic' section must acknowledge the invariant tripwires"
    );

    // The enumeration count must stay in lockstep with the code: #1984
    // demoted the realloc site (FIVE → FOUR release-surviving tripwires), so
    // the doc must state FOUR and must not carry the stale FIVE heading.
    assert_count(
        &doc,
        "Four release-surviving invariant tripwires",
        1,
        "sefer_alloc.rs tripwire enumeration heading",
    );
    assert!(
        !doc.contains("Five release-surviving"),
        "sefer_alloc.rs still claims FIVE release-surviving tripwires — a \
         stale count (the realloc site was demoted to `debug_assert!` by \
         #1984)"
    );

    // Keep the historical demotion and current fallible replacement distinct.
    assert!(
        doc.contains("first demoted to `debug_assert!`"),
        "sefer_alloc.rs must preserve the #1984 demotion history"
    );
    assert!(
        doc.contains("fallible")
            && doc.contains("`canonical_base_of(key)?`")
            && doc.contains("payload-derived")
            && doc.contains("payload_offset"),
        "sefer_alloc.rs must describe the current no-panic canonical-root \
         resolution rather than a retained debug assertion"
    );
}

// Conservative lexical guard, not a Rust parser/feature resolver. Unknown
// feature predicates remain visible (both feature branches are audited).
const RELEASE_SCAN_FILES: &[&str] = &[
    "registry/heap_core/free/dealloc_own_base.rs",
    "registry/heap_core/free/dealloc.rs",
    "registry/heap_core/state/tcache_flush.rs",
    "registry/heap_core/state/ownership.rs",
    "registry/heap_core/alloc/hot.rs",
    "alloc_core/large/alloc_core_large_cache.rs",
    "alloc_core/alloc_core/mem/realloc_fastpath.rs",
];

// (file, exact source message, count, one-line reason). No magazine exceptions.
const RELEASE_ALLOWLIST: &[(&str, &str, usize, &str)] = &[
    (
        "alloc_core/large/alloc_core_large_cache.rs",
        "large_cache_slot_take: empty base slot",
        1,
        "Documented occupancy tripwire for a proven occupied base slot.",
    ),
    (
        "alloc_core/large/alloc_core_large_cache.rs",
        "large_cache_slot_take: empty extension slot",
        1,
        "Documented occupancy tripwire for a proven occupied extension slot.",
    ),
    (
        "alloc_core/large/alloc_core_large_cache.rs",
        "large_cache_slot_take: idx out of base range with extension disabled",
        1,
        "Documented take range tripwire when the extension is disabled.",
    ),
    (
        "alloc_core/large/alloc_core_large_cache.rs",
        "large_cache_slot_set: idx out of base range with extension disabled",
        1,
        "Documented insertion range tripwire when the extension is disabled.",
    ),
];

#[derive(Debug)]
struct Token {
    text: String,
    line: usize,
    message: Option<String>,
}

fn panic_tokens(source: &str) -> Vec<Token> {
    let b = source.as_bytes();
    let (mut i, mut line) = (0, 1);
    let mut out = Vec::new();
    while i < b.len() {
        let (start, token_line) = (i, line);
        if b[i].is_ascii_whitespace() {
            line += usize::from(b[i] == b'\n');
            i += 1;
            continue;
        }
        if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i..].starts_with(b"/*") {
            let mut depth = 1;
            i += 2;
            while i < b.len() && depth != 0 {
                if b[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    line += usize::from(b[i] == b'\n');
                    i += 1;
                }
            }
            continue;
        }
        let prefix = i + usize::from(matches!(b[i], b'b' | b'c'));
        let mut quote = prefix + 1;
        if b.get(prefix) == Some(&b'r') {
            while b.get(quote) == Some(&b'#') {
                quote += 1;
            }
        }
        let raw = b.get(prefix) == Some(&b'r') && b.get(quote) == Some(&b'"');
        let mut message = None;
        if raw || b.get(prefix) == Some(&b'"') {
            let hashes = if raw { quote - prefix - 1 } else { 0 };
            let opening = if raw { quote } else { prefix };
            i = opening + 1;
            let content = i;
            while i < b.len() {
                if b[i] == b'"'
                    && (!raw || b.get(i + 1..i + 1 + hashes) == Some(&b[prefix + 1..quote]))
                {
                    // Preserve escape spelling, rather than interpreting Rust.
                    message = Some(source[content..i].to_owned());
                    i += 1 + hashes;
                    break;
                }
                if !raw && b[i] == b'\\' {
                    i += 1;
                    if i == b.len() {
                        break;
                    }
                }
                line += usize::from(b[i] == b'\n');
                i += 1;
            }
        } else if b[i] == b'\'' && i + 1 < b.len() {
            // Characters are inert; lifetimes such as 'static are not chars.
            let width = source[i + 1..].chars().next().unwrap().len_utf8();
            let end = if b[i + 1] == b'\\' {
                source[i + 2..].find('\'').map(|n| i + 2 + n)
            } else {
                Some(i + 1 + width)
            };
            if let Some(end) = end.filter(|&end| b.get(end) == Some(&b'\'')) {
                line += source[i..=end].bytes().filter(|&c| c == b'\n').count();
                i = end + 1;
            } else {
                i += 1;
            }
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
        } else {
            i += source[i..].chars().next().unwrap().len_utf8();
        }
        out.push(Token {
            text: source[start..i].to_owned(),
            line: token_line,
            message,
        });
    }
    out
}

fn token_group_end(tokens: &[Token], start: usize) -> usize {
    let mut stack = Vec::new();
    for (i, t) in tokens.iter().enumerate().skip(start) {
        if t.message.is_some() {
            continue;
        }
        match t.text.as_str() {
            "(" => stack.push(")"),
            "[" => stack.push("]"),
            "{" => stack.push("}"),
            ")" | "]" | "}" => {
                assert_eq!(
                    stack.pop(),
                    Some(t.text.as_str()),
                    "unbalanced source at line {}",
                    t.line
                );
                if stack.is_empty() {
                    return i;
                }
            }
            _ => {}
        }
    }
    panic!("unterminated group at line {}", tokens[start].line);
}

// Three-valued evaluation: suppress only definitely false release cfgs.
fn release_cfg(tokens: &[Token]) -> Option<bool> {
    let first = tokens.first()?.text.as_str();
    if tokens.len() == 1 {
        return match first {
            "test" | "debug_assertions" => Some(false),
            _ => None,
        };
    }
    if tokens.get(1)?.text != "(" || tokens.last()?.text != ")" {
        return None;
    }
    let inner = &tokens[2..tokens.len() - 1];
    if first == "not" {
        return release_cfg(inner).map(|v| !v);
    }
    if !matches!(first, "all" | "any") {
        return None;
    }
    let (mut begin, mut depth) = (0, 0usize);
    let mut values = Vec::new();
    for (i, t) in inner.iter().enumerate() {
        match t.text.as_str() {
            "(" => depth += 1,
            ")" => depth -= 1,
            "," if depth == 0 => {
                values.push(release_cfg(&inner[begin..i]));
                begin = i + 1;
            }
            _ => {}
        }
    }
    if begin < inner.len() {
        values.push(release_cfg(&inner[begin..]));
    }
    let decisive = first == "any";
    if values.contains(&Some(decisive)) {
        Some(decisive)
    } else if values.iter().all(Option::is_some) {
        Some(!decisive)
    } else {
        None
    }
}

fn release_panic_sites(source: &str) -> Vec<(usize, String, String)> {
    let ts = panic_tokens(source);
    let mut sites = Vec::new();
    let mut i = 0;
    while i < ts.len() {
        let text = ts[i].text.as_str();
        if text == "#" && ts.get(i + 1).is_some_and(|t| t.text == "[") {
            let end = token_group_end(&ts, i + 1);
            let attr = &ts[i + 2..end];
            if attr.first().is_some_and(|t| t.text == "cfg")
                && attr.get(1).is_some_and(|t| t.text == "(")
                && attr.last().is_some_and(|t| t.text == ")")
                && release_cfg(&attr[2..attr.len() - 1]) == Some(false)
            {
                i = end + 1;
                while i + 1 < ts.len() && ts[i].text == "#" && ts[i + 1].text == "[" {
                    i = token_group_end(&ts, i + 1) + 1;
                }
                // Attached item/block/statement; signature groups don't end it.
                while i < ts.len() {
                    match ts[i].text.as_str() {
                        "{" => {
                            i = token_group_end(&ts, i) + 1;
                            break;
                        }
                        ";" => {
                            i += 1;
                            break;
                        }
                        "(" | "[" => i = token_group_end(&ts, i) + 1,
                        _ => i += 1,
                    }
                }
                continue;
            }
            i = end + 1;
            continue;
        }
        let macro_call = ts.get(i + 1).is_some_and(|t| t.text == "!")
            && ts
                .get(i + 2)
                .is_some_and(|t| matches!(t.text.as_str(), "(" | "[" | "{"));
        if text.starts_with("debug_assert") && macro_call {
            i = token_group_end(&ts, i + 2) + 1;
            continue;
        }
        let expect = text == "expect"
            && i > 0
            && ts[i - 1].text == "."
            && ts.get(i + 1).is_some_and(|t| t.text == "(");
        if expect || (matches!(text, "panic" | "unreachable") && macro_call) {
            let open = i + if expect { 1 } else { 2 };
            let end = token_group_end(&ts, open);
            let message = ts[open + 1..end]
                .iter()
                .find_map(|t| t.message.clone())
                .unwrap_or_else(|| "<nonliteral or absent message>".to_owned());
            sites.push((ts[i].line, text.to_owned(), message));
            // Keep scanning arguments to report nested calls too.
        }
        i += 1;
    }
    sites
}

#[test]
fn production_release_panic_sites_match_explicit_allowlist() {
    let mut found = Vec::new();
    let mut errors = Vec::new();
    for &file in RELEASE_SCAN_FILES {
        for (line, kind, message) in release_panic_sites(&read_src(file)) {
            if !RELEASE_ALLOWLIST
                .iter()
                .any(|&(f, m, _, _)| file == f && message == m)
            {
                errors.push(format!(
                    "{}:{line}: {kind}: {message:?}",
                    src_path(file).display()
                ));
            }
            found.push((file, line, kind, message));
        }
    }
    for &(file, message, count, reason) in RELEASE_ALLOWLIST {
        assert!(!reason.is_empty() && !reason.contains('\n'));
        let actual = found
            .iter()
            .filter(|(f, _, _, m)| *f == file && m == message)
            .count();
        if actual != count {
            errors.push(format!(
                "{}: {message:?}: expected {count}, found {actual}; {reason}",
                src_path(file).display()
            ));
        }
    }
    let inventory = found
        .iter()
        .map(|(file, line, kind, message)| {
            format!("{}:{line}: {kind}: {message:?}", src_path(file).display())
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(errors.is_empty(), "Unexpected release panic sites / allowlist drift:\n{}\nALL scanned release sites:\n{inventory}", errors.join("\n"));
}

#[test]
fn release_panic_scanner_handles_strings_comments_and_cfg() {
    let source = r####"
// .expect("comment")
/* panic!("block comment") /* nested */ */
const S: &str = r###"panic!("raw string") // inert"###;
let c = '(';
let lifetime: &'static str = "unreachable!(fake)";
debug_assert_eq!(value.expect("debug macro"), 1);
#[cfg(test)]
#[inline]
fn tests() { panic!("test item"); }
#[cfg(all(feature = "production", debug_assertions))]
{ value.expect("debug block"); }
#[cfg(any(test, debug_assertions))]
fn debug_only() { panic!("debug item"); }
#[cfg(not(debug_assertions))]
{ value
    .expect(
        "release // message with \"quotes\" and )",
    ); }
#[cfg(any(test, feature = "production"))]
fn maybe_release() { unreachable!(r#"raw release"#); }
panic!();
"####;
    let sites = release_panic_sites(source);
    assert_eq!(sites.len(), 3, "{sites:?}");
    assert_eq!(
        sites[0],
        (
            17,
            "expect".to_owned(),
            r#"release // message with \"quotes\" and )"#.to_owned()
        )
    );
    assert_eq!(sites[1].2, "raw release");
    assert_eq!(sites[2].2, "<nonliteral or absent message>");
}
