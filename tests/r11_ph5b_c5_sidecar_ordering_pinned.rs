//! Source pins for pending-bit publication/cuts and the production owner
//! issue (start-bit) ordering. D1′ phase-1b: the ClassLeaves inventory is
//! gone with the leaves — the only owner write left is the single-writer
//! start-bit set, pinned here against the production source.

/// Source text with CRLF normalised and `//` comment lines dropped.
fn src(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn bitmap_src() -> String {
    src(include_str!(
        "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"
    ))
}

fn scan_src() -> String {
    src(include_str!(
        "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"
    ))
}

/// Body of `fn <name>(` up to the end of its 4-space-indented block.
fn fn_body(text: &str, name: &str) -> String {
    let start = text
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("fn {name} present"));
    let rest = &text[start..];
    let end = rest
        .find("\n    }\n")
        .unwrap_or_else(|| panic!("end of fn {name}"));
    rest[..end].to_string()
}

#[test]
fn producer_publish_is_acqrel_fetch_or() {
    let body = fn_body(&bitmap_src(), "publish");
    assert!(
        body.contains(".fetch_or(1 << (granule % 64), Ordering::AcqRel)"),
        "producer publication must be an AcqRel fetch_or on the pending word:\n{body}"
    );
}

#[test]
fn owner_issue_start_store_is_release() {
    let body = fn_body(&bitmap_src(), "issue");
    assert!(
        body.contains(".store(old | mask, Ordering::Release)"),
        "issue must Release-store the start bit before any handoff:
{body}"
    );
    assert!(
        body.contains(".load(Ordering::Acquire)"),
        "issue must Acquire-load the start word first (single-writer load -> or -> store):
{body}"
    );
}

#[test]
fn owner_cut_is_acqrel_swap() {
    // D1′ merge: `next_cut` now has THREE cut swap(0) sites (the r18 bench
    // baseline path, its zero-word fast path and the plain production path).
    // The pin must hold for EVERY site: weakening any single site to a weaker
    // ordering is a mutant, not a refactor — a file-wide `contains` probe
    // would stay green and let the R5b-14 mutant survive.
    let body = fn_body(&scan_src(), "next_cut");
    assert_eq!(
        body.matches(".swap(0, Ordering::AcqRel)").count(),
        3,
        "every cut swap(0) in next_cut must stay AcqRel (pairs with publish):\n{body}"
    );
}
