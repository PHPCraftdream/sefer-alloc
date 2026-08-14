# `aligned-vmem` — round 11 closing review (2026-08-14)

**Type:** closing / remediation-verification review ("did the fix round actually
work"), not a fresh from-scratch audit.

**Subject:** the six merged fix tasks (#920–#926) landed on `main` between
`d1de3bc` (exclusive) and `cc35f1a` (inclusive), against the findings in
`docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md` (V-1 … V-39).

**Reviewed range:**

```
git log --oneline d1de3bc..HEAD          # 14 commits, 6 merges + f4330d6 + cc35f1a
git diff d1de3bc..HEAD -- crates/vmem    # 737 diff lines
```

**Host:** `x86_64-pc-windows-msvc`, rustc 1.97.0 (2d8144b78 2026-07-07).
Linux/macOS conclusions below are code-reading + cross-compile conclusions
(cross-checked with `--target x86_64-unknown-linux-gnu`), not execution.

---

## Executive summary / verdict

**The fix round is substantially correct — 23 of the 24 claimed findings are
genuinely closed — but it must NOT be shipped or pushed as-is: one new test
introduced in task #923 will fail on the Linux CI runner.**

The one blocking item:

> **C-1 (HIGH).** `crates/vmem/tests/huge_pages.rs:153-174`
> (`reserve_aligned_huge_64k_single_call_path`, added for V-25) is
> **not `#[cfg]`-gated to Windows** and calls
> `reserve_aligned_huge(64 KiB, 64 KiB).expect(...)`. On Linux with
> `huge-pages`, `unix_reserve` (`crates/vmem/src/lib.rs:2148-2154`) rejects
> **any** huge request whose `size`/`align` are not multiples of
> `LINUX_HUGE_PAGE_SIZE` (2 MiB), so this returns `None` and the `expect`
> panics. `.github/workflows/ci.yml:148/167` runs
> `cargo test -p aligned-vmem --all-features` on `ubuntu-latest`.

Everything else found is LOW or INFO: four doc/comment claims that the fixes
themselves got wrong or left stale, one behaviour change shipped without a
regression test, one test whose docstring claims a guarantee it cannot provide
(demonstrated counterfactually), and a handful of notes.

**No soundness, leak, double-free, or memory-safety defect was introduced by
this round.** I re-walked every changed `unsafe` site (the V-32 fast-reserve
sub-path in `win_reserve_commit`, the V-6 fall-through, the V-2 `granted_huge`
gate, the V-12 subtraction) specifically looking for one, and did not find one.
That is an explicit null result, not an omission.

**Counts (new findings by this review):** 0 CRITICAL · 1 HIGH · 6 LOW · 4 INFO
(11 findings: C-1; C-2, C-3, C-3b, C-4, C-5, C-6; C-7, C-8, C-9, C-10).

---

## 1. Per-finding verification of the CHANGELOG's claims

The CHANGELOG section `#### aligned-vmem — round 11 …` claims 24 findings
fixed. Verified individually against the review's own text and the diff:

| Finding | Claim | Verdict | Evidence |
|---|---|---|---|
| V-1 | comment-only correction | **CLOSED (deliberately doc-only)** | `lib.rs:172-185`; see §2 |
| V-2 | THP hint only when `MAP_HUGETLB` granted | **CLOSED** | `lib.rs:2222-2229` (`if huge` → `if granted_huge`) |
| V-3 | document `granted_huge`'s implicit argument | **CLOSED** | `lib.rs:2321-2323` |
| V-6 | retry routed through the unconditional alignment check | **CLOSED (code)** | `lib.rs:1701-1758`; test claim overstated — see C-3 |
| V-7 | code + 4 doc sites agree | **CLOSED** | see §3 |
| V-8 | document discarded `VirtualFree` BOOL | **CLOSED** | `lib.rs:2062-2066` (decommit), `:2076-2080` (release) |
| V-9 | reconcile `reservation_len` wording | **CLOSED, but see C-6** | `lib.rs:726-728` |
| V-11 | overflow check hoisted into `validate_size_align` | **CLOSED** | `lib.rs:931-941`; counterfactual in §5 |
| V-12 | reject `base < reservation` | **CLOSED, but see C-4** | `lib.rs:793`, `:801`, `:811` |
| V-13 | `ReservationParts::new` | **CLOSED (constructor), doc overclaims — C-3b** | `lib.rs:872-882` |
| V-14 | document rounding granularity | **CLOSED** | `lib.rs:1582-1585` |
| V-19 | alignment check before empty-range return | **CLOSED (code); untested, doc now contradicts — C-5** | `lib.rs:1288-1293`, `:1378-1383` |
| V-20 | `compile_error!` for non-unix/non-windows | **CLOSED** | `lib.rs:2833-2852`; see C-8 |
| V-21 | crate-wide `allow(dead_code)` removed | **CLOSED** | `lib.rs:93-125` (attr deleted), `fault_injection.rs:118-130` |
| V-23 | document `Relaxed` vs `Release` | **CLOSED** | `fault_injection.rs:83-85` |
| V-25 | single-call large-page branch exercised | **CLOSED on Windows; BREAKS LINUX — C-1** | `tests/huge_pages.rs:153-174` |
| V-26 | `align > size` test | **CLOSED** | `tests/smoke.rs:187-215` |
| V-27 | containment invariant on crate output | **CLOSED** | `tests/smoke.rs:167-182`; counterfactual in §5 |
| V-32 | Windows fast-reserve for `align <= 64 KiB` | **CLOSED (logic correct); doc drift — C-2** | `lib.rs:1767-1825`; see §7 |
| V-34 | README example is a runnable test | **CLOSED** | `tests/readme_example.rs`, `README.md:19-36` |
| V-35 | document why `as_ptr` returns `*mut` | **CLOSED** | `lib.rs:517-522` |
| V-36 | `Reservation::align()` accessor | **CLOSED** | `lib.rs:592-597` |
| V-37 | document `release`'s null no-op | **CLOSED** | `lib.rs:1056-1059` (verified against `:1061-1064`, which does early-return before `mock::record`) |
| V-39 | shorten `Cargo.toml` description | **CLOSED** | `Cargo.toml:7` (~600 chars → 171) |

Findings the review raised and the round did **not** action, correctly (they
were INFO / already-tracked / explicitly deferred): V-4, V-5, V-10, V-15, V-16,
V-17, V-18, V-22, V-24, V-28, V-29, V-30, V-31, V-33, V-38. See C-9 for a
process note about where those should have been recorded.

---

## 2. V-1 — comment-only, fast-path code untouched

**Confirmed, null result.** The only V-1 hunk in the diff is the design-rationale
comment at `crates/vmem/src/lib.rs:172-185`. The arithmetic it now states is
correct and matches the review's: hit = 1 syscall, miss = 3 (`mmap` + `munmap` +
`mmap`), expected `3 - 2p` versus a flat `1`, break-even at `p = 1`, 87–131 %
more syscall traffic at the crate's own measured 34.4–56.7 % hit rates, residual
justification = 32-bit address-space economy.

The fast path itself (`try_reserve_aligned_exact`, `lib.rs:2258-2325`) is
byte-for-byte behaviourally identical: the only edit inside that function in the
whole range is the three-line V-3 comment at `:2321-2323`. `unix_reserve`'s
dispatch into it (`:2155-2159`) is unchanged. The `if huge`→`if granted_huge`
edit is in `unix_reserve`'s **over-reserve** path (`:2222`), i.e. V-2, not V-1.

`docs/perf/OPEN_ITEMS.md` item 46's card was updated in the same round
(Status → "re-evaluation required", Next trigger → "keep / disable / gate on
32-bit"), which is the correct handling under CLAUDE.md's current-state-index
convention. **Null result: no unintended behaviour change smuggled in under
V-1.**

Correctness note on the V-2 code change (the one real behaviour change on the
Unix path): `libc_madvise_hugepage` is a genuine `madvise(MADV_HUGEPAGE)` only
under `target_os = "linux"` (`lib.rs:2785-2790`) and an explicit empty no-op
elsewhere (`:2792-2796`), so narrowing the guard from `huge` to `granted_huge`
changes behaviour on Linux only, exactly where the review said it should, and is
a no-op everywhere else. `granted_huge` is `true` in the success branch
(`:2188`) and `false` in the ordinary-page fallback (`:2182`) — precisely the
distinction V-2 asked for.

---

## 3. V-7 — all four doc sites vs. the code

**Confirmed consistent, null result.** The code (`win_reserve_commit`) now:

- single-call path (`lib.rs:1688-1758`): passes `extra_commit_flags` into the
  combined `VirtualAlloc`, and returns `granted_huge = extra_commit_flags != 0`
  (`:1757`) — the **only** place huge pages can be granted;
- two-call path: commits with plain `MEM_COMMIT` (`lib.rs:1859-1860`) and
  returns `granted_huge = false` unconditionally (`:1894`, and `:1871` on the
  fallback return).

The four doc sites all now say the same thing, in near-identical words:

- `granted_huge` field doc — `lib.rs:493-497`
- `Reservation::is_huge` rustdoc — `lib.rs:617-621`
- `reserve_aligned_huge` rustdoc — `lib.rs:1530-1534`
- `README.md:65`

All four state that large pages are only ever requested/granted via the
single-call fast path (`align <=` the OS allocation granularity), and that the
two-call path never requests them so `is_huge()`/`granted_huge` is always
`false` there. That matches the code exactly.

One imprecision, not worth a finding: the phrase "the two-call path used for
`align >` that threshold" implies the two-call path is *only* used for large
aligns; it is also used for a partial initial commit (`commit_len != size`) and
for a failed single-call alignment check. The load-bearing half of the sentence
("`is_huge()` is always `false` for a reservation that takes it") is true for
all three entry conditions, so the claim is not wrong, only narrower than the
code.

---

## 4. The `f4330d6` scope-violation revert

**Confirmed clean, null result.**

- `git show f4330d6` is a 6-line-for-6-line restoration of the `checked_add`
  form in exactly two places, with no other content.
- Current state: `win_reserve_commit` has the safe form at **two** sites
  (`lib.rs:1795-1797` inside the V-32 misaligned-candidate fallback, and
  `lib.rs:1809-1811` in the `align > granularity` branch) — task #921's V-32
  restructuring split the single original site into two, and the revert's
  content survived into both. `unix_reserve` has it at `lib.rs:2160-2162`.
  There is **no** bare `size + align` *expression* left anywhere in
  `crates/vmem/src/`: `grep -rn 'size + align' crates/vmem/src/` returns 16
  hits and every one of them is inside a `//`, `///` or `//!` comment.
- The revert did not clobber #921's work: every #921 hunk (V-6 fall-through,
  V-7 plain-`MEM_COMMIT` commit, V-8 `let _ =` on both `VirtualFree` wrappers,
  V-32 fast-reserve) is present in the merged tree.
- The redundancy is genuinely belt-and-suspenders, not load-bearing: all three
  public reserve entry points call `validate_size_align` first
  (`lib.rs:1018` `try_reserve_aligned`, `:1446` `try_reserve_aligned_lazy`,
  `:1556` `try_reserve_aligned_huge`) and that function now contains the V-11
  overflow rejection (`:936-941`). `leak_zeroed_pages` reaches the backend only
  through `try_reserve_aligned`. So V-11's check does cover every real call
  path, and the retained `checked_add`s are pure defence-in-depth — which is the
  right call for an `unsafe`-adjacent path.

---

## 5. New tests: they run, and (mostly) they bite

`cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals"`
→ **all green on Windows**: `smoke` 28/28, `huge_pages` 2/2, `readme_example`
1/1, `lazy_commit` 11/11, `fault_injection` 5/5, `mock` 10/10,
`vmemerror_io_bridge` 3/3, `min_page` 2/2.
`cargo test -p aligned-vmem --all-features` → also green on Windows.

### Counterfactuals actually performed

Three real revert-and-observe cycles (all reverted afterwards; see §9):

1. **V-11 / `try_reserve_overflow_is_invalid_argument_on_all_platforms`** —
   deleted the `size.checked_add(align).is_none()` block from
   `validate_size_align` and re-ran. Result: **FAILS**, for the right reason —
   `size/align causing overflow must be invalid_argument, not OS-specific
   error: VmemError { os_code: Some(87) }` (`tests/smoke.rs:1049`). This is
   exactly the Unix/Windows classification divergence V-11 described.
   **Non-vacuous.**

2. **V-27 / containment assertions in `reserve_is_aligned_and_writable`** —
   changed `win_reserve_commit`'s two-call return from
   `Ok((base, region, over, false))` to `Ok((base, region, size / 2, false))`
   and re-ran. Result: **FAILS** at `tests/smoke.rs:179`, `usable end must be
   <= reservation end`. **Non-vacuous.**

3. **V-6 / `reserve_aligned_huge_64k_single_call_path`** — restored the
   pre-#921 shape of the huge-page retry (early
   `return Ok((n, n, commit_len, false))` with no alignment check) and re-ran
   `--test huge_pages`. Result: **BOTH TESTS STILL PASS.** See C-3.

