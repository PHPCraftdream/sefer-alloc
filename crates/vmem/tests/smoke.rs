//! Smoke tests for `aligned-vmem`: reservation alignment, read/write, decommit
//! round-trip, RAII vs manual release, and contract rejection.

use aligned_vmem::{
    decommit_lazy, leak_zeroed_pages, page_size, recommit, release, reserve_aligned,
    try_reserve_aligned, Reservation, VmemError, PAGE,
};
use std::panic;
use std::sync::Mutex;

const MIB: usize = 1024 * 1024;

/// Zero-trust review of task #882: under `bench-internals`, every
/// `decommit`/`decommit_lazy` call increments the PROCESS-GLOBAL
/// `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` counters (see
/// `libc_madvise` in `lib.rs`). libtest runs this file's tests on parallel
/// threads by default, so any test that both resets those counters and then
/// asserts an EXACT count on them would otherwise race against every other
/// test in this same binary that also calls `decommit`/`decommit_lazy`
/// concurrently — mirroring the exact hazard `tests/fault_injection.rs`'s own
/// `SERIAL` mutex already exists to prevent for its process-global hooks.
/// Every test in this file that calls `decommit`/`decommit_lazy` takes this
/// lock for its whole body so the shared counters are exercised
/// single-threaded when it matters (`bench-internals` builds); the lock
/// itself is cheap enough to hold unconditionally even when the feature is
/// off, so no `#[cfg]` branching is needed here.
static SERIAL: Mutex<()> = Mutex::new(());

// task #719: `unsafe impl Send for Reservation {}` had no test at all pinning
// the claim. `assert_send` intentionally checks ONLY `Send`, not `Sync` --
// `Reservation` is documented as deliberately NOT `Sync` (unsynchronised
// writes through the raw pointer), and Rust auto-traits mean there is no
// unconditional POSITIVE assertion to write for a negative claim; proving the
// negative would need a `compile_fail` doctest or a `trybuild` dependency,
// mirroring the same tradeoff `sefer-region`'s own `Handle<T>` static
// assertions already made (see `crates/region/tests/handle_static_asserts.rs`).
const fn assert_send<T: Send>() {}
const _: () = assert_send::<Reservation>();

/// V7 fix: Reservation now has a Debug impl.
#[test]
fn reservation_has_debug_output() {
    let r = reserve_aligned(4 * MIB, 4 * MIB).expect("reserve 4 MiB");
    let debug_str = format!("{:?}", r);
    assert!(
        debug_str.contains("Reservation"),
        "Debug output should contain type name"
    );
    assert!(
        debug_str.contains("base"),
        "Debug output should contain base field"
    );
    assert!(
        debug_str.contains("len"),
        "Debug output should contain len field"
    );
    assert!(
        debug_str.contains("reservation_len"),
        "Debug output should contain reservation_len field"
    );
    assert!(
        debug_str.contains("align"),
        "Debug output should contain align field"
    );
    assert!(
        debug_str.contains("granted_huge"),
        "Debug output should contain granted_huge field"
    );
}

/// Pins `Reservation::is_huge()` to read the `granted_huge` field: the
/// ordinary (non-huge) path hard-codes `granted_huge: false` as a literal
/// two call frames up (`reserve_aligned` -> `try_reserve_aligned` ->
/// `reserve_aligned_raw(..).map(...)`, `lib.rs:955-967`, the literal at
/// `:963`), so this assertion is unconditionally true on every
/// platform/feature combo and cannot fail against a regression on this
/// path -- it is NOT a W2 regression guard. The real W2 regression test
/// (non-Linux Unix used to return true for `reserve_aligned_huge`
/// reservations) is `huge_pages.rs:61-62`'s
/// `#[cfg(not(target_os = "linux"))] assert!(!r.is_huge())`, which calls
/// the huge path and does fail if `HUGE_SUPPORTED` is reverted to an
/// unconditional `true`.
#[test]
fn ordinary_reservation_never_reports_huge() {
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("ordinary reservation");
    assert!(
        !r.is_huge(),
        "an ordinary reservation must never report huge"
    );
}

/// Round-5 closing review (QC8): on the Windows single-call fast path
/// (`align <= 64 KiB`, `commit_len == size`, task #848), `reservation_len()`
/// is documented (`lib.rs`'s `reservation_len` rustdoc) to report
/// `commit_len`, NOT the true OS reservation size — Windows internally
/// rounds VA reservations up to the 64 KiB allocation granularity. This is
/// at least two paths in the crate where `reservation_len()` deliberately
/// does NOT report the true reservation size — the other being any
/// page-rounding `mmap` where the OS page size exceeds the requested
/// granularity (e.g. Apple-Silicon macOS's 16 KiB pages) — and until now
/// nothing asserted this (Windows) one.
#[cfg(windows)]
#[test]
fn windows_single_call_fast_path_reservation_len_reports_commit_len_not_true_size() {
    let r = reserve_aligned(PAGE, PAGE).expect("reserve 4 KiB, aligned to 4 KiB");
    assert_eq!(
        r.reservation_len(),
        r.len(),
        "on the Windows single-call fast path, reservation_len() reports \
         commit_len (== requested size), not the true (64 KiB-rounded) OS \
         reservation"
    );
}

