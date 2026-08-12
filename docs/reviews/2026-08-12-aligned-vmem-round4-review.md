# `aligned-vmem` — round-4 review (fresh pass over the post-#858–#864 tree)

**Date:** 2026-08-12
**Scope:** `crates/vmem/` in full — `src/{lib,error,mock,fault_injection}.rs`, all 7 files under
`tests/`, `benches/vmem_bench.rs`, `examples/v20_849_unix_exact_reserve_hit_rate.rs`,
`Cargo.toml`, `README.md`; plus the `aligned-vmem`-touching parts of
`.github/workflows/ci.yml`, the root `Cargo.toml`, `src/alloc_core/alloc_core_core_diag.rs`,
`src/alloc_core/os.rs`, `CHANGELOG.md`, `docs/perf/OPEN_ITEMS.md` and
`docs/CORRECTNESS_OPEN_ITEMS.md`.
**Reviewed tree:** local `main` @ `8804fc9` (`git status` clean at start).
**Toolchain:** `cargo`/`rustc` stable as installed on this host; Windows 10 Pro x86_64, 4 KiB page.
**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add` / `git commit` / version bump. Every command quoted below was actually run
on this host, and every `file:line` citation was read in the current tree before being written
down.

**One executed experiment, run OUTSIDE the repository.** F1's counterfactual required deleting a
guard from `src/lib.rs`. That was done on a throwaway copy of the crate under `%TEMP%`
(`$TEMP/vmem_cf_r4`, created with `cp -r crates/vmem/.`, `[workspace]` appended so it resolved
standalone, `benches`/`examples` and the `bench-scale-tool` dev-dep stripped so it built without
them). The repository working tree was never touched, and the temp directory was deleted
afterwards. Everything reported from that experiment is a real observed test result, not a
prediction.

**Relationship to the prior three rounds.** This pass does not re-report V1–V21
(`docs/reviews/2026-08-12-aligned-vmem-code-quality-review.md`), W1–W16 + P-A/P-B/P-C
(`docs/reviews/2026-08-12-aligned-vmem-post-campaign-closing-review.md`), or F1–F11
(`docs/reviews/2026-08-12-aligned-vmem-round3-review.md`). All eleven round-3 findings were
spot-checked in the current tree and all eleven landed (see "Checked and explicitly NOT findings").
To stay unambiguous against the `V`-, `W`- and round-3 `F`-series, this round's findings are
numbered **R1…R13**.

**Platform honesty up front.** This host is Windows/x86-64 with a 4 KiB page. R1's counterfactual
was **executed here**, on real Windows, against the real `VirtualAlloc` backend — it is an
observation, not a derivation. R7 and R8 are reasoned from the Linux `mmap(2)` and Win32
`VirtualAlloc2` specifications respectively and are labelled as such. Everything else was read
directly in the current tree.

---

## Verdict up front

**Round 3's own closing note — "approaching diminishing returns after 39 findings" — is right about
the crate's *source* and wrong about its *verification*.** I found nothing new in the `unsafe`
blocks, the atomics, the reservation lifecycle, the provenance discipline, or the error-capture
timing. Three rounds of review are visible in that code and it holds up. The performance null
result re-confirms for Linux and for the Windows fast path.

**But there is one substantial, previously-unseen finding, and it is a verification finding, not a
source finding (R1, HIGH).** `mock` is a *non-additive, backend-replacing* Cargo feature — the
crate documents this at length in three places — and `--all-features` turns it on. Four of the six
`cargo test -p aligned-vmem` invocations in `ci.yml` use `--all-features`, **including both of the
platform rows that rounds 2 and 3 added specifically to cover the platform backends**
(`test-macos`, task #856/W14-2, `ci.yml:802`; `test-windows`, task #858/F2, `ci.yml:777`). Under
`mock`, `decommit`/`recommit`/`commit_range` never reach the OS and `reserve_aligned_lazy` is
re-routed to the **eager** backend (`lib.rs:1241-1244`). So those two rows exercise a thread-local
recording stub, not `VirtualAlloc`/`MEM_DECOMMIT` and not `madvise(MADV_FREE_REUSABLE)`.

The concrete cost, **verified by execution** rather than argued: with `&& commit_len == size`
deleted from `win_reserve_commit` (`lib.rs:1453`) — reintroducing verbatim the bug #848's own
zero-trust review caught and that `tests/lazy_commit.rs:70-116` exists to guard —

| invocation | result |
|---|---|
| `cargo test --features lazy-commit --test lazy_commit` | **FAILED** — 10 passed, 1 failed (`lazy_reserve_small_align_still_reserves_full_span`) |
| `cargo test --all-features --test lazy_commit` | **ok** — 11 passed, 0 failed |

`--all-features` is what CI runs on Windows. Round 3's F2 called that row "the row that makes the
#848 regression test non-vacuous for the first time"; it does not. **The `commit_len == size` guard
has zero non-vacuous coverage on any CI row, on any platform, in any feature configuration** — a
guard on a Windows-only code path protecting a bug that has already happened once.

**Everything else is small.** R2 is the same defect one file over (the `fault-injection` suite's
only CI row is Linux, where the "real backend" it claims to prove coexistence with is a
compile-time `Ok(())`). R3 is round-3-F3 residue of exactly the kind round 3 itself was documenting
— task #859's fix removed one stale claim from the root crate's forwarding accessors and
introduced two new wrong ones. R4/R5 are a coverage gap and an API decision that should be settled
before the next publish. R6–R13 are doc drift, one named perf opportunity, and process.

**Publish posture (task #658).** Nothing here is a soundness blocker. Exactly two things want a
decision before `cargo publish`: **R5** (`ReservationParts` derives `Clone`, so the single-use
release token can be duplicated — removing `Clone` after publish is breaking) and **R1** (not a
publish blocker per se, but publishing a crate whose Windows backend has never been exercised by
its own test suite in CI is a posture choice worth making deliberately). R4 (`is_huge()` has no
test at all) is cheap and worth doing first because it is the one public method whose contract has
been rewritten by three consecutive rounds.

---

## What was verified green (so the negatives below are read in context)

| command | result |
|---|---|
| `cargo test -p aligned-vmem --all-features` | **green** — lib 0, `fault_injection` **0**, `huge_pages` 1, `lazy_commit` 11, `min_page` 2, `mock` 9, `smoke` 18, `vmemerror_io_bridge` 3, doctests 0; 0 failed |
| `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` | **green**, exit 0 |
| `cargo clippy -p aligned-vmem --all-targets -- -D warnings` (default row) | **green**, exit 0 |
| `cargo doc -p aligned-vmem --all-features --no-deps` | **green**, zero warnings |
| `grep -rn "cfg(test)" crates/vmem/src` | no match — no inline `#[cfg(test)] mod tests`, CLAUDE.md-conformant |
| `git status` at session start | clean |

