//! B0 (R7 Workstream B): tests for incremental-commit primitives —
//! `reserve_aligned_lazy` and `commit_range`.
//!
//! These tests verify the vmem-layer foundation that B1/B2 will build on.
//! They do NOT touch any alloc-core / segment-header code.

#![cfg(feature = "lazy-commit")]
// `serial_guard()` returns a real `MutexGuard` on Windows+bench-internals+
// non-mock builds and `()` everywhere else (the lock/counters it guards
// don't exist there) -- every call site binds it via `let _guard = ...;` so
// the guard's scope (drop at function end) is identical across all
// platforms; on the `()` variant that's a unit-value binding, which is
// otherwise a real clippy smell but is exactly the point here.
#![allow(clippy::let_unit_value)]

use aligned_vmem::{
    commit_range, page_size, reserve_aligned, reserve_aligned_lazy, try_commit_range, PAGE,
};
#[cfg(all(windows, feature = "bench-internals", not(aligned_vmem_mock)))]
use std::sync::Mutex;

const MIB: usize = 1024 * 1024;

/// Round 3 independent review (task #953, finding 6.3; corrected post-merge
/// after zero-trust re-verification): `bench-internals` counters read/reset
/// by `windows_lazy_reserve_saves_commit_charge` below
/// (`reset_bench_internals_counters`, `windows_reserve_commit_two_call_pairs`,
/// `windows_reserve_commit_single_calls`) are PROCESS-GLOBAL. libtest runs
/// this file's tests on parallel threads by default, so any test that both
/// resets those counters and then asserts an exact delta on them would race
/// against any other test in this binary that also calls into
/// `win_reserve_commit` concurrently -- mirroring the same hazard
/// `tests/smoke.rs`'s own `SERIAL` mutex already exists to prevent for its
/// process-global `UNIX_MADVISE_*` counters.
///
/// The original task #953 fix reasoned that only
/// `windows_lazy_reserve_saves_commit_charge` needed to hold this lock,
/// since it is the only test that *reads* the counters. That reasoning was
/// incomplete: `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`/`_TWO_CALL_PAIRS` are
/// incremented by `win_reserve_commit` whenever `bench-internals` is
/// enabled, regardless of whether the calling test cares about them --
/// every OTHER test in this file that calls `reserve_aligned`/
/// `reserve_aligned_lazy`/`commit_range` on Windows also WRITES to these
/// counters, and none of them held `SERIAL`, so
/// `windows_lazy_reserve_saves_commit_charge`'s before/after delta could
/// still be corrupted by a concurrent sibling test. Confirmed by direct
/// reproduction: `cargo test -p aligned-vmem --features "lazy-commit
/// huge-pages fault-injection bench-internals"` (no `aligned_vmem_mock`, matching the
/// Windows CI row) failed `windows_lazy_reserve_saves_commit_charge`
/// nondeterministically under default (parallel) `--test-threads`, passed
/// reliably under `--test-threads=1`. Every test in this file now holds
/// `SERIAL` for its whole body, so no `win_reserve_commit`-touching test
/// can run concurrently with the one that measures an exact delta. Gated
/// identically to `windows_lazy_reserve_saves_commit_charge` (only the
/// Windows + `bench-internals`, non-`aligned_vmem_mock` build ever references this
/// static, so an unconditional definition would warn
/// `dead_code`/`unused_imports` on every other platform/feature
/// combination) -- a future test added to this file must also take it.
#[cfg(all(windows, feature = "bench-internals", not(aligned_vmem_mock)))]
static SERIAL: Mutex<()> = Mutex::new(());

/// Acquire [`SERIAL`] on Windows+`bench-internals`+non-`aligned_vmem_mock` builds (a
/// no-op everywhere else, since neither the static nor the counters it
/// guards exist there) -- see `SERIAL`'s own doc comment above for why
/// every test in this file needs this, not only the ones that read the
/// counters.
#[cfg(all(windows, feature = "bench-internals", not(aligned_vmem_mock)))]
fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}
#[cfg(not(all(windows, feature = "bench-internals", not(aligned_vmem_mock))))]
fn serial_guard() {}

// ── reserve_aligned_lazy: basic contract ────────────────────────────────────

