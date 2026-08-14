# `aligned-vmem` — independent pre-release review (round 2)

**Date:** 2026-08-14
**Reviewer:** independent blind read (no prior review docs, no commit history consulted)
**Target:** `crates/vmem/` (crate name `aligned-vmem`), all of `src/`, `tests/`, `benches/`,
`examples/`, `Cargo.toml`, `README.md`.
**Method:** full read of every listed file plus `cargo check`/`clippy`/`test`/`doc` under several
feature combinations. No repository file was modified by this review.

---

## Executive summary

**Verdict: ship-able, with one fix I would block on.**

This is a careful crate. The `unsafe` discipline is real, not decorative: every block has
a `// SAFETY:` argument that actually corresponds to the code beneath it, error codes are
captured immediately after the failing syscall and before cleanup FFI, strict provenance
(`.addr()` / `.with_addr()`) is used consistently instead of integer round-trips, every
early return between a successful reserve and the function exit releases exactly once, and
the FFI declarations, struct layouts and per-OS constants I checked are all correct. The
test suite is substantial (≈2,400 lines for ≈3,000 lines of implementation) and much of it
carries a written non-vacuity argument — the `from_raw_parts` `#[should_panic]` family and
the two-sided fault-injection concurrency oracle are better than what most released crates
ship. `cargo clippy -D warnings` and `cargo test` are green on the review host under both
`--all-features` (59 tests) and the publication feature set (54 tests).

**The one blocking finding is W-1:** on Windows, `Reservation::is_huge()` reports `true`
after the `MEM_LARGE_PAGES` request failed and the ordinary-page retry succeeded
(`src/lib.rs:1784` derives the flag from what was *requested*, not from which call
produced the pointer). I reproduced this on the review host — `reserve_aligned_huge(64 KiB,
64 KiB)` returns `is_huge() == true` on a size that can *never* be a large-page allocation
on x86_64. `is_huge()` exists for exactly one purpose, stated identically in three rustdoc
blocks and the README: to tell a real grant apart from the fallback. On the one
platform/feature combination where the flag is supposed to mean something, it answers the
opposite of the truth on the common (unprivileged-process) path. The fix is three lines;
the reason to block is that this is a *published API contract* being wrong at 0.2.0, and
the sibling Unix code already does it correctly.

**The most valuable non-blocking finding is P-1**, and it is unusual in that the crate has
already measured and written down the conclusion without acting on it: the Unix exact-size
`mmap` fast path costs `3 − 2p` syscalls per reservation, and at the crate's own measured
hit rates (34.4%–56.7%) that is 1.87–2.31 syscalls versus a flat 1.0 without it. On 64-bit
Unix it is a pure pessimization whose only benefit — address-space economy — matters on
32-bit. Either gate it on `target_pointer_width = "32"` or give `mmap` an aligned address
hint (P-2) so the hit rate actually approaches 1. Both are small changes on the crate's
hottest path.

Four things I would fix in the same pass because they are one-liners in a pre-publication
window: `fault-injection = ["lazy-commit"]` (F-1 — the feature is currently *inert* without
it), the `mock::record` reentrancy guard (M-1 — a `GlobalAlloc` consumer testing with
`mock` panics inside its own allocator), documenting the BSD decommit no-op (U-1/D-2 —
the README's "Platform caveats" section enumerates three divergences and omits the four
targets whose constants the crate ships), and documenting the
`decommit`-uses-`page_size()`-but-`recommit`-uses-`PAGE` asymmetry (A-1).

Nothing I found is a memory-safety defect. The soundness review came back clean: no
double-free path, no leak path, no provenance violation, no FFI signature mismatch, no
`debug_assert!` guarding an invariant that matters in release (the two places where that
temptation existed — the Windows fast-path alignment premise and the Unix `_SC_PAGESIZE`
premise — are both deliberately real runtime checks, with the reasoning written out). The
findings below are contract, documentation, feature-wiring, test-oracle and performance
issues.

**Method note.** This was a blind read: no prior review documents, no `CHANGELOG.md`, no
open-items indexes, and no git history were consulted. Every finding is derived from the
current source, its tests, and commands run against it (`cargo clippy`, `cargo test`, and
one scratch consumer crate built *outside* the repository to reproduce W-1). No file in
the repository was modified by this review.

---

## Sections

1. Manifest & feature surface
2. Windows backend
3. Unix backend
4. Public API, `Reservation`, miri backend
5. Mock backend & fault injection
6. Tests — coverage assessment
7. Benchmarks & examples
8. Documentation accuracy
9. Performance (real hot paths only)
10. Smaller items & general improvements
11. Findings index by severity

---

## 1. Manifest & feature surface (`crates/vmem/Cargo.toml`)

The manifest is unusually well documented — the `mock` feature's Cargo-unification
hazard note (`Cargo.toml:62-86`) is exactly the kind of warning most crates omit, and
the `docs.rs` feature list (`:26-28`) deliberately excludes `mock`/`bench-internals`
for good stated reasons. Findings here are small.

### F-1 (MEDIUM) — `fault-injection` is silently inert without `lazy-commit`

`Cargo.toml:99` declares `fault-injection = []` with no dependency on `lazy-commit`.
Its only consumer is `try_commit_range` (`src/lib.rs:1416-1426`), which is itself
`#[cfg(feature = "lazy-commit")]` (`src/lib.rs:1391`). So
`--features fault-injection` alone compiles cleanly, exposes the whole public
`fault_injection` module (`arm_fail_next`, `arm_fail_at`), and **arming a fault
does nothing at all** — `should_fail_commit` is never called and is explicitly
dead-code-allowed for exactly that combination (`src/fault_injection.rs:121-130`).

*Concrete failure:* a consumer writes an OOM test — `aligned_vmem::fault_injection::arm_fail_next(1)`
then asserts its own allocator surfaces an OOM — builds with `aligned-vmem = { version = "0.2",
features = ["fault-injection"] }`, and the test passes vacuously *in the wrong direction*
(no fault is ever injected, the commit succeeds, and if the assertion is `assert!(result.is_err())`
the test fails with a confusing message; if it is a "did not crash" style test it passes
while testing nothing).

*Fix:* `fault-injection = ["lazy-commit"]`. This is a one-line manifest change that is
purely additive from a consumer's perspective and is best made **before** 0.2.0 locks the
feature graph.

### F-2 (LOW) — `mock` silently disables `fault-injection`

When both features are on, `try_commit_range` takes the `#[cfg(feature = "mock")]` arm
(`src/lib.rs:1400-1408`) and the `fault_injection` hook in the `not(mock)` arm
(`:1416-1426`) is compiled out. `src/fault_injection.rs:118-120` records this in a code
comment, but neither the `fault-injection` feature doc in `Cargo.toml:89-99` — which
stresses that the two are "DISTINCT" — nor the `fault_injection` module rustdoc
(`src/fault_injection.rs:1-16`) tells a consumer that enabling `mock` anywhere in the
build graph (which, per the manifest's own unification warning, can happen *without the
consumer asking for it*) turns their real-path fault injection into a no-op. Given the
unification hazard the manifest already documents, this is the exact combination most
likely to occur accidentally.