4. **V-12 / the new `base >= reservation` check** — added a temporary test
   passing `base = reservation - PAGE` and asserting the panic message. Result:
   in the default (debug, overflow-checks-on) profile it panics at
   `lib.rs:793` with `attempt to subtract with overflow`, **not** with the
   assert's message; in `--release` it panics with the intended
   `base must be >= reservation`. See C-4.

`reserve_aligned_with_align_greater_than_size` (V-26) and `readme_example`
(V-34) are new-coverage tests, not regression guards for a specific fix, so a
revert-the-fix counterfactual does not apply; both were confirmed to execute and
to exercise real code (`reserve_aligned(PAGE, 4 MiB)` takes the Windows two-call
path with a real head offset; `readme_example` performs a real 4 MiB reservation,
write/read-back, `into_parts`, and manual `release`).

---

## 6. Regressions / lint gates

All clean, no regressions:

| Gate | Result |
|---|---|
| `cargo fmt -p aligned-vmem --check` | clean |
| `cargo clippy -p aligned-vmem --all-targets --features "lazy-commit huge-pages fault-injection bench-internals" -- -D warnings` | clean |
| `cargo clippy -p aligned-vmem --all-targets --all-features -- -D warnings` | clean |
| `cargo clippy -p aligned-vmem --all-targets -- -D warnings` (default features) | clean |
| `cargo clippy -p aligned-vmem --all-targets --features fault-injection -- -D warnings` | clean (**the V-21 case** — the crate-wide `allow(dead_code)` is gone and nothing became dead) |
| `cargo clippy -p aligned-vmem --all-targets --features "fault-injection lazy-commit" -- -D warnings` | clean |
| `cargo clippy -p aligned-vmem --all-targets --features mock -- -D warnings` | clean |
| `cargo clippy -p aligned-vmem --all-targets --features "mock fault-injection" -- -D warnings` | clean |
| same, `--target x86_64-unknown-linux-gnu` (`fault-injection`, `--all-features`) | clean |
| `RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem --features "…"` | clean |
| `cargo check -p aligned-vmem --target x86_64-unknown-linux-gnu --features "…"` | clean |