Note the `fault_injection` line in that table: **0 tests** under `--all-features`. That is by
design (`tests/fault_injection.rs:15-19` requires `mock` OFF) and is R2's starting point.

---

# Findings

## Category 1 — verification: what CI actually executes

### R1 — HIGH — `--all-features` enables `mock`, which replaces the backend; both platform CI rows added by rounds 2 and 3 to cover the Windows and Darwin backends therefore run the recording stub instead. Executed counterfactual: with #848's `commit_len == size` guard deleted, the regression test FAILS under `--features lazy-commit` and PASSES under `--all-features`

**Citations.** `crates/vmem/src/lib.rs:1241-1244` (the mock re-route), `:1453` (the guard),
`:977-982` and `:1017-1022` (`decommit`/`decommit_lazy`'s `#[cfg(not(feature = "mock"))]` impl
calls), `:1066-1074` (`try_recommit`'s mock branch), `:1155-1163` (`try_commit_range`'s mock
branch), `crates/vmem/tests/lazy_commit.rs:70-116` (the #848 regression test),
`crates/vmem/tests/smoke.rs:142-147` (the zero-fill assertion, `#[cfg(not(any(miri, feature =
"mock")))]`), `.github/workflows/ci.yml:160`, `:777`, `:802`, `:832`, `:874`, `:894` (all six
`cargo test -p aligned-vmem` rows).

**The mechanism.** `mock` is documented three separate times as non-additive and
backend-replacing — `crates/vmem/src/mock.rs:25-38`, `crates/vmem/Cargo.toml`'s `mock` feature
comment, and `docs/CORRECTNESS_OPEN_ITEMS.md` item 42. `--all-features` includes it. Under `mock`:

```rust
// lib.rs:1235-1244 — try_reserve_aligned_lazy
#[cfg(feature = "mock")]
let raw = reserve_aligned_raw(size, align);          // EAGER backend
#[cfg(not(feature = "mock"))]
let raw = reserve_aligned_lazy_raw(size, align, initial_commit);
```

`reserve_aligned_lazy_raw` is not merely bypassed — it is *dead*, which is why it carries
`#[cfg_attr(feature = "mock", allow(dead_code))]` (`lib.rs:1650`). And `reserve_aligned_raw` always
calls `win_reserve_commit(size, align, size, 0)` — i.e. `commit_len == size` — so the guard's
`false` branch is unreachable under `mock` by construction.

**Executed counterfactual (real Windows host, throwaway copy under `%TEMP%`).** Replaced
`if align <= WIN_ALLOCATION_GRANULARITY && commit_len == size {` with
`if align <= WIN_ALLOCATION_GRANULARITY {` — nothing else changed:

```
### A) --features lazy-commit (no mock)
test lazy_reserve_small_align_still_reserves_full_span ... FAILED
test result: FAILED. 10 passed; 1 failed; 0 ignored

### B) --all-features (mock ON)
test lazy_reserve_small_align_still_reserves_full_span ... ok
test result: ok. 11 passed; 0 failed; 0 ignored
```

**Failure scenario, concrete.** Someone simplifies `win_reserve_commit`'s dispatch (the guard's
condition reads like a redundant optimisation detail, which is exactly why its own 27-line comment
at `lib.rs:1437-1452` insists it is not). Every CI row stays green:

* `test-windows` (`ci.yml:777`) is `--all-features` → mock → guard unreachable. **Green.**
* `test-macos` (`ci.yml:802`) is `--all-features` → mock, and Unix has no such guard anyway.
  **Green.**
* `aligned-vmem-gates` (`ci.yml:160`) is `--all-features` on ubuntu. **Green.**
* `test-workspace`'s three rows: `:832` default features (`lazy_commit.rs` compiles to 0 tests),
  `:874` `--all-features` (mock), `:894` `--features "fault-injection lazy-commit"` — real backend,
  but **ubuntu**, where `reserve_aligned_lazy_raw` just forwards to `reserve_aligned_raw`
  (`lib.rs:2034-2040`) and the guard does not exist. **All green.**
* The root crate's Windows rows (`ci.yml:749-773`) do reach `reserve_aligned_lazy` for real (via
  `production` → `primordial-lazy-commit` → `aligned-vmem/lazy-commit`), but always with
  `align = SEGMENT = 4 MiB`, which fails the `align <= 64 KiB` half of the condition regardless.
  **Green, and structurally incapable of discriminating.**

A Windows consumer calling `reserve_aligned_lazy(size, align <= 64 KiB, initial_commit < size)`
then gets a reservation silently truncated to `initial_commit` bytes and a failing `commit_range`
past that point — the exact reproduction recorded in `lazy_commit.rs:78-82`.

**The same defect, second and third instances.** This is not only about one guard:

* **Windows `MEM_DECOMMIT` / `recommit`.** Round 3's F2 named `smoke.rs`'s
  "`decommit`/`recommit` round-trips against real `MEM_DECOMMIT`" as running only on non-Windows
  CI, and closed it by adding an `--all-features` Windows row. Under `mock`, `decommit`
  (`lib.rs:971-982`) records and returns without calling `decommit_pages_impl` at all, and
  `smoke.rs:142` compiles the zero-fill assertion out. So the Windows row added to cover
  `MEM_DECOMMIT` does not issue `MEM_DECOMMIT`.
* **Darwin `MADV_FREE_REUSABLE`.** `ci.yml:799-802`'s own comment says the row "Exercises the
  Darwin vmem backend (mmap/madvise/MADV_DONTNEED) on real macOS hardware". `mmap` yes (reservation
  is real under `mock`). `madvise` no: `madv_free_advice()` (`lib.rs:2057-2070`), the one function
  that selects `MADV_FREE_REUSABLE` on Darwin, is reachable only from `decommit_pages_impl`, which
  `mock` bypasses — which is precisely why it carries `#[cfg_attr(feature = "mock",
  allow(dead_code))]` (`lib.rs:2055`). The `#[cfg_attr]` list in `lib.rs:84-97` is, read the right
  way, an exact inventory of everything `--all-features` cannot execute.