/// V8 fix: ReservationParts prevents swapping len and align.
#[test]
fn reservation_parts_prevents_parameter_swap() {
    let r = reserve_aligned(4 * MIB, 4 * MIB).expect("reserve 4 MiB");
    let parts = r.into_reservation_parts();

    // Verify the struct fields are correctly populated.
    assert!(!parts.ptr.is_null());
    // reservation_len may be larger than requested due to over-reserve for alignment.
    assert!(parts.len >= 4 * MIB);
    assert_eq!(parts.align, 4 * MIB);

    // Verify we can release via release_parts.
    unsafe { aligned_vmem::release_parts(parts) };

    // Verify the old tuple path still works (backwards compatibility).
    let r2 = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve 2 MiB");
    let (raw, raw_len, raw_align) = r2.into_parts();
    unsafe { aligned_vmem::release(raw, raw_len, raw_align) };

    // Verify ReservationParts::as_tuple bridges the two.
    let r3 = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve 2 MiB");
    let parts3 = r3.into_reservation_parts();
    let (raw2, raw_len2, raw_align2) = parts3.as_tuple();
    unsafe { aligned_vmem::release(raw2, raw_len2, raw_align2) };
}

#[test]
fn reserve_is_aligned_and_writable() {
    let span = 4 * MIB;
    let r = reserve_aligned(span, span).expect("reserve 4 MiB aligned 4 MiB");
    let base = r.as_ptr();
    assert!(!base.is_null());
    assert_eq!(base.addr() % span, 0, "base must be span-aligned");
    assert_eq!(r.len(), span);

    // Write/readback the whole span at page stride to fault pages in.
    // SAFETY: base is valid for r.len() bytes; we own it exclusively.
    unsafe {
        let mut off = 0;
        while off < span {
            base.add(off).write(0xA5);
            assert_eq!(base.add(off).read(), 0xA5);
            off += PAGE;
        }
    }
    // RAII: dropping `r` releases the reservation.
}

#[test]
fn manual_release_via_into_parts() {
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: valid for r.len().
    unsafe { base.write(0x11) };
    let (raw, raw_len, raw_align) = r.into_parts();
    assert!(!raw.is_null());
    assert_eq!(raw_align, span);
    // SAFETY: triple came from into_parts, released exactly once.
    unsafe { release(raw, raw_len, raw_align) };
}

#[test]
fn decommit_recommit_roundtrip() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let span = 4 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: write into the second half.
    unsafe {
        base.add(span / 2).write(0x77);
        assert_eq!(base.add(span / 2).read(), 0x77);
    }
    // Decommit the second half, then recommit it.
    // SAFETY: base is a live reservation; [span/2, span) is page-aligned and
    // contains nothing we still need.
    unsafe {
        assert!(recommit(base, 0, 0), "empty range no-op reports success");
        aligned_vmem::decommit(base, span / 2, span);
        assert!(
            recommit(base, span / 2, span),
            "recommit of a live reservation's decommitted range must succeed"
        );
        // After recommit the page reads as zero (fresh OS page). Skipped under
        // miri AND under the `mock` feature: both model decommit/recommit as
        // no-ops (no real RSS / zero-fill-on-recommit), so the previously-
        // written byte legally persists — this zero-fill guarantee is a
        // real-OS property.
        //
        // ALSO skipped on the Darwin family (confirmed as a real,
        // failing-test-level gap by this crate's first real-macOS CI run,
        // 2026-08-13 -- the underlying hazard was already known repo-wide
        // since Round 9, see docs/CORRECTNESS_OPEN_ITEMS.md item 48 and
        // decommit()'s own rustdoc):
        // `MADV_DONTNEED` is advisory-only for anonymous memory on all XNU-
        // based targets (macOS/iOS/tvOS/watchOS share the same kernel and
        // MADV_DONTNEED semantics, not just macOS) and does NOT reliably
        // unmap/zero the pages the way it does on Linux, so this assertion
        // is not a platform bug in the crate under test -- it is a real,
        // confirmed gap in decommit()'s "return physical backing to the OS"
        // promise on the Darwin family specifically. The guarantee genuinely
        // holds on Linux (MADV_DONTNEED) and Windows (VirtualFree(MEM_DECOMMIT)
        // + VirtualAlloc(MEM_COMMIT)).
        #[cfg(not(any(
            miri,
            feature = "mock",
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos"
        )))]
        assert_eq!(
            base.add(span / 2).read(),
            0,
            "recommitted page must be zeroed"
        );
    }
}

