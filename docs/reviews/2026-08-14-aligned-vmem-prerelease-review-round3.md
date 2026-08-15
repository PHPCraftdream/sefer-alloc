# `aligned-vmem` — pre-release review (round 3)

**Date:** 2026-08-14
**Reviewer:** independent closing read of the post-round-2-fix-pass tree (`main` @ `493d077`)
**Target:** `crates/vmem/` (crate name `aligned-vmem` 0.2.0) — `src/`, `tests/`, `benches/`, `examples/`, `Cargo.toml`, `README.md`
**Method:** STRICT READ-ONLY. No `cargo` commands were run, no files outside this report were
modified. Every finding is derived from reading the current source and cross-checking against the
crate's own rustdoc/README claims and the platform APIs' documented contracts. Four parallel
delegated audit passes (unsafe/FFI, Tier-F doc-conformance, tests/oracles, perf/API/arithmetic)
were each followed by a personal line-level verification of every claim; claims that did not
survive verification are listed as refuted in §6 rather than silently dropped.
**Position in the review chain:** this is the "third-level closing review" the round-2 checkpoint
(`docs/checkpoints/2026-08-14-vmem-r2-complete.md`) left as an open question. Its primary jobs:
(a) verify the round-2 fix pass (tasks #943–#950, commits `b5fe743`…`493d077`) introduced no new
defects, and (b) take a fresh independent pass with angles the prior ~13 rounds did not cover.

---

## Executive summary

**Verdict: ship-able after one documentation-sync pass. No new code defect was found.**

The round-2 fix pass itself is clean: all fifteen fixes (W-1/W-2, U-1/U-2/U-3, P-1, F-1, G-2,
M-1/M-2, A-1/A-2/A-3, G-1) were re-read at their sites and none introduced a regression (§1).
The soundness re-audit of the FFI layer, error-path resource lifecycle, errno-capture discipline,
provenance handling and arithmetic came back with only null results (§6) — consistent with the
two prior pre-release reviews' conclusions.

What the fix pass left behind is **documentation drift**: four of its behavior changes landed in
code + rustdoc but were not fully propagated to `README.md` (the crates.io landing page) and to
two crate-level doc spots. The two that matter are DOC-1 (README's alignment contract still
documents the old `PAGE` granularity for `recommit`/`commit_range`, which the A-1 fix changed to
`page_size()` — a consumer following README on a 16 KiB-page host gets rejected calls the README
says are valid) and DOC-2 (the crate-level doc, `reserve_aligned`'s rustdoc and README's API
table still describe the Unix exact-size fast path — with its hit-rate numbers — as current
behavior, but the P-1 fix made it 32-bit-only, so on 64-bit Unix, every realistic consumer, it
never runs). Both are one-paragraph fixes; both are worth landing **before** 0.2.0 publishes,
because README accuracy cannot be retrofitted into already-rendered crates.io pages without a
new release.

Everything else is LOW/INFO: a README exception list that is one item short (DOC-3), a README
caveats section that omits the BSD arms U-1 added (DOC-4), one rustdoc comment that contradicts
its own cfg routing (DOC-5), a privilege wording that elides the enabled-vs-held distinction
(DOC-6), two stale internal line-number citations of exactly the class the crate's own tests
warn about (DOC-7), one public test-hook naming/visibility nit (API-1), one defense-in-depth
suggestion (NUM-1), and one avoidable doomed syscall on the Windows huge path (PERF-1).

The known-open items carried forward (not re-litigated here): the `mock`-as-Cargo-feature →
`--cfg` decision (CORRECTNESS_OPEN_ITEMS item 42, flagged URGENT, maintainer call), the
Darwin/BSD eager-decommit zero-fill gap (item 48, fix needs a `MAP_FIXED` re-mmap round),
deferred perf items P-2 (mmap aligned hint) and P-3 (`VirtualAlloc2`), F-2 (`mock` silently
disables `fault-injection` — documented in code comment only) and F-3 (no fault-injection seam
on `recommit`). §3 lists them with their recorded owners.

---

## 1. Verified round-2 fixes (spot-check log)

Each round-2 fix re-read at its site; state recorded so the null result is on the record.

| Fix | Site | State |
|---|---|---|
| W-1 `is_huge()` after large-page retry fallback | `lib.rs:1909/1940/1984` | ✅ `huge_granted` cleared in the retry arm; returned value tracks which call succeeded |
| W-2 duplicate plain-commit retry on two-call path | `lib.rs:2086-2094` | ✅ removed — commit failure captures err, releases region once, returns `Err` |
| U-1 BSD `MADV_FREE` arms | `lib.rs:2656-2686`, `2884-2894` | ✅ FreeBSD/DragonFly 5, NetBSD/OpenBSD 6; eager-path BSD caveat added to `decommit` rustdoc (`lib.rs:1344-1363`) — README half NOT synced (DOC-4) |
| U-2 Android support | `lib.rs:2716`, `2358`, `3021-3027` | ✅ `MAP_ANON` / `HUGE_SUPPORTED` / `libc_mmap` / `_SC_PAGESIZE`-fallback arms all include `android`; the `compile_error!` guard excludes it |
| U-3 over-reserve alignment re-check | `lib.rs:2429-2437` | ✅ `debug_assert!` added (runtime arithmetic self-evident, so assert-strength is right) |
| P-1 exact-size fast path gated to 32-bit | `lib.rs:2367-2376`, `2488` | ✅ `#[cfg(target_pointer_width = "32")]` — **but see DOC-2: public docs still describe the 64-bit behavior** |
| F-1 `fault-injection = ["lazy-commit"]` | `Cargo.toml:101` | ✅ |
| G-2 `u32→i32` io::Error cast | `error.rs:138-163` | ✅ `i32::try_from` + `io::Error::other` fallback preserves the original error |
| M-1 mock reentrancy guard | `mock.rs:208-302` | ✅ `RECORDING` `Cell<bool>` guard; reentrant records silently dropped. The teardown asymmetry is load-bearing and correct: `RECORDING` is a `Cell` (no `Drop` → no TLS destructor is registered → `.with` can never panic at teardown), while `CALLS` (a `RefCell<Vec<_>>`, has `Drop`) uses `try_with`. Recorded here so a future "consistency" pass does not "fix" the `.with` into a regression |
| M-2 TLS-teardown panic | `mock.rs:296-298` | ✅ `try_with` on the destructible cell only |
| A-1 `recommit`/`commit_range` validate against `page_size()` | `lib.rs:1489-1491`, `1580-1582` | ✅ — **but see DOC-1: README still documents the old `PAGE` contract** |
| A-2 six safe `Reservation::{decommit,decommit_lazy,recommit,try_recommit,commit_range,try_commit_range}` | `lib.rs:698-843` | ✅ bounds-checked against `self.len()` before forwarding; ordering + alignment still validated inside the free fns; rustdoc contracts match the code exactly (checked clause by clause) |
| A-3 `is_empty` deleted | — | ✅ only the matching `#[allow(clippy::len_without_is_empty)]` remains |
| G-1 `release()` miri-path assert | `lib.rs:1245-1259` | ✅ multi-clause assert before the backend — **but see DOC-3: README's exception list not updated** |
| T-1..T-5 test additions (task #949) | `tests/` | ✅ verified genuine, incl. CI coverage (§6.4): the Windows lazy two-call oracle runs in the Windows CI row `cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals"` (`ci.yml:790`; same row on macOS at `:822`) |

**Net assessment of the fix pass:** no new code defect at any fix site.

---

## 2. Findings

### DOC-1 (MEDIUM) — README's alignment contract still documents the pre-A-1 `PAGE` granularity for `recommit`/`commit_range`

`README.md:103`: "`recommit`/`commit_range` offsets must be multiples of `PAGE`." — plus the
explanatory note at `README.md:105-108`, which justifies a granularity *asymmetry* between the
decommit and commit sides that A-1 (task #947) removed: `try_recommit` (`lib.rs:1489-1491`) and
`try_commit_range` (`lib.rs:1580-1582`) now validate `start`/`end` against `page_size()`, same
as the decommit side (only the silent-skip vs reject *behavior* asymmetry remains, and that one
is still documented correctly).

*Concrete consumer impact:* Apple-Silicon macOS (16 KiB pages). A consumer following README's
contract works in 4 KiB units and calls `recommit(base, 4096, 8192)` — previously a well-formed
no-op `Ok(())`, now `Err(VmemError::invalid_argument())` / `false` per the README-following
expectation. The lib.rs rustdoc is already correct (`lib.rs:1468`, `:1525`: "multiples of the
runtime page size ([`page_size()`])"); only README lags.

*Fix:* one line + one sentence — update `README.md:103` to `page_size()` and reword the note to
cover only the remaining silent-skip-vs-reject asymmetry.

### DOC-2 (MEDIUM) — the Unix exact-size fast path is described as current behavior in three public places, but P-1 made it 32-bit-only

- `README.md:44` (API table): "exact-size mmap fast path on Unix; on fast-path miss … On Unix, a
  fast-path miss holds `size + align` bytes of virtual address space … (measured hit rate:
  34.4% at 64 KiB align, 46.7% at 1 MiB, 56.7% at 4 MiB …)".
- `lib.rs:24-27` (crate-level doc): "On Unix, first tries an ordinary exact-size `mmap` and
  checks whether the kernel happened to place it at an `align`-aligned address (fast path; hit
  rate depends on the OS's placement heuristics …). On a miss (wrong alignment), over-reserves
  `size + align` bytes…".
- `lib.rs:1076-1090` (`reserve_aligned`'s rustdoc): same description + the "**Cost on Unix
  fast-path miss:**" paragraph with the hit-rate numbers.

Since P-1 (task #944), `unix_reserve` gates the fast path behind
`#[cfg(target_pointer_width = "32")]` (`lib.rs:2371`), and `try_reserve_aligned_exact` itself is
compiled only for 32-bit (`lib.rs:2488`). On 64-bit Unix — every realistic consumer — the fast
path **never runs**: every reservation is a single over-reserving `mmap` of `size + align`
(the deliberate outcome P-1 was shipped for). The docs above describe behavior that no longer
exists on 64-bit, and the hit-rate numbers frame a trade-off that no longer applies (on 64-bit
the "miss" cost is paid on *every* reservation by design; on 32-bit the fast path remains and
the numbers are WSL2-specific anyway).

Related minor staleness in the same cluster: `unix_exact_reserve_attempts()`'s doc
("`lib.rs:210-216`") says the counter is "Unix only — always 0 on Windows/miri" — it is now also
always 0 on 64-bit Unix; and the stale citation at `lib.rs:183` (DOC-7) points at the hit-rate
narrative.

*Fix:* reword the three spots to "on 32-bit Unix, first tries an exact-size `mmap` …; on 64-bit
Unix, always over-reserves `size + align` in one `mmap` (the fast path measured 34–57% hit
rates, i.e. 1.87–2.31 syscalls/reservation vs a flat 1.0, so it is enabled only where address
space is scarce)". Keep the hit-rate numbers as the historical justification, clearly scoped.

### DOC-3 (LOW) — README's "never panics" exception list is one item short

`README.md:122-129` states there are exactly "Two exceptions to 'never panics'":
`recommit`/`commit_range` rejecting, and `from_raw_parts`. The G-1 fix (task #947) added a
third: `release()` now panics on a contract-violating `(reservation_len, align)` pair via the
multi-clause assert at `lib.rs:1248-1259` (a null pointer remains a documented no-op). Not
memory-unsafe — the violated contract is already UB — but the README sentence exists precisely
to enumerate this surface for `GlobalAlloc` authors, and `release` runs on their `dealloc` path.
*Fix:* one sentence.

### DOC-4 (LOW) — README's platform caveats and `decommit_lazy` row omit the BSD arms the U-1 fix added

`README.md:53` (`decommit_lazy` row) lists "Linux `MADV_FREE`, macOS/iOS `MADV_FREE_REUSABLE`,
Windows falls back to `decommit`" and `README.md:131-166` ("Platform caveats") covers
Windows / huge pages / Darwin only. The U-1 fix added real BSD arms — FreeBSD/DragonFly
`MADV_FREE` = 5, NetBSD/OpenBSD `MADV_FREE` = 6 (`lib.rs:2665-2671`, `2884-2894`) — and extended
`decommit`'s rustdoc to name all four BSDs for the eager-path advisory-only caveat
(`lib.rs:1344-1363`), but README was not touched: the word "BSD" appears nowhere in README.md.
A BSD consumer reading the landing page has no way to learn that `decommit_lazy` does something
real on their OS while eager `decommit` is advisory-only there.
*Fix:* extend the table row and add one caveat bullet.

### DOC-5 (LOW) — `madv_free_advice`'s own doc comment contradicts its cfg routing for tvOS/watchOS

`lib.rs:2642-2650` (the function's doc): "tvOS/watchOS (same XNU kernel as macOS/iOS):
`MADV_FREE_REUSABLE`'s value comes from XNU, so it MAY work identically there too" — read as a
statement about what this function *does or might do* for those targets. The cfg arms
(`lib.rs:2661-2685`) route `tvos`/`watchos` to the `MADV_DONTNEED` fallback (they are absent
from both the `macos/ios` arm and the `freebsd/…` arms, so they hit the catch-all), and
`decommit_lazy`'s public doc (`lib.rs:1406-1409`) correctly states that tvOS/watchOS fall back
to `MADV_DONTNEED`. The private helper's doc reads as if the XNU value were live there.
*Fix:* reword to "tvOS/watchOS are routed to the `MADV_DONTNEED` fallback (see `decommit_lazy`'s
doc); `MADV_FREE_REUSABLE` is a plausible future widening, not current behavior."

### DOC-6 (LOW) — `SeLockMemoryPrivilege`: "has" vs "enabled"

The Windows huge-page limitation text appears three times (`lib.rs:511-523` on the struct,
`:623-636` on `is_huge`, `:1723-1731` on `reserve_aligned_huge`) and each says the requirement
is that "the calling process **has** `SeLockMemoryPrivilege`". Win32 requires the privilege to
be **enabled** (`AdjustTokenPrivileges` with `SE_PRIVILEGE_ENABLED`) immediately before the
`VirtualAlloc(MEM_LARGE_PAGES)` call — a process with the privilege granted-but-not-enabled
fails exactly like an unprivileged one and silently falls back to ordinary pages. As written, a
consumer who granted the privilege via secpol and did nothing else will read conditions 1–3 as
satisfied, expect `is_huge() == true`, and always observe `false` with no documented reason.
The crate cannot (and should not) enable the token itself; the doc just needs one clause:
"…and has **enabled** it via `AdjustTokenPrivileges` (the crate does not do this for you)".

### DOC-7 (INFO) — two stale internal line-number citations (self-identified anti-pattern class)

- `lib.rs:183`: "At real hit rates (34.4%-56.7%, see lib.rs:882-885)" — `:882-885` is now inside
  `from_raw_parts`' `# Safety` docs; the hit-rate narrative lives at `:1086-1090`.
- `lib.rs:2913`: "…poison the decommit-offset rounding callers are told to base on
  `page_size()` (`:227-230`)" — `:227-230` is now the `UNIX_EXACT_RESERVE_ATTEMPTS` doc/static;
  the validation guard is at `:402-408`.

Round-2's D-4 fixed two other sites of this exact class; these two remain. The crate's own
`tests/smoke.rs:77-86` documents the pattern's cost ("two prior line-range citations … both
drifted stale within one round of being written") and the established mitigation (cite by
symbol name, as `smoke.rs` now does). *Fix:* replace both with symbol-name citations.

### API-1 (INFO) — `validate_page_size_public`: naming and visibility of the bench-internals test hook

`lib.rs:395-397`: the T-5 extraction (task #949) made page-size validation testable by exposing
it as `pub fn validate_page_size_public` under `bench-internals`. Two nits before the feature
surface locks: (a) the `_public` suffix is a naming smell — no other item in the crate carries
it; `validate_page_size` would do (the private impl is `validate_page_size_impl`); (b) the
sibling diagnostic counters are `#[doc(hidden)]` while this fully-documented test hook is not —
under `bench-internals` it renders as ordinary reference API on docs.rs's feature-conditional
pages and in IDE completion for anyone who enables the feature. Consider `#[doc(hidden)]` to
match the counter-family convention.

### NUM-1 (INFO) — `end - start` in the per-OS impls relies entirely on caller-side validation

`lib.rs:2119` (Windows `decommit_pages_impl`), `:2132` (Windows `recommit_pages_impl`),
`:2575` (Unix `decommit_pages_impl`): `let len = end - start;` with no local guard. All current
callers validate `start <= end` before the call (`lib.rs:1366/1434/1490/1581` + the safe
`Reservation` methods forward through them), so **no reachable bug exists today** — but the
invariant lives entirely at a distance, and the crate's own convention elsewhere in this file is
to place a cheap `debug_assert!` at exactly such internal boundaries (e.g. `lib.rs:2434`). One
`debug_assert!(start <= end)` per impl would make the next refactor that adds an internal
caller fail loudly in debug instead of handing a wrapped length to `VirtualFree`/`madvise`.
(Four agent-flagged variants of this were merged here; the agents' concrete overflow scenarios
were partly incorrect — `usize::MAX - (usize::MAX - 4096)` is `4096`, no wrap — the only
wrapping input is `start > end`, which is validated. The defense-in-depth suggestion stands.)

### PERF-1 (INFO) — Windows huge path: doomed `MEM_LARGE_PAGES` syscall is avoidable for the size-mismatch failure class

`lib.rs:1905-1947`: on the single-call path with `huge-pages` enabled, a request whose `size`
is not a multiple of the system's large-page minimum (`GetLargePageMinimum()`, 2 MiB on
x86_64) pays a guaranteed-failing `VirtualAlloc(MEM_RESERVE|MEM_COMMIT|MEM_LARGE_PAGES)` plus
the ordinary retry — 2 syscalls — on **every** such call, forever, before the documented
fallback engages. Querying `GetLargePageMinimum()` once (it is another `kernel32` export the
crate already links) and short-circuiting the large-page flag when `size % min != 0` removes
the doomed syscall for that failure class; privilege failures (the other common class) still
need the retry. Best-effort contract and `is_huge()` semantics unchanged. Micro, but it is on
the reservation hot path for any huge-pages consumer with mixed sizes.

---

## 3. Carried-open items (not re-litigated, listed for completeness)

| Item | Owner / record |
|---|---|
| `mock` as Cargo feature vs `--cfg` flag | CORRECTNESS_OPEN_ITEMS item 42, URGENT, maintainer call; the 0.2.0 publish gate is the recorded deadline ("settle the `--cfg` question before 0.2.0 publishes if it is going to be settled at all") |
| Darwin/BSD eager-decommit zero-fill gap (needs `MAP_FIXED` re-mmap design) | item 48; rustdoc + README caveat current; fix deferred to its own round |
| P-2: `mmap` aligned address hint (would let the 32-bit-only fast path approach a ~1.0 hit rate if ever re-enabled broadly) | deferred, CHANGELOG round-2 entry |
| P-3: `VirtualAlloc2` + `MemExtendedParameterAddressRequirements` — collapses the Windows 2-syscall + `size+align` over-reserve for `align > 64 KiB` (the flagship 2/4 MiB segment case) | deferred, CHANGELOG round-2 entry |
| F-2: `mock` silently disables `fault-injection` (documented in a code comment only, not in the feature's public docs) | known LOW from round 2 |
| F-3: no fault-injection seam on `try_recommit` (the most crash-prone real path has only the `mock` seam) | known LOW from round 2 |
| `FAIL_AT_COUNTER` wrap at 2^32 calls while a target is armed-but-unreached | known INFO from round 2 |
| `arm_fail_at` disarm-vs-rearm race (single armer assumed) | scoped out in `fault_injection.rs:47-57` |

---

## 4. Tests — assessment

No new test findings survived verification (one delegated claim about CI gating was refuted —
see §6.4). Positive results on the record:

- The five round-2 additions (task #949) are genuine: the Windows lazy two-call oracle
  (`tests/lazy_commit.rs:364-407`) hard-asserts both counter directions and a write/read of the
  committed prefix; the 64 KiB `is_huge() == false` hard-assert
  (`tests/huge_pages.rs:158-184`) is airtight by construction (`GetLargePageMinimum()` ≥ 2 MiB
  ⇒ a 64 KiB large-page grant is impossible, so `false` is the only correct answer on any
  host); the over-reserved-span decommit/recommit test (`tests/smoke.rs:295-340`) exercises the
  one structural shape (nonzero head offset) the suite never saw before; the `release(NULL)`
  no-op test (`tests/mock.rs:409-421`) asserts both the no-crash and the no-record halves
  (the no-record half is what catches removal of the null early-return); the page-size
  validation test pins the valid-value, zero, non-pow2, and sub-`PAGE` arms of the extracted
  pure function.
- CI coverage for the feature matrix is real: the Windows (`ci.yml:790`) and macOS (`ci.yml:822`)
  jobs each run `cargo test -p aligned-vmem --features "lazy-commit huge-pages
  fault-injection bench-internals"` (mock excluded deliberately, with the reason written at the
  step), and a separate `--all-features` row covers the mock backend itself; the fault-injection
  suite has its own no-mock row (`ci.yml:918`).
- The nine `from_raw_parts` `#[should_panic]` tests all carry `expected =` messages; the weak
  oracle in `ordinary_reservation_never_reports_huge` is self-documented as such in its own doc
  comment, which also names the real regression guard — the crate's honest-vacuity convention
  working as intended.

---

## 5. Manifest, benches, examples — notes

- `bench-scale-tool = "0.1"` (dev-dep) resolves from the crates.io registry with a lockfile
  checksum (`Cargo.lock:64-67`) — no unpublished-dev-dep hazard for `cargo publish`.
- The docs.rs metadata (`Cargo.toml:26-28`) deliberately excludes `mock` and `bench-internals`
  from the rendered feature set, with the reasoning recorded — still correct.
- `benches/vmem_bench.rs` and `examples/v20_849_unix_exact_reserve_hit_rate.rs`: unchanged since
  round 2's assessment (B-1/B-2 there); the example's single-Bernoulli-trial methodology note
  remains exemplary. One consequence of P-1 worth knowing when reading that example in the
  future: it now measures a path (exact-size attempts) that only executes on 32-bit targets.

---

## 6. Verified-good / null results (and refuted delegated claims)

Recorded so the absence of findings is on the record, and so refuted agent claims cannot
resurface as folklore.

**6.1 FFI & soundness (re-audited this round):** Windows `extern "system"` declarations and the
`#[repr(C)] SYSTEM_INFO` layout match the Win32 headers (field order/widths incl. the
`DWORD_PTR` mask); Unix `extern "C"` declarations match `mmap`/`munmap`/`madvise`/`sysconf`,
with the two-arm `OffT` width alias classifying every `cfg(unix)` target. Every `Err` path
captures `errno`/`GetLastError` immediately after the failing syscall and before cleanup FFI.
Every early return between a successful reserve and function exit releases exactly once. Drop /
`into_parts` / `into_reservation_parts` / `release` / `release_parts` cannot double-free.
`unsafe impl Send` for `Reservation` is justified; `Sync` correctly withheld. Strict provenance
is used consistently (`.addr()` reads, `.with_addr()` derivation); **refuted**: a delegated
claim that `p.addr() == MAP_FAILED` is a strict-provenance violation — address comparison via
`.addr()` is precisely the sanctioned non-exposing form, and the choice is documented
(task #776/F8).

**6.2 Arithmetic:** all `size + align` paths use `checked_add` (`lib.rs:1110`, `:2022`,
`:2036`, `:2377`); fit computations are fully checked (`:2062-2066`, `:2413-2417`);
`align_up_addr` is checked; `21 << 26` fits `i32`; the `u32→usize` widenings in
`query_os_page_size` are lossless; `error.rs`'s `c as u32` round-trips safely through
`i32::try_from` in the `io::Error` bridge. (See NUM-1 for the one boundary worth an assert.)

**6.3 Syscall counts (Tier E, re-confirmed):** release = 1 (`munmap` / one
`VirtualFree(MEM_RELEASE)`); decommit = 1 (`madvise` / `VirtualFree(MEM_DECOMMIT)`); recommit =
1 on Windows, 0 on Unix; `commit_range` = 1 on Windows, 0 on Unix; reserve = 1 on the Windows
single-call path and on 64-bit Unix, 2 on the Windows two-call path, 1–3 on 32-bit Unix
(P-1's whole point). `page_size()` is `#[inline]` + cached (one relaxed load). Zero heap
allocations on any real backend path. No missed-`#[inline]` or redundant-validation cost found
on per-op paths — the new safe `Reservation` methods bounds-check once and the free fn's
alignment checks are the API contract itself, not duplication.

**6.4 Refuted delegated claims** (checked against the tree, listed to close them):
- "`bench-internals` is not part of `--all-features` / the Windows lazy oracle never runs in
  CI" — false on both halves: `--all-features` includes every declared feature, and the
  dedicated no-mock test rows (`ci.yml:790` Windows, `:822` macOS) enable `bench-internals`
  explicitly.
- "`Reservation`/`VmemError` lack `#[non_exhaustive]` (semver hazard)" — not applicable: both
  structs have entirely private fields, so adding fields is semver-minor; `non_exhaustive`
  guards literal construction/pattern-matching, which private fields already prevent.
  `ReservationParts` and `mock::Call` (enum + every variant) carry it correctly where it
  matters.
- "`release(NULL)` test would survive removal of the null check" — false: with the check
  removed, `mock::record` fires and the test's no-record assertion fails (and `NonNull::
  new_unchecked(null)` is UB that miri flags).
- "`ordinary_reservation_never_reports_huge` is a vacuous oracle worth flagging" — it is weak,
  but the test's own doc comment already says so in writing, names the mutation it cannot catch,
  and points to the real regression guard; flagging it again adds nothing.
- U-FFI-1's overflow scenario (see NUM-1) and U-FFI-2 (see §6.1).

---

## 7. Findings index

| ID | Sev | Area | One-line |
|---|---|---|---|
| **DOC-1** | **MEDIUM** | README | Alignment contract still says `recommit`/`commit_range` take `PAGE` multiples; code validates `page_size()` since A-1 (`README.md:103` vs `lib.rs:1489/1580`) |
| **DOC-2** | **MEDIUM** | docs | Crate doc + `reserve_aligned` rustdoc + README describe the Unix exact-size fast path (with hit rates) as current; P-1 made it 32-bit-only (`lib.rs:24-27`, `:1076-1090`, `README.md:44` vs `lib.rs:2371/2488`) |
| DOC-3 | LOW | README | "Two exceptions to never panics" — `release()`'s assert (G-1) is a third (`README.md:122-129` vs `lib.rs:1248-1259`) |
| DOC-4 | LOW | README | Platform caveats + `decommit_lazy` row omit the BSD `MADV_FREE` arms U-1 added; "BSD" appears nowhere in README |
| DOC-5 | LOW | rustdoc | `madv_free_advice` doc implies tvOS/watchOS may use `MADV_FREE_REUSABLE`; cfg arms route them to `MADV_DONTNEED` (`lib.rs:2642-2650` vs `:2673-2685`) |
| DOC-6 | LOW | rustdoc | "process has `SeLockMemoryPrivilege`" (×3) — must be *enabled* via `AdjustTokenPrivileges`; granted-but-not-enabled always silently falls back (`lib.rs:515/627/1728`) |
| DOC-7 | INFO | docs | Two stale internal line-number citations (`lib.rs:183`, `:2913`) of the class round-2 D-4 fixed elsewhere |
| API-1 | INFO | API | `validate_page_size_public` — `_public` suffix + not `#[doc(hidden)]` while sibling counters are (`lib.rs:395-397`) |
| NUM-1 | INFO | robustness | `end - start` in per-OS impls relies on distant caller validation; add `debug_assert!(start <= end)` per the file's own convention (`lib.rs:2119/2132/2575`) |
| PERF-1 | INFO | Windows perf | Huge path pays a doomed `MEM_LARGE_PAGES` syscall on every size-not-multiple request; a cached `GetLargePageMinimum()` pre-check removes that failure class (`lib.rs:1905-1947`) |

**Recommendation:** fix DOC-1 and DOC-2 (one short commit each) plus the one-liners DOC-3…DOC-7
before publishing 0.2.0; API-1/NUM-1/PERF-1 can ride the next round. No code change is required
for soundness or correctness by this review.
