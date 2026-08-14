# `aligned-vmem` — closing review of the round-11 **closing-review fix pass** (2026-08-14)

**Type:** closing / remediation-verification review of a remediation round
("did the fix pass for C-1 … C-10 actually work"), not a fresh audit.

**Subject:** the seven merged fix tasks (#928–#935) plus the interspersed
zero-trust follow-up commits, landed on `main` between `cc35f1a` (exclusive)
and `b8966ba` (`HEAD`, inclusive), against the findings in
`docs/reviews/2026-08-14-aligned-vmem-round11-closing-review.md` (C-1 … C-10).

**Reviewed range:**

```
git log --oneline cc35f1a..HEAD                     # 18 commits (7 merges + 11 non-merge)
git diff cc35f1a..HEAD -- crates/vmem docs/CORRECTNESS_OPEN_ITEMS.md CHANGELOG.md
#   CHANGELOG.md                    |  18 ++
#   crates/vmem/src/lib.rs          | 110 ++--   (net +30 lines)
#   crates/vmem/tests/huge_pages.rs |   7 +-
#   crates/vmem/tests/smoke.rs      |  84 ++++   (net +82 lines)
#   docs/CORRECTNESS_OPEN_ITEMS.md  |  52 ++-
```

**Host:** `x86_64-pc-windows-msvc`. Linux/macOS conclusions are code-reading +
cross-compile conclusions (`--target x86_64-unknown-linux-gnu`), not execution.

---

## Executive summary / verdict

**The fix pass is correct on every axis that can break a build or a user: all
ten findings are actioned or accounted for, the blocking C-1 is genuinely
closed, the two new tests are genuinely non-vacuous (verified by
revert-and-observe, three separate reverts), and every one of the eleven gate
invocations in §3 is green. Nothing found here blocks the release.**

The single most reassuring measurement is also the cheapest one: **the entire
shipping-code delta of this round in `crates/vmem/src/` is three lines** — the
`base_addr - res_addr` subtraction moved from its own `let` binding into the
`assert!`'s boolean chain. Everything else in `src/` is comments and rustdoc;
everything else in the round is tests, `CHANGELOG.md`, and
`docs/CORRECTNESS_OPEN_ITEMS.md`. Verified mechanically:

```
git diff cc35f1a..HEAD -- crates/vmem/src/lib.rs \
  | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)' | grep -vE '^[+-]\s*(///|//!|//|$)'
-        let offset = base_addr - res_addr; // SAFETY: guarded by `base_addr >= res_addr` assert below
-                    .checked_add(offset)
+                    .checked_add(base_addr - res_addr)
```

So the round's soundness risk is structurally bounded to one expression move,
which I traced clause-by-clause (§2.2) and confirmed correct.

**But the campaign's own recurring pattern — "the fix creates the next round's
finding" — did reproduce, in a milder form, five times.** Every instance is a
doc/comment/index defect, none is a code defect:

- two of the four C-2 doc corrections and the C-6 rewrite each fixed the stale
  claim they were asked to fix and introduced a *new* inaccuracy or omission in
  the same sentence (**R-4**, **R-5**);
- the C-10 comment added to *remove* an inconsistency now contradicts an
  existing comment six lines below it (**R-12**);
- the C-9 index additions cite `lib.rs` line numbers that are stale by 9–59
  lines — several of them made stale *by this same round's own parallel edits*,
  and in direct violation of a standing warning written into the very
  closure-narrative this round re-opened (**R-2**), and one of the three new
  items misdescribes its own mechanism (**R-3**);
- the item-42 tier move fixed one direction of the link but left the closure
  trail still reading `**CLOSED**` (**R-1**).

One finding is not a doc defect but a latent test-hygiene regression: the new
C-4 test uses out-of-bounds pointer arithmetic (`raw.sub(1)`) where the
immediately-preceding sibling test in the same file deliberately uses
`wrapping_add(1)` and documents why (**R-6**).

One finding is a scope gap: **C-7 was silently not actioned** — no task
addressed it, no CHANGELOG line records the decision, and it is in neither
open-items index (**R-8**). It is harmless code, but the round's own C-9 work
existed precisely to prevent unindexed leftovers.

**Explicit null results** (things checked that were fine — §6 lists all of
them): the three human-caught issues (`67f6236`, `1d2e821`, `53ba5dc`) are all
fixed correctly and completely, including the short-circuit ordering trace and
a full re-audit of every `use aligned_vmem::{…}` block in all eight test files
against each symbol's own `#[cfg]` gate; no soundness, leak, double-free or UB
defect in shipping code; no conflict-marker residue; the repo's own
`verify-commit-prefixes.mjs` R30-12 taxonomy lint passes on all 32 commits.

**Counts (new findings by this review):** 0 CRITICAL · 0 HIGH · 5 LOW · 5 INFO
(10 findings: R-1 … R-6 LOW-or-below; R-7 … R-12 INFO). Recommendation:
**SHIP** — none of these is a release blocker; R-4/R-5/R-6 are worth a small
follow-up pass, and R-8/R-2 are worth one index edit each.

---

## 1. Per-finding verification: C-1 … C-10

| Finding | Sev (orig) | Verdict | Where |
|---|---|---|---|
| C-1 | HIGH/BLOCKING | **CLOSED** (independently verified) | §1.1 |
| C-2 | LOW (4 sites) | **CLOSED 4/4**, but site 1 introduced a new inaccuracy (**R-4**) | §1.2 |
| C-3 | LOW | **CLOSED** | §1.3 |
| C-3b | LOW | **CLOSED** | §1.4 |
| C-4 | LOW | **CLOSED** (code + test; test caveats **R-6**, **R-7**) | §1.5 |
| C-5 | LOW | **CLOSED** (code already correct; test + doc added) | §1.6 |
| C-6 | LOW | **CLOSED**, but dropped two documented invariants (**R-5**) | §1.7 |
| C-7 | INFO | **NOT ACTIONED, NOT RECORDED** (**R-8**) | §1.8 |
| C-8 | INFO | **CLOSED** | §1.9 |
| C-9 | INFO | **CLOSED with defects** (**R-1**, **R-2**, **R-3**) | §1.10 |
| C-10 | INFO | **CLOSED**, new internal contradiction (**R-12**) | §1.11 |

### 1.1 C-1 (BLOCKING) — genuinely closed, verified independently of the comment

`crates/vmem/tests/huge_pages.rs:155-157` now reads:

```rust
#[test]
#[cfg(windows)]
fn reserve_aligned_huge_64k_single_call_path() {
```

I did **not** take the fix comment's word for the Linux behaviour. Traced from
the test's own call:

1. `reserve_aligned_huge(64 KiB, 64 KiB)` (`tests/huge_pages.rs:159`) →
   `try_reserve_aligned_huge` (`crates/vmem/src/lib.rs:1569`) →
   `reserve_aligned_huge_raw` → `unix_reserve(size, align, huge = true)`.
2. `unix_reserve`'s **first statement** (`crates/vmem/src/lib.rs:2174-2179`):

```rust
#[cfg(all(target_os = "linux", feature = "huge-pages"))]
if huge
    && (!size.is_multiple_of(LINUX_HUGE_PAGE_SIZE)
        || !align.is_multiple_of(LINUX_HUGE_PAGE_SIZE))
{
    return Err(VmemError::invalid_argument());
}
```

3. `LINUX_HUGE_PAGE_SIZE = 2 * 1024 * 1024` (`crates/vmem/src/lib.rs:2603`).
   `65536 % 2 MiB != 0` → `Err` → `Option::None` → `.expect("64 KiB huge
   reservation")` **panics**. The guard is reached before any `mmap`, so no
   host-configuration variable can rescue it.

Both halves of C-1 therefore confirmed: the pre-fix test **would** have
panicked on `ubuntu-latest` under `.github/workflows/ci.yml:167`
(`cargo test -p aligned-vmem --all-features`), and `#[cfg(windows)]` excludes
it there as a language property, not as a claim.

Two collateral checks on the gate, both clean:

- **The import stays used on non-Windows.** `use aligned_vmem::reserve_aligned_huge;`
  (`tests/huge_pages.rs:36`) is ungated; had the Windows-only test been its only
  non-Linux consumer, macOS would have gone red on `unused_imports` under
  `-D warnings`. It is not — `reserve_aligned_huge_ordinary_page_sized_request_succeeds`
  (`:43-50`) is ungated and calls it. Verified.
- **The gate is `#[cfg(windows)]`, not `#[cfg(not(target_os = "linux"))]`,** so
  macOS loses the coverage the prior review said it would have kept. That is the
  fix the review itself recommended first and the test's own docstring justifies
  ("the Windows SINGLE-CALL large-page branch"), so this is a deliberate,
  documented narrowing, not a defect. Recorded, not filed.

Post-fix test counts on this Windows host: `huge_pages` 2/2 (§3). On Linux the
file now compiles 5 tests (1 ungated + 4 `#[cfg(target_os = "linux")]`), none of
which is the 64 KiB one.

### 1.2 C-2 — all four stale doc sites corrected; site 1 now has a *different* inaccuracy

| Site | Location now | Verdict |
|---|---|---|
| 1. module design comment | `crates/vmem/src/lib.rs:186-192` | corrected, but see **R-4** |
| 2. `from_raw_parts` rustdoc | `crates/vmem/src/lib.rs:715-718` | **correct** |
| 3. `win_reserve_commit` doc header | `crates/vmem/src/lib.rs:1653-1671` | **correct** |
| 4. `reservation_len` rustdoc (3rd path) | `crates/vmem/src/lib.rs:566-579` | **correct** |

Site 3 was the one the prior review called "the single most likely place a
future reader looks first", and it is the best of the four. I checked its every
claim against the code:

- *"Single-call fast path (`align <= WIN_ALLOCATION_GRANULARITY && commit_len ==
  size`)"* — matches the guard at `:1713` exactly.
- *"Returns `(base, base, commit_len, extra_commit_flags != 0)`"* — matches the
  return at `:1782`; the third element is `commit_len`, which equals `size` on
  that path by the guard.
- *"Two-call path (**all other cases**)"* — correct, and notably *more* accurate
  than site 1 (see R-4), because it does not try to re-enumerate the entry
  conditions.
- *"commits `commit_len` bytes with plain `MEM_COMMIT` (no extra flags
  applied)"* — matches the commit call at `:1884-1885` and the `extra_commit_flags != 0`
  retry at `:1890-1892`, which is itself byte-identical plain `MEM_COMMIT`.
- *"The reserve size is conditional … attempts to reserve exactly `size` bytes
  and uses it if the result happens to already satisfy alignment; otherwise …
  `size + align`"* — matches `:1792-1850` exactly, both branches.
- *"the fourth element is always `false` … (Windows rejects `MEM_LARGE_PAGES` on
  pre-reserved regions anyway)"* — matches the `false` returns at `:1896` and
  `:1919`, and the claim about `MEM_LARGE_PAGES` requiring a combined
  `MEM_RESERVE | MEM_COMMIT` call is correct.

Site 4 is likewise accurate: the third bullet's example
(`reserve_aligned_lazy` with `align <= 64 KiB`, fast-reserve hit, `over = size`)
matches `:1812` (`(candidate, size)`).

Site 2's narrowing (dropping the "or the initial commit is partial" disjunct) is
correct for every reachable case: on real Windows the fast-reserve alignment
check cannot miss (a `VirtualAlloc(NULL, …)` base is 64 KiB-granular and `align`
is a power of two `<= 64 KiB`, hence a divisor), so with `align <= 64 KiB` the
crate never over-reserves regardless of `commit_len`.

### 1.3 C-3 — closed

`crates/vmem/tests/huge_pages.rs:151-154` now says the V-6 check "is
unobservable on a conforming Windows host and is NOT regression-tested by this
or any test in this crate — it guards against `WIN_ALLOCATION_GRANULARITY` being
wrong, a condition that cannot be constructed without a fake/mocked allocator
backend." That is exactly what the prior review demonstrated by counterfactual,
stated without overclaim, and it does not contradict the test's surviving
(true) primary claim about being the first exerciser of the single-call
`MEM_LARGE_PAGES` branch. Clean fix.

### 1.4 C-3b — closed

`crates/vmem/src/lib.rs:867-872` (struct doc) and `:887-891`
(`ReservationParts::new` doc) both now say the constructor "closes the
`release_parts` round-trip … Reconstructing a full `Reservation` via
`from_raw_parts` additionally requires the usable `base` and `len`, which the
caller must record separately — `ReservationParts` alone is insufficient
whenever the reservation was over-reserved for alignment."

Checked against the types: `ReservationParts` carries `ptr`/`len`/`align`
(`:875-882`); `from_raw_parts` takes five values including the usable `base` and
usable `len` (`:750-756`). The new text is exactly the true statement the prior
review asked for, in both places, with no residual overclaim. Clean fix.

### 1.5 C-4 — closed (code correct, test non-vacuous; two caveats)

Code: §2.2 traces the short-circuit ordering in full and confirms it.
Test: `crates/vmem/tests/smoke.rs:997-1024`, counterfactually verified in §4.1.
Caveats **R-6** (out-of-bounds pointer arithmetic) and **R-7** (profile
dependence) below; neither invalidates the fix.

### 1.6 C-5 — closed

The V-19 code reorder was already correct at `cc35f1a` and is untouched
(`try_recommit` `:1301-1307`, `try_commit_range` `:1391-1397`). This round added
what was missing:

- `recommit_rejects_misaligned_empty_range` (`crates/vmem/tests/smoke.rs:361-411`)
  — counterfactually verified in §4.2, **both** halves (the `try_recommit` half
  and the `lazy-commit`-gated `try_commit_range`/`commit_range` half bite
  independently);
- `commit_range`'s doc (`:1337-1339`) now reads "A **well-formed** no-op (empty
  range, `start == end`) returns `true`", matching `recommit`'s existing
  wording, and `try_commit_range`'s doc (`:1381`) got the same qualifier. The
  literal contradiction the prior review found (`commit_range(base, 5, 5)`
  matching "`start == end` ⇒ `true`") is gone.

### 1.7 C-6 — closed, but with a documented-invariant regression

`crates/vmem/src/lib.rs:731-738` now splits the contract by platform exactly as
the prior review's preferred fix described, and every technical claim in it is
correct: Unix's `release_reservation` passes `reservation_len` to `munmap`
(`:2361`), miri's passes it as the `Layout` size to `dealloc` (`:2860`),
and Windows' `VirtualFree(MEM_RELEASE)` ignores it. See **R-5** for what the
rewrite dropped.

### 1.8 C-7 — not actioned, and not recorded anywhere

See **R-8**. The code (`crates/vmem/src/lib.rs:1886-1900`) is unchanged and, per
the prior review's own §8, harmless.

### 1.9 C-8 — closed

`crates/vmem/src/lib.rs:2863-2873` is now a `//` block instead of `///`,
immediately above the `#[cfg(all(not(windows), not(unix), not(miri)))]
compile_error!(…)`. Verified comment-only:
`git show c7ec951 -- crates/vmem/src/lib.rs` yields **zero** non-comment changed
lines. The `unused_doc_comments` warning can no longer fire on the unsupported
targets the block exists to inform, and no supported target is affected (the
item is `cfg`'d out there).

### 1.10 C-9 — actioned, with three defects

Item 42 was moved into `[A]` with a current-state card, and items 52-54 were
added for V-4, the V-18 sub-observation, and V-29/V-31 — so the substance of C-9
(these findings now live in the index, not only in a review doc) is genuinely
closed. Three defects: **R-1** (closure trail not updated), **R-2** (stale line
citations), **R-3** (item 53 misdescribes its own mechanism). Two adjacent,
pre-existing observations recorded as **R-9** and **R-10**.

### 1.11 C-10 — closed, with a new internal contradiction

`crates/vmem/src/lib.rs:2341-2345` now explains why `try_reserve_aligned_exact`
gates the THP hint on `if huge` while `unix_reserve` gates on `if granted_huge`.
The core argument is correct for Linux — `MAP_HUGETLB` is all-or-nothing, the
function returns early on `mmap` failure (`:2294-2297`) and on the alignment
miss (`:2330-2333`), so on Linux reaching the hint does imply a grant. See
**R-12** for the non-Linux overstatement.

---

## 2. Re-derivation of the three already-caught-and-fixed issues

All three were fixed **correctly and completely**. Details below; each was
re-derived from the code, not from the commit message.

### 2.1 `67f6236` — item 42's tier move

- **Physically correct now.** Item 42 sits at `docs/CORRECTNESS_OPEN_ITEMS.md:86-108`,
  inside `### [A] Active` (header at `:61`), before `### [T] Tracked` (header at
  `:110`). Its card leads with `**Status:** UNRESOLVED and URGENT`, followed by
  `Next trigger` and `Evidence` — the current-state-card shape CLAUDE.md's
  R34-24 rule requires.
- **No orphan or duplicate.** `grep -n 'Cargo-feature-unification'` returns
  exactly three hits: `:86` (the moved item), `:2374` (its closure narrative,
  numbered 3 in the resolved trail), `:2396` (prose inside that narrative). The
  old `[T]`-section block is gone — the numbered-item scan runs `… 41 (:1844),
  43 (:1911) …` with no 42 between them.
- **No number collision among open items.** The only open item numbered 42 is
  `:86`. (A *different* item numbered 42 exists in the resolved trail at
  `:3355`; that collision is pre-existing — **R-10**.)
- **One thing left undone:** the closure trail still says `**CLOSED**` — **R-1**.

Its evidence citation `crates/vmem/Cargo.toml:62-87` was checked and is accurate
(the `mock` hazard block runs `:55-87`, and `:87` is `mock = []`; the "stays free
only until 0.2.0 ships" sentence is inside the cited range).

### 2.2 `1d2e821` — the dead `_offset` removal, and the C-4 reordering itself

- **No leftovers.** `grep -n '_offset' crates/vmem/src/lib.rs` → no matches. The
  two surviving locals in `from_raw_parts`, `base_addr` (`:801`) and `res_addr`
  (`:802`), are both used (in the assert chain and in the assert message). The
  whole-round non-comment delta quoted in the executive summary confirms nothing
  else in `src/` moved.
- **The reordering is correct — traced, not assumed.** The `assert!` at
  `crates/vmem/src/lib.rs:803-825` is one boolean expression whose operands are
  joined exclusively by `&&`, which Rust evaluates left-to-right with
  short-circuit semantics in *both* debug and release. Operand order:

  1. `align.is_power_of_two()`
  2. `align >= PAGE`
  3. `reservation_len != 0`
  4. `reservation_len.is_multiple_of(PAGE)`
  5. `len != 0`
  6. `len.is_multiple_of(PAGE)`
  7. **`base_addr >= res_addr`**  ← `:810`
  8. `base_addr.is_multiple_of(align)`
  9. **`len.checked_add(base_addr - res_addr).is_some_and(…)`**  ← `:812-814`
  10. `std::alloc::Layout::from_size_align(reservation_len, align).is_ok()`

  Operand 9 contains the only subtraction, and it is strictly to the right of
  operand 7. If `base_addr < res_addr`, operand 7 is `false`, the `&&` chain
  short-circuits, and operand 9 is **never evaluated** — so
  `base_addr - res_addr` cannot underflow. Confirmed empirically in §4.1: with
  the pre-fix ordering restored the panic is `attempt to subtract with overflow`
  at `lib.rs:803`; with the fix in place it is the intended multi-clause assert
  message.

  Also confirmed: no *other* path in `from_raw_parts` computes the offset. The
  constructed `Self { … }` (`:827-834`) stores only the five arguments plus
  `granted_huge: false`.

### 2.3 `53ba5dc` — the import gate, and a full re-audit of the class

- **Both previously-broken configurations verified by running them myself:**
  `cargo test -p aligned-vmem` (no `--features` at all) and
  `cargo test -p aligned-vmem --features mock` both compile and pass — full
  counts in §3.
- **The gate is minimal and correct.** `crates/vmem/tests/smoke.rs:4-9`:

  ```rust
  #[cfg(feature = "lazy-commit")]
  use aligned_vmem::{commit_range, try_commit_range};
  use aligned_vmem::{
      decommit_lazy, leak_zeroed_pages, page_size, recommit, release, reserve_aligned,
      try_reserve_aligned, Reservation, VmemError, PAGE,
  };
  ```

  Exactly the two `#[cfg(feature = "lazy-commit")]`-gated symbols were moved
  (`commit_range` at `crates/vmem/src/lib.rs:1374-1376`, `try_commit_range` at
  `:1389-1391`). **Nothing was over-gated:** I checked every one of the ten
  symbols left in the unconditional block against its definition — `release`
  `:1074`, `decommit_lazy` `:1245`, `recommit` `:1288`, `leak_zeroed_pages`
  `:1612`, `page_size`, `reserve_aligned`, `try_reserve_aligned`, `Reservation`,
  `VmemError`, `PAGE` — **none** carries a `#[cfg(feature = …)]` attribute. All
  ten are reachable under default features, which is why the default build now
  passes.
- **Full re-audit of the whole class across all eight test files** (asked for in
  point 5), each import checked against the symbol's own gate in `lib.rs`:

  | Test file | File-level gate | Imports | Verdict |
  |---|---|---|---|
  | `smoke.rs` | none | see above | **OK** (post-`53ba5dc`) |
  | `lazy_commit.rs:7` | `#![cfg(feature = "lazy-commit")]` | `commit_range`, `try_commit_range`, `reserve_aligned_lazy`, `reserve_aligned`, `PAGE` | **OK** — every gated symbol needs exactly `lazy-commit` |
  | `huge_pages.rs:34` | `#![cfg(feature = "huge-pages")]` | `reserve_aligned_huge` (ungated import, `:36`); `try_reserve_aligned_huge`/`VmemError`/`PAGE` behind `#[cfg(target_os = "linux")]` (`:37-38`) | **OK** — both huge fns are `huge-pages`-gated, satisfied by the file gate |
  | `fault_injection.rs:15-19` | `#![cfg(all(feature = "fault-injection", feature = "lazy-commit", not(feature = "mock")))]` | `fault_injection::{arm_fail_at, arm_fail_next}`, `commit_range`, `reserve_aligned_lazy`, `PAGE` | **OK** — `lazy-commit` is in the file gate, so the two `lazy-commit` symbols resolve |
  | `mock.rs:5` | `#![cfg(feature = "mock")]` | `mock::{self, Call}`, `decommit`, `decommit_lazy`, `page_size`, `recommit`, `reserve_aligned`, `try_reserve_aligned`, `PAGE` | **OK** — all seven crate fns ungated |
  | `readme_example.rs` | none | `release`, `reserve_aligned` | **OK** — both ungated |
  | `min_page.rs` | none | `MIN_PAGE`, `PAGE` | **OK** |
  | `vmemerror_io_bridge.rs` | none | `VmemError` | **OK** |

  **Null result: no remaining unconditional-import-of-a-gated-symbol exists in
  `crates/vmem/tests/`.** The one in-body `#[cfg(feature = "lazy-commit")]` in
  `smoke.rs` (`:391`) and the `#[cfg(all(unix, feature = "bench-internals",
  not(feature = "mock"), not(miri)))]` test at `:659` are both correctly gated,
  and `smoke.rs:375` reaches `try_recommit` by full path
  (`aligned_vmem::try_recommit`), needing no import.

---

## 3. Full regression sweep — actual results

Every command run on `x86_64-pc-windows-msvc` at `HEAD = b8966ba`, working tree
clean.

| Command | Result |
|---|---|
| `cargo test -p aligned-vmem` (**default features**) | **PASS** — `smoke` **30/30**, `vmemerror_io_bridge` **3/3**, `min_page` **2/2**, `readme_example` **1/1**, lib-unit 0, `huge_pages`/`lazy_commit`/`fault_injection`/`mock` compile to 0 tests (file-level `cfg`), doc-tests 0. **0 failed.** |
| `cargo test -p aligned-vmem --features mock` | **PASS** — `smoke` **30/30**, `mock` **10/10**, `vmemerror_io_bridge` **3/3**, `min_page` **2/2**, `readme_example` **1/1**. **0 failed.** |
| `cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals"` | **PASS** — `smoke` **30/30**, `lazy_commit` **11/11**, `fault_injection` **5/5**, `huge_pages` **2/2**, `vmemerror_io_bridge` **3/3**, `min_page` **2/2**, `readme_example` **1/1**, `mock` 0 (feature off). **0 failed.** |
| `cargo test -p aligned-vmem --all-features` | **PASS** — `smoke` **30/30**, `lazy_commit` **11/11**, `mock` **10/10**, `huge_pages` **2/2**, `vmemerror_io_bridge` **3/3**, `min_page` **2/2**, `readme_example` **1/1**, `fault_injection` **0** (correctly self-disabled by its `not(feature = "mock")` gate). **0 failed.** |
| `cargo clippy -p aligned-vmem --all-targets -- -D warnings` | **clean** |
| `cargo clippy -p aligned-vmem --all-targets --features "lazy-commit huge-pages fault-injection bench-internals" -- -D warnings` | **clean** |
| `cargo clippy -p aligned-vmem --all-targets --all-features -- -D warnings` | **clean** |
| `cargo clippy -p aligned-vmem --all-targets --features fault-injection -- -D warnings` | **clean** (the V-21 row) |
| `cargo clippy -p aligned-vmem --all-targets --features "fault-injection,lazy-commit" -- -D warnings` | **clean** |
| `cargo fmt -p aligned-vmem --check` | **clean** (exit 0, no output) |
| `cargo check -p aligned-vmem --target x86_64-unknown-linux-gnu --features "lazy-commit huge-pages fault-injection bench-internals" --tests` | **clean** |
| *(extra, per open item 51)* `cargo clippy -p aligned-vmem --target x86_64-unknown-linux-gnu --all-targets --all-features -- -D warnings` | **clean** |
| *(extra)* `node scripts/verify-commit-prefixes.mjs` | **PASS** (32 commits; 2 informational warnings, both correct — see §6) |
| *(extra)* conflict-marker grep over `crates/vmem/` + `docs/CORRECTNESS_OPEN_ITEMS.md` | **no residue** |

The CHANGELOG's own verification claims ("30/30 `smoke.rs`", "2/2
`huge_pages.rs`", "5/5 `fault_injection.rs`", "11/11 `lazy_commit.rs`", "1/1
`readme_example.rs`", default and `mock`-alone both green, five clippy rows
clean, `fmt` clean) are therefore **all independently reproduced**, with one
overclaim about what the Linux cross-compile proves (**R-11**).

---

## 4. Counterfactuals — the new tests genuinely bite

Three revert-and-observe cycles were performed; all reverts were undone and the
tree confirmed clean (§7).

### 4.1 C-4 — `from_raw_parts_rejects_base_below_reservation_immediately`

**Revert applied:** restored the pre-fix ordering in
`crates/vmem/src/lib.rs` — reintroduced `let offset = base_addr - res_addr;`
above the `assert!` and changed operand 9 back to `.checked_add(offset)`.

**Result: FAILS, for exactly the documented reason.**

```
test from_raw_parts_rejects_base_below_reservation_immediately - should panic ... FAILED
thread '…' panicked at crates\vmem\src\lib.rs:803:22:
attempt to subtract with overflow
note: panic did not contain expected string
      panic message: "attempt to subtract with overflow"
 expected substring: "base must be >= reservation"
test result: FAILED. 0 passed; 1 failed; …
```

That is precisely the C-4 defect: the generic overflow panic at the subtraction
line instead of the informative multi-clause assert. **Non-vacuous.**

### 4.2 C-5 — `recommit_rejects_misaligned_empty_range` (both halves)

**Revert A:** swapped the two guards in `try_recommit`
(`crates/vmem/src/lib.rs:1302-1307`) so the `start == end` early return runs
before the alignment check.

```
test recommit_rejects_misaligned_empty_range ... FAILED
thread '…' panicked at crates\vmem\tests\smoke.rs:378:18:
called `Result::unwrap_err()` on an `Ok` value: ()
```

`try_recommit(base, 5, 5)` returned `Ok(())` — the exact pre-V-19 behaviour.
**Non-vacuous.**

**Revert B** (run separately, with `try_recommit` restored): the same swap
applied to `try_commit_range` (`:1392-1397`), under `--features lazy-commit`.

```
test recommit_rejects_misaligned_empty_range ... FAILED
thread '…' panicked at crates\vmem\tests\smoke.rs:395:18:
called `Result::unwrap_err()` on an `Ok` value: ()
```

Failure moved to line 395 — the `try_commit_range` assertion — proving the
`lazy-commit`-gated half of the test is independently non-vacuous, not carried
by the first half. **This is stronger than the CHANGELOG claims** (which only
says the test was counterfactually verified once).

### 4.3 What the counterfactuals do *not* prove

The `(0, 0)` "well-formed no-op still succeeds" assertions in the same test
(`smoke.rs:384-387`, `:399-402`, `:407-410`) are positive-path coverage, not regression
guards for this round's change — a revert of the reorder leaves them passing.
That is fine and correctly framed by the test's own docstring, which claims only
to "pin that behavior change". Recorded for completeness, not filed.

---

## 5. What this round got wrong or introduced new

### R-1 — LOW — item 42 is simultaneously `[A] URGENT` and `**CLOSED**`

`docs/CORRECTNESS_OPEN_ITEMS.md:86-108` (active) vs `:2374-2380` (closure trail).

**Scenario.** `67f6236` correctly moved item 42 into `[A]` and gave it
`Status: UNRESOLVED and URGENT`. Its card cross-references the closure narrative
("*See 'Recently resolved' section item 3 for the prior deferral context*"), but
the **reverse link was never added**: `:2374` still opens
`3. **Deferred decision — aligned-vmem's mock Cargo-feature-unification hazard
…** — **CLOSED** (updated 2026-08-09, task #778/F5 …)` with no reopen marker,
and the section's own header (`:2186`) reads "Recently resolved (closure trail —
**do not re-list as open**)". A reader who reaches the item through the closure
trail — which is exactly the reading path the trail exists to serve — concludes
it is settled.

**Severity LOW:** it is a documentation-consistency defect in a
current-state-index that CLAUDE.md's R34-24 rule governs explicitly ("a closed /
null / rejected item must NOT look active … the round that closes it MUST update
the card"; the mirror case, an item that stops being closed, is the same
defect), but the active card is correct and the one-line fix is obvious.

