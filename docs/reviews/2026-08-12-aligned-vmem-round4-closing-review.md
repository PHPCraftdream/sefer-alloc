# `aligned-vmem` — round-4 CLOSING review (verification of the R1–R13 remediation)

**Date:** 2026-08-13 (the round-4 review this closes is dated 2026-08-12; the eight remediation
commits are timestamped 2026-08-13 00:24–01:04 CEST. The filename keeps the campaign's
`2026-08-12-` prefix so it sorts with its siblings.)
**Scope:** verification of the eight tasks (#867–#874, letters A–H) that remediated
`docs/reviews/2026-08-12-aligned-vmem-round4-review.md`'s findings R1–R13 — every touched file in
the round's diff (`git diff 8804fc9..HEAD`: 12 files, +673/−32), plus the code each of those
changes makes a claim about.
**Reviewed tree:** local `main` @ `a59569d72d918f409639884dd1fb02fa11b546dc`.
`git status --short` at session start showed exactly one entry: the untracked round-4 review
document itself (`?? docs/reviews/2026-08-12-aligned-vmem-round4-review.md`) — see CR10 note.
`origin/main` = `8804fc91c1c0019c63afa605e9729a2f2475f576`, i.e. **all sixteen of this round's
commits are unpushed and have never run in CI.**
**Toolchain:** `cargo`/`rustc` stable as installed on this host; Windows 10 Pro x86_64, 4 KiB page.
**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add` / `git commit` / `git push` / version bump. Every command quoted below was
actually run on this host; every `file:line` citation was read in the current tree before being
written down.

**Two executed experiments, both run OUTSIDE the repository.** CR2's counterfactuals required
editing `crates/vmem/src/lib.rs`. That was done on a throwaway copy under `%TEMP%`
(`$TEMP/vmem_guard_cf`, holding only `scripts/lib.mjs`, `scripts/vmem-doc-drift-guard.mjs` and
`crates/vmem/src/lib.rs` — the guard resolves `REPO_ROOT` from its own file location
(`scripts/lib.mjs:14`), so a copied tree is a faithful sandbox). The repository working tree was
never touched; the temp directory was deleted afterwards. Everything reported from those
experiments is a real observed exit code, not a prediction.

**Relationship to the prior four rounds.** This pass does not re-report V1–V21, W1–W16 + P-A/P-B/P-C,
F1–F11, or R1–R13. It verifies R1–R13's remediation and reports only what is new. To stay
unambiguous against the `V`-, `W`-, `F`- and `R`-series, this pass's own findings are numbered
**CR1…CR10** (closing-review series).

---

## Verdict up front

**Eleven of the thirteen findings are genuinely, verifiably closed. Two — R1 and R6 — landed fixes
whose *mechanism* does not do what the fix's own commit message says it does, and I confirmed both
by execution rather than by reading.**

* **R1's CI fix silently DELETED the `--all-features` row it was supposed to supplement**, on both
  `test-windows` and `test-macos`, leaving behind a comment block that still describes the deleted
  step ("The step below … runs with `--all-features` … This is the only row that runs
  `tests/mock.rs`"). The new everything-except-`mock` row is real and does exactly what R1 wanted —
  I ran it here and it takes `fault_injection.rs` from 0 → 5 tests — but the round also *removed*
  `tests/mock.rs` from both platform jobs entirely, which un-covers round-3 F1's own fix on the one
  runner (macOS/arm64, 16 KiB page) that F1 existed for. **CR1.**
* **R6's new `scripts/vmem-doc-drift-guard.mjs` cannot catch either of the two real historical
  drifts it was written to prevent.** Its commit message claims it was "verified against an injected
  counterfactual that the guard now fails on the historical phrasing". I re-ran that experiment
  three ways on a scratch copy: the guard fails only on a synthetic sentence stripped of every
  qualifier word; it **passes clean** on the verbatim round-3 F4 sentence and on the verbatim
  round-4 R6 sentence, because both contain "align" and "Windows", which its own OR-qualifier
  accepts. The `\bconditional\b` word-boundary fix the round's zero-trust review added is correct
  as far as it goes and I confirmed it holds — it is simply not the load-bearing part of the
  predicate. **CR2.**

**Three further findings are the round's own new residue — the same "the fix introduces the next
round's finding" pattern that produced R3 this round and F3 last round, now at its third
consecutive occurrence:** task #873 (R9) *reverted* task #859's round-3 F3 correction and restored a
`Cargo.toml` claim that is false about this crate (**CR3**); task #870 (R3) fixed two of the three
accessors carrying the identical half-condition defect and left the third (**CR5**); task #871 (R7)
fixed one paragraph of a doc comment and left the deleted-code description two paragraphs above it
in the same comment (**CR6**).

**One finding is in the fabrication-check category the prompt asked me to run, and it is real,
though it is a mis-citation rather than an invention:** the brand-new
`docs/CORRECTNESS_OPEN_ITEMS.md` card — whose entire purpose is durable cross-session institutional
memory — attributes the closure of the round-2 CHANGELOG gap to commit `7663811`. `7663811` closed
the **round-1** gap, predates the round-3 review the card says caught it, and is the campaign's
**fabrication-incident commit**. The card's own body says the correct thing (task #863) two bullets
earlier. **CR4.**

**Everything else is small and correct.** R2, R4, R5, R8, R10, R11, R12, R13 all landed cleanly and
survived adversarial re-checking, including independent SHA verification of all seven citations in
the new CHANGELOG entry and independent verification of every measured figure in the new design note
against the R32-13 report it cites.

**Publish posture (task #658).** **R5 is resolved** — `Clone` is gone from `ReservationParts`
(`crates/vmem/src/lib.rs:733`) and a whole-repository grep finds no `.clone()` on a
`ReservationParts` value anywhere, so the one item round 4 called a hard publish gate is closed
while it is still free to close. Two *new* items should be settled before `cargo publish`: **CR1**
(publishing a crate whose Windows/macOS jobs no longer run its own mock suite, with CI comments that
misdescribe what runs, is a worse posture than round 4 started with) and **CR9** (the `mock`
feature-unification hazard's documented deferral argument rests on "this crate has not yet had its
first `crates.io` publish", which is false — 0.1.0 is already published, and it cites the wrong task
number). Neither is a soundness blocker.

---

## What was verified green — every command below was executed on this host

| command | result |
|---|---|
| `cargo test -p aligned-vmem --all-features` | **green**, exit 0 — lib 0, `fault_injection` **0**, `huge_pages` 1, `lazy_commit` 11, `min_page` 2, `mock` **9**, `smoke` **19**, `vmemerror_io_bridge` 3, doctests 0; 0 failed |
| `cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast` | **green**, exit 0 — lib 0, `fault_injection` **5**, `huge_pages` 1, `lazy_commit` 11, `min_page` 2, `mock` **0**, `smoke` **19**, `vmemerror_io_bridge` 3, doctests 0; 0 failed |
| `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` | **green**, exit 0 |
| `cargo clippy -p aligned-vmem --all-targets -- -D warnings` (default row) | **green**, exit 0 |
| `cargo fmt --check -p aligned-vmem` | **green**, no output, exit 0 |
| `cargo doc -p aligned-vmem --all-features --no-deps` | **green**, zero warnings |
| `cargo build -p sefer-alloc --features bench-internals` | **green** |
| `cargo clippy -p sefer-alloc --features bench-internals` | **green**, zero warnings |
| `cargo doc -p sefer-alloc --features "bench-internals internals" --no-deps` | **green**, zero warnings (checks that R3's edited intra-doc links in `alloc_core_core_diag.rs` still resolve) |
| `node scripts/vmem-doc-drift-guard.mjs` | **OK**, exit 0 — but see CR2 for what that "OK" is worth |
| `python -c "yaml.safe_load(open('.github/workflows/ci.yml'))"` + step enumeration | parses; enumerated steps are the evidence for CR1 |

**Read the two test rows against each other — that pair IS the R1/R2 close.** `fault_injection`
goes 0 → 5 and `mock` goes 9 → 0 between the two feature sets, exactly as R1 predicted, on real
Windows. That is the executed proof that the new CI row reaches the real backend and that
`tests/mock.rs` is correctly `cfg`-excluded from it. It is also the executed proof of CR1's other
half: nine tests exist that the new row does not run, and no Windows or macOS row runs them now.

A caveat on the clippy rows for honesty: both completed in under a second, i.e. served from
`cargo`'s cache against this unmodified tree. The tree they were computed against is the tree the
test rows above compiled from scratch, so the result is sound, but they were not full recompiles.

---

# Findings — new this pass

## CR1 — MEDIUM-HIGH — R1's fix **replaced** the `--all-features` row instead of supplementing it, on both platform jobs, and left behind a comment block describing the step it deleted; `tests/mock.rs` now runs on no Windows and no macOS job, which un-covers round-3 F1's own fix on the exact runner F1 was written for

`.github/workflows/ci.yml:774-784` (`test-windows`), `:806-816` (`test-macos`), against commit
`e0d921a`'s diff and against R1's own prescription
(`docs/reviews/2026-08-12-aligned-vmem-round4-review.md:188-196`).

**What R1 asked for, verbatim:** *"Replace, **or supplement**, the two platform rows with the
everything-except-`mock` feature set … **Keeping the existing `--all-features` row alongside it is
still worthwhile — it is the only row that runs `tests/mock.rs`** — but it should not be described
anywhere as platform-backend coverage."*

**What landed.** `e0d921a` deleted `- run: cargo test -p aligned-vmem --all-features --no-fail-fast`
from both jobs and added the new row, then added a *second* comment block introducing an
`--all-features` step that was never written. The current `test-windows` tail reads:

```yaml
      # Task #858/F2: aligned-vmem crate Windows test coverage for the REAL
      # backend (VirtualAlloc/MEM_COMMIT/MEM_DECOMMIT) on real Windows hardware.
      # NOTE: this uses an explicit feature list that EXCLUDES `mock` … The step
      # below (also from F2) runs with `--all-features` for the mock's own
      # coverage, but it is NOT a Windows-backend test.
      - run: cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast
      # Task #858/F2: aligned-vmem crate mock coverage. Tests the mock recording
      # backend itself (NOT the real VirtualAlloc/MEM_DECOMMIT backend) with all
      # optional features enabled. This is the only row that runs tests/mock.rs.
```

The job ends there. "The step below" does not exist. "This is the only row that runs
`tests/mock.rs`" is a comment attached to nothing. `test-macos:806-816` is the identical shape.

**Verified, not read:** parsing `ci.yml` with `yaml.safe_load` and enumerating
`jobs['test-windows']['steps']` / `jobs['test-macos']['steps']` returns, for each job, exactly one
`-p aligned-vmem` invocation — the new feature-set row. No `--all-features` step is present in
either job. (The file is still valid YAML; a trailing comment after the last list item is legal, so
nothing breaks loudly. That is precisely the problem.)

**Concrete cost, three parts.**

1. **`tests/mock.rs`'s 9 tests now run on ubuntu only.** The complete set of `cargo test -p
   aligned-vmem` rows is `:160` (`aligned-vmem-gates`, ubuntu, `--all-features`), `:781` (windows,
   no `mock`), `:813` (macos, no `mock`), `:847` (`test-workspace`, ubuntu, default features →
   `mock` off → the file compiles to 0 tests), `:889` (`test-workspace`, ubuntu, `--all-features`),
   `:909` (`test-workspace`, ubuntu, `fault-injection lazy-commit`). Two ubuntu rows, nothing else.
2. **This directly un-covers round-3 F1.** F1 (task #858, commit `75bba05`) changed
   `crates/vmem/tests/mock.rs:35` from `start == PAGE` to `start == page_size()` **because the
   `--all-features` macOS row task #856 added was going to red on it** — GitHub's `macos-latest` is
   Apple Silicon, `page_size() == 16384`. The round-3 CHANGELOG entry written *in this same round*
   (task #872) records that as "confirmed as the actual cause of the `test macos (production)` CI
   job failure". That macOS row is now gone, so the page-size-dependence class F1 closed has no
   16 KiB-page CI coverage again. Round 4 fixed a Windows/Darwin backend-coverage hole by opening a
   page-size-portability hole one round after the latter was closed by a red CI run.
3. **The comments now actively mislead.** A reader auditing platform coverage reads "This is the
   only row that runs `tests/mock.rs`" inside `test-windows` and concludes Windows runs it. This is
   the same class of defect as the one R1 itself was filed against (`ci.yml:799-802`'s old comment
   claiming the macOS row exercised `madvise`), reintroduced by R1's own fix.

**Failure scenario, concrete.** Someone changes `mock::Call::DecommitLazy`'s recorded `start`
semantics, or reintroduces a hardcoded `PAGE` in a `mock.rs` assertion. Both ubuntu rows are 4 KiB
hosts and stay green; `test-macos` no longer compiles the file; `test-windows` no longer compiles
the file. The failure surfaces on a user's arm64 Mac, not in CI — which is the exact scenario the
macOS row was added (task #856/W14-2) to prevent.

**Fix.** Restore the deleted step under its already-written comment in both jobs:
`- run: cargo test -p aligned-vmem --all-features --no-fail-fast`. This is a two-line change and it
makes the existing comments true instead of false. Nothing else about R1's fix needs touching — the
new row is correct and I verified it works.

## CR2 — MEDIUM — the new doc-drift guard passes clean on **both** real historical drift sentences it was written to catch; its commit message's counterfactual claim holds only for a synthetic sentence with every qualifier word removed

`scripts/vmem-doc-drift-guard.mjs:85-106` (the predicate), `:16-28` (its own KNOWN LIMITATION
header), `scripts/check-all.mjs:236-245` (the wiring), against commit `1519e0c`'s message.

**The predicate.** `hasTrigger = /over-reserv|trim/`; `hasQualifier = /align|\bconditional\b|Windows/`;
a doc block is a violation iff `hasTrigger && !hasQualifier`. Blocks are formed by joining an
entire contiguous run of `///` / `//!` lines.

**The `\bconditional\b` fix is real and it holds.** In `unconditionally`, the substring
`conditional` is preceded by `n` and followed by `l`, so neither `\b` can match — the anchored form
correctly refuses to treat "unconditionally" as a qualifier. I confirmed that by inspection and it
is not the problem.

**The problem is that the qualifier is an OR, and every real drift sentence contains "align" and
"Windows" anyway.** Three executed counterfactuals on the scratch copy:

| # | injected text | expected by the fix's own claim | actual |
|---|---|---|---|
| A | `/// this crate unconditionally over-reserves memory and keeps the mapping` (the synthetic probe), as its own isolated doc block | FAIL | **FAIL**, exit 1, correctly names `lib.rs:2440` |
| B | the **verbatim round-3 F4 sentence** restored into `reserve_aligned`'s rustdoc: `/// On a miss (wrong alignment), over-reserves \`size + align\` / /// bytes and keeps the full mapping. On Windows, / /// unconditionally over-reserves \`size + align\` bytes and keeps the full / /// mapping.` | FAIL | **PASS**, exit 0 |
| C | the **verbatim round-4 R6 sentence** restored into the module `//!` doc: `//! allocator's segments) by over-reserving \`size + align\` bytes and keeping the / //! full mapping, plus page-granularity decommit/recommit …` | FAIL | **PASS**, exit 0 |

Test B is the decisive one. The guard's own header explicitly scopes its known weakness to the
module top-doc and claims the opposite for exactly the case B tests: *"It reliably catches drift in
the SHORT, single-purpose doc comments (individual method/function docs) that were the site of 4 of
the 5 historical recurrences; the module top-doc (the 5th, R6's own fix) is the weak spot."*
`reserve_aligned`'s rustdoc **is** a short single-purpose method doc, and the guard passes on the
literal sentence that was drift #4 in it. The documented limitation understates the real one by a
category.

**Two further scope gaps, both cheap to state.** The guard reads exactly one file —
`${REPO_ROOT}/crates/vmem/src/lib.rs` (`:40`). Two of the five historical drift sites are outside
it: `crates/vmem/Cargo.toml:7` (the crates.io package description, named explicitly by round-3 F4)
and `crates/vmem/README.md:40` (named by W5). A sixth recurrence in either is invisible to this
guard by construction.

**Why this matters more than a normal LOW.** `npm run check` now prints a green
`vmem-doc-drift-guard` line on every pre-push run. W5 asked for this guard two rounds ago precisely
because five rounds of humans failed to catch this sentence; the round that finally added it has
produced a green light that does not discriminate. A guard that always passes is worse than no
guard, because it retires the vigilance that was catching the drift.

**Fix.** Split blocks on sentence boundaries (`.` followed by whitespace) before testing, and make
the predicate positional rather than set-membership: the real invariant is "a sentence containing
`over-reserv`/`trim` must, **in that sentence**, be scoped by a condition" — e.g. require one of
`if|when|on a miss|for \`?align|<=|>` in the same sentence, and treat `unconditionally` as an
outright trigger regardless of qualifiers. Extend the file list to `crates/vmem/Cargo.toml` and
`crates/vmem/README.md`. Until then, the guard's header should say plainly that it catches only
fully-unqualified sentences and has never been shown to catch a real historical drift.

## CR3 — MEDIUM — task #873's R9 fix **reverted** task #859's round-3 F3 correction: `crates/vmem/Cargo.toml:107` claims `#[doc(hidden)]` **accessors** again, which is false about this crate, and now contradicts the CHANGELOG entry written 20 minutes earlier in the same round

`crates/vmem/Cargo.toml:106-107`, against `crates/vmem/src/lib.rs:207`, `:216`, `:229`, `:242`
(the four `#[doc(hidden)]` statics) and `:246-250`, `:277-281`, `:300-304` (the accessors, none of
which carries `#[doc(hidden)]`); against commit `fe19572`'s message and `CHANGELOG.md:321`.

**The history, in order.**

* Round-2 task #853 hid the four **statics** and deliberately left the accessor functions public
  and documented.
* Round-3 **F3** flagged `crates/vmem/Cargo.toml`'s "`#[doc(hidden)]` **accessors**" clause as
  describing "the inverse of what shipped".
* Round-3 task #859 (`fe19572`) fixed it. Its commit message states the reasoning explicitly:
  *"task #853 actually hid the STATICS (correct) and left the accessor FUNCTIONS public (also
  correct, intentional) — reworded to '`#[doc(hidden)]` statics'."*
* Round-4 task #873 (`bb729bf`, R9) changed it back to
  `statics gated on \`bench-internals\` with \`#[doc(hidden)]\` accessors`, justifying the reversal
  as matching *sefer-alloc's* convention (`src/alloc_core/alloc_core_core_diag.rs:69-71`, where
  private statics are wrapped by `#[doc(hidden)] pub fn` accessors — which is accurate about
  sefer-alloc).

