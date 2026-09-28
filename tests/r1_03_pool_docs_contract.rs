//! Docs-contract tripwire for R1-03 (src review round 1): the
//! empty-small-segment pool is a SAME-CLASS free-list reserve, not a carve
//! reserve — `reserve_small_segment` always performs a genuine fresh OS
//! reservation and never pops a pooled segment as a carve target. Before this
//! task, README.md and `SmallSegmentPoolConfig`'s module doc both claimed the
//! opposite ("the next allocation that would otherwise reserve a fresh
//! segment pops a pooled one" / "every retained slot is reusable (popped on
//! the next reserve)"), contradicting the pool implementation's own comment
//! (`alloc_core_small_pool_impl.rs`: "the pool is a free-list reserve, not a
//! carve reserve").
//!
//! This file pins the corrected wording and fails if the carve-reuse promise
//! reappears. Verified red-before/green-after by reverting the two doc files
//! to their pre-fix text and confirming both assertion groups fail.

const README: &str = include_str!("../README.md");
const POOL_CONFIG: &str = include_str!("../src/alloc_core/config/small_segment_pool_config.rs");

/// Windows CI checks these files out with CRLF line endings; the needles use
/// `\n`, so compare against an LF-normalized copy.
fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn readme_states_pool_is_free_list_reserve_not_carve_reserve() {
    let readme = lf(README);
    assert!(
        readme.contains("same-class free-list reserve, not a carve reserve"),
        "README must state the pool's actual reuse mechanism"
    );
    assert!(
        readme.contains(
            "a pooled segment is never popped as a\ncarve target for a class it has no free blocks for"
        ),
        "README must state reserve_small_segment never pops a pooled segment"
    );
    assert!(
        readme.contains(
            "does not accelerate a class-switching or\nproducer\u{2192}consumer handoff workload"
        ),
        "README must state which workload shape does NOT benefit from raising \
         the pool caps"
    );
}

#[test]
fn pool_config_doc_states_pool_is_free_list_reserve_not_carve_reserve() {
    let pool_config = lf(POOL_CONFIG);
    assert!(
        pool_config.contains("The pool is a same-class free-list reserve, not a carve reserve."),
        "SmallSegmentPoolConfig's module doc must state the pool's actual reuse \
         mechanism"
    );
    assert!(
        pool_config.contains(
            "it never pops a pooled segment\n//! as a carve target, for its own class or any other"
        ),
        "SmallSegmentPoolConfig's module doc must state reserve_small_segment \
         never pops a pooled segment as a carve target"
    );
    assert!(
        pool_config.contains(
            "its free-list blocks\n//! are popped by a later same-class `find_segment_with_free` hit — never by\n//! `reserve_small_segment`, which always reserves fresh"
        ),
        "SmallSegmentPoolConfig's 'Bounded — no permanent pin' section must \
         describe reuse as a same-class free-list pop, not a reserve-time pop"
    );
}

#[test]
fn stale_carve_reuse_promise_is_gone() {
    let readme = lf(README);
    let pool_config = lf(POOL_CONFIG);
    assert!(
        !readme.contains(
            "pops a pooled\none with no OS syscall, no metadata re-init, and no page fault"
        ),
        "README still makes the stale carve-reuse promise"
    );
    assert!(
        !pool_config.contains(
            "(`reserve_small_segment`) pops a pooled segment first: no OS syscall, no\n\
             //! metadata re-init, no page fault"
        ),
        "SmallSegmentPoolConfig still makes the stale carve-reuse promise"
    );
    assert!(
        !pool_config.contains("every retained slot is reusable (popped on the next\n//! reserve)"),
        "SmallSegmentPoolConfig still claims a pooled slot is popped on the next \
         reserve"
    );
}