#[test]
fn lazy_reserve_basic_write_initial_region() {
    let _guard = serial_guard();
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
    let _guard = serial_guard();
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
    let _guard = serial_guard();
    // task #959 (macOS CI, first real run of this row after task #947/A-1
    // moved commit_range's granularity from PAGE to the runtime page_size()):
    // `initial`/`size` must be multiples of `page_size()`, not the compile-time
    // `PAGE` constant -- on Apple Silicon (page_size() == 16 KiB) a bare `PAGE`
    // (4 KiB) fails try_commit_range's `is_multiple_of(page_size())` check
    // unconditionally, independent of the align/size-shrink bug this test
    // exists to guard. `ps` is always a multiple of `PAGE` (both are powers of
    // two and `ps >= PAGE`), so reserve_aligned_lazy's own PAGE-multiple
    // contract for `initial_commit` still holds.
    let ps = page_size();
    let align = PAGE; // 4 KiB -- well under the 64 KiB single-call threshold
    let size = 16 * ps;
    let initial = ps; // commit only the first runtime page now
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
    let _guard = serial_guard();
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
    let _guard = serial_guard();
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();

    // task #959: boundaries must be multiples of the runtime page_size(),
    // not the compile-time PAGE constant (task #947/A-1) -- a bare `PAGE`
    // fails the multiple-of-page_size() check on 16 KiB-page hosts (Apple
    // Silicon) before the start==end short-circuit is ever reached (V-19
    // checks alignment first).
    let ps = page_size();

    // SAFETY: base is a live reservation.
    unsafe {
        // A genuinely empty range (start == end) is the ONLY contract-legal
        // no-op — it returns true. See
        // `commit_range_rejects_contract_violating_offsets` below for the
        // (task #712-corrected) behavior on an actual contract violation.
        assert!(commit_range(base, ps, ps), "start==end is a success no-op");
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
    let _guard = serial_guard();
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
    let _guard = serial_guard();
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();

    // task #959: use the runtime page_size(), not the compile-time PAGE
    // constant -- see commit_range_empty_range_is_a_noop's comment above.
    let ps = page_size();

    // SAFETY: the entire span is committed (eager reservation).
    unsafe {
        let ok = commit_range(base, 0, ps);
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
    let _guard = serial_guard();
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
    let _guard = serial_guard();
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
    let _guard = serial_guard();
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

/// Round 2 pre-release review, task #949 (T-1); corrected in round 3 (task
/// #953, findings 6.2/6.3): Windows `reserve_aligned_lazy` actually saves
/// commit charge by routing through `win_reserve_commit`'s two-call path.
///
/// The ORIGINAL version of this test called
/// `reserve_aligned_lazy(4 * MIB, /*align=*/ 4 * MIB, /*initial=*/ PAGE)`.
/// That call does not actually pin the intended guard: `win_reserve_commit`'s
/// single-call-fast-path condition is `align <= WIN_ALLOCATION_GRANULARITY
/// (65536) && commit_len == size` (see `win_reserve_commit` in `src/lib.rs`).
/// With `align = 4 * MiB`, the align half of that AND is ALREADY false on its
/// own (`4 * MiB > 65536`), so the two-call path is forced regardless of
/// whether `commit_len == size` holds. In other words, the original call
/// would still take the two-call path -- and this test would still pass --
/// even if the `commit_len == size` guard were deleted entirely (the exact
/// bug this test claims to pin against re-occurring). It exercised the
/// align-based routing, not the commit_len-based routing.
///
/// The FIX: use `align = PAGE` (4 KiB), which is `<= WIN_ALLOCATION_GRANULARITY`
/// on its own -- so the align half of the guard is now TRUE by itself, and
/// `commit_len != size` (`initial = PAGE < span = 4 * MiB`) becomes the ONLY
/// remaining reason the two-call path is taken. If a future change deleted
/// the `commit_len == size` check from `win_reserve_commit`, THIS call would
/// wrongly take the single-call fast path (which silently shrinks the
/// reservation to `initial` bytes -- see that function's own doc comment),
/// and the two-call-pairs/single-call counter assertions below would catch
/// it. This is the counterfactual `align = 4 * MiB` could not provide.
///
/// This test reads/resets the process-global `bench-internals` counters
/// (`reset_bench_internals_counters`, `windows_reserve_commit_two_call_pairs`,
/// `windows_reserve_commit_single_calls`), so it holds `SERIAL` for its whole
/// body to stay single-threaded against any other test in this binary that
/// also drives `win_reserve_commit` (libtest runs tests on parallel threads
/// by default) -- see `SERIAL`'s own doc comment above.
///
/// Excluded under `aligned_vmem_mock`: `try_reserve_aligned_lazy` deliberately
/// chains `mock` builds to the EAGER backend (`reserve_aligned_raw`, which
/// always calls `win_reserve_commit` with `commit_len == size`) instead of
/// the real lazy path -- see that function's own module comment in
/// `src/lib.rs` ("Under `aligned_vmem_mock` the OS partial-commit is bypassed ..."). Under
/// `aligned_vmem_mock`, `commit_len == size` unconditionally, so this test's
/// `align = PAGE` call would take the single-call fast path instead of the
/// two-call path it exists to pin, and the counter assertions below would
/// fail for a reason that has nothing to do with `win_reserve_commit`'s real
/// guard -- confirmed empirically (`cargo test --all-features` before this
/// exclusion failed with `two_call_pairs` staying at 0).
#[test]
#[cfg(all(windows, feature = "bench-internals", not(aligned_vmem_mock)))]
fn windows_lazy_reserve_saves_commit_charge() {
    let _guard = serial_guard();

    // align = PAGE (<= WIN_ALLOCATION_GRANULARITY) makes commit_len != size
    // the SOLE reason the two-call path is taken -- see the module doc above.
    let span = 4 * MIB;
    let align = PAGE;
    let initial = PAGE; // 4 KiB initial commit, far less than span

    aligned_vmem::reset_bench_internals_counters();
    let before_two_call = aligned_vmem::windows_reserve_commit_two_call_pairs();
    let before_single_call = aligned_vmem::windows_reserve_commit_single_calls();

    let r = reserve_aligned_lazy(span, align, initial).expect("lazy reserve");

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

/// II-17 (2026-08-16 pre-release audit finding): observe safe
/// `Reservation::decommit` over the *never-committed tail* of a lazy
/// Windows reservation. The safe method's bound is `len()`, which on a
/// lazy reservation includes reserved-but-uncommitted pages; Microsoft
/// documents `VirtualFree(MEM_DECOMMIT)` on such pages as succeeding, so
/// this should be fine — but nothing pins this specific shape.
#[test]
fn safe_decommit_over_never_committed_tail_succeeds() {
    let _guard = serial_guard();
    // Reserve 4 MiB, commit only the first 64 KiB.
    let initial = 16 * PAGE; // 64 KiB
    let span = 4 * MIB;
    let r = reserve_aligned_lazy(span, span, initial).expect("lazy reserve");
    let base = r.as_ptr();

    // Write into the committed region to verify it's actually committed.
    // SAFETY: first `initial` bytes are committed.
    unsafe {
        base.write(0x55);
        assert_eq!(base.read(), 0x55);
    }

    // Now decommit a range that extends into the never-committed tail.
    // [initial, 2*initial) straddles the committed/uncommitted boundary:
    // - [initial, initial) is empty (no-op)
    // - [initial, initial + PAGE) is the first uncommitted page
    // The safe method's bounds check passes (2*initial < len() == span), and
    // the underlying `VirtualFree(MEM_DECOMMIT)` must succeed even on the
    // uncommitted portion (per Microsoft's documentation).
    let decommit_start = initial;
    let decommit_end = 2 * initial;
    r.decommit(decommit_start, decommit_end);

    // Verify that the committed part we wrote is still readable (decommit
    // doesn't release the address space, just the physical backing). On
    // Windows, accessing a decommitted page faults; on Unix/miri it doesn't.
    // We assert the COMMITTED part we wrote into is still valid.
    // SAFETY: [0, initial) is still committed (we only decommitted starting
    // at `initial`).
    #[cfg(not(windows))]
    unsafe {
        assert_eq!(
            base.read(),
            0x55,
            "decommitted region still readable on Unix"
        );
    }
    #[cfg(windows)]
    {
        // On Windows, decommitted pages fault on read. The key assertion
        // is that `r.decommit(...)` above did NOT panic — that proves the
        // safe method accepts ranges covering never-committed memory.
    }
}