**Why the reversal is wrong anyway.** The sentence's subject is this crate: *"**Mirrors**
sefer-alloc's own `bench-internals` convention (…)"*. `aligned-vmem` does **not** mirror it — its
statics are `#[doc(hidden)] pub static` and its accessors carry only
`#[cfg_attr(docsrs, doc(cfg(…)))]`, verified at all six sites above. So the fix traded a clause that
was wrong about this crate's attribute placement for a clause that is wrong about this crate
*mirroring* the convention, and restored the exact word F3 filed a finding against.

**And it desynchronizes the round from itself.** `CHANGELOG.md:321` — written by task #872 at
00:25, thirteen minutes before task #873's edit at 00:45, both merged to `main` in this round —
records: *"reworded to '`#[doc(hidden)]` statics'"*. As of `HEAD` the file says "accessors". The
round's own historical record now describes a state the round itself undid.

**Fix.** Either restore #859's wording, or rewrite the clause so its subject is unambiguous:
`(AtomicU64 storage and increments both gated on bench-internals; the statics are #[doc(hidden)],
the accessor functions are public — sefer-alloc's own convention is the inverse placement)`. Then
correct `CHANGELOG.md:321` in the same commit, or the two disagree either way.

## CR4 — LOW-MEDIUM — the new `docs/CORRECTNESS_OPEN_ITEMS.md` card mis-attributes the round-2 CHANGELOG-gap closure to `7663811` — a commit that closed the **round-1** gap, predates the review the card says caught it, and is the campaign's fabrication-incident commit; its round-1 bullet names the wrong task too