**Fix:** add one line under `:2380` — "**RE-OPENED** 2026-08-14 (task #934/C-9)
— see `[A]` item 42; the deadline this deferral was conditioned on has fired."

### R-2 — LOW — the new items 52-54 cite line numbers stale by 9–59 lines, several made stale by this round's own edits

`docs/CORRECTNESS_OPEN_ITEMS.md:2164-2182`.

**Scenario.** Verified every `file:line` citation in the three new items against
`HEAD` and against `cc35f1a`:

| Cited | Claimed | Actual at `HEAD` | Actual at `cc35f1a` | Verdict |
|---|---|---|---|---|
| item 53: `granted_huge: false` | `lib.rs:824` | **`:833`** | `:824` | correct at base, **made stale by this round** (+9) |
| item 52: `MADV_FREE` | `lib.rs:2585` | **`:2615`** | `:2585` | correct at base, **made stale by this round** (+30) |
| item 52: `MADV_FREE_REUSABLE` | `lib.rs:2588` | **`:2618`** | `:2588` | same (+30) |
| item 52: `madv_free_advice` | `lib.rs:2304-2317` | **`:2448`** | `:2418` | stale **already at base** (−114) |
| item 52: `MAP_ANON` supported list | `lib.rs:2348-2362` | **`:2491`/`:2506`** | — | stale already at base |
| item 52: the `MADV_DONTNEED` doc | `lib.rs:2292-2298` | not there | — | stale already at base |
| item 52: constant block | `lib.rs:2576-2588` | **`:2610-2618`** | `:2580-2588` | (+30) |
| item 53: "documented decommit advice" | `lib.rs:1487-1495` | unrelated code (inside `try_reserve_aligned_lazy`) | — | wrong (see **R-3**) |
| item 54: deprecated `is_empty` | `lib.rs:538-540` | **`:551`** | `:550` | stale already at base (−12) |
| item 54: `release(null, …)` early return | `lib.rs:1015-1018` | **`:1074-1078`** | `:1060-1064` | stale already at base (−45) |
| item 54: `leak_zeroed_pages` test | `tests/smoke.rs:637` | **`:737-740`** | `:684-687` | stale already at base (−47) |
| item 54: `min_page_equals_page` | `tests/min_page.rs:8-10` | `:7-10` | — | **accurate** |
| item 54: `MIN_PAGE` | `lib.rs:160` | `:160` | `:160` | **accurate** |
| item 42: `mock` hazard block | `Cargo.toml:62-87` | `:55-87` (contains the cited sentence) | — | **acceptable** |