### F-3 (LOW) — `fault-injection` cannot reach `recommit`, only `commit_range`

`should_fail_commit` is consulted from `try_commit_range` only. `try_recommit`
(`src/lib.rs:1303-1324`) has no real-path hook — its only fault path is `mock`
(`:1310-1318`), which replaces the backend. So the single most interesting real-backend
failure in this crate — *Windows `VirtualAlloc(MEM_COMMIT)` refused on recommit after a
decommit, where a caller that ignores the `false` return then writes and takes a
`STATUS_ACCESS_VIOLATION`* — has **no** deterministic test seam at all. The `recommit`
rustdoc (`:1274-1277`) documents the `false`-means-do-not-write contract precisely, and
`tests/smoke.rs:290-320` says in as many words "We cannot portably force a commit-charge
failure without an FFI test seam". The seam exists; it just was not wired to this
function. Adding the same two-line hook to `try_recommit` closes it.

### F-4 (INFO) — `bench-internals` counters are process-global, and one accessor's doc overstates

`windows_reserve_commit_calls()` (`src/lib.rs:311-314`) sums the two path counters. That
is correct as a *per-call* total (each `win_reserve_commit` call increments exactly one of
the two — verified: the single-call path increments only on the aligned-success return at
`:1776`, and the alignment-miss fall-through reaches the two-call increment at `:1897`
or `:1914`). No defect; noted only because the doc phrase "both paths combined" could be
read as "syscalls", which it is not (the fast path's large-page retry issues 2 syscalls
and counts 1 — correctly documented at `:236-240`).

---

## 2. Windows backend (`VirtualAlloc` / `VirtualFree`)

### W-1 (HIGH) — `is_huge()` reports `true` after the large-page request failed and the ordinary-page retry succeeded

`src/lib.rs:1715-1785`, specifically the interaction of `:1731-1750` with `:1784`.

The single-call fast path issues
`VirtualAlloc(NULL, commit_len, MEM_RESERVE|MEM_COMMIT|extra_commit_flags, PAGE_READWRITE)`.
If that returns NULL **and** `extra_commit_flags != 0` (i.e. `MEM_LARGE_PAGES` from
`reserve_aligned_huge_raw`, `:2008`), it retries with plain `MEM_RESERVE|MEM_COMMIT`
(`:1737-1742`) and binds the result to the same `base`. The success return is then:

```rust
return Ok((base, base, commit_len, extra_commit_flags != 0));   // lib.rs:1784
```

`granted_huge` is derived from **what was requested**, not from which of the two calls
actually produced `base`. After the retry, `extra_commit_flags` is still `MEM_LARGE_PAGES`,
so the reservation is stamped `granted_huge = true` while running on ordinary 4 KiB pages.

*Concrete failure scenario:* Windows, `--features huge-pages`, an ordinary (non-elevated,
no `SeLockMemoryPrivilege`) process — which is the **default** state for every Windows
process; the privilege must be granted explicitly via secpol/`AdjustTokenPrivileges`.
Call `reserve_aligned_huge(2 * 1024 * 1024, 64 * 1024)`. The first `VirtualAlloc` fails
with `ERROR_PRIVILEGE_NOT_HELD` (1314), the retry succeeds with normal pages, and
`r.is_huge()` returns `true`.

*Why this is HIGH:* `is_huge()` exists for exactly one purpose, stated three times in the
rustdoc — "This is the 'best-effort' observable: a caller … can now detect whether the
huge-page feature actually engaged, rather than receiving only an indistinguishable
`Ok(Reservation)` on every fallback" (`:610-613`, mirrored at `:481-484` and in
`reserve_aligned_huge`'s doc at `:1544-1545`, "If any of these conditions fail, the
function falls back to ordinary pages and returns a reservation with
`Reservation::is_huge` == `false`"). On the single platform/feature combination where the
flag is supposed to be meaningful, it reports the opposite of the truth on the *common*
path. The in-code comment at `:1779-1783` reasons about Windows "stripping the flag
silently" but does not notice that the function's own retry branch is the thing that
strips it.

Note the Unix half gets this right — `unix_reserve`'s ordinary-page retry explicitly sets
`granted_huge = false` (`:2209`) — which makes the Windows behaviour an inconsistency
between the two halves as well as a bug.

*Fix:* track which call produced `base` (a `let mut huge_granted = extra_commit_flags != 0;`
set to `false` in the retry arm) and return that instead of `extra_commit_flags != 0`.

**EMPIRICALLY CONFIRMED** — not reasoned. Reproduced on the review host (Windows 10 Pro
19045, x86_64, ordinary unprivileged user process) via a scratch consumer crate outside
the repository that path-depends on `crates/vmem` with `features = ["huge-pages"]`:

```
reserve_aligned_huge(64K,64K): ok base=0x16bd7780000 len=65536 reservation_len=65536 is_huge=true
reserve_aligned_huge(4M,4M):   is_huge=false reservation_len=8388608
```

The 64 KiB case is airtight independent of privileges: `GetLargePageMinimum()` is 2 MiB on
x86_64, so a 64 KiB `MEM_LARGE_PAGES` request **cannot** succeed on any Windows host — the
first `VirtualAlloc` necessarily failed, the ordinary-page retry necessarily produced the
returned pointer, and `is_huge()` still answers `true`. The 4 MiB case returns `false`
only because `align > 64 KiB` routes it to the two-call path, which never sets the flag at
all.

*Why the existing tests miss it:* `tests/huge_pages.rs:155-177`
(`reserve_aligned_huge_64k_single_call_path`) is the one test that exercises this exact
path, and it explicitly declines the oracle — "We do NOT assert on `is_huge()` because
privilege availability varies: some hosts have `SeLockMemoryPrivilege` and genuinely grant
huge pages, most don't and fall back to ordinary pages. Either behavior is correct under
the crate's best-effort contract" (`:173-176`). That reasoning is wrong for this size:
64 KiB can never be a large-page allocation, so `false` is the only correct answer here
and the assertion was safe to make. The sibling test's
`#[cfg(not(target_os = "linux"))] assert!(!r.is_huge())` (`tests/huge_pages.rs:61-62`)
does hold on Windows only because it uses `align = 4 MiB`, which routes around the bug.

### W-2 (LOW/MEDIUM) — the two-call path's "retry with ordinary pages" is a verbatim duplicate of the call that just failed

`src/lib.rs:1886-1899`. The first commit is:

```rust
let committed = unsafe { VirtualAlloc(base.as_ptr().cast(), commit_len, MEM_COMMIT, PAGE_READWRITE) };
```

