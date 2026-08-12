# `aligned-vmem` — post-campaign closing review (fresh pass over the #842–#850 tree)

**Date:** 2026-08-12
**Scope:** `crates/vmem/` (package `aligned-vmem` 0.2.0) in full, read again as *fresh* code —
`src/{lib,error,mock,fault_injection}.rs`, `tests/` (7 files), `benches/vmem_bench.rs`,
`examples/v20_849_unix_exact_reserve_hit_rate.rs`, `Cargo.toml`, `README.md`, plus the
`aligned-vmem`-touching parts of `.github/workflows/ci.yml`.
**Reviewed tree:** `main` @ `9e8e52d` (task #850). `git status --short -- crates/vmem
docs/reviews` is empty at review start; the working tree's modifications are all in
`crates/region/` and root docs, none in scope.
**Toolchain:** `rustc 1.97.0` / `cargo 1.97.0`, Windows 10 Pro x86_64; `nightly-x86_64-pc-windows-msvc`
for the `--cfg docsrs` rustdoc check. Cross-target `cargo check` for
`x86_64-unknown-{linux-gnu,freebsd,netbsd}` (all installed locally).
**Nature:** read-only. Nothing was modified other than the creation of this document. No
`git add` / `git commit`. Every command quoted below was actually run on this host.

**Relationship to the prior review.** `docs/reviews/2026-08-12-aligned-vmem-code-quality-review.md`
(findings V1–V21) drove tasks #842–#850. This pass does **not** re-report V1–V21. It re-verifies
the claimed fixes where something looked suspicious, and reports what is *new* — including
several defects the fix campaign itself introduced. Findings are numbered `W1…W16` to keep them
unambiguous against the `V`-series.

**Platform honesty up front.** This host is Windows/x86-64, 4 KiB page. W2 and W9 are
**REASONED-FROM-SPEC + code-reading** on non-Linux-Unix / large-page hosts that do not exist here
or in this project's CI; each says so in its own body. W1, W5 (the doc half), and the whole
"verified green" table are **executed**, with receipts.

---

## Verdict up front

**One HIGH: `main` is red under `cfg(miri)` right now.** `cargo +nightly miri test -p aligned-vmem
--all-features` — the exact command `docs/CORRECTNESS_OPEN_ITEMS.md` item 41 names as the one this
crate owes — does not compile. Neither does the root crate under
`cargo miri test --features "production exact-span-large large-reserved-capacity internals"`.
Two `E0308` tuple-arity errors at `crates/vmem/src/lib.rs:2239` and `:2250`, introduced by
`84ca221` (task #844) — the commit whose own message describes fixing a *different* instance of
exactly this bug class — and live through the six commits since. This is the **third** instance in
this campaign of "verification ran in one configuration, the break lives in another": #843 broke
Unix (fixed by `b228e69` after five commits), #848 broke the lazy-commit reservation length
(caught in zero-trust review before landing), #844 broke miri (still open).

**Two more real defects, both in the observable the campaign added to make huge-pages falsifiable.**
`Reservation::is_huge()` returns `true` for an ordinary-page reservation on every non-Linux Unix
(W2), and — in the opposite direction — the `is_huge()`/`MEM_LARGE_PAGES` documentation's flat
"always `false` on Windows" is now wrong for `align <= 64 KiB`, because #848's single-call path
happens to issue exactly the `MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES` shape V3 said was
required (W3). #848 partially fixed V3 without noticing, and #843's documentation was never
revisited.

**The structural cause of all three is one CI gap (W14):** nothing in CI compiles this crate under
`cfg(miri)`, on macOS, or on Windows. `aligned-vmem-gates` (`ci.yml:128-150`) and `test-workspace`
(`:782-810`) are both `ubuntu-latest`; `test-windows`/`test-macos` run the *root* crate only;
`feature-powerset` is root-scoped. A one-line, no-nightly `RUSTFLAGS="--cfg miri" cargo check -p
aligned-vmem --all-features` added to `aligned-vmem-gates` costs seconds and would have caught W1
on the commit that introduced it.

**Publish posture.** W1 should be fixed before the 0.2.0 publish — it is a compile error in a
configuration this crate advertises support for ("miri-friendly" is the third clause of the
crates.io description). W2 and W3 make the crate's own documented capability signal report the
wrong answer on two of three platform families; both are `0.2.1`-able, but W3 in particular is a
*shipping README + crates.io description* claim. W11 (the `bench-internals` `pub static`s) is the
only remaining item that is genuinely breaking to fix after publish. Nothing found here is a
soundness hole, and no `unsafe` block's proof was found wanting.

---

## What was verified green (so the negatives below are read in context)

| command | result |
|---|---|
| `cargo test -p aligned-vmem --all-features` | **green** — 34 tests (smoke 18, lazy_commit 9, mock 8, huge_pages 1, min_page 2, vmemerror_io_bridge 3, fault_injection 0 by its own `not(mock)` gate) |
| `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` | **green**, exit 0 |
| `cargo clippy -p aligned-vmem --all-targets -- -D warnings` (default row) | **green**, exit 0 |
| `cargo fmt -p aligned-vmem --check` | **green** |
| `RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc -p aligned-vmem --no-deps --features "lazy-commit huge-pages fault-injection"` | **green, zero output** — the `#![cfg_attr(docsrs, feature(doc_cfg))]` opt-in at `lib.rs:83` is genuinely correct (see "not findings") |
| `cargo check -p aligned-vmem --target x86_64-unknown-{linux-gnu,freebsd,netbsd}`, default **and** `lazy-commit,huge-pages,mock,fault-injection,bench-internals` | **6/6 green** — `b228e69`'s Unix break has not recurred |
| `cargo package --list -p aligned-vmem --allow-dirty` | 19 entries; README, both LICENSEs, the bench and the new example all included |
| `RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem` (no features) | **green** |
| `RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem --features lazy-commit` / `--features huge-pages` | **RED — see W1** |

---

# Findings

## Category 1 — bugs

### W1 — HIGH — the miri backend does not compile with `lazy-commit` or with `huge-pages`; `cargo miri test -p aligned-vmem --all-features` and the root crate's `large-reserved-capacity` miri path are both broken today

`crates/vmem/src/lib.rs:2239` and `:2250` (the two `.map()` closures), against
`crates/vmem/src/lib.rs:2183-2186` (miri's `reserve_aligned_raw`, a **3**-tuple).

`84ca221` (task #844) correctly reverted miri's `reserve_aligned_raw` from the 4-tuple #843 gave
it back to the 3-tuple shape the Windows/Unix backends use — but did not update its two
**feature-gated** callers, which still destructure four elements:

```rust
// lib.rs:2234-2242, #[cfg(all(miri, feature = "lazy-commit"))]
reserve_aligned_raw(size, align).map(|(base, reservation, reservation_len, _granted_huge)| {
    (base, reservation, reservation_len)
})
// lib.rs:2244-2253, #[cfg(all(miri, feature = "huge-pages"))]  — same shape
```

**Receipt (executed on this host):**

```
$ RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem --features lazy-commit,huge-pages
error[E0308]: mismatched types
    --> crates\vmem\src\lib.rs:2239:43
     = note: expected tuple `(NonNull<u8>, NonNull<u8>, usize)`
                found tuple `(_, _, _, _)`
error[E0308]: mismatched types
    --> crates\vmem\src\lib.rs:2250:43
error: could not compile `aligned-vmem` (lib) due to 2 previous errors
```

Full matrix under `--cfg miri`: `[]` OK, `[mock]` OK, `[fault-injection]` OK, `[bench-internals]`
OK, **`[lazy-commit]` FAIL**, **`[huge-pages]` FAIL**, `[all]` FAIL (2 errors).

**Reachable from the root crate too, not only from `-p aligned-vmem`** — `Cargo.toml:371`
(`large-reserved-capacity = ["exact-span-large", "aligned-vmem/lazy-commit"]`), `:579`
(`bench-internals = ["aligned-vmem?/bench-internals", "aligned-vmem?/lazy-commit"]`), `:776`/`:796`
(`alloc-lazy-commit` and its sibling bundles). Receipt:

```
$ RUSTFLAGS="--cfg miri" cargo check -p sefer-alloc \
    --features "production exact-span-large large-reserved-capacity internals"
error[E0308]: mismatched types
    --> crates\vmem\src\lib.rs:2239:43
error: could not compile `aligned-vmem` (lib) due to 1 previous error
```

That is the feature whose *entire documented purpose* is the deferred-commit
`aligned_vmem::commit_range` path (`Cargo.toml:339-362`) — the one a maintainer would most
plausibly reach for miri on.

**Why it survived:** #844's own verification line reads `cargo +nightly miri test -p aligned-vmem
--test smoke` — **default features**, so neither gated function was compiled. `cargo test
--all-features` (which #844 also ran, 34 green) never sets `cfg(miri)`. And `#844` explicitly
decided *not* to add a miri CI row for this package. Six commits later the break is still here.

**Consequence for the open-items index.** `docs/CORRECTNESS_OPEN_ITEMS.md:1846-1847` states, as
item 41's live Status card, that the `leak_zeroed_pages` intentional leak "is the only remaining
blocker" for a `cargo miri test -p aligned-vmem` CI step. That is now false — there are two
compile errors in front of it. Per CLAUDE.md's own "OPEN_ITEMS indexes are CURRENT-STATE"
convention, the card needs updating in the same commit that fixes this.

**Fix:** delete the `_granted_huge` element from both patterns (2 characters' worth of change
each), and add the CI guard from W14.

### W2 — MEDIUM — `Reservation::is_huge()` returns `true` for an ordinary-page reservation on every non-Linux Unix, contradicting its own documented contract

`crates/vmem/src/lib.rs:1863` (`Ok((base, base, size, huge))`), `:1758` (`granted_huge = huge;`),
`:2097-2107` (`libc_mmap`), doc at `:430-448` and `:1166-1167`.

`libc_mmap`'s `huge` parameter is consumed by exactly one conditional:

```rust
// lib.rs:2103-2107
#[cfg(all(target_os = "linux", feature = "huge-pages"))]
if huge { flags |= MAP_HUGETLB; }
let _ = huge; // silence unused on non-linux / no huge-pages builds
```

On macOS / iOS / tvOS / watchOS / FreeBSD / NetBSD / OpenBSD / DragonFly the parameter is
*discarded* — the crate's own rustdoc says so plainly (`:1166-1167`: "Currently a **no-op on macOS
and other non-Linux Unix** — it falls back to an ordinary reservation"). But both Unix return
paths report the *request*, not the *grant*:

* fast path, `:1863`: `Ok((base, base, size, huge))` — `huge` is the argument, unconditionally.
* over-reserve path, `:1758`: `granted_huge = huge;` on the branch where the first `mmap`
  succeeded — and on non-Linux that first `mmap` was never a huge-page request at all, so it
  always succeeds and the fallback branch at `:1744-1753` (which correctly sets `false`) is
  unreachable.

**Failure scenario (concrete):** on Apple Silicon macOS with `huge-pages` enabled, an arena calls
`reserve_aligned_huge(2 * MIB, 2 * MIB)`. It gets ordinary 16 KiB pages. `r.is_huge()` returns
`true`. The caller — following this crate's own `decommit` rustdoc (`:863-872`: "on both Windows
and Linux, decommit **does not work** on huge-page reservations … Use `reserve_aligned` instead if
you need decommit functionality") — therefore skips its RSS-reclamation `decommit` pass entirely,
permanently retaining resident memory it could have returned to the OS, on a reservation where
`madvise(MADV_DONTNEED)` would have worked perfectly.

This defeats the exact purpose the flag was added for. V3's fix options said an
`is_huge()`/`granted_huge()` signal "is what turns *best-effort* from unfalsifiable into testable";
on five-plus targets it is now falsifiably wrong.

REASONED-FROM-SPEC + code-reading: no macOS/BSD host is available here or in this project's CI for
this crate (see W14). The reasoning needs no host — `libc_mmap` demonstrably ignores its own
`huge` argument off Linux, and both call sites demonstrably return that same argument.

**Fix:** have `libc_mmap` report what it actually requested (e.g. return `(ptr, requested_huge)`,
or introduce a `const HUGE_SUPPORTED: bool` that is `false` off Linux) and thread that — not the
caller's wish — into `granted_huge`. One line at each of `:1758` and `:1863`.

### W3 — MEDIUM — the Windows large-page documentation is now wrong in the *opposite* direction: #848's single-call path issues exactly the combined call V3 said was required, so `is_huge()` **can** be `true` on Windows for `align <= 64 KiB` — while four doc sites still promise it never is

`crates/vmem/src/lib.rs:1344-1387` (the single-call fast path), `:1559` (`reserve_aligned_huge_raw`
passing `MEM_LARGE_PAGES` into it), versus `:357-364` (the `granted_huge` field doc), `:440-443`
(`is_huge`'s rustdoc), `:1191-1198` (`reserve_aligned_huge`'s rustdoc), `:1553-1558` (the code
comment inside `reserve_aligned_huge_raw`), and `README.md:58-60`.

V3's diagnosis was: `MEM_LARGE_PAGES` must be specified together with `MEM_RESERVE | MEM_COMMIT` in
a **single** call, and this crate splits reserve from commit, so it can never engage. #843's
resolution was documentation-only ("on Windows, this flag will always be `false`"). #848 then added,
for a completely unrelated perf reason, precisely this:

```rust
// lib.rs:1351-1356, taken when `align <= WIN_ALLOCATION_GRANULARITY && commit_len == size`
let p = VirtualAlloc(
    core::ptr::null_mut(),
    commit_len,
    MEM_RESERVE | MEM_COMMIT | extra_commit_flags,   // extra_commit_flags == MEM_LARGE_PAGES here
    PAGE_READWRITE,
);
…
Ok((base, base, commit_len, extra_commit_flags != 0))   // :1387 — granted_huge = true
```

**Failure scenario (concrete):** a Windows process holding `SeLockMemoryPrivilege` calls
`reserve_aligned_huge(2 * MIB, PAGE)` — `align = 4 KiB ≤ 64 KiB`, `commit_len == size`, `size` a
multiple of `GetLargePageMinimum()`. The single combined call is well-formed per the Win32
contract, succeeds with genuine large pages, and `is_huge()` returns `true`. The published rustdoc
the caller read says it "always returns `false`" and that the implementation "is not functional in
the current release". A caller who believed that doc and hard-coded the ordinary-page assumption
(e.g. proceeding to `decommit` the range, which large-page regions reject) gets the wrong
behaviour with no signal.

The **inversion** is the part worth naming: the *natural* huge-page call —
`reserve_aligned_huge(2 * MIB, 2 * MIB)`, align equal to the large-page size — has `align > 64 KiB`
and therefore takes the unchanged two-call path, where `MEM_LARGE_PAGES` on a commit-only call is
still malformed and still always falls back. So on Windows today, large pages are obtainable
**only if you ask for small alignment**, which is the opposite of what any caller would try. There
is also no `GetLargePageMinimum()` validation anywhere (V3's fix option (a) named it; only the
Linux `LINUX_HUGE_PAGE_SIZE` guard at `:1718-1724` was ever implemented).

REASONED-FROM-SPEC: this host has no `SeLockMemoryPrivilege`, so the success branch cannot be
exercised locally — the same limitation V3 recorded.

**Fix (either is defensible, but the current text cannot stand):** (a) route the huge path through
the single-call shape unconditionally (drop the `align <= 64 KiB` restriction *for the large-page
attempt only*, falling back to the two-call ordinary path on failure) and validate `size`/`align`
against `GetLargePageMinimum()` — this actually delivers V3's capability; or (b) keep the code and
correct all four doc sites plus the README to state the real, narrow condition
(`align <= 64 KiB` + privileged + `size` a `GetLargePageMinimum()` multiple).

### W4 — MEDIUM — #848 shipped a runtime perf change with no path-activation oracle, and silently invalidated the one counter that would have been it

`crates/vmem/src/lib.rs:207-217` (the counter's rustdoc), `:173-180` (the module-level
`bench-internals` design comment), `:1326` (the in-source perf claim), and the four increment
sites `:1373`, `:1384`, `:1455`, `:1472`.

`WINDOWS_RESERVE_COMMIT_CALLS`'s own rustdoc says, verbatim:

> total number of `win_reserve_commit` calls … **Each call issues exactly 2 syscalls**
> (`VirtualAlloc(MEM_RESERVE)` + `VirtualAlloc(MEM_COMMIT)` …)

and the module comment at `:176-180` adds "there is no fast/slow-path split to measure on Windows
today (that is exactly what step 3, a `VirtualAlloc2` prototype, would introduce)". After #848 the
single-call path issues **one** syscall and increments the **same** counter (`:1384`), so the
counter now sums two different call shapes with no way to separate them — and the comment
asserting no split exists is describing code that no longer exists. Anyone re-running
`docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md`'s methodology against current HEAD
gets a number whose units silently changed.

Compounding it, `:1321-1326` states the benefit as fact in the shipped source:

> This saves ~4.6 µs of a ~13.7 µs reserve+commit pair (~33% reduction).

Those figures are R32-13's measurements of the **old** two-call code. Nothing measured the new
code. The crate's own bench (`benches/vmem_bench.rs:41-47`) uses `RESERVE_SIZE = RESERVE_ALIGN =
64 KiB` — exactly on the fast path's boundary, i.e. the arm that *would* measure it — and was not
run for this. Per CLAUDE.md's R30-8 rule (a judge must report per-arm evidence the arm took the
code path it claims to measure) and R30-12 (`perf(...)` is for changes that alter what ships,
which this one is), #848 is a `perf`-class commit with an unverified headline.

**Failure scenario:** a future round reads `WINDOWS_RESERVE_COMMIT_CALLS` as its
path-activation oracle — the role `R32_13_…_GATE.md` already assigned it and CLAUDE.md's
benchmark-hook rule endorses — and computes syscalls as `2 × calls`. On any workload with
`align <= 64 KiB` that is wrong by up to 2×, in the direction that flatters the change.

**Fix:** split into two counters (`WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` /
`…_TWO_CALL_PAIRS`, or add a `WINDOWS_SINGLE_CALL_RESERVES` hit counter beside the existing
total), correct `:207-217` and `:173-180`, and either measure the 64 KiB bench arm A/B or demote
`:1321-1326`'s claim to an explicitly-labelled hypothesis inherited from R32-13.

### W5 — MEDIUM — V1's fix changed the Unix reservation strategy, and six doc sites — including the **crates.io package description** — still describe the removed trim; this is the third drift of the same sentence

`Cargo.toml:7`, `README.md:40`, `crates/vmem/src/lib.rs:24`, `:416`, `:526`, `:538`, `:664-665`,
against the code at `:1786-1808`.

Task #842 adopted V1's fix option 3 — keep the whole `over` mapping, never trim — and the code says
so explicitly at `:1786-1791` ("Keep the entire over-reserve mapping as the reservation, exactly as
the Windows backend does. This removes the `munmap` trim calls…"). Six pieces of prose still
describe the old behaviour:

| site | current text | reality after #842 |
|---|---|---|
| `Cargo.toml:7` (crates.io description) | "exact-size mmap fast path on Unix, **over-reserve on Windows**" | Unix over-reserves too, on every fast-path miss |
| `README.md:40` | same sentence | same |
| `lib.rs:664-665` (`reserve_aligned` rustdoc) | "over-reserve + **trim** fallback on an alignment miss; Windows always over-reserves and never trims" | Unix never trims either; the asymmetry it draws no longer exists |
| `lib.rs:24` (module doc) | "via the classic over-reserve + **trim** technique" | no trim on any platform |
| `lib.rs:416` (`reservation_ptr`) | "due to the over-reserve + **trim** technique" | over-reserve only |
| `lib.rs:526`, `:538` (`from_raw_parts`) | "the over-reserve + **trim** technique this crate itself uses internally" | same |

This exact sentence has now drifted **three times**: `CHANGELOG.md:131` records task #640 fixing an
"over-reserve + trim" overclaim in the crate description, then task #650 fixing "two more copies of
the same over-reserve overclaim (in `reserve_aligned`'s own rustdoc and the README table)" that
#640 missed — and #842 has now flipped the truth back the other way, leaving all of them wrong
again in the opposite direction. It is worth a one-line grep guard (`grep -n 'trim' crates/vmem/`)
in whatever gate reviews this crate, rather than a fourth hand-pass.

**The substantive half, not just wording:** the change is a real, user-visible resource trade the
docs no longer mention anywhere. With #849's own measured Unix hit rates (`35d51e6`: 34.4% at
64 KiB, 46.7% at 1 MiB, 56.7% at 4 MiB), a Linux consumer now holds `size + align` bytes of VA for
roughly **43-66% of its reservations**, permanently, versus `size` before — i.e. ~1.43× expected VA
per 4 MiB segment, ~1.66× at 64 KiB. On 64-bit that is cheap; under `vm.overcommit_memory=2`
(strict accounting) it is a proportional increase in commit charge, and the crate's own README
pitch is "hand pages back to the OS, keep the address reservation". Nobody measured the delta and
no doc states it. At minimum `reserve_aligned`'s rustdoc and the README's "What it does" table
should say that a Unix fast-path miss retains `size + align`.

---

## Category 2 — code smells and campaign residue

### W6 — LOW — the V14/V15 refactor orphaned two doc comments onto the wrong items, and left one helper with no doc at all

`crates/vmem/src/lib.rs:678-684` and `:702-705`.

```rust
// :678-684
/// Fallible [`reserve_aligned`]: returns a [`VmemError`] carrying the OS cause
/// (`errno` / `GetLastError`) on failure instead of a bare `None`.
///
/// A contract violation (bad `size`/`align`) returns
/// [`VmemError::invalid_argument`] without touching the OS.
/// Private helper: validate `size`/`align` for reservation.
fn validate_size_align(size: usize, align: usize) -> Result<(), VmemError> {
```

The first four lines belong to `try_reserve_aligned`, which was moved down to `:752` and correctly
grew its own copy — the original was left stranded on top of the new private helper. Same shape at
`:702-705`: `/// Private helper: finish a reservation from a raw backend result.` now sits on
`struct RawReservation`, while `fn finish_reservation` at `:716` has no doc comment at all.

Not user-visible (both items are private, so `#![deny(missing_docs)]` does not fire), but it is
exactly the artefact a mechanical extract-function refactor leaves, and it makes the next reader
believe `validate_size_align` is the public fallible entry point.

### W7 — LOW — V15's `RawReservation` does not actually remove the transposition hazard it was created for

`crates/vmem/src/lib.rs:705-750`, against `:1291-1298`, `:1315-1320`, `:1538-1546`, `:1549-1560`,
`:1656-1663`, `:1713-1717`, `:1827-1831`, `:2183-2186`, `:2234-2242`, `:2245-2253`.

V15's finding was that *nine backend functions* return an unnamed
`(NonNull<u8>, NonNull<u8>, usize)` whose first two elements are the same type and are easy to
transpose. `RawReservation` was added — but **every one of those nine signatures still returns the
bare tuple**. The struct is built by `.map()`-ing *from* a tuple at exactly two call sites (`:774`
and `:1151`), i.e. one line further along the same data path; the transposition window (backend
`Ok((a, b, c))` construction sites) is untouched. `finish_reservation_huge` (`:735-750`) still
takes a bare `(NonNull<u8>, NonNull<u8>, usize, bool)` 4-tuple, so the huge path never sees the
named struct at all, and the crate now has two parallel "finish" helpers with different parameter
shapes.

This is not a regression — the code is exactly as safe as before V15 — but the finding V15 raised
is still open, and a future reader will reasonably assume it was closed. Either push
`RawReservation` down into the backend return types (the real fix, ~9 signatures) or record in
`RawReservation`'s own doc that it is a call-site convenience, not the hazard's elimination.

### W8 — LOW — `page_size()`'s own rustdoc still carries the exact "silently do partial work" wording V2 corrected, and is now stale twice over

`crates/vmem/src/lib.rs:268-271`, against the module doc at `:76-79` and the code at `:874`/`:908`.

The module doc was corrected by #842 to the accurate all-or-nothing statement ("`madvise(2)`
rejects the entire call (all-or-nothing) when `addr` is not a multiple of the real page size").
The identical sentence one screen down was not:

```rust
// :268-271
/// **Correctness:** on Apple Silicon macOS the page size is 16 KiB, and on some
/// Linux configurations 64 KiB. A caller that decommits at 4 KiB-but-not-page
/// multiples would silently do partial work; use this value (not [`PAGE`]) to
/// round decommit offsets.
```

It is now wrong for a *second* reason as well: since #842, `decommit`/`decommit_lazy` reject such a
range in the crate itself (`:874`, `:908`) and never reach `madvise` at all — so the behaviour is
neither "partial" nor an OS-level rejection, it is a crate-level silent skip. This is the
publicly-rendered doc of the very function callers are told to use for rounding.

### W9 — LOW — `tests/mock.rs::records_reserve_and_decommit` will fail on any 16 KiB- or 64 KiB-page host, as a direct consequence of the V2 fix

`crates/vmem/tests/mock.rs:18-23`, against `crates/vmem/src/lib.rs:874`, `:908`.

```rust
unsafe {
    decommit(base, 0, PAGE);              // PAGE == 4096
    decommit_lazy(base, PAGE, 2 * PAGE);
}
let calls = mock::drain();
assert_eq!(calls.len(), 3, "reserve + decommit + decommit_lazy");
```

Both entry points now validate against `page_size()` **before** recording the mock call. On Apple
Silicon macOS (`page_size() == 16384`) `4096.is_multiple_of(16384)` is false, so both calls return
at `:874`/`:908` without recording, `calls.len()` is `1`, and the test fails. The `mock` feature's
whole selling point is "deterministically test your OOM-handling **on any target**" — this is the
crate's own mock suite failing to be target-independent.

Not caught by CI: `aligned-vmem-gates` and `test-workspace` are both `ubuntu-latest` (4 KiB), and
`test-macos` runs the root crate only (W14).

**Fix:** use `page_size()` (already `pub`) instead of `PAGE` for the offsets in this test — a
two-token change that also documents the new contract by example.

### W10 — LOW — the README's alignment-contract bullet still says `decommit` offsets must be multiples of `PAGE`, and its API table omits every API the campaign added

`crates/vmem/README.md:93`, `:38-49`.

* `:93` — "`decommit`/`recommit`/`commit_range` offsets must be multiples of `PAGE`." Since #842,
  `decommit`/`decommit_lazy` require multiples of **`page_size()`**; only `recommit`/`commit_range`
  still validate against `PAGE` (`lib.rs:963`, `:1052`). The README states the looser rule for the
  one pair where it is now wrong, and does not mention that the crate's two decommit entry points
  now use a *different* granularity from its two commit entry points — an asymmetry worth one
  sentence, since a caller pairing `decommit(base, PAGE, 2*PAGE)` with `recommit(base, PAGE,
  2*PAGE)` on a 16 KiB host gets "nothing decommitted" + "`true`" + stale data, which is the same
  observable V2 was about.
* `:38-49` — the "What it does" table has no row for `MIN_PAGE` (`lib.rs:154`),
  `Reservation::into_reservation_parts` / `ReservationParts` / `release_parts` (`:484`, `:638`,
  `:825`), `Reservation::is_huge` (`:446`, mentioned only in the feature prose at `:60-61`), or
  `impl From<VmemError> for std::io::Error` (`error.rs:138`). Five of the campaign's nine tasks
  added public API; the README's own summary of the public API records none of it.

---

## Category 3 — API surface and publish readiness

### W11 — LOW-MEDIUM — `bench-internals` commits three `pub static AtomicU64` to the permanent 0.2 API, contradicting the Cargo.toml comment that says they follow sefer-alloc's `#[doc(hidden)]` convention

`crates/vmem/src/lib.rs:196`, `:205`, `:217` (the statics) plus `:224`, `:233`, `:242`, `:254`
(the accessors); `crates/vmem/Cargo.toml:98-109`.

The feature's own Cargo.toml doc says it "Mirrors sefer-alloc's own `bench-internals` convention
(`AtomicU64` storage, always compiled; **`#[doc(hidden)]` accessors**)". It does not: all seven
items are plain `pub` with `#[cfg_attr(docsrs, doc(cfg(...)))]` badges, i.e. deliberately
*documented* public API. Two consequences:

1. **Semver.** A `pub static` is API surface; removing or renaming any of the three after publish
   is breaking. Marking them `#[doc(hidden)]` now costs one attribute per item and preserves every
   in-repo use (`Cargo.toml:579` forwards the feature; the root crate re-exports the accessors).
   W4 already argues at least one of them needs to be *replaced*, which is precisely the change
   that becomes breaking the moment 0.2.0 is published.
2. **Correctness of the instrument.** Because the statics are `pub`, downstream code can
   `UNIX_EXACT_RESERVE_HITS.store(999, Relaxed)`. The accessor pair (`unix_exact_reserve_hits()` /
   `…_attempts()`) plus `reset_bench_internals_counters()` is already the complete intended
   surface; exposing the raw cells adds nothing and lets a consumer corrupt a measurement window
   that the crate's own examples and this project's perf gates read.

The `docs.rs` metadata (`Cargo.toml:27`) deliberately excludes `bench-internals`, so these items
never render on the published docs page anyway — the badges on them are decorative. That is a
strong hint they were meant to be hidden.

### W12 — INFO — `ReservationParts` has no public constructor, so the self-hosting pattern V8 was written for still has to use the raw 3-tuple

`crates/vmem/src/lib.rs:636-657`, `:484-498`, `:825-833`.

`#[non_exhaustive] pub struct ReservationParts` can be produced only by
`Reservation::into_reservation_parts()`. The documented motivating use case is
"your allocator records the reservation in its own **self-hosted metadata**" (`:471-482`) — and an
allocator that stores `(ptr, len, align)` as three words in its own segment header (which is
exactly what `src/alloc_core/alloc_core.rs:208` does for the in-repo consumer) cannot rebuild a
`ReservationParts` to hand to `release_parts`. It must call the old `release(raw, raw_len,
raw_align)` — the precise 3-tuple whose argument transposition V8 exists to prevent. The struct
therefore only helps callers who keep the struct itself alive, which is the case that was already
served by keeping the `Reservation`.

Non-blocking (adding `ReservationParts::new` later is additive), but worth recording so a future
reader does not conclude V8 closed the hazard for the pattern it names.

### W13 — INFO — `decommit`/`decommit_lazy` are still the only entry points with no fallible form; V2's fix moved the silent no-op from the kernel into the crate without changing what the caller can observe

`crates/vmem/src/lib.rs:873-889`, `:907-923`.

Both return `()` and both `return` silently when the range fails the `page_size()` predicate. From
the caller's seat, the V2 failure scenario is unchanged: on a 16 KiB-page host,
`decommit(base, PAGE, 2*PAGE)` still does nothing, still reports nothing, `recommit` still returns
`true`, and the range still holds its old bytes. What changed is that the silence is now the
crate's documented choice rather than a discarded `EINVAL` — which is a genuine improvement in
*honesty*, and is exactly the option V2 offered ("a silent no-op is already the documented
behaviour for a violated range"), but it is not a behavioural fix and the campaign's own record
should not read as though it were.

`README.md:99-107` argues the asymmetry is intentional ("`decommit`'s `()` return has no
write-permitting sentinel to misuse"). That argument is sound for *safety* and does not cover
*silent data staleness*. Adding `try_decommit(base, start, end) -> Result<(), VmemError>` beside
the infallible form — the same `try_*` pairing every other entry point already has — is additive
and would let a caller detect the rejection. Deliberately filed as INFO, not a defect: it is a
design choice, and it is non-breaking to revisit after publish.

---

## Category 4 — CI and process coverage

### W14 — MEDIUM — nothing in CI compiles this crate under `cfg(miri)`, on macOS, or on Windows; W1 and W2 both live in exactly those blind spots, and the crate's headline claims cover all three

`.github/workflows/ci.yml:128-150` (`aligned-vmem-gates`, `runs-on: ubuntu-latest`), `:782-810` +
`:852` + `:872` (`test-workspace`, `runs-on: ubuntu-latest`), `:732-759` (`test-windows` — root
crate only), `:761-780` (`test-macos` — root crate only), `:1066-1270` (every `miri-*` job — root
crate only), `:1939-1990` (`feature-powerset` — `cargo hack check` with no `-p`, so root crate
only).

Three gaps, each mapping to a finding above:

1. **`cfg(miri)` is never compiled for this package in any configuration.** → W1 shipped and
   survived six commits. The cheapest possible guard needs no nightly and no miri interpreter:
   `RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem --all-features` is a plain `cargo check`
   that runs in seconds and fails today. (A genuine `cargo miri test -p aligned-vmem` row is the
   better answer and is already filed as `docs/CORRECTNESS_OPEN_ITEMS.md` item 41, still blocked on
   the `leak_zeroed_pages` intentional leak — but the compile-only guard is available immediately
   and independently.)
2. **No macOS/aarch64 row for this package.** → W2's false `is_huge()` and W9's page-size-dependent
   test failure are both invisible. `test-macos` exists (`:761`) and could gain one
   `cargo test -p aligned-vmem --all-features` step; that alone would turn W9 red. Note the
   *Windows* gap has the same shape but the opposite cost — the Windows backend is the one with
   the newest code (#848's single-call path) and `test-windows` runs the root crate only, so
   `win_reserve_commit`'s new branch has no CI execution anywhere.
3. **`feature-powerset` never sweeps this crate.** The crate has 6 features (5 real + 1 alias); a
   depth-2 powerset is ~22 `cargo check` invocations — trivially affordable even per-PR, and it
   is the class of sweep that catches "compiles under `--all-features` and under default, fails
   under one specific pair", which is what W1 is under `cfg(miri)`.

The positive half, verified: the two test files added by #850 (`tests/min_page.rs`,
`tests/vmemerror_io_bridge.rs`) carry no `#![cfg(...)]` gate and therefore **do** run in both
`cargo test -p aligned-vmem --all-features` invocations (`:150` and `:852`) — 2 + 3 tests,
confirmed in this review's own run. The example added by #849 **is** compiled by
`cargo clippy -p aligned-vmem --all-features --all-targets` (`:147`), because `--all-features`
satisfies its `required-features = ["bench-internals"]`; it is correctly *skipped* by the
default-features clippy row (`:144`) rather than failing it, which is exactly the hazard #849's
own commit message says it was guarding against. Both #850 additions and the #849 example are
genuinely covered.

### W15 — LOW — #849's measurement has no report, no raw log, no summary CSV, no immutable source identity, and no open-items entry — although its numbers are the explicit gate for the follow-up work

`35d51e6`'s commit body (the only place the numbers exist), against CLAUDE.md's R22-14 boundary
rule and `docs/perf/OPEN_ITEMS.md`.

#849 published four measured figures (480/480, 165/480, 224/480, 272/480 across four align
regimes), used them to overturn the prior review's hand-derived ~0.1% prediction by three orders
of magnitude, and stated the result gates whether V20/P17's guards get implemented. CLAUDE.md's
R22-14 rule says explicitly that *"any report whose verdict … rests on measured numbers owes raw
logs + a summary CSV, regardless of whether the measurement came from criterion/iai, a
process-level judge, **or an ad-hoc probe built for a single one-off question**"* — and that the
test is "does the verdict rest on a number obtained by running something", not the measurement's
pedigree. This one does. What exists: a committed, reproducible probe
(`examples/v20_849_unix_exact_reserve_hit_rate.rs` — good, and better than R21-2's original
position). What does not: any `docs/perf/*.md`, any `_raw_*.log`, any `*_summary.csv`, and any
record of the measurement identity (the WSL2 kernel/build it ran on).

Compounding it, the commit's own honest caveat — "Measured on WSL2 … should not be treated as
definitive … without a bare-metal re-measurement" — is recorded in **no index**.
`docs/perf/OPEN_ITEMS.md` has zero `vmem` entries. Per CLAUDE.md's "Round start: check BOTH
open-items indexes" convention, a fresh session inherits no memory of this; the in-session TaskList
(#849, now `completed`) does not survive a session boundary. The bare-metal remeasure is exactly
the kind of item that hung for four rounds in the R18-8 incident that motivated the rule.

**Fix:** a short `docs/perf/R_V20_849_UNIX_EXACT_RESERVE_HIT_RATE.md` carrying the 30-run
aggregate table, the exact reproduction command, the WSL2 identity, and a
`…_summary.csv`; plus one `docs/perf/OPEN_ITEMS.md` card for the bare-metal remeasure with a
Status/Next-trigger block.

### W16 — LOW — the #842-#850 campaign has no `CHANGELOG.md` entry and no post-work tasks, unlike every prior sub-crate round in this same sweep

`CHANGELOG.md` (no hit for `#842`…`#850`), against `CHANGELOG.md:236-259` (the previous
`aligned-vmem` rounds, both with full per-task bullets) and the TaskList shape of every sibling
crate's round (`#744-747` aligned-vmem, `#748-751` numa-shim, `#752-755` size-classes, `#829-832`
sefer-region — each round carried explicit checkpoint / CHANGELOG / commit-docs / closing-review
post-work tasks).

Nine tasks landed nine commits changing shipping code, adding public API, and changing a runtime
behaviour on both platform backends. None of that is in the CHANGELOG, so the record of what 0.2.0
actually contains — which is the document a `cargo publish` reader reaches for — is missing the
most recent and most substantive third of it. This review is the campaign's closing review; the
CHANGELOG entry and a `/checkpoint` are the two remaining post-work steps the pattern calls for.

---

## Category 5 — performance

Null result first, unchanged from the prior review and re-confirmed here: **there is no CPU-bound
hot path in this crate.** Every public entry point is one syscall deep; `page_size()` is a single
relaxed load after the first call; the counters are compiled out by default. The only levers are
syscall count and address space.

### P-A — V20/P17's follow-up: the measurement says **do nothing**, and that conclusion is worth recording rather than leaving implicit

#849's aggregate (34.4% at 64 KiB, 46.7% at 1 MiB, 56.7% at 4 MiB) straddles the ~50% break-even
the counter comment at `lib.rs:164-172` derives. Reading it honestly:

* The **`align > threshold` skip** V20 proposed is **not** supported. At the flagship 4 MiB regime
  the hit rate is 56.7% — *above* break-even, so skipping the fast path there would make things
  worse, not better. The 64 KiB regime (34.4%) is below break-even, but that inverts the proposed
  rule (skip the fast path for *small* aligns, keep it for large), which is both counterintuitive
  and, at these sample sizes and on WSL2, not a number to hard-code a dispatch threshold from.
  **Correct action: leave the code alone**, exactly as the campaign did.
* The **one free, zero-risk change** the data supports unconditionally is V20's other guard, which
  needs no measurement at all: for `align <= page_size()` the alignment check at `lib.rs:1849`
  (`!region_addr.is_multiple_of(align)`) is **provably** false — a kernel `mmap` result is always
  page-aligned — so the fast path can skip it and the miss branch is statically unreachable. It
  saves no syscall (the 100% arm confirms it: 480/480), but it removes a branch and, more usefully,
  documents the invariant in code. Low value, near-zero risk.
* The bare-metal remeasure remains the gate for anything further (W15).

### P-B — INFO — `decommit`/`decommit_lazy` each perform two `page_size()` loads per call

`crates/vmem/src/lib.rs:874`, `:908`: `!start.is_multiple_of(page_size()) || !end.is_multiple_of(page_size())`.
Hoisting to one `let ps = page_size();` removes one relaxed atomic load from a path whose next
instruction is a `madvise` syscall — i.e. genuinely immeasurable. Listed only for completeness;
the readability gain is the real argument.

### P-C — the Unix no-trim change's address-space cost is unmeasured

Covered under W5's second half. `benches/vmem_bench.rs` could answer the VA question directly with
one added arm (reserve N spans, hold them, read `/proc/self/statm`) but nothing does today, and the
crate documents no VA-per-reservation figure at all.

---

## Checked and explicitly NOT findings

* **The docs.rs opt-in (task #850's own self-reported near-miss) is genuinely correct.**
  `crates/vmem/src/lib.rs:83` carries `#![cfg_attr(docsrs, feature(doc_cfg))]`, and a real
  nightly `RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc -p aligned-vmem --no-deps --features
  "lazy-commit huge-pages fault-injection"` completes with **zero output**. Badge coverage was
  audited item-by-item against every `#[cfg(feature = …)]`-gated `pub` item: all 15 in `lib.rs`
  (2 modules, 3 statics, 4 fns, 4 lazy-commit fns, 2 huge-pages fns) carry a matching
  `#[cfg_attr(docsrs, doc(cfg(feature = …)))]` under the **correct** feature name, and the items
  inside `mock`/`fault_injection` carry (redundant but harmless) badges that agree with their
  enclosing module's. Nothing is missing a badge; nothing has a badge under the wrong feature.
* **V5's two test leaks are genuinely fixed and the tests are not weakened.**
  `tests/smoke.rs:309-361` — both now `catch_unwind` the panicking `from_raw_parts` call, `release`
  the reservation, then `resume_unwind` the *original payload*, so
  `#[should_panic(expected = "align must be a power of two >= PAGE")]` and
  `#[should_panic(expected = "must form a valid Layout")]` still discriminate the two distinct
  messages. A reintroduced bug fails via a message mismatch, not silently.
* **#848's `commit_len == size` guard is correct, and its regression test is non-vacuous.**
  `tests/lazy_commit.rs:70-116` uses `align = PAGE` (under the 64 KiB threshold) with
  `initial_commit < size` — the exact shape that would take the unsound fast path — and its
  load-bearing assertion is the `commit_range(base, initial, size)` past `initial_commit`, not
  `r.len()` (which is set from the request regardless). Verified by reading the guard at
  `lib.rs:1344` against the test's parameters.
* **The V1 no-trim fix is correct as written.** `lib.rs:1786-1808` returns
  `(base, region_ptr, over)`; `release_reservation` (`:1867-1871`) `munmap`s exactly
  `(region_ptr, over)`, and `region_ptr` is a kernel-chosen `mmap` result (provably page-aligned)
  with `over = size + align` — so V1's misaligned-`munmap` class is now structurally unreachable,
  not merely guarded. The hugetlb guard at `:1718-1724` still holds independently.
* **`fault_injection`'s atomics are unchanged since #718/#775 and remain correct as written.**
  `Release`/`Acquire` pairing at `:108`/`:139`, `fetch_update` at `:125-133` with the documented
  `then` (lazy) vs `then_some` (eager) underflow note. The third, narrower disarm-vs-rearm race is
  still declared out of scope in the module's own doc (`:47-57`) and is not re-opened.
* **`impl From<VmemError> for std::io::Error` (`error.rs:138-148`) is correct**, and
  `tests/vmemerror_io_bridge.rs` covers all three arms (raw code → `from_raw_os_error`,
  invalid-argument → `InvalidInput`, unknown → `Other`) with real assertions on
  `raw_os_error()`/`kind()`/message. The `code as i32` cast is theoretically lossy for Windows
  codes above `i32::MAX`; `GetLastError` values in this crate's reachable set (commit-charge /
  parameter errors) are four digits, so this is noted, not filed.
* **`error.rs`'s V17 de-duplication is real** — one `#[cfg(not(miri))] last_os_error_code` at
  `:150-155` plus the `#[cfg(miri)]` arm at `:157-160`, no duplicated body.
* **`page_size()` is now genuinely consumed inside the crate** (`lib.rs:874`, `:908`), closing V2's
  secondary observation that the crate held the correct value and declined to use it.
* **`mock::Call`'s V6 constructors are complete** — all 8 variants have one (`mock.rs:136-191`),
  and `tests/mock.rs:129-155` exercises three of them from the integration-test crate (a genuinely
  separate crate, so the `#[non_exhaustive]` enforcement is real, not vacuous).
* **`tests/min_page.rs`'s two tests are near-tautological but not vacuous** — `MIN_PAGE` is defined
  as `= PAGE` so `min_page_equals_page` can only fail if someone redefines it, which is exactly the
  contract V13 asked to pin. Filed as "checked", not as test-vacuity.
* **`Reservation`'s `Debug`, `is_empty`'s deprecation, `Send`-not-`Sync`, `from_raw_parts`'s
  `Layout` assertion, the `_SC_PAGESIZE` per-OS table, the `off_t` width note, `MAP_ANON`'s BSD
  value, and the `mock` feature-unification decision.** All re-read; all as previously decided.
  The four BSD `_SC_*` values were again not independently re-derived from vendor headers here
  (same limitation `#714` recorded); `docs/CORRECTNESS_OPEN_ITEMS.md` item 43 already owns that.
* **`cargo package --list`** — the published tarball includes `README.md`, both licences, the bench
  and the new example. `bench-iters.txt`'s absence is V19's known gap and is now documented in the
  bench file's own header (`benches/vmem_bench.rs:19-29`), which is what #850 committed to.
* **The `#[non_exhaustive]` posture across the public surface.** `mock::Call` (enum + all 8
  variants), `ReservationParts` — both correct. `VmemError` and `Reservation` have private-only
  fields, so `#[non_exhaustive]` would add nothing.

---

## Recommended order

1. **W1** — fix the two `cfg(miri)` tuple patterns (`lib.rs:2239`, `:2250`) and, in the same
   commit, add the compile-only miri guard from W14.1 to `aligned-vmem-gates` and update
   `docs/CORRECTNESS_OPEN_ITEMS.md` item 41's Status card, which currently misstates the remaining
   blockers. **Do not publish 0.2.0 before this.**
2. **W3** — decide the Windows large-page question (make it work unconditionally, or correct the
   four doc sites + README). This is the only finding whose current text is *shipping-false* on the
   crates.io page.
3. **W2** — make `granted_huge` report the grant rather than the request. Same session as W3; both
   are the same feature's observable.
4. **W5** — one grep-driven pass over `trim`/`over-reserve` (7 sites incl. `Cargo.toml:7`), plus
   one sentence in `reserve_aligned`'s rustdoc and the README table stating that a Unix fast-path
   miss retains `size + align`.
5. **W14** — the remaining two CI rows: `cargo test -p aligned-vmem --all-features` on
   `test-macos`, and a `cargo hack check --feature-powerset --depth 2 -p aligned-vmem` step
   (~22 invocations, cheap enough for the per-PR path).
6. **W4** — split the Windows counter and correct its rustdoc + the module design comment; either
   measure the 64 KiB bench arm or relabel `lib.rs:1321-1326`'s claim as R32-13-inherited
   hypothesis.
7. **W8, W6, W10, W9** — four small, obviously-correct edits (one stale rustdoc paragraph, two
   orphaned doc comments, two README lines, one test's `PAGE` → `page_size()`). Batchable in
   minutes.
8. **W11** — the one decision that is breaking to revisit after publish: `#[doc(hidden)]` on the
   three `bench-internals` statics (and, if W4's split lands, the counter rename becomes free).
   Settle **before** the 0.2.0 publish.
9. **W15, W16** — the campaign's own paper trail: the #849 measurement report + summary CSV + an
   `OPEN_ITEMS.md` card for the bare-metal remeasure, and the `CHANGELOG.md` entry for
   #842-#850.
10. **W7, W12, W13** — the three recorded-not-defect items: `RawReservation`'s scope, a
    `ReservationParts` constructor, and the `try_decommit` question. All non-breaking to revisit
    after publish; none blocks anything.
11. **P-A's free guard** (skip the provably-true alignment check for `align <= page_size()`) and
    **P-B** (hoist the double `page_size()` load). Readability, not measurable performance.
</content>
</invoke>