#[test]
fn recommit_is_fallible_and_reports_success_on_the_happy_path() {
    // Non-regression for the fallible `recommit` API (bug-hunt 2026-07-09):
    // `recommit` now returns `bool` (`true` = committed / no-op, `false` = OS
    // refused OR a contract violation). We cannot portably force a
    // commit-charge failure without an FFI test seam, so this locks the
    // SUCCESS contract: a well-formed recommit of a decommitted range on a
    // live reservation returns `true`; a genuinely EMPTY range (`start ==
    // end`) is also a success no-op. A `false` from a genuine OOM is the
    // path `carve_block`/`carve_batch` translate into a null carve.
    //
    // task #712: a contract-VIOLATING range (misaligned, or `start > end`)
    // used to also return `true` here, clamped to the WRITE-PERMITTING
    // sentinel — see `recommit_rejects_contract_violating_offsets` below for
    // the corrected (and separately regression-tested) behavior.
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: base is a live reservation for `span` bytes.
    unsafe {
        assert!(recommit(base, 0, 0), "empty range is a success no-op");
        aligned_vmem::decommit(base, span / 2, span);
        assert!(
            recommit(base, span / 2, span),
            "recommit of decommitted range on a live reservation succeeds"
        );
        // Writing into the now-committed range must not fault.
        base.add(span / 2).write(0x5C);
        assert_eq!(base.add(span / 2).read(), 0x5C);
    }
}

#[test]
fn recommit_rejects_contract_violating_offsets() {
    // task #712 (rust-intel audit MEDIUM, already crashed an in-repo
    // consumer): `recommit`/`try_recommit` used to clamp a contract
    // VIOLATION (misaligned offsets, or `start > end`) to the same
    // WRITE-PERMITTING sentinel a genuine success reports (`true` /
    // `Ok(())`) — on Windows, a caller that (incorrectly) trusted that
    // sentinel and wrote into the range took a hard
    // `STATUS_ACCESS_VIOLATION`, since nothing was actually committed. Fixed
    // to return `false` / `Err(VmemError::invalid_argument())` for a genuine
    // violation, while a truly EMPTY range (`start == end`) stays a success
    // no-op (see the happy-path test above).
    let span = 2 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: base is a live reservation for `span` bytes; none of the calls
    // below reach the real commit syscall (all are rejected before it).
    unsafe {
        assert!(
            !recommit(base, 1, PAGE),
            "misaligned start must be rejected, not silently permitted"
        );
        assert!(
            !recommit(base, 0, PAGE + 1),
            "misaligned end must be rejected, not silently permitted"
        );
        assert!(
            !recommit(base, span + PAGE, span),
            "start > end (inverted range) must be rejected, not silently permitted"
        );
        assert!(
            aligned_vmem::try_recommit(base, 1, PAGE)
                .unwrap_err()
                .is_invalid_argument(),
            "the fallible form must carry VmemError::invalid_argument(), not an OS code"
        );
    }
}

#[test]
fn rejects_bad_contracts() {
    assert!(reserve_aligned(0, PAGE).is_none(), "zero size rejected");
    assert!(
        reserve_aligned(PAGE, 3).is_none(),
        "non-pow2 align rejected"
    );
    assert!(reserve_aligned(PAGE, 64).is_none(), "align < PAGE rejected");
    assert!(
        reserve_aligned(PAGE + 1, PAGE).is_none(),
        "non-page-multiple size rejected"
    );
}

#[test]
fn page_size_is_a_valid_os_page() {
    // 0.2: `page_size()` now queries the OS (was a hardcoded 4 KiB). It must be
    // a non-zero power of two and at least the crate's minimum granularity
    // `PAGE` (4 KiB). On x86_64/aarch64-4k it is 4 KiB; on Apple Silicon macOS
    // 16 KiB; on some Linux configs 64 KiB — all satisfy this invariant.
    let ps = page_size();
    assert!(ps.is_power_of_two(), "page size must be a power of two");
    assert!(ps >= PAGE, "OS page size must be at least PAGE (4 KiB)");
    // Cached: a second call returns the same value.
    assert_eq!(page_size(), ps);
    assert_eq!(PAGE, 4096);
}

