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

use std::collections::BTreeSet;
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

/// Regression-guard against `Cargo.toml`'s `alloc-global` feature comment
/// reverting to the blanket "never panic/abort" overclaim (Sol-F3, task
/// #565, `docs/reviews/2026-08-05-sol-release-readonly-review.md` finding
/// F3, P1 release blocker).
///
/// Before this task, `Cargo.toml`'s `alloc-global` comment stated "Every
/// entry point is no-panic (a panic in a global allocator aborts the
/// process): checked branches return null on OOM, never panic/abort." This
/// is contradicted by two REAL, intentional, release-surviving termination
/// paths reachable from `SeferAlloc`'s `GlobalAlloc` impl:
///
///   1. Five invariant tripwires documented in `src/global/sefer_alloc.rs`'s
///      module doc (R34-16, task #535, pinned separately by
///      `tests/no_panic_doc_accuracy.rs`) — release `assert!`/`.expect()`/
///      `unreachable!()` sites kept as deliberate defence-in-depth, which
///      abort the process via the `#[rustc_nounwind]` `GlobalAlloc` shims.
///   2. `Registry::ensure_chunk` in `src/registry/bootstrap.rs` calls
///      `std::process::abort()` directly and unconditionally on
///      chunk-materialisation OOM on the ALLOC path (the free path is
///      fallible instead, via `slot_or_none`/`try_ensure_chunk`, R34-15).
///
/// This test pins three things so the Cargo.toml wording cannot silently
/// drift back to the false blanket claim without a conscious update:
///
///   * Cargo.toml's `alloc-global` comment block does NOT contain the old
///     unqualified overclaim ("never panic/abort" as a blanket guarantee).
///   * Cargo.toml's `alloc-global` comment block DOES point a reader at
///     `sefer_alloc.rs` (the tripwires doc) and at the direct
///     `std::process::abort()` call, so the corrected wording itself cannot
///     silently regress into vague prose that re-introduces the same gap.
///   * `src/registry/bootstrap.rs` still actually contains the
///     `std::process::abort()` call the corrected wording cites — if that
///     call is ever removed/softened, this test forces a conscious
///     Cargo.toml wording update rather than leaving a now-stale citation.
///
/// Doc-only guard: reads Cargo.toml + source text, never links the crate, so
/// it runs in every feature configuration.
#[test]
fn cargo_toml_alloc_global_panic_contract_is_accurate() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo = fs::read_to_string(manifest.join("Cargo.toml")).expect("read Cargo.toml");

    // Isolate the `alloc-global` feature's comment block (the lines
    // immediately above the `alloc-global = [...]` feature line) rather than
    // scanning the whole file, so this guard cannot be satisfied by
    // unrelated text elsewhere in Cargo.toml.
    let feature_line_idx = cargo
        .lines()
        .position(|l| l.trim_start().starts_with("alloc-global = ["))
        .expect("Cargo.toml must define `alloc-global = [\"...\", ...]`");
    let lines: Vec<&str> = cargo.lines().collect();
    let mut block_start = feature_line_idx;
    while block_start > 0 && lines[block_start - 1].trim_start().starts_with('#') {
        block_start -= 1;
    }
    let block = lines[block_start..feature_line_idx].join("\n");

    assert!(
        !block.contains("never panic/abort") || block.contains("NOT a blanket"),
        "Cargo.toml's `alloc-global` comment reintroduced the blanket \
         'never panic/abort' overclaim without the qualifying caveat (Sol-F3 \
         regression). Ordinary failure paths are no-op/null, but five \
         release-surviving invariant tripwires (src/global/sefer_alloc.rs) \
         and registry-chunk alloc-path OOM (src/registry/bootstrap.rs) \
         terminate the process by design. See \
         docs/reviews/2026-08-05-sol-release-readonly-review.md finding F3."
    );
    assert!(
        block.contains("sefer_alloc.rs"),
        "Cargo.toml's `alloc-global` comment must point readers at \
         `src/global/sefer_alloc.rs`'s module doc (the five release-surviving \
         invariant tripwires) — Sol-F3 regression guard."
    );
    assert!(
        block.contains("std::process::abort()"),
        "Cargo.toml's `alloc-global` comment must cite the direct \
         `std::process::abort()` call on the alloc path (registry chunk OOM, \
         `src/registry/bootstrap.rs`) — Sol-F3 regression guard."
    );

    // The corrected wording cites a REAL abort() call — if bootstrap.rs's
    // abort is ever removed/softened without updating Cargo.toml, fail here
    // instead of leaving a stale citation.
    let bootstrap = fs::read_to_string(manifest.join("src").join("registry").join("bootstrap.rs"))
        .expect("read src/registry/bootstrap.rs");
    assert!(
        bootstrap.contains("std::process::abort()"),
        "src/registry/bootstrap.rs no longer calls `std::process::abort()` \
         directly — Cargo.toml's `alloc-global` comment cites this call as \
         part of the accurate panic/abort contract (Sol-F3); if the abort \
         was removed or made fallible, update Cargo.toml's wording \
         accordingly rather than leaving a stale citation."
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

/// Regression-guard against doc-drift in the `unsafe` inventory counts in
/// `README.md`.
///
/// The README's "Where `unsafe` lives" section pins two verifiable counts as
/// exact tokens in its summary line below the tier-2 table: the tier-1
/// module-level seam count and the tier-2 item-scoped allow count + file
/// count. Those numbers silently rot every time a seam is added/removed or a
/// tier-2 `#[allow(unsafe_code)]` site is introduced (the R6 file-splits, for
/// example, scattered the old `segment_header.rs` / `heap_core.rs` tier-2
/// sites across new files and the README was not updated — task #203 /
/// DOCS-SYNC). This test recomputes the true counts from the SAME grep the
/// README documents as source-of-truth
/// (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`) and asserts
/// the README's tokens match, so a future drift fails CI at the source rather
/// than being discovered by a human reader.
///
/// Companion to `architecture_test_file_count_matches_reality` (which pins the
/// integration-test-file count the same way).
///
/// Doc-only guard: reads source text + README, never links the crate, so it
/// runs in every feature configuration.
#[test]
fn readme_unsafe_inventory_counts_match_reality() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Collect every `*.rs` file under src/ AND crates/ recursively (the same
    // two trees the README's grep command spans).
    let mut files = Vec::new();
    rs_files(&manifest.join("src"), &mut files);
    rs_files(&manifest.join("crates"), &mut files);
    assert!(
        !files.is_empty(),
        "no source files found under src/ or crates/"
    );

    // Replicate `^\s*#!?\[allow\(unsafe_code\)\]` from the README, split into
    // the two tiers: tier 1 = `#![...]` (module-level seam), tier 2 = `#[...]`
    // (item-scoped). Comment-proof by construction: after `trim_start()` a
    // `//`-prefixed line begins with `//`, not `#!?`+`[`, so it cannot match.
    const TIER1_PREFIX: &str = "#![allow(unsafe_code)]";
    const TIER2_PREFIX: &str = "#[allow(unsafe_code)]";

    let mut tier1 = 0usize;
    let mut tier2 = 0usize;
    let mut tier2_files: Vec<PathBuf> = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read source");
        for line in text.lines() {
            let stripped = line.trim_start();
            if stripped.starts_with(TIER1_PREFIX) {
                tier1 += 1;
            } else if stripped.starts_with(TIER2_PREFIX) {
                tier2 += 1;
                if !tier2_files.contains(file) {
                    tier2_files.push(file.clone());
                }
            }
        }
    }
    assert!(tier1 > 0, "no tier-1 `#![allow(unsafe_code)]` seams found");
    assert!(tier2 > 0, "no tier-2 `#[allow(unsafe_code)]` sites found");

    let readme = fs::read_to_string(manifest.join("README.md")).expect("read README.md");
    // Collapse whitespace runs (including newlines) to a single space so the
    // check is resilient to markdown line-wrapping — the tokens are pinned in
    // the prose summary, which may reflow across line breaks.
    let readme_flat: String = readme
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    // Tier-1 token: README summary says e.g. "**17** tier-1 module-level seams".
    let t1_needle = format!("**{tier1}** tier-1 module-level seams");
    assert!(
        readme_flat.contains(&t1_needle),
        "README.md tier-1 count drifted: re-grep found {tier1} `#![allow(unsafe_code)]` \
         module-level seams but the README does not contain the token `{t1_needle}`. \
         Update the tier-1 count in the summary line below the tier-2 table.",
    );

    // Tier-2 tokens: README summary says e.g. "**33** tier-2 item-scoped allows
    // across **14** files" — pin BOTH the site count and the distinct-file count.
    let t2_sites_needle = format!("**{tier2}** tier-2 item-scoped allows");
    let t2_files_needle = format!("across **{}** files", tier2_files.len());
    assert!(
        readme_flat.contains(&t2_sites_needle),
        "README.md tier-2 site count drifted: re-grep found {tier2} \
         `#[allow(unsafe_code)]` item-scoped sites but the README does not contain \
         the token `{t2_sites_needle}`. Update the tier-2 summary.",
    );
    assert!(
        readme_flat.contains(&t2_files_needle),
        "README.md tier-2 file count drifted: re-grep found tier-2 sites across \
         {} distinct files but the README does not contain the token \
         `{t2_files_needle}`. Update the tier-2 summary.",
        tier2_files.len(),
    );
}

