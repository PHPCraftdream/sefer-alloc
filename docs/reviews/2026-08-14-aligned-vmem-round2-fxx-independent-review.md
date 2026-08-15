# aligned-vmem — Round 2 independent read-only review (fxx)

**Date:** 2026-08-14
**Scope:** `git diff 6f94f89..493d077 -- crates/vmem` (branch `main`)
**Reviewer:** independent read-only pass (no code executed; manual reading only)
**Method:** cluster-by-cluster reading of the full diff plus the post-diff state of
`crates/vmem/src/{lib,mock,error}.rs`, `Cargo.toml`, `README.md`, and the four test
files, with counterfactual reasoning ("would this test fail without the fix") done
by code reading, not by running anything.

Known-and-already-fixed defects from the in-round reviews (NOT re-reported here,
per the review brief): the task #944/U-2 Android huge-pages wiring gap (fixed in
`23f8ea8`) and the task #949 mislabeled review citations (fixed in `6788208`).

## Findings by cluster

### 1. Windows path (W-1 grant tracking, W-2 dead retry branch)

**Verdict: fix is correct; no new defects found.** Reasoning:

- `crates/vmem/src/lib.rs:1909` — `let mut huge_granted = extra_commit_flags != 0;`
  initialized from the request, cleared at `:1940` iff the ordinary-page retry is
  the call that actually succeeded. The two reachable success shapes of the
  single-call path therefore report correctly: initial-call success with
  `MEM_LARGE_PAGES` → `true`; fallback success → `false`. This is exactly the
  W-1 false-positive fix, and the NOTE at `:1977-1983` now describes the code
  truthfully (the pre-fix NOTE admitted `granted_huge` was just the request flag).
- Alignment-check fall-through (`:1966-1971`): if the single-call base fails the
  `align` check, the block releases it and falls into the two-call path, which
  hard-returns `false` as the fourth element (`:2103`) — so a huge grant that is
  discarded on the alignment fall-through can never leak a stale `true`. Consistent.
- W-2 deletion: the removed two-call retry branch re-issued a byte-identical
  `VirtualAlloc(base, commit_len, MEM_COMMIT, PAGE_READWRITE)` after the same
  call had just failed (the two-call commit never carried `extra_commit_flags`),
  so it could only succeed on a transient race — deleting it is behaviorally
  safe, and the error-capture-before-cleanup ordering (`:2090-2092`) is preserved.
- Cross-checked the only remaining `granted_huge` producers: `reserve_aligned_raw`
  (passes `extra_commit_flags = 0`, discards the flag), `reserve_aligned_lazy`'s
  raw path (`0`), and `reserve_aligned_huge_raw` (`MEM_LARGE_PAGES`). The
  `reserve_aligned_huge` rustdoc's three-condition Windows limitation list
  (`:1723-1737`) matches the code post-fix.
- `from_raw_parts` hard-codes `granted_huge: false` (`:990`) with a comment —
  consistent with `is_huge()`'s "conservative" contract.

### 2. Unix path (U-1 BSD MADV_FREE, U-2 Android, U-3 alignment debug_assert, P-1 32-bit gate)