/// Round-6 review (S6) / `docs/CORRECTNESS_OPEN_ITEMS.md` item 43: the
/// generic `is_power_of_two() && >= PAGE` check above cannot catch a wrong
/// `_SC_PAGESIZE` constant on a real OS, because `page_size()`'s own silent
/// fallback (`lib.rs`'s `page_size()`, the `queried >= PAGE &&
/// queried.is_power_of_two()` guard) returns `PAGE` (4 KiB) whenever the
/// queried value is garbage -- and 4 KiB is itself a power of two `>= PAGE`,
/// so a broken macOS `_SC_PAGESIZE` constant would pass the generic test
/// silently instead of failing it. This crate's macOS CI runner is
/// `macos-26-arm64` (Apple Silicon, aarch64), where the page size is
/// architecturally, unconditionally 16 KiB -- a hard expected value on
/// hardware this crate's CI already runs. Assert it exactly, closing the
/// gap the generic test structurally cannot.
///
/// Excluded under `miri` (task #890, review finding T3): `lib.rs`'s
/// `query_os_page_size()` has a `#[cfg(miri)]` arm that unconditionally
/// returns `PAGE` (4 KiB) -- miri has no real OS page to query -- so under
/// miri on aarch64 Darwin `page_size()` is always 4096, and this assertion
/// would fail by construction, not because of a real bug. Matches the
/// `not(miri)` exclusion this crate's other real-OS-property assertions
/// already use (e.g. the zero-fill assertion above, the madvise oracle
/// below, and this file's own
/// `decommit_recommit_roundtrip`, whose
/// `not(miri)`-gated zero-fill read is mirrored by
/// `lazy_commit.rs`'s `sequential_commit_range_grows_incrementally` for the
/// identical real-OS-zero-fill-vs-miri distinction -- corrected round 8,
/// task #900/U2, from a prior version of this comment that misnamed the
/// precedent as `recommit_is_fallible_and_reports_success_on_the_happy_path`,
/// which has no `#[cfg]` gate and no zero-fill read at all (its only
/// post-recommit assertion is a write-then-read-back true on every backend
/// including miri and mock) -- itself a correction of an even earlier
/// version (round 7, task #895/TC5) that misnamed the precedent as
/// `decommit_lazy_roundtrip`'s sibling, which does not exist).
#[cfg(all(target_os = "macos", target_arch = "aarch64", not(miri)))]
#[test]
fn apple_silicon_page_size_is_16_kib() {
    assert_eq!(page_size(), 16 * 1024);
}

#[test]
fn try_reserve_reports_invalid_argument() {
    // 0.2 fallible API: a contract violation yields InvalidArgument (no OS call).
    let e = match try_reserve_aligned(0, PAGE) {
        Ok(_) => panic!("zero size must be rejected"),
        Err(e) => e,
    };
    assert!(e.is_invalid_argument());
    assert_eq!(e.os_code(), None);
    assert_eq!(e, VmemError::invalid_argument());
    // A well-formed request succeeds.
    let r = try_reserve_aligned(2 * MIB, 2 * MIB).expect("valid request");
    assert_eq!(r.len(), 2 * MIB);
}

#[test]
fn decommit_lazy_roundtrip() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // 0.2 MADV_FREE variant: decommit_lazy then recommit and write.
    let span = 4 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();
    // SAFETY: write, lazily decommit, recommit, write again.
    unsafe {
        base.add(span / 2).write(0x9E);
        decommit_lazy(base, span / 2, span);
        assert!(
            recommit(base, span / 2, span),
            "recommit after decommit_lazy must succeed"
        );
        base.add(span / 2).write(0x3C);
        assert_eq!(base.add(span / 2).read(), 0x3C);
    }
}

