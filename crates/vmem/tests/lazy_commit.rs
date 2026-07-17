//! B0 (R7 Workstream B): tests for incremental-commit primitives —
//! `reserve_aligned_lazy` and `commit_range`.
//!
//! These tests verify the vmem-layer foundation that B1/B2 will build on.
//! They do NOT touch any alloc-core / segment-header code.

#![cfg(feature = "lazy-commit")]

use aligned_vmem::{commit_range, reserve_aligned, reserve_aligned_lazy, PAGE};

const MIB: usize = 1024 * 1024;

// ── reserve_aligned_lazy: basic contract ────────────────────────────────────

#[test]
fn lazy_reserve_basic_write_initial_region() {
    // Reserve 4 MiB, commit only the first 64 KiB.
    let initial = 16 * PAGE; // 64 KiB
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, initial).expect("lazy reserve 4 MiB");
    let base = r.as_ptr();

    assert!(!base.is_null());
    assert_eq!(base as usize % span, 0, "base must be span-aligned");
    assert_eq!(r.len(), span);

    // Write/read within the committed initial region — must not fault.
    // SAFETY: base is valid for at least `initial` committed bytes.
    unsafe {
        for off in (0..initial).step_by(PAGE) {
            base.add(off).write(0xAB);
            assert_eq!(base.add(off).read(), 0xAB);
        }
    }
    // Drop releases the entire reservation (including uncommitted tail).
}

#[test]
fn lazy_reserve_then_commit_range_grows_accessible() {
    // Reserve 4 MiB, commit first 64 KiB, then commit the next 64 KiB via
    // commit_range, then write into it.
    let chunk = 16 * PAGE; // 64 KiB
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("lazy reserve");
    let base = r.as_ptr();

    // Write into initial committed region.
    // SAFETY: first `chunk` bytes are committed.
    unsafe {
        base.write(0x11);
        assert_eq!(base.read(), 0x11);
    }

    // Commit the next chunk.
    // SAFETY: base is the as_ptr of a live reservation; [chunk, 2*chunk) is
    // within the span and currently reserved-but-uncommitted (or already
    // committed on Unix/miri).
    let ok = unsafe { commit_range(base, chunk, 2 * chunk) };
    assert!(ok, "commit_range must succeed on a live reservation");

    // Write into the newly committed region.
    // SAFETY: [chunk, 2*chunk) is now committed.
    unsafe {
        base.add(chunk).write(0x22);
        assert_eq!(base.add(chunk).read(), 0x22);
    }
    // Drop releases everything.
}

#[test]
fn lazy_reserve_commit_entire_remainder() {
    // Reserve 2 MiB, commit first 64 KiB, then commit the entire remainder
    // in one commit_range call. Proves that commit_range handles large ranges.
    let initial = 16 * PAGE; // 64 KiB
    let span = 2 * MIB;
    let r = reserve_aligned_lazy(span, span, initial).expect("lazy reserve 2 MiB");
    let base = r.as_ptr();

    // Commit the rest: [initial, span).
    // SAFETY: base is a live reservation, [initial, span) is within the span.
    let ok = unsafe { commit_range(base, initial, span) };
    assert!(ok, "commit_range for the full remainder must succeed");

    // Write at the very end of the now-fully-committed span.
    // SAFETY: entire span is committed.
    unsafe {
        let last_page = span - PAGE;
        base.add(last_page).write(0x33);
        assert_eq!(base.add(last_page).read(), 0x33);
    }
}

// ── commit_range: contract validation ───────────────────────────────────────

#[test]
fn commit_range_noop_on_bad_offsets() {
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();

    // SAFETY: base is a live reservation.
    unsafe {
        // start >= end → no-op, returns true.
        assert!(commit_range(base, PAGE, PAGE), "start==end is no-op");
        assert!(commit_range(base, 2 * PAGE, PAGE), "start>end is no-op");
        // Misaligned offsets → no-op, returns true.
        assert!(commit_range(base, 1, PAGE), "misaligned start is no-op");
        assert!(commit_range(base, 0, PAGE + 1), "misaligned end is no-op");
    }
}

#[test]
fn commit_range_idempotent_on_already_committed() {
    // Committing a range that is already committed (from the eager path)
    // must succeed without error — MEM_COMMIT is idempotent on Windows.
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();

    // SAFETY: the entire span is committed (eager reservation).
    unsafe {
        let ok = commit_range(base, 0, PAGE);
        assert!(ok, "recommitting an already-committed page must succeed");
    }
}

// ── reserve_aligned_lazy: contract rejection ────────────────────────────────

