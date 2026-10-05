//! Ph5b C5 (task #2094): source pins for the production remote-free publication
//! orderings that `tests/loom_sidecar_bitmap.rs` models.
//!
//! The Loom model is a reduced-geometry SHADOW of `SidecarBitmap`: a weakened
//! production ordering cannot turn it red. These pins bind the model's three
//! atomic edges to the production sites — producer `publish` (`fetch_or`,
//! AcqRel), owner `issue` class store (Release) and owner word cut
//! (`swap(0, ..)`, AcqRel). A lost-publication mutant (e.g. `fetch_or` →
//! `Relaxed`) turns this file red.

/// Source text with CRLF normalised and `//` comment lines dropped.
fn src(text: &'static str) -> String {
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
fn owner_issue_class_store_is_release() {
    let body = fn_body(&bitmap_src(), "issue");
    assert!(
        body.contains(".store(class + 1, Ordering::Release)"),
        "issue must Release-store the class before any handoff:\n{body}"
    );
}

#[test]
fn owner_cut_is_acqrel_swap() {
    let text = scan_src();
    assert!(
        text.contains(".swap(0, Ordering::AcqRel)"),
        "the owner's word cut must be an AcqRel swap(0) (pairs with publish)"
    );
}