— note `MEM_COMMIT` alone; `extra_commit_flags` is **not** OR-ed in on this path (by
design, per `:1915-1920`). The failure handler then does:

```rust
if extra_commit_flags != 0 {
    // Best-effort large pages: retry the commit with ordinary pages.
    let plain = unsafe { VirtualAlloc(base.as_ptr().cast(), commit_len, MEM_COMMIT, PAGE_READWRITE) };
```

which is byte-for-byte the same call. The comment describes a fallback semantic that does
not exist. Consequences: (a) on a genuine commit-charge exhaustion with
`huge-pages` enabled and `align > 64 KiB`, the crate burns a second guaranteed-failing
`VirtualAlloc` before giving up; (b) the `VmemError` finally reported is captured after
the *second* failure, so a reader of the code cannot tell that the two are the same call;
(c) a future maintainer reading `:1889` will believe large pages are attempted on the
two-call path. Dead branch — delete it, or make it actually differ.

### W-3 (INFO / verified-good) — release, provenance, and fit arithmetic

Checked and found correct, recorded so the null result is on the record:

- `release_reservation` (`:1924-1930`) passes `dwSize = 0` with `MEM_RELEASE`, and always
  receives `self.reservation` (the region base, not the aligned base) — correct for both
  the single-call path (where `reservation == base`, `:1784`) and the two-call path
  (`:1921`).
- Every early-return between a successful `MEM_RESERVE` and the function's exit releases
  the region exactly once (`:1772`, `:1820`, `:1875`, `:1904`, `:1910`); no path leaks a
  reservation and none double-releases (the fall-through at `:1772-1773` releases and then
  re-reserves from scratch).
- The aligned base is derived with `region_ptr.with_addr(base_addr)` (`:1882`), preserving
  provenance over the whole `over`-byte region rather than an integer-to-pointer cast.
- `over = size + align` always admits a fit: `align_up_addr(region_addr, align) - region_addr < align`,
  so `base_addr + size <= region_addr + align + size = region_end` (`:1862-1866`). The
  `None` arm is genuinely unreachable, and it correctly classifies as `invalid_argument`
  rather than reading a meaningless `GetLastError`.
- `SYSTEM_INFO`'s `#[repr(C)]` layout (`:2026-2038`) matches the Win32 header field order
  and widths (`WORD, WORD, DWORD, LPVOID, LPVOID, DWORD_PTR, DWORD, DWORD, DWORD, WORD, WORD`),
  including the `DWORD_PTR` (`usize`) mask field; only `dwPageSize` and
  `dwAllocationGranularity` are read. `MEM_COMMIT`/`MEM_RESERVE`/`MEM_DECOMMIT`/`MEM_RELEASE`/
  `MEM_LARGE_PAGES`/`PAGE_READWRITE` constants (`:2061-2076`) are all correct.
- `extern "system"` is the right ABI for the Win32 functions on both `x86_64-pc-windows-msvc`
  and `i686-pc-windows-msvc` (where it means `stdcall`).

---

## 3. Unix backend (`mmap` / `munmap` / `madvise`)

### U-1 (MEDIUM) — `decommit` / `decommit_lazy` are effectively no-ops on all four BSDs, and this is undocumented

`madv_free_advice()` (`src/lib.rs:2451-2464`) resolves to `MADV_DONTNEED` for every target
that is not Linux/macOS/iOS — which, given the `MAP_ANON` cfg list (`:2495-2509`), means
FreeBSD, DragonFly, NetBSD, OpenBSD, tvOS and watchOS. `decommit`'s eager path uses
`MADV_DONTNEED` (`:2381`) on every Unix.

On the BSDs, `MADV_DONTNEED` for anonymous memory does not free pages — it only
deprioritises them (FreeBSD `madvise(2)`: "`MADV_DONTNEED` — Allows the VM system to
decrease the in-memory priority of pages"; the page-freeing advice is `MADV_FREE`, value 5
on FreeBSD/DragonFly and 6 on NetBSD/OpenBSD). So on all four BSDs **both** `decommit` and
`decommit_lazy` silently fail to return any physical memory, exactly the shape of the
Darwin gap the crate documents at length (`:1163-1178`, `:1215-1235`).

The asymmetry is the finding: the Darwin caveat gets ~16 lines of rustdoc on `decommit`,
a second block on `decommit_lazy`, a scoped-off test assertion
(`tests/smoke.rs:273-285`), and a `bench-internals` oracle — while the BSDs, which the
crate explicitly claims to support by shipping their `MAP_ANON` and `_SC_PAGESIZE`
constants, get no mention anywhere in the public docs. A consumer reading `decommit`'s
rustdoc on FreeBSD is told "return their physical backing to the OS … Re-access after
decommit produces fresh zero-filled pages … implicitly on Linux" with only a Darwin
exception listed.

*Concrete failure:* an allocator on FreeBSD calls `decommit` on its idle segments to shed
RSS under pressure; RSS does not drop; nothing reports an error (`libc_madvise` discards
the return value by design, `:2806`), and the caller's memory-pressure logic is silently
inoperative.

*Fix (cheap, correctness-improving, and independently worth doing):* extend
`madv_free_advice()` with BSD arms (`MADV_FREE` = 5 on FreeBSD/DragonFly, 6 on
NetBSD/OpenBSD) so at least `decommit_lazy` really frees, and add a one-paragraph
"non-Linux Unix" caveat to `decommit`'s rustdoc covering the BSDs alongside Darwin.
If the constants cannot be verified on hardware, the doc half alone still closes the
worse problem (a documented promise the code does not keep).

### U-2 (LOW) — Android is named as a supported target in one place and rejected by `compile_error!` in another

`src/lib.rs:2700-2716` defines `OffT` with an arm explicitly covering
`target_os = "android"` and a comment reasoning about bionic's 32-bit `off_t`
("Android's bionic is REASONED-FROM-SPEC …"). But Android is `cfg(unix)` and is *not*
`target_os = "linux"`, so it matches neither `MAP_ANON` arm (`:2493-2509`) and instead
hits the `compile_error!` at `:2526-2547` — `aarch64-linux-android` cannot build this
crate at all. The `OffT` reasoning is therefore dead, and a reader is left with two
contradictory statements about Android support. Either add an Android arm to `MAP_ANON`
(the Linux values apply) or drop Android from the `OffT` comment.

### U-3 (LOW) — the exact-size fast path's alignment guarantee is checked, but the over-reserve path's is not (asymmetric defence)

`try_reserve_aligned_exact` deliberately re-checks `region_addr.is_multiple_of(align)`
unconditionally at `:2332`, with a long justification (`:2308-2331`) that a wrong
`_SC_PAGESIZE` on an unverified BSD must not be able to produce a misaligned base in a
release build. Good. The over-reserve path computes `base_addr` from `align_up_addr`
(`:2223`) and then asserts nothing — which is fine because the arithmetic is
self-evidently correct, but it does mean the crate's alignment guarantee rests on two
different mechanisms with two different strengths. Not a defect; noted for symmetry with
the Windows path, which *does* re-check its computed base (`:1768`). A one-line
`debug_assert!(base_addr.is_multiple_of(align))` after `:2228` would make the intent
uniform at zero release cost.

