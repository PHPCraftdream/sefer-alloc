# `aligned-vmem` — code-quality / bug-hunt / API-ergonomics / perf review

**Date:** 2026-08-12
**Scope:** `crates/vmem/` (package `aligned-vmem` 0.2.0) in full — `src/{lib,error,mock,fault_injection}.rs`,
`tests/` (5 files), `benches/vmem_bench.rs`, `Cargo.toml`, `README.md` — read as *fresh* code
quality: smells, API/ergonomics, real bugs, and measurable perf.
**Reviewed tree:** `main` @ `05ce375373c59bb2db98dd584acda52c03f00d1b`; `git status --short --
crates/vmem docs/reviews` is empty (the working tree's modifications are all in `crates/region/`
and root docs, none in scope).
**Toolchain:** `rustc 1.97.0 (2d8144b78 2026-07-07)` / `cargo 1.97.0`, Windows 10 Pro x86_64;
`miri 0.1.0 (3659db0d3e 2026-07-05)` on `nightly-x86_64-pc-windows-msvc`.
**Nature:** read-only. Nothing was modified other than the creation of this document. No
`git add` / `git commit`. Every command quoted below was actually run on this host.

**Explicitly out of scope (already found, already fixed, not re-opened):** everything in
`docs/reviews/2026-08-06-aligned-vmem-publish-readiness-review.md`,
`…-2026-08-07-…-rust-intel-audit.md`, `…-2026-08-09-…-round-closing-review.md` and
`…-2026-08-10-…-publish-readiness.md` — the `#712` success-sentinel clamp, `#713`'s errno
capture timing, `#714`'s `_SC_PAGESIZE` table and hugetlb trim guard, `#715`'s `non_exhaustive`
decision and the `mock` feature-unification hazard, `#716`'s huge-pages/miri test gaps,
`#717`/`#776 F8`'s strict-provenance conversion, `#718`/`#775`'s atomics and its regression
oracle, `#719`/`#776`'s hygiene bundle, and `#699`'s fault-injection CI row. The known-open
README 0.1→0.2 migration-section gap is also not re-listed. Where a finding below *touches*
one of those, it says so and cites it rather than presenting it as new.

**Platform honesty up front.** This host is Windows/x86-64 with a 4 KiB page. Findings V1, V2
and V4-Linux are **reasoned from the `mmap(2)`/`madvise(2)`/`munmap(2)` specifications and
this crate's own source**, not reproduced on a 16 KiB-page (Apple Silicon macOS) or 64 KiB-page
(aarch64 Linux) host — none is available here, and none is in this project's CI. Finding V3 is
likewise reasoned from the Win32 `VirtualAlloc` contract (this host has no
`SeLockMemoryPrivilege`, so the large-page path cannot be exercised even locally). Each such
finding says REASONED-FROM-SPEC in its own body. V5 was **executed** and is the one finding
here with a machine-produced receipt.

---

## Verdict up front

**As code, the crate is in good condition and its previous four review rounds are visible in
it.** 1 880 lines in `lib.rs` + 3 small modules; every `unsafe` block carries a `// SAFETY:`
proof; zero `TODO`/`FIXME`/`unimplemented!`/`dbg!` anywhere in `src/`, `tests/` or `benches/`;
`cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` is clean; the whole
suite is green; and a strict-provenance miri run over `tests/smoke.rs` reports **zero** UB.
The `try_*`/infallible pairing is genuinely in lockstep (each infallible form is a one-line
`.ok()`/`.is_ok()` forward), which is the right shape and is why `#712`'s class of bug cannot
silently reappear on one side only.

**Four real defects are new.** Three of them are the *same bug class as `#714`'s hugetlb leak,
in code paths `#714` did not touch*: the crate validates every offset and size against the
compile-time constant `PAGE` (4 KiB) but hands those values to syscalls that require the
**real** OS page granularity, and then discards the syscall's error return. On a 16 KiB- or
64 KiB-page Unix that produces (V1) an unbounded VA leak on the over-reserve trim path and
(V2) a `decommit` that silently does nothing while `recommit` still reports success — i.e. the
documented "recommit yields fresh zero-filled pages" guarantee silently returns stale data.
The fourth (V3) is that the Windows `MEM_LARGE_PAGES` request is malformed per the Win32
contract, so the `huge-pages` feature can never actually engage on Windows and always
degrades to ordinary pages with no way for a caller to notice.

**The single most valuable structural change would fix a bug and a perf item at once**: stop
trimming on the Unix over-reserve path (keep the whole `size + align` mapping as the
reservation, exactly as the Windows backend already does). That removes every `munmap` whose
alignment V1 is about, and cuts the miss-path syscall count from 6 to 2 per
reserve/release lifecycle. See V1's "Fix options" and P18.

**Publish posture:** V3 is the only finding that makes a *shipping README claim* untrue on
the most-used platform, and V1/V2 are the only ones that can lose a user's data or address
space. None is a soundness hole. If the 0.2.0 publish is time-boxed, V3's one-paragraph doc
correction plus V2's doc correction are enough to make the published text honest; V1 and V2's
code fixes can land in 0.2.1 without a semver problem.

---

## What was verified green (so the negatives below are read in context)