Two distinct failures: (a) citations inherited verbatim from the pre-release
review doc without re-checking (already wrong when written), and (b) citations
correct when written but invalidated by the *same round's* concurrent
`lib.rs` edits, which shifted everything after `:800` by +9 and everything after
`:1671` by +30 (the round's net `+30` lines).

**This is the reproduction of a failure the campaign already documented and
warned against, inside the very item this round re-opened.** The closure
narrative at `docs/CORRECTNESS_OPEN_ITEMS.md:2412-2416` says, verbatim:
"*(Cited by feature name, not line range — round-5 closing review QC6 found a
line-range citation into this exact block go stale within the same round that
wrote it.)*"

**Severity LOW:** every item is still findable by symbol name, and none of the
underlying observations is wrong; but this index is the artifact a fresh session
inherits with no other memory, and a wrong `file:line` costs that session real
time.

**Fix:** cite by symbol/function/constant name (the convention the same file
already adopted for the `mock` block), or re-derive the numbers as a final step
of the round, after all branches merge.

### R-3 — LOW — item 53 misdescribes its own mechanism (`decommit` vs `reserve_aligned_huge`, and "on Darwin")

`docs/CORRECTNESS_OPEN_ITEMS.md:2170-2174`.

**Scenario.** The item states: "*the documented decommit advice
(`crates/vmem/src/lib.rs:1487-1495`) says 'use `is_huge()` to detect' the
huge-page-incompatibility case **on Darwin** (where `decommit` silently fails to
release physical memory)*". Three things are wrong, checked against the code:

1. `lib.rs:1487-1495` is not documentation at all — it is executable code inside
   `try_reserve_aligned_lazy` (the `#[cfg(feature = "mock")] let raw = …`
   dispatch).
2. `decommit`'s **huge-page-incompatibility** paragraph (`lib.rs:1150-1159`) is
   about **Windows and Linux** — "*on both Windows and Linux, decommit does not
   work on huge-page reservations*" — not Darwin. `decommit`'s **Darwin**
   paragraph (`:1161-1176`) is about **ordinary (non-huge)** reservations and
   never mentions `is_huge()`. The item merges two unrelated documented gaps
   into one.
3. The phrase "*use `is_huge()` to detect*" does not appear in `decommit`'s docs
   at all; the nearest real text is in **`reserve_aligned_huge`**'s rustdoc
   (`lib.rs:1518-1520`): "*To detect whether huge pages were actually granted …
   use the returned `Reservation::is_huge` method.*" `decommit`'s own doc only
   refers to reservations "*returned by `reserve_aligned_huge` with
   `Reservation::is_huge == true`*" (`:1152`).

Additionally, the item's **Evidence** line cites pre-release-review finding
**V-9**, whereas the closing review that requested this indexing attributed the
sub-observation to **V-18** ("*V-18's new sub-observation*", C-9 bullet 3).

