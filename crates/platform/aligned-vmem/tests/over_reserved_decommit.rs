//! An over-reserved span (base above the reservation start) survives a
//! decommit/recommit round trip. Moved out of `smoke.rs` (structure limit).

// Windows: the lazy path must be real (the mock backend serves a lazy reserve
// through the eager one-region path).
#![cfg(not(any(
    miri,
    all(windows, any(not(feature = "lazy-commit"), aligned_vmem_mock))
)))]

#[cfg(not(windows))]
use aligned_vmem::reserve_aligned;
use aligned_vmem::{page_size, recommit};

/// Round 2 pre-release review, task #949 (T-3); corrected in round 3 (task
/// #953, finding 6.4): decommit/recommit on a genuinely OVER-RESERVED span
/// (nonzero head offset, i.e. `as_ptr() != reservation_ptr()`). Every other
/// decommit/recommit test in this file uses `size == align`, which takes a
/// zero-offset path (`base == reservation`). This test forces a nonzero head
/// offset so that `base.add(...)` arithmetic is exercised against a base that
/// genuinely sits above the underlying OS reservation's start.
///
/// The ORIGINAL version of this test used `align = 4 * size` (16 KiB, with
/// `size = PAGE` = 4 KiB) and asserted nothing about the actual offset
/// obtained. That does not reliably force a nonzero offset on either real
/// backend:
///
/// - **Windows**: `win_reserve_commit`'s two-call path has a fast-reserve
///   sub-path (task #921/V-32) for `align <= WIN_ALLOCATION_GRANULARITY`
///   (64 KiB) that tries `VirtualAlloc(NULL, size, MEM_RESERVE)` first and
///   uses the result directly (zero offset, `base == region`) whenever the
///   candidate address HAPPENS to already be aligned to `align` -- which is
///   likely for a small `align` like 16 KiB, since `VirtualAlloc` bases are
///   already 64-KiB-granular. Using `align = 128 * 1024` (128 KiB, >
///   `WIN_ALLOCATION_GRANULARITY`) unconditionally skips that fast-reserve
///   sub-path and forces the `over = size + align` two-call path (see
///   `win_reserve_commit` in `src/lib.rs`) -- but even on THAT path,
///   `base_addr = align_up_addr(region_addr, align)` can still land on a
///   zero offset if `VirtualAlloc(NULL, over, MEM_RESERVE)`'s returned
///   address HAPPENS to already be `align`-aligned (confirmed empirically:
///   a single 128-KiB-align attempt landed on a zero offset in local
///   testing). So `align > WIN_ALLOCATION_GRANULARITY` is necessary to
///   exercise the over-reserve code path at all, but not sufficient to
///   guarantee a nonzero offset on any single attempt.
/// - **Unix**: `unix_reserve`'s over-reserve path computes
///   `base = align_up_addr(region_addr, align)` from whatever address the
///   kernel's `mmap(NULL, over, ...)` happens to hand back; the same
///   "already happens to be aligned" possibility applies.
///
/// Both platforms are therefore handled the same way: a bounded retry loop
/// that keeps every zero-offset reservation ALIVE (in `extras`) while
/// retrying, so a later successful (nonzero-offset) attempt cannot
/// accidentally reuse a just-released zero-offset address (a live
/// reservation's address space cannot be handed back to a fresh
/// `mmap`/`VirtualAlloc` call). The test then hard-asserts the premise
/// (`as_ptr() != reservation_ptr()`) immediately after obtaining the
/// reservation, so a regression in the over-reserve path fails loudly with
/// an explicit diagnostic instead of silently exercising the zero-offset
/// path under a misleading test name.
#[test]
// WJ-1: on Windows an eagerly committed span is one exact region (no head
// offset), so the over-reserved shape is reached through the lazy path
// (`commit_len < size`), which needs `lazy-commit`.
fn decommit_recommit_roundtrip_on_over_reserved_span() {
    // Excluded under miri: the fallback backend (src/os/miri.rs) is a plain
    // std::alloc model that returns (base, base, size) — it never
    // over-reserves, so a nonzero head offset is structurally impossible
    // there and this test's premise cannot hold (same class as the Darwin
    // 16 KiB-page exclusion documented in smoke.rs's module comment).
    // task #959 (second bug of the same class, found on the SAME macOS CI
    // run that exposed the `align` bug above -- masked by it until `align`
    // was fixed, since the retry loop below never used to terminate): `size`
    // is used as the `decommit`/`recommit` boundary at the bottom of this
    // test, both of which require multiples of the runtime `page_size()`,
    // not the compile-time `PAGE` constant (task #947/A-1). A bare `PAGE`
    // (4 KiB) fails that check unconditionally on 16 KiB-page hosts (Apple
    // Silicon). `page_size()` also satisfies `reserve_aligned`'s own
    // PAGE-multiple contract for `size` (it is always a power-of-two
    // multiple of `PAGE`), so this is safe on every host.
    #[cfg(not(windows))]
    let size = page_size();
    // Windows: two pages, so the lazy reserve below can commit one page first
    // (`initial_commit` must be non-zero and below `size` to stay off the
    // one-region path).
    #[cfg(windows)]
    let size = 2 * page_size();

    // Windows: align > WIN_ALLOCATION_GRANULARITY (64 KiB) unconditionally
    // skips win_reserve_commit's fast-reserve sub-path, and a lazy reserve
    // (commit_len < size) skips the one-region re-reserve too, forcing the
    // over-reserve two-call path (see smoke.rs's module doc). Unix has no
    // such fast-path shortcut to skip, so any align > size that forces an
    // over-reserve exercises the same code path; both still need the retry
    // loop below because `align_up_addr` can still land on a zero offset.
    //
    // task #959 (first real macOS CI run of this test): on Unix, `align`
    // must be a multiple of the RUNTIME `page_size()`, not the compile-time
    // `PAGE` constant. `mmap`'s own return addresses are granular to the
    // real OS page size, so aligning to exactly `4 * PAGE` (16 KiB) is
    // indistinguishable from the OS's own allocation granularity on a
    // 16 KiB-page host (Apple Silicon) -- every over-reserve base is then
    // ALREADY `align`-aligned by construction, deterministically, not
    // probabilistically, so the retry loop below exhausts its 1000-attempt
    // bound without ever observing a nonzero offset. Aligning to a multiple
    // of the runtime page size that's meaningfully larger than it (matching
    // the ratio this test already used on 4 KiB-page hosts) restores a real
    // ASLR-driven distribution on every currently-supported page size.
    #[cfg(windows)]
    let align = 128 * 1024; // 128 KiB, > the 64 KiB WIN_ALLOCATION_GRANULARITY
    #[cfg(not(windows))]
    let align = 4 * page_size();

    let mut extras = Vec::new();
    let r = loop {
        #[cfg(not(windows))]
        let candidate = reserve_aligned(size, align).expect("reserve with align > size");
        #[cfg(windows)]
        let candidate = {
            let mut lazy = aligned_vmem::reserve_aligned_lazy(size, align, page_size())
                .expect("lazy reserve with align > size");
            lazy.ensure_committed(size).expect("commit the span");
            lazy.into_reservation()
        };
        if candidate.as_ptr() != candidate.reservation_ptr() {
            break candidate;
        }
        // Zero-offset attempt: keep it alive (do not drop/release) so its
        // address space stays reserved and cannot be reused by the next
        // reserve call, then try again.
        extras.push(candidate);
        assert!(
            extras.len() < 1000,
            "over-reserved span offset stayed zero for 1000 consecutive attempts; \
             this indicates a genuine regression in the over-reserve path, not flakiness"
        );
    };
    // `extras`' reservations are dropped (released) here, after `r` has
    // already been chosen from a DIFFERENT, still-live address range.

    // Premise check: this test's whole point is a genuinely offset base. A
    // zero offset here would mean the rest of the test silently exercises
    // the already-covered zero-offset path under this test's name.
    assert!(
        r.as_ptr() != r.reservation_ptr(),
        "test premise: over-reserved span must have nonzero head offset"
    );
    let base = r.as_ptr();

    // Write/read at an offset within the span to confirm the base arithmetic works.
    // NOTE: `offset` is a WRITE/READ offset into the mapped span for this
    // test's own probing only -- it is never passed to `decommit`/`recommit`
    // below, which always operate on the whole `[0, size)` range from `base`.
    let offset = size / 2; // 2 KiB into the span (not page-aligned; irrelevant
                           // here since decommit/recommit below use [0, size))
                           // SAFETY: base is valid for `size` bytes; offset is within that range.
    unsafe {
        base.add(offset).write(0x88);
        assert_eq!(base.add(offset).read(), 0x88, "initial write/read works");

        // Decommit and recommit the whole span.
        aligned_vmem::decommit(base, 0, size);
        assert!(
            recommit(base, 0, size),
            "recommit of decommitted range on an over-reserved span succeeds"
        );

        // Verify zero-fill (same exclusions as decommit_recommit_roundtrip above).
        #[cfg(not(any(
            miri,
            aligned_vmem_mock,
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_os = "watchos"
        )))]
        assert_eq!(
            base.add(offset).read(),
            0,
            "recommitted page on over-reserved span must be zeroed"
        );

        // Write and read again to confirm the offset arithmetic still works.
        base.add(offset).write(0x99);
        assert_eq!(
            base.add(offset).read(),
            0x99,
            "post-recommit write/read works"
        );
    }
}