### U-4 (INFO / verified-good) — constants, provenance, and the huge-page munmap-alignment argument

Checked against each OS's headers and found correct:

- `PROT_READ`/`PROT_WRITE`/`MAP_PRIVATE` (`:2467-2471`), `MAP_ANON` = `0x20` on Linux and
  `0x1000` on Darwin/BSD (`:2493-2509`), `MAP_FAILED` = `usize::MAX` (`:2608`),
  `MADV_DONTNEED` = 4 (all listed targets), `MADV_FREE` = 8 (Linux),
  `MADV_FREE_REUSABLE` = 7 (Darwin), `MADV_HUGEPAGE` = 14 (Linux),
  `MAP_HUGETLB` = `0x40000` and `MAP_HUGE_2MB` = `21 << 26` (Linux) — all correct, and
  `21 << 26` fits `i32` without overflow.
- `_SC_PAGESIZE`'s per-OS table (`:2647-2680`) is the right *shape* (this really is a
  per-OS name table, not a POSIX constant) and the Darwin value 29 / Linux value 30 are
  correct. The BSD values (47/28) are marked REASONED-FROM-SPEC and are additionally
  defended by `page_size()`'s `>= PAGE && is_power_of_two()` guard (`:404-408`) and by
  U-3's unconditional alignment recheck — a defence-in-depth chain that is genuinely
  sound: a wrong value cannot produce a misaligned reservation, only a suboptimal
  `page_size()`.
- The `MAP_HUGETLB` `munmap`-alignment argument (`:2142-2168`) holds: requiring both
  `size` and `align` to be 2 MiB multiples (`:2175-2181`) makes `over = size + align` a
  2 MiB multiple, and the single whole-mapping `munmap` at the kernel-guaranteed
  huge-aligned `region_ptr` is conformant. The exact-size path's miss `munmap`
  (`:2334`) unmaps `size` at `region_ptr`, also conformant.
- Strict provenance is used consistently: `.addr()` for reads (`:2222`, `:2307`, `:2759`)
  and `.with_addr()` for the derived base (`:2242`); no exposed `usize`→pointer casts
  remain on either backend.

---

## 4. Public API surface, `Reservation`, and the miri backend

### A-1 (MEDIUM) — `decommit`/`recommit` validate against *different* constants, and only one of them is the real page size

- `decommit` / `decommit_lazy` validate `start`/`end` against `page_size()`
  (`src/lib.rs:1180-1183`, `:1248-1251`).
- `recommit` / `try_recommit` / `commit_range` / `try_commit_range` validate against the
  compile-time `PAGE` = 4 KiB (`:1304`, `:1394`).

The asymmetry in *behaviour on violation* (silent skip vs. rejection) is deliberate,
well-argued, tested, and documented in `README.md:100-106`. The asymmetry in the
*granularity constant* is not explained anywhere and is the more consequential half.

*Concrete failure:* Apple Silicon macOS (16 KiB pages). A caller follows the `recommit`
rustdoc, which says "`start`/`end` must be multiples of [`PAGE`]" (`:1283`), and works in
4 KiB units. `decommit(base, PAGE, 2*PAGE)` is silently discarded (`4096 % 16384 != 0`),
`recommit(base, PAGE, 2*PAGE)` returns `true`. The caller believes it decommitted and
recommitted a page; nothing happened, no error was reported, and RSS never dropped. The
crate's own test suite proves it knows this can happen — `tests/smoke.rs:698-718` has a
`if page_size() > PAGE { … }` arm asserting the decommit half is skipped — but no rustdoc
on `recommit`/`commit_range` tells a consumer that the *commit* side accepts offsets the
*decommit* side rejects.

Today this cannot corrupt memory (on Windows `page_size() == PAGE`, and on Unix
`recommit` is a no-op, `:2389-2408`), so it is MEDIUM rather than HIGH — but it is a
latent trap: a future Unix decommit implementation that made `recommit` do real work would
turn it into a real one. Minimum fix is documentation; better is to validate both sides
against `page_size()` (the two are the same value on every host where `recommit` currently
does anything at all, so this is behaviour-preserving in practice).

### A-2 (LOW) — `decommit`'s doc says "within the span", but nothing can check it

`decommit`/`decommit_lazy`/`recommit`/`commit_range` all take a bare `base: *mut u8` with
no length, so `end <= len` is unenforceable and is (correctly) pushed into the `# Safety`
contract. Worth calling out as an *API ergonomics* item before the API locks: the whole
family would be both safer and more convenient as inherent methods on `&Reservation`
(`r.decommit(start, end)`), where `len` is known and the bounds check is free and safe.
The free functions must stay for the `into_parts` self-hosted flow, but adding the
`Reservation` methods is purely additive and removes the single most likely consumer
mistake (an out-of-range `end`) from the unsafe surface entirely. This is exactly the
kind of change that is cheap now and semver-awkward later.

### A-3 (LOW) — `Reservation::is_empty` is deprecated but still shipping in a pre-1.0 crate

