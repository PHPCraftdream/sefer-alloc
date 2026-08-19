# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.2.0 - Unreleased

### Added

- **`impl Debug for LazyReservation`** (task #1107) — prints `committed_len()`
  FIRST, then the inner reservation through its own `Debug`. The watermark is
  the type's whole reason to exist and was previously invisible in a panic
  message or an `{:?}`. Written so as not to reintroduce the H1 bypass removed
  in the same wave: the inner handle is rendered, never borrowed out, and the
  guard that pins that property passes unmodified.
- Safe `Reservation` methods for page-level memory management: `decommit`,
  `try_decommit`, `decommit_lazy`, `recommit`, `try_recommit`, `commit_range`,
  `try_commit_range` — **all seven take `&mut self`**; see the BREAKING entry
  under "Changed" for why, and for the migration. (Until task #1120 this line
  listed six of the seven, omitted `try_decommit`, and described the
  pre-#1113 `&self` receivers.)
- **`LazyReservation`** — a `Reservation` that also tracks how much of itself is
  committed, so callers of the lazy path no longer have to invent that
  bookkeeping. Exactly one number is tracked, a watermark: `[0, committed_len())`
  is committed. `ensure_committed(len)` is idempotent and monotone (a call at or
  below the watermark issues no syscall and no error), which is what removes the
  "did I already commit this?" question entirely; it rounds UP to a page, so
  `len` need not be page-aligned. `shrink_committed(len)` also rounds UP, so a
  page holding bytes you asked to KEEP is never released.
  `into_reservation()` is the explicit door out for callers keeping their own
  commit state. The mutators take `&mut self` — not bookkeeping hygiene, but the
  crate stating a requirement it always had: the watermark is racy, concurrent
  committers must serialise, and under the raw primitives nothing ever said so.
  Arbitrary committed/uncommitted HOLES are deliberately not representable —
  that is a per-page bitmap, i.e. an allocator's job, not this crate's. (item 66
  / R7-2)
- **`lazy_commit_is_honored()`** (`const fn`) — whether this platform's backend
  actually honors `initial_commit`, i.e. whether "lazy" is real here. Only the
  Windows native backend performs a genuine two-phase
  reserve-then-commit-prefix; Unix delegates to the eager path, miri models no
  RSS, and the mock backend chains to eager deliberately. Previously this was
  discoverable only by reading the backend source. Third member of the family
  `decommit_reclaims_and_zeroes()` / `is_huge()`: where a platform difference
  exists, expose it as something to branch on rather than a caveat in prose.
  `LazyReservation` derives its initial watermark FROM this query, which is what
  makes the two incapable of disagreeing.
- **`try_decommit()`** — the fallible twin `decommit` never had. Of this crate's
  state-changing primitives, `decommit`/`decommit_lazy` were the only pair with
  no `try_*` form, and also the only ones that silently do nothing on a contract
  violation: silent AND unreportable in one place. An EMPTY page-aligned range
  is a well-formed no-op returning `Ok(())`, distinguished from a violation —
  `decommit`'s single `start >= end` early return conflates the two. The OS
  refusing or ignoring the request is deliberately NOT an error: decommit is
  best-effort by nature, and reporting that as `Err` would promise a portable
  guarantee the platforms do not give.
- **`Reservation::try_decommit()`** — the safe-method twin of the free
  `try_decommit()` above, completing the family at both layers the way
  `recommit`/`try_recommit` and `commit_range`/`try_commit_range` already
  were: `Ok(())` on success or a well-formed no-op,
  `Err(VmemError::invalid_argument())` on a violated range (misaligned,
  `start > end`, or `end > self.len()`), never panics on any profile.
  Huge-page reservations take the same early-exit as `Reservation::decommit`
  (skip the backend call, count the attempt, `Ok(())` — OS refusal is
  deliberately not an error). Closes the doc-trail dead end where
  `Reservation::decommit`'s forwarded tripwire message says "Use
  try_decommit for the fallible form", pointing safe-API callers at an
  `unsafe fn` with a raw-pointer signature (task #1079). Purely additive:
  no existing signature changed.
- `MIN_PAGE` constant as an alias for `PAGE` with clearer semantics
- `page_size()` function to query the actual OS page size at runtime
- `ReservationParts` typed wrapper and `into_reservation_parts()` method
- `Reservation::into_reservation_parts()` typed form for manual release
- `ReservationFullParts` + `Reservation::into_full_parts()` — lossless six-field
  round-trip that preserves `base`, usable `len` and `granted_huge`, which
  `ReservationParts` discards (R4-11)
- `Reservation::decommit_reclaims_and_zeroes()` — `const fn` capability query
  reporting whether the current platform's ordinary native backend actually delivers
  decommit's reclaim + zero-fill semantics (`false` on Darwin and the four BSDs,
  where `MADV_DONTNEED` is advisory-only, and under miri where the backend is a no-op).
  Makes a guarantee that was previously documented in prose only something callers
  can branch on (R4-3, R5-1)
- `Reservation::can_decommit_reclaim_and_zero()` — instance-level query that combines
  the platform guarantee with the reservation's huge-page status, returning `false`
  for huge-page reservations where decommit silently fails (R5-1)
- `VmemError::last_os_error()` for OS error capture with preserved errno
- `bench-internals` feature with diagnostic counters for path activation:
  - `unix_exact_reserve_attempts()` / `unix_exact_reserve_hits()`
  - `windows_reserve_commit_calls()` / `windows_reserve_commit_single_calls()` / `windows_reserve_commit_two_call_pairs()`
  - `windows_large_page_retry_failures()` / `windows_large_page_alignment_failures()` — separate counters for "both initial and retry failed" vs "succeeded but misaligned" large-page failure modes (R4-5/R5-4)
  - `unix_madvise_attempts()` / `unix_madvise_successes()`
  - `windows_virtualfree_decommit_attempts()` / `windows_virtualfree_decommit_failures()`
  - `windows_virtualfree_release_failures()`
  - `unix_munmap_failures()`
  - `huge_decommit_attempts()` for tracking decommit calls on huge-page reservations
  - `windows_large_page_plain_fallback_successes()` — the "large-page attempt failed, ordinary-page retry succeeded" case, which neither `windows_reserve_commit_single_calls()` (counts logical completion regardless of which attempt won) nor `windows_large_page_retry_failures()` (counts only when BOTH failed) can distinguish (R7-3)
  - `reset_bench_internals_counters()`
  - `validate_page_size()` for testing page size validation logic
- Mock backend converted from Cargo feature to build-time `--cfg aligned_vmem_mock` flag (no Cargo feature unification risk)
- `fault-injection` feature for deterministic OOM testing on the real commit path
- `try_reserve_aligned()` / `try_reserve_aligned_huge()` / `try_reserve_aligned_lazy()` fallible forms returning `Result<_, VmemError>`
- `reserve_aligned_huge()` for requesting OS large pages (Linux `MAP_HUGETLB`, Windows `MEM_LARGE_PAGES`)

### Changed

- **BREAKING (pre-publish): `reserve_aligned_lazy` / `try_reserve_aligned_lazy`
  now return `LazyReservation`** instead of `Reservation`. Callers that keep
  their own commit bookkeeping add `.into_reservation()`. Chosen over adding a
  second parallel constructor so there is ONE obvious way to make a lazy
  reservation; 0.2.0 is unpublished, so the change costs nothing.
- **[BREAKING, HIGH] `Reservation`'s seven OS-state mutators now take
  `&mut self`: `decommit`, `try_decommit`, `decommit_lazy`, `recommit`,
  `try_recommit`, `commit_range`, `try_commit_range` (task #1113).** This is
  what actually closes the H1 watermark-bypass class recorded under "Fixed"
  below. Removing the one
  accessor that leaked a `&Reservation` (task #1104) left the CLASS open: an
  independent reviewer reopened it four ways — a public field, a trait method
  returning `&crate::Reservation`, `impl AsRef<crate::Reservation>`, and a
  `&'a Reservation` newtype — each with a working exploit performing a real
  `VirtualFree(MEM_DECOMMIT)` from 100%-safe code while the text guard stayed
  green. With `&mut self`, a leaked shared borrow is READ-ONLY and the class is
  closed by the type system rather than policed by a string scan
  (`error[E0596]: cannot borrow *shared as mutable`).
  **Migration:** bind the reservation as `let mut r = ...` at the call site.
  If a `Reservation` is held behind a shared reference — inside a struct whose
  method takes `&self`, inside an `Rc`, or captured by a `Fn` closure — those
  three patterns no longer compile and need `RefCell`/`Mutex` (single-threaded
  and cross-thread respectively) or a `&mut self` method. Note that
  `Reservation` being `Send + !Sync` does NOT cover this: `Sync` governs
  cross-thread sharing, while all three broken patterns are single-threaded
  aliasing. Zero call sites outside this crate's own tests needed changing —
  `crates/numa-shim`, `examples/`, `benches/` and the root crate all use the
  free `pub unsafe fn` API, which is unaffected.
- **`decommit` now `debug_assert!`s on a range-contract violation.** Its
  signature returns `()` and has nowhere to report one, so a debug build says so
  loudly and a consumer's own test fails at the mistake instead of quietly
  decommitting nothing. Zero cost in release; the fallible form is
  `try_decommit`.
- `page_size()` granularity is now used for decommit/recommit validation instead of compile-time `PAGE` constant
- **lazy-commit contract tightened:** `reserve_aligned_lazy` now requires both `size` and `initial_commit` to be multiples of the runtime `page_size()` (not just `PAGE`). This prevents unwritable tails on systems where `page_size() > PAGE` (e.g., 64 KiB Windows configurations or 16 KiB macOS), where `commit_range` only accepts page_size()-aligned offsets. Mainstream Windows (page_size() == 4096) is unaffected. (R6-2)
- All OS error paths now capture `errno`/`GetLastError` immediately before cleanup FFI
- Unix 64-bit fast path disabled: syscall economy (one `mmap` call at the cost of extra virtual address space held per reservation). Exception on Linux: when `align == LINUX_HUGE_PAGE_SIZE` with huge pages requested, an exact-size `MAP_HUGETLB` fast path avoids the over-reserve (kernel guarantees huge-page-aligned base).
- `reserve_aligned_huge()` semantics fixed: reports actual huge-page grant only on platforms where it's observable
- `granted_huge` tracking added for Linux huge pages; non-Linux Unix correctly reports `is_huge() == false`
- **BREAKING**: `Reservation::from_raw_parts` signature changed to require new `granted_huge: bool` parameter
- `decommit`/`recommit` contract narrowed on Darwin/BSDs: explicitly best-effort hint with no zero-fill guarantee (Linux/Windows guarantees unchanged)
- Windows single-call fast path for `align <= WIN_ALLOCATION_GRANULARITY` (typically 64 KiB) on full-span commit; when requesting large pages (`MEM_LARGE_PAGES`), the threshold widens to `GetLargePageMinimum()` (typically 2 MiB).

### Fixed

- **[BREAKING, HIGH, publication blocker] `LazyReservation::as_reservation()` is
  REMOVED: it was a 100%-safe bypass of the watermark the type exists to
  guarantee (task #1104, finding H1 of the 2026-08-18 publication-readiness
  audit, whose NO-GO verdict rested on it).** At the time, all seven of
  `Reservation`'s OS-state mutators took `&self`, not `&mut self` — see the
  BREAKING entry above, which changed that and is what actually closes this
  class — so a borrowed `&Reservation`
  handed out by a `&self` accessor let safe code change the mapping under the
  watermark: `r.as_reservation().decommit(0, page_size())` left
  `committed_len()` promising a prefix that Windows had already decommitted —
  a caller range-checking against it before writing gets
  `STATUS_ACCESS_VIOLATION`. Committing PAST the watermark via
  `commit_range`/`recommit` through the same borrow was equally available.
  Deleted rather than narrowed to a read-only view type: its only caller in the
  entire repository needed `len()`, already proxied directly, and a view type is
  one `impl Deref<Target = Reservation>` away from resurrecting every mutator.
  The two remaining exits are deliberate and documented on the type — the raw
  `as_ptr()` (every USE of which is already `unsafe`) and the CONSUMING
  `into_reservation()`. Pinned by `tests/lazy_reservation_no_borrowed_reservation.rs`,
  which fails on the token, on any `&Reservation` code line in `src/`, on a
  `Deref`/`AsRef`/`Borrow` route, or on any change to the type's public method
  set. Affects only consumers built against the unpublished 0.2.0 tree;
  crates.io still serves 0.1.0, which never had `LazyReservation`.

- **[docs, MEDIUM, publish blocker] the public huge-page contract promised a
  Linux-only exception that the code applies to Android as well (task #1105,
  finding M1).** `reserve_aligned_huge`'s rustdoc and the README said the extra
  2 MiB-multiple requirement holds "except on Linux with `huge-pages` enabled",
  while the check is gated
  `any(target_os = "linux", target_os = "android")` — so an Android caller got
  `invalid_argument()` for a request the published docs said would be accepted.
  The crate's own tests already treated the contract as Linux/Android-common:
  the tests were right and the shipping documentation was wrong. 35 sites
  corrected across the rustdoc, `os/unix.rs`, `lib.rs` and the README, sweeping
  BOTH directions (three "non-Linux Unix" phrasings were wrong about Android;
  four comments described a `target_os = "linux"` cfg arm that does not exist).
  Android is not built in CI, so no test can catch this class — a new
  `scripts/vmem-linux-android-pairing-guard.mjs` flags a sentence naming Linux
  next to a pair-gated mechanism marker with no Android satisfier.

- **[docs, LOW] three publication-audit discrepancies in documentation and the
  error model (task #1106, findings L1/L2/L3).** (1) The `fault-injection`
  module doc claimed `try_commit_range` "always" reaches the real backend while
  its own function doc 100 lines down said the hook is compiled out under
  `aligned_vmem_mock` — five CI rows build exactly that combination; the doc now
  states the precedence, and a `compile_error!` was deliberately NOT used
  because those rows are intentional. (2) A SUCCESSFUL zero-address `mmap`,
  which the crate rejects and unmaps, was reported through
  `os_refusal_unknown_code()` and documented as a "genuine OS refusal"; the
  doc and `Display` now distinguish "the crate rejected an unusable OS grant"
  from a real refusal. A distinct error kind was considered and rejected on
  evidence: no site anywhere matches on which source produced the sentinel.
  (3) The README promised the whole `cfg(unix)` family while `os/unix.rs`
  holds `compile_error!` arms rejecting every Unix family outside
  Linux/Android and Darwin/BSD, and all of MIPS — replaced by an enumerated
  matrix, each row keyed to the arm enforcing it, with CI-verified separated
  from reasoned-from-spec.

- **[correctness fix, MEDIUM, publish blocker] `set_page_size_override` accepted
  a page SMALLER than the machine's real one, while the module's own "Why this
  is a safe `fn`" section claimed — WITHOUT qualification — that the override
  "can only make validation STRICTER ... never accepts one the real page would
  reject" (task #1085 / finding M1).** On a 16/64 KiB-page host (macOS ARM64,
  aarch64-64k Linux) `set_page_size_override(Some(4096))` was accepted, so
  `Reservation::decommit(0, 4096)` passed range validation and reached
  `madvise(base, 4096, MADV_DONTNEED)` — where the kernel rounds the LENGTH up
  to the real page and discards all 16 KiB, destroying 12 KiB of live data
  outside the requested range through a 100%-safe API. The setter now also
  requires `ps >=` the real page, queried FRESH so the check bypasses the
  override cache (comparing against the cache would wrongly reject a legal
  64 KiB → 16 KiB downshift while failing to pin the invariant that matters).
  The safety section is rewritten from unconditional to conditional and records
  the one residual assumption honestly: if the OS query itself fails on a
  big-page host the floor degrades to `PAGE` — the same degraded assumption
  `page_size()` already makes with no override armed. The seam stays behind
  `--cfg aligned_vmem_page_size_override` and is unreachable in a shipped build;
  the fix exists because the SAFETY CLAIM was false as written, and a crate
  about to be published should have that claim true by construction.

  **Follow-up (task #1095 / finding H1, same wave): the regression test above
  ran in NO gate row, locally or in CI, until this wave.**
  `tests/page_size_override.rs` is `#![cfg(aligned_vmem_page_size_override)]`-
  gated at the file level, and the six places that set that cfg all target
  ROOT-crate test targets (`--test lazy_initial_commit_forced_page` et al.):
  RUSTFLAGS reaches this crate's LIBRARY there, but cargo never builds this
  PACKAGE's own integration-test targets for them, while every row that does
  build them runs without the cfg. So the only oracle pinning a declared
  publication blocker executed nowhere, and `cargo test -p aligned-vmem
  --all-features --test page_size_override` printed "running 0 tests", exit 0.
  Two self-verifying rows were added — one in `scripts/check-all.mjs` carrying
  `expectTest: 'override_floor_is_the_real_os_page_size'`, one in ci.yml's
  `aligned-vmem-gates` job with the equivalent `tee` + `grep -F` postcondition
  — so that a lost or typo'd cfg turns the row RED instead of green-and-dead.

- **[correctness fix, MEDIUM, publish blocker] `Reservation::try_decommit`
  answered `Ok(())` for a malformed range on a huge-page reservation (task
  #1084 / finding M3).** The `is_huge()` early return sat ahead of ALL range
  validation, while the free `try_decommit` validates first and this method's
  own rustdoc promises `Err(VmemError::invalid_argument())` on a contract
  violation. A caller who deliberately chose the FALLIBLE form precisely to
  detect a bad range was told everything was fine. Validation now runs before
  the skip, using the exact negation of the free function's own predicate (which
  re-checks after the forward, so the two layers cannot drift apart silently).
  Behavior changes only for huge + malformed; the eager `decommit` is
  deliberately unchanged and its huge-path silence is now documented instead.

- **[docs, MEDIUM, publish blocker] `Reservation::decommit`'s `# Panics` section
  was itself false about empty ranges (task #1084 / finding M2).** It claimed
  "Empty and out-of-bounds ranges are checked by this method first and never
  panic"; only the out-of-bounds half was true. There is no empty-range check in
  the body, so `decommit(1, 1)` — empty, in bounds, MISALIGNED — forwards to the
  free function and trips its `debug_assert!`, a panic reachable from 100%-safe
  code in every debug build. Resolved by correcting the DOC, not the code: the
  free predicate deliberately blesses only page-ALIGNED empty ranges, the
  method's single pre-check exists to uphold the forwarded `unsafe fn`'s safety
  contract (emptiness is not a safety matter), and — decisive — widening the
  predicate to bless any empty range would silently flip the free
  `try_decommit(1, 1)` from `Err` to `Ok`, weakening exactly the validation the
  M3 fix above strengthens. A method-layer `start == end` filter is additionally
  the shape task #1079 already rejected for disarming the tripwire. Note the
  coverage gap this surfaced and did NOT close (filed as task #1094): nothing
  pins the free `try_decommit(1, 1)` → `Err`, so that flip would pass
  unnoticed. (Gap closed in the same wave by task #1094:
  `empty_misaligned_range_is_reported` in `tests/try_decommit.rs` pins
  `(1, 1)` and `(ps + 1, ps + 1)` → `Err`, with the widening re-applied
  temporarily to show the new test red under it.)

- **[docs, LOW] `Reservation::decommit`'s SUMMARY line still made an
  unqualified claim about empty misaligned ranges two paragraphs above its
  own corrected `# Panics` section (task #1097 / finding L4).** The M2 fix
  above rewrote `# Panics` but left the summary saying an empty MISALIGNED
  range "is a contract violation like any other" with no profile split —
  while in a RELEASE build `decommit(1, 1)` IS a silent no-op (the free
  function returns at `start >= end` once the `debug_assert!` is compiled
  out), so a consumer skimming only the summary read a violation promise
  the release build does not keep. The summary now states the profile
  split inline. The same unconditional shape was swept across the family:
  the free `decommit`'s "a no-op if the range is empty" became "empty AND
  page-aligned" with the empty-misaligned case named in the violated-range
  clause, and the `recommit`/`commit_range` "well-formed no-op (empty
  range, `start == end`)" parentheticals at both layers became "an empty
  PAGE-ALIGNED range" (the old parenthetical implied empty ⇒ well-formed,
  false for misaligned endpoints). `decommit_lazy` (both layers) and both
  `try_decommit` layers were already qualified — reviewed, unchanged.

- **[docs + test, LOW] the synthetic-`granted_huge` SAFETY comment and the
  pre-fix counter figures in `tests/reservation_decommit_contract.rs`, and
  the forced-page-suite description in `tests/page_size_override.rs`
  (tasks #1098/L2 and #1096/L1).** The SAFETY comment on
  `method_try_decommit_reports_malformed_range_on_huge_flagged_reservation`
  now discloses BOTH `from_raw_parts` contract bullets its synthesis
  violates (the accuracy bullet AND the Windows single-call
  `MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES` commit-state bullet —
  previously only the first was cited), and its PRIMARY justification is
  now the re-derived reader enumeration — `is_huge()` itself,
  `can_decommit_reclaim_and_zero()` (a pure query the review's own
  enumeration had missed), and the three decommit-family huge-skips;
  `Drop` and `from_raw_parts`' assert block read nothing — instead of the
  weaker "the rustdoc says it is not UB" intent argument. A new mechanical
  guard, `tests/granted_huge_reader_enumeration.rs`, pins that reader set
  so a future reader breaks a stated invariant rather than silently
  invalidating prose. The pre-fix `huge_decommit_attempts()` figures
  ("== 3 after the malformed calls", "this read 4") were DERIVED, not
  observed — the pre-fix test aborts at the first `.is_err()` assert with
  the counter at 1 — and are now labeled as such, with the values observed
  for real via a temporarily relaxed probe under the temporarily reverted
  fix order. `tests/page_size_override.rs` claimed the root crate's
  forced-page suites "arm 64 KiB, then downshift to 16 KiB with no restore
  in between"; they actually iterate [16 KiB, 64 KiB] (an UPSHIFT) with
  the restore guard constructed inside the loop, or arm a single 64 KiB —
  comment corrected; the justified design (floor = fresh real-page query,
  not the armed cache value) is unchanged and still correct.

- **`Reservation::decommit`'s rustdoc promised "the same silent-skip behavior
  as the free `decommit` function" for a contract-violating range — half
  true: RELEASE silently skips, but DEBUG panics via the forwarded free
  function's task-#1051 `debug_assert!` tripwire, and the safe method
  carried no `# Panics` section.** The doc now states the profile split
  accurately (mirroring the free function's already-correct "Contract
  violations, by build profile" vocabulary instead of paraphrasing it) and
  carries `# Panics`. Doc-only — no behavior changed; the deliberate choice
  NOT to pre-filter inside the method (which would have made the old doc
  true but disarmed the tripwire for every safe-API caller) is recorded in
  task #1079. `Reservation::decommit_lazy`'s doc gained the matching
  opposite statement: silent no-op on EVERY profile (the task-#1072
  asymmetry). New `tests/reservation_decommit_contract.rs` pins the safe
  METHOD layer's per-profile behavior in both directions — debug tripwire
  fires through the method, release silently skips with a mock call-log
  no-record oracle — since every pre-existing oracle observed only the free
  functions. (task #1079)
- **`page_size_override::set_page_size_override(Option<usize>)`** — test-only
  injection of a simulated runtime page size (e.g. 64 KiB on a 4 KiB host),
  so consumers of this crate can exercise their `page_size()`-validated call
  paths against the exact failure mode that is invisible on small-page hosts
  by construction (values aligned to the compile-time `PAGE` only). Compiled
  solely under the build-time `--cfg aligned_vmem_page_size_override` flag —
  deliberately NOT a Cargo feature, the same task-#962 conversion rationale
  as `--cfg aligned_vmem_mock`: a feature unifies across the dependency
  graph and would let any downstream feature resolution enable the override,
  while a cfg flag is explicit per-build only. Safe by design: the override
  can only make validation STRICTER (every validator compares against
  `page_size()`), OS calls stay legal (a larger power-of-two multiple is
  also a real-page multiple), and misaligned ranges are rejected or silently
  skipped — fail-closed degradation, never UB. First consumer:
  sefer-alloc's forced-page lazy-commit call-site regression
  (`tests/lazy_initial_commit_forced_page.rs`, task #1080). (task #1080)

#### Prerelease audit round 7 (`docs/reviews/2026-08-16-aligned-vmem-prerelease-audit-r7.md`)

Eleven findings, no proven UB or memory-safety defect among them. One code
change with runtime effect (R7-11), one observable-string change (R7-7), one new
diagnostic counter (R7-3), one gate-coverage fix (R7-8); the rest are
documentation corrections or recorded decisions. R7's own verdict was a
CONDITIONAL NO-GO on two conditions — **only one of which is closed here**, see
"Still open" below.

- **Zero-fill after decommit is no longer promised platform-wide (R7-1, release
  blocker, CLOSED).** `Reservation`'s and `as_ptr()`'s rustdoc said a
  decommitted range comes back as fresh zero pages "on Unix". That holds for
  exactly one of four Unix cases. Both sites now carry a per-platform matrix:
  Windows (unmapped, access faults), Linux eager `decommit` (zeroed via
  `MADV_DONTNEED`), Linux `decommit_lazy` (old contents until the kernel
  reclaims; a write cancels the free), Darwin/BSD (advisory, does not reliably
  zero), huge reservations (old contents either way). The huge bullet also
  states the mechanism difference the old text erased: the SAFE methods skip the
  backend call because they can read `is_huge()`, the FREE functions cannot and
  still issue the syscall the OS then ignores — do not read "no-op" as "no
  syscall" for the free functions.
- **`VmemError::invalid_argument`'s `Display` no longer mislabels every
  rejection (R7-7).** Six distinct argument-contract checks return this error,
  but `Display` printed `"size/align contract violation"` for all of them, so a
  rejected commit RANGE reported a size/align fault. Now
  `"argument contract violation"`, with the constructor's doc enumerating all six
  rejecting classes read off the actual call sites.
- **`mmap` returning address zero is no longer treated as failure-and-leaked
  (R7-11).** `libc_mmap` used NULL as its failure signal but only tested for
  `MAP_FAILED`. POSIX does not guarantee `mmap(NULL, ...)` never returns address
  zero; such a mapping would have been read as a failure and never unmapped. The
  two cases are now distinguished and a successful zero-address mapping is
  `munmap`'d before the error is returned. Unix-only path, compiled under
  `x86_64-unknown-linux-gnu` and `i686-unknown-linux-gnu` but never executed —
  no portable test can reach it (it needs a kernel that actually maps at zero).
- **Seven stale documentation sites corrected (R7-9)**, including a README
  fragment orphaned mid-sentence, a `Cargo.toml` comment still calling
  `huge_decommit_attempts` an "upper bound incompatibility rate" (it counts
  early exits since the R6-7 fix) while also claiming the `bench-internals`
  feature did not exist ten lines above its own declaration, and a
  `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS` doc describing a "third best-effort
  retry" that path cannot perform (it never requests `MEM_LARGE_PAGES`).
- **The module doc's "64-bit Unix compiles the exact-size path out entirely"
  now names its exception (R7-9).** The Linux AND Android huge-page path
  (`any(target_os = "linux", target_os = "android")` + `feature = "huge-pages"`,
  `align == 2 MiB`) is not keyed on pointer width and does still fire on 64-bit.
  Both rustdoc sites say so, and say Android explicitly — omitting it is the
  exact shape of an earlier round's R6-3 finding.

##### Recorded decisions, deliberately not code changes

- **R7-4 — NULL a second time.** The claim that a failed Linux/Android huge
  exact-size `mmap` is followed by a guaranteed-to-also-fail second huge attempt
  is false in general: the two calls request DIFFERENT sizes (`size` vs
  `size + align`), so a fragmented or bounded hugetlb pool can satisfy one and
  refuse the other. Recorded with that counterexample as perf open item 52 so a
  third review does not re-raise it. Settling it needs a Linux host with and
  without a configured pool — unreachable from this project's dev host and CI.
- **R7-5 (64-bit Unix retains `size + align` of VA) and R7-10 (backend tuples /
  monolithic `lib.rs`)** — recorded as perf open items 53 and 54. R7 asks
  explicitly that neither be acted on in 0.2.0.
- **R7-6** — the uniform (not `cfg`-gated) lazy validation against the runtime
  `page_size()` needed no new record: `validate_initial_commit`'s own doc
  already states the decision in full, including the half a summary would drop —
  that on a 16 KiB-page host `reserve_aligned_lazy(size, align, PAGE)` is now
  REJECTED where it previously worked.

##### Still open — owner decision, NOT closed by this round

- **R7-2 / correctness open item 66: `Reservation` carries no committed-length
  state,** so a lazy handle's committed prefix is a documented contract rather
  than a checkable one. This is the SECOND of R7's two conditional-NO-GO
  conditions and cannot be closed by code. Five options are recorded on the
  item: a `committed_len` field; a separate `LazyReservation` type; returning
  `(Reservation, usize)`; explicitly ACCEPTING the caller-tracked contract; or
  excluding `lazy-commit` from the supported release profile. All five are cheap
  only while 0.2.0 is unpublished. What both reviews rule out is leaving the
  question unanswered.

- **Lazy reservation documentation corrected:** `Reservation` type and `as_ptr()` docs now explicitly state that lazy reservations on Windows only commit the `initial_commit` prefix; the tail must be committed via `commit_range` before it's writable. README platform caveats updated with this information. (R6-1 variant 1)
- **`from_raw_parts` Windows commit-state documentation rewritten:** No longer inaccurately requires all Windows reservations to be created with `MEM_RESERVE | MEM_COMMIT`. Instead documents that partial-commit (lazy) reservations are valid, and explains the `granted_huge` compatibility requirements. (R6-1 variant 2)
- Android build support added with correct `_SC_PAGESIZE` constant wiring
- 32-bit Linux glibc/musl FFI fix: `off_t` type correctly declared as 64-bit on all musl targets (was mismatched for 32-bit musl)
- BSD `decommit_lazy` no-ops fixed: FreeBSD/DragonFly/NetBSD/OpenBSD now dispatch to their own real `MADV_FREE` constant values instead of an undefined/wrong one
- macOS CI failures caused by `PAGE` vs `page_size()` validation mismatch
- `release()` panic hardened with informative multi-clause assert under miri
- Fixed data race in `SERIAL` mutex for bench-internals tests
- Wire-thread drop split documented in mock module
- FFI struct layout for `SystemInfo` matches real `SYSTEM_INFO` (union head flattened)
- Over-reserve documentation corrected to reflect "no trim" behavior
- Documentation gaps fixed: huge-pages semantics, alignment contract, API completeness
- 32-bit Linux/Android no longer issues the same `MAP_HUGETLB` mmap twice for a
  single 2 MiB-aligned huge reservation: the generic 32-bit exact-size fast path
  is now skipped when the huge-page exact-size path above already attempted it.
  Saves a syscall and a second draw against a scarce hugetlb pool, and makes
  `UNIX_EXACT_RESERVE_ATTEMPTS` count one attempt per logical reserve on every
  platform (R4-2/R3-1)
- MIPS targets now fail to compile with an explanatory `compile_error!` instead
  of building successfully and then failing every `reserve_aligned` call at
  runtime with an undiagnosed `EBADF` (MIPS `MAP_ANON`/`MAP_HUGETLB` values
  differ from the `asm-generic` constants this crate hardcodes) (R4-1)
- `mock::drain()` no longer holds the `RefCell` borrow across the returned
  `Vec`'s allocation, which could reenter `record()` and panic with
  `BorrowMutError`; it now `mem::take`s the log under a short borrow (R4-9)
- `fault_injection`'s one-shot self-disarm can no longer cancel a concurrent
  `arm_fail_at`: the two-atomic target/counter protocol is replaced by a single
  mutex-guarded state, closing the last of the three races this module
  documented (R4-8)
- Three `// SAFETY:` comments on the Windows decommit path claimed the caller
  must pass a COMMITTED range — a precondition the crate deliberately violates
  in a CI-covered test; all now state the real `MEM_RESERVE`d-region contract
- `from_raw_parts`'s contract documentation corrected: it takes six arguments,
  not five, and now requires runtime `page_size()` alignment rather than only
  the compile-time `PAGE` lower bound (R4-10, R4-6)
- `from_raw_parts` documentation fixed to accurately reflect what the constructor's
  `assert!` checks versus what remains the caller's responsibility. The documentation
  previously required `len` and `reservation_len` to be multiples of both `PAGE`
  and `page_size()`, and claimed "both are asserted at construction", but the
  actual `assert!` only checks against `PAGE`. The fix clarifies: (a) logical
  lengths (`len`, `reservation_len`) require only `PAGE` multiple (checked by
  the assert), (b) addresses and operations (`base`, `reservation`, `decommit`/
  `decommit_lazy` arguments) require `page_size()` alignment (NOT checked by the
  assert, remains caller responsibility), and (c) `reservation_len` may under-report
  the actual OS mapping size on hosts where `page_size()` > `PAGE` (harmless for
  correctness, documented now) (R5-2)
- `into_full_parts` documentation fixed: replaced "persists metadata across restarts"
  (misleading — raw pointers don't survive process restarts) with "hands off
  reservations between components within the same process", and added explicit
  warning that dropping or forgetting `ReservationFullParts` does NOT release the
  underlying OS reservation (R5-5)

### Removed

- Deprecated `Reservation::is_empty()` method (use `len() == 0` instead)
- `mock` Cargo feature (replaced by `--cfg aligned_vmem_mock` build flag)