/// Collapse all runs of ASCII whitespace (including newlines) to a single
/// space, so a token check is resilient to markdown line-wrapping. Shared by
/// the production-bundle and seam-inventory doc-drift guards below (the
/// `readme_unsafe_inventory_counts_match_reality` test above inlines the same
/// transform; this factored copy keeps the two new guards concise).
fn flatten_whitespace(s: &str) -> String {
    s.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse the `production = ["a", "b", ...]` feature line out of Cargo.toml,
/// returning the ordered list of feature names. String-only, no `toml`
/// dependency — the existing doc-drift guards in this file all avoid linking
/// the crate, and this keeps that property (so the test runs in every feature
/// configuration).
fn parse_production_feature_list(cargo: &str) -> Vec<String> {
    let line = cargo
        .lines()
        .find(|l| l.trim_start().starts_with("production = ["))
        .expect("Cargo.toml must define `production = [\"...\", ...]`");
    let start = line.find('[').unwrap() + 1;
    let end = line.rfind(']').unwrap();
    line[start..end]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Derive a Rust module path from a source file path relative to `src/`
/// (e.g. `src/alloc_core/sidecar.rs` -> `alloc_core::sidecar`). Returns `None`
/// for `mod.rs` / `lib.rs` (crate or parent-module roots, which have no own
/// path segment and are never themselves a named seam submodule).
fn file_to_module_path(src: &Path, file: &Path) -> Option<String> {
    let rel = file.strip_prefix(src).ok()?;
    let stem = rel.with_extension("");
    let last = stem.file_name()?.to_str()?;
    if last == "mod" || last == "lib" {
        return None;
    }
    let path: String = stem
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("::");
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// True iff `token` is a pure `::`-separated module path (at least two
/// segments, only `[a-zA-Z0-9_:]`). Rejects membrane-function paths that carry
/// `{...}` / `(...)` or lack `::`, so the seam-bullet extractor does not
/// confuse a function membrane (e.g. `alloc_core::node::{write_usize, ...}`)
/// with a seam module.
fn is_pure_module_path(token: &str) -> bool {
    token.contains("::")
        && !token.contains('{')
        && !token.contains('}')
        && !token.contains('(')
        && !token.contains(')')
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b':')
}

/// Extract the `` * `module::path` `` bullet entries from `src/lib.rs`'s
/// INTERNAL seam inventory — the `* `-prefixed list between the anchor
/// "confined modules lift this with" (the line that introduces the
/// `#![allow(unsafe_code)]` module list) and the conclusion "is enforced by
/// the compiler" (the line that closes it). Only pure `::`-separated module
/// paths are collected; membrane-function bullets (which live in the WIDER
/// soundness-boundary section below the conclusion) are rejected by
/// `is_pure_module_path` and, in any case, fall outside the bounded region.
fn extract_seam_bullets(lib_rs: &str) -> Vec<String> {
    let start_anchor = "confined modules lift this with";
    let end_anchor = "is enforced by the compiler";
    let start = lib_rs
        .find(start_anchor)
        .expect("lib.rs seam-inventory start anchor `confined modules lift this with`");
    let after_start = start + start_anchor.len();
    let end = lib_rs[after_start..]
        .find(end_anchor)
        .expect("lib.rs seam-inventory end anchor `is enforced by the compiler`");
    let region = &lib_rs[after_start..after_start + end];

    let mut seams = Vec::new();
    for line in region.lines() {
        // Strip the leading `//` comment prefix, then look for `* ` bullets.
        let stripped = line.trim_start_matches("//").trim_start();
        let after_star = match stripped.strip_prefix("* ") {
            Some(s) => s,
            None => continue,
        };
        // The first backtick-enclosed token is the module path.
        let open = match after_star.find('`') {
            Some(i) => i,
            None => continue,
        };
        let rest = &after_star[open + 1..];
        let close = match rest.find('`') {
            Some(i) => i,
            None => continue,
        };
        let token = &rest[..close];
        if is_pure_module_path(token) {
            seams.push(token.to_string());
        }
    }
    seams
}

/// Regression-guard against doc-drift in the textual `production` feature
/// bundle. (R34-21/task #540, release-stabilization audit finding F-10.)
///
/// `Cargo.toml`'s `production = [...]` line is the single source of truth for
/// which features the `production` bundle shorthand expands to. Several doc
/// sites spell the bundle out in prose (rather than just saying
/// "`production`") so a reader need not open Cargo.toml. Those prose
/// expansions silently rot every time a feature joins `production`
/// (R13-9/task #279 added `class-aware-dirty` and `primordial-lazy-commit` to
/// `production`, but four doc sites kept listing only the pre-R13-9 set —
/// unnoticed for over 20 rounds until the release-stabilization audit). This
/// test parses `production = [...]` from Cargo.toml, renders the feature list
/// in the `a + b + c` form the prose uses, and asserts every doc site that
/// writes the bundle out as a `+`-separated list contains the FULL canonical
/// list — catching both "bundle removed" and "stale partial list" drift.
///
/// Companion to `readme_unsafe_inventory_counts_match_reality` (which pins
/// the README's aggregate unsafe-seam COUNTS the same way).
///
/// Doc-only guard: reads Cargo.toml + source/README text, never links the
/// crate, so it runs in every feature configuration.
#[test]
fn production_feature_bundle_doc_sites_match_cargo_toml() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo = fs::read_to_string(manifest.join("Cargo.toml")).expect("read Cargo.toml");
    let features = parse_production_feature_list(&cargo);
    assert!(
        features.len() >= 2,
        "parsed an implausibly small `production` feature list: {features:?}"
    );

    // The rendered form used by every prose site: `feat1 + feat2 + ... + featN`.
    let canonical = features.join(" + ");
    // The longest prefix common to all stale forms seen at audit time (the
    // first 4 features). Every textual bundle description starts with this
    // prefix; a site is stale iff the prefix appears WITHOUT the full
    // canonical continuation. Anchoring on 4 features (not the whole list)
    // catches partial lists of any shorter length (4-, 5-, or 6-feature stale
    // forms), not just the exact pre-R13-9 shape.
    let anchor = if features.len() >= 4 {
        features[..4].join(" + ")
    } else {
        canonical.clone()
    };

    // Every doc site that writes the `production` bundle out as a
    // `+`-separated feature list (rather than just saying "`production`").
    let sites: &[&str] = &["src/lib.rs", "src/global/sefer_alloc.rs", "README.md"];

    let mut offenders = Vec::new();
    for rel in sites {
        let path = manifest.join(rel);
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let flat = flatten_whitespace(&text);

        // Walk every occurrence of the `+`-separated bundle prefix; each must
        // extend to the full canonical list. A prefix that stops short is a
        // stale partial bundle.
        let mut from = 0usize;
        let mut canonical_hits = 0usize;
        while let Some(rel_off) = flat[from..].find(&anchor) {
            let pos = from + rel_off;
            if flat[pos..].starts_with(&canonical) {
                canonical_hits += 1;
            } else {
                let end_ctx = (pos + 80).min(flat.len());
                let ctx = &flat[pos..end_ctx];
                offenders.push(format!(
                    "{rel}: stale/partial `production` bundle (does not list \
                     all {n} features) near: `{ctx}`",
                    n = features.len(),
                ));
            }
            from = pos + anchor.len();
        }
        if canonical_hits == 0 {
            offenders.push(format!(
                "{rel}: the canonical `production` bundle `{canonical}` does \
                 not appear at all — the doc site may have dropped the prose \
                 expansion entirely",
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "the `production` feature bundle in Cargo.toml is `{canonical}` \
         ({n} features), but these doc sites that spell the bundle out in \
         prose do not match (stale/partial or missing). Cargo.toml's \
         `production = [...]` is the single source of truth — update every \
         prose site to the full list:\n{}",
        offenders.join("\n"),
        n = features.len(),
    );
}

/// Regression-guard against doc-drift in `src/lib.rs`'s prose seam inventory.
/// (R34-21/task #540, release-stabilization audit finding F-9.)
///
/// `src/lib.rs`'s header comment inventories the INTERNAL tier-1
/// `#![allow(unsafe_code)]` seam modules, mirroring README §"Where unsafe
/// lives". That prose list silently rots every time a seam module is added
/// (R13-6 added `alloc_core::large_cache_extended` and R14-9/task #294 added
/// `alloc_core::sidecar`, but neither was added to the lib.rs header — the
/// crate root's "complete, verifiable picture" under-stated the seam set for
/// over 20 rounds). The sibling `readme_unsafe_inventory_counts_match_reality`
/// pins the README's AGGREGATE counts; this test pins the lib.rs prose LIST
/// against the SAME canonical grep
/// (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`), set-for-set,
/// so a missing or phantom seam fails CI at the source.
///
/// Doc-only guard: reads source text, never links the crate, so it runs in
/// every feature configuration.
#[test]
fn lib_rs_seam_inventory_matches_canonical_grep() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = manifest.join("src");

    // (1) Recompute the actual set of INTERNAL tier-1 seam modules from the
    // canonical grep, src/ tree only (the lib.rs header inventories INTERNAL
    // seams; the crates/ dependency seams are documented in a separate
    // section above it).
    let mut files = Vec::new();
    rs_files(&src, &mut files);
    assert!(!files.is_empty(), "no source files found under src/");

    const TIER1_PREFIX: &str = "#![allow(unsafe_code)]";
    let mut actual: BTreeSet<String> = BTreeSet::new();
    for file in &files {
        let text = fs::read_to_string(file).expect("read source");
        if text
            .lines()
            .any(|l| l.trim_start().starts_with(TIER1_PREFIX))
        {
            if let Some(module_path) = file_to_module_path(&src, file) {
                actual.insert(module_path);
            }
        }
    }
    assert!(!actual.is_empty(), "no tier-1 seams found under src/");

    // (2) Extract the documented set from lib.rs's prose seam inventory.
    let lib_rs = fs::read_to_string(src.join("lib.rs")).expect("read lib.rs");
    let documented: BTreeSet<String> = extract_seam_bullets(&lib_rs).into_iter().collect();
    assert!(
        !documented.is_empty(),
        "no seam bullets found in src/lib.rs header — the inventory region \
         anchors (`confined modules lift this with` / `is enforced by the \
         compiler`) may have moved"
    );

    // (3) Bidirectional set comparison — no missing, no phantom.
    let missing: Vec<&String> = actual.difference(&documented).collect();
    let phantom: Vec<&String> = documented.difference(&actual).collect();
    assert!(
        missing.is_empty() && phantom.is_empty(),
        "src/lib.rs's prose seam inventory drifted from the canonical grep \
         `grep -rnE '^\\s*#!?\\[allow\\(unsafe_code\\)\\]' src/ crates/` \
         (src/ tier-1 modules only):\n  actual tier-1 src/ seams ({}): \
         {:#?}\n  documented in lib.rs ({}): {:#?}\n  MISSING from lib.rs (a \
         real seam with no prose entry): {:#?}\n  PHANTOM in lib.rs \
         (documented but no real seam file): {:#?}\n  Add the missing \
         module(s) to lib.rs's INTERNAL-seam inventory (or remove a phantom), \
         matching the existing `* `module::path` — ...` bullet style.",
        actual.len(),
        actual,
        documented.len(),
        documented,
        missing,
        phantom,
    );
}

/// Parse the compile-time offset pinned by a `const _: () = assert!(
/// core::mem::offset_of!(PerClass, <field>) == <n>, ...)` block in
/// `registry/tcache.rs`. The `==` operand is the source of truth
/// (compile-time checked), so this is the canonical value the prose must
/// match. Returns `None` if the assert block is absent (e.g. a field was
/// removed or the assert renamed) so the caller can fail with a clear message.
fn parse_perclass_const_assert_offset(text: &str, field: &str) -> Option<u64> {
    let needle = format!("offset_of!(PerClass, {field}) ==");
    let pos = text.find(&needle)?;
    let after = &text[pos + needle.len()..];
    let digits: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// From `PerClass`'s F4 layout-prose region, extract the offset number the
/// doc claims for `field`. For the given field, the claim is the number
/// following the first "offset" token that appears after the field's
/// backtick-quoted `` `<field>` `` mention and before the next field's token
/// (so a later field's offset, or an incidental "offset" inside a rationale,
/// cannot be misread as this field's claim). Returns `None` if the field or
/// its offset claim is absent.
fn parse_perclass_prose_offset(region: &str, field: &str, all_fields: &[&str]) -> Option<u64> {
    let token = format!("`{field}`");
    let pos = region.find(&token)?;
    let after = &region[pos + token.len()..];
    // Bound the search window at the earliest next `` `<other_field>` ``
    // mention so this field's number is not confused with a sibling's.
    let mut window_end = after.len();
    for other in all_fields {
        if *other == field {
            continue;
        }
        let other_tok = format!("`{other}`");
        if let Some(p) = after.find(&other_tok) {
            window_end = window_end.min(p);
        }
    }
    let window = &after[..window_end];
    let off = window.find("offset")?;
    let digits: String = window[off + "offset".len()..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Regression-guard against doc-drift between `PerClass`'s documented field
/// offsets and its compile-time `offset_of!` const-asserts.
/// (R34-22/task #541, bench-review finding P3 item 6.)
///
/// `PerClass`'s struct doc comment spells out the `#[repr(C)]` field layout
/// in prose ("`count` at offset 0, `virgin_mask` (when present) at offset 2,
/// `slots` ... starting at offset 8"). Those prose numbers are load-bearing
/// documentation of the magazine cache-line locality claim, but they are
/// hand-typed and silently disagree with the const-asserts that actually pin
/// the layout if someone edits one without the other. This was the exact
/// R34-22 finding: the prose said `virgin_mask` "at offset 1" while the
/// `offset_of!(PerClass, virgin_mask) == 2` const-assert (with its "1 pad
/// byte after u8 count" rationale) said 2 — the assert is the source of
/// truth (compile-time checked) and the prose was stale. This test parses
/// each `offset_of!(PerClass, <field>) == N` const-assert and asserts the
/// struct's doc prose mentions the SAME N for that field, so a future
/// prose/assert drift fails CI at the source rather than being discovered by
/// a human reviewer.
///
/// `virgin_mask` (the field that actually drifted) is the motivating case;
/// `count`/`slots` are the sibling claims in the same prose sentence and
/// fall under the same drift class, so they are pinned too at no extra cost.
///
/// Doc-only guard: reads source text, never links the crate, so it runs in
/// every feature configuration (the `#[cfg(feature = "virgin-zero-skip")]` on
/// the `virgin_mask` assert is irrelevant — the assert TEXT is in the file
/// regardless of features).
#[test]
fn perclass_doc_offsets_match_const_asserts() {
    let path = src_dir().join("registry").join("tcache.rs");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    // The three fields whose offsets are both prose-documented AND pinned by a
    // const-assert in the same file.
    let fields: &[&str] = &["count", "virgin_mask", "slots"];

    // Extract the F4 prose region that lists the offsets as one sentence,
    // bounded by the anchors that introduce and close it. If either anchor
    // moved, the layout paragraph was rewritten and this test's anchors need
    // updating — fail loudly rather than silently passing on an empty region.
    let start_anchor = "fixes the field order to declaration order";
    let end_anchor = "see the `offset_of!` const-asserts";
    let region = text.find(start_anchor).and_then(|s| {
        let after_s = s + start_anchor.len();
        text[after_s..]
            .find(end_anchor)
            .map(|e| &text[after_s..after_s + e])
    });
    let region = match region {
        Some(r) => r,
        None => panic!(
            "the F4 layout prose anchors (`{start_anchor}` ... `{end_anchor}`) \
             were not found in registry/tcache.rs — has the layout paragraph \
             been rewritten? Update this test's anchors to match."
        ),
    };

    let mut offenders = Vec::new();
    for field in fields {
        let Some(assert_val) = parse_perclass_const_assert_offset(&text, field) else {
            offenders.push(format!(
                "no `offset_of!(PerClass, {field}) == N` const-assert found in \
                 registry/tcache.rs — was the assert removed or renamed?"
            ));
            continue;
        };
        match parse_perclass_prose_offset(region, field, fields) {
            Some(prose_val) if prose_val == assert_val => {}
            Some(prose_val) => offenders.push(format!(
                "field `{field}`: prose says offset {prose_val} but the \
                 compile-time `offset_of!` const-assert says {assert_val}. The \
                 assert is the source of truth (compile-checked); update the \
                 prose to match."
            )),
            None => offenders.push(format!(
                "field `{field}`: no `offset <n>` prose claim found in the F4 \
                 layout paragraph — was `{field}`'s offset mention removed?"
            )),
        }
    }

    assert!(
        offenders.is_empty(),
        "PerClass's documented field offsets in registry/tcache.rs disagree \
         with its own `offset_of!` const-asserts (the compile-time source of \
         truth). The const-asserts pin the #[repr(C)] magazine cache-line \
         layout; the prose must state the SAME numbers. Drifts:\n{}",
        offenders.join("\n"),
    );
}

/// Regression-guard for the THIRD link in a chain
/// `tests/dirty_by_class_sidecar_sizing_tripwire.rs`'s v4 tripwire (R19-9,
/// task #345) only pins two of: (1) the REAL compiled constants
/// (`AllocCore::dbg_small_class_count`/`dbg_words_per_class`) agree with (2)
/// that test file's own `EXPECTED_BYTES` constants, per feature combo. This
/// test checks the missing link 2 <-> 3: that `EXPECTED_BYTES` also agrees
/// with (3) the PROSE snapshot numbers in
/// `src/alloc_core/dirty_by_class.rs`'s "## Sizing and lazy materialisation"
/// module-doc section. Editing that prose to a wrong number left the
/// existing tripwire green (it never reads the doc comment); this test reads
/// the doc comment's source text and pins the exact byte-count tokens for
/// all three feature combos, so such a doc-only drift now fails here instead.
///
/// Deliberately does NOT depend on `AllocCore::dbg_*` (unlike the sidecar
/// tripwire, which needs `alloc-segment-directory` compiled in to call those
/// debug accessors) — it only reads source text, following this file's
/// existing "doc-only guard... runs in every feature configuration" pattern,
/// so the three literals below are cross-checked against
/// `dirty_by_class_sidecar_sizing_tripwire.rs`'s own `EXPECTED_BYTES`
/// constants by inspection/duplication, not by linking the crate. If a
/// future change to `MAX_SEGMENTS`/`SMALL_CLASS_COUNT`/`WORDS_PER_CLASS`
/// updates the sidecar tripwire's `EXPECTED_BYTES` constants, the SAME change
/// must update the three literals here (and the doc comment prose itself) —
/// exactly the discipline the sidecar tripwire's own doc comment already
/// demands of `dirty_by_class.rs`'s prose.
#[test]
fn dirty_by_class_doc_snapshot_matches_sidecar_tripwire_expected_bytes() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = fs::read_to_string(
        manifest
            .join("src")
            .join("alloc_core")
            .join("dirty_by_class.rs"),
    )
    .expect("read dirty_by_class.rs");

    // Must match `dirty_by_class_sidecar_sizing_tripwire.rs`'s
    // `EXPECTED_BYTES` for `#[cfg(not(feature = "medium-classes"))]`.
    const EXPECTED_BYTES_DEFAULT: usize = 25_088;
    // Must match `EXPECTED_BYTES` for
    // `#[cfg(all(feature = "medium-classes", not(feature = "medium-classes-wide")))]`.
    const EXPECTED_BYTES_MEDIUM: usize = 28_160;
    // Must match `EXPECTED_BYTES` for `#[cfg(feature = "medium-classes-wide")]`.
    const EXPECTED_BYTES_WIDE: usize = 29_696;

    let cases: &[(&str, usize)] = &[
        ("the default 49-class table", EXPECTED_BYTES_DEFAULT),
        ("55 classes under `medium-classes`", EXPECTED_BYTES_MEDIUM),
        (
            "58 classes under `medium-classes-wide`",
            EXPECTED_BYTES_WIDE,
        ),
    ];

    let mut offenders = Vec::new();
    for (label, expected) in cases {
        // The doc prose spells byte counts with a thousands separator, e.g.
        // "25,088 bytes" — match the exact comma-grouped token so a stray
        // digit edit is caught.
        let grouped = group_thousands(*expected);
        let needle = format!("{grouped} bytes");
        if !text.contains(&needle) {
            offenders.push(format!(
                "expected token `{needle}` ({label}) not found in \
                 src/alloc_core/dirty_by_class.rs's \"## Sizing and lazy \
                 materialisation\" doc comment"
            ));
        }
    }

    assert!(
        offenders.is_empty(),
        "dirty_by_class.rs's doc-comment snapshot prose has drifted from \
         tests/dirty_by_class_sidecar_sizing_tripwire.rs's own EXPECTED_BYTES \
         constants (which are themselves re-derived from the real compiled \
         constants, see that test's own tripwire). Update BOTH the doc \
         comment's \"## Sizing and lazy materialisation\" prose and, if the \
         underlying constants changed, EXPECTED_BYTES in both that test and \
         this one, in the same change:\n{}",
        offenders.join("\n"),
    );
}

/// Format `n` with comma thousands separators, e.g. `25088` -> `"25,088"`.
/// Small, local, and only used by the doc-snapshot check above — not worth a
/// dependency for a single call site.
fn group_thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// Tripwire against the R29-11 (task #442) gap: every `honest-reject` section
/// heading in `docs/perf/*.md` must be indexed somewhere in
/// `docs/perf/OPEN_ITEMS.md`.
///
/// `docs/perf/IAI_BASELINE.md` carries eight trigger-bearing honest-reject
/// sections (X4/X5/X6/G1/T10/R1/R3/R5-R2b). From R3's first appearance
/// (2026-07-13) through R29, none of these was ever indexed in
/// `OPEN_ITEMS.md` — the scope rule technically covered `docs/perf/*.md`, but
/// no round's start ritual surfaced the file because it was never *named*
/// there. R29-11 migrated the seven still-unindexed ones (R3 was already
/// partially closed by R29-10's item 17) and named the file explicitly in
/// `OPEN_ITEMS.md`'s Scope. This guard keeps that closed: any future
/// `## <TOKEN> ... honest-reject ...` heading added to a `docs/perf/*.md` file
/// whose `TOKEN` does not appear in `OPEN_ITEMS.md` fails the build.
///
/// It is deliberately a cheap grep-shaped check, NOT a markdown parser: it
/// matches only H2 headings (`## `) containing the literal substring
/// `honest-reject`, and extracts the first whitespace-delimited token after
/// `##` (e.g. `X4`, `G1`, `T10`, `R5-R2b`). Token-boundary matching avoids
/// `R1` matching inside `R10`/`R17` while still matching the whole `R5-R2b`.
/// The corpus was verified clean at introduction (the only `## ...
/// honest-reject` headings in `docs/perf/*.md` are the eight in
/// `IAI_BASELINE.md`; other perf docs mention `honest-reject` only in prose).
///
/// Doc-only guard: reads doc text, never links the crate, so it runs in every
/// feature configuration.
#[test]
fn honest_reject_sections_are_indexed() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let perf_dir = manifest.join("docs").join("perf");
    let index_path = perf_dir.join("OPEN_ITEMS.md");
    let index_text = fs::read_to_string(&index_path).expect("read docs/perf/OPEN_ITEMS.md");

    // Collect every `## <TOKEN> ... honest-reject ...` heading across the
    // perf-doc corpus, with its file + line for the diagnostic.
    let mut md_files = Vec::new();
    collect_md_files(&perf_dir, &mut md_files);
    assert!(!md_files.is_empty(), "no .md files found under docs/perf");

    let mut missing: Vec<String> = Vec::new();
    for file in &md_files {
        let text =
            fs::read_to_string(file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        for (i, line) in text.lines().enumerate() {
            // Only real H2 headings that contain "honest-reject". This matches
            // both "honest-reject" and "honest-rejects" (X4's plural heading).
            if !(line.starts_with("## ") && line.contains("honest-reject")) {
                continue;
            }
            let token = match line.trim_start_matches('#').split_whitespace().next() {
                Some(t) => t,
                None => continue,
            };
            if !token_appears(&index_text, token) {
                missing.push(format!(
                    "{}:{}: heading token `{}` not indexed in docs/perf/OPEN_ITEMS.md",
                    file.display(),
                    i + 1,
                    token
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "every `## <TOKEN> honest-reject` heading in docs/perf/*.md must be \
         indexed in docs/perf/OPEN_ITEMS.md (see R29-11/task #442 and the \
         `IAI_BASELINE.md` named-source note in OPEN_ITEMS.md's Scope). These \
         honest-reject section tokens are missing from the index — add an \
         [L]-tier entry citing the section, or, if the reject is superseded, \
         note it under the relevant existing item:\n{}",
        missing.join("\n"),
    );
}

/// Recursively collect every `.md` file under `dir` into `out`.
fn collect_md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_md_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

/// Does `token` appear in `haystack` as a bounded token — i.e. the characters
/// immediately before and after the match (if any) are non-alphanumeric?
/// `R1` thus does NOT match inside `R10`/`R17`, while the whole `R5-R2b`
/// (whose internal `-` is non-alphanumeric) still matches as a contiguous
/// unit bounded on both sides by non-alphanumeric characters.
fn token_appears(haystack: &str, token: &str) -> bool {
    let bytes = haystack.as_bytes();
    let tb = token.as_bytes();
    let n = tb.len();
    if n == 0 || n > bytes.len() {
        return false;
    }
    let is_alnum = |c: u8| c.is_ascii_alphanumeric();
    let mut from = 0usize;
    while let Some(rel) = haystack[from..].find(token) {
        let start = from + rel;
        let end = start + n;
        let left_ok = start == 0 || !is_alnum(bytes[start - 1]);
        let right_ok = end == bytes.len() || !is_alnum(bytes[end]);
        if left_ok && right_ok {
            return true;
        }
        from = start + 1;
        if from + n > bytes.len() {
            break;
        }
    }
    false
}

/// One row of `docs/FEATURE_PROMOTION_STATUS.md`'s survey table, as needed by
/// [`every_undecided_feature_has_exactly_one_owner_with_a_next_trigger`].
struct SurveyRow {
    /// The `feature` column, e.g. "`exact-span-large`" (backticks included,
    /// as written in the table — only used for diagnostic messages).
    feature: String,
    /// 1-indexed line number in `FEATURE_PROMOTION_STATUS.md`, for diagnostics.
    line: usize,
    /// Every distinct `OPEN_ITEMS.md` item number cited in this row's
    /// `evidence citation` column, in the order they appear.
    owner_items: Vec<u32>,
}

/// Parse `docs/FEATURE_PROMOTION_STATUS.md`'s `## Survey table` into rows.
///
/// Deliberately a plain pipe-split over `| col | col | ... |` lines, matching
/// this file's established style (`readme_unsafe_inventory_counts_match_reality`,
/// `honest_reject_sections_are_indexed`) rather than a real markdown-table
/// parser — the table has a fixed 5-column shape
/// (`feature | shipped-behind-flag | has-gate-report | promotion verdict |
/// evidence citation`) that a `|`-split handles exactly, and this test only
/// needs the 1st (feature), 4th (verdict), and 5th (evidence) columns.
///
/// A row is collected only if column 1, trimmed, starts with a backtick
/// (`` ` ``) — this excludes the header row (`| feature | ... |`) and the
/// `|---|---|...|` separator row, which do not.
fn parse_survey_rows(text: &str) -> Vec<SurveyRow> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
            continue;
        }
        let cols: Vec<&str> = trimmed
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cols.len() != 5 {
            continue;
        }
        let feature = cols[0];
        if !feature.starts_with('`') {
            continue; // header row ("feature") or separator row ("---")
        }
        let verdict = cols[3];
        let evidence = cols[4];
        // Only rows whose OWN verdict cell literally carries CONDITIONAL-GO or
        // NEVER-DECIDED are in scope. This deliberately excludes an alias row
        // like `alloc-lazy-commit` ("**reduces to `small-segment-lazy-commit`**")
        // — its verdict text names neither marker, so it never entered this
        // scan in the first place; the alias intentionally shares its target
        // feature's owner rather than needing its own.
        if !(verdict.contains("CONDITIONAL-GO") || verdict.contains("NEVER-DECIDED")) {
            continue;
        }
        rows.push(SurveyRow {
            feature: feature.to_string(),
            line: i + 1,
            owner_items: extract_open_items_citations(evidence),
        });
    }
    rows
}

/// Extract every `OPEN_ITEMS.md` item number cited in `text`, matching the
/// established citation phrasing `` `OPEN_ITEMS.md` item N `` (used
/// consistently across this file's evidence columns) — order-preserving,
/// duplicates within the same citation text kept as-is (callers that need
/// "distinct owners" dedupe separately; a single row citing item N twice in
/// its own prose is not the failure mode this test targets).
fn extract_open_items_citations(text: &str) -> Vec<u32> {
    const NEEDLE: &str = "OPEN_ITEMS.md";
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(pos) = rest.find(NEEDLE) {
        let after = &rest[pos + NEEDLE.len()..];
        // Expect (with the established phrasing) something like "` item 28"
        // immediately after the filename — skip the closing backtick and any
        // whitespace, then require the literal word "item".
        let after_tick = after.trim_start_matches('`');
        let after_tick = after_tick.trim_start();
        if let Some(stripped) = after_tick.strip_prefix("item ") {
            let digits: String = stripped
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = digits.parse::<u32>() {
                out.push(n);
            }
        }
        rest = after;
    }
    out
}

/// Does `docs/perf/OPEN_ITEMS.md` contain a numbered active-list item `N`
/// (i.e. a line starting with `N. **`, the exact format every active item in
/// the `## Open items` section uses) that also carries a `Next trigger`
/// bullet? Returns `(item_exists, has_next_trigger)`.
fn open_items_has_numbered_entry_with_trigger(open_items_text: &str, item_no: u32) -> (bool, bool) {
    let marker = format!("{item_no}. **");
    let lines: Vec<&str> = open_items_text.lines().collect();
    let Some(start) = lines.iter().position(|l| l.starts_with(&marker)) else {
        return (false, false);
    };
    // Scan forward from the item's title line until the NEXT numbered item
    // (a line matching `^\d+\. \*\*`) or a `##`/`###` section heading, and
    // check for a "Next trigger:" bullet inside that span.
    let mut has_trigger = false;
    for line in &lines[start + 1..] {
        let t = line.trim_start();
        if t.starts_with("###") || t.starts_with("## ") {
            break;
        }
        // A new numbered item: digits followed by ". **".
        let digits_prefix: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits_prefix.is_empty() && t[digits_prefix.len()..].starts_with(". **") {
            break;
        }
        if t.contains("Next trigger:") {
            has_trigger = true;
        }
    }
    (true, has_trigger)
}

/// Tripwire against the R18-8/R22-3 zero-owner failure mode and its
/// duplicate-owner sibling, applied to `docs/FEATURE_PROMOTION_STATUS.md`'s
/// promotion-verdict survey table (added R30-14, task #463).
///
/// **Why `FEATURE_PROMOTION_STATUS.md`, not `OPEN_ITEMS.md`, is the source of
/// CONDITIONAL-GO/NEVER-DECIDED truth for this check:** both files use the
/// string "CONDITIONAL-GO" in prose, but `OPEN_ITEMS.md`'s occurrences
/// outside its own feature-promotion items are almost all about DEFERRED
/// DESIGN PROPOSALS (e.g. items 2/5/14 in the `[D]` tier — a design doc's own
/// verdict on an unimplemented optimization, not a shipped Cargo feature
/// awaiting a promotion decision). `FEATURE_PROMOTION_STATUS.md` was built by
/// R29-12 (task #443) specifically as a survey of shipped, non-`production`
/// Cargo features and their promotion status — its `## Survey table` is a
/// literal one-row-per-feature enumeration with a dedicated `promotion
/// verdict` column, making it the authoritative source for "which shipped
/// FEATURES are undecided," as opposed to "which design PROPOSALS carry a
/// conditional recommendation."
///
/// **What this test asserts, per flagged row:**
/// 1. **NO ZERO owners** — the row's `evidence citation` column names at
///    least one `OPEN_ITEMS.md item N`. This is the R18-8/R22-3 failure mode
///    CLAUDE.md's own "Round start" bullet names by incident: R18-8/task #336
///    found R14-4's explicitly-flagged-open item hung unnoticed through
///    rounds 15–17 because no index owned it; R22-3/task #354 found R19-1's
///    commit-message follow-ups existed in NEITHER index for the same reason.
///    At the time this test was written, three features
///    (`exact-span-large`/`large-reserved-capacity`/`large-cache-extended`)
///    were in exactly this zero-owner state — `FEATURE_PROMOTION_STATUS.md`
///    itself said so in its own "Other features found in a SIMILAR (less
///    sharp) shape" section ("R29-12 does NOT create index entries for these
///    three"). R30-14 closed that gap by adding `OPEN_ITEMS.md` items 28/29/30
///    (one per feature) so this test is non-vacuously satisfiable today.
/// 2. **The cited item number actually exists** in `OPEN_ITEMS.md`'s numbered
///    active list (not a dangling reference to a renumbered/removed item).
/// 3. **The cited item carries a stated `Next trigger:` bullet** — an owner
///    entry with no stated trigger is not a real owner, just a pointer.
///
/// **What this test asserts, across ALL flagged rows together:**
/// 4. **NO DUPLICATE owners** — no two features with their OWN, independent
///    CONDITIONAL-GO/NEVER-DECIDED verdict cite the same `OPEN_ITEMS.md` item
///    number. This project's CLAUDE.md cites R13-9 for the general PATTERN
///    this guards against (hand-written CI feature-isolation rows silently
///    becoming duplicates of each other once the underlying feature list
///    changed) as a "same underlying problem in miniature" to the
///    `cargo-hack` motivation, not as a citable instance of two
///    `OPEN_ITEMS.md` entries colliding on one item number specifically —
///    being honest about that distinction (per this test's own doc comment
///    obligation): R13-9 is the closest ON-RECORD precedent for the general
///    "rows silently drift into duplicates" class this rule targets, not a
///    literal prior instance of THIS exact check's failure mode, which is
///    reasoned from that general pattern rather than a second citable
///    incident. The one legitimate multi-row share in the current table
///    (`alloc-lazy-commit`, a pure alias whose own verdict text is "reduces
///    to `small-segment-lazy-commit`", not CONDITIONAL-GO/NEVER-DECIDED
///    itself) is excluded by construction — [`parse_survey_rows`] only
///    collects rows whose OWN verdict cell carries one of the two markers, so
///    the alias row never enters this scan and its shared item-26 citation
///    with `small-segment-lazy-commit` is not flagged as a duplicate.
///
/// Doc-only guard: reads doc text, never links the crate, so it runs in every
/// feature configuration.
#[test]
fn every_undecided_feature_has_exactly_one_owner_with_a_next_trigger() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let status_path = manifest.join("docs").join("FEATURE_PROMOTION_STATUS.md");
    let status_text =
        fs::read_to_string(&status_path).expect("read docs/FEATURE_PROMOTION_STATUS.md");
    let open_items_path = manifest.join("docs").join("perf").join("OPEN_ITEMS.md");
    let open_items_text =
        fs::read_to_string(&open_items_path).expect("read docs/perf/OPEN_ITEMS.md");

    let rows = parse_survey_rows(&status_text);
    assert!(
        !rows.is_empty(),
        "parse_survey_rows found no CONDITIONAL-GO/NEVER-DECIDED rows in \
         docs/FEATURE_PROMOTION_STATUS.md's survey table — either the table's \
         shape changed (this test's parser expects a 5-column pipe table) or \
         every previously-undecided feature has since been DECIDED (update \
         this test's own non-vacuity assumption if so, rather than leaving a \
         silently-passing empty-set check)."
    );

    let mut errors: Vec<String> = Vec::new();

    // Per-row checks: exactly one citation, item exists, item has a trigger.
    for row in &rows {
        match row.owner_items.len() {
            0 => errors.push(format!(
                "FEATURE_PROMOTION_STATUS.md:{}: {} is CONDITIONAL-GO/NEVER-DECIDED \
                 but its evidence citation names NO `OPEN_ITEMS.md item N` — this is \
                 the R18-8/R22-3 zero-owner failure mode (see CLAUDE.md's \"Round \
                 start\" bullet). Add a dedicated OPEN_ITEMS.md owner item.",
                row.line, row.feature
            )),
            1 => {
                let item_no = row.owner_items[0];
                let (exists, has_trigger) =
                    open_items_has_numbered_entry_with_trigger(&open_items_text, item_no);
                if !exists {
                    errors.push(format!(
                        "FEATURE_PROMOTION_STATUS.md:{}: {} cites OPEN_ITEMS.md item \
                         {item_no}, but no numbered item {item_no} exists in \
                         docs/perf/OPEN_ITEMS.md — dangling owner citation.",
                        row.line, row.feature
                    ));
                } else if !has_trigger {
                    errors.push(format!(
                        "FEATURE_PROMOTION_STATUS.md:{}: {} cites OPEN_ITEMS.md item \
                         {item_no}, which exists but has no stated \"Next trigger:\" \
                         bullet — an owner entry needs a stated trigger, not just a \
                         pointer.",
                        row.line, row.feature
                    ));
                }
            }
            n => errors.push(format!(
                "FEATURE_PROMOTION_STATUS.md:{}: {} cites {n} distinct OPEN_ITEMS.md \
                 items ({:?}) in one row's evidence citation — a feature should have \
                 exactly ONE owner item, not several competing ones.",
                row.line, row.feature, row.owner_items
            )),
        }
    }

    // Cross-row check: no two DIFFERENT flagged features share the same
    // owner item number (the R13-9-pattern duplicate-owner failure mode).
    let mut seen: std::collections::HashMap<u32, Vec<&str>> = std::collections::HashMap::new();
    for row in &rows {
        if row.owner_items.len() == 1 {
            seen.entry(row.owner_items[0])
                .or_default()
                .push(&row.feature);
        }
    }
    for (item_no, features) in &seen {
        if features.len() > 1 {
            errors.push(format!(
                "OPEN_ITEMS.md item {item_no} is cited as the owner by {} DIFFERENT \
                 features with their own independent CONDITIONAL-GO/NEVER-DECIDED \
                 verdicts ({:?}) — each undecided feature needs its OWN owner item, \
                 not a shared one (the R13-9-pattern duplicate-owner failure mode).",
                features.len(),
                features
            ));
        }
    }

    assert!(
        errors.is_empty(),
        "every CONDITIONAL-GO/NEVER-DECIDED feature in \
         docs/FEATURE_PROMOTION_STATUS.md's survey table must have EXACTLY ONE \
         active docs/perf/OPEN_ITEMS.md owner item with a stated next trigger \
         (R30-14/task #463):\n{}",
        errors.join("\n"),
    );
}