`src/lib.rs:545-553`. The deprecation note is honest and correct ("always `false` for any
valid instance"). Since 0.2.0 is not yet published and the item is dead-by-construction,
deleting it outright is strictly better than shipping a `#[deprecated]` item that must
then be carried until 1.0. Note also that leaving it in means every consumer who writes
the idiomatic `len()`/`is_empty()` pair gets a clippy `len_without_is_empty` interaction
to reason about for no benefit.

### A-4 (INFO / verified-good) — `from_raw_parts`, `into_parts`, `Drop`, and `Send`

- The `assert!` at `:805-828` checks eight documented invariants and the ordering is
  correct: `base_addr - res_addr` (`:815`) is evaluated *after* `base_addr >= res_addr`
  (`:812`) in the same `&&` chain, so a `base < reservation` violation reaches the
  informative message instead of a debug-build subtraction overflow. `tests/smoke.rs`
  has a dedicated `#[should_panic]` test for each of the seven checkable clauses
  (`:761-1024`) — this is genuinely thorough coverage, and each test releases the real
  reservation it borrowed before re-raising, so it does not leak under miri.
- `into_parts` (`:655-659`) and `into_reservation_parts` (`:674-688`) both
  `core::mem::forget(self)`; neither can double-free.
- `Drop` (`:840-854`) releases `self.reservation`/`self.reservation_len` — the *full*
  mapping, matching what every backend returned.
- `unsafe impl Send` is justified and pinned by a compile-time `assert_send::<Reservation>()`
  (`tests/smoke.rs:39-40`); `Sync` is correctly *not* implemented (auto-`!Sync` via
  `NonNull`).
- Miri backend (`:2841-2864`): `alloc`/`dealloc` with a `Layout` reconstructed from
  `(reservation_len, align)` — the pair is exactly what `reserve_aligned_raw` allocated
  with (`:2847`, `:2852` return `size` as `reservation_len`), so the layout round-trip is
  exact.
- `leak_zeroed_pages` (`:1614-1638`): the `#[cfg(miri)] write_bytes` closes the one
  backend that does not hand back zeroed memory, and the round-up uses `checked_add`
  (`:1618`). Correct.

---

## 5. Mock backend & fault injection

### M-1 (MEDIUM) — `mock::record` is reentrancy-unsafe, which breaks the crate's headline "safe inside `GlobalAlloc`" claim under that feature

`src/mock.rs:244-246`:

```rust
pub(crate) fn record(call: Call) {
    CALLS.with(|c| c.borrow_mut().push(call));
}
```

The `RefMut` guard is live across `Vec::push`. When the `Vec` needs to grow, `push`
allocates through the global allocator.

*Concrete failure:* a consumer whose `#[global_allocator]` is implemented on top of
`aligned-vmem` (the crate's stated primary use case — `README.md:112-114`: "never a panic,
so this is safe to call from inside a `GlobalAlloc::alloc` body") runs its test suite with
`--features aligned-vmem/mock`. Inside `GlobalAlloc::alloc` it calls `reserve_aligned` →
`mock::record` → `CALLS.borrow_mut()` → `Vec::push` → realloc → global allocator →
`reserve_aligned` → `mock::record` → `CALLS.borrow_mut()` **while the outer guard is
still held** → `panic!("already mutably borrowed: BorrowMutError")` inside an allocator.

The crate already recognises exactly this hazard for the *miri* backend and documents it
prominently in the module header (`src/lib.rs:8-12`: "A consumer that installs itself as
`#[global_allocator]` cannot use this crate under miri, because the miri backend routes
allocations through the global allocator and would create a reentrancy hazard"). The
identical hazard in `mock` is neither documented nor guarded.

*Fixes, cheapest first:* (a) document it next to the existing miri sentence; (b) build the
`Call` and drop the borrow before pushing is not enough — the push itself is the
allocating step, so the real fix is a reentrancy guard (a thread-local `bool` that makes
`record` a no-op while already recording) or `try_borrow_mut().ok()` so the reentrant
call is dropped instead of panicking.

### M-2 (LOW) — `mock::record` can panic during thread teardown

Same function. `Reservation::drop` calls `mock::record` (`src/lib.rs:843-847`). If a
`Reservation` is owned by a `thread_local!`, its destructor runs during TLS teardown, in
unspecified order relative to `CALLS`'s own destructor. `LocalKey::with` panics if the
value is already destroyed — a panic in `Drop`, i.e. an abort if anything else is
unwinding. `try_with(...).ok()` is a one-word fix and is the standard idiom for exactly
this.

### M-3 (INFO / verified-good) — fault-injection atomics

`src/fault_injection.rs` is the strongest file in the crate. `FAIL_NEXT`'s decrement is a
genuine `fetch_update` RMW (`:138-146`), the lazy `then(|| next - 1)` avoids the eager
underflow panic `then_some` would introduce, and the `Release`/`Acquire` pairing between
`arm_fail_at`'s counter-reset-then-target-store (`:110-113`) and `should_fail_commit`'s
target load (`:152`) is correct and correctly explained. The remaining disarm-vs-rearm
race is explicitly scoped out in the module doc (`:47-57`) rather than hidden. The
concurrency test (`tests/fault_injection.rs:197-271`) arms *half* the calls precisely so
the oracle is two-sided, and its doc records that the previous one-sided oracle was
verified incapable of catching the bug — this is unusually rigorous test design.

One residual: `FAIL_AT_COUNTER.fetch_add` (`:154`) never resets while a target is armed
but unreached, so at `2^32` commits it wraps and could spuriously fire. Purely theoretical
(INFO).

---

## 6. Tests — coverage assessment

The suite is large (≈2,400 lines of tests for ≈3,000 lines of implementation) and, unusually,
most tests carry a written argument for *why they are not vacuous*. Several are genuinely
excellent: the `from_raw_parts` `#[should_panic]` family, the two-sided fault-injection
concurrency oracle, and `decommit_contract_violation_never_reaches_madvise`
(`tests/smoke.rs:659-734`), which uses the `bench-internals` madvise counter as a real
path-activation oracle rather than trusting the call log.

Gaps found:

### T-1 (MEDIUM) — nothing verifies that `reserve_aligned_lazy` actually leaves the tail uncommitted

Every test in `tests/lazy_commit.rs` would pass verbatim against an implementation in
which `reserve_aligned_lazy_raw` simply forwarded to the eager path — which is *literally
what the Unix (`src/lib.rs:2423-2429`), miri (`:2913-2920`) and `mock` (`:1486-1487`)
implementations do*. The feature's entire purpose is saving Windows commit charge, and no
test on any platform asserts that any commit charge was saved, or even that the two-call
path was taken.

The oracle already exists and is unused: `windows_reserve_commit_two_call_pairs()` /
`windows_reserve_commit_single_calls()` (`src/lib.rs:322-334`). A `#[cfg(all(windows,
feature = "bench-internals"))]` test asserting that `reserve_aligned_lazy(span, span,
PAGE)` increments the *two-call* counter and not the single-call one would pin the
`commit_len == size` guard at `:1715` — the guard whose absence, per its own comment
(`:1699-1714`), silently shrank reservations to `initial_commit` bytes and was caught only
by a hand-built repro during review. That is a real regression that shipped once; nothing
currently prevents it from shipping again.

### T-2 (MEDIUM) — `is_huge()` has no test that can fail on Windows (see W-1)

Covered in detail under W-1. Summary: `tests/huge_pages.rs:155-177` reaches the buggy path
and deliberately asserts nothing about `is_huge()`, on reasoning that does not hold for a
64 KiB request.

### T-3 (LOW) — no test exercises `decommit`/`recommit` on an over-reserved (base != reservation) span

Every decommit/recommit test uses `size == align`, and on Windows those take either the
single-call path (`base == reservation`) or a fast-reserve where the candidate is already
aligned. A test using `align = 4 * size` (so `base > reservation` by a nonzero head
offset) would exercise `base.add(start)` arithmetic against a genuinely offset base. The
arithmetic is simple enough that this is LOW, but it is the one structural shape the
decommit tests never see.

### T-4 (LOW) — `release(NULL, …)` early-return is documented but untested

`src/lib.rs:1072-1080` documents a null-pointer no-op *and* its mock-log side effect ("the
mock recorder is also skipped in this case, so a `mock`-based test's expected call log may
desync"). Neither half has a test. One three-line test in `tests/mock.rs` would pin both.

### T-5 (LOW) — `PAGE_SIZE_CACHE`'s fallback branch is unreachable in tests

`page_size()`'s guard (`src/lib.rs:404-408`) — the defence against a wrong `_SC_PAGESIZE`
on an unverified BSD, which is the *stated reason* the guard exists — cannot be exercised
without injecting a bogus `query_os_page_size`. `tests/smoke.rs:429-440` only checks the
happy path's invariants, and its own sibling comment (`:442-453`) admits the generic test
"structurally cannot" catch a wrong constant. Extracting the validation into a small pure
function (`fn validated_page_size(queried: usize) -> usize`) and testing *that* directly
would make the guard testable on every host at zero runtime cost.

### T-6 (INFO) — feature-combination coverage

`fault-injection` without `lazy-commit` (see F-1) is compiled by the workspace's
feature-powerset CI but has no behavioural test, because there is nothing to test — the
feature is inert. `mock` + `fault-injection` is explicitly excluded by
`tests/fault_injection.rs:15-19` with a correct justification. Both are consistent with
the findings above rather than separate defects.

*Verified locally on the review host:* `cargo test -p aligned-vmem --all-features` →
59 passed / 0 failed across 9 binaries, but note that `tests/fault_injection.rs` compiles
to **0 tests** under `--all-features` (because `mock` is on). `.github/workflows/ci.yml`
covers this correctly with a separate `--features "lazy-commit huge-pages fault-injection
bench-internals"` row (`:789`, `:823`, `:920`), so this is not a live coverage hole — but
it is worth knowing that the single most "complete-looking" invocation is precisely the
one that skips the fault-injection suite.

---

## 7. Benchmarks & examples

Both files are honest and non-vacuous, and both carry more caveat text than most crates'
entire benchmark suites. Two observations, neither a defect:

### B-1 (INFO) — the bench never exercises the Windows two-call path at the size it matters

`benches/vmem_bench.rs:39-43` fixes `RESERVE_SIZE = RESERVE_ALIGN = 64 KiB`, which on
Windows is exactly the single-call fast path boundary (`align <= WIN_ALLOCATION_GRANULARITY`).
Three of the four arms therefore measure only the fast path. The fourth
(`reserve_release_1mb`, `:143-155`) does hit the two-call path. Since the crate's flagship
use case is a 4 MiB-aligned allocator segment (the example file calls it exactly that,
`examples/v20_849_unix_exact_reserve_hit_rate.rs:68-72`), and 4 MiB is the *expensive*
Windows shape (`size + align` over-reserve + two syscalls), an arm at that size would
measure the case a consumer actually pays for.

### B-2 (INFO) — the example's methodology note is exemplary and should not be lost

`examples/v20_849_unix_exact_reserve_hit_rate.rs:14-24` and `:31-45` document that a
single-process run of the hit-rate measurement is **one Bernoulli trial, not `ITERS`
trials** (the ASLR `mmap_base` draw fixes the alignment residue for the whole process),
and that an alloc-then-immediately-free loop measures one address class repeatedly and
yields a spurious 100%. Both are exactly the kind of measurement trap that normally
produces a wrong published number. Recorded here as a positive finding.

---

## 8. Documentation accuracy (rustdoc + README)

### D-1 (HIGH, same root cause as W-1) — three documents state a guarantee the Windows code does not keep

`README.md:48` ("`Reservation::is_huge() -> bool` | Detect whether a reservation actually
got large/huge pages on either platform"), `src/lib.rs:605-633` and `src/lib.rs:1536-1550`
all promise that `is_huge()` distinguishes a real grant from the ordinary-page fallback.
On the Windows single-call path it does not (W-1, empirically confirmed). The README line
is the strongest claim of the three because it is unqualified.

### D-2 (MEDIUM) — README's "Platform caveats" section enumerates three divergences and omits the BSDs

`README.md:129-164` presents itself as *the* list of "platform divergences worth knowing
before you rely on it" for `decommit`. It covers Windows, huge pages, and Darwin. Per U-1,
FreeBSD/DragonFly/NetBSD/OpenBSD — targets this crate ships constants for and therefore
claims to support — have the same (or worse) advisory-only `MADV_DONTNEED` behaviour as
Darwin and are not mentioned. Likewise `README.md:51` lists `decommit_lazy`'s per-platform
advice as "Linux `MADV_FREE`, macOS/iOS `MADV_FREE_REUSABLE`, Windows falls back to
`decommit`" — silently omitting the "every other Unix → `MADV_DONTNEED`" arm that
`src/lib.rs:2460-2463` actually implements.

### D-3 (LOW) — `try_reserve_aligned_exact`'s counter doc slightly overstates what it measures

`src/lib.rs:210-218` says `UNIX_EXACT_RESERVE_ATTEMPTS` "increments BEFORE the `mmap`
call, so it includes both alignment misses and OS-level failures" — correct
(`:2291-2292` precedes `:2295`). But the module-level narrative at `:170-185` uses the
resulting ratio as "the real hit rate" for a syscall-cost model that assumes a miss costs
exactly 3 syscalls; an *OS-failure* miss costs only 1 (the `mmap` failed, no `munmap`
runs — `:2296-2300`). With OOM being rare this does not change the conclusion, but the
two denominators are not the same population and the doc treats them as one. Naming the
numerator/denominator populations explicitly would remove the ambiguity.

### D-4 (LOW) — doc drift: the module header's "0.1 doc" reference and a stale internal cross-reference

- `src/lib.rs:2270` — "1-syscall exact-size mmap fast path (see the 0.1 doc)" refers to a
  document that is not in this crate; a reader has no way to follow it.
- `src/lib.rs:2092` — "the huge-page decommit case documented around lib.rs:1093" cites a
  line number that no longer points at the huge-page decommit discussion (that text is now
  at `:1152-1161`). The crate elsewhere explicitly warns against line-number citations for
  exactly this reason (`tests/smoke.rs:77-81`: two prior line-range citations "both drifted
  stale within one round of being written"), so this is a self-identified anti-pattern that
  survived in `src/`.
- `src/lib.rs:182` cites "real hit rates (34.4%-56.7%, see lib.rs:882-885)" — `:882-885`
  is now inside `ReservationParts`'s field definitions; the hit-rate numbers live at
  `:931-935`. Same class.

### D-5 (INFO) — the doc-comment volume is itself a maintenance risk

`src/lib.rs` is 2,960 lines, of which — by inspection — well over half is comment. Several
individual comments run 40+ lines and restate the same fact in three places (the Windows
`is_huge` limitation appears verbatim at `:486-498`, `:615-627`, and `:1536-1550`; the
Darwin caveat at `:1163-1178`, `:1215-1235`, `:2394-2406`, plus `README.md:153-164`, plus
two test files). W-1 is direct evidence of the cost: three copies of the `is_huge`
contract all say the right thing and the *one* place that implements it disagrees with all
three. Consolidating each such block to one canonical location with intra-doc links would
reduce both the drift surface and the review burden.

---

## 9. Performance — real reserve/commit/decommit/release paths only

I looked specifically for unnecessary syscalls, redundant checks, and over-reservation in
`reserve_aligned` / `reserve_aligned_lazy` / `decommit` / `recommit` / `release`. Two real
opportunities, one of which the crate has already measured and documented but not acted
on; and a set of explicit null results.

### P-1 (HIGH-value, LOW-risk) — the Unix exact-size fast path is a net syscall *loss*, by the crate's own numbers

`src/lib.rs:2182-2186` tries `try_reserve_aligned_exact` first on every Unix reservation.
On a hit that is 1 syscall (`mmap`); on a miss it is 3 (`mmap` + `munmap` + the
over-reserve `mmap`, `:2295`/`:2334`/`:2198`). Without the fast path the cost is a flat 1
syscall (`mmap(size + align)`). Expected cost is therefore `3 − 2p`, which exceeds 1 for
every hit rate `p < 1`.

The crate has already measured `p`: 34.4% at 64 KiB align, 46.7% at 1 MiB, 56.7% at 4 MiB
(`:932-935`, `README.md:42`). That is **1.87–2.31 syscalls per reservation versus a flat
1.0** — 87%–131% more syscall traffic than not having the fast path at all. The crate's own
module comment states this conclusion in as many words (`:176-184`: "At real hit rates …
this is 87%-131% MORE syscall traffic than not having the fast path at all. What the fast
path still buys is address-space economy on 32-bit targets"), and then keeps the fast path
anyway with no `#[cfg(target_pointer_width = "32")]` gate.

*Recommendation:* gate the exact-size attempt on `target_pointer_width = "32"` (where
`size + align` of VA genuinely matters), or delete it. On 64-bit Unix — every realistic
consumer — this is a free ~1.9× reduction in reservation syscalls. Nothing about the
public contract changes: `reservation_len` is already documented as possibly exceeding
`size` (`:564-591`), and `tests/smoke.rs:131` already asserts only `parts.len >= 4 * MIB`.

### P-2 (MEDIUM-value) — nothing hints the kernel toward an aligned address

`libc_mmap` always passes `addr = NULL` (`:2749`). Linux's `mmap(2)` treats a non-NULL
`addr` without `MAP_FIXED` as a *hint*: "the kernel … will attempt to create the mapping
there. If another mapping already exists there, the kernel picks a new address." Passing
an `align`-rounded hint (e.g. the previous successful base rounded up, or simply
`align_up(last_end, align)`) is the standard trick used by production allocators to make
the exact-size path hit nearly always, and it costs nothing when it misses — the existing
alignment recheck at `:2332` already handles a hint that was not honoured.

This is the direct fix for P-1's root cause: the crate's own rustdoc says the hit rate
"depends on the OS's placement heuristics, **not on any hint this crate passes**"
(`:921-924`) — that is a statement about a choice, not a constraint. With a hint, `p`
plausibly approaches 1 and the fast path becomes what it was designed to be (1 syscall,
no over-reservation). It needs one `AtomicUsize` of state and a measurement to confirm;
the `bench-internals` counters and `examples/v20_849_unix_exact_reserve_hit_rate.rs`
already exist to measure it, which makes this an unusually cheap experiment.

### P-3 (MEDIUM-value, Windows) — the two-call over-reserve path is avoidable on Windows 10+

For `align > 64 KiB` — i.e. the flagship 2 MiB / 4 MiB allocator-segment case — every
Windows reservation costs 2 syscalls and reserves `size + align` of address space
(`:1836-1852`, `:1886`). `VirtualAlloc2` (Windows 10 / Server 2016 and later) accepts a
`MEM_EXTENDED_PARAMETER` of type `MemExtendedParameterAddressRequirements` carrying an
`Alignment` field, and returns a correctly-aligned region in **one** call with **no**
over-reservation. That would collapse both the syscall count and the VA waste for the
crate's headline use case.

Caveat stated honestly: `VirtualAlloc2` lives in `kernelbase.dll` and is normally linked
via `onecore.lib`/`mincore.lib`, not `kernel32`, so wiring it up without adding a
`windows-sys` dependency needs either a `#[link(name = "onecore")]` block or runtime
`GetProcAddress` resolution with a fallback to the current path (which must be kept
anyway for pre-Windows-10 targets, if any are still supported). That is real work and a
real portability decision — but the payoff is on the exact path a segment-allocating
consumer hits on every segment.

### P-4 (LOW) — `decommit`/`decommit_lazy` call `page_size()` on every invocation

`:1180` and `:1248`. `page_size()` is a relaxed atomic load plus a branch on the hot path
(`:390-393`), so this is already close to free — but it is not `#[inline]`, so each call is
a real function call on the decommit path. Marking `page_size()` `#[inline]` (or splitting
the cached fast path into an inlinable wrapper around a `#[cold]` slow path) is a
zero-risk micro-improvement on the one hot path that consults it. Worth about one branch
per decommit; noted for completeness, not as a priority.

### P-5 (null results — checked and found already minimal)

Stated explicitly so the absence of findings is on the record:

- **`release` / `Drop`:** exactly one `munmap` (`:2364`) or one `VirtualFree(…, 0,
  MEM_RELEASE)` (`:1929`) per reservation. The earlier head/tail-trim design (two extra
  `munmap`s) is gone; the current shape is the minimum the platform allows. Nothing to
  improve.
- **`recommit` / `commit_range` on Unix:** already a pure `Ok(())` with no syscall
  (`:2389-2417`), correctly, since `mmap` committed eagerly.
- **`decommit` on Windows:** a single `VirtualFree(MEM_DECOMMIT)` (`:2094`); there is no
  cheaper primitive (`DiscardVirtualMemory` exists and is *lazier*, not cheaper, and would
  change the documented semantics).
- **Validation cost:** `validate_size_align` (`:947-959`) is four branches plus one
  `checked_add`, all on values already in registers. `try_recommit`'s guard is three
  branches. Neither is measurable next to a syscall.
- **Over-reservation size:** `over = size + align` is the minimum that *guarantees* a fit
  for an arbitrary kernel-chosen base; `size + align - page_size()` would also work but
  saves one page and complicates the proof. Not worth it.
- **The `bench-internals` counters** are fully compiled out when the feature is off
  (`:207-208` gates the `AtomicU64` import itself, and every `fetch_add` is
  `#[cfg]`-gated, not branch-gated) — verified by reading, and consistent with the
  feature's own zero-cost claim in `Cargo.toml:126`.
- **`finish_reservation` / `RawReservation`** (`:980-1026`) are trivially inlinable
  by-value moves; no copies of the span, no allocation anywhere in the reservation path.
  The crate performs **zero** heap allocations in the real backend (the only `Vec` is in
  `mock`).

---

## 10. Smaller items and general improvements

### G-1 (LOW) — `release` can panic under miri on a contract violation, unlike `from_raw_parts`

`release` (`src/lib.rs:1076-1088`) forwards straight to `release_reservation`, whose miri
backend does `Layout::from_size_align(reservation_len, align).expect("release: invalid
layout")` (`:2862`). `from_raw_parts` was hardened against exactly this class of deferred
panic (`:762-828`, with seven dedicated tests) — `release` was not. Passing a swapped
`(len, align)` pair to `release` under miri panics with a bare message instead of the
informative multi-clause diagnostic its sibling produces. The crate's own API design
already acknowledges this footgun (`release_parts`/`ReservationParts` exist specifically
to prevent the swap, `:862-874`); adding the same `assert!` to `release` would make the
two entry points consistent.

### G-2 (LOW) — `From<VmemError> for io::Error` narrows `u32` → `i32` unchecked

`src/error.rs:141`: `std::io::Error::from_raw_os_error(code as i32)`. Win32 `GetLastError`
returns a `DWORD`; codes with the high bit set (any `HRESULT`-shaped value, e.g.
`0x8007000E`) become negative and produce a nonsensical `io::Error`. No `VirtualAlloc`
failure produces such a code in practice, so this is theoretical — but a
`i32::try_from(code)` with an `os_refusal_unknown_code()`-style fallback costs nothing and
removes the lossy cast entirely.

### G-3 (INFO) — API ergonomics worth settling before 0.2.0 locks

Collected here because a pre-publication window is the only cheap time for them:

1. **`decommit`/`recommit`/`commit_range` as `Reservation` methods** (see A-2) — removes
   the bounds obligation from the unsafe contract.
2. **Delete `is_empty`** (see A-3) rather than shipping it deprecated.
3. **`fault-injection = ["lazy-commit"]`** (see F-1).
4. **Resolve the `mock`-as-Cargo-feature question**, which `Cargo.toml:74-86` explicitly
   flags as "free today and stays free only until 0.2.0 ships". The manifest's own analysis
   is right that a `--cfg` flag would be strictly safer; whichever way it goes, it should go
   deliberately now rather than by omission.
5. **`Reservation` does not implement `Clone`** (correct) **but also has no way to
   re-derive an aligned sub-span handle**; consumers doing sub-allocation must fall back to
   raw pointers. Not a defect, just the shape of the API — worth a sentence in the README so
   consumers know it is intentional.

### G-4 (INFO) — structure and conventions

Against the workspace's stated "one file, one export" convention, `src/lib.rs` holds the
entire public API plus three complete platform backends plus the miri fallback in one
2,960-line file. The file's own header (`:114-121`) proposes exactly the right refactor —
`os_windows` / `os_unix` / `os_miri` private modules behind one shared signature, which
would also eliminate every `#[cfg_attr(feature = "mock", allow(dead_code))]` attribute
(there are 20+) — and defers it as "a larger refactor than this crate's 0.2.0 release
should carry". That judgement is defensible for a release, but the cost is already being
paid: the `#[cfg]` density around the Unix/Windows/miri/`mock`/`lazy-commit`/`huge-pages`/
`bench-internals`/`fault-injection` matrix is the single hardest thing to review in this
crate, and W-1 lives precisely in the kind of branch that density hides.

---

## 11. Findings index by severity

| ID | Sev | Area | One-line |
|---|---|---|---|
| **W-1 / D-1** | **HIGH** | Windows / docs | `is_huge()` returns `true` after the large-page request failed and the ordinary-page retry succeeded — **empirically reproduced** (`lib.rs:1731-1750` + `:1784`) |
| **P-1** | HIGH (perf) | Unix | The exact-size fast path costs 1.87–2.31 syscalls/reservation vs. a flat 1.0, by the crate's own measured hit rates (`lib.rs:2182-2186`) |
| **F-1** | MEDIUM | features | `fault-injection` without `lazy-commit` compiles and is completely inert (`Cargo.toml:99`) |
| **U-1 / D-2** | MEDIUM | Unix / docs | `decommit`/`decommit_lazy` are effectively no-ops on all four BSDs; undocumented (`lib.rs:2451-2464`) |
| **A-1** | MEDIUM | API | `decommit` validates against `page_size()`, `recommit`/`commit_range` against `PAGE`; undocumented asymmetry (`lib.rs:1180` vs `:1304`) |
| **M-1** | MEDIUM | mock | `mock::record` holds a `RefMut` across an allocating `Vec::push` — reentrancy panic for a `GlobalAlloc` consumer (`mock.rs:244-246`) |
| **T-1** | MEDIUM | tests | Nothing verifies `reserve_aligned_lazy` leaves the tail uncommitted; the oracle exists and is unused |
| **T-2** | MEDIUM | tests | The one test on W-1's path deliberately declines the assertion that would catch it (`huge_pages.rs:173-176`) |
| **P-2** | MEDIUM (perf) | Unix | `mmap` is never given an aligned address hint (`lib.rs:2749`) |
| **P-3** | MEDIUM (perf) | Windows | `VirtualAlloc2` + `MEM_EXTENDED_PARAMETER` would collapse the 2-syscall + `size+align` path for `align > 64 KiB` |
| **W-2** | LOW/MED | Windows | The two-call path's "retry with ordinary pages" is a verbatim duplicate of the call that just failed (`lib.rs:1886-1899`) |
| **F-2** | LOW | features | `mock` silently disables `fault-injection`; not in the public docs |
| **F-3** | LOW | features | `fault-injection` cannot reach `recommit`, the most crash-prone real path |
| **U-2** | LOW | Unix | Android is reasoned about in `OffT` and rejected by `compile_error!` in `MAP_ANON` |
| **U-3** | LOW | Unix | Over-reserve path does not re-check its computed alignment, unlike the other two paths |
| **A-2** | LOW | API | Decommit family takes a bare `base` with no length; would be safer as `Reservation` methods |
| **A-3** | LOW | API | `is_empty` ships `#[deprecated]` in a crate that has not published it yet |
| **M-2** | LOW | mock | `mock::record` can panic during TLS teardown from `Drop` |
| **G-1** | LOW | API | `release` can panic under miri where `from_raw_parts` gives a diagnostic |
| **G-2** | LOW | error | `u32 → i32` unchecked cast in the `io::Error` bridge |
| **T-3/4/5** | LOW | tests | No offset-base decommit test; `release(NULL,…)` untested; `page_size()` fallback untestable |
| **D-3/4/5** | LOW/INFO | docs | Counter-population ambiguity; three stale line-number citations; heavy doc duplication |
| **P-4** | LOW (perf) | both | `page_size()` is not `#[inline]` on the decommit path |
| **F-4, U-4, W-3, A-4, M-3, B-1/2, P-5, G-3/4** | INFO | — | Verified-good null results and pre-release suggestions |