/// task #882 (S2+S4): item 48's root cause ("`MADV_DONTNEED` is
/// advisory-only for anonymous memory on Darwin") was ASSERTED from a single
/// failing byte, not established -- the failure (a byte survived a
/// decommit+recommit cycle) is equally consistent with a totally different
/// hypothesis (H2: the `madvise(2)` syscall itself FAILED on that CI runner
/// for an unrelated reason), because `libc_madvise` discards `madvise`'s
/// return value BY DESIGN (task #719) and nothing else in the crate could
/// tell the two hypotheses apart. This test is the empirical oracle: under
/// `bench-internals`, `libc_madvise` also records success/failure into
/// `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES`. Asserting
/// `unix_madvise_successes() == unix_madvise_attempts() > 0` here proves the
/// `madvise` SYSCALL ITSELF succeeded for both the eager (`decommit`,
/// `MADV_DONTNEED`) and lazy (`decommit_lazy`, `MADV_FREE_REUSABLE`) call
/// sites -- ruling OUT H2 when it passes (the syscall did return 0), which
/// then leaves H1 (advisory-only semantics) as the only remaining
/// explanation for the stale byte, WITHOUT this crate having macOS hardware
/// to run the confirmation on directly. **This test HAS now run and passed
/// on real macOS CI** (round 7, task #895/TC6, correcting a prior version of
/// this comment that still described the run as future work): CI run
/// `31692217669`, job `94421845398` (`test macos (production)`, image
/// `macos-26-arm64`), `unix_madvise_attempts() == unix_madvise_successes()
/// == 2` -- H2 is ruled out. This does NOT by itself confirm H1: the
/// stale-byte half of the H1 argument comes from a DIFFERENT CI run
/// (`31676133649`, commit `e60e46a`, before this file's zero-fill assertion
/// was scoped off Darwin); see `docs/CORRECTNESS_OPEN_ITEMS.md` item 48's
/// Root-cause bullet for the full two-run wording -- keep this comment in
/// sync with that bullet if either changes. Also restores
/// the effect-observing coverage lost when commit 9c777bc scoped the
/// zero-fill assertion off macOS in `decommit_recommit_roundtrip` above: that
/// scoping meant NO test on any platform still observed whether macOS
/// decommit/recommit has any real effect at all -- this test at least proves
/// the syscall path is exercised and reports success, even though it cannot
/// prove the OS-level RSS/zero-fill outcome without real hardware.
///
/// `bench-internals`-gated (diagnostic-only counters, matches this crate's
/// established `bench-internals` convention -- see `UNIX_EXACT_RESERVE_HITS`
/// et al. in `lib.rs`) and `target_os = "macos"`-gated for the H1-vs-H2
/// question specifically. NOTE (round-6 closing review, SC1): this test's
/// eager-`decommit` half would be redundant on Linux/Windows, which already
/// have a passing zero-fill assertion in `decommit_recommit_roundtrip`
/// above -- but `decommit_lazy_roundtrip` below has NO effect-observing
/// assertion on ANY platform (it only checks that a write after
/// decommit_lazy+recommit reads back, which is true whether `madvise`
/// succeeded, failed, or was never called). This oracle's `madvise`-success
/// counters are `unix`-wide (`libc_madvise` is `#[cfg(all(unix, not(miri)))]`,
/// not macOS-specific), so a Linux instance of the same assertion would
/// close that gap too -- not done here because no CI row runs
/// `bench-internals` against the real (non-mock) Unix backend on Linux; see
/// `docs/CORRECTNESS_OPEN_ITEMS.md` item 48's S4 sub-note.
/// Also excluded under `mock` (the recording backend never calls the real
/// `madvise(2)`, so the counters would stay at 0 by construction, not by
/// answering the question) and `miri` (no real FFI).
#[cfg(all(
    target_os = "macos",
    feature = "bench-internals",
    not(feature = "mock"),
    not(miri)
))]
#[test]
fn macos_decommit_madvise_syscall_actually_succeeds() {
    use aligned_vmem::{
        decommit, decommit_lazy, reset_bench_internals_counters, unix_madvise_attempts,
        unix_madvise_successes,
    };

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let span = 4 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();

    // Clean counters so this measurement window is not polluted by any
    // earlier test in the same binary that also exercised `libc_madvise`
    // (e.g. `decommit_recommit_roundtrip`/`decommit_lazy_roundtrip` above,
    // which run in the same process under `cargo test`'s default
    // multi-threaded-but-shared-process test execution).
    //
    // Round-6 closing review SC9: this also zeroes UNIX_EXACT_RESERVE_ATTEMPTS/
    // _HITS and the Windows counters, which every `reserve_aligned` call in
    // this file's ~14 other tests also increments -- none of them holds
    // SERIAL or asserts on those counters today, so there is no live race,
    // but a future test that does add such an assertion would get a
    // silently flaky result rather than a compile error unless it also
    // joins SERIAL's contract.
    reset_bench_internals_counters();

    // SAFETY: base is a live, exclusively-owned reservation for `span` bytes;
    // both decommit calls target disjoint page-aligned halves.
    unsafe {
        decommit(base, 0, span / 2);
        decommit_lazy(base, span / 2, span);
    }

    // Round-6 closing review SC10: this exact-count assertion is correct
    // TODAY (verified: libc_madvise is the sole incrementer, called only
    // from decommit_pages_impl's two arms) but item 48's Darwin lazy-path
    // alternative fix note (S9) records a candidate future change that
    // would add a second call per cycle (MADV_FREE_REUSE from `recommit`).
    // If that lands, update this count and the message below in the same
    // commit -- otherwise this test starts failing with a message that
    // reads like an H2 confirmation when only the call count changed.
    let attempts = unix_madvise_attempts();
    let successes = unix_madvise_successes();
    assert_eq!(
        attempts, 2,
        "exactly one madvise(2) call expected per decommit()/decommit_lazy() \
         call above (eager MADV_DONTNEED + lazy MADV_FREE_REUSABLE), got {attempts}"
    );
    assert_eq!(
        successes, attempts,
        "the madvise(2) SYSCALL ITSELF must succeed (return 0) for both the \
         eager and lazy decommit call sites on macOS -- if this fails, item \
         48's root cause is H2 (the syscall failed), not H1 (advisory-only \
         semantics); {successes}/{attempts} succeeded"
    );

    // `base` was decommitted, not deallocated -- still a live reservation
    // that must be released exactly once, via `r`'s own Drop here.
    drop(r);
}