`docs/CORRECTNESS_OPEN_ITEMS.md:61-78` (the new `[A]` item 1), specifically `:64` and `:77`, against
`git log`.

Two mis-citations in a card whose stated purpose is that *"a fresh session inherits no memory of the
debt"* — i.e. the one artifact in this round that is supposed to be trustworthy without
re-derivation.

1. **`:77` (the Evidence line):** *"commit 7663811 (task #857) **which itself closed the round-2
   gap** after it was caught by round-3's review"*.
   Verified: `git log -1 7663811` → *"docs: task #857 paper trail — #849 perf report + OPEN_ITEMS
   card + CHANGELOG (W15/W16)"*, dated 2026-08-12. `git log -S"tasks #842-850" -- CHANGELOG.md`
   returns exactly `7663811`. So `7663811` wrote the **round-1** (#842–850) entry, it was prompted
   by round-2's W16 (not round-3's review), and the **round-2** (#851–857) gap was closed by
   `c14bd3a` (task #863) — which the card's own bullet at `:70-72` states correctly two lines
   earlier. The Evidence line contradicts the body it is evidence for.
2. **`:64` (the round-1 bullet):** *"Round 1 (tasks #842-850, **closed by task #855**)"*. Task #855
   is the W6–W13 hygiene bundle (`e8e204a`); it wrote no CHANGELOG entry. The round-1 entry was
   written by task #857 / `7663811`, as (1) establishes.

**Why `7663811` specifically is the wrong commit to name loosely.** Its own message documents that
its delegated delivery *"was substantially FABRICATED — discarded entirely and rewritten from
verified facts"*, including an invented CHANGELOG draft with non-existent test filenames, another
crate's `mock::Call` variants, and a never-measured "~15-20% reduction" perf number. Of every SHA in
this campaign, it is the one whose citation should be exact. Naming it as the closer of a gap it did
not close, in the durable index, is how a future round inherits a wrong fact with a real SHA
attached to it.

**Not a fabrication.** Both errors are mis-attributions of real commits, not invented content. I
checked every other verifiable claim in the card and the rest holds.

**Fix.** `:77` → *"commit `c14bd3a` (task #863), which closed the round-2 gap after round-3's F11
caught it"*; `:64` → *"closed by task #857 (`7663811`)"*.

## CR5 — LOW — R3 corrected the two split accessors' half-stated dispatch condition and left the **third** accessor, the derived sum, carrying the identical defect

`src/alloc_core/alloc_core_core_diag.rs:129-137` (`dbg_windows_reserve_commit_calls`), against the
two accessors R3 did fix (`:145-150`, `:157-162`) and `crates/vmem/src/lib.rs:1521` (the real
condition).

R3 item 2 was: *"`:147-148` and `:158-160` state only half the dispatch condition … The real
condition is `align <= WIN_ALLOCATION_GRANULARITY && commit_len == size`."* Task #870 fixed both
cited accessors, correctly. The sum accessor sitting directly above them still reads:

```rust
/// Unix/miri). The sum of the single-call fast path (`align <= 64 KiB`) and
/// the two-call traditional path (`align > 64 KiB`). See
```

Same omission, same file, sixteen lines up, in the accessor that is the *actual* oracle surface —
`examples/r32_13_windows_reserve_commit_decomposition_gate.rs` reads
`HeapCore::dbg_windows_reserve_commit_calls()`, as round-3 F3 established. R3 cited `:145-155`,
`:157-166` and `:169-183` and did not cite `:129-143`, so this is genuinely surviving residue
rather than a missed prescription — which is exactly the pattern (fix the cited lines, leave the
identical defect one item over) that F3 → R3 → this finding now demonstrates three rounds running.

**Fix.** One clause: `the single-call fast path (\`align <= 64 KiB\` **and \`commit_len == size\`**)
and the two-call traditional path (**everything else**)`.

## CR6 — LOW — R7 removed the stale head/tail-trim clauses from one paragraph of `unix_reserve`'s doc comment and left the identical deleted-code description two paragraphs above it, in the same comment

`crates/vmem/src/lib.rs:1825-1835` (the task #714 paragraph, untouched by this round) against
`:1841-1850` (the REASONED-FROM-SPEC paragraph, correctly rewritten by task #871) and against the
three surviving `munmap` sites `:1918`, `:1996`, `:2017`.

R7's fix is right where it was applied: the "`head` provably `0`" / "`tail_len` provably
huge-page-aligned" clauses are gone, the surviving `over`-alignment clause is foregrounded, and the
new sentence correctly states that the head offset is non-zero in general for
`align > LINUX_HUGE_PAGE_SIZE`. I re-derived the 4 MiB/2 MiB example and it is correct.

But the paragraph ten lines above, in the same `///` block, still says:

```
/// A non-huge-aligned `size` would cause `munmap` calls on the over-reserved
/// tail to fail `EINVAL` (silently discarded by this function's own
/// `let _ = munmap(...)` cleanup calls), leaking the ENTIRE mapping …
```

Two claims, both about code task #842 deleted, and R7 named exactly this deletion as its own
premise:

* **"`munmap` calls on the over-reserved tail"** — there is no tail `munmap`. `grep -n libc_munmap
  crates/vmem/src/lib.rs` returns three sites: `:1918` (`libc_munmap(region_ptr, over)`, the
  whole mapping, fit-failure path), `:1996` (`libc_munmap(region_ptr, size)`, the whole exact-size
  mapping, fast-path alignment miss), `:2017` (`libc_munmap(reservation, reservation_len)`, the
  whole mapping, `release_reservation`). None is a sub-range trim.
* **"this function's own `let _ = munmap(...)` cleanup calls"** — no such form exists; both
  in-function sites are bare `unsafe { libc_munmap(…) };`.

Doc-only, and the *conclusion* (keep the huge-page-alignment restriction) remains correct for the
surviving reason. Filed because it is the identical residue class R7 was, and because the reader
most likely to hit it is the reader R7 was written for: someone deciding whether the
`reserve_aligned_huge` size/align restriction is still needed now that the trim is gone.

**Fix.** Rewrite the failure-mode sentence in terms of the surviving whole-mapping `munmap`: a
non-huge-aligned `size` makes `over = size + align` non-huge-aligned, so the whole-mapping `munmap`
in `release_reservation` fails `EINVAL` and leaks the mapping plus its pinned huge pages.

## CR7 — INFO — the new design note's TL;DR describes the current Windows two-call path as a three-call `VirtualAlloc` / `VirtualFree` / `VirtualAlloc` "dance", contradicting the "Syscalls: 2 → 1" claim in the same bullet and the code it cites

`docs/perf/ALIGNED_VMEM_VIRTUALALLOC2_VA_OPTIMIZATION_OPPORTUNITY.md:37-38`, against
`crates/vmem/src/lib.rs:1523-1610` (the two-call path).

```markdown
* **Syscalls:** 2 → 1 (no separate `VirtualAlloc(MEM_RESERVE)` + alignment-finding
  `VirtualFree` + `VirtualAlloc(MEM_COMMIT)` dance)
```

The success path is: `winapi_virtual_reserve(over)` (`:1528`) → compute the aligned base
arithmetically via `align_up_addr` + `with_addr` (`:1547-1564`) → `VirtualAlloc(base, commit_len,
MEM_COMMIT | extra_commit_flags, …)` (`:1570-1577`). **No `VirtualFree`.** The only
`winapi_virtual_release` calls in the function are on the fit-failure path (`:1560`) and the two
commit-failure cleanup paths (`:1591`, `:1598`), all of which return `Err`. R8's own original text
got this right ("2 syscalls"), and the note's §2 gets it right; only the TL;DR invents the third
call, in the same bullet that says the count is 2.

Zero downstream consequence — the note is design-only and changes no code — but it is a factual
claim about the exact code path the note exists to describe, in the first bullet a reader sees.

**Also verified and clean, in the same file, so this INFO is read in context:** every measured
figure the note cites was independently checked against
`docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md` and matches exactly — median
`MEM_RESERVE` 4,580.5 ns (`R32_13:160-161`), median `MEM_COMMIT` 9,133.0 ns (`:160-161`), pair
13,713.5 ns (`:160`), avoidable share 4.33–4.78% with 4.60% median (`:195`), and R32-13's own
`VirtualAlloc2` step-3 rejection (`:36`). The commit (`a9eca93`) touched exactly one file, +431/−0,
so R8's "no code changes to `crates/vmem/src/`" constraint held.

## CR8 — INFO — `scripts/check-all.mjs:237` cites a task/finding combination that does not exist

```js
    // R4 (task #854/R6): grep-based guard against the doc-comment drift class
```

`R4` this round is the `is_huge()` coverage finding, not a doc-drift guard. Task #854 is round-2's
W5 fix. The guard is task #871, closing R6, implementing what W5 (task #854's own finding) asked
for. Three identifiers, none of which pair correctly. Cosmetic; one line.

**Fix.** `// R6 (task #871; the guard W5/task #854 asked for two rounds ago):`.

## CR9 — INFO, publish-relevant, PRE-EXISTING (not introduced this round) — `crates/vmem/Cargo.toml:78` bases the `mock` feature-unification hazard's deferral on "this crate has not yet had its first `crates.io` publish", which is false, and cites another crate's task number

`crates/vmem/Cargo.toml:75-83`, against `crates/vmem/Cargo.toml:3` (`version = "0.2.0"`) and the
session task list (task **#658**: *"aligned-vmem — publish 0.2.0 (local already bumped, **crates.io
still shows 0.1.0**)"*; task #659 is *racy-ptr-cell*).

The deferral argument for converting `mock` from a Cargo feature to a `--cfg` flag reads:

> A stronger fix … was evaluated and DEFERRED: **this crate has not yet had its first `crates.io`
> publish (task #659)**, so removing `mock` as a Cargo feature later remains possible without an
> immediate breaking-change cost …

Both halves are wrong. `aligned-vmem` 0.1.0 is on crates.io (per task #658's own title), so removing
the `mock` feature is *already* a breaking change; and the cited task number belongs to a different
crate. This is round-2 text (task #715), not this round's — I flag it here only because round 4
explicitly re-opened the publish question, R5 was closed on exactly the "do it before publish while
it is still free" reasoning, and this paragraph applies the same reasoning from a false premise.

**Fix (decision, not text).** Confirm 0.1.0's published feature list against crates.io, then either
re-state the deferral honestly (the hazard ships either way; converting now is itself the breaking
change) or take the conversion in the 0.2.0 window. Correct the task number to #658 regardless.

## CR10 — INFO — round 4's own tasks (#867–#874) have no CHANGELOG entry as of `HEAD`, and this round's own review document is still untracked

`git log --oneline -20` (sixteen commits, tasks #867–#874) against
`grep -nE "#86[7-9]|#87[0-4]" CHANGELOG.md` → no match; `CHANGELOG.md:316` is the newest entry and
covers #858–864. `git status --short` → `?? docs/reviews/2026-08-12-aligned-vmem-round4-review.md`.

Stated as an observation with its caveat, not as an accusation: a round is not closed until its
post-work lands, and this closing review is part of that post-work, so the entry may well be written
in the same pass that reads this. But the shape is worth naming out loud, because the round's own
new `[A]` card (`docs/CORRECTNESS_OPEN_ITEMS.md:75`) reads **"Current number: 3 confirmed
recurrences (aligned-vmem rounds 1, 2, and 3)"** — and the fourth is sitting in `git log` as that
sentence is written. If the #867–874 entry lands, update the card's Current-number and Evidence
lines in the same commit; if it does not, the card is understating its own subject by one.

Note also that round 3's F11 item 3 (untracked review documents) recurs here for round 4's document
by the same mechanism, and this document will make it two.

---

## Checked and explicitly NOT findings — R1–R13, verified one by one in the current tree

* **R1 (HIGH, CI runs `mock` not the backend) — the substantive half is CLOSED and I proved it by
  execution.** `ci.yml:781` and `:813` both run `cargo test -p aligned-vmem --features "lazy-commit
  huge-pages fault-injection bench-internals" --no-fail-fast`. Run here on real Windows: 5
  `fault_injection` tests (0 under `--all-features`), 11 `lazy_commit`, 1 `huge_pages`, 2
  `min_page`, 19 `smoke`, 3 `vmemerror_io_bridge`, **0 `mock`** — exactly the shape R1 predicted,
  with `mock.rs` correctly `cfg`-excluded rather than silently skipped. The `commit_len == size`
  guard at `lib.rs:1521` now has a CI row that would fail if it were deleted, on the one platform
  where it exists. That is the hole R1 opened the round to close, and it is closed. The deletion of
  the companion `--all-features` row is CR1.
* **R2 (MEDIUM, `fault-injection` never runs where `commit_range` is a real syscall) — CLOSED, for
  free, exactly as R2 predicted.** `tests/fault_injection.rs` requires `mock` OFF; the new Windows
  row includes `fault-injection` and excludes `mock`; the file goes from 0 tests under
  `--all-features` to 5 under the new set, verified by execution above. Windows is where
  `commit_range_impl` (`lib.rs:1697-1701`) is a real `VirtualAlloc(MEM_COMMIT)` rather than Unix's
  compile-time `Ok(())`, so the module doc's "proves the fault-injection hook coexists with (and
  does not replace) the real backend" claim becomes falsifiable for the first time on the first
  push.
* **R3 (MEDIUM, three inaccuracies task #859 introduced) — all three CLOSED.** (1)
  `alloc_core_core_diag.rs:147` now reads *"(the Windows allocation granularity — the Windows page
  size is 4 KiB)"*, which matches `lib.rs:1760` (`WIN_ALLOCATION_GRANULARITY`, commented "64 KiB")
  and `lib.rs:392` (`info.dw_page_size`, the field `query_os_page_size` actually returns). (2) Both
  accessors now state `and commit_len == size` / `or commit_len != size`, matching `lib.rs:1521`'s
  real condition verbatim. (3) `dbg_reset_vmem_bench_internals_counters`'s list is now four entries
  for the word "four", with the derived sum `dbg_windows_reserve_commit_calls` dropped — and the
  drop is correct: `reset_bench_internals_counters` (`lib.rs:304`) resets four statics, and the sum
  is computed, not stored. The identical defect in the *third* accessor is CR5.
* **R4 (LOW-MEDIUM, `is_huge()` untested) — CLOSED, and non-vacuously.** `tests/smoke.rs:54-62`
  adds `ordinary_reservation_never_reports_huge` (reserve 2 MiB/2 MiB, assert `!is_huge()`); it
  runs — smoke went 18 → 19 tests in both feature configurations, observed. `tests/huge_pages.rs:58-62`
  adds `#[cfg(not(target_os = "linux"))] assert!(!r.is_huge(), …)` inside
  `reserve_aligned_huge_ordinary_page_sized_request_succeeds`, which is not `#[ignore]`d and does
  execute (the file is `#![cfg(feature = "huge-pages")]`, and `huge-pages` is in both the
  `--all-features` row and the new CI feature set; huge_pages ran 1 test in both, green). The
  commit message is honest that reverting W2 does not fail either assertion *on Windows* — and it is
  right, because W2 was a Unix-only fix. The assertion that matters is the macOS one: on non-Linux
  Unix, `HUGE_SUPPORTED` is `false` (`lib.rs:2127-2134`) and both return sites thread
  `HUGE_SUPPORTED && huge` (`:1899`, `:2011`), so reverting either to bare `huge` makes
  `reserve_aligned_huge(4 MiB, 4 MiB).is_huge()` return `true` on Darwin and reds this assertion on
  the macOS row. R4 asked for "exactly the macOS-row-shaped assertion W2's fix has never had"; that
  is what landed, and the new macOS row runs it.
* **R5 (LOW-MEDIUM, `ReservationParts: Clone`) — CLOSED, and it is the one publish gate this round
  named.** `crates/vmem/src/lib.rs:733` is now `#[derive(Debug, PartialEq, Eq)]`. A
  whole-repository grep for `ReservationParts` (all `.rs`/`.md`/`.toml`/`.mjs`, not just
  `crates/vmem`) returns 40 hits and **no `.clone()` call on a `ReservationParts` value** anywhere —
  the only in-crate consumers are `into_reservation_parts` (`:573`), `release_parts` (`:937-938`),
  `as_tuple`, and `tests/smoke.rs:65-90`, none of which clones. The escape hatch R5 preferred
  (`as_tuple(self)` returning a `Copy` tuple) is intact at `:744+`. The whole suite compiles and
  passes in both feature configurations, so nothing internal depended on the derive. Removing a
  derived `Clone` after 0.2.0 would have been breaking; doing it now cost nothing, which was the
  entire argument.
* **R6 (LOW, 5th over-reserve drift) — the doc half is CLOSED; the guard half is CR2.**
  `lib.rs:24-30` no longer states the mechanism unconditionally: it now describes the Unix
  exact-size fast path and its miss, and both Windows paths including "over-reserving nothing —
  base == region" for `align <= 64 KiB`. It matches `reserve_aligned`'s own rustdoc (`:764-773`),
  `Cargo.toml:7` and `README.md:40`, so all five sites of this sentence family finally agree. The
  adjacent `reservation_len()` caveat landed too (`:483-492`), and it is accurate: the Windows
  single-call path returns `commit_len` (`:1520`), Windows rounds VA reservations up to the 64 KiB
  granularity, and `VirtualFree(base, 0, MEM_RELEASE)` ignores the length (`release_reservation`,
  `:1613+`), so the caveat's "harmless for correctness" is right for the right reason.
* **R7 (LOW, stale huge-page justification) — the cited paragraph is CLOSED; the paragraph above it
  is CR6.** `lib.rs:1841-1850` now keeps only the `over = size + align` clause, states that the head
  offset is non-zero in general for `align > LINUX_HUGE_PAGE_SIZE` with the correct 4 MiB/2 MiB
  worked example, and attributes the reason it no longer matters to task #842's whole-mapping
  decision. The restriction it justifies (`:1859-1865`) and its three regression tests
  (`tests/huge_pages.rs:69-109`) are untouched, which is the correct outcome — R7 was explicit that
  the restriction is still needed.
* **R8 (LOW, design note) — CLOSED, and it survived the fabrication check.**
  `docs/perf/ALIGNED_VMEM_VIRTUALALLOC2_VA_OPTIMIZATION_OPPORTUNITY.md` (431 lines) exists, is
  labelled `DESIGN-ONLY (reasoned from spec)` in its own title and again at `:15-19`, records its
  base revision as `8804fc9`, and states the availability floor (Win10 1803+/Server 2016+), the
  `GetProcAddress`-vs-link-time trade-off, and the Windows-version-floor policy question as the
  gating decision. `git show a9eca93 --stat` confirms exactly one file changed, +431/−0 — **no code
  leaked into `crates/vmem/src/`**. Every cited R32-13 figure re-verified line-by-line against the
  source report (see CR7's closing paragraph). The TL;DR's phantom `VirtualFree` is CR7.
* **R9 (INFO, "always compiled" storage claim) — the `lib.rs` half is CLOSED; the `Cargo.toml` half
  is CR3.** `lib.rs:182-184` now reads *"`AtomicU64` storage, increments gated on `bench-internals`
  … (storage itself is also gated, not compiled without the feature)"*, which matches the code
  directly beneath it: `use core::sync::atomic::AtomicU64;` at `:186-187` and all four statics carry
  `#[cfg(feature = "bench-internals")]`.
* **R10 (INFO, `debug_assert!`'s reach) — CLOSED.** `lib.rs:368-371` adds the note, and I re-derived
  its call-graph claim independently: `query_os_page_size` is called from exactly one site,
  `page_size()`'s cold path (`:348`), which is reached from `decommit`, `decommit_lazy`, and the
  Unix-only `try_reserve_aligned_exact` (`:1973`). No Windows reservation path calls it —
  `validate_size_align` uses the `PAGE` constant and `win_reserve_commit` asks the OS nothing. The
  note says precisely that and claims nothing more.
* **R11 (INFO, counter fidelity) — both CLOSED, and both re-verified against the code.** (1) Both
  `WINDOWS_RESERVE_COMMIT_*` docs now say "number of **successful** … calls" (`:211`, `:224`),
  matching increments that sit after every fallible step (`:1516`, `:1601`). (2)
  `UNIX_EXACT_RESERVE_ATTEMPTS`'s doc now discloses that it "increments BEFORE the `mmap` call, so
  it includes both alignment misses and OS-level failures" — verified at `:1972-1973`, where the
  `fetch_add` is the first statement of `try_reserve_aligned_exact`, before `libc_mmap` at `:1976`
  and before its `MAP_FAILED` early return at `:1977-1981`. The `MAP_HUGETLB`-refusal case R11
  called out is exactly this path.
* **R12 (INFO, missing #858–864 CHANGELOG entry) — CLOSED, and I re-verified ALL SEVEN cited SHAs
  rather than the three requested.** `CHANGELOG.md:316-327`. `git log -1` on each: `75bba05` (#858,
  F1/F2), `fe19572` (#859, F3), `fd032af` (#860, F4/F5), `22f1e55` (#861, F6/F7), `91d5555` (#862,
  F8/F9/F10), `c14bd3a` (#863, F11), `d66c3c7` (#864, numa-shim) — every one resolves, and every
  one's subject line matches the task number and finding set the entry attributes to it. No invented
  SHAs, no misattributed tasks. The entry also correctly carries the round's scope caveat
  ("`aligned-vmem` is a workspace member library not consumed by `sefer-alloc`'s own `production`
  feature bundle"). The companion `docs/CORRECTNESS_OPEN_ITEMS.md` card exists at `:61-78` with the
  full Status / Current-number / Next-trigger / Evidence block CLAUDE.md's R34-24 rule requires, in
  a newly-created `### [A] Active` section that does not collide with any existing heading
  (`grep -n "^### \[" ` returns exactly `:61` `[A]` and `:79` `[T]`). Its two mis-citations are CR4.
* **R13 (INFO, CLAUDE.md's stale example) — CLOSED, and the replacement is verified.** CLAUDE.md's
  exception 3 now reads *"(e.g. `crates/numa/src/lib.rs`, `crates/malloc-bench/src/lib.rs`)"*.
  `ls crates/numa/src` → `lib.rs`. `ls crates/malloc-bench/src` → `lib.rs`. Both genuinely
  single-file; `crates/vmem/src/` (4 files) is correctly no longer cited.

## Also re-checked, weighted toward what this round's diff could plausibly have broken

* **No out-of-scope edits.** `git diff 8804fc9..HEAD --stat` is 12 files: `ci.yml`, `CHANGELOG.md`,
  `CLAUDE.md`, `crates/vmem/{Cargo.toml,src/lib.rs,tests/huge_pages.rs,tests/smoke.rs}`,
  `docs/CORRECTNESS_OPEN_ITEMS.md`, the new design note, `scripts/{check-all.mjs,vmem-doc-drift-guard.mjs}`,
  `src/alloc_core/alloc_core_core_diag.rs`. Every one is traceable to a specific R-finding. No TODO,
  no placeholder, no commented-out code, no stray debugging artifact anywhere in the diff.
* **No new `unsafe`, no new public surface.** The round's only `lib.rs` code change is the deletion
  of `Clone` from a derive list; everything else in `lib.rs` is doc comments. No new `pub fn`, no
  new `dbg_*` hook, nothing matching CLAUDE.md's benchmark-hook rule (a safe `pub fn` taking a raw
  pointer and touching allocator metadata). `cargo clippy -p aligned-vmem --all-features
  --all-targets -- -D warnings` and the default row are both green.
* **The root crate still builds and documents cleanly with the touched forwarders.**
  `cargo build -p sefer-alloc --features bench-internals` green; `cargo clippy -p sefer-alloc
  --features bench-internals` green with zero warnings; `cargo doc -p sefer-alloc --features
  "bench-internals internals" --no-deps` green with zero warnings — which specifically confirms R3's
  edited rustdoc (including the intra-doc link list it shortened) still resolves.
* **`ci.yml` is still valid YAML** and every other `aligned-vmem` row is unchanged: `:160`
  (`aligned-vmem-gates`, `--all-features`), `:164` (the miri `cfg` compile gate), `:847`/`:889`/`:909`
  (`test-workspace`'s three rows), `:2029` (the weekly feature-powerset sweep). Only the two platform
  jobs were touched.
* **The `mock` non-additivity documentation is intact and correctly cited.** `ci.yml`'s new comments
  point at `crates/vmem/Cargo.toml:55-84`; I checked the range — `:55` is the first line of the
  `mock` feature block and `:84` is `mock = []`. The citation is exact.
* **`grep -rn "cfg(test)" crates/vmem/src` → no match**, so the round introduced no inline test
  module; the two new tests went to `tests/`, per CLAUDE.md.
* **No new doctests.** `cargo test -p aligned-vmem` reports `Doc-tests aligned_vmem … running 0
  tests` in both configurations; the module doc's example is still a ```` ```text ```` fence
  (`lib.rs:51`).
* **Round-3 fixes this round could have regressed, spot-checked and intact:** `tests/mock.rs:35`
  still reads `start == page_size()` (F1's fix survives in the source — its CI coverage is CR1);
  `lib.rs:2392-2410`'s miri 3-tuple destructuring (W1) unchanged; `HUGE_SUPPORTED` still
  `all(target_os = "linux", feature = "huge-pages")` with both Unix sites threading
  `HUGE_SUPPORTED && huge`; the `debug_assert!` F7 added is still at `:361-364`; `is_huge`'s
  F8-repaired paragraph structure is intact at `:518-531`; F9's two-call NOTE is still at
  `:1604-1608`.
* **Performance: null, for the fourth round running.** This round changed no algorithm, no feature
  composition, and no default. Per CLAUDE.md's R30-12 taxonomy the eight commits are correctly
  prefixed — `ci`, `test`, `docs` ×4, `bench` (the design note), and one `fix(vmem)` (the `Clone`
  removal, which changes a compile-time API surface and no runtime behavior). Nothing in this round
  is or claims to be `perf(runtime)`.

---

## Recommended order

1. **CR1** — restore the two deleted `--all-features` steps under the comments that already describe
   them. Two lines. It is the only finding here that costs real CI coverage, and it costs coverage
   that a red macOS run bought one round ago.
2. **CR2** — either fix the guard's predicate (per-sentence + positional qualifier + widen to
   `Cargo.toml`/`README.md`) or downgrade its header to state honestly that it has never been shown
   to catch a real historical drift. A green check that cannot fail on the thing it guards is the
   worst of the three states.
3. **CR3, CR5, CR6** — three one-clause doc corrections, batchable in one pass. All three are the
   same residue class; fixing them together is also the natural moment to grep for the *fourth*
   instance rather than waiting for round 5 to find it.
4. **CR4** — two citation corrections in the durable index. Minutes, and it is the one artifact in
   this round designed to be trusted without re-derivation.
5. **CR10** — write the #867–874 CHANGELOG entry and commit the two round-4 review documents; then
   update the new `[A]` card's Current-number from 3 to 4 (or, if the entry lands in the same pass,
   record that the fourth was caught before it aged).
6. **CR7, CR8** — two cosmetic corrections, no urgency.
7. **CR9** — a decision, not an edit: confirm 0.1.0's published feature set and re-state or re-take
   the `mock`-as-`--cfg` conversion in the 0.2.0 window.

Nothing here is a breaking change. Nothing here reopens a V-, W-, F- or R-series finding: CR3 and
CR5 are *adjacent* to F3/R3 and CR6 to R7, but each is a distinct site that the corresponding
finding did not cite.

## On "is this crate closing in on ready for 0.2.0?" — an honest answer

**The source: yes, and this round adds evidence rather than doubt.** Four rounds have now failed to
find a soundness hole, a race, a panic-safety gap, or a provenance defect. This round's only code
change to `lib.rs` was deleting a derive. R5 — the single item round 4 called a hard publish gate,
because it is the one that stops being free the moment 0.2.0 is on crates.io — is closed, verified
by grep and by a full green suite in two feature configurations.

**The verification: closer, but the pattern that produced R1 has not been retired.** R1 was
described by round 4 as "a first-order hole that three rounds of source review could not have found,
because it lives in the interaction between a Cargo feature's semantics and a CI invocation's
flags." Round 4's fix for it introduced a second-order version of the same thing: a CI file whose
comments and whose steps disagree, which no test and no lint can catch, and which I found only by
running a YAML parser over the file and enumerating what actually executes. The same is true of CR2:
`npm run check` reports the drift guard green, and the guard is green on the historical drift. Both
findings are of the form "the verification artifact says something the verification does not do" —
which is round 4's own thesis, one level up.

**Concretely, before `cargo publish`:** CR1 (restore the two rows) and CR9 (settle the `mock`
premise) want resolving. CR2 wants resolving before anyone relies on the guard's green light.
Everything else on this list can ship. And the standing precondition from round 3 still has not been
met: **none of this has ever run in CI** — `origin/main` is still `8804fc9`, sixteen commits behind,
so every green claim in this document (and in rounds 1–4) remains a local claim on one Windows host
with a 4 KiB page. Per CLAUDE.md's own "Then confirm CI went green — do not assume it", the push and
the landing-SHA confirmation are the next real gate, and CR1 is the thing worth fixing on the near
side of it.
