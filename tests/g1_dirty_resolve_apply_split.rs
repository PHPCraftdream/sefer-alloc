//! Drift-detector for the G1 (P1) use-after-free fix: the resolve/apply
//! split of the cross-thread dirty-bit pipeline.
//!
//! Review finding G1 (`docs/reviews/2026-09-10-074442-sefer-alloc-global-review-sol-codex-run-1.md`,
//! §G1, P1): the old `set_dirty_bit_for_segment(base, packed)` ran AFTER the
//! successful ring publish and re-read the segment header
//! (`segment_id_at` / `owner_state_atomic`) — but by then the owner may have
//! drained the just-published record, dropped `live_count` to zero, and
//! RELEASED the segment (unmap/decommit), so those reads were a genuine
//! allocator-side use-after-free. The fix splits the helper into
//! `resolve_dirty_bit_target` (called BEFORE the publish, when the block is
//! still counted in the owner's `live_count` and the segment cannot be
//! released) and `apply_resolved_dirty_bit` (called ONLY after the publish,
//! touching only a process-lifetime `&'static HeapSlot` plus bitmap
//! arithmetic — never segment memory).
//!
//! This test mechanically pins that contract against source drift:
//!
//!   1. `apply_resolved_dirty_bit` takes exactly ONE parameter, the resolved
//!      target — NO `base` pointer. Re-adding a post-publish segment read
//!      requires changing this signature, which fails this test and forces a
//!      conscious review.
//!   2. `ResolvedDirtyTarget` holds a `&'static` slot reference and no raw
//!      segment pointer (`*mut`), so the snapshot survives a post-publish
//!      segment release by construction.
//!   3. Inside `push_with_overflow_retry`, the SINGLE `resolve_dirty_bit_target(`
//!      call is strictly BEFORE BOTH publish sites (`ring.push(packed)` and
//!      `ring.try_push_uncounted(packed)`) — resolve-before-publish ordering.
//!      (fxx round-2 review §4.2: the retry path used to call
//!      `resolve_dirty_bit_target` a second time with identical `base`/`packed`
//!      inputs; since `ResolvedDirtyTarget` is `Copy` and G1 already guarantees
//!      nothing is published between the fast-path resolve and the retry
//!      branch — reaching the retry branch requires both the fast-path push
//!      AND the immediate `push_to_heap_overflow` attempt to have failed —
//!      the retry path now reuses the fast path's snapshot instead of
//!      re-resolving.)
//!   4. The old function name `set_dirty_bit_for_segment` no longer appears
//!      anywhere in the file (the old post-publish helper is fully removed).
//!
//! Structural guards run in every configuration. Runtime lifetime coverage
//! lives in `g1_delayed_notification.rs`.

use std::fs;
use std::path::Path;

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn xthread_source() -> String {
    fs::read_to_string(
        manifest_dir()
            .join("src")
            .join("registry")
            .join("heap_core_xthread")
            .join("overflow.rs"),
    )
    .expect("read src/registry/heap_core_xthread/overflow.rs")
    .replace("\r\n", "\n")
}

#[test]
fn apply_fn_takes_no_segment_pointer() {
    let src = xthread_source();
    assert!(
        src.contains("fn apply_resolved_dirty_bit(target: ResolvedDirtyTarget)"),
        "G1 contract drifted: `apply_resolved_dirty_bit` must take exactly \
         one parameter, the resolved `ResolvedDirtyTarget` snapshot — no \
         `base` segment pointer. A post-publish signature change means \
         segment memory is (or may soon be) touched after the ring publish \
         again; re-review the G1 fix in \
         docs/reviews/2026-09-10-074442-sefer-alloc-global-review-sol-codex-run-1.md"
    );
}

#[test]
fn resolved_target_holds_no_raw_segment_pointer() {
    let src = xthread_source();
    let start = src
        .find("struct ResolvedDirtyTarget {")
        .expect("ResolvedDirtyTarget struct must exist");
    let end = src[start..]
        .find('}')
        .expect("closing brace of ResolvedDirtyTarget");
    let block = &src[start..start + end];

    assert!(
        block.contains("slot: &'static"),
        "G1: `ResolvedDirtyTarget.slot` must be a process-lifetime \
         `&'static HeapSlot` so the snapshot stays valid across the publish"
    );
    let fields: Vec<_> = block
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("#["))
        .collect();
    assert_eq!(
        fields,
        [
            "slot: &'static crate::registry::heap_slot::HeapSlot,",
            "word: usize,",
            "bit: u64,",
            "packed: u32,"
        ],
        "G1: the snapshot must not gain a segment pointer or another borrowed field"
    );
}

#[test]
fn resolve_calls_precede_both_publish_sites_in_push_with_overflow_retry() {
    let src = xthread_source();
    let lines: Vec<&str> = src.lines().collect();

    let fn_start = lines
        .iter()
        .position(|l| l.contains("fn push_with_overflow_retry("))
        .expect("`fn push_with_overflow_retry(` must exist");
    // The function body ends at the next item's doc comment line.
    let fn_end = lines[fn_start + 1..]
        .iter()
        .position(|l| l.starts_with("    /// "))
        .map_or(lines.len(), |off| fn_start + 1 + off);
    let body = &lines[fn_start..fn_end];

    let first_fast_push = body
        .iter()
        .position(|l| l.contains("ring.push(packed)"))
        .expect("fast-path `ring.push(packed)` publish site");
    let first_retry_push = body
        .iter()
        .position(|l| l.contains("ring.try_push_uncounted(packed)"))
        .expect("retry-path `ring.try_push_uncounted(packed)` publish site");

    let resolve_lines: Vec<usize> = body
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("resolve_dirty_bit_target("))
        .map(|(i, _)| i)
        .collect();
    // fxx round-2 review §4.2: the retry path used to redundantly re-resolve
    // with identical inputs; it now reuses the fast path's `Copy` snapshot
    // (G1 guarantees nothing is published between the two sites), so exactly
    // ONE `resolve_dirty_bit_target(` call remains, and it must precede BOTH
    // publish sites.
    assert!(
        resolve_lines.len() == 1,
        "G1/§4.2: `push_with_overflow_retry` must contain exactly one \
         `resolve_dirty_bit_target` call, reused via the `Copy` \
         `ResolvedDirtyTarget` for both the fast and retry publish sites; \
         found {}",
        resolve_lines.len()
    );

    assert!(
        resolve_lines[0] < first_fast_push,
        "G1: `resolve_dirty_bit_target` (body line {}) must run BEFORE the \
             fast-path publish (`ring.push(packed)` at body line {}) — after \
             the publish the segment may already be released, so no segment \
             memory may be read",
        resolve_lines[0],
        first_fast_push
    );
    assert!(
        resolve_lines[0] < first_retry_push,
        "G1: `resolve_dirty_bit_target` (body line {}) must also run BEFORE \
             the retry-path publish (`ring.try_push_uncounted(packed)` at \
             body line {}) — it is the SAME snapshot reused, per §4.2",
        resolve_lines[0],
        first_retry_push
    );
}

#[test]
fn old_set_dirty_bit_for_segment_is_gone() {
    let src = xthread_source();
    assert!(
        !src.contains("set_dirty_bit_for_segment"),
        "G1: the old post-publish helper `set_dirty_bit_for_segment` read \
         segment memory AFTER the ring publish (use-after-free); it was \
         replaced by the resolve/apply split. Its reappearance in \
         src/registry/heap_core_xthread/overflow.rs is a regression — re-read the G1 \
         section of \
         docs/reviews/2026-09-10-074442-sefer-alloc-global-review-sol-codex-run-1.md"
    );
}