Note the distinction this makes for C-1: Linux **compiles** clean; it is the
**runtime** assertion in the new test that fails there, which no cross-compile
check can catch.

Existing regression guards were checked for silent weakening by the V-32 change:
`tests/lazy_commit.rs:70-116` (`lazy_reserve_small_align_still_reserves_full_span`,
the task #848 guard) asserts `reservation_len() >= size`. Post-V-32 the Windows
lazy path returns `reservation_len == size` instead of `size + align`, so the
assertion still holds — and it is still non-vacuous against the #848 bug, which
produced `4096 < 65536`. **Not weakened.**

---

## 7. Line-by-line scrutiny of the genuinely new logic (V-32)

`crates/vmem/src/lib.rs:1767-1825`. Traced in full:

- **Entry conditions.** The `align <= WIN_ALLOCATION_GRANULARITY` sub-path is
  reachable only when the single-call fast path was declined, i.e. when
  `commit_len != size` (the lazy path) or when the single-call alignment check
  failed. Both are handled.
- **`align_up_addr` / `fits`.** On the fast-reserve hit, `region = candidate`
  and `over = size`; `align_up_addr(region_addr, align) == region_addr` (the
  branch is taken only when `candidate_ptr.addr().is_multiple_of(align)`), so
  `end = region_addr + size` and `region_end = region_addr + over =
  region_addr + size` → `end <= region_end` holds with equality. `base ==
  region`, head offset 0. **Correct, no off-by-one.**
- **Commit range.** `commit_len <= size` is a caller invariant (`reserve_aligned_raw`
  passes `commit_len == size`; `reserve_aligned_lazy_raw` passes
  `initial_commit <= size`), so `[base, base+commit_len)` is inside the
  `size`-byte reservation. **Correct.**
- **Leak-freedom on the miss branch** (`:1788-1807`): `winapi_virtual_release(candidate_ptr)`
  runs before the `over = size + align` reserve; if that second reserve returns
  NULL we `return Err` with nothing outstanding. If it succeeds, the single
  downstream `winapi_virtual_release(region_ptr)` on each later failure exit
  (`:1848`, `:1877`, `:1883`) releases exactly the surviving region.
  **No leak, no double-free.**
- **Overflow.** The miss branch keeps `checked_add` (`:1795-1797`); the hit
  branch needs none (`over = size`). **Correct.**
- **Counters.** `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS` is still incremented on
  this path, which is right — it is still a reserve + commit pair.
- **Dead-branch status.** On real Windows the miss branch is unreachable
  (`VirtualAlloc(NULL, …)` returns a 64 KiB-granular base and `align` is a
  power of two `<= 64 KiB`, hence a divisor), exactly as the CHANGELOG states.
  Keeping it as a defensive fall-through is the same trust class as the
  task #917 check it mirrors. **Consistent with the crate's own convention.**
- **Syscall count.** Hit: `reserve(size)` + `commit` = 2, same as before.
  Miss: 3. No regression on the reachable path.

**No correctness defect found in the new logic.** Its only fallout is
documentation (C-2).

---

## 8. The now-redundant `MEM_COMMIT` retry (prompt point 8)

`crates/vmem/src/lib.rs:1859-1879`.

**Confirmed: harmless, not a correctness bug.** After #921's V-7 fix the initial
commit is `VirtualAlloc(base, commit_len, MEM_COMMIT, PAGE_READWRITE)` (`:1859-1860`)
and the `extra_commit_flags != 0` fallback issues a byte-identical call
(`:1865-1867`). Reachability: `extra_commit_flags != 0` on the two-call path
requires `reserve_aligned_huge` with `align > 64 KiB` (or the pathological
single-call-alignment-miss fall-through), **and** the commit to fail — i.e.
genuine commit-charge exhaustion. In that case the cost is one extra failing
`VirtualAlloc` before the same `Err` is returned. `VmemError::last_os_error()`
is still captured immediately after the final failing syscall (`:1875`), before
the cleanup `VirtualFree` — the task #713 timing contract is preserved. No
state is mutated between the two attempts, so no ordering hazard exists.

**Verdict: worth exactly one INFO line for a future cleanup pass (recorded as
C-7), not more.** Deleting it would also let `extra_commit_flags` be dropped
from the two-call half of the function entirely, which is a small readability
win — but it is dead-on-arrival code in a rare OOM path, and touching
`win_reserve_commit` again purely for tidiness carries more review cost than the
saving is worth.

---

## 9. Findings

### C-1 — HIGH — the new V-25 test will fail on the Linux CI runner

`crates/vmem/tests/huge_pages.rs:153-174`
(`reserve_aligned_huge_64k_single_call_path`), against
`crates/vmem/src/lib.rs:2148-2154`.

**Scenario.** On `ubuntu-latest`, `.github/workflows/ci.yml:167` runs
`cargo test -p aligned-vmem --all-features`. `tests/huge_pages.rs` is gated only
on `#![cfg(feature = "huge-pages")]` (`:34`), so this test compiles and runs
there. It calls:

```rust
let r = reserve_aligned_huge(SIZE, SIZE).expect("64 KiB huge reservation"); // SIZE = 64 * 1024
```

`reserve_aligned_huge` → `try_reserve_aligned_huge` (`lib.rs:1548-1550`) →
`reserve_aligned_huge_raw` → `unix_reserve(size, align, huge = true)`, whose
first statement is:

```rust
#[cfg(all(target_os = "linux", feature = "huge-pages"))]
if huge && (!size.is_multiple_of(LINUX_HUGE_PAGE_SIZE)
    || !align.is_multiple_of(LINUX_HUGE_PAGE_SIZE))
{
    return Err(VmemError::invalid_argument());
}
```

`LINUX_HUGE_PAGE_SIZE == 2 * 1024 * 1024` (`lib.rs:2573`). `65536 % 2 MiB != 0`,
so this returns `Err` → `Option::None` → `.expect(...)` **panics**.

This is not a hypothetical: the same file already contains
`reserve_aligned_huge_rejects_non_huge_page_aligned_align`
(`tests/huge_pages.rs:89-101`), a Linux-gated test that asserts precisely this
rejection for `align = PAGE`. `align = 64 KiB` is in the same rejected class.
The new test therefore asserts, on Linux, the exact opposite of what a
neighbouring test in the same file asserts. The file's own module doc
(`:5-17`, `:19-32`) explains this constraint and records that the pre-existing
positive test uses 4 MiB "on every platform" for exactly this reason — the
constraint was documented at the top of the file the new test was appended to.
It is also stated, in this round's own edits, one line above the V-7 README
rewording that task #924 landed: `crates/vmem/README.md:62-64` — "**on Linux,
`size` and `align` must both additionally be multiples of the huge-page size
(2 MiB), or the request is rejected up front**". Two of the six parallel tasks
therefore touched adjacent text describing the exact rule the third one's new
test violates.

macOS is unaffected (the guard is `target_os = "linux"`; on Darwin
`libc_mmap`'s `huge` flag is a documented no-op, the exact-size fast path misses
on a 16 KiB-page host and the over-reserve path yields a 64 KiB-aligned base, so
the test passes). Windows is where it was authored and verified.

**Severity HIGH:** it red-lights CI on a supported, actively-tested platform,
and the failure is a hard panic in a test the round added — the release cannot
go out with it.

**Fix (one line):** add `#[cfg(windows)]` to the test — its own docstring says
it exists to exercise "the Windows SINGLE-CALL large-page branch", so gating it
to Windows loses nothing. (A `#[cfg(not(target_os = "linux"))]` gate would also
work and would keep macOS coverage; a Linux-conditional `SIZE` of 2 MiB would
*not* work, because 2 MiB `align` on Windows takes the two-call path and defeats
the test's whole purpose.)

### C-2 — LOW — four doc sites still describe the pre-V-32 / pre-V-7 Windows two-call behaviour

Introduced by task #921's own V-32 (and, for site 3, V-7) changes; all four now
over-state when and how the crate over-reserves / commits on Windows. Verified
that the two *other* copies of the same paragraph — the crate module doc
(`lib.rs:24-32`) and `reserve_aligned`'s rustdoc (`:905-911`) — are **not**
stale: both describe the eager `reserve_aligned` entry point, whose two-call
path is reached only for `align > 64 KiB`, i.e. the branch V-32 did not
change.

1. `crates/vmem/src/lib.rs:189-191` (module design comment): "*two syscalls (the
   traditional path for larger alignments **or a partial initial commit**,
   over-reserving `size + align` …)*". After V-32, a partial initial commit with
   `align <= 64 KiB` — i.e. the ordinary `reserve_aligned_lazy(size, PAGE, n)`
   case — does **not** over-reserve.
2. `crates/vmem/src/lib.rs:710-713` (`from_raw_parts` rustdoc): "*this crate
   over-reserves `size + align` and keeps the full mapping whenever the
   exact-size fast path misses, or on Windows when `align > 64 KiB` **or the
   initial commit is partial (`commit_len != size`)***". Same stale disjunct, in
   a **public** doc this time.
3. `crates/vmem/src/lib.rs:1639-1646` (`win_reserve_commit`'s own doc header):
   "*Reserves `size + align` bytes, finds the aligned base, and commits
   `commit_len` bytes (with `extra_flags` OR-ed into `MEM_COMMIT`, e.g.
   `MEM_LARGE_PAGES`)*". **Both** halves of that sentence are now false for the
   two-call path: V-32 made the reserve length conditional, and V-7 made the
   commit unconditionally plain `MEM_COMMIT` (`:1859-1860`). This is the
   function's own contract comment, sitting directly above the code both fixes
   changed — the single most likely place a future reader looks first.
4. `crates/vmem/src/lib.rs:563-585` (`reservation_len` rustdoc): the "at least
   two paths under-report the true OS reservation size" list names the Windows
   single-call fast path and `mmap` page-rounding. V-32 adds a **third**:
   the Windows two-call fast-reserve sub-path now returns `over = size`
   (`lib.rs:1787`) while `VirtualAlloc(MEM_RESERVE)` consumes a 64 KiB-granular
   region — so `reserve_aligned_lazy(PAGE, PAGE, PAGE)` reports
   `reservation_len() == 4096` against 64 KiB of real VA, exactly the shape the
   first bullet already documents for the eager path.

**Severity LOW:** no behavioural consequence (the value is advisory on Windows
and `VirtualFree(MEM_RELEASE)` ignores lengths), but (2) and (3) are public
API docs, and this crate's whole reviewing history is about doc claims drifting
from code — V-7 and V-9 in this very round are the same defect class.

### C-3 — LOW — the new V-25 test's docstring claims a V-6 regression guard it cannot provide

`crates/vmem/tests/huge_pages.rs:151-152`:

> *This also serves as a regression guard for task #921's V-6 alignment-check
> fix in the Windows single-call path.*

The CHANGELOG repeats it ("doubles as a regression guard for task #921's V-6
fix").

**Demonstrated false by counterfactual.** I restored the pre-#921 shape of the
retry branch (early `return Ok((n, n, commit_len, false))` with no alignment
check) and re-ran `--test huge_pages`: **both tests passed.** They must — on
real Windows the base the retry returns *is* 64 KiB-aligned, so the presence or
absence of the check makes no observable difference. V-6's fix only matters in
the world the task #917 comment distrusts (a wrong
`WIN_ALLOCATION_GRANULARITY`), which no test can construct without injecting a
fake allocator.

The test's *primary* claim — that it is the first test to execute the Windows
single-call `MEM_LARGE_PAGES` branch (V-25's actual finding) — **is true** and
is real new coverage (`align == size == 64 KiB` → `lib.rs:1688` guard true →
combined `VirtualAlloc` with `extra_commit_flags = MEM_LARGE_PAGES`).

**Severity LOW:** the coverage is genuine, only the guard claim is not. But this
project's own methodology (CLAUDE.md: "verify the tests are not vacuous —
counterfactual") makes an unearned "regression guard" label exactly the kind of
claim that gets trusted later and shouldn't be. Fix: delete the sentence, or
replace it with "the V-6 check is unobservable on conforming Windows and is not
regression-tested by this or any test".

### C-3b — LOW — `ReservationParts`'s new doc claims a use case it still cannot serve

`crates/vmem/src/lib.rs:858-860` (struct doc) and `:873-877`
(`ReservationParts::new` doc):

> *The motivating use case ("self-hosted metadata") is now fully realized:
> construct a `ReservationParts` from saved fields, then reconstruct a
> `Reservation` via `Reservation::from_raw_parts` when needed.*

`ReservationParts` holds three fields — `ptr` (the **reservation** base,
`:864-865`), `len` (the **reservation** length, `:866-867`), `align`.
`Reservation::from_raw_parts` (`lib.rs:740-746`) requires **five**: `base`,
`len` (the *usable* span), `reservation`, `reservation_len`, `align`. For any
over-reserved reservation — the Unix fast-path miss and the Windows
`align > 64 KiB` two-call path, i.e. the two cases `from_raw_parts` was
explicitly built for (`:709-713`) — `base != ptr` and the usable `len !=
reservation_len`, and **neither is recoverable from a `ReservationParts`.** The
round-trip works only in the degenerate `base == reservation` case.

The honest pre-round text ("*cannot currently be realized because you need to
construct a `ReservationParts` from already-saved fields*") was replaced by a
claim that is stronger than what shipped. This is the same overclaiming-comment
pattern V-12 was filed about, reproduced one round later — the review predicted
this exact recurrence in its V-12 write-up.

**Severity LOW:** `ReservationParts::new` is a genuine, correct, useful addition
(it does close the "no public constructor" half of V-13); only the doc sentence
overstates the result. Fix: say what is true — the constructor closes the
`release_parts` round-trip; reconstructing a full `Reservation` additionally
needs the usable `base`/`len`, which the caller must store separately.

### C-4 — LOW — V-12's new check runs *after* the subtraction it is documented to guard

`crates/vmem/src/lib.rs:793`:

```rust
let offset = base_addr - res_addr; // SAFETY: guarded by `base_addr >= res_addr` assert below
```

with `base_addr >= res_addr` appearing at `:801`, inside the `assert!` that
starts at `:794`.

**Measured, both profiles** (temporary test, `base = reservation - PAGE`):

- debug / `overflow-checks = true` (the default `cargo test` profile): panics at
  `crates/vmem/src/lib.rs:793` with `attempt to subtract with overflow` — the
  carefully-written multi-clause assert message at `:807-816` is **never
  reached**;
- `--release`: the subtraction wraps, the assert's `base_addr >= res_addr`
  clause short-circuits before the `len.checked_add(offset)` clause at `:803`,
  and the intended `base must be >= reservation` message is produced.

So the fix is *functionally* correct in both profiles (the contract violation
always panics, never constructs a bogus `Reservation`), but the diagnostic is
profile-dependent, and the inline `// SAFETY:` comment is wrong about the
ordering it claims. Note also that **no test covers the new check** — every one
of the eight `from_raw_parts_rejects_*` tests in `tests/smoke.rs` predates this
round and none passes `base < reservation`.

**Severity LOW:** no unsafety, no wrong result — a diagnostics-quality and
comment-accuracy defect. Fix: move the subtraction below the `assert!` (or use
`base_addr.wrapping_sub(res_addr)` with the comment corrected), and add the
missing negative test.

### C-5 — LOW — V-19's behaviour change shipped with no test, and one doc now contradicts it

`crates/vmem/src/lib.rs:1288-1293` (`try_recommit`) and `:1378-1383`
(`try_commit_range`) — the reorder is correct and does what V-19 asked.

Two gaps:

1. **No regression test.** `try_recommit(base, 5, 5)` / `try_commit_range(base, 5, 5)`
   changed from `Ok(())` to `Err(invalid_argument)` — a public,
   observable behaviour change — and nothing asserts it.
   `recommit_rejects_contract_violating_offsets` (`tests/smoke.rs:321-357`)
   covers only *non-empty* misaligned ranges (`(1, PAGE)`, `(0, PAGE+1)`,
   `(span+PAGE, span)`); `decommit_recommit_roundtrip` (`:308`) covers the
   aligned-empty `(0, 0)` case, which still returns `true`. The one input class
   the fix changed is untested, so a future revert of the reorder is invisible.
2. **`commit_range`'s doc now contradicts the code.** `lib.rs:1323-1326`:
   "*A genuinely empty range (`start == end`) is a no-op returning `true`; any
   other contract violation (misaligned, or `start > end`) returns `false`*".
   Read literally, `commit_range(base, 5, 5)` matches the first clause
   (`start == end`) and should return `true`; it now returns `false`.
   `recommit`'s parallel doc (`:1257-1258`) says "*a **well-formed** no-op —
   empty range, `start == end`*", whose "well-formed" qualifier survives the
   change — so only the `commit_range` copy needs the same word.

**Severity LOW:** the new behaviour is the *desired* one and is strictly safer
(it can only turn a `true` into a `false`, never the reverse — the direction
task #712's incident was about). The defect is the missing guard plus one stale
sentence.

### C-6 — LOW — V-9's rewording drops the Unix/miri hard requirement on `reservation_len`

`crates/vmem/src/lib.rs:726-728`, inside `from_raw_parts`'s **`# Safety`**
section:

> *`reservation_len` is the value this crate itself would report via
> `Reservation::reservation_len` for an equivalent reservation …*

The review offered two fixes: weaken the wording, **or** "state that the field
is advisory on Windows". The weakening route was taken, but it weakens the
statement on *all* platforms, and on two of them the field is not advisory at
all:

- Unix: `release_reservation` (`lib.rs:2327-2332`) passes it straight to
  `munmap(reservation, reservation_len)`. Too small → the tail of the mapping is
  permanently leaked.
- miri: `release_reservation` (`lib.rs:2830-…`) uses it as the `Layout` size for
  `std::alloc::dealloc`. A mismatch is **undefined behaviour**, and closing
  exactly that Drop-reachable hazard is the documented reason this `assert!`
  exists (`:750-776`).

The replacement text is also self-referential for the documented consumer: the
"cross-crate handoff" caller (`numa-shim` on Windows, `:705-708`) is adopting a
reservation this crate did **not** create, so "the value this crate would report
for an equivalent reservation" gives it nothing operational to compute.

**Severity LOW** (not MEDIUM): the previous wording's contradiction was real and
had to be resolved, no current caller is misled (the crate's own value is always
the true `mmap` length on Unix), and the numeric constraint
`reservation_len >= len + (base - reservation)` immediately after it is
unchanged and still enforced. Fix: split by platform — "on Unix/miri this MUST
be the full length of the underlying mapping, because `release` passes it to
`munmap`/`dealloc`; on Windows `VirtualFree(MEM_RELEASE)` ignores it, so it is
advisory there".

### C-7 — INFO — the two-call `MEM_COMMIT` retry is now a no-op duplicate

`crates/vmem/src/lib.rs:1861-1879`. Full analysis in §8. Harmless; one wasted
`VirtualAlloc` on a genuine-OOM path that also requested huge pages with
`align > 64 KiB`. Recorded so a future cleanup pass can delete the branch and
drop `extra_commit_flags` from the two-call half of the function.

### C-8 — INFO — the `compile_error!` doc comment produces an `unused_doc_comments` warning on the targets it fires for

`crates/vmem/src/lib.rs:2833-2852`: a `///` doc block is attached to a
`compile_error!` macro invocation. Verified with a standalone `rustc --edition 2021`
reproduction: when the `cfg` is active, rustc emits both the intended error and
`warning: unused doc comment … rustdoc does not generate documentation for macro
invocations`. When the `cfg` is inactive (every supported target) the item is
stripped and no warning appears, so no supported build is affected.

The V-20 fix itself **works**: the macro resolves from the `std` prelude on a
`std`-having unsupported target (`wasm32-wasip1`, `x86_64-fortanix-unknown-sgx`)
and produces the attributable message. (A `no_std` target such as
`x86_64-unknown-none` fails earlier and differently — `can't find crate for
std` — which is itself attributable, and is out of the review's stated scope.)

**Severity INFO.** Fix if desired: make the block `//` comments instead of `///`.

### C-9 — INFO — the round closed 24 findings but indexed none of the ones it left open

`docs/CORRECTNESS_OPEN_ITEMS.md` is untouched by the whole range
(`git diff --stat d1de3bc..HEAD` lists only `docs/perf/OPEN_ITEMS.md`). Per
CLAUDE.md's round convention ("*When a gate report / commit / review newly flags
an open item, add it to the appropriate index in the same commit*"), the
following review-flagged items now exist only inside a review document:

- **V-22** — the `mock` non-additive-feature `--cfg` decision. Item 42 in
  `docs/CORRECTNESS_OPEN_ITEMS.md:1887-1893` is marked **Closed** (doc-only
  fix), but the review's point is that the manifest's own deadline ("*stays free
  only until 0.2.0 ships*") **is now**, and nothing in this round records a
  decision either way. This is the single largest irreversible call in the
  release and it is currently unrecorded.
- **V-4** — `decommit_lazy` falls back to `MADV_DONTNEED` on all four BSDs where
  `MADV_FREE` exists. Not indexed anywhere.
- **V-18's new sub-observation** — `from_raw_parts` hard-codes
  `granted_huge: false` (`lib.rs:824`), so an *adopted* huge reservation reports
  `is_huge() == false` and a caller following `decommit`'s own advice ("use
  `is_huge()` to detect the case") fails **open** into the broken behaviour.
  The review states explicitly that existing item 48 does not record this.
- **V-29 / V-31** — the documented-tautological `min_page_equals_page` test and
  the remaining untested corners.

**Severity INFO** (process, not code). Recorded because this repository's own
stated failure mode is exactly "a flagged item that lives in neither index and
is rediscovered two rounds later".

### C-10 — INFO — a cosmetic asymmetry left by the V-2 fix

`crates/vmem/src/lib.rs:2314-2318` (`try_reserve_aligned_exact`) still gates the
THP hint on `if huge`, while `unix_reserve` (`:2222`) now gates it on
`if granted_huge`. Both are *correct*: on the exact fast path a successful
`mmap` with `MAP_HUGETLB` implies the grant (which is exactly what V-3's new
comment at `:2321-2323` documents), and the helper is a compiled no-op on
non-Linux (`:2792-2796`). But two sibling functions now spell the same concept
two different ways, which is how the next reader ends up re-deriving the
argument. One clarifying comment on the fast-path site would close it.

---

## 10. Read-only compliance

Every temporary edit made for the counterfactuals in §5 was reverted:

- `crates/vmem/src/lib.rs` — restored via `git checkout --` after each of the
  three source-level counterfactuals;
- `crates/vmem/tests/zz_tmp_review.rs` — temporary file, deleted.

Final state verified: `git diff --stat` is **empty**, `git status --porcelain`
shows only `?? docs/checkpoints/2026-08-13-2100.md` (pre-existing, untracked,
not created by this review) plus this report. No `git add`, `git commit`,
`git push`, branch creation, or version bump was performed. `HEAD` is unchanged
at `cc35f1addb87ffe1d6dcbc5daf5e583a1eb7f3db`.

---

## Findings index

| ID | Sev | Area | File:line | One line |
|---|---|---|---|---|
| C-1 | **HIGH** | Tests / CI | `crates/vmem/tests/huge_pages.rs:153-174` | The new V-25 test is not `#[cfg]`-gated and panics on Linux, where 64 KiB huge requests are rejected by `lib.rs:2148-2154` |
| C-2 | LOW | Docs | `lib.rs:189-191`, `:710-713`, `:1639-1646`, `:563-585` | Four sites still say the Windows two-call path over-reserves `size + align` on a partial commit; V-32 made that false |
| C-3 | LOW | Tests | `crates/vmem/tests/huge_pages.rs:151-152` | "Also a regression guard for V-6" is false — demonstrated: the test passes with V-6's fix reverted |
| C-3b | LOW | API docs | `lib.rs:858-860`, `:873-877` | `ReservationParts` still cannot round-trip through `from_raw_parts` (no `base`/usable `len`); the new doc says it can |
| C-4 | LOW | API | `lib.rs:793` | V-12's `base >= reservation` check runs after the subtraction it claims to guard; debug builds panic with "subtract with overflow" instead; no test |
| C-5 | LOW | commit | `lib.rs:1288-1293`, `:1378-1383`, `:1323-1326` | V-19's behaviour change has no regression test, and `commit_range`'s doc still describes the old behaviour |
| C-6 | LOW | API docs | `lib.rs:726-728` | V-9's rewording drops the Unix/miri hard requirement that `reservation_len` be the true mapping length (`munmap`/`Layout` consume it) |
| C-7 | INFO | Windows | `lib.rs:1861-1879` | Post-V-7, the large-page commit retry is a byte-identical duplicate of the call that just failed — harmless, deletable |
| C-8 | INFO | cfg | `lib.rs:2833-2852` | `///` on a `compile_error!` invocation adds an `unused_doc_comments` warning on the targets it fires for |
| C-9 | INFO | Process | `docs/CORRECTNESS_OPEN_ITEMS.md` (untouched) | V-22 (the 0.2.0-deadline `mock` decision), V-4, V-18's new sub-observation, V-29/V-31 were left open but indexed nowhere |
| C-10 | INFO | Unix | `lib.rs:2314-2318` vs `:2222` | V-2 left the two THP-hint sites spelling the same concept differently (`if huge` vs `if granted_huge`); both correct |

### Explicit null results

- **No soundness/UB/leak/double-free defect introduced** by any of the six
  tasks — every changed `unsafe` site re-walked (§2, §4, §7).
- **V-1's fast-path code is byte-for-byte behaviourally unchanged** (§2).
- **V-7's four doc sites and the code agree** (§3).
- **`f4330d6`'s revert is clean and coexists correctly with #921's V-32
  restructuring**; no bare `size + align` survives anywhere in
  `crates/vmem/src/` (§4).
- **V-32's new fast-reserve sub-path is correct** — alignment, `fits`,
  containment, leak-freedom on the fallback, overflow guard, counters (§7).
- **The `MEM_COMMIT` retry redundancy is harmless**, not a correctness bug (§8).
- **No lint/format/test regression** in any of the eleven gate invocations run,
  including the V-21 `--features fault-injection` row and the Linux
  cross-compile (§6).
- **The task #848 regression guard was not weakened** by V-32 (§6).
