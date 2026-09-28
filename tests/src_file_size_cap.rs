//! R1-11 tripwire (src review round 1,
//! `docs/reviews/2026-09-28-005939-src-review-oxx-round-1.md`): the repo's
//! 1000-line file-size cap (cited by `src/registry/heap_core_xthread/overflow.rs`
//! and `stall.rs`) had no check, and `src/registry/heap_overflow.rs` grew to
//! 1367 lines unnoticed. Root-crate `src/` only: the cap is this crate's
//! convention, not the published `crates/*` seams'. Source-text only, so
//! it runs in every feature configuration.

use std::fs;
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 1000;

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_source_file_exceeds_the_line_cap() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs(&root.join("src"), &mut files);
    assert!(
        files.len() > 100,
        "scanner found only {} files",
        files.len()
    );

    let mut over: Vec<String> = files
        .iter()
        .filter_map(|p| {
            let n = fs::read_to_string(p).expect("read source").lines().count();
            (n > MAX_LINES).then(|| format!("{} ({n} lines)", p.display()))
        })
        .collect();
    over.sort();
    assert!(
        over.is_empty(),
        "files over the {MAX_LINES}-line cap — split them (pure move):\n{}",
        over.join("\n")
    );
}
