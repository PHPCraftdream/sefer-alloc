//! Drift-detector: pins the count of write sites to `RemoteFreeRing`'s `head`
//! cursor against the module doc's F10 monotonicity proof.
//!
//! R33-4 (task #509): the module doc's formally-stated soundness argument
//! (`src/alloc_core/remote_free_ring.rs` ~line 101) claimed "the only OTHER
//! write site" was `dbg_set_cursors` when there were actually FOUR
//! (`drain`, `init_in_place`, `dbg_set_cursors`, `dbg_advance_head_only`).
//! The round-32 readonly review (§3, finding F3 [P2]) caught it. This test
//! mechanically re-derives the write-site count from the source text and
//! fails if a new site is added or an existing one removed without updating
//! both this test's expected count and the module doc's enumeration — the
//! same structural-drift-detection pattern `tests/ci_clippy_matrix_consistency.rs`
//! uses for a different file pair.
//!
//! Doc/config-only guard: reads source text, never links the crate, so it
//! runs in every feature configuration.

use std::fs;
use std::path::Path;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The write sites to `head` this test pins. If the code gains or loses a
/// write site, update BOTH this constant AND the module doc's F10 enumeration
/// (`src/alloc_core/remote_free_ring.rs` ~line 101).
const EXPECTED_HEAD_WRITE_SITE_COUNT: usize = 4;

#[test]
fn head_write_site_count_matches_doc() {
    let src = fs::read_to_string(
        manifest_dir()
            .join("src")
            .join("alloc_core")
            .join("remote_free_ring.rs"),
    )
    .expect("read src/alloc_core/remote_free_ring.rs")
    .replace("\r\n", "\n");

    // Three write patterns exist:
    // (a) atomic store via method: `self.head().store(...)`  — does NOT match
    //     `self.cached_head().store(...)` (`self.head()` is not a substring
    //     of `self.cached_head()`, so there is no false positive from the
    //     cached-head stores at :840/:965).
    // (b) atomic store via FIELD:   `self.head.store(...)` — the R34-17/task
    //     #536 `DrainHeadPublish` guard holds `head: &'static AtomicU32` (a
    //     field, not a method) and publishes via `self.head.store(...)`.
    //     `self.head.store` is NOT a substring of `self.head().store` (the
    //     latter has `head()` with parens before the dot), so the two patterns
    //     are disjoint and cannot double-count a single site.
    // (c) raw write:     a line containing `write_u32` AND `, HEAD_OFF)` —
    //     does NOT match the `CACHED_HEAD_OFF` variant (`, CACHED_HEAD_OFF)`
    //     is a distinct substring from `, HEAD_OFF)`) and does NOT match read
    //     accessors (which use `atomic_u32_at`, not `write_u32`).
    let write_sites: Vec<(usize, String)> = src
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            line.contains("self.head().store(")
                || line.contains("self.head.store(")
                || (line.contains("write_u32") && line.contains(", HEAD_OFF)"))
        })
        .map(|(i, line)| (i + 1, line.trim().to_string()))
        .collect();

    assert_eq!(
        write_sites.len(),
        EXPECTED_HEAD_WRITE_SITE_COUNT,
        "src/alloc_core/remote_free_ring.rs has {} write site(s) to `head`, \
         expected {} (R33-4/task #509). The module doc's F10 monotonicity \
         proof (~line 101) enumerates every write site to `head`; if you \
         added or removed one, update BOTH the module doc's enumeration AND \
         this test's EXPECTED_HEAD_WRITE_SITE_COUNT. Write sites found:\n{}",
        write_sites.len(),
        EXPECTED_HEAD_WRITE_SITE_COUNT,
        write_sites
            .iter()
            .map(|(n, l)| format!("  :{n} {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