**The underlying hazard is real and worth tracking** — `from_raw_parts` hard-codes
`granted_huge: false` (`lib.rs:833`), so an adopted huge reservation reports
`is_huge() == false`, and a caller gating `decommit` on `is_huge()` fails open
into the Windows/Linux huge-page-incompatibility case. Only the write-up is
wrong.

**Severity LOW:** the item is tracked as INFO, not a live bug, so the
misdescription costs a future reader a re-derivation rather than a wrong action.

### R-4 — LOW — the C-2 module-comment fix traded an over-broad claim for an under-broad one

`crates/vmem/src/lib.rs:189-192`.

**Before** (the claim C-2 asked to fix): "*two syscalls (the traditional path
for larger alignments **or a partial initial commit**, over-reserving
`size + align` …)*" — entry conditions right, over-reserve claim wrong.

**After** (what shipped): "*two syscalls (the traditional path for alignments
> 64 KiB **or when the fast-reserve sub-path's alignment check misses**,
over-reserving `size + align` …)*" — over-reserve claim now right, **entry
conditions now wrong**.

**Concrete scenario.** `reserve_aligned_lazy(64 KiB, PAGE, PAGE)` on Windows —
i.e. the ordinary `lazy-commit` case, and the most common one:

- `align = 4 KiB <= WIN_ALLOCATION_GRANULARITY` and `commit_len = 4 KiB != size
  = 64 KiB`, so the single-call guard at `crates/vmem/src/lib.rs:1713` is
  **false** and the two-syscall path is taken;