**Finding 2.1 — HIGH — Android support (U-2) misses a sixth wiring site: the
`_SC_PAGESIZE` sysconf index is wrong on bionic, so `page_size()` is broken on
Android.**
File: `crates/vmem/src/lib.rs:2926-2958` (`_SC_PAGESIZE` per-OS table) vs the
Android arms added throughout the file (`:2716`, `:2783`, `:2358`, etc.).
Task #944/U-2 (plus follow-up `23f8ea8`) wired Android into `MAP_ANON`,
`MAP_HUGETLB`, `MAP_HUGE_2MB`, `HUGE_SUPPORTED`, `LINUX_HUGE_PAGE_SIZE`,
`MADV_FREE`, `MADV_HUGEPAGE`, `madv_free_advice`, `libc_mmap`,
`libc_madvise_hugepage`, and `unix_reserve`'s huge guard — but NOT into the
`_SC_PAGESIZE` table, whose fallback arm (`:2944-2957`, "Linux and most other
unices use 30") Android now silently inherits. Bionic does not use glibc's
`confname.h` numbering: bionic's own `bits/sysconf.h` defines
`_SC_PAGESIZE = 0x0027` (= 39), while 30 (= 0x1e) lands on an unrelated
`_SC_XOPEN_*` table entry (REASONED-FROM-SPEC from bionic headers, same
evidence register as the file's own BSD-constant notes).
Concrete failure scenario: on any Android target, `query_os_page_size()` calls
`sysconf(30)`, which returns an unrelated value (e.g. `1` or an X/Open version
number — not a power of two ≥ 4096), so `validate_page_size_impl` silently
falls back to `PAGE` = 4096. On the 4 KiB-page devices this is accidentally
correct; on 16 KiB-page Android devices (arm64, Android 15+ — the configuration
Google now actively pushes) `page_size()` returns 4096 while the real page size
is 16384, and every caller told to round to `page_size()` (`decommit`,
`decommit_lazy`, `recommit`, `commit_range`, and the new safe
`Reservation::*` wrappers) will pass 4 KiB-aligned-but-not-16 KiB-aligned
offsets to `madvise`, which fails `EINVAL` — and `libc_madvise` deliberately
ignores errors, so decommit becomes an undetectable silent no-op (RSS never
drops, stale data stays resident). This is *verbatim* the poison scenario the
task #714 comment block above the table warns about ("if that unrelated value
happened to be a power of two it would silently pass `page_size()`'s validation
guard"), except on Android it fails the guard and poisons via the fallback
instead. Before this round the gap was unreachable (Android didn't compile —
it hit the `MAP_ANON` `compile_error!`); U-2 made Android compile and thereby
armed it. The prior in-round review counted 5 wiring sites and caught 3 — this
is a 6th site outside that list, i.e. the U-2 fix is still incomplete.
Suggested fix: add `#[cfg(target_os = "android")] { 39 }` as its own arm (with
the same REASONED-FROM-SPEC caveat), and exclude Android from the `30` fallback
arm's `not(any(...))` list; ideally also extend the fallback arm's comment to
say "glibc/musl Linux", since "most other unices" has now been wrong twice
(BSDs in task #714, Android here).

**Finding 2.2 — MEDIUM — `reserve_aligned`'s public rustdoc contradicts P-1:
it still documents the exact-size fast path as unconditional on Unix.**
File: `crates/vmem/src/lib.rs:1076-1090`. The rustdoc says "On Unix, first
tries an ordinary exact-size `mmap` ... On a miss (wrong alignment),
over-reserves `size + align` bytes", and the "**Cost on Unix fast-path
miss**" paragraph presents the `size + align` VA hold as a miss-only cost with
measured hit rates. After P-1 (`:2371` — `#[cfg(target_pointer_width = "32")]`
on the fast-path call; `:2488` — 32-bit-only cfg on `try_reserve_aligned_exact`)
this is false on every 64-bit Unix target, where `reserve_aligned` ALWAYS
over-reserves and the "hit rate" is structurally 0%. The round updated the
module-level design comment (`:169-185`) and the private helper's doc but left
the crate's primary public entry-point doc stale — a publish-facing desync of
exactly the class this round's own D-findings were fixing. Fix: add "on 32-bit
targets" to the fast-path sentence and state that 64-bit always over-reserves.

**Finding 2.3 — LOW — the round rewrote the module-doc paragraph containing a
stale line-number citation and kept the stale citation.**
File: `crates/vmem/src/lib.rs:183` — "At real hit rates (34.4%-56.7%, see
lib.rs:882-885)". The cited numbers actually live at `lib.rs:1087-1088`
(inside `reserve_aligned`'s rustdoc); lines 882-885 are `from_raw_parts`'s
`# Safety` section. The citation was already off (~50 lines) at base `6f94f89`
(numbers were then at `:932`) and the round's own +150-line `Reservation`
insertion widened the drift to ~200 lines while commit `d3eafa0` was editing
this very sentence. The repo already converted stale line citations to symbol
names once (task #901/U-3); this one should cite `reserve_aligned`'s doc by
symbol, not by line.

**Finding 2.4 — LOW — `UNIX_EXACT_RESERVE_*` counter rustdoc not updated for
P-1: "always 0" list omits 64-bit Unix.**
File: `crates/vmem/src/lib.rs:210-234`. Both counters' docs still say "Unix
only — always 0 on Windows/miri". After P-1 they are also permanently 0 on all
64-bit Unix targets (the only increment sites, `:2495`/`:2541`, are inside the
now 32-bit-only `try_reserve_aligned_exact`). A bench harness reading these on
x86_64 Linux gets 0/0 and the doc gives no hint why. One sentence fixes it.

**Finding 2.5 — LOW — `madv_free_advice`'s new doc implies tvOS/watchOS get
`MADV_FREE_REUSABLE`; the code gives them `MADV_DONTNEED`.**
File: `crates/vmem/src/lib.rs:2642-2650` (doc) vs `:2652-2685` (body). The
rewritten doc reads "tvOS/watchOS (same XNU kernel as macOS/iOS):
`MADV_FREE_REUSABLE`'s value comes from XNU, so it MAY work identically there
too" — but the `MADV_FREE_REUSABLE` arm is cfg'd `any(macos, ios)` only, so
tvOS/watchOS fall into the final `not(any(...))` arm and get `MADV_DONTNEED`
(4), which on XNU is the advisory-only no-op the same round's U-1 text
documents. The OLD doc stated this correctly ("other Unix (including
tvOS/watchOS ...): `MADV_DONTNEED`"); the rewrite lost the operative fact.
Consequence: a reader (or future fixer) believes `decommit_lazy` frees pages on
tvOS/watchOS when it is actually the advisory no-op. Doc-only fix: restate that
tvOS/watchOS currently take the `MADV_DONTNEED` fallback arm.

**Checked and clean in this cluster:**
- BSD `MADV_FREE` values (`:2880-2888`): FreeBSD 5 / DragonFly 5 / NetBSD 6 /
  OpenBSD 6 match each OS's `sys/mman.h`; `MADV_DONTNEED` = 4 is likewise
  correct on all four BSDs and Darwin. The `madv_free_advice` arm set is
  exhaustive and non-overlapping (linux+android / macos+ios / freebsd+dragonfly
  / netbsd+openbsd / fallback).
- Android cfg audit across all NON-sysconf sites (see 2.1 for the one miss):
  `MAP_ANON` 0x20, `MAP_HUGETLB` 0x40000, `MAP_HUGE_2MB`, `MADV_FREE` 8,
  `MADV_HUGEPAGE` 14 are kernel-ABI values, correct under bionic; the
  `compile_error!` guard (`:2749-2770`) and the `MAP_ANON` Darwin/BSD arm do
  not overlap with the new `any(linux, android)` arms — no target matches two
  arms, no supported target matches none.
- U-3 `debug_assert!` (`:2434-2437`): correct as a debug-only check — the
  over-reserve base comes from `align_up_addr` arithmetic with no
  unverified-constant dependency, unlike the exact path's runtime check whose
  rationale (`:2511-2534`) it explicitly mirrors. It cannot fire on valid
  input and adds no release-mode cost.
- P-1 leftovers: no dead code found on 64-bit — `libc_mmap`/`libc_munmap`
  remain used by the over-reserve path; the exact-path fn, its counters'
  increments, and the 32-bit cfg on the call site are mutually consistent
  (`all(unix, not(miri), target_pointer_width = "32")` ⊇ every context the
  call site can be compiled in).

### 3. New safe Reservation API (A-1/A-2/A-3) + release() assert (G-1)

**Finding 3.1 — LOW (unsafe-hygiene regression) — `release()`'s new
`NonNull::new_unchecked` has no `// SAFETY:` comment and replaced a fully safe
construction.**
File: `crates/vmem/src/lib.rs:1261` — `let nn = unsafe {
NonNull::new_unchecked(reservation) };` (introduced by `b5fe743`). It IS sound
(null is checked at `:1245`), but: (a) every other unsafe operation in this
crate carries a per-site `// SAFETY:` comment — the repo's own tier-2 unsafe
discipline, re-audited as recently as task #894/T7 — and this new site has
none; (b) the pre-change code used the safe `match NonNull::new(...)` pattern,
so the fix-pass *added* an unchecked unsafe operation where a safe one
sufficed. Fix: either restore `NonNull::new(...).expect(...)`/`match`, or add
the one-line SAFETY comment citing the `:1245` null check.

**Finding 3.2 — LOW — `release()`'s rustdoc has no `# Panics` section for the
new G-1 assert, which is a native-path behavior change.**
File: `crates/vmem/src/lib.rs:1216-1259`. Before G-1, the doc's own words held:
"The native (`munmap`/`VirtualFree`) paths ignore `align`" — a caller passing a
degenerate `align`/`reservation_len` (in breach of the `# Safety` contract but
harmless natively) got a working release. Now the same call panics on every
backend, and nothing in the rustdoc says so. Side effect worth one sentence
too: the assert runs BEFORE `mock::record`, so under `mock` a contract-violating
release now also disappears from the call log (previously it was recorded, then
only miri panicked). Both are defensible hardening choices — but undocumented.

**Finding 3.3 — LOW (32-bit edge) — G-1's `Layout` clause can panic on a
technically crate-constructed reservation near 2 GiB on 32-bit targets.**
File: `crates/vmem/src/lib.rs:1253` (and the pre-existing twin at `:972`).
`validate_size_align` (`:1102-1114`) only rejects `size + align` overflow of
`usize`, not of `isize::MAX`. On a 32-bit Unix target, `reserve_aligned(size ≈
0x7fff_f000, PAGE)` that misses the exact fast path over-reserves `over =
0x8000_0000 > isize::MAX`; if the kernel grants it (feasible under a 3G/1G
split), the resulting `Reservation` has `reservation_len` for which
`Layout::from_size_align` fails — so `into_parts` + `release` now panics on a
reservation the crate itself created, where pre-G-1 the native `munmap` path
worked. Cheapest closure: reject `size + align > isize::MAX as usize` in
`validate_size_align` (also future-proofs `from_raw_parts`'s identical clause
and the >isize::MAX-allocated-object concern generally).

**Checked and clean in this cluster:**
- A-1: `recommit` delegates to `try_recommit` (`:1475-1478`) and `commit_range`
  to `try_commit_range` (`:1564-1567`), and both `try_` bodies now use
  `page_size()` (`:1489-1491`, `:1580-1582`); `decommit`/`decommit_lazy`
  already did. No remaining `PAGE`-literal validation in the
  decommit/recommit/commit family — the 16 KiB-page (Apple Silicon)
  inconsistency A-1 targeted is fully closed. Docs updated at all four sites.
- A-2 bounds logic: each wrapper checks `end > self.len()` and the delegated
  free function checks `start`-vs-`end` ordering and page-multiplicity, so the
  conjunction covers every out-of-contract shape (including `start > end` with
  small `end`, and `start > len` with `end <= len` which implies `start > end`).
  No arithmetic in the wrappers → no overflow. Offsets are relative to
  `as_ptr()` (the aligned usable base), which is exactly the base the free
  functions' `# Safety` sections name — correct for over-reserved spans.
- A-2 soundness of a *safe* fn that discards/unmaps memory contents: holds
  because `Reservation` exposes the span only as raw pointers (`as_ptr`);
  safe code cannot hold references into it, so any reference invalidated by a
  safe `decommit` was created by caller `unsafe` code that owns that proof
  obligation. The Windows write-before-recommit crash and huge-page/Darwin
  no-op caveats are cross-referenced from each wrapper's doc.
- A-3: `is_empty` deleted cleanly — `#[allow(clippy::len_without_is_empty)]`
  added at `:558`, no stale references anywhere in `crates/vmem` (code, tests,
  README). Legitimate in an unpublished 0.2.0.

### 4. mock.rs reentrancy + TLS teardown (M-1/M-2)

**Verdict: the fix is correct and complete; no new defects.** Verified points:

- The M-1 guard (`RECORDING: Cell<bool>`, `crates/vmem/src/mock.rs`) does not
  reintroduce M-2 through its own TLS key: `Cell<bool>` has no destructor, so
  the `thread_local!` never registers one and `LocalKey::with` on it cannot
  enter the destroyed/panic state during teardown. Likewise
  `RESERVE_FAILS`/`COMMIT_FAILS` are `RefCell<u32>` (no drop glue) — `CALLS`
  (`RefCell<Vec<Call>>`) is the ONLY key with a destructor, and it is exactly
  the one moved to `try_with`. The fix's coverage is complete, not partial.
- Reentrancy loss semantics: a reentrant `record` is silently dropped — this IS
  data loss in the call log, but it is (a) confined to the pathological
  consumer-allocator-reentry case that previously *panicked inside an
  allocator*, and (b) honestly documented in `record`'s doc comment ("The
  reentrant call's own recording is lost"). Acceptable for a test-observability
  surface; no silent loss was introduced on any normal path.
- The unguarded `recording.set(false)` (no RAII): the justification in the doc
  comment is sound — the only panic sources inside the guarded region are
  allocation failure (aborts) and `Vec` capacity overflow (unreachable at test
  scales); and even a leaked `true` flag would only mute that one thread's
  further recording, never corrupt state.
- Send/Sync: everything is thread-local; no cross-thread visibility change.
- The *test* that ships with M-1 is vacuous — see Finding 6.5.

### 5. error.rs cast (G-2) + Cargo.toml feature graph (F-1) + CI feature-combination impact

**Verdict: both fixes correct; null result.** Verified points:

- `crates/vmem/src/error.rs:138-163`: the `From<VmemError> for io::Error` match
  is exhaustive over all constructible states (`Some(code)` fits-i32 /
  doesn't-fit, `None`+invalid-arg, `None` refusal) — every producer
  (`from_os_code`, `invalid_argument`, `os_refusal_unknown_code`,
  `last_os_error`) lands in exactly one arm. The overflow arm preserves the
  error via `io::Error::other` instead of a negative-code misinterpretation.
  Note (INFO, pre-existing): `last_os_error_code` (`:166-170`) still does
  `c as u32` on the platform's `raw_os_error()`; a hypothetical negative raw
  code round-trips to a >`i32::MAX` u32 — which the NEW code now routes to the
  `Err(_)` arm correctly, so G-2 also retroactively contains that older cast.
- `crates/vmem/Cargo.toml`: `fault-injection = ["lazy-commit"]` is additive.
  Every CI row that enables `fault-injection` already enables `lazy-commit`
  explicitly (`.github/workflows/ci.yml:161`, `:789`, `:823`, `:920`) or via
  `--all-features` (`:164`, `:167`, `:793`, `:828`, `:900`), so no existing
  combination changes meaning, and the previously-possible pointless
  combination (hook compiled with no consumer) is now unrepresentable. The
  Cargo.toml comment update matches the code (`should_fail_commit`'s only
  consumer sits inside `try_commit_range`, `lib.rs:1603`, itself
  `lazy-commit`-gated). `scripts/check-matrix.mjs` does not reference vmem
  features at all — no drift possible there.

### 6. New tests (T-1..T-5 + the 12 earlier round-2 tests) — counterfactual check

**Finding 6.1 — HIGH — `reservation_decommit_in_bounds_matches_free_function`'s
zero-fill assert will go red on Linux `--all-features` and macOS CI: it lacks
the miri/mock/Darwin exclusions its sibling test deliberately carries.**
File: `crates/vmem/tests/smoke.rs` (new A-2 test, the `#[cfg(not(windows))]`
block asserting `*p == 0` "Linux should zero-fill on re-access").
The adjacent, pre-existing `decommit_recommit_roundtrip` guards its identical
zero-check with `#[cfg(not(any(miri, feature = "mock", target_os = "macos",
target_os = "ios", target_os = "tvos", target_os = "watchos")))]`
(`tests/smoke.rs:273-280`) with a long comment explaining why each exclusion
exists. The new test copies the scenario but guards only on `not(windows)`.
Concrete failures, deterministic by construction:
1. **Linux, `mock` on** (`ci.yml:167` and `:900` both run `cargo test -p
   aligned-vmem --all-features` on ubuntu): under `mock`, `decommit` is
   record-only (`lib.rs:1369-1380` — the real `decommit_pages_impl` is
   `#[cfg(not(feature = "mock"))]`), so the page still holds the `0xAB`
   written earlier; the test reads `0xAB`, asserts `== 0`, fails. smoke.rs has
   no file-level `mock` gate (only `tests/mock.rs` does), so this compiles and
   runs.
2. **macOS, real backend** (`ci.yml:823` and `:828`): the Darwin
   `MADV_DONTNEED` advisory-only gap — this crate's own docs and item 48 call
   it "confirmed as a real, failing-test-level gap by this crate's first
   real-macOS CI run" — means the old byte can legally read back; the same
   round's T-3 test (`decommit_recommit_roundtrip_on_over_reserved_span`)
   correctly excludes the Darwin family, this one doesn't.
The defect is invisible on the Windows dev machine (the whole block is
compiled out under `cfg(windows)`), which is exactly why the round's local
"full verification matrix" stayed green. Since round-2 commits are not yet
pushed, the first push will land red. Fix: replace `#[cfg(not(windows))]` with
the sibling's full exclusion list (and keep a `write`-then-read fallback for
the excluded configs if desired).

**Finding 6.2 — MEDIUM — T-1 (`windows_lazy_reserve_saves_commit_charge`,
`tests/lazy_commit.rs`) does not pin the guard it claims to pin: its
counterfactual fails.**
The doc comment says it is "pinning the `commit_len == size` guard in
`win_reserve_commit` that a prior bug already broke once". But the test calls
`reserve_aligned_lazy(4 MiB, /*align=*/4 MiB, PAGE)` — and the single-call
fast path requires `align <= WIN_ALLOCATION_GRANULARITY` (64 KiB) *in
addition to* `commit_len == size` (`lib.rs:1905`). With `align = 4 MiB` the
single-call path is skipped by the align condition alone, so both assertions
(two-call +1, single-call +0) pass even if the `commit_len == size` guard is
deleted outright. To actually discriminate the guard, the test must use
`align <= 64 KiB` with `initial < span` (e.g. `reserve_aligned_lazy(4 * MIB,
PAGE, PAGE)`), which routes to two-call *only because of* the guard.

**Finding 6.3 — MEDIUM — T-1 is additionally flaky by construction: exact
counter deltas asserted without serialization against parallel tests in the
same binary.**
`tests/lazy_commit.rs` has no `SERIAL` mutex (verified — the file's only gate
is `#![cfg(feature = "lazy-commit")]`). The test calls
`reset_bench_internals_counters()` and then asserts `after_two_call ==
before_two_call + 1` and `after_single_call == before_single_call` on
PROCESS-GLOBAL atomics, while libtest runs the file's other tests (each doing
its own `reserve_aligned_lazy`/`reserve_aligned`, each bumping
`WINDOWS_RESERVE_COMMIT_*`) on parallel threads. Any interleaved reservation
between the `before_` and `after_` reads breaks the exact equality. This is
precisely the hazard `tests/smoke.rs:15-29`'s SERIAL comment documents for the
madvise counters — the established in-crate pattern was not applied. Affects
the Windows CI rows with `bench-internals` (`ci.yml:789`, `:793`). Fix: add
the same `static SERIAL: Mutex<()>` discipline (or `--test-threads=1`-style
isolation) to this file.

**Finding 6.4 — MEDIUM — T-3 (`decommit_recommit_roundtrip_on_over_reserved_span`,
`tests/smoke.rs`) never establishes the "genuinely offset base" premise it
exists to test — and is structurally incapable of it on Windows.**
The doc claims `align = 4 * size` forces `base > reservation`. Reality:
- **Windows:** `size = PAGE`, `align = 16 KiB <= 64 KiB`, `commit_len == size`
  → the single-call fast path (`lib.rs:1905`) returns `base == region` (zero
  head offset) whenever VirtualAlloc's 64 KiB-aligned base satisfies 16 KiB
  alignment — i.e. always. The test therefore exercises exactly the same
  zero-offset shape as every pre-existing test, on the platform where the
  offset arithmetic (`base.add(start)`) it wants to cover is Windows-specific
  `MEM_DECOMMIT`/`MEM_COMMIT` code.
- **Unix (64-bit):** the over-reserve path runs, but whether
  `align_up_addr` produces a nonzero offset depends on where `mmap` lands
  (3-in-4 chance at best, 0 if the kernel hands back 16 KiB-aligned bases,
  which many do consistently); the test never asserts
  `r.as_ptr() != r.reservation_ptr()`, so a zero-offset run silently passes
  as vacuous.
Fix: assert the premise (`as_ptr() != reservation_ptr()`), and obtain it
deterministically — e.g. retry-loop holding previous reservations until the
offset is nonzero, and on Windows use `align > 64 KiB` (which forces the
two-call over-reserve path) instead of 16 KiB. Also: the comment `// 2 KiB
offset (still page-aligned for decommit)` is doubly wrong — 2048 is not
page-aligned and `offset` is never passed to `decommit` (the test decommits
`[0, size)`).

**Finding 6.5 — MEDIUM — the M-1 test (`reentrancy_guard_silently_drops_nested_calls`,
`tests/mock.rs`) is vacuous with respect to reentrancy: every assertion passes
without the M-1 fix.**
The test performs only ordinary, non-reentrant reserve/decommit/recommit/drop
sequences and asserts call counts — all of which the pre-fix
`CALLS.with(|c| c.borrow_mut().push(call))` implementation satisfies
identically. Its own comments concede the gap twice ("The reentrancy case
itself is tested implicitly by the existence of the guard" — circular; "we
can't easily test the full allocator-back-to-aligned-vmem reentrancy...").
Counterfactual verdict: removing the `RECORDING` guard fails NO assertion in
this test. The same is true of M-2 (no test constructs a TLS-owned
`Reservation` dropped at thread teardown). Per the repo's own zero-trust rule
("verify the tests are not vacuous — would they fail without the fix"), M-1/M-2
shipped effectively untested. A real M-1 test is feasible in a dedicated
integration-test binary: install a `#[global_allocator]` wrapper that calls
`reserve_aligned`+drop from inside `alloc()` (mock feature on), then perform a
recorded operation that grows `CALLS` — pre-fix this panics with
`BorrowMutError`, post-fix it completes with the outer record intact.

**Finding 6.6 — LOW — the new A-2 tests lock `SERIAL` with `.unwrap()` instead
of the file's established poison-tolerant idiom.**
File: `tests/smoke.rs` (new `reservation_*` tests use `SERIAL.lock().unwrap()`;
pre-existing tests use `.unwrap_or_else(|e| e.into_inner())`). If any test
panics while holding the lock — which Finding 6.1 makes a certainty on two CI
jobs — every subsequent `.unwrap()` test fails spuriously with `PoisonError`,
turning one red test into a cascade and obscuring the real culprit.

**Finding 6.7 — INFO — T-2's attribution comment mislabels the bug's task.**
`tests/huge_pages.rs` (new assert block): "If this assertion fails, it's the
W-1 bug (task #949)" — W-1 is task #943's fix; #949 is the test-writing task.
Given the round already had one citation-fixing follow-up commit (`6788208`)
for exactly this test batch, worth correcting. The test itself is a GENUINE
regression test: pre-`268af20`, a 64 KiB `MEM_LARGE_PAGES` request (which can
never succeed — `GetLargePageMinimum` is 2 MiB) fell back to ordinary pages
but still returned `granted_huge = extra_commit_flags != 0` = `true`, so this
assert fails on the old code and passes on the new — counterfactual holds.

**Counterfactual-clean tests in this batch:** T-2 (above), T-4
(`release_null_is_noop_and_not_recorded` — would fail if the null early-return
or the recorder-skip were removed), T-5 (`validate_page_size_falls_back_on_invalid_values`
— directly exercises all three invalid classes plus three valid pass-throughs
of the newly extracted pure function), and the A-2 out-of-bounds /
misaligned-rejection tests (each fails if the corresponding wrapper bounds
check or free-fn validation were removed).

### 7. General: docs/README consistency, unsafe hygiene, file-structure conventions

- **Unsafe hygiene:** one new unsafe site without a `// SAFETY:` comment
  (Finding 3.1). All other new unsafe blocks (the six A-2 wrappers, the
  Windows retry, the Unix debug_assert surroundings) carry per-site SAFETY
  comments (the wrappers' "same safety argument as X above" cross-reference
  style is terse but acceptable). No new `#[allow(unsafe_code)]` attributes
  were added; the self-verifying grep from CLAUDE.md is unaffected.
- **File structure:** no new files; `mock.rs`/`error.rs` remain single-focus;
  the crate falls under the single-file-seam-crate sanction for its multiple
  exports. `validate_page_size_public` (INFO): unlike every other
  `bench-internals` test-only export in this file, it is NOT `#[doc(hidden)]`
  — it renders in public rustdoc as if it were stable API. Suggest adding
  `#[doc(hidden)]` for consistency with `UNIX_EXACT_RESERVE_*` et al., or
  documenting why it is deliberately visible.
- **README:** the new Reservation-ownership note matches the API (no sub-span
  handles exist). No README text references the deleted `is_empty` or the
  64-bit exact-path behavior (the README does not describe the Unix fast path
  at that level of detail — checked; the stale-doc problem is confined to
  lib.rs's own rustdoc, Findings 2.2-2.4).
- **Doc-comment truthfulness:** Findings 2.2/2.3/2.4/2.5/3.2 are the doc
  desyncs found; everything else sampled (win_reserve_commit header, decommit
  Darwin/BSD paragraphs, Cargo.toml feature comments, the huge-pages Windows
  limitation list) matches the code as of `493d077`.

## Summary

**17 findings: 2 HIGH (2.1, 6.1), 5 MEDIUM (2.2, 6.2, 6.3, 6.4, 6.5),
7 LOW (2.3, 2.4, 2.5, 3.1, 3.2, 3.3, 6.6), 3 INFO (5.x error-cast note,
6.7, 7.x doc-hidden inconsistency).**

Top 3 by severity:

1. **HIGH 2.1** — Android support (U-2) is still incomplete: bionic's
   `_SC_PAGESIZE` is 39, not the glibc 30 the fallback arm supplies, so
   `page_size()` on Android silently falls back to 4096 — wrong on 16 KiB-page
   Android 15+ devices, silently no-op'ing every decommit (`lib.rs:2926-2958`).
   The exact same "sixth unwired site" failure mode as the wiring gap the
   in-round review already caught — one more site remained.
2. **HIGH 6.1** — the new safe-API test's zero-fill assert
   (`tests/smoke.rs`, `reservation_decommit_in_bounds_matches_free_function`)
   is missing the miri/mock/Darwin exclusions and will deterministically fail
   the Linux `--all-features` CI steps and the macOS steps on first push;
   invisible locally because the dev machine is Windows.
3. **MEDIUM 2.2** — `reserve_aligned`'s public rustdoc still documents the
   exact-size Unix fast path as unconditional after P-1 made it 32-bit-only —
   the primary entry point's published behavior description is wrong for every
   64-bit Unix user.

The substantive round-2 fixes themselves verified well: W-1/W-2 (Windows grant
tracking) is correct and fully consistent with its docs; A-1/A-2/A-3 and G-1
are sound (with three LOW hygiene/doc caveats); M-1/M-2 is a complete fix
(though shipped with a vacuous test); F-1 and G-2 are clean null results; the
BSD `MADV_FREE` constants are all correct per each OS's headers.

**Publication verdict for 0.2.0: NOT READY YET — conditionally ready after a
small fix pass.** Blocking items: 2.1 (ship-stopping if Android is a claimed
supported target of this release — alternatively, back out the Android cfg
arms or mark Android unsupported until the sysconf table is wired and the
crate can be smoke-tested against bionic) and 6.1 (guaranteed CI red on push,
which the repo's own push gate forbids). Strongly recommended in the same
pass: 2.2 (public-doc correctness), 6.2/6.3/6.4/6.5 (the round's new tests
under-deliver their claimed coverage), and the cheap LOWs (3.1 SAFETY comment,
6.6 poison-tolerant locking). None of the remaining items requires design
work; all are localized edits.