/// task #902 (review finding U7, LOW): mirrors
/// `decommit_silently_skips_contract_violating_offsets` in `mock.rs`, but at
/// the real-syscall layer instead of the mock call-log layer -- proves a
/// contract-violating `decommit`/`decommit_lazy` call never even reaches
/// `libc_madvise` (the counters it increments stay untouched), not merely
/// that the crate's own mock recorder saw nothing. Without this, a future
/// "simplification" that changed the validation base in `lib.rs`'s
/// `decommit`/`decommit_lazy` from `page_size()` to the crate's smaller
/// `PAGE` constant (both guards currently read `let ps = page_size();`) would
/// forward a `PAGE`-aligned-but-not-`page_size()`-aligned offset straight to
/// `madvise(2)` on any host where the OS page size exceeds `PAGE` (e.g. a 16
/// KiB-page Apple Silicon host) -- `madvise` rejects the WHOLE call in that
/// case (see `decommit`'s own rustdoc on the all-or-nothing failure mode),
/// which this crate's `mock`-feature test suite has no way to observe at
/// all, and which would go undetected on any CI runner whose OS page size
/// happens to equal `PAGE` (the common case). Gated on any Unix (not just
/// macOS): `unix_madvise_attempts()`'s counters are `unix`-wide, matching
/// `macos_decommit_madvise_syscall_actually_succeeds` above's own note that a
/// Linux instance of this style of assertion was a known gap.
#[cfg(all(unix, feature = "bench-internals", not(feature = "mock"), not(miri)))]
#[test]
fn decommit_contract_violation_never_reaches_madvise() {
    use aligned_vmem::{reset_bench_internals_counters, unix_madvise_attempts};

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let span = 4 * MIB;
    let r = reserve_aligned(span, span).expect("reserve");
    let base = r.as_ptr();

    // Clean slate: see the identical rationale in
    // `macos_decommit_madvise_syscall_actually_succeeds` above (this file
    // runs tests on parallel threads by default; SERIAL plus a reset keeps
    // this measurement window uncontaminated by any other test in this
    // binary that also calls `decommit`/`decommit_lazy`).
    reset_bench_internals_counters();

    // SAFETY: base is a live reservation for `span` bytes; both calls below
    // are contract VIOLATIONS (misaligned start; inverted start > end) that
    // the crate's own guard must reject before any real syscall is issued.
    unsafe {
        aligned_vmem::decommit(base, 1, PAGE);
        decommit_lazy(base, PAGE, 0);
    }

    // task #904 (round-8 closing review, UC4): the two calls above are
    // rejected under EITHER validation base (`page_size()` or `PAGE`), so by
    // themselves they cannot detect the specific mistake this test's own doc
    // comment names -- a future `let ps = page_size();` -> `let ps = PAGE;`
    // edit at the `decommit`/`decommit_lazy` guards. Only a
    // `PAGE`-aligned-but-not-`page_size()`-aligned offset discriminates the
    // two bases: rejected under `page_size()`, forwarded to `madvise(2)`
    // under `PAGE`. That offset exists only when `page_size() > PAGE` (e.g.
    // 16 KiB Apple Silicon); on every other host (4 KiB pages) the two bases
    // are the same value and no offset can tell them apart, so this arm is a
    // no-op there and lives only on the macOS CI runner. Both `decommit`'s
    // and `decommit_lazy`'s guards get their own discriminating call below
    // (task #906, V2-1) -- a single call cannot cover both, since each
    // function validates independently.
    if page_size() > PAGE {
        // SAFETY: same live reservation; `PAGE` is a genuine multiple of
        // `PAGE` but (by the `if` above) NOT a multiple of `page_size()`, so
        // this is still a contract violation under the crate's real
        // validation base and must be rejected the same way.
        unsafe {
            aligned_vmem::decommit(base, PAGE, 2 * PAGE);
        }
        // SAFETY: same live reservation; same contract argument as the
        // `decommit` call immediately above -- `PAGE` is a genuine multiple
        // of `PAGE` but (by the `if` above) NOT a multiple of `page_size()`,
        // so this is a contract violation under `decommit_lazy`'s own real
        // validation base too. Without this call, a future edit that swaps
        // `decommit_lazy`'s guard from `page_size()` to `PAGE` (lib.rs) is
        // NOT caught here: the only `decommit_lazy` call above
        // (`decommit_lazy(base, PAGE, 0)`) is rejected under EITHER base by
        // `start >= end` alone, so it cannot discriminate.
        unsafe {
            decommit_lazy(base, PAGE, 2 * PAGE);
        }
    }

    let attempts = unix_madvise_attempts();
    assert_eq!(
        attempts, 0,
        "a contract-violating decommit()/decommit_lazy() call must never \
         reach libc_madvise (the real madvise(2) syscall) at all -- got \
         {attempts} attempt(s), meaning the crate's validation guard was \
         bypassed or removed (or, on a host where page_size() > PAGE, that \
         the validation base was changed from page_size() to PAGE)"
    );

    // `base` was never actually decommitted (all calls above were rejected
    // before any OS effect) -- still a live reservation, released exactly
    // once via `r`'s own Drop here.
    drop(r);
}

