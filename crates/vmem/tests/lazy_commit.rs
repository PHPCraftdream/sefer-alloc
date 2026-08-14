//! B0 (R7 Workstream B): tests for incremental-commit primitives —
//! `reserve_aligned_lazy` and `commit_range`.
//!
//! These tests verify the vmem-layer foundation that B1/B2 will build on.
//! They do NOT touch any alloc-core / segment-header code.

#![cfg(feature = "lazy-commit")]

use aligned_vmem::{commit_range, reserve_aligned, reserve_aligned_lazy, try_commit_range, PAGE};

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
fn lazy_reserve_small_align_still_reserves_full_span() {
    // Regression test for task #848 (V21/P18): the Windows single-call
    // VirtualAlloc(MEM_RESERVE | MEM_COMMIT) optimization for align <= 64 KiB
    // must NOT apply when initial_commit < size -- a single combined call
    // can only reserve and commit the SAME byte range, so taking that path
    // here would silently shrink the actual reservation down to
    // `initial_commit` bytes, breaking every later `commit_range` call past
    // that point. Concretely reproduced during zero-trust review of #848's
    // delegated diff: align=4 KiB (well under the 64 KiB threshold),
    // size=64 KiB, initial_commit=4 KiB -- the buggy version returned a
    // 4 KiB reservation instead of a >=64 KiB one, and `commit_range` past
    // the first page failed.
    let align = PAGE; // 4 KiB -- well under the 64 KiB single-call threshold
    let size = 16 * PAGE; // 64 KiB
    let initial = PAGE; // commit only the first page now
    let r = reserve_aligned_lazy(size, align, initial).expect("lazy reserve, small align");
    let base = r.as_ptr();
    assert_eq!(r.len(), size, "len() echoes the requested size");
    assert!(
        r.reservation_len() >= size,
        "the OS reservation must cover the full requested span (got {})",
        r.reservation_len()
    );

    // The initially committed page is writable.
    // SAFETY: the first `initial` bytes are committed.
    unsafe {
        base.write(0x33);
        assert_eq!(base.read(), 0x33);
    }

    // commit_range past `initial` must succeed -- this is the exact call
    // the single-call fast path broke when it shrank the reservation.
    // SAFETY: `[initial, size)` is within the live reservation's span.
    let ok = unsafe { commit_range(base, initial, size) };
    assert!(
        ok,
        "commit_range beyond initial_commit must succeed even when align <= 64 KiB"
    );

    // SAFETY: [initial, size) is now committed.
    unsafe {
        base.add(size - PAGE).write(0x44);
        assert_eq!(base.add(size - PAGE).read(), 0x44);
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
fn commit_range_empty_range_is_a_noop() {
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();

    // SAFETY: base is a live reservation.
    unsafe {
        // A genuinely empty range (start == end) is the ONLY contract-legal
        // no-op — it returns true. See
        // `commit_range_rejects_contract_violating_offsets` below for the
        // (task #712-corrected) behavior on an actual contract violation.
        assert!(
            commit_range(base, PAGE, PAGE),
            "start==end is a success no-op"
        );
    }
}

#[test]
fn commit_range_rejects_contract_violating_offsets() {
    // task #712 (rust-intel audit MEDIUM, already crashed an in-repo
    // consumer): `commit_range`/`try_commit_range` used to clamp a contract
    // VIOLATION (misaligned offsets, or `start > end`) to the same
    // WRITE-PERMITTING sentinel a genuine success reports (`true` /
    // `Ok(())`). Renamed from this test's former name
    // (`commit_range_noop_on_bad_offsets`), which asserted exactly that
    // buggy behavior — a misaligned/inverted range is not a "no-op", it is a
    // rejected contract violation the caller MUST NOT treat as "safe to
    // write".
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();

    // SAFETY: base is a live reservation; none of the calls below reach the
    // real commit syscall (all are rejected before it).
    unsafe {
        assert!(
            !commit_range(base, 2 * PAGE, PAGE),
            "start > end (inverted range) must be rejected, not silently permitted"
        );
        assert!(
            !commit_range(base, 1, PAGE),
            "misaligned start must be rejected, not silently permitted"
        );
        assert!(
            !commit_range(base, 0, PAGE + 1),
            "misaligned end must be rejected, not silently permitted"
        );
        assert!(
            try_commit_range(base, 1, PAGE)
                .unwrap_err()
                .is_invalid_argument(),
            "the fallible form must carry VmemError::invalid_argument(), not an OS code"
        );
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
        // task #716: this test never writes offset 0 -- on a real OS backend
        // that byte is a fresh, zero-filled page (guaranteed by the OS), but
        // under miri's `std::alloc`-based fallback (documented as NOT
        // zeroing, unlike a real OS) reading it is a genuine uninitialized-
        // memory read. Mirrors the identical, already-established gate in
        // tests/smoke.rs's `decommit_recommit_roundtrip`
        // for the exact same real-OS-zero-fill-vs-miri distinction
        // (corrected round 8, task #900/U2: a prior version of this comment
        // misnamed the precedent as
        // `recommit_is_fallible_and_reports_success_on_the_happy_path`,
        // which has no `#[cfg]` gate and no zero-fill read at all).
        #[cfg(not(miri))]
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

/// Round 2 pre-release review, task #949 (T-1): Windows `reserve_aligned_lazy` actually saves
/// commit charge. Every existing test in this file would pass verbatim even if
/// `reserve_aligned_lazy_raw` simply forwarded to the eager path internally
/// (which is literally what the Unix/miri/mock backends already do). This test
/// uses the `bench-internals` oracles to verify that the Windows two-call path
/// is actually taken when `commit_len != size`, pinning the `commit_len == size`
/// guard in `win_reserve_commit` that a prior bug already broke once.
#[test]
#[cfg(all(windows, feature = "bench-internals"))]
fn windows_lazy_reserve_saves_commit_charge() {
    // Use a small initial commit to force the two-call path (commit_len != size).
    let span = 4 * MIB;
    let initial = PAGE; // 4 KiB initial commit, far less than span

    aligned_vmem::reset_bench_internals_counters();
    let before_two_call = aligned_vmem::windows_reserve_commit_two_call_pairs();
    let before_single_call = aligned_vmem::windows_reserve_commit_single_calls();

    let r = reserve_aligned_lazy(span, span, initial).expect("lazy reserve");

    let after_two_call = aligned_vmem::windows_reserve_commit_two_call_pairs();
    let after_single_call = aligned_vmem::windows_reserve_commit_single_calls();

    // The two-call-pairs counter must have incremented (we took the lazy path).
    assert_eq!(
        after_two_call,
        before_two_call + 1,
        "reserve_aligned_lazy with small initial_commit must take the two-call path"
    );

    // The single-call counter must NOT have incremented.
    assert_eq!(
        after_single_call, before_single_call,
        "reserve_aligned_lazy with small initial_commit must NOT take the single-call path"
    );

    // Verify the reservation actually works (write/read the committed region).
    let base = r.as_ptr();
    // SAFETY: first `initial` bytes are committed.
    unsafe {
        base.write(0xAA);
        assert_eq!(base.read(), 0xAA);
    }
}