- inside it, `align <= WIN_ALLOCATION_GRANULARITY` so the fast-reserve sub-path
  runs, and its check at `:1809` (`candidate_ptr.addr().is_multiple_of(align)`)
  **hits** (a `VirtualAlloc(NULL, …)` base is 64 KiB-granular, hence a multiple
  of 4 KiB) — so the alignment check does *not* miss.

The case therefore matches **neither** of the comment's two stated conditions,
yet it demonstrably takes the two-syscall path. The real entry condition
(`commit_len != size`) was deleted outright, and an *internal sub-branch* of the
two-call path was promoted into the position of an entry condition — two
different levels of the control flow conflated in one sentence.

`win_reserve_commit`'s own header (C-2's site 3, `:1662-1663`) gets this right by not
re-enumerating: "*Two-call path (**all other cases**)*".

**Severity LOW:** an internal `//` design comment, not public rustdoc, and no
behavioural consequence — but the sentence is now wrong for the most common
`lazy-commit` call shape, and it is the crate's own top-of-file orientation
comment.

**Fix:** "*or two syscalls (all other cases: `align > 64 KiB`, or a partial
initial commit — over-reserving `size + align` only when `align > 64 KiB` or the
fast-reserve sub-path's alignment check misses …)*".

### R-5 — LOW — the C-6 rewrite deleted two documented `reservation_len` invariants that the code still enforces

`crates/vmem/src/lib.rs:731-738` vs. the pre-round `:726-728`.

**Scenario.** The replaced bullet said three things:

```
- `reservation_len` is the value this crate itself would report via
  `Reservation::reservation_len` for an equivalent reservation,
  a non-zero multiple of `PAGE`,
  `reservation_len >= len + (base - reservation)`.
```

C-6 asked for the first clause to be split per platform. The rewrite did that
correctly — and dropped clauses 2 and 3 entirely. They appear nowhere else in
`from_raw_parts`'s `# Safety` section (`:720-748`), whose `reservation_len`
bullet is now the only mention of the parameter.

Both clauses are still **enforced at runtime** by the `assert!` (`:806-807`
`reservation_len != 0 && reservation_len.is_multiple_of(PAGE)`; `:812-814` the
`len + offset` bound), and each has a dedicated test
(`from_raw_parts_rejects_zero_reservation_len_immediately`,
`…_non_page_multiple_reservation_len_immediately`,
`…_insufficient_reservation_len_immediately`). The result is a
docs-behind-code inversion in the direction this crate's whole review history is
about:

- the assert's own panic string (`:818` and `:822`) names clauses the public docs no
  longer state: "*reservation_len must be non-zero and a multiple of PAGE; … ;
  reservation_len must be >= len + (base - reservation)*";
- the in-source comment at `:784-786` still says the checks "*enforce the
  documented nonzero/page-multiple invariants*" — documentation that has just
  been removed;