#[test]
fn leak_zeroed_pages_is_zeroed_and_static() {
    // 0.2 helper: reserve zeroed pages leaked for the process lifetime.
    let size = 3 * PAGE + 7; // rounds up to 4 pages
    let p = leak_zeroed_pages(size).expect("leak zeroed");
    let base = p.as_ptr();
    assert_eq!(base as usize % PAGE, 0, "PAGE-aligned");
    // SAFETY: valid for at least `size` bytes, guaranteed zeroed on every backend.
    unsafe {
        for off in 0..size {
            assert_eq!(base.add(off).read(), 0, "byte {off} must be zero");
        }
        // Writable.
        base.write(0x42);
        assert_eq!(base.read(), 0x42);
    }
    assert!(leak_zeroed_pages(0).is_none(), "zero size rejected");
}

/// task #719: `from_raw_parts` used to accept any `align` and defer
/// validation to `Drop` time (a `Layout::from_size_align(...).expect(...)`
/// panic inside the miri backend's `release_reservation`, reachable from
/// `Drop::drop` -- if that fires while ANOTHER panic is already unwinding,
/// the process aborts). Fixed by validating the documented `align` contract
/// immediately, at the unsafe call site.
#[test]
#[should_panic(expected = "align must be a power of two >= PAGE")]
fn from_raw_parts_rejects_non_power_of_two_align_immediately() {
    // Reserve real memory so `base`/`reservation` are non-null and valid --
    // the panic under test is specifically about the `align` contract, not
    // the already-tested null checks.
    let r = reserve_aligned(PAGE, PAGE).expect("reserve");
    let (raw, raw_len, align) = r.into_parts();
    // SAFETY: `raw`/`raw_len` are a genuinely live reservation from the line
    // above; `align = 3` is deliberately NOT a power of two, which is exactly
    // the contract violation this test proves panics immediately instead of
    // being silently accepted and deferred to `Drop`. The process never
    // reaches a point where this "reservation" is used unsoundly -- the
    // `assert!` fires before `Self` is even constructed.
    let panic_info = panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        Reservation::from_raw_parts(raw, PAGE, raw, raw_len, 3)
    }))
    .err();
    // Release the reservation to avoid leaking under miri.
    // SAFETY: `raw`/`raw_len`/`align` are exactly the triple `into_parts`
    // just produced from a live, exclusively-owned reservation.
    unsafe { release(raw, raw_len, align) };
    // Re-panic with the original payload to satisfy `#[should_panic]`.
    std::panic::resume_unwind(panic_info.unwrap());
}

/// task #776 (F7): the original `assert!` validated only `align`, leaving
/// half of the SAME Drop-reachable-panic hazard open -- `Layout::from_size_align`
/// also fails when `reservation_len` overflows `isize::MAX` once rounded up
/// to `align`, and `from_raw_parts`'s own `# Safety` contract requires
/// `reservation_len` to be a valid, non-zero `PAGE`-multiple. Proves the
/// extended check closes this half too.
#[test]
#[should_panic(expected = "must form a valid Layout")]
fn from_raw_parts_rejects_an_overflowing_reservation_len_immediately() {
    let r = reserve_aligned(PAGE, PAGE).expect("reserve");
    let (raw, raw_len, align) = r.into_parts();
    // SAFETY: `raw`/`align` come from a genuinely live reservation above;
    // `reservation_len = usize::MAX` deliberately overflows `isize::MAX` when
    // `Layout::from_size_align` rounds it up to `align` -- exactly the
    // contract violation this test proves panics immediately. The process
    // never reaches a point where this "reservation" is used unsoundly.
    let panic_info = panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        Reservation::from_raw_parts(raw, PAGE, raw, usize::MAX, align)
    }))
    .err();
    // Release the reservation to avoid leaking under miri.
    // SAFETY: `raw`/`raw_len`/`align` are exactly the triple `into_parts`
    // just produced from a live, exclusively-owned reservation.
    unsafe { release(raw, raw_len, align) };
    // Re-panic with the original payload to satisfy `#[should_panic]`.
    std::panic::resume_unwind(panic_info.unwrap());
}

/// The positive-path complement to the panic test above: a genuinely valid
/// `align` must construct and behave normally (readable/writable, releases
/// cleanly on drop) -- `from_raw_parts`'s validation must reject ONLY
/// contract violations, not legitimate input.
#[test]
fn from_raw_parts_accepts_a_valid_reservation() {
    let r = reserve_aligned(PAGE, PAGE).expect("reserve");
    let (raw, raw_len, align) = r.into_parts();
    // SAFETY: `raw`/`raw_len`/`align` are exactly the triple `into_parts`
    // just produced from a live, exclusively-owned reservation, adopted back
    // with `base == reservation` and `len == raw_len` (the reservation has no
    // over-reserved head/tail here, since `size == align == PAGE`).
    let adopted = unsafe { Reservation::from_raw_parts(raw, raw_len, raw, raw_len, align) };
    let base = adopted.as_ptr();
    // SAFETY: the adopted reservation is a live, committed PAGE-byte span.
    unsafe {
        base.write(0x77);
        assert_eq!(base.read(), 0x77);
    }
    // Dropping `adopted` releases the reservation exactly once.
}

