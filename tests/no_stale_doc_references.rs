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

/// Regression-guard against the SPECIFIC pre-task-H1 `thread_free` prose in
/// `registry/heap_core.rs`, `global/fallback.rs`, and `global/sefer_alloc.rs`.
///
/// Task #13 (the W3/H1 hoist) moved the cross-thread free-stack head OUT of an
/// inline `HeapCore` field into the owning `HeapSlot::thread_free` slot word
/// (and `FALLBACK_TFS` for the fallback heap). Task #31 rewrote the module-doc
/// and method-doc blocks in those two files that still described the OLD
/// mechanism (a `Box`-allocated stack, "install" as the binding step, an inline
/// head field). This test fails if any of those exact stale phrases reappear.
///
/// Task #38 additionally REMOVED the `install_thread_free` method itself (it
/// was a dead call on the TLS bind-slow path — `bind_thread_free` at claim
/// time, which runs strictly before `finish_bind`, already guarantees
/// `thread_free` is bound). So the bare token `install_thread_free` is now
/// ALSO banned outside the one file allowed to mention it historically
/// (`global/tls_heap.rs`'s `finish_bind` doc, which explains the removal in
/// the past tense — "this used to also call ..."). A reintroduced call site
/// or doc claiming the method still exists would be a genuine regression.
///
/// Doc-only guard: reads source text, never links the crate, so it runs in
/// every feature configuration.
#[test]
fn no_stale_pre_h1_thread_free_prose() {
    // (file, list-of-forbidden-substrings). Each substring is an exact phrase
    // removed by task #31 that would only reappear via a genuine regression.
    let cases: &[(&str, &[&str])] = &[
        (
            "registry/heap_core.rs",
            &[
                "ThreadFreeStack is Box-allocated",
                "`ThreadFreeStack` is `Box`-allocated",
                "hands out the address of the INLINE",
                "installed separately by\n    /// [`install_thread_free`]",
                "install_thread_free",
            ],
        ),
        (
            "global/fallback.rs",
            &[
                "already-initialised (in `new`) inline `thread_free` field",
                "wired purely from the stable inline field",
                "install_thread_free",
            ],
        ),
        ("global/sefer_alloc.rs", &["install_thread_free"]),
    ];

    let mut offenders = Vec::new();
    for (rel, needles) in cases {
        let path = src_dir().join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        for needle in *needles {
            if text.contains(needle) {
                offenders.push(format!("{}: stale phrase reintroduced: {needle:?}", rel));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "task #13 (H1) hoisted the cross-thread free-stack head out of an \
         inline `HeapCore` field into the owning slot's `thread_free` word \
         (and `FALLBACK_TFS`); task #31 rewrote the docs. These pre-H1 stale \
         phrases (Box-allocated stack / inline head field) were reintroduced:\n{}",
        offenders.join("\n"),
    );
}

/// Regression-guard against doc/comment drift back to the removed
/// abandon/adopt segment-transfer substrate (round4 task #97 / R4-5, commit
/// `65d441a`).
///
/// The abandoned-segments Treiber stack + adoption CAS (`HeapRegistry::{
/// abandon_segments, push_abandoned_segment, pop_abandoned_segment,
/// try_adopt}`, `Registry::abandoned_segs`, `OWNER_STATE_ABANDONED`) is gone;
/// TLS teardown (`src/global/tls_heap.rs`) does whole-slot reuse instead — the
/// `HeapCore` stays whole in its slot for the next claimant, nothing is
/// abandoned or adopted. This test bans the removed API's exact identifiers
/// outside the handful of files that legitimately name them in the PAST
/// tense while explaining the removal.
///
/// This is deliberately narrower than a blanket "abandon"/"adopt" word-stem
/// ban: those stems are ALSO the live [`AbandonGuard`] type name in
/// `global/tls_heap.rs` (the TLS destructor guard — a name that outlived the
/// behaviour it was named for, not renamed by this guard's scope), the
/// `ABANDONED_TAIL` sentinel used by the still-live `deferred_large`
/// cross-thread-free stack ([`crate::alloc_core::segment_header`]), and
/// "adopting thread" prose in `concurrent/sharded_region.rs` describing an
/// unrelated, still-live shard-reuse mechanism — all correct and not the
/// target of this guard.
///
/// Doc-only guard: reads source text, never links the crate, so it runs in
/// every feature configuration.
#[test]
fn no_stale_abandon_adopt_substrate_references() {
    // Exact identifiers from the removed API surface (commit 65d441a's
    // message enumerates the full removed list). None of these collide with
    // `AbandonGuard`, `ABANDONED_TAIL`, or generic "adopt"/"abandon" prose.
    let forbidden_tokens: &[&str] = &[
        "try_adopt",
        "abandon_segments",
        "push_abandoned_segment",
        "pop_abandoned_segment",
        "abandoned_segs",
        "OWNER_STATE_ABANDONED",
    ];

    // Files allowed to mention the removed identifiers — each does so only in
    // a module-doc sentence that explicitly frames it as removed/historical
    // (grep-verified at the time this guard was written; see the file diffs
    // for the exact "previously ... was removed" phrasing).
    let allowed: &[&str] = &["global/tls_heap.rs", "registry/bootstrap.rs"];
    let allowed_paths: Vec<PathBuf> = allowed
        .iter()
        .map(|rel| src_dir().join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)))
        .collect();

    let mut files = Vec::new();
    rs_files(&src_dir(), &mut files);

    let mut offenders = Vec::new();
    for file in &files {
        if allowed_paths.contains(file) {
            continue;
        }
        let text = fs::read_to_string(file).expect("read source");
        for (i, line) in text.lines().enumerate() {
            for token in forbidden_tokens {
                if line.contains(token) {
                    offenders.push(format!("{}:{}: {}", file.display(), i + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the abandon/adopt segment-transfer substrate was removed in round4 \
         (task #97 / R4-5, commit 65d441a); TLS teardown does whole-slot \
         reuse instead. These lines reference the removed API's identifiers \
         outside the files allowed to name them historically:\n{}",
        offenders.join("\n"),
    );
}

/// Regression-guard for a checkable NUMERIC claim in the overview docs.
///
/// `docs/ARCHITECTURE.md` states the count of integration-test files as
/// `tests/*.rs (<N> files, as of commit ...)`. That number silently rots every
/// time a test file is added or removed. This test recomputes the true count
/// and asserts the exact `(<N> files` token is present in ARCHITECTURE.md, so a
/// drift fails CI at the source rather than being discovered by a human reader.
///
/// Doc-only guard: it reads file names + doc text, never links the crate, so it
/// runs in every feature configuration. It is deliberately anchored to ONE
/// easy-to-automate claim (a file count via directory listing) rather than
/// attempting to parse every benchmark number out of markdown — wall-clock
/// numbers are host-dependent and their prose is too free-form to assert
/// robustly, so those are instead pinned to a dated "as of commit" freshness
/// stamp in the doc (an honest "may have drifted, re-verify" marker) rather than
/// a brittle exact-match test.
#[test]
fn architecture_test_file_count_matches_reality() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tests_dir = manifest.join("tests");

    let mut count = 0usize;
    for entry in fs::read_dir(&tests_dir).expect("read tests dir") {
        let path = entry.expect("dir entry").path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("rs") {
            count += 1;
        }
    }
    assert!(count > 0, "no test files found");

    let arch = manifest.join("docs").join("ARCHITECTURE.md");
    let text = fs::read_to_string(&arch).expect("read ARCHITECTURE.md");

    let needle = format!("({count} files");
    assert!(
        text.contains(&needle),
        "docs/ARCHITECTURE.md test-file count is stale: there are {count} \
         `tests/*.rs` files but the doc does not contain the token `{needle}`. \
         Update the `tests/*.rs (<N> files, as of commit ...)` line to {count}.",
    );
}
