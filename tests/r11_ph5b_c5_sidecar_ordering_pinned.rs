//! Source pins for pending-bit publication/cuts, the Dense test backing's
//! class store, and production ClassLeaves atomic operations and construction.
//! The reduced Loom shadow is not sensitive to production ordering changes;
//! class_leaves_publication_miri exercises the real mixed-leaf publication edge.

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

fn leaves_src() -> String {
    src(include_str!(
        "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap/leaf_classes.rs"
    ))
}

fn atomic_inventory(body: &str) -> Vec<(&str, Vec<&str>)> {
    let mut inventory = Vec::new();
    for (dot, _) in body.match_indices('.') {
        let rest = &body[dot + 1..];
        let name_end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let op = &rest[..name_end];
        let args = rest[name_end..].trim_start();
        if !args.starts_with('(') {
            continue;
        }
        let mut depth = 0;
        let end = args
            .char_indices()
            .find_map(|(i, c)| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                (depth == 0).then_some(i)
            })
            .expect("balanced method arguments");
        let orderings: Vec<_> = args[..end]
            .match_indices("Ordering::")
            .map(|(i, _)| {
                let ordering = &args[i + "Ordering::".len()..end];
                let len = ordering
                    .find(|c: char| !c.is_ascii_alphabetic())
                    .unwrap_or(ordering.len());
                &ordering[..len]
            })
            .collect();
        if !orderings.is_empty() {
            inventory.push((op, orderings));
        }
    }
    inventory
}

#[test]
fn class_leaves_ordering_inventory_is_exact() {
    let text = leaves_src();
    for (name, expected) in [
        (
            "prepared",
            vec![("load", vec!["Acquire"]), ("load", vec!["Acquire"])],
        ),
        (
            "prepare",
            vec![("load", vec!["Acquire"]), ("store", vec!["Release"])],
        ),
        (
            "issue",
            vec![
                ("load", vec!["Acquire"]),
                ("store", vec!["Release"]),
                ("load", vec!["Acquire"]),
                ("store", vec!["Release"]),
            ],
        ),
        (
            "encoded",
            vec![
                ("load", vec!["Acquire"]),
                ("load", vec!["Acquire"]),
                ("load", vec!["Acquire"]),
            ],
        ),
    ] {
        let body = fn_body(&text, name);
        assert_eq!(
            atomic_inventory(&body),
            expected,
            "ClassLeaves::{name}:\n{body}"
        );
    }
}

#[test]
fn mixed_leaf_published_after_its_initialization() {
    let body = fn_body(&leaves_src(), "prepare");
    let initialization = "for i in 0..LEAF_GRANULES {\n            unsafe { fresh.add(i).write(AtomicU8::new(old)) };\n        }";
    let publication = "self.mixed[leaf].store(fresh, Ordering::Release)";
    assert_eq!(
        body.matches(initialization).count(),
        1,
        "initialize every fresh slot:\n{body}"
    );
    assert_eq!(
        body.matches(publication).count(),
        1,
        "publish once:\n{body}"
    );
    assert!(
        body.find(initialization).unwrap() + initialization.len() < body.find(publication).unwrap(),
        "initialize the entire leaf before mixed publication:\n{body}"
    );
}

#[test]
fn production_small_sidecar_uses_class_leaves() {
    let text = src(include_str!(
        "../src/registry/segment_route/small_sidecar.rs"
    ));
    let body = fn_body(&text, "bitmap");
    assert_eq!(
        body.matches("SidecarBitmap::from_leaves(&self.pending, &self.summary, &self.classes)")
            .count(),
        1,
        "production bitmap must borrow ClassLeaves:\n{body}"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut dirs = vec![root];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).expect("read production source directory") {
            let entry = entry.expect("production source entry");
            let kind = entry.file_type().expect("source entry type");
            let path = entry.path();
            if kind.is_dir() {
                dirs.push(path);
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                let text = src(&std::fs::read_to_string(&path).expect("read Rust source"));
                for (i, _) in text.match_indices("from_initialized(") {
                    assert!(
                        text[..i].ends_with("fn "),
                        "no Dense constructor calls in production: {}",
                        path.display()
                    );
                }
            }
        }
    }
}

#[test]
fn producer_publish_is_acqrel_fetch_or() {
    let body = fn_body(&bitmap_src(), "publish_pending_half");
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