#[test]
fn distinct_reservations_do_not_overlap() {
    let span = 2 * MIB;
    let a = reserve_aligned(span, span).expect("a");
    let b = reserve_aligned(span, span).expect("b");
    let pa = a.as_ptr() as usize;
    let pb = b.as_ptr() as usize;
    // Non-overlapping usable spans.
    assert!(pa + span <= pb || pb + span <= pa, "reservations overlap");
}

// ── VmemError classification (task #713) ────────────────────────────────────

#[test]
fn vmem_error_kinds_are_distinguishable() {
    // task #713 (fold-in, §B26): `os_code()` used to be `Some(0)` for BOTH a
    // genuine `code 0` OS refusal and "no code available" -- storing
    // `Option<u32>` internally closes that ambiguity. Pin all three kinds and
    // their pairwise distinctions.
    let invalid = VmemError::invalid_argument();
    let real_zero = VmemError::from_os_code(0);
    let unknown = VmemError::os_refusal_unknown_code();
    let real_nonzero = VmemError::from_os_code(1455); // ERROR_COMMITMENT_LIMIT

    assert!(invalid.is_invalid_argument());
    assert_eq!(invalid.os_code(), None);

    assert!(!real_zero.is_invalid_argument());
    assert_eq!(
        real_zero.os_code(),
        Some(0),
        "a genuine OS refusal reporting code 0 must still be distinguishable \
         via os_code() == Some(0)"
    );

    assert!(!unknown.is_invalid_argument());
    assert_eq!(
        unknown.os_code(),
        None,
        "an OS refusal with no known code must report os_code() == None, \
         DISTINCT from invalid_argument() (also None) via is_invalid_argument()"
    );

    assert!(!real_nonzero.is_invalid_argument());
    assert_eq!(real_nonzero.os_code(), Some(1455));

    // `invalid` and `unknown` both have `os_code() == None` but must not be
    // conflated -- this is the whole point of the fix.
    assert_ne!(invalid, unknown);
    assert!(invalid.is_invalid_argument() != unknown.is_invalid_argument());

    // Display must not claim a specific code when none is known.
    let unknown_msg = format!("{unknown}");
    assert!(
        !unknown_msg.contains("code 0"),
        "os_refusal_unknown_code() must not print as if code 0 (ERROR_SUCCESS) \
         were the genuine cause: {unknown_msg}"
    );
    assert!(format!("{real_zero}").contains("code 0"));
}

#[test]
// task #714 zero-trust re-verification found this incompatible with miri:
// under miri's `std::alloc`-based fallback backend, there is no OS-level
// commit-charge limit to refuse against -- miri's own interpreter tries to
// genuinely honor the 64 TiB request and exhausts ITS OWN resources
// ("resource exhaustion: tried to allocate more memory than available to
// compiler") instead of returning null the way a real OS does. This test is
// specifically about the REAL backend's OS-refusal classification, which the
// miri fallback does not model at all -- skip under miri rather than assert
// on miri-interpreter-specific resource limits this test was never about.
#[cfg_attr(miri, ignore)]
fn try_reserve_huge_size_is_a_genuine_os_refusal_not_invalid_argument() {
    // task #713 end-to-end: a well-formed (page-aligned, power-of-two-aligned)
    // but far-past-any-realistic-commit-budget size must reach the real OS
    // backend and be classified as a genuine OS refusal -- NOT
    // VmemError::invalid_argument(), which is reserved for a contract
    // violation rejected BEFORE ever touching the OS. Verified concretely on
    // Windows: 1 << 46 (64 TiB) fails with `ERROR_COMMITMENT_LIMIT` (raw code
    // 1455), captured correctly (not a stale/irrelevant errno from
    // intervening cleanup FFI, and not the ambiguous `Some(0)` a pre-#713
    // `VmemError` could not tell apart from "no code available").
    let huge = 1usize << 46;
    match try_reserve_aligned(huge, PAGE) {
        Err(e) => assert!(
            !e.is_invalid_argument(),
            "a well-formed (if absurd) size/align must never be classified \
             as a caller contract violation: {e:?}"
        ),
        Ok(_) => {
            // Only plausible with genuinely enormous overcommit-backed
            // virtual memory; not itself a defect if it ever happens -- the
            // crate never promised a size ceiling beyond page-alignment.
        }
    }
}