**Fix.** Replace, or supplement, the two platform rows with the everything-except-`mock` feature
set, e.g. `cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection
bench-internals" --no-fail-fast`. That single row on `test-windows` is what would have caught the
counterfactual above; the same row on `test-macos` is what makes the Darwin `madvise` claim true.
Keeping the existing `--all-features` row alongside it is still worthwhile — it is the only row
that runs `tests/mock.rs` — but it should not be described anywhere as platform-backend coverage.
Worth considering as a durable guard: a `#[cfg(all(windows, not(feature = "mock")))]` compile-time
assertion, or simply a note at `lib.rs:1241` pointing at this hazard, so the next person who reads
the mock re-route knows what it costs in coverage.

### R2 — MEDIUM — the `fault-injection` suite's only CI row is Linux, where the "real backend" it claims to prove coexistence with is a compile-time `Ok(())`; the one platform where `commit_range` is a real syscall never runs it

`crates/vmem/tests/fault_injection.rs:1-19` (the module doc's claim and the `not(feature = "mock")`
gate), against `crates/vmem/src/lib.rs:2025-2028` (Unix `commit_range_impl`),
`:2356-2358` (miri `commit_range_impl`), `:1641-1645` (Windows `commit_range_impl`), and
`.github/workflows/ci.yml:894` (the only row that compiles the file to non-zero tests).

The file's own module doc states its purpose:

> These tests run against the REAL OS backend (no `mock` feature): … `commit_range` issues genuine
> `VirtualAlloc`/no-op-Unix commit syscalls — the armed hooks only intercept the specific call(s)
> under test, **proving the fault-injection hook coexists with (and does not replace) the real
> backend.**

On the only platform CI runs it, `commit_range_impl` is:

```rust
// lib.rs:2025-2028
unsafe fn commit_range_impl(_base: *mut u8, _start: usize, _end: usize) -> Result<(), VmemError> {
    // Unix: pages are already accessible (eager mmap). Always succeeds.
    Ok(())
}
```

**Failure scenario, concrete.** Change `try_commit_range`'s real branch (`lib.rs:1164-1184`) so
that it returns `Ok(())` directly after the fault-injection check instead of calling
`commit_range_impl` — i.e. delete the backend entirely from the fault-injection path. Every
assertion in `tests/fault_injection.rs` still passes on Linux: `zero_arming_is_a_pure_disarm`
(`:276-288`) asserts `ok == true`, which `Ok(())` satisfies; the "must hit the real backend and
succeed" assertions at `:56-60`, `:96-100` and `:141-142` likewise. The three real writes
(`:64-67`, `:104-107`) succeed because the Unix backend committed everything eagerly at reservation
time, not because `commit_range` did anything. The coexistence claim is only falsifiable on
Windows.

R1's fix does not close this one: the file requires `mock` OFF, so any `--all-features` row —
including the one R1 discusses — excludes it by cfg. It needs its own Windows row, or the
everything-except-`mock` row R1 proposes (which includes `fault-injection`) added to
`test-windows`. One row closes both.

`fail_next_is_atomic_under_concurrent_callers` (`:197-271`) is genuinely platform-independent and
is not affected by this — it tests `fetch_update`, not the backend. Its own oracle was already
repaired by task #775 and I re-read it; it is correct.

## Category 2 — round-3 fix residue

### R3 — MEDIUM — task #859's fix for round-3 F3 removed one stale claim from the root crate's forwarding accessors and introduced two new factually-wrong ones, plus recreated the exact "says N, lists N+1" defect F3 itself named

`src/alloc_core/alloc_core_core_diag.rs:145-155`, `:157-166`, `:169-183`, against
`crates/vmem/src/lib.rs:1453` and `:1728-1743`.

Three separate problems, all introduced by the fix rather than surviving it:

1. **`:147` — "the OS page size" is wrong by a factor of 16.**
   > `/// Unix/miri). The fast path applies when `align <= 64 KiB` (the OS page size),`

   64 KiB is `WIN_ALLOCATION_GRANULARITY` (`lib.rs:1732`, commented "64 KiB - VirtualAlloc alignment
   guarantee"), the *allocation granularity*. The Windows page size is 4 KiB —
   `dw_page_size`, the field `query_os_page_size` actually reads (`lib.rs:364`). The vmem-side
   docs get this right (`lib.rs:207-215` says nothing about page size); the root-crate forwarder,
   which is the surface a measurement round reads, states the wrong quantity as a parenthetical
   fact.

2. **`:147-148` and `:158-160` state only half the dispatch condition.** Both accessors describe
   the split as `align <= 64 KiB` vs `align > 64 KiB`. The real condition is
   `align <= WIN_ALLOCATION_GRANULARITY && commit_len == size` (`lib.rs:1453`) — and the omitted
   half is precisely the one that caused the #848 bug and that R1 shows is uncovered. A reader of
   these docs measuring on a `lazy-commit` build with `align <= 64 KiB` would attribute every
   two-call-path increment to "align > 64 KiB" and conclude their build was doing something it was
   not. Note that the root crate's `bench-internals` feature *does* pull in
   `aligned-vmem/lazy-commit` (root `Cargo.toml:579`), so this is the configuration a measurement
   round is actually in.

3. **`:169-176` — "reset all four" followed by five links.** Round-3 F3 item 2 was, verbatim,
   "says … 'reset all **three**' … and lists three; there are now four". The fix updated the word
   to "four" and expanded the list to five entries, one of which
   (`dbg_windows_reserve_commit_calls`) is the derived *sum*, not a counter. There are four
   statics; the sentence and its own list disagree again, one round later.

**Fix.** `:147` → "(the Windows allocation granularity — the Windows page size is 4 KiB)"; add the
`commit_len == size` half to both accessors' descriptions; drop `dbg_windows_reserve_commit_calls`
from the reset list (it is not reset — it is computed from two entries that are).

### R4 — LOW-MEDIUM — `Reservation::is_huge()` has zero test assertions anywhere in the crate, after three consecutive rounds rewrote its contract

`crates/vmem/src/lib.rs:508-512` (the method), `:482-507` (its 26-line contract), `:416` (the
field), against `grep -rn "is_huge" crates/vmem/tests crates/vmem/benches crates/vmem/examples` →
**no match**.

`is_huge()` is the observable V3 introduced, W2 corrected on Unix (`HUGE_SUPPORTED`, `lib.rs:2099-2106`),
W3 corrected in the Windows docs, and round-3 F8/F9 corrected again. Its documented contract now
has a Unix clause (`HUGE_SUPPORTED && huge`, `lib.rs:1871`/`:1983`), a Windows single-call clause
(`extra_commit_flags != 0`, `:1496`), a Windows two-call clause (`:1587`, flagged INFO by F9 as
deriving from the request rather than the grant), and a `from_raw_parts` clause (always `false`,
`:668`). None of the four is asserted anywhere. The only thing any test does with `granted_huge` is
`smoke.rs:48-51`, which checks that the string `"granted_huge"` appears in the `Debug` output.

**Failure scenario, concrete.** Revert W2's fix — change `lib.rs:1871` from
`granted_huge = HUGE_SUPPORTED && huge;` back to `granted_huge = huge;` and `lib.rs:1983` from
`HUGE_SUPPORTED && huge` to `huge`. `cargo test -p aligned-vmem --all-features` stays green on
every platform, including the macOS row added expressly to cover non-Linux Unix huge-page
behaviour, and `is_huge()` silently starts returning `true` for ordinary-page reservations on
macOS again — the precise bug W2 was written to close.

**Fix.** Two assertions, both cheap and both true on every platform:
`assert!(!reserve_aligned(2*MIB, 2*MIB).unwrap().is_huge())` (an ordinary reservation is never
huge) in `tests/smoke.rs`, and — in `tests/huge_pages.rs`, next to the existing
`reserve_aligned_huge_ordinary_page_sized_request_succeeds` — a
`#[cfg(not(target_os = "linux"))] assert!(!r.is_huge())` pinning the "non-Linux Unix and Windows
`align > 64 KiB` never report huge" half of the contract. Both would fail against the reverted code
above; the second is exactly the macOS-row-shaped assertion W2's fix has never had.

## Category 3 — API / semver hygiene, to settle before the next publish

### R5 — LOW-MEDIUM — `ReservationParts` derives `Clone`, so the single-use release token introduced to make manual release *safer* can be duplicated and released twice; `Reservation` itself is deliberately not `Clone`

`crates/vmem/src/lib.rs:709` (`#[derive(Debug, Clone, PartialEq, Eq)]`) against `:695-707` (the
type's own rationale), `:912-920` (`release_parts`), and `:388-417` (`Reservation`, which has a
hand-written `Debug` at `:419-430` and no `Clone`).

`ReservationParts` exists because V8 found that `into_parts()` → `release()` is "a 3-tuple of two
indistinguishable `usize`s, and swapping them compiles". Its doc says so:

> A named structure (instead of a raw tuple) prevents the footgun of accidentally swapping the
> `len` and `align` fields, which would be undefined behavior on the native backend…

It closes the transposition footgun and opens a strictly worse one. `parts.clone()` compiles, is
safe code, and yields two values each of which `release_parts` will accept:

```rust
let parts = r.into_reservation_parts();
let dup = parts.clone();                    // safe code, no warning
unsafe { release_parts(parts) };
unsafe { release_parts(dup) };              // double VirtualFree(MEM_RELEASE) / munmap
```

`release_parts` is `unsafe` and its contract says "must be released exactly once", so this is not
unsound *by contract* — but neither was the tuple swap V8 removed, and the whole argument for this
type is that a linear resource token should be shaped so the wrong use does not compile. A `Clone`
on a release token is the opposite of that. `Reservation`, which is the same resource one step
earlier, is correctly not `Clone`.

**Why it is a publish-gate item and not a nit.** Removing a derived `Clone` from a published type
is a breaking change. Round 3 correctly recorded `ReservationParts`'s missing constructor (W12) as
an *additive* decision safe to defer past publish; this one is the opposite polarity and cannot be
deferred the same way.

**Fix (three options, in my order of preference).** (a) Drop `Clone`, keeping `Copy` off too —
`as_tuple(self)` (`:726-728`) already gives a caller the escape hatch if they need the raw values
more than once. (b) Keep `Clone` and document the hazard explicitly in `release_parts`'s `# Safety`
section, naming `clone()` as the concrete way to violate "exactly once". (c) Leave as is, recorded
as a deliberate decision in `docs/CORRECTNESS_OPEN_ITEMS.md` alongside W12's entry — worse than (a)
or (b), but better than the current state, where nothing anywhere records that this was considered.

## Category 4 — documentation drift

### R6 — LOW — the crate's front-page rustdoc still states the over-reserve mechanism unconditionally; this is the fifth drift of the sentence family W5 fixed three times and round-3 F4 fixed a fourth

`crates/vmem/src/lib.rs:19-28` (specifically `:24`), against `:741-749`
(`reserve_aligned`'s now-correct rustdoc), `:160-180` (the module design comment, correct),
`crates/vmem/Cargo.toml:7` (correct since task #860) and `crates/vmem/README.md:40` (correct since
task #860).

```rust
//! # Why not `region` / `memmap2` / `mmap-rs`?
//! …
//! `aligned-vmem` does one different thing: hand you an *anonymous* span whose
//! **base is aligned to a power of two you choose** (e.g. 2 MiB / 4 MiB for an
//! allocator's segments) by over-reserving `size + align` bytes and keeping the
//! full mapping, plus page-granularity decommit/recommit …
```

That is the docs.rs landing page's first substantive paragraph, and it is the one place the
sentence was not corrected. It is wrong for the Unix exact-size fast-path hit (34.4–56.7% of
reservations by the crate's own measured numbers — `README.md:40`, task #849) and wrong for the
whole Windows `align <= 64 KiB` path (`lib.rs:1494-1496`, "base == region", no over-reserve at
all).

W5 counted three drifts of this sentence (`#640`, `#650`, `#842`) and recommended a
`grep -n 'trim' crates/vmem/` guard. F4 was the fourth and recommended widening the guard to
"over-reserve". Neither guard exists in `ci.yml` or `scripts/`, and this is the fifth.

**Adjacent, same root cause and worth fixing in the same pass:** `Reservation::reservation_len()`'s
doc (`lib.rs:475-480`) says "The full size of the underlying OS reservation." On the Windows
single-call path it returns `commit_len` (`:1496`), which equals `size` — but Windows rounds every
VA reservation up to the 64 KiB allocation granularity, so a `reserve_aligned(4096, 4096)` reports
`reservation_len() == 4096` while consuming 64 KiB of address space. Harmless for correctness
(`VirtualFree(base, 0, MEM_RELEASE)` ignores the length) and harmless for the in-repo consumers
(`src/alloc_core/os.rs` always uses `align = SEGMENT = 4 MiB`), but it is a resource-model claim
that is now false on one of the two Windows paths — and the *previous* version of exactly this
assumption is what broke a `crates/numa/` test (task #864) one round ago.

**Fix.** One sentence at `:24`, matching the wording already used at `:741-749`. Then add the
`over-reserve`/`trim` grep to `scripts/check-all.mjs` so there is not a sixth.

### R7 — LOW — `unix_reserve`'s huge-page doc justifies its size/align restriction with a "head is provably 0, tail_len is provably huge-page-aligned" argument about `munmap` calls that task #842 deleted — and the "head provably 0" half is false on its own terms for any `align > 2 MiB`

`crates/vmem/src/lib.rs:1812-1824` (the justification), against `:1899-1904` (the no-trim comment
task #842 added, in the same function) and `:1879-1898` (the code, which has no head/tail
`munmap`).

```
/// … with both huge-page-aligned, `over = size + align` is also
/// huge-page-aligned, the kernel-guaranteed huge-page-aligned `region_addr`
/// makes `head` provably `0` (`align_up_addr` of an already-aligned address
/// to an aligned `align` is a no-op), and `tail_len` (the difference of two
/// huge-page-aligned addresses) is provably huge-page-aligned too — so every
/// `munmap` this function can still reach is provably conformant …
```

Two problems:

1. **`head` and `tail_len` no longer exist.** V1/task #842 replaced the head/tail trim with "keep
   the entire over-reserve mapping" (`lib.rs:1899-1904`). Every `munmap` reachable from this
   function today is a whole-mapping unmap: `libc_munmap(region_ptr, over)` on the fit-failure
   path (`:1891`), `libc_munmap(region_ptr, size)` on the fast-path alignment miss (`:1969`), and
   `release_reservation`'s `munmap(reservation, reservation_len)` (`:1987-1991`). Only the *first*
   clause of the justification ("`over = size + align` is also huge-page-aligned") still does any
   work; the other two describe deleted code.
2. **"`head` provably `0`" is false for `align > LINUX_HUGE_PAGE_SIZE`.** The kernel guarantees an
   anonymous `MAP_HUGETLB` mapping starts at a *huge-page*-aligned (2 MiB) address. It guarantees
   nothing about 4 MiB. For `align = 4 MiB` — the value `sefer-alloc`'s own `SEGMENT` uses — a
   2 MiB-aligned `region_addr` that is not 4 MiB-aligned gives
   `align_up_addr(region_addr, 4 MiB) - region_addr == 2 MiB ≠ 0`. The parenthetical
   ("`align_up_addr` of an already-aligned address to an aligned `align` is a no-op") is only valid
   when the address is aligned to `align` itself, which is the thing not guaranteed.

**Why it matters despite being doc-only.** The restriction it justifies is a real narrowing of the
public contract (`reserve_aligned_huge` rejects `size`/`align` that are not 2 MiB multiples,
`lib.rs:1831-1837`) with three regression tests behind it (`tests/huge_pages.rs:64-104`). A future
round reading this paragraph either believes a false invariant about `align_up_addr`, or — more
likely — concludes the restriction was tied to the trim, sees the trim is gone, and removes the
restriction. The restriction is still needed, for the surviving first clause alone: `munmap` on a
`MAP_HUGETLB` mapping requires a huge-page-aligned length, and `over = size + align` is
huge-page-aligned only because both operands are.

**Fix.** Delete the two clauses about `head`/`tail_len`; keep and foreground the `over`-alignment
clause; add one sentence noting that the head offset is *not* zero in general (only the whole
mapping is ever unmapped, which is why that no longer matters).

### R8 — LOW (perf opportunity, REASONED-FROM-SPEC, unmeasured) — the Windows `align > 64 KiB` path — the crate's own flagship allocator-segment case — still costs 2 syscalls and `size + align` of VA; `VirtualAlloc2` + `MEM_ADDRESS_REQUIREMENTS.Alignment` does it in one call with no over-reserve

`crates/vmem/src/lib.rs:1497-1589` (the two-call path), `:1732` (`WIN_ALLOCATION_GRANULARITY`),
`:1679-1688` (the crate's whole Win32 declaration surface — three functions).

For `align = 4 MiB, size = 4 MiB` (`src/alloc_core/os.rs`'s only reservation shape) the current
path is: `VirtualAlloc(NULL, 8 MiB, MEM_RESERVE)` → compute aligned base →
`VirtualAlloc(base, 4 MiB, MEM_COMMIT)`, holding 8 MiB of address space for the reservation's
lifetime to deliver 4 MiB of usable span. This is not a defect — it is the only thing plain
`VirtualAlloc` can do — and #848 already took the one win available without new API surface.

`VirtualAlloc2` (Windows 10 1803+ / Server 2016+, exported from `kernelbase.dll`) accepts a
`MEM_EXTENDED_PARAMETER` of type `MemExtendedParameterAddressRequirements` pointing at a
`MEM_ADDRESS_REQUIREMENTS { LowestStartingAddress, HighestEndingAddress, Alignment }`, where
`Alignment` is an arbitrary power of two ≥ 64 KiB. One call returns a base aligned to `Alignment`
with **no over-reserve**: 2 syscalls → 1, and `size + align` VA → `size` VA. mimalloc resolves and
uses exactly this API for the same reason, which is a useful precedent for both the shape and the
resolution strategy.

**The honest cost.** `VirtualAlloc2` is not in `kernel32.dll`, so an `extern "system"` declaration
alongside the existing three (`lib.rs:1679-1688`) would create a hard link-time dependency this
crate does not currently have, and would silently drop pre-1803 Windows support. Doing it properly
means `GetProcAddress` resolution behind a `OnceLock<Option<fn…>>` with fallback to the existing
two-call path — which is real new `unsafe` surface in a crate whose entire premise is minimal,
auditable `unsafe`. That trade-off is a decision, not a defect, and it is the reason I am filing
this as an opportunity rather than a finding. The crate also does not state a minimum Windows
version anywhere (`Cargo.toml` has `rust-version` but no platform floor), which would have to be
settled first.

**Same shape, other platforms, for completeness.** FreeBSD / DragonFly / NetBSD provide
`MAP_ALIGNED(n)` (`n` = log2 alignment, shifted into the `flags` word), which would turn
`try_reserve_aligned_exact` into a guaranteed hit on those targets and eliminate the over-reserve
fallback entirely — 3 syscalls on a miss → 1 always. All four BSDs are in this crate's `MAP_ANON`
cfg list (`lib.rs:2080-2094`) and none is in CI, so this is spec-reading only. Linux has no
equivalent; macOS has none. **I am not claiming a measured number for any of this**, and it does
not reopen V20/P17 (which was about *dispatch order* on Unix and correctly closed NO-GO in P-A) —
different mechanism, different platform.

### R9 — INFO — the `bench-internals` design comment says the counter storage is "always compiled"; in this crate both the storage AND the increments are `#[cfg(feature = "bench-internals")]`

`crates/vmem/src/lib.rs:182-184` and `crates/vmem/Cargo.toml:106-108`, against `lib.rs:186-187`,
`:194-196`, `:203-205`, `:216-218`, `:229-231`.

> `// AtomicU64` storage, always compiled (like sefer-alloc's own `dbg_*`
> `// counters); increments gated on bench-internals so a plain build carries`
> `// zero extra instructions.`

Every one of the four statics carries `#[cfg(feature = "bench-internals")]`, as does the
`use core::sync::atomic::AtomicU64;` that makes them nameable (`:186-187`). The design the comment
describes — storage always present, increments gated, so a non-bench build can still *read* a zero
— is not what shipped: without the feature the statics do not exist, the accessors do not exist,
and a consumer cannot read anything. The `Cargo.toml` feature doc repeats the same claim
("`AtomicU64` storage, always compiled; `#[doc(hidden)]` statics").

Nothing breaks; the shipped design is arguably the better one (a plain build carries no static
storage either). But the comment is the only description of the intended design, and it describes a
different design from the code directly beneath it. Round-3 F3 already corrected the
"accessors"/"statics" clause of this same comment; this clause was not looked at in the same pass.

### R10 — INFO — round-3 F7's `debug_assert!` landed exactly where F7 asked for it, but that location is never reached on the Windows reservation path it guards

`crates/vmem/src/lib.rs:352-365` (`query_os_page_size` with the new assertion at `:357-363`),
against `:1453` (the guarded fast path) and the call graph
`reserve_aligned → try_reserve_aligned → validate_size_align → reserve_aligned_raw →
win_reserve_commit`.

`query_os_page_size` is called from exactly one place: `page_size()`'s cold path (`:320`), which is
itself called from `decommit` (`:967`), `decommit_lazy` (`:1007`), and `try_reserve_aligned_exact`
(`:1967`, Unix only). **No Windows reservation path calls it.** `validate_size_align` (`:765-770`)
uses the `PAGE` constant; `win_reserve_commit` never asks the OS anything. A Windows program that
reserves and releases without ever decommitting evaluates the assertion zero times. In `--release`
it is compiled out regardless.

This is not a regression and not a bad fix — F7 explicitly proposed "a `debug_assert!(…)` alongside
the existing `GetSystemInfo` call", and that is what shipped. It is recorded here only so nobody
reads the assertion as covering the fast path. The stronger form F7 offered as an alternative —
"or deriving the constant from it" — would have no such gap, at the cost of turning a `const` into
a runtime query on the reserve hot path. Given that every Windows target this crate supports
reports 64 KiB, the current state is defensible; it just should not be mistaken for coverage.

### R11 — INFO — two counter-fidelity nits in the `bench-internals` instruments

`crates/vmem/src/lib.rs:1481-1493` and `:1563-1581` (Windows), `:1945-1946` and `:1972-1973`
(Unix).

1. **The Windows counters count only *successful* calls.** Both `WINDOWS_RESERVE_COMMIT_*`
   increments sit after the success check; every early `return Err(...)` (`:1485`, `:1488`,
   `:1511`, `:1572`, `:1578`) increments nothing. Their rustdoc says "total number of
   `win_reserve_commit` calls that took the … path" — a failed call takes a path too. Under OOM
   pressure a syscall-count derivation from these counters undercounts.
2. **`UNIX_EXACT_RESERVE_ATTEMPTS` counts `mmap` failures in the hit-rate denominator.** The
   increment is at function entry (`:1946`); an `mmap` that returns `MAP_FAILED` (`:1950-1954`)
   is counted as an attempt but is not an alignment miss. The counter's own doc frames the ratio
   as "attempts that succeeded (the `mmap` landed already `align`-aligned)", i.e. an
   *alignment* hit rate, which a genuine OOM silently deflates. This matters most in exactly the
   regime the counters exist to measure — `huge = true`, where the `MAP_HUGETLB` mmap failing is
   the *common* case on an unconfigured host.

Both are diagnostic-only and neither affects behaviour. Filed because they are the same class of
defect as W4 and round-3 F6 — a counter whose documented unit does not match what it counts — and
because `docs/perf/OPEN_ITEMS.md` item 46 schedules a bare-metal remeasure that would read them.

## Category 5 — process / conventions

### R12 — INFO — round 3's own tasks (#858–#864) have no CHANGELOG entry; this is the third consecutive round with this gap, and neither open-items index tracks it

`CHANGELOG.md:304-312` covers tasks #851–#857 (written by task #863, closing round-3 F11 item 2).
`grep -nE "Task #85[89]|Task #86[0-4]" CHANGELOG.md` returns nothing — the seven commits that
*fixed* round 3 (`75bba05`, `fe19572`, `fd032af`, `22f1e55`, `91d5555`, `c14bd3a`, `d66c3c7`) are
unrecorded. Neither `docs/perf/OPEN_ITEMS.md` nor `docs/CORRECTNESS_OPEN_ITEMS.md` carries an item
for it, so — per CLAUDE.md's own "the in-session TaskList does not survive a session boundary"
argument — a fresh session inherits no memory of the debt.

The recurrence is the point: W16 flagged it for the #842–#850 campaign, F11 flagged it again for
#851–#857 and closed that one, and #858–#864 reproduced it immediately. Two rounds in a row closing
the *instance* without closing the *pattern* suggests the fix belongs in the round-close checklist
(or an open-items card), not in another one-off task.

Two things round 3 *did* close and that I re-verified: all seven `aligned-vmem` review documents
are now tracked (`git ls-files docs/reviews | grep aligned-vmem` → 7 files), and
`docs/CORRECTNESS_OPEN_ITEMS.md` item 41's Status card is current and honest (sub-item 1, the
intentional `leak_zeroed_pages` leak, correctly remains the sole runtime miri blocker). Item 41's
inline citations `crates/vmem/src/lib.rs:2239` and `:2250` are now stale by ~130 lines — cosmetic,
noted, not worth a task on its own.

### R13 — INFO — CLAUDE.md's "single-file seam crates" exception names `crates/vmem/src/lib.rs` as its example; the crate has been four files for some time

CLAUDE.md, "File and module structure", sanctioned exception 3:

> **single-file seam crates in `crates/`** — for a crate that is one file (e.g.
> `crates/vmem/src/lib.rs`, `crates/numa/src/lib.rs`, `crates/malloc-bench/src/lib.rs`) …

`crates/vmem/src/` holds `lib.rs`, `error.rs`, `mock.rs`, `fault_injection.rs`. The crate is still
plainly within the *spirit* of the exception — it is one focused seam library, there is no `mod.rs`
anywhere, tests all live in `tests/` (verified: `grep -rn "cfg(test)" crates/vmem/src` → no match),
and each of the three non-`lib` files has exactly one responsibility. So this is not a violation to
fix in the crate; it is the convention's *example text* that has gone stale, and it is worth a
one-line correction the next time CLAUDE.md is touched so a future reader does not conclude the
crate drifted.

For the record, the two structural items round 2 raised in this area remain correctly recorded as
deliberate: the "reorganize the three backends as `#[cfg]`-selected private modules" refactor
(`lib.rs:104-111`, explicitly deferred past 0.2.0), and the twelve
`#[cfg_attr(feature = "mock", allow(dead_code))]` attributes V16 named as the cost of `mock` being
a partial backend replacement. R1 gives that second one a new significance — the attribute list is
a precise inventory of what `--all-features` cannot execute — but the design decision itself is
unchanged and I am not reopening it.

---

## Checked and explicitly NOT findings

All eleven round-3 findings were re-checked in the current tree. All eleven landed:

* **F1 (`tests/mock.rs`'s page-size assertion)** — fixed. `tests/mock.rs:35` now reads
  `if start == page_size()`, matching the call at `:22`. `PAGE` is still imported and still used
  (`:88`), so the import did not go dead.
* **F2 (Windows CI row)** — the row exists (`ci.yml:777`). It does **not** achieve what F2 claimed;
  see R1. The row itself is a genuine addition and should stay.
* **F3 (root-crate counter docs + missing forwarders)** — the two forwarders exist
  (`alloc_core_core_diag.rs:153`, `:165`), the "Each call issues exactly 2 syscalls" claim is gone,
  root `Cargo.toml:564` names the split statics, and `crates/vmem/Cargo.toml:104-108` no longer
  says "accessors". Three new problems were introduced in the same edit; see R3.
* **F4 (`reserve_aligned`'s "unconditionally over-reserves")** — fixed at `lib.rs:741-749` and in
  `Cargo.toml:7`. The module-header instance was missed; see R6.
* **F5 (README API table)** — fixed. `README.md:42-45` now has both
  `into_parts() -> (*mut u8, usize, usize)` and `into_reservation_parts() -> ReservationParts` as
  separate rows with correct types and labels.
* **F6 (single-call counter's retry disclosure)** — fixed. `lib.rs:212-215` now discloses that the
  large-page retry issues a second syscall and is still counted as 1, symmetrically with the
  two-call counter's own note at `:223-226`.
* **F7 (allocation-granularity guard)** — landed at `lib.rs:357-363`, exactly as F7 proposed. Its
  reach is narrower than it looks; see R10.
* **F8 (`is_huge`'s lost paragraph, list continuation, stray escapes)** — fixed. `lib.rs:499-507`
  now has the "If any of these conditions fail…" paragraph as its own paragraph, the
  `from_raw_parts`-always-false note is present, and `grep -n '\\"' crates/vmem/src/lib.rs` returns
  no match.
* **F9 (Windows two-call `granted_huge`)** — the explanatory NOTE landed at `lib.rs:1582-1586`,
  stating plainly that the flag is derived from the request and that the branch is documented-but-
  not-enforced-unreachable. Correct resolution for an INFO.
* **F10 (`mock::Call::Release`'s variant doc)** — fixed. `mock.rs:85-87` now names both producers
  (`crate::release` and RAII `Drop`).
* **F11 (paper trail)** — items 1 and 3 closed (CI has run; all seven review docs are tracked);
  item 2 closed for #851–#857 and immediately reopened for #858–#864, see R12.

Also re-verified from rounds 1 and 2, weighted toward things a later commit could have undone:

* **W1's miri compile fix holds.** `lib.rs:2364-2371` and `:2373-2382` both destructure 3-tuples;
  the CI compile gate is at `ci.yml:164`.
* **W2's `HUGE_SUPPORTED`** is `true` only under `all(target_os = "linux", feature = "huge-pages")`
  (`lib.rs:2099-2106`) and both Unix return sites use `HUGE_SUPPORTED && huge` (`:1871`, `:1983`).
  Untested, though — see R4.
* **V1's no-trim fix is intact.** `lib.rs:1899-1921` returns the whole mapping; `release_reservation`
  (`:1987-1991`) `munmap`s exactly `(reservation, reservation_len)`. No `munmap` on a computed
  sub-offset survives anywhere in `unix_reserve`.
* **P-A's free alignment-check skip is still sound.** `lib.rs:1967` is
  `if align > page_size() && !region_addr.is_multiple_of(align)`. I re-derived the boundary case
  (`align == page_size()` on a 16 KiB host: the guard is skipped, and an `mmap` result is
  16 KiB-aligned by the kernel's own contract, so the skipped check would have been true anyway).
  Correct.
* **P-B holds.** `decommit` (`:967`) and `decommit_lazy` (`:1007`) each hoist one
  `let ps = page_size();`.
* **`fault_injection`'s atomics are unchanged and still correct** — `Release`/`Acquire` pair at
  `:108`/`:139`, `fetch_update` with the lazy-`then` underflow note at `:125-133`, and the third
  disarm-vs-rearm race still declared out of scope in the module doc (`:47-57`). The `SERIAL`
  mutex in `tests/fault_injection.rs:34` still serialises the process-global hooks against
  libtest's thread pool. `fail_next_is_atomic_under_concurrent_callers`'s post-#775 oracle
  (`armed == calls / 2`) is genuinely two-sided; I re-derived the argument in its doc comment and
  it is correct.
* **`error.rs` is unchanged and fully covered.** The three-way classification, the
  `From<VmemError> for std::io::Error` bridge (`:138-148`), and the single de-duplicated
  `last_os_error_code` (`:150-160`) all hold, and all three `io::Error` arms have real assertions
  in `tests/vmemerror_io_bridge.rs`.
* **`mock::Call`'s constructors are complete** — all 8 variants (`mock.rs:138-193`), exercised
  from the integration-test crate at `tests/mock.rs:130-155`.
* **V5's two `from_raw_parts` test leaks stay fixed and non-vacuous** (`smoke.rs:309-333`,
  `:341-361`): both `catch_unwind` → `release` → `resume_unwind` the *original* payload, so the two
  distinct `#[should_panic(expected = …)]` strings still discriminate.
* **Not re-raised, still open, deliberately:** `benches/vmem_bench.rs`'s asymmetric `black_box`
  usage (V18 sub-item; round 3 recorded it explicitly so round 4 would not rediscover it as new).
  Confirmed still present at `:54-57` vs `:69`; still stylistic; still affects nothing published.

---

## Categories with nothing to report

Stated explicitly, per the review mandate, rather than left silent:

* **Soundness / UB.** Nothing new. Every `unsafe` block carries a `// SAFETY:` comment; I re-read
  all of them. The strict-provenance discipline (`.addr()` / `.with_addr()`) is complete at all
  three native address-computation sites (`lib.rs:1522`/`:1543`, `:1878`/`:1898`, `:1961`).
  `from_raw_parts`'s construction-time `Layout` validation (`:654-661`) still covers both halves of
  the Drop-reachable-panic hazard. No aliasing, no double-free, no use-after-free path found.
* **Concurrency.** One shared-mutable-state module (`fault_injection`), unchanged since #718/#775,
  re-audited above. `PAGE_SIZE_CACHE` (`lib.rs:158`, `:316-336`) is a benign racy-init cache: two
  threads may both query the OS and both store, but they store the same value, and the `0`
  sentinel is unambiguous. `Reservation`'s `Send`-not-`Sync` posture is correct and the `Send`
  claim is pinned by a `const _: () = assert_send::<Reservation>();` at `smoke.rs:20-21`.
* **Panic safety.** `#![deny(missing_docs)]` is on; the only reachable panics are
  `from_raw_parts`'s two documented `expect`s and its `assert!`, all at construction time, none
  reachable from `Drop`. `release_reservation`'s miri-only `.expect("release: invalid layout")`
  (`:2333`) *is* Drop-reachable, but is unreachable given `from_raw_parts`'s validation — which is
  exactly why task #776 extended that validation.
* **Performance on the paths that ship.** Re-confirmed null, for the third round running, and I am
  not manufacturing an item to avoid saying so. Every public entry point is one syscall deep;
  `page_size()` is a single relaxed load after the first call; `align_up_addr` is two arithmetic
  ops; the `bench-internals` counters are compiled out by default. The only remaining levers are
  syscall count and address space, and both are settled for Linux (P-A: leave the dispatch alone)
  and for Windows `align <= 64 KiB` (#848). R8 is the one genuinely-unexplored lever, and I have
  labelled it spec-read and unmeasured rather than dressing it up as a finding.
* **Dead code / duplication.** `cargo clippy --all-targets -D warnings` is green on both the
  default and `--all-features` rows; the twelve `#[cfg_attr(feature = "mock", allow(dead_code))]`
  attributes are individually justified in `lib.rs:84-97` and I spot-checked four of them against
  the actual call graph. No orphaned helper, no TODO, no placeholder, no commented-out code.
* **New safe `pub fn` accepting a raw pointer and touching allocator metadata** (CLAUDE.md's
  benchmark-hook rule). None. `decommit`/`decommit_lazy`/`recommit`/`commit_range` all take raw
  pointers and are all correctly `unsafe fn`; nothing in this crate matches the R25-1 shape.

---

## Recommended order

1. **R1** — one CI row per platform with everything-except-`mock`
   (`--features "lazy-commit huge-pages fault-injection bench-internals"`) on `test-windows`, and
   the same on `test-macos`. This is the only finding this round that closes a real,
   demonstrated-by-execution hole, and the Windows half also closes **R2** for free.
2. **R4** — two `is_huge()` assertions. Cheapest non-vacuous coverage available for the one public
   method three rounds have rewritten and none has tested.
3. **R5** — decide `ReservationParts`' `Clone` before publish. Drop it, or document the hazard in
   `release_parts`' `# Safety`, or record the decision — but do not let it ship undecided, because
   only one of those three stays available after 0.2.0 is on crates.io.
4. **R3** — three corrections in `src/alloc_core/alloc_core_core_diag.rs` (`:147`'s "OS page size",
   the missing `commit_len == size` half in both accessors, the four-vs-five reset list). Minutes,
   and it is the surface a measurement round reads.
5. **R6, R7** — the two doc-drift fixes, batchable in one pass, plus the `over-reserve`/`trim` grep
   guard in `scripts/check-all.mjs` that W5 asked for two rounds ago and would have caught R6.
6. **R12** — write the #858–#864 CHANGELOG entry, and file the *pattern* (not just this instance)
   so a fourth recurrence is not the way it gets noticed again.
7. **R9, R10, R11, R13** — four small notes; batchable, none blocking anything.
8. **R8** — a decision to schedule, not a fix to apply. It needs a Windows-version floor policy and
   a `GetProcAddress`-vs-link-time call before any code is written, and it should be measured
   before it is believed.

Nothing in this list is a breaking change except R5 option (a), which is breaking only if deferred
past publish — which is the argument for doing it now. Nothing here reopens a round-1, round-2 or
round-3 finding.

**On the "diminishing returns" question round 3 asked.** For the crate's source: yes, genuinely.
Three rounds have converged and this pass found no new bug in it. For the crate's *verification*:
no, not yet — R1 is a first-order hole that three rounds of source review could not have found,
because it lives in the interaction between a Cargo feature's semantics and a CI invocation's
flags, and the only way to see it was to break the code and watch which rows noticed. A fifth round
of reading `lib.rs` would be padding. A round that asks "for each guard in this crate, which CI
invocation would fail if I deleted it?" would not be.