- a caller who satisfies the surviving text ("*the full length of the underlying
  OS mapping*") will in practice satisfy both dropped clauses, so no correct
  caller is broken — but an *incorrect* caller now gets a panic naming a rule
  the docs never gave them.

The prior review explicitly assumed these would survive: C-6's own text says
"*the numeric constraint `reservation_len >= len + (base - reservation)`
immediately after it is unchanged and still enforced*".

**Severity LOW:** no soundness or behavioural change; a public-`# Safety`
completeness regression on an `unsafe fn`, which is a higher-than-usual bar for
"just docs".

**Fix:** append to the bullet — "*It must in all cases be a non-zero multiple of
`PAGE` with `reservation_len >= len + (base - reservation)`; both are asserted
at construction.*"

### R-6 — LOW — the new C-4 test uses out-of-bounds pointer arithmetic where its own sibling test deliberately does not

`crates/vmem/tests/smoke.rs:1015`:

```rust
Reservation::from_raw_parts(raw.sub(1), PAGE, raw, raw_len, align)
```

**Scenario.** `raw` is the `*mut u8` reservation base returned by
`into_parts()`. `<*mut T>::sub`'s documented safety contract requires the
resulting pointer to be in bounds of (or one past the end of) the same allocated
object; one byte **before** the base is neither, so `raw.sub(1)` is
library-level UB and a hard error under Miri's strict provenance
("out-of-bounds pointer arithmetic").

What makes this a genuine finding rather than pedantry is that **the immediately
preceding test in the same file already solved this exact problem and documented
why**, at `crates/vmem/tests/smoke.rs:918-932`:

```rust
/// The misaligned `base` is constructed via `.wrapping_add(1)`,
/// which is safe because the pointer is never dereferenced …
let misaligned_base = raw.wrapping_add(1);
```

The new test even handles Miri elsewhere in its own body ("*Release the
reservation to avoid leaking under miri*", `:1019-1022`), so the author was
thinking about Miri and still used the unsound form. The crate's published
`description` (`crates/vmem/Cargo.toml:7`) advertises "miri-friendly".

**Not currently caught by anything:** CI's only Miri touch for this crate is
`RUSTFLAGS="--cfg miri" cargo check -p aligned-vmem --all-features`
(`.github/workflows/ci.yml:171`), a compile-only gate — there is no
`cargo miri test -p aligned-vmem` job, which is this index's own **open item 41**
(`docs/CORRECTNESS_OPEN_ITEMS.md:1844`). So this is latent today and becomes a
failure the moment item 41 is closed.

**Severity LOW:** test-only, not shipping code, and unobservable under the
current CI matrix — but it is a self-inflicted regression against a convention
the same file states explicitly 80 lines earlier, and it is pre-loaded to break
the exact CI job the project has already committed to adding.

**Fix (one word):** `raw.wrapping_sub(1)`.

### R-7 — INFO — the C-4 test can only fail in an `overflow-checks` profile

`crates/vmem/tests/smoke.rs:1005-1024`.

The workspace `[profile.release]` (`Cargo.toml:2384-2386`) sets only
`lto`/`codegen-units`; `overflow-checks` is therefore off in release. With the
fix reverted, a release build would *wrap* `base_addr - res_addr` rather than
panic, the `&&` chain would then short-circuit on `base_addr >= res_addr`, and
the assert would emit exactly the message the test asserts — so the test would
**pass against the unfixed code** in `--release`.

Reasoned, not executed: a concurrent process in this shared workspace held the
Cargo artifact lock for the duration of this review and the release run could
not complete. The prior round's review measured precisely this release
behaviour, independently (its §9, C-4: "*in `--release`: the subtraction wraps …
and the intended `base must be >= reservation` message is produced*").

**Severity INFO:** CI and `npm run check` run `cargo test` in the debug profile,
where the guard is fully effective; this only bounds the claim, it does not
weaken the shipped guard. Worth one sentence in the test's docstring (which
already says "*in debug builds (`overflow-checks = true`)*" — so the docstring
is, notably, already accurate; only the CHANGELOG's unqualified "*personally
counterfactually verified*" omits the profile condition).

### R-8 — INFO — C-7 was dropped with no task, no CHANGELOG line, and no index entry

`crates/vmem/src/lib.rs:1886-1900`; `CHANGELOG.md` §"round 11 closing-review fix
pass"; `docs/CORRECTNESS_OPEN_ITEMS.md`.

**Scenario.** The closing review raised ten findings, C-1 … C-10. The fix pass
maps seven tasks onto **nine** of them (C-1/C-3 → #928; C-2/C-3b/C-4/C-6 → #929;
C-5 → #930; C-2 → #931; C-10/C-2 → #932; C-8 → #933; C-9 → #934). **C-7 appears
nowhere**: not in the CHANGELOG's task bullets, not in its summary paragraphs,
not as a new open-item, and the code is unchanged (`extra_commit_flags != 0`
retry at `:1888-1892`, still byte-identical to the commit that just failed at
`:1884-1886`).

Leaving C-7 unfixed is the **right call** — the prior review's own §8 rates it
harmless, one wasted `VirtualAlloc` on a genuine-OOM path that also requested
huge pages with `align > 64 KiB`, and says "*touching `win_reserve_commit` again
purely for tidiness carries more review cost than the saving is worth*". The
defect is that the decision is nowhere recorded. Under CLAUDE.md's round
convention every review-flagged item must be **closed, deferred with a written
reason, or indexed**; C-7 is none of the three.

**Severity INFO:** process only; the code is fine either way. It is worth filing
because this round's own task #934 (C-9) existed specifically to stop
review-flagged items from living only inside a review document — and C-7 is now
exactly that.

**Fix:** one line in the CHANGELOG ("C-7 deliberately not actioned — see the
closing review §8") or a fourth new item (55) in
`docs/CORRECTNESS_OPEN_ITEMS.md` alongside 52-54.

### R-9 — INFO — the item-42 tier fix was not applied to the two other items with the identical mismatch

`docs/CORRECTNESS_OPEN_ITEMS.md:1000` (item 11) and `:1149` (item 13).

Both are labelled `**[A, filed 2026-08-02, task #498] …**` inline while sitting
physically **below** the `### [T] Tracked, not yet actioned` header (`:110`) —
the exact defect `67f6236` fixed for item 42, with the same consequence in the
same direction (an item claiming active status parked in a tier implying no
imminent action).

**Severity INFO:** pre-existing (filed 2026-08-02, twelve days before this
round), so not a regression — recorded because the round's own commit message
articulated the general rule while fixing only the one instance in front of it,
and a reader of that commit could reasonably conclude the file is now
internally consistent. It is not.

### R-10 — INFO — "item 42" is now ambiguous across three places in the file

`docs/CORRECTNESS_OPEN_ITEMS.md:86`, `:2374`, `:3355`.

Three different anchors now answer to "42" or to item 42's content:

1. `:86` — item **42**, active `[A]`, `aligned-vmem`'s `mock` feature-unification
   hazard (this round's move);
2. `:2374` — the closure narrative for that same item, numbered **3** in the
   resolved trail's own sequence;
3. `:3355` — a **different** item, also numbered **42**, `sefer-region`'s
   packaged benchmark writing outside its package root — which is itself marked
   `— **OPEN**` while sitting inside the section headed "*do not re-list as
   open*".

**Severity INFO:** the collision and the mis-filed `OPEN` at `:3355` both
pre-date this round. Recorded because promoting item 42 to the `[A]` tier makes
"item 42" a phrase a future session is now much more likely to grep for, and it
resolves to three different things.

### R-11 — INFO — the CHANGELOG overstates what the Linux cross-compile proves

`CHANGELOG.md`, round-11-closing-review-fix-pass verification paragraph: "*Linux
cross-compile (`x86_64-unknown-linux-gnu --tests`) confirms the V-25 test is
correctly excluded there*".

A `cargo check --tests` cannot distinguish "the test is `cfg`'d out" from "the
test compiles and would panic at runtime". C-1 was a **runtime** panic — the
pre-fix test compiled cleanly on Linux, which is precisely why no cross-compile
gate caught it (the prior review said so explicitly: "*Linux compiles clean; it
is the runtime assertion in the new test that fails there, which no
cross-compile check can catch*"). The cross-compile proves the *fix did not
break the Linux build*, which is valuable and is what it should claim.

The exclusion itself is guaranteed by `#[cfg(windows)]` as a language property —
that is the real evidence, and it is stronger than the cited one.

**Severity INFO:** the conclusion is true; only the stated evidence does not
support it. Filed because this campaign's standing practice is that an
evidence-claim mismatch is itself a finding regardless of whether the conclusion
happens to hold.

### R-12 — INFO — C-10's new comment contradicts an existing comment six lines below it

`crates/vmem/src/lib.rs:2341-2345` vs `:2351-2354`.

The new C-10 comment ends: "*The whole function returns early if `mmap` fails,
so reaching this line means huge pages were actually granted.*" — stated
unconditionally.

Six lines later, the pre-existing comment on the return value says: "*On
non-Linux Unix the `huge` flag is silently ignored, so we report false. This is
correct because `MAP_HUGETLB` fails the WHOLE `mmap` call when 2 MiB hugetlb
pages are unavailable…*" — and the return is
`Ok((base, base, size, HUGE_SUPPORTED && huge))`, i.e. the code itself refuses to
claim a grant off Linux.

So on macOS/FreeBSD/etc. the new sentence is false: `huge` was ignored, no huge
pages were granted, and the very next comment says so. It is harmless in effect
(`libc_madvise_hugepage` is an explicit empty no-op outside `target_os = "linux"`,
`:2822-2826`), and the argument the comment makes *is* sound for Linux, which is
the only platform where the hint does anything.

**Severity INFO:** comment-only, self-contained within one function, no
behavioural consequence. Filed because C-10's entire purpose was to remove a
"two sibling sites spell the same concept two different ways" confusion, and the
fix introduced a fresh contradiction at six lines' distance.

**Fix:** scope the sentence — "*…so on Linux reaching this line means
`MAP_HUGETLB` was granted; off Linux the flag was a no-op and this hint is
itself compiled out.*"

---

## 6. Explicit null results

Stated affirmatively, per this project's convention that a closing review
finding nothing on a given axis is a useful result:

- **C-1 is genuinely closed**, verified by independently tracing
  `unix_reserve`'s huge-page guard rather than trusting the fix comment (§1.1),
  including the two collateral risks (unused import on non-Windows; macOS
  coverage) — both clean.
- **All three human-caught issues (`67f6236`, `1d2e821`, `53ba5dc`) are fixed
  correctly and completely** (§2), including a clause-by-clause short-circuit
  trace of the `assert!` (§2.2) and a full symbol-by-symbol re-audit of every
  `use aligned_vmem::{…}` block in all eight test files against each symbol's own
  `#[cfg]` gate (§2.3). **No remaining unconditional import of a feature-gated
  symbol exists anywhere in `crates/vmem/tests/`.**
- **Nothing was over-gated by `53ba5dc`**: all ten symbols left in `smoke.rs`'s
  unconditional `use` block were individually checked and none carries a
  `#[cfg(feature = …)]` attribute.
- **No soundness, UB, leak, double-free, or memory-safety defect was introduced
  in shipping code.** This is unusually easy to assert this round: the entire
  non-comment `src/` delta is three lines (executive summary), and both the
  before and after forms compute the same value on every in-contract input,
  differing only in *where* an out-of-contract input panics.
- **No `unsafe` block, FFI call site, or `#[cfg]` gate in `src/` was touched.**
- **All eleven required gate invocations plus two extras are green** (§3), with
  exact counts, on default / `mock` / the named combo / `--all-features`, five
  clippy rows, `fmt`, and the Linux cross-compile.
- **Both new tests are non-vacuous**, and the C-5 test is non-vacuous in *both*
  of its two independent halves (§4).
- **No conflict-marker residue** anywhere in `crates/vmem/` or
  `docs/CORRECTNESS_OPEN_ITEMS.md`.
- **`node scripts/verify-commit-prefixes.mjs` PASSES** on all 32 commits in the
  unpushed range. Its two "direction 2 — hidden runtime change?" warnings
  (`f9e4545` and `90379b0`, both `docs(vmem):` prefixes touching
  `crates/vmem/src/lib.rs`) were checked individually and are **false
  positives**: extracting the non-comment changed lines from each yields **zero**
  lines for both, and for `c7ec951` (C-8) as well. All three are genuinely
  comment-only, so the `docs(…)` prefix is the correct R30-12 taxonomy slot.
- **The CHANGELOG's numerical verification claims are all independently
  reproduced** (§3) — 30/30, 2/2, 5/5, 11/11, 1/1, default+`mock` green, five
  clippy rows, `fmt` clean. Only the Linux-cross-compile evidence claim is
  overstated (R-11).
- **`win_reserve_commit`'s rewritten doc header (C-2 site 3) is accurate in
  every one of its six checkable claims** (§1.2) — the most substantial doc
  rewrite of the round, and the cleanest.
- **C-2 sites 2 and 4, and both C-3b sites, introduce no new inaccuracy**
  (§1.2, §1.4).

---

## 7. Read-only compliance

Three temporary source edits were made for the §4 counterfactuals, all to
`crates/vmem/src/lib.rs`, each reverted via `git checkout -- crates/vmem/src/lib.rs`
immediately after its measurement:

1. C-4 ordering revert (`let offset = …` reintroduced above the `assert!`);
2. `try_recommit` guard swap;
3. `try_commit_range` guard swap.

**Final state verified:**

- `git diff --stat` → **empty**;
- `git status --porcelain` → only `?? docs/checkpoints/2026-08-13-2100.md`
  (pre-existing, untracked, not created by this review) and
  `?? docs/reviews/2026-08-14-aligned-vmem-round11cr-closing-review.md` (this
  report, deliberately left untracked);
- `git rev-parse HEAD` → `b8966ba24601781964a84a3356c250583d140a84`, unchanged;
- and, after the last revert, `cargo test -p aligned-vmem --features lazy-commit
  --test smoke` re-run to confirm the restored tree is green again:
  **30 passed; 0 failed**.

No `git add`, `git commit`, `git push`, branch creation, worktree mutation, or
version bump was performed.

**One environmental note affecting §3 and §4:** this workspace was shared with
at least one concurrent build process throughout the review (observed via a
long-lived multi-GB `rustc` and repeated `Blocking waiting for file lock on
build directory` / `on artifact directory` messages). Every result reported in
§3 and §4 was obtained cleanly and is reproducible; the only measurement this
cost was the `--release`-profile variant of the C-4 counterfactual, which is
reported as reasoned-not-executed in **R-7** with that limitation stated
explicitly rather than papered over.

---

## Findings index

| ID | Sev | Area | File:line | One line |
|---|---|---|---|---|
| R-1 | LOW | Index | `docs/CORRECTNESS_OPEN_ITEMS.md:86` vs `:2374` | Item 42 is `[A] URGENT` in the open tier and still `**CLOSED**` in the "do not re-list as open" closure trail; only the forward cross-reference was added |
| R-2 | LOW | Index | `docs/CORRECTNESS_OPEN_ITEMS.md:2164-2182` | New items 52-54 cite `lib.rs` lines stale by 9–59; several were made stale by this round's own +30-line edits, against a warning written in the very narrative this round re-opened |
| R-3 | LOW | Index | `docs/CORRECTNESS_OPEN_ITEMS.md:2170-2174` | Item 53 cites executable code as documentation, attributes a Windows/Linux huge-page gap to Darwin, quotes `reserve_aligned_huge`'s rustdoc as `decommit`'s, and cites V-9 where the review said V-18 |
| R-4 | LOW | Docs | `crates/vmem/src/lib.rs:189-192` | The C-2 module-comment fix deleted the real second entry condition (`commit_len != size`) and promoted an internal sub-branch in its place; `reserve_aligned_lazy(64 KiB, PAGE, PAGE)` now matches neither stated condition |
| R-5 | LOW | API docs | `crates/vmem/src/lib.rs:731-738` | The C-6 rewrite dropped "non-zero multiple of `PAGE`" and "`reservation_len >= len + (base - reservation)`" from an `unsafe fn`'s `# Safety` section while the `assert!` still enforces and names both |
| R-6 | LOW | Tests | `crates/vmem/tests/smoke.rs:1015` | The new C-4 test uses `raw.sub(1)` (out-of-bounds pointer arithmetic, Miri UB) where the sibling test 80 lines earlier deliberately uses `wrapping_add(1)` and documents why |
| R-7 | INFO | Tests | `crates/vmem/tests/smoke.rs:1005-1024` | The C-4 guard can only fail with `overflow-checks` on; in `--release` it would pass against the unfixed code |
| R-8 | INFO | Process | `crates/vmem/src/lib.rs:1886-1900`; `CHANGELOG.md` | C-7 was silently not actioned — no task, no CHANGELOG line, no index entry; leaving it is correct, not recording it is not |
| R-9 | INFO | Index | `docs/CORRECTNESS_OPEN_ITEMS.md:1000`, `:1149` | Items 11 and 13 carry the identical `[A]`-label-inside-`[T]`-section mismatch `67f6236` fixed for item 42 (pre-existing) |
| R-10 | INFO | Index | `docs/CORRECTNESS_OPEN_ITEMS.md:86`, `:2374`, `:3355` | "Item 42" now resolves to three different anchors, one of which is a different item marked `OPEN` inside the closure trail (pre-existing) |
| R-11 | INFO | Docs | `CHANGELOG.md` (fix-pass verification paragraph) | "Linux cross-compile confirms the V-25 test is correctly excluded" — a `cargo check` cannot show that; `#[cfg(windows)]` does |
| R-12 | INFO | Docs | `crates/vmem/src/lib.rs:2341-2345` vs `:2351-2354` | C-10's new comment claims unconditionally that reaching the hint implies a huge-page grant; the comment six lines below says the flag is silently ignored off Linux |

**Severity legend used:** LOW = a real defect with no behavioural or build
consequence today, costing a future reader accuracy or time; INFO = a process,
framing, or pre-existing observation recorded so it is not rediscovered.
No CRITICAL, HIGH, or MEDIUM finding was identified.
