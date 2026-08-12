//! Smoke tests for `aligned-vmem`: reservation alignment, read/write, decommit
//! round-trip, RAII vs manual release, and contract rejection.

use aligned_vmem::{
    decommit_lazy, leak_zeroed_pages, page_size, recommit, release, reserve_aligned,
    try_reserve_aligned, Reservation, VmemError, PAGE,
};
use std::panic;

const MIB: usize = 1024 * 1024;

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

/// Non-huge reservation never reports huge (regression for W2 fix:
/// non-Linux Unix used to return true for ordinary-page reservations).
#[test]
fn ordinary_reservation_never_reports_huge() {
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("ordinary reservation");
    assert!(
        !r.is_huge(),
        "an ordinary reservation must never report huge"
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
    assert_eq!(base as usize % span, 0, "base must be span-aligned");
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
        #[cfg(not(any(miri, feature = "mock")))]
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