#[test]
fn lazy_reserve_rejects_bad_contracts() {
    // Zero initial_commit.
    assert!(
        reserve_aligned_lazy(4 * MIB, 4 * MIB, 0).is_none(),
        "zero initial_commit rejected"
    );
    // initial_commit > size.
    assert!(
        reserve_aligned_lazy(PAGE, PAGE, 2 * PAGE).is_none(),
        "initial_commit > size rejected"
    );
    // Non-page-multiple initial_commit.
    assert!(
        reserve_aligned_lazy(4 * MIB, 4 * MIB, PAGE + 1).is_none(),
        "non-page-multiple initial_commit rejected"
    );
    // Zero size (inherited from reserve_aligned contract).
    assert!(
        reserve_aligned_lazy(0, PAGE, PAGE).is_none(),
        "zero size rejected"
    );
    // Non-pow2 align.
    assert!(
        reserve_aligned_lazy(PAGE, 3, PAGE).is_none(),
        "non-pow2 align rejected"
    );
}

// ── release after partial commit ────────────────────────────────────────────

#[test]
fn release_via_into_parts_after_partial_commit() {
    // Verify that into_parts + release works correctly even when the
    // reservation is only partially committed.
    let initial = 16 * PAGE; // 64 KiB
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, initial).expect("lazy reserve");
    let base = r.as_ptr();

    // Write into the committed region.
    // SAFETY: first `initial` bytes are committed.
    unsafe {
        base.write(0xCC);
    }

    // Take ownership manually and release.
    let (raw, raw_len, raw_align) = r.into_parts();
    assert!(!raw.is_null());
    // SAFETY: triple from into_parts, released exactly once.
    unsafe { aligned_vmem::release(raw, raw_len, raw_align) };
}

// ── eager fallback equivalence ──────────────────────────────────────────────

#[test]
fn lazy_reserve_full_commit_equals_eager() {
    // When initial_commit == size, lazy-reserve is functionally identical to
    // the eager path: the entire span is committed.
    let span = 2 * MIB;
    let r_lazy =
        reserve_aligned_lazy(span, span, span).expect("lazy reserve with full initial commit");
    let r_eager = reserve_aligned(span, span).expect("eager reserve");

    // Both must produce valid, writable spans of the same length.
    assert_eq!(r_lazy.len(), span);
    assert_eq!(r_eager.len(), span);
    assert_eq!(r_lazy.as_ptr() as usize % span, 0);
    assert_eq!(r_eager.as_ptr() as usize % span, 0);

    // Write to the last page of each — both must succeed.
    // SAFETY: both spans are fully committed and valid for `span` bytes.
    unsafe {
        let off = span - PAGE;
        r_lazy.as_ptr().add(off).write(0xDD);
        r_eager.as_ptr().add(off).write(0xEE);
        assert_eq!(r_lazy.as_ptr().add(off).read(), 0xDD);
        assert_eq!(r_eager.as_ptr().add(off).read(), 0xEE);
    }
}

// ── multiple sequential commit_range calls ──────────────────────────────────

#[test]
fn sequential_commit_range_grows_incrementally() {
    // Simulate the B1/B2 pattern: start with a small committed region and
    // grow it in steps via commit_range.
    let chunk = 16 * PAGE; // 64 KiB per step
    let span = 2 * MIB;
    let r = reserve_aligned_lazy(span, span, chunk).expect("lazy reserve");
    let base = r.as_ptr();

    let mut frontier = chunk;
    // Grow in 5 steps (total: 6 chunks = 384 KiB committed).
    for step in 0..5 {
        let new_frontier = frontier + chunk;
        if new_frontier > span {
            break;
        }
        // SAFETY: base is a live reservation; [frontier, new_frontier) is within span.
        let ok = unsafe { commit_range(base, frontier, new_frontier) };
        assert!(
            ok,
            "commit_range step {} must succeed (frontier {} -> {})",
            step, frontier, new_frontier
        );
        // Write at the start of the newly committed chunk.
        // SAFETY: [frontier, new_frontier) is now committed.
        unsafe {
            base.add(frontier).write((step as u8) + 1);
            assert_eq!(base.add(frontier).read(), (step as u8) + 1);
        }
        frontier = new_frontier;
    }
    // Verify all written bytes are still accessible and correct.
    // SAFETY: all chunks from [0, frontier) are committed and were written.
    unsafe {
        assert_eq!(base.read(), 0, "initial region byte not overwritten");
        for step in 0..5u8 {
            let off = chunk + (step as usize) * chunk;
            if off >= frontier {
                break;
            }
            assert_eq!(
                base.add(off).read(),
                step + 1,
                "step {} value mismatch",
                step
            );
        }
    }
}