| command | result |
|---|---|
| `cargo test -p aligned-vmem --all-features --no-fail-fast` | **green** — smoke 16, lazy_commit 10, mock 7, huge_pages 1, fault_injection 0 (its own `not(feature = "mock")` gate; covered by CI's dedicated row per `#699`), lib 0, doctests 0 |
| `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` | **green**, exit 0 |
| `cargo doc -p aligned-vmem --features "lazy-commit huge-pages fault-injection" --no-deps` | **green**, zero warnings (the docs.rs feature set — `#776 F6`'s intra-doc links stay resolved) |
| `MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-disable-isolation" cargo +nightly miri test -p aligned-vmem --test smoke` | **15 passed, 1 ignored, zero UB** … then **3 leak errors, exit 1** — see V5 |
| `grep -rniE "todo|fixme|unimplemented|dbg!" src/ tests/ benches/` | no hits |
| `bench-scale-tool 0.1.0` dependency tree | zero transitive deps — the README's "zero dependencies" claim survives the dev-dependency added by `#758` |

---

# Findings

## Category 1 — bugs, invariants, error handling

### V1 — MEDIUM — the Unix over-reserve trim leaks the entire tail mapping when `size` is not a multiple of the **real** page size (16 KiB Apple Silicon / 64 KiB aarch64) — `#714`'s hugetlb bug, in the ordinary path

`crates/vmem/src/lib.rs:1396-1406` (the trim), `:1729-1743` (`libc_munmap` discarding the
error), `:523` (the size contract that permits the trigger).

`unix_reserve`'s fallback path computes

```
head     = base_addr - region_addr          // :1396
tail_len = region_end - tail_start          // :1397, tail_start = base_addr + size
munmap(region_ptr,               head)      // :1400
munmap(region_ptr @ tail_start,  tail_len)  // :1405
```

`munmap(2)` requires `addr` to be a multiple of the **OS** page size (length is rounded up;
address is not). Walk the two calls:

* `head` is safe by construction. `region_addr` is always page-aligned (kernel-chosen `mmap`
  result) and `align` is a power of two ≥ 4 KiB, so `align_up_addr(region_addr, align)` is
  page-aligned for both `align ≤ page_size()` (no-op) and `align > page_size()` (an
  `align`-multiple is a page-multiple). Therefore `head` is a page-multiple. ✔
* `tail_start = base_addr + size` is page-aligned **iff `size` is a multiple of
  `page_size()`** — and the public contract at `:523` only requires `size` to be a multiple of
  `PAGE` (4 KiB). ✘

**Failure scenario (concrete):** on Apple Silicon macOS (`page_size() == 16384`, which this
crate's own module doc at `:67-73` explicitly calls out), `reserve_aligned(3 * PAGE, 4 * MIB)`
is a contract-legal call. `try_reserve_aligned_exact` misses (a 16 KiB-aligned `mmap` result
is 4 MiB-aligned with probability ≈ 1/256), so the fallback runs: `over = 4 MiB + 12 KiB`,
`head` is a clean multiple of 16 KiB, and then `munmap(base + 12288, ~4 MiB)` is called with a
`12288 % 16384 != 0` address → **`EINVAL`**, which `libc_munmap` deliberately discards
(`:1729-1743` — that discard is documented as safe *precisely because* "every caller of this
function already establishes page/huge-page alignment before calling", which is exactly the
premise that fails here). The reservation is returned to the caller as `(base, base, size)`,
so the later `release` only unmaps `size`. Result: **~4 MiB of address space plus a kernel VMA
leaked per call, unbounded across calls**, with no error surfaced anywhere — `reserve_aligned`
returns `Some`.

Severity is MEDIUM rather than HIGH because the trigger needs both a non-4 KiB-page Unix host
*and* a `size` that is a 4 KiB- but not page-multiple; it is not memory-unsafety, and typical
allocator usage (`size == align == 4 MiB`) is unaffected. It is not LOW because the leak is
unbounded, silent, and reachable from a call the crate documents as valid.

**Fix options, in increasing order of ambition:**
1. Reject up front: require `size.is_multiple_of(page_size())` in `try_reserve_aligned` — the
   same shape `#714` chose for hugetlb, and consistent with the module doc that already tells
   callers to round to `page_size()`. Slightly narrows the 0.1 contract (same trade-off
   `#714`/F3 already accepted, and the same README paragraph would need extending).
2. Round the trim instead of the request: `tail_start_rounded = align_up(tail_start,
   page_size())` and unmap `[tail_start_rounded, region_end)`; the sub-page remainder stays
   mapped inside the caller's own span, which is harmless (it was already inside `over`).
3. **Do not trim at all** — keep the whole `over` mapping as the reservation and return
   `(base, region_ptr, over)`, which is byte-for-byte what the Windows backend already does
   and what `Reservation`'s `reservation_ptr`/`reservation_len` fields exist for. This makes
   the alignment question disappear (there is exactly one `munmap`, at the kernel-chosen,
   provably page-aligned `region_ptr`) **and** removes 2 syscalls per reservation — see P18.
   Cost: up to `align` bytes of untouched VA held for the reservation's lifetime (no RSS; a
   commit-charge cost only under `vm.overcommit_memory=2`).

### V2 — MEDIUM — `decommit` / `decommit_lazy` validate against `PAGE`, not `page_size()`: on a 16 KiB/64 KiB-page host a legal call is a **total silent no-op**, and `recommit` still reports success — so the documented zero-fill-on-recommit guarantee returns stale data

`crates/vmem/src/lib.rs:598-614` (`decommit`), `:632-648` (`decommit_lazy`), `:1479-1492`
(`decommit_pages_impl`), `:1749-1759` (`libc_madvise` discarding the error), doc at `:67-73`
and `:576-578`.

Both entry points accept any `start`/`end` that are multiples of `PAGE` (4 KiB) and pass them
straight to `madvise(2)`, which returns `EINVAL` when `addr` is not a multiple of the real page
size. `libc_madvise` discards the return (documented at `:1749-1759` as safe because "the
mapping stays exactly as valid … not a memory-safety concern" — true, but the argument only
covers *safety*, not the *behavioural* guarantee below).

**Failure scenario (concrete):** Apple Silicon macOS, `page_size() == 16384`. A caller does
`decommit(base, PAGE, 2 * PAGE)` — legal per the published contract. `madvise(base + 4096,
4096, MADV_DONTNEED)` fails `EINVAL`; nothing is decommitted; the error is dropped. The caller
then calls `recommit(base, PAGE, 2 * PAGE)`, whose Unix backend is an unconditional `Ok(())`
(`:1497-1501`), and reads the range — expecting the crate's own documented behaviour, "Re-access
after decommit produces fresh zero-filled pages" (`:576-578`). It gets **the old bytes**. For
the crate's stated target audience (allocators, arenas, slabs — a `calloc` fast path is the
textbook consumer of decommit-then-recommit zeroing) that is a correctness break, not a
performance one.

Two secondary points in the same place:

* The module doc's phrasing is wrong in the caller's favour: "A caller that decommits at
  4 KiB-but-not-page multiples would silently do **partial** work" (`:72-73`). It is not
  partial — `madvise` rejects the whole call, so **nothing** is decommitted. "Partial" invites
  a reader to assume the aligned prefix still got freed.
* `page_size()` is exported and documented as the thing callers must round to, but is **never
  called anywhere inside the crate** (verified: `grep -n "page_size()" src/*.rs` hits only its
  own definition and comments). The crate holds the correct value in a cached atomic and
  declines to use it for its own validation.

**Fix options:** validate against `page_size()` in `decommit`/`decommit_lazy` (a silent no-op
is already the documented behaviour for a violated range, so tightening the predicate changes
nothing observable on a 4 KiB host); or keep the validation and correct the docs to state the
all-or-nothing failure mode plus the consequence for the zero-fill guarantee. The first is
strictly better and costs one relaxed load per call, which is noise next to a `madvise`.

### V3 — MEDIUM — the Windows `MEM_LARGE_PAGES` request is malformed per the Win32 contract, so `huge-pages` can never engage on Windows; it silently degrades to ordinary pages and no API can tell the caller

`crates/vmem/src/lib.rs:1181-1186` (`reserve_aligned_huge_raw` → `win_reserve_commit(..,
MEM_LARGE_PAGES)`), `:1029-1040` (the plain `MEM_RESERVE` first call), `:1074-1099` (the
`MEM_COMMIT | MEM_LARGE_PAGES` second call and its fallback retry), `:914-916`/`Cargo.toml:44-51`/
`README.md:53-57` (the advertised behaviour).

The Windows path is: `VirtualAlloc(NULL, over, MEM_RESERVE, PAGE_READWRITE)` (no large-page
flag), then `VirtualAlloc(base, size, MEM_COMMIT | MEM_LARGE_PAGES, PAGE_READWRITE)` on the
already-reserved sub-range. `VirtualAlloc`'s documented contract for `MEM_LARGE_PAGES`
(0x20000000) states that (a) the flag **must be specified together with `MEM_RESERVE` and
`MEM_COMMIT`** — i.e. large pages can only be obtained by a single combined
reserve+commit call, never by committing into a previously reserved ordinary region — and
(b) size and alignment must be multiples of `GetLargePageMinimum()` (2 MiB on x64), which this
crate never queries and never validates. Every established large-page implementation follows
(a) (`mimalloc`, `jemalloc` both issue one `MEM_LARGE_PAGES | MEM_RESERVE | MEM_COMMIT` call).

**Failure scenario (concrete):** a Windows process that *has* been granted
`SeLockMemoryPrivilege` calls `reserve_aligned_huge(4 * MIB, 4 * MIB)` expecting the reduced
TLB pressure the feature is sold for. The `MEM_COMMIT | MEM_LARGE_PAGES` call fails
(`ERROR_INVALID_PARAMETER`), the best-effort retry at `:1086-1093` succeeds with ordinary
pages, and the function returns `Ok` — indistinguishable from success. The advertised
capability is unreachable on Windows for 100 % of calls, and there is no counter, flag or
accessor anywhere in the public API (`Reservation` exposes `as_ptr`/`len`/`reservation_ptr`/
`reservation_len` only) by which a caller could discover it. The crate's own
`huge_pages.rs` test suite cannot catch this: its one non-Linux test
(`reserve_aligned_huge_ordinary_page_sized_request_succeeds`) asserts only that the *fallback*
works.

REASONED-FROM-SPEC: not executed — this host has no `SeLockMemoryPrivilege`, so even a correct
implementation would fall back here, and the fallback is exactly what makes the defect
invisible.

**Fix options:** (a) issue one `VirtualAlloc(NULL, size, MEM_RESERVE | MEM_COMMIT |
MEM_LARGE_PAGES, PAGE_READWRITE)` attempt first and fall back to the current two-call ordinary
path on failure, plus validate `size`/`align` against `GetLargePageMinimum()` the way the Linux
side already validates against `LINUX_HUGE_PAGE_SIZE` (`:1336-1342`) — that also makes the two
platforms' contracts symmetric; or (b) if that is more than 0.2.0 should carry, state plainly
in the rustdoc/README/`Cargo.toml` that Windows large pages are **not currently obtained** and
the flag is aspirational, so the shipped text stops claiming a capability the code cannot
deliver. Either way, an `is_huge()`/`granted_huge_pages()` signal is what turns "best-effort"
from unfalsifiable into testable.

### V4 — LOW-MEDIUM — `decommit`/`recommit` on a huge-page reservation are silently unsupported on **both** platforms, and nothing documents it

Windows: large-page allocations cannot be decommitted at all — `VirtualFree(.., MEM_DECOMMIT)`
on a large-page region fails, and `winapi_virtual_decommit` (`:1262-1265`) discards the `i32`
return. Linux: `MADV_DONTNEED`/`MADV_FREE` on a `MAP_HUGETLB` mapping is accepted only at
huge-page granularity, so any `PAGE`-granular offset gets `EINVAL` — discarded at `:1749-1759`.

**Failure scenario:** a consumer enables `huge-pages` + uses `decommit` (the exact combination
the crate's own README pitches: "hand pages back to the OS, keep the address reservation"),
calls `decommit(huge_base, 0, 2 * MIB)` and then `recommit`, which returns `true`. Nothing was
returned to the OS, the RSS accounting the consumer is optimising for does not move, and the
range still holds its old contents — the same stale-data shape as V2, reached from a
completely supported feature combination on a mainstream 4 KiB-page host.

Today `decommit`'s rustdoc documents the Windows-vs-Linux *semantic* divergence in detail
(`:588-597`) but says nothing about huge pages; `reserve_aligned_huge`'s rustdoc says nothing
about decommit. This is at minimum a documentation defect, and arguably an argument for
rejecting the combination outright (a `Reservation` that knows it is huge could make
`decommit` a documented `Err`/no-op).

### V5 — LOW — `cargo miri test -p aligned-vmem` **fails today** (3 leaks), two of them genuine test-side leaks, for a crate whose headline claim is "miri-friendly" — and no CI job runs miri on this package

VERIFIED, not reasoned. Command and receipt:

```
$ MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-disable-isolation" \
    cargo +nightly miri test -p aligned-vmem --test smoke
test result: ok. 15 passed; 0 failed; 1 ignored; …
error: memory leaked: alloc100780 (Rust heap, size: 16384, align: 4096) …
error: memory leaked: alloc77238  (Rust heap, size: 4096,  align: 4096), allocated here:
          … from_raw_parts_rejects_an_overflowing_reservation_len_immediately
              at crates\vmem\tests\smoke.rs:276:13
error: memory leaked: alloc89387  (Rust heap, size: 4096,  align: 4096), allocated here:
          … from_raw_parts_rejects_non_power_of_two_align_immediately
              at crates\vmem\tests\smoke.rs:256:13
error: aborting due to 3 previous errors        # exit code 1
```

Three separate observations:

1. **Zero UB.** Under `-Zmiri-strict-provenance` the whole smoke suite is clean — this is the
   strongest available confirmation that `#717`/`#776 F8`'s `.addr()`/`.with_addr()` conversion
   is genuinely sound, and it is worth recording as a positive.
2. **Two of the three leaks are real test bugs, not intentional.** `smoke.rs:256` and `:276`
   both call `r.into_parts()` (which suppresses `Drop` by contract) and then deliberately panic
   inside `from_raw_parts`, so nothing ever releases the reservation. On the real OS backend
   that leaks a 4 KiB mapping per test run — invisible without a leak checker, which is exactly
   why it survived. Fix: wrap the panicking call in `catch_unwind` and `release` the triple
   afterwards, or reserve/release around the assertion.
3. **The third leak (16 384 B = `3 * PAGE + 7` rounded up) is `leak_zeroed_pages`, and is by
   design** — but it means this file can never pass a default miri run without either
   `-Zmiri-ignore-leaks` or splitting that test out. So "miri-friendly" is currently a claim
   about the *backend*, not about the *suite*; the `#[cfg(miri)]` code paths (including
   `leak_zeroed_pages`'s explicit zeroing at `:980-985`, which is a documented behavioural
   guarantee, and `release_reservation`'s `Layout::expect` at `:1805`) are exercised by no miri
   job in `.github/workflows/ci.yml` — every `miri-*` job there is root-crate-scoped.

Related, same paragraph of the same docs: `reserve_aligned`'s miri backend is
`std::alloc::alloc` (`:1793`) with the in-code justification "under miri the consumer is not
the global allocator, so no reentrancy". That is an assumption about the consumer, not a
property of this crate — and `leak_zeroed_pages`'s own rustdoc (`:947-956`) advertises the
opposite use case verbatim: "used by allocators for pre-main bookkeeping structures that **must
not route through the very allocator they implement**". Under miri it routes through exactly
that allocator. One sentence in the crate-level miri paragraph ("a consumer that installs
itself as `#[global_allocator]` cannot use this crate under miri") closes the gap; it is the
same hazard class `numa-shim`'s `#777` hit for real.

## Category 2 — API surface and ergonomics

### V6 — MEDIUM — `mock::Call`'s variant-level `#[non_exhaustive]` makes the recorded log unconstructable downstream, so its `PartialEq`/`Eq` derives are dead weight for the consumers the mock exists for

`crates/vmem/src/mock.rs:55-132` (the enum), `:145-148` (`drain`).

`#[non_exhaustive]` on a struct-like *variant* forbids external crates from constructing it and
from matching it without `..`. `mock`'s entire purpose is to let a downstream consumer assert
"my code issued exactly these OS calls" — the natural assertion,
`assert_eq!(mock::drain(), vec![Call::Reserve { size: 2 * MIB, align: 2 * MIB }])`, **does not
compile** outside this crate. The crate's own integration tests already show the degraded
shape they were forced into (`tests/mock.rs:24-33`, `:126`):

```rust
assert!(matches!(calls[0], Call::Reserve { size, align, .. } if size == 2*MIB && align == 2*MIB));
```

which is longer, loses the "and nothing else" property of a `Vec` comparison, and silently
degrades to `true` for any future variant that gains the same field names. `#[derive(PartialEq,
Eq)]` on `Call` is now only usable for comparing two *drained* values against each other.

This is not a request to revert `#715`'s decision (which is correct on semver grounds — adding
a field to a struct variant is breaking without it). It is a request to pay the ergonomics debt
the decision incurred, before publish, in one of three ways: (a) `pub fn` constructors
(`Call::reserve(size, align)`) — smallest change, restores `assert_eq!`; (b) field accessors
(`Call::size()`, `Call::base()`) plus a documented `matches!` idiom; or (c) a small assertion
helper on the drained log (`mock::assert_calls([Expect::Reserve { .. }])`). Doing nothing is
also a defensible choice — but then the `PartialEq`/`Eq` derives and the `Vec<Call>` return
type advertise an assertion style that consumers cannot use.

### V7 — MEDIUM — `Reservation` has no `Debug`

`crates/vmem/src/lib.rs:298-307`. There is no `#[derive(Debug)]` and no manual impl (verified:
the only `derive` in `lib.rs` is on `DecommitKind` at `:1860`). Any downstream struct that owns
a `Reservation` therefore cannot `#[derive(Debug)]` — a routine, annoying papercut for a type
whose whole job is to be stored in someone else's allocator metadata. `VmemError` (manual
`Debug`/`Display`) and `mock::Call` (derived) both have it; `Reservation` is the odd one out.
A derived impl would print the raw `NonNull`s; a small manual one printing
`base`/`len`/`reservation_len`/`align` is better and is ~8 lines. Non-breaking either way.

### V8 — LOW-MEDIUM — `into_parts()` → `release()` is a 3-tuple of two indistinguishable `usize`s, and swapping them compiles

`crates/vmem/src/lib.rs:369-373` (`into_parts`), `:557-569` (`release`).

`let (raw, raw_len, raw_align) = r.into_parts(); release(raw, raw_len, raw_align)` and
`release(raw, raw_align, raw_len)` are both well-typed. The second is catastrophic: on Unix it
`munmap`s `align` bytes instead of `reservation_len` (partial unmap → leak, or an unmap of a
range the caller still uses if `align > len`), and under miri it builds a different `Layout`
than the allocation used, which is instant UB. This is the crate's *documented* self-hosting
pattern ("use `into_parts` when your allocator records the reservation in its own metadata"),
so it is on the path the README recommends. A `#[non_exhaustive] pub struct ReservationParts {
pub ptr, pub len, pub align }` returned by `into_parts` and consumed by an added
`release_parts(ReservationParts)` (keeping the current `release` for compatibility) makes the
mistake unrepresentable. Cheap to add now, breaking to add later.

### V9 — LOW — `Drop` does not record a `mock::Call::Release`, so the mock cannot observe the crate's own recommended (RAII) release path

`crates/vmem/src/lib.rs:482-490` (`Drop`) vs `:562-566` (`release`, which does record).
A consumer using the mock to prove "every reservation I take is released exactly once" can only
see the manual `into_parts` + `release` path; the RAII path — the one the docs push first — is
invisible. Adding the record inside `Drop` (or, better, having `Drop` call `release` so there is
one recording site) closes it. Note this also makes `Call::Release` counts meaningful for
leak-style assertions, which is the main thing a recording mock is for.

### V10 — LOW — two doc statements no longer match the code they describe

1. `crates/vmem/src/mock.rs:158-163`: `fail_next_reserve`'s rustdoc says armed reservations
   "return `Err(VmemError::last_os_error())` without allocating". Since `#776`/F2 they return
   `VmemError::os_refusal_unknown_code()` (`mock.rs:183-190`) — and `tests/mock.rs:142-158`
   asserts exactly that. The doc now describes the bug the fix removed, and it is the *public*
   half of the pair (`take_reserve_fault`, which carries the correct comment, is
   `pub(crate)`).
2. `crates/vmem/src/fault_injection.rs:88-95`: `arm_fail_at(k)`'s rustdoc says "the k-th call to
   the real commit path from now fails". A call consumed by `arm_fail_next` returns *before*
   `FAIL_AT_COUNTER` is incremented (`:122-138`), so when both hooks are armed the k-th
   *counted* call is not the k-th *real commit* call. The behaviour is deliberate and is pinned
   by `tests/fault_injection.rs:113-143` — but only the test says so; a reader of the public
   doc would predict the opposite. One clause ("calls already consumed by `arm_fail_next` are
   not counted") fixes it.

### V11 — LOW — docs.rs will render three optional features with no per-item feature badges

`crates/vmem/Cargo.toml:26-27` sets `features = [...]` but not `rustdoc-args = ["--cfg",
"docsrs"]`, and no item carries `#[cfg_attr(docsrs, doc(cfg(feature = "…")))]`. The published
page will therefore show `commit_range`, `reserve_aligned_lazy`, `reserve_aligned_huge` and the
`fault_injection` module side by side with the always-available API and no indication that they
need a feature — for a crate whose README leads with its feature list. The
`alloc-lazy-commit` → `lazy-commit` compat alias (`Cargo.toml:41-42`) has the same shape of gap
in the other direction: its "kept for one release" policy exists only in a `Cargo.toml` comment,
with nothing in the rendered docs or the README telling a 0.1 user when it disappears (the
known-open migration-section gap is the same root cause; noted here only because the fix is the
same edit).

### V12 — INFO — `VmemError` has no `std::io::Error` bridge

`crates/vmem/src/error.rs`. A consumer that wants to propagate a reservation failure into an
`io::Result` must hand-write the conversion, and `os_code() -> Option<u32>` forces raw numeric
comparison (`1455` for `ERROR_COMMITMENT_LIMIT` appears as a bare literal in this crate's own
test at `smoke.rs:330`). `impl From<VmemError> for std::io::Error` (mapping `Some(code)` →
`io::Error::from_raw_os_error`, `invalid_argument()` → `ErrorKind::InvalidInput`, unknown →
`ErrorKind::Other`) is ~10 lines, non-breaking, and removes the numeric literals from consumer
code. The crate already depends on `std` (`error.rs:136`, `std::thread_local`), so there is no
`no_std` cost.

### V13 — INFO — `PAGE` is named for what it is not

`crates/vmem/src/lib.rs:113-120`. The constant is the crate's *minimum* granularity, and every
place it appears has to disclaim that it is not the page size — the module doc (`:67-73`), the
constant's own doc, `page_size()`'s doc (`:227-230`), and the README (`:46-47`). The name is
the reason V1 and V2 exist as bugs rather than as obvious type errors. Renaming is breaking
against the published 0.1.0, so the realistic move is an added
`pub const MIN_PAGE: usize = PAGE;` (or `GRANULARITY`) used in new docs and a soft-deprecation
note on `PAGE` — not a rename.

## Category 3 — code smells and structure

### V14 — LOW — the reservation entry points are three near-identical bodies, and the size/align predicate is written out verbatim three times

`crates/vmem/src/lib.rs:522-545` (`try_reserve_aligned`), `:833-884` (`try_reserve_aligned_lazy`),
`:920-941` (`try_reserve_aligned_huge`). Each is: same 4-clause validation → `mock` fault check
→ `mock` record → call a `*_raw` → the identical
`.map(|(base, reservation, reservation_len)| Reservation { base, len: size, reservation,
reservation_len, align })` closure. The predicate
`size == 0 || !align.is_power_of_two() || align < PAGE || !size.is_multiple_of(PAGE)` appears at
`:523`, `:838-841` and `:921` character-for-character.

That triplication is not hypothetical risk: V1 and V2's fixes each need the *same* predicate
changed in three (V1) and two (V2) places, and a future contract change that lands in two of
three is exactly the divergence class `#712` was. A private
`fn validate_size_align(size, align) -> Result<(), VmemError>` plus a private
`fn finish(size, align, raw) -> Result<Reservation, VmemError>` removes ~50 lines and makes the
three public functions read as "validate; record; pick a backend".

### V15 — LOW — nine backend functions return an unnamed `(NonNull<u8>, NonNull<u8>, usize)` whose first two elements are the same type and are easy to transpose

`:1000`, `:1020`, `:1172`, `:1181`, `:1280`, `:1331`, `:1429`, `:1516`, `:1525`, `:1785`,
`:1836`, `:1845`. `(base, reservation, reservation_len)` — `base` and `reservation` are
interchangeable at the type level, they differ on exactly one path (`win_reserve_commit`
returns `base != region`; every Unix/miri path returns `base == base`), and transposing them on
Windows would produce a reservation released at the wrong address. A named
`struct RawReservation { base, reservation, reservation_len }` costs nothing at runtime and
makes the one asymmetric path self-documenting.

### V16 — LOW — twelve `#[cfg_attr(feature = "mock", allow(dead_code))]` attributes are the cost of `mock` being a *partial* backend replacement

`:1123`, `:1136`, `:1161`, `:1171`, `:1244`, `:1261`, `:1478`, `:1496`, `:1506`, `:1515`,
`:1537`, `:1598`, `:1603`, `:1812`, `:1819`, `:1827`, `:1835`, `:1867` (plus the 25-line module
comment at `:77-99` explaining them). Each one is individually justified and the narrowing from
a crate-wide `allow` was itself a fix (`#646`/F8, `#719`). The residual smell is structural:
`mock` replaces decommit/recommit/commit_range but *not* reserve/release, so every platform's
`*_impl` trio goes dead under `mock` while the reserve path stays live. If the three backends
were three `#[cfg]`-selected private modules (`os_windows` / `os_unix` / `os_miri`) with one
shared private signature, `mock` could be a **fourth** module selected by the same `#[cfg]` and
every one of these attributes would disappear. That is a larger refactor than 0.2.0 should
carry, and CLAUDE.md's "single-file seam crate" exception explicitly sanctions the current
shape — recorded as the option, not as a defect.

### V17 — LOW — `error.rs` duplicates `last_os_error_code` verbatim for `unix` and `windows`

`crates/vmem/src/error.rs:138-150`: two functions, identical bodies, differing only in their
`#[cfg]`. One `#[cfg(not(miri))]` covers both, and the `#[cfg(miri)]` arm at `:152-155` stays.
Trivial, but it is the only duplication left in the file and it invites the two copies to drift.

### V18 — LOW — `benches/vmem_bench.rs` is compiled by **no** CI job, and its arms are not mutually comparable

Verified: `cargo test -p aligned-vmem --all-features --no-run` builds six executables (lib +
five integration tests) and **not** the bench; `.github/workflows/ci.yml` has clippy rows for
the root crate and for `sefer-region` (`:63-64`, added as `sefer-region`'s release gate) but
none for `aligned-vmem`, so no `--all-targets` compile of this package happens anywhere. A
rename of any public function used by the bench would leave CI green. One line —
`cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings`, mirroring
`ci.yml:63` — closes it and simultaneously gives this crate the same release-gating row
`sefer-region` got from `#793`.

Two smaller things inside the file: `black_box` usage differs per arm (`reserve_release_cycle`
wraps both arguments and the result; `reserve_decommit_release_cycle` wraps only `base`/`len`
and not the `reserve_aligned` call — `:42-45` vs `:57-61`), so the arms carry different amounts
of optimisation-barrier overhead and their absolute ns/op are not strictly comparable to each
other. And all four arms use `align == size`, which on Unix means the exact-mmap fast path
*misses* ~15/16 of the time at 64 KiB and essentially always at 1 MiB — the bench therefore
measures the over-reserve path almost exclusively, with `bench-internals` off so nothing
records which path each sample took. Enabling `bench-internals` in the bench and printing
`unix_exact_reserve_hits()/_attempts()` alongside the ns/op would make each arm
self-describing (and would answer P17 as a side effect).

### V19 — INFO — the bench's iteration counts live outside the published package

`bench-scale-tool`'s manifest resolves to `<workspace-root>/bench-iters.txt`, which is tracked
at the repo root (entries `vmem_bench::reserve_release = 75169`, `…_1mb = 26665`,
`…_decommit_release = 60637`, `…_decommit_recommit_release = 38389`) and is **not** part of
`crates/vmem/`. From a published tarball `cargo bench -p aligned-vmem` finds no manifest and
JIT-calibrates each workload at 1 s, i.e. silently produces different iteration counts than the
in-repo run — the same "packaged bench is not self-sufficient" shape `sefer-region` filed as
`#820`/F6. Nothing in the crate documents this. Since no vmem numbers are published in the
README, the impact today is nil; it becomes real the moment a perf table cites the bench.

## Category 4 — performance

Null result first, so the two positives below are read in proportion: **there is no CPU-bound
hot path in this crate to optimise.** Every public entry point is dominated by one syscall;
`page_size()` is a single relaxed load after the first call (`:233-236`); `align_up_addr` is
two arithmetic ops; the `bench-internals` counters are compiled out by default and are already
consumed by a real gate (`docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md`), so
they are not a dangling instrument. The only real levers are **syscall count** and **address
space**, and both are structural.

### V20 (P17) — MEDIUM opportunity, **unmeasured on Unix and cheap to measure** — the exact-size fast path is a net syscall *loss* for exactly the `align ≫ page` case the crate is built for

`crates/vmem/src/lib.rs:1343-1345` (`unix_reserve` tries it unconditionally), `:1429-1466`
(`try_reserve_aligned_exact`).

`unix_reserve` always tries a 1-syscall exact `mmap(size)` first and, on an alignment miss,
`munmap`s it and runs the full over-reserve path. Per-reservation syscalls:

| outcome | syscalls (reserve) | syscalls (+ release) |
|---|---|---|
| fast-path hit | 1 (`mmap`) | 2 |
| fast-path miss | 5 (`mmap`, `munmap`, `mmap`, `munmap` head?, `munmap` tail) | 6 |
| fast path skipped entirely | 3 | 4 |

The break-even is a ~50 % hit rate, which is exactly what the counter comment at `:132-146`
says the survey computed — and the hit rate is a function of `align`: for `align ≤ page_size()`
it is **100 % by construction** (a kernel `mmap` result is always page-aligned, so the
`is_multiple_of(align)` check at `:1451` cannot fail); for `align = 4 MiB` on a 4 KiB page it is
~1/1024 in the absence of transparent-huge-page address alignment (Linux aligns anonymous
mappings ≥ 2 MiB to 2 MiB when THP is enabled, which lifts the 2 MiB case and only partially the
4 MiB case). The crate's own flagship use case — an allocator asking for a `SEGMENT`-aligned
`SEGMENT` — sits in the losing regime.

Two guards, neither of which needs new instrumentation:

* `align <= page_size()` → take the fast path and **skip the alignment check entirely** (it is
  provably true). Saves nothing in syscalls but removes a branch and makes the intent explicit.
* `align > <threshold>` → skip the fast path, saving **2 syscalls per reservation** on every
  miss.

The threshold is the one open number, and `UNIX_EXACT_RESERVE_HITS`/`_ATTEMPTS` already exist to
settle it. `R32_13_…_GATE.md` §0 states verbatim that the Unix hit rate was "**Not measured on
this run** … A future Linux-side task can read it directly; no new instrumentation work is
needed." That is still true, and it is a 20-minute WSL/Linux run of the existing
`benches/vmem_bench.rs` with `--features bench-internals` (see V18) plus a second arm at
`align = 4 MiB`. Until it is run this stays an opportunity, not a claim.

### V21 (P18) — LOW-MEDIUM opportunity, Windows, quantified from this project's own measurement — one combined `VirtualAlloc` for `align ≤ 64 KiB`

`crates/vmem/src/lib.rs:1020-1110` (`win_reserve_commit`), `:1212` (`dw_allocation_granularity`,
declared and never read).

The Windows path is unconditionally two syscalls — `VirtualAlloc(NULL, size + align,
MEM_RESERVE)` then `VirtualAlloc(base, size, MEM_COMMIT)` — plus a `size + align` VA
over-reserve that is never trimmed (Windows cannot partially release a reservation). But
`VirtualAlloc(NULL, …)` already returns a base aligned to the system **allocation granularity**
(64 KiB on every supported Windows target), so for `align ≤ 64 KiB` a single
`VirtualAlloc(NULL, size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE)` satisfies the alignment
contract by construction: **one syscall instead of two, and `size` instead of `size + align`
VA.**

Quantified from this repo's own numbers, measured on this same machine:
`docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md` §0 reports median
`MEM_RESERVE` ≈ **4 580 ns** and `MEM_COMMIT` ≈ **9 133 ns** per segment. Removing the separate
reserve call is therefore worth ~4.6 µs of a ~13.7 µs pair — a ~33 % cut of the
reserve+commit cost **in that alignment regime** (a combined call should cost about what the
commit costs; that is the hypothesis a measurement would confirm, using the
`WINDOWS_RESERVE_COMMIT_CALLS` counter as the path-activation oracle and
`benches/vmem_bench.rs`'s existing 64 KiB arm as the workload).

**Stated honestly, this does not help the in-repo consumer.** `sefer-alloc`'s segments use
`align = 4 MiB > 64 KiB`, so they stay on the two-call path; R32-13 additionally measured the
whole reservation path at only 4.3-4.8 % of the Windows segment lifecycle. This is a win for
`aligned-vmem` as a standalone crate with small-align callers (including its own 64 KiB bench
arm), not for the allocator — which is exactly why it belongs in this review and not in a
sefer-alloc perf gate.

Adjacent, and deliberately **not** recommended: `VirtualAlloc2` +
`MEM_EXTENDED_PARAMETER_ADDRESS_REQUIREMENTS` would remove the `size + align` over-reserve for
`align > 64 KiB` too (2× less VA for 4 MiB/4 MiB segments). R32-13 rejected that "step 3" on a
**latency** basis (4.6 % materiality); the address-space axis was never evaluated separately.
On 64-bit, VA is not scarce enough to justify a Win10-1803+ API floor — noted so the existing
rejection is understood as latency-scoped rather than absolute.

**And the free one:** V1's fix option 3 (stop trimming on Unix) is simultaneously a perf change
— it takes the miss path from 6 syscalls per reserve/release lifecycle to 2, and the hit path
stays at 2. Cost: up to `align` bytes of untouched VA per live reservation, i.e. adopting the
Windows trade-off on Unix. It is measurable with the existing bench and it makes V1
structurally impossible rather than merely guarded. If only one thing from this review is
implemented, this is the one with the best ratio.

---

## Checked and explicitly NOT findings

* **Strict provenance (`#717`, `#776`/F8).** All three native address computations use
  `.addr()`/`.with_addr()`; `mmap`'s `MAP_FAILED` comparison uses `.addr()` too (`:1721`). The
  miri run above (`-Zmiri-strict-provenance`) reports **zero** UB across 15 tests. Sound.
* **`VmemError`'s three-way classification (`#713`).** `invalid_argument` / `from_os_code` /
  `os_refusal_unknown_code` are genuinely distinguishable and tested
  (`smoke.rs:321-367`). The `Display` impl does not fabricate "code 0". Correct.
* **`recommit`/`commit_range` rejecting a violated offset range (`#712`).** Verified in source
  and by test; the `start == end` early-`Ok` before the alignment check
  (`:685-690`, `:772-777`) is a *genuinely empty* range and permits no write, so it is not a
  reopening of `#712`.
* **`from_raw_parts`'s `Layout` assertion (`#719` item 4, `#776`/F7).** Covers both the `align`
  and the `reservation_len` halves in one `Layout::from_size_align(...).is_ok()` call; both
  halves have their own `#[should_panic]` test. Complete. (The *leak* those two tests cause is
  V5, a different issue.)
* **`fault_injection`'s `Release`/`Acquire` pairing and `fetch_update` (`#718`).** Correct as
  written; the third, narrower disarm-vs-rearm race is already declared out of scope in the
  module's own doc (`:47-57`, `#776`/F15) and is not re-opened here.
* **`#714`'s Linux hugetlb `size`/`align` guard.** The reasoning (both huge-page-aligned ⇒
  `head == 0` and `tail_len` huge-page-aligned) checks out. V1 is the *ordinary*-page instance
  of the same class, which `#714` did not address.
* **The `mock` Cargo feature-unification hazard (`#715`/§C10).** Documented in three places
  (`Cargo.toml:60-81`, `mock.rs:25-38`, `README.md:60-64`) with the `--cfg` alternative weighed
  and deferred. Not re-opened; V6 is about the *ergonomics* of the `non_exhaustive` decision,
  not the feature's exposure.
* **The `bench-internals` counters as a "hook with no consumer".** They have one:
  `Cargo.toml:579` forwards `aligned-vmem?/bench-internals`, `AllocCore`/`HeapCore` re-export
  them, and `R32_13_…_GATE.md` uses them as its path-activation oracle. Not dangling.
* **`let _ =` discards on `munmap`/`madvise`.** Each carries the "why" comment `#719` required.
  V1/V2/V4 are not about the missing comment — they are about the *premise* those comments
  rest on ("every caller already establishes alignment") being false on non-4 KiB-page hosts.
* **`is_empty`'s deprecation, `#[must_use]` coverage, `Send`-not-`Sync`, the `off_t` width note,
  the `_SC_PAGESIZE` per-OS table.** All read; all as previously decided. The BSD `_SC_*` values
  were not independently re-derived from vendor headers here — that would need the four BSD
  source trees, and `#714` already recorded them as REASONED-FROM-SPEC.
* **README 0.1→0.2 migration section.** Known-open, excluded by this review's brief.
* **Packaging.** `bench-scale-tool 0.1.0` is on crates.io with zero transitive dependencies, so
  the dev-dependency added by `#758` neither blocks `cargo publish` nor falsifies the
  "zero dependencies, 100 % Rust" claim.

---

## Recommended order

1. **V1** — the unbounded VA leak. Prefer fix option 3 (stop trimming), which also delivers P18's
   free syscall win and makes the bug class unreachable rather than guarded.
2. **V2** — validate decommit offsets against `page_size()`, and correct the "partial
   decommit" wording; the stale-data consequence is the part that must reach the rustdoc.
3. **V3** — either fix the Windows large-page call shape or stop claiming `MEM_LARGE_PAGES`
   works. Do not publish 0.2.0 with the current text unchanged.
4. **V4** — document (or reject) `decommit` on huge-page reservations. Same session as V3.
5. **V10, V17, V7** — three small, obviously-correct edits (two stale doc statements, one
   duplicated function, one missing `Debug`). Batchable in minutes.
6. **V18** — add `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` to
   `ci.yml`, giving this crate the release gate `sefer-region` already has and compiling the
   bench for the first time.
7. **V5** — fix the two leaking `from_raw_parts` tests, then decide whether a
   `miri -Zmiri-ignore-leaks` row for this package is worth a CI job; add the
   global-allocator-under-miri caveat sentence either way.
8. **V6, V8** — the two API decisions that are cheap now and breaking later. Both should be
   settled *before* the 0.2.0 publish, not after.
9. **V14, V15** — the two structural de-duplications. Best done immediately after V1/V2, since
   those fixes touch exactly the triplicated predicate.
10. **V20 (P17)** — run the Unix hit-rate measurement on WSL/Linux (existing counters, existing
    bench, no new instrumentation) and act on the number rather than on the bound.
11. **V21 (P18)** — the Windows single-call reservation for `align ≤ 64 KiB`, measured against
    the existing 64 KiB bench arm with `WINDOWS_RESERVE_COMMIT_CALLS` as the oracle.
12. **V9, V11, V12, V13, V16, V19** — polish, ergonomics and one deferred refactor option;
    none blocks anything.
