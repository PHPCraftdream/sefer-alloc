# `aligned-vmem` — round-9 readonly review (post-round-8-closing, post-push, CI green)

**Scope:** `crates/vmem/` in full (`src/lib.rs` all 2,653 lines, `src/error.rs`,
`src/mock.rs`, `src/fault_injection.rs`, all seven `tests/*.rs`,
`benches/vmem_bench.rs`, `examples/v20_849_unix_exact_reserve_hit_rate.rs`,
`Cargo.toml`, `README.md`), the round-8 diff (`8380607..b6bfdac`, including the
closing commit `b39882b`'s UC1–UC6 fixes), `CHANGELOG.md`'s round-8 entry, and
both open-items indexes' entries for this crate
(`docs/CORRECTNESS_OPEN_ITEMS.md` items 1, 2, 41, 42, 43, 48, 49, 50, 51;
`docs/perf/OPEN_ITEMS.md` item 46 including its S12/T10 blocks).

**Review type:** READ-ONLY. No file in the repository was modified by this
review other than the creation of this document. No `git add` / `git commit` /
`git push` / branch, worktree or ref mutation. Every command quoted below was
executed on this host (or read from the real GitHub Actions API); every
`file:line` citation was read in the current tree before being written down.

**Base revision:** local `main` @ `b6bfdac` ("docs: commit session checkpoint
files"), one commit above `b39882b` ("fix(vmem), docs: round-8 closing review —
fix UC1-UC6, write CHANGELOG entry, commit both round-8 review docs").
`git fetch && git rev-parse origin/main` → `b6bfdac08562e7cc8a5369ffc3fd7ca3a7838909`,
identical to local `HEAD`; `git log origin/main..HEAD --oneline | wc -l` → **0**.
`git status --porcelain` → clean (the three checkpoint files that were untracked
during round 8 were committed by `b6bfdac`).

**Toolchain / host:** `rustc 1.97.0` (2d8144b78 2026-07-07),
stable-x86_64-pc-windows-msvc; Windows 10 Pro, 4 KiB page. **No Darwin host and
no Darwin target** — every Darwin claim below is reasoned from spec or read from
the real CI job log of the landing SHA, never executed here. Per item 51's own
`Next trigger`, this round DID run the Unix cross-compile clippy row (see the
verification block below) — the first round to include it in its stated matrix
from the start rather than as a closing-review afterthought.

**Finding prefix:** `V2` (ninth round; the single-letter `V` was round 1's and
is not reused). Prior prefixes deliberately not reused: `V`/`W`/`P` (rounds
1–2), `F` (round 3), `R`/`CR` (round 4 + closing), `Q`/`QC` (round 5 + closing),
`S`/`SC` (round 6 + closing), `T`/`TC` (round 7 + closing), `U`/`UC` (round 8 +
closing).

**Date:** 2026-08-13.

---

## Verdict up front

**Round 8's closing remediation (`b39882b`) is clean, and this is the
campaign's first effectively-null code round.** All six UC findings landed on
the right content (§"Round-8 closing pass — UC1–UC6 verification"), the whole
verification matrix is green here re-executed rather than taken on trust —
including, for the first time as a first-class matrix row, the
`x86_64-unknown-linux-gnu` cross-compile clippy check item 51 owns — and CI is
green on the landing SHA read from the remote, with UC4's new discriminating
test arm **confirmed executed and passing on the real 16 KiB Apple Silicon
runner** (job `94496764432`), not merely compiled.

**This round found zero pre-existing defects in the crate.** Both LOW findings
live in text round 8's own closing commit wrote one commit ago — the campaign's
signature pattern ("round N's fix is round N+1's finding source") holding for a
ninth consecutive time, now in its weakest form yet:

- **V2-1 (LOW):** UC4's fix added the discriminating third call for
  `decommit`'s validation guard but not for `decommit_lazy`'s — the test's doc
  comment and assertion message both name the guard PAIR, while a
  `page_size()` → `PAGE` swap in the lazy guard alone still leaves the whole
  suite green on every platform including the macOS runner. The lazy guard is
  precisely the one item 48's own `Next trigger` (the S9 Darwin lazy-path fix)
  would touch next.
- **V2-2 (LOW):** the Windows `query_os_page_size` NOTE still lists
  `try_reserve_aligned_exact` among `page_size()`'s callers — U1's fix removed
  that call entirely last round; the only remaining runtime callers are
  `decommit`/`decommit_lazy`.
- **V2-3 (INFO):** the same UC4 hunk left a trailing "both calls" comment above
  a test body that now makes three calls on 16 KiB hosts.

**Performance: null, ninth consecutive round** — re-derived this round, not
inherited (see "Categories with nothing to report"). **Safety: null at every
severity** — every `unsafe` block in `src/` was re-read against its call site;
nothing unsound found on any tested platform, and no new `unsafe` token has
entered the crate since round 8's U1 hunk (which removed a conjunct, added
none). **Publish readiness (task #658): nothing here blocks 0.2.0** —
`cargo package --list` still returns the identical 20-file set, and no
publish-facing surface changed since U6's attribution landed.

**The pivot verdict this round was explicitly asked for: PIVOT. A ninth
straight read of `lib.rs` is no longer finding real things, and this round is
the proof by demonstration** — see the final section for the full argument and
the concrete replacement plan. The short version: this round's only findings
are in one-commit-old remediation text, which means the campaign is now purely
auditing its own tail, and every remaining genuinely-unknown answer (item 43's
BSD half, item 48's Darwin fix and S4 remainder, item 50's U10 half, item 41's
miri step, U1's fast-path observability) is blocked on infrastructure, not on
reading.

---

## What was verified green — every command below was executed on this host

```
$ git fetch && git rev-parse origin/main
b6bfdac08562e7cc8a5369ffc3fd7ca3a7838909        # == local HEAD; 0 unpushed

$ gh run list --commit b6bfdac08562e7cc8a5369ffc3fd7ca3a7838909
completed  success  CI                 main  push  31714828163  36m33s
completed  success  Kani verification  main  push  31714828124  39s

$ gh run view 31714828163 --json jobs -q '.jobs[]|select(.name|test("macos"))|…'
test macos (production)   success   94496764432

$ gh api repos/PHPCraftdream/sefer-alloc/actions/jobs/94496764432/logs
  # step `cargo test -p aligned-vmem --features "lazy-commit huge-pages
  #   fault-injection bench-internals" --no-fail-fast`, tests/smoke.rs:
  test decommit_contract_violation_never_reaches_madvise ... ok
  # -- the UC4 discriminating arm's FIRST execution on a real 16 KiB host:
  #    `page_size() > PAGE` is true there, so `decommit(base, PAGE, 2*PAGE)`
  #    genuinely ran and was rejected by the real page_size()-based guard.

$ cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast
fault_injection 5 / huge_pages 1 / lazy_commit 11 / min_page 2 / mock 0 /
smoke 20 / vmemerror_io_bridge 3 / doc-tests 0        => 42 passed, 0 failed

$ cargo test -p aligned-vmem --all-features --no-fail-fast
0 / 0 / 1 / 11 / 2 / 10 / 20 / 3                       => 47 passed, 0 failed

$ cargo clippy -p aligned-vmem --all-targets -- -D warnings                          -> clean
$ cargo clippy -p aligned-vmem \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings                                                     -> clean
$ cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings           -> clean
$ cargo clippy -p aligned-vmem --target x86_64-unknown-linux-gnu \
    --features "lazy-commit huge-pages fault-injection bench-internals" \
    --all-targets -- -D warnings                                                     -> clean
    # item 51's row, run as part of THIS round's standard matrix (seconds, warm)
$ cargo fmt -p aligned-vmem -- --check                                               -> clean

$ node scripts/vmem-doc-drift-guard.mjs
[vmem-doc-drift-guard] OK: no unconditional over-reserve/trim statements found  (exit 0)

$ cargo package -p aligned-vmem --list --allow-dirty
20 files                                # identical set to rounds 6/7/8; still no docs/

$ grep -rnE '^(<<<<<<<|=======|>>>>>>>)$' crates/vmem/ docs/CORRECTNESS_OPEN_ITEMS.md \
      docs/perf/OPEN_ITEMS.md CHANGELOG.md
(no output)
```

Test counts are identical to round 8 (42 / 47), consistent with `b39882b`
adding no test and disabling none (UC4 extended an existing test's body).
`Doc-tests aligned_vmem … 0 passed` — the no-doctests convention holds.

**Not re-run this round, deliberately:** the `RUSTFLAGS="-W
unsafe_op_in_unsafe_fn"` sweep behind item 49's count (forces a full workspace
rebuild; the item's card — correctly, per TC7 — instructs re-derivation on
demand rather than trusting a hardcoded list). Its "9 of 10 remaining" figure
is recorded as *inherited*, not re-verified, by this round — same posture as
round 8.

---

## Round-8 closing pass (`b39882b`) — UC1–UC6 verification

Checked before looking for anything new, because nine consecutive rounds have
found the closing fix to be the next round's bug source.

| # | Status in the current tree | Evidence |
|---|---|---|
| UC1 | **CLOSED** | `CHANGELOG.md:425` carries `#### aligned-vmem — round-8 follow-up (2026-08-13, tasks #897-903)` with a bullet per task PLUS a dedicated "Round-8 closing review findings (UC1-UC6) … all fixed in this same pass" sub-section — so the closing pass's own work is covered by the same entry, and round 8 is NOT left owing a second entry. I verified all seven cited merge SHAs (`491afe9`, `a469643`, `2195ad2`, `ccc017e`, `f654bda`, `f90a8a4`, `70df07e`) against `git log` — all real, all matching their stated tasks. Item 1's headline (`docs/CORRECTNESS_OPEN_ITEMS.md:63`) now says "**eight** times … (rounds 1-8)" and its `Current number` bullet (`:75`) records the 8th instance — headline and bullet agree, closing the U8/UC1 staleness chain with no residue. |
| UC2 | **CLOSED, and the convention question is genuinely settled, not deferred** | Both round-8 review docs are committed (`git show --stat b39882b`: +880 and +669 lines) and `git log --oneline -- <path>` resolves for both, so item 50's three path citations resolve. The new item 2 (`docs/CORRECTNESS_OPEN_ITEMS.md:79-84`) resolves the apparent two-convention conflict as two correctly-scoped conventions (this campaign commits its docs because its own index re-cites them by path across rounds; the root-crate R34 campaign does not because its index cites task numbers/SHAs instead) — I spot-verified its evidence: the four cited prior closing commits (`7c6e4be`, `e60e46a`, `1dbd6b4`, `8380607`) all exist and all committed review docs, and the R34-2 counter-convention text is really in `CHANGELOG.md`. |
| UC3 | **CLOSED** | Item 50's U11 bullet (`docs/CORRECTNESS_OPEN_ITEMS.md:2137`) now reads past-tense: "**Corrected round-8 closing review (task #904, UC3)** … task #897 (merge `491afe9`, this same round) already removed that dependency … The guard's acceptance side is now cosmetic, not load-bearing." The `Next trigger` bullet (`:2139`) likewise: "round 8's U1 fix already landed (`491afe9`, this same round), so this guard's untested acceptance side is purely cosmetic now". No conditional-tense residue found. |
| UC4 | **CLOSED for `decommit`'s guard — and confirmed live on real hardware — but the fix reproduces its own finding one function over → V2-1** | `smoke.rs:583-591` adds the third call `decommit(base, PAGE, 2*PAGE)` gated on `page_size() > PAGE`, with an accurate SAFETY comment (PAGE is genuinely a multiple of `PAGE` and, under the gate, genuinely not a multiple of `page_size()`); the assert message (`:594-601`) was extended to name the swap class. I re-derived the discrimination: under the correct guard the call is rejected (start = 4096 is not a multiple of 16384) → `attempts == 0`; under a swapped `decommit` guard it is forwarded → `attempts == 1` → assert fires. **And it ran for real:** the macOS CI log for the landing SHA shows `test decommit_contract_violation_never_reaches_madvise ... ok` on `macos-26-arm64`, where the gate is true — the first execution of this arm anywhere. What the fix does NOT do: discriminate `decommit_lazy`'s guard (its only lazy call is `decommit_lazy(base, PAGE, 0)`, rejected by `start >= end` under either base) — see V2-1. |
| UC5 | **CLOSED as filed; the item works** | Item 51 (`docs/CORRECTNESS_OPEN_ITEMS.md:2142-2146`) carries all four R34-24 fields, correctly records Status OPEN (the matrix amendment is durable only in the item itself), and its `Next trigger` instructs future rounds to run the cross-compile row. This round complied: the `--target x86_64-unknown-linux-gnu` clippy row ran as part of the standard matrix above (clean, seconds warm — matching the item's own cost estimate). |
| UC6 | **CLOSED as record-only, correctly** | No code change was made and none was owed; the record lives in the closing review and in item 48's S4-remainder / item 50 U10 linkage. Nothing to re-verify beyond confirming no stray code change rode along: `git show b39882b --stat` touches only `CHANGELOG.md`, `smoke.rs`, `docs/CORRECTNESS_OPEN_ITEMS.md`, and the two review docs. |

Surface checks on `b39882b`'s diff, all negative as expected: no `src/` file
touched at all (the round-8 closing commit's only code change is in
`tests/smoke.rs`), so no new `unsafe` token, no public-item change, no `#[cfg]`
change, no feature-composition change. `docs/perf/OPEN_ITEMS.md` untouched —
correct; nothing in UC1–UC6 concerned it.

---

## Category 1 — round-8-remediation follow-ons (the only findings)

### V2-1 — LOW — UC4's discriminating call covers `decommit`'s guard only; `decommit_lazy`'s validation base remains undiscriminated by any test on any platform, while the test's doc comment and assertion message both claim the swap class is now caught for the guard PAIR

**Where:** `crates/vmem/tests/smoke.rs:572-591` (the UC4 block: the doc comment
at `:572-582` names "a future `let ps = page_size();` -> `let ps = PAGE;` edit
at the **`decommit`/`decommit_lazy` guards**", the discriminating call at
`:588-590` is `aligned_vmem::decommit(...)` only); `:594-601` (the assert
message's "or, on a host where page_size() > PAGE, that **the validation base
was changed from page_size() to PAGE**" — unqualified as to which guard);
versus the two guards at `crates/vmem/src/lib.rs:1086-1088` (`decommit`) and
`:1154-1156` (`decommit_lazy`), and the lazy-side test calls that exist:
`smoke.rs:569` (`decommit_lazy(base, PAGE, 0)` — rejected by `start >= end`
under EITHER base) and `tests/mock.rs:279` (same shape, same non-discrimination).

Mechanically checked: perform the exact edit the comment names, but in
`decommit_lazy` only — `let ps = page_size();` → `let ps = PAGE;` at
`lib.rs:1154`. Every call the suite makes to `decommit_lazy` is then still
handled identically:

- `smoke.rs:569` / `mock.rs:279`: `start (PAGE) >= end (0)` short-circuits
  before the alignment terms are read, under either base.
- Every other `decommit_lazy` call site in `tests/` (`smoke.rs:398`, `:497`,
  `mock.rs:22`) passes a well-formed, `page_size()`-aligned range (the
  `span/2` offsets are 1/2/... MiB multiples).

So the whole suite — including the macOS CI rows, where the change is live —
stays green, and the test whose own assert message says a base swap would be
caught tells the next reviewer the case is covered. This is verbatim UC4's
finding, shifted from "neither guard discriminated" to "one of the two named
guards discriminated": the ninth instance of the campaign's round-N-fix →
round-N+1-finding pattern, and its mildest (the fix did exactly what UC4's own
"Fix" block specified — that block's example code also called only `decommit`).

**Why the lazy guard is the MORE likely one to be edited next, not the less.**
Item 48's `Next trigger` and its S9 alternative-fix block
(`docs/CORRECTNESS_OPEN_ITEMS.md:2123-2124`) describe the next real code change
planned for this crate's decommit surface: rerouting Darwin's paths around
`MADV_FREE_REUSABLE`/`MADV_FREE_REUSE` — work that lives precisely in
`decommit_lazy`'s guard-and-dispatch neighbourhood. A contributor doing that
work, seeing `recommit` validate against `PAGE` two functions down while the
guard they are editing validates against `page_size()`, is the exact
"plausible-looking unification" actor U7 originally posited — and the eager
guard's new test arm would catch them only if they also touched `decommit`.

**Failure scenario (concrete).** The S9 implementer unifies `decommit_lazy`'s
base to `PAGE` while restructuring the lazy path; suite green everywhere
including `test macos (production)`. On the 16 KiB Apple Silicon host, a
consumer's 4-KiB-aligned lazy decommit is now forwarded to `madvise(2)`, which
rejects the ENTIRE call with `EINVAL` (the all-or-nothing failure mode
`page_size()`'s own rustdoc at `lib.rs:377-383` warns about);
`libc_madvise` discards the return by design (task #719), so the reclaim
silently never happens — RSS stays high with zero diagnostic — and
`decommit_contract_violation_never_reaches_madvise`'s message continues to
assert the swap class is guarded.

**Fix (three lines, same shape as UC4's own fix):** add
`aligned_vmem::decommit_lazy(base, PAGE, 2 * PAGE);` inside the existing
`if page_size() > PAGE` block at `smoke.rs:583-591`, under the same SAFETY
comment (the contract argument is identical). Optionally mirror in
`tests/mock.rs`'s `decommit_silently_skips_contract_violating_offsets` with a
`page_size() > PAGE`-gated `decommit_lazy(base, PAGE, 2 * PAGE)` +
no-`Call::DecommitLazy` assertion — that sibling runs under `--all-features`
on the macOS row too, giving the lazy guard a second, mock-layer oracle for
free. If the current single-function arm is kept instead, `:572-582` and the
assert message should be narrowed to claim only `decommit`'s guard.

### V2-2 — LOW — the Windows `query_os_page_size` NOTE still names `try_reserve_aligned_exact` as a `page_size()` caller; round 8's U1 fix removed that call entirely, so the comment's caller inventory is stale one commit after the fix that invalidated it

**Where:** `crates/vmem/src/lib.rs:434-437` (the NOTE under the
allocation-granularity `debug_assert!` in the Windows `query_os_page_size`
arm):

> NOTE: This debug_assert fires only when `query_os_page_size()` is called,
> which happens on the cold path (decommit/decommit_lazy) **or the Unix-only
> `try_reserve_aligned_exact`**. It does NOT fire on the Windows single-call
> reservation fast path, which uses `WIN_ALLOCATION_GRANULARITY` directly.

versus the current caller set. Mechanically checked: `grep -n "page_size()"
crates/vmem/src/lib.rs` — the only runtime call sites of `page_size()` (and
therefore of `query_os_page_size()`, on first call) in `src/` are `decommit`
(`:1086`) and `decommit_lazy` (`:1154`). `try_reserve_aligned_exact`
(`:2095-2158`) no longer contains the token at all: U1's fix (`e0dbe85`,
task #897, round 8) deleted the `align > page_size() &&` conjunct that was its
one consultation, and its 24-line replacement comment (`:2117-2140`) documents
exactly that. The NOTE was written in the task #848 era, was accurate then
(as a crate-wide caller inventory; the Unix-only caller could of course never
reach this *Windows* arm, which the "Unix-only" qualifier already conceded),
and became false in the same round whose closing commit is otherwise this
review's only diff — the identical staleness class as round 8's own U3
(an anchor invalidated by its own commit), one notch milder because it is a
code comment rather than a sync-contract anchor.

**Failure scenario.** Item 50's U11 `Next trigger` names the
`sanitize_page_size` extraction as the remaining work on exactly this
function's neighbourhood. The implementer doing that extraction reads this
NOTE to enumerate when the guard/debug_assert can fire, concludes the reserve
fast path still depends on `page_size()`, and either re-derives U1's
already-removed dependency from scratch (the cost) or — worse — carries the
false dependency into the extracted function's own documentation, recreating
in prose the coupling U1 just removed from code.

**Fix:** one clause — drop "or the Unix-only `try_reserve_aligned_exact`"
(and optionally note "since task #897 the reserve fast path no longer consults
`page_size()` at all"). The rest of the NOTE stays accurate.

### V2-3 — INFO — the UC4 hunk's trailing comment still says "both calls were rejected" above a test body that now makes three calls on the hosts where the new arm is live

**Where:** `crates/vmem/tests/smoke.rs:603-605`:

> // `base` was never actually decommitted (**both calls** were rejected before
> // any OS effect) -- still a live reservation, released exactly once via
> // `r`'s own Drop here.

On any host where `page_size() > PAGE` (the macOS CI runner — the only place
the arm currently executes), the body issues THREE `decommit`/`decommit_lazy`
calls, all rejected. The comment predates the UC4 hunk and was not updated by
it; its substantive claims (nothing was decommitted; exactly-once release via
Drop) remain true for all three calls, so this is a word, not a wrongness —
recorded because it sits six lines under the round-8 closing commit's own
edit, and because V2-1's fix will touch this exact block anyway ("all calls
above" is the two-character-cheaper wording that stays correct under any
future arm count).

---

## Checked and explicitly NOT findings

Recorded so round 10 (or whatever replaces it — see the final section) does not
re-derive them.

- **The UC4 arm's own correctness, both directions.** Under the correct guard
  on a 16 KiB host: `start = PAGE` is not a `page_size()` multiple → rejected,
  contents preserved, `attempts` stays 0 — confirmed by the real macOS run
  (job `94496764432`, test ok). Under the named swap: forwarded, `attempts`
  becomes 1, assert fires with a message that names the cause. The gate
  `page_size() > PAGE` also correctly makes the arm a no-op on 4 KiB hosts
  (where no offset can discriminate the bases) rather than a false failure.
  The SAFETY comment's claim that `PAGE` is not a multiple of `page_size()`
  under the gate is arithmetically right (both powers of two, `PAGE` strictly
  smaller). V2-1 is about the arm's *scope*, not its content.
- **Item 50's seven `file:line` citations, re-resolved post-`b39882b`.**
  Counters `lib.rs:206-252` (Windows statics at `:239`/`:252`), retry-counting
  claim `:233-236`, two-call claim `:244-249`, accessors `:282-330`, guard
  `:390-406` (acceptance test `:400`), `query_os_page_size` arms `:409-445`,
  `PAGE_SIZE_CACHE` `:168` — all still correct; `b39882b` touched no `src/`
  file, so no shift occurred.
- **Item 43's U1-appended sentence** (`docs/CORRECTNESS_OPEN_ITEMS.md:1926-1940`)
  reads past-tense and coherently against the older decommit-rounding text
  above it; no UC3-shape staleness.
- **`windows_reserve_commit` counter docs vs code, re-traced.** Single-call
  path increments once on the ordinary success (`lib.rs:1645-1646`) and once
  on the large-page-retry success (`:1634-1635`) — "a second syscall but still
  counted as 1" is accurate; the retry correctly returns `granted_huge =
  false` (`:1636`) while the non-retry success returns
  `extra_commit_flags != 0` (`:1649`), reached only when the flagged call
  itself succeeded. Two-call path: ordinary-retry returns `false` (`:1718`),
  the final success returns the requested flag with the documented
  "unreachable in practice for MEM_LARGE_PAGES" note (`:1736-1739`). All
  consistent with W2/QC-era conclusions.
- **U4's replacement SAFETY proof** (`lib.rs:1624-1625`) remains accurate for
  the fresh `VirtualAlloc(NULL, …, MEM_RESERVE | MEM_COMMIT, …)` retry; the
  genuine two-call sibling (`:1711`) still describes a real live reservation.
- **`try_recommit`/`try_commit_range`'s empty-range-before-alignment ordering**
  (`start == end` returns `Ok` even for a misaligned equal pair, e.g.
  `(1, 1)`): matches both functions' documented precedence ("a genuinely empty
  range (`start == end`) is a no-op returning true; any OTHER contract
  violation …") — the docs state the empty case first, the code checks it
  first. Not drift.
- **Overflow discipline, re-checked at every wrap-capable site:**
  `size.checked_add(align)` on both backends (`:1652-1654`, `:1998-2000`),
  `align_up_addr`'s `checked_add` (`:2649-2653`), `leak_zeroed_pages`'s
  round-up with `?` propagation (`:1518`), both fit computations'
  `checked_add` chains, and both `len = end - start` sites preceded by their
  ordering guard. Unchanged from round 8's audit.
- **`fault_injection`'s atomics** — unchanged since #718/#775; the module doc's
  third-hazard scope note (`fault_injection.rs:47-57`) still accurately
  declares the disarm-vs-rearm race out of scope, and
  `fail_next_is_atomic_under_concurrent_callers`'s `armed == calls / 2` oracle
  is still the two-sided post-#775 shape.
- **`mock.rs`'s thread-local isolation** (`std::thread_local!` at `:201-208`)
  — still no shared state across libtest's parallel threads; the smoke-side
  counter tests still all take `SERIAL` (`smoke.rs:180`, `:250`, `:390`,
  `:473`, `:552`) and `libc_madvise` remains the sole incrementer of the
  madvise counters, reachable only from `decommit_pages_impl`'s two arms.
- **`error.rs` in full** — the three-way classification, the
  `Option<u32>`-typed code, the `io::Error` bridge (`from_raw_os_error` /
  `InvalidInput` / `other`), and the immediate-capture timing contract are
  consistent between code, docs, and `tests/vmemerror_io_bridge.rs` +
  `vmem_error_kinds_are_distinguishable` at every site. Nothing new.
- **README/`lib.rs` publish surfaces after U6/U9** — the WSL2 attribution
  matches item 46's card verbatim (34.4/46.7/56.7, "30-run aggregate",
  kernel/ASLR caveat) on both surfaces; the provenance paragraph's scope is
  back to "in the public API" with the honest `tests/` caveat, and the
  remaining 8 `as usize` casts in `tests/` are exactly the set U9/UC recorded
  (shifted by UC4's +22 lines in `smoke.rs`, which cites no line numbers
  anywhere durable — checked).
- **Structure / CLAUDE.md conventions.** No inline `#[cfg(test)] mod tests` in
  `src/`; no `mod.rs`; zero runnable doctests (re-confirmed by an explicit
  `cargo test --doc` run: 0 tests); every illustrative snippet is a
  ` ```text ` fence; the four-file-crate vs "single-file seam crate" question
  remains settled per R13. The crate-level `#![allow(unsafe_code)]` is the
  sanctioned tier-1 seam declaration.
- **Semver / API surface.** `b39882b` changed no public item, no `#[cfg]` on
  any shipping item, and `Cargo.toml` is untouched since round 7. The
  `alloc-lazy-commit` one-release compat alias remains a documented decision
  awaiting 0.3.0, not a defect.
- **CI coverage.** No new gap beyond the standing one (no Linux row runs
  `bench-internals` against the real non-`mock` backend — item 48's S4
  remainder, now with three named beneficiaries per UC6). The macOS row
  remains the only Unix + `bench-internals` + non-`mock` row, which is why
  V2-1's fix targets it.
- **Round-9's own obligations under items 1 and 2, stated so they are not
  reproduced as findings next round:** whatever pass closes THIS round must
  (a) write the round-9 CHANGELOG entry in the same closing task — item 1's
  proposed standing rule is still awaiting the human decision, and rounds 6–8
  all failed the within-round catch — and (b) commit this review doc, per the
  item-2 convention this campaign settled last round. A ninth CHANGELOG
  recurrence is otherwise the single most predictable finding of any round-9
  closing review.

---

## Categories with nothing to report

- **Memory safety / UB — null, fifth consecutive round.** Every `unsafe` block
  in `src/lib.rs` was read against its call site this round (the whole file
  was re-read, all 2,653 lines). No safe `pub fn` accepts a raw pointer at
  all; the seven `bench-internals` accessors are argument-less `AtomicU64`
  reads; provenance discipline (`.addr()`/`.with_addr()`) is intact at all six
  derivation sites on both native backends; `Send`-not-`Sync` on `Reservation`
  still matches its documented reasoning. The one `unsafe`-adjacent change
  since round 7 (U1) *removed* a conditional around an existing check.
- **Performance — null, ninth consecutive round, re-derived not inherited.**
  Checks run this round: (1) syscall counts per entry point unchanged —
  Windows single-call = 1 (retry +1, still counted once), two-call = 2 (+1
  best-effort), Unix fast-path hit = 1, miss = 3, decommit/decommit_lazy = 1
  `madvise`; (2) U1's now-unconditional alignment test costs one
  `AND`+branch after an `mmap` syscall, and the conjunct it replaced was
  measured (#849, 480/480) to save zero syscalls — nothing to reclaim; (3)
  `page_size()` remains a single relaxed load after first call, invoked once
  per decommit-family call and nowhere on the reserve paths (post-U1); (4)
  every counter, its storage and its `use` stay feature-gated — a plain build
  carries no extra instruction; (5) the design space remains covered by the
  two filed ideas (`docs/perf/ALIGNED_VMEM_VIRTUALALLOC2_VA_OPTIMIZATION_OPPORTUNITY.md`
  for Windows `VirtualAlloc2`/BSD `MAP_ALIGNED`; item 46's S12 mmap-hint retry
  with its T10 plumbing note for Linux/Darwin) — no third mechanism emerged
  from this re-read, and nothing in this crate sits on an allocation hot path
  (reservation is a per-segment cold path).
- **Error contracts** — no drift found anywhere between `VmemError`'s kinds,
  their docs, the bridge, and their tests.

---

## Recommended order

1. **V2-1** — one `decommit_lazy(base, PAGE, 2 * PAGE)` call inside the
   existing gated block (plus, optionally, the mock-layer mirror). Do it
   BEFORE any S9/item-48 lazy-path work starts — that work is the realistic
   actor for the uncovered swap.
2. **V2-2** — one clause deleted from the `query_os_page_size` NOTE. Cheap,
   and it de-stales the exact neighbourhood item 50's U11 extraction will
   visit.
3. **V2-3** — two words ("both calls" → "all calls above"), naturally folded
   into V2-1's edit since they touch the same block.
4. **The round-9 CHANGELOG entry + committing this doc** — per items 1 and 2;
   see the obligations bullet above. This is process, not a finding, but it is
   the difference between this round closing clean and a round-10 review
   opening with the tenth recurrence.

Nothing here is publish-blocking for 0.2.0 (task #658): V2-1 is test-side,
V2-2/V2-3 are comments, and no packaged surface changed.

---

## The explicit verdict this round was asked for: pivot, with the argument

Round 8 argued diminishing returns with a number and predicted a ninth read
would find nothing the reading methodology can reach. **This round is the
empirical test of that prediction, and the prediction held.** Stated with the
same honesty the brief demanded:

**What a full ninth pass over the crate actually produced:** zero pre-existing
code defects, zero safety findings at any severity, zero performance findings
(ninth consecutive null), zero contract/doc drift anywhere in code that
existed before round 8 — and exactly two LOW findings plus one INFO, all three
of them in text written by round 8's own closing commit, one commit ago. The
find curve, extended:

| Round | Code-relevant findings | Where they lived |
|---|---|---|
| 1–3 | many, incl. real bugs | the crate itself |
| 4–5 | several | the crate's contracts/API surface |
| 6–7 | several | the world changed (real macOS CI arrived) |
| 8 | 1 MEDIUM (U1) + citation debris | one unexamined direction, 3× previously cleared |
| **9** | **0 in the crate; 2 LOW + 1 INFO in round 8's own one-commit-old remediation** | **the campaign's own tail** |

A review campaign whose entire yield is defects in its own previous
remediation diff has become a self-auditing loop. That loop is not worthless —
V2-1 is a real coverage gap with a realistic actor — but it no longer needs a
2,653-line full-crate read to produce: a diff-scoped review of each closing
commit (a ~200-line read) would have found all three of this round's findings,
and the citation-resolver script round 8 sketched would have found V2-2's
class mechanically.

**Every remaining genuinely-unknown answer is blocked on infrastructure, not
reading — verified against both indexes this round:**

1. **A Linux CI row running `bench-internals` against the real (non-`mock`)
   backend** — now has FOUR named beneficiaries: item 48's S4 remainder (the
   Linux madvise oracle), item 50's U10 template, U1's fast path's only
   observable (UC6), and V2-1's fix arm gaining a second live host
   (any 64 KiB-page or hugepage-configured runner; on standard 4 KiB Linux the
   arm stays a no-op, but the eager/lazy attempt-counting assertions run).
2. **Item 41's `cargo miri test -p aligned-vmem` CI step** — also the thing
   that would make the 8 remaining `tests/` `as usize` casts matter or not.
3. **Item 43's BSD half** — needs a runner; no amount of reading resolves an
   empirical constant.
4. **The ~50-line citation resolver** (round 8's sketch) — would have caught
   U2/U3/U8 in round 8 and V2-2's class this round, for free, forever.
5. **Item 50's U11 `sanitize_page_size` extraction** — one small refactor that
   converts a structurally-untestable guard into a testable pure function.

**Recommendation:** fix V2-1/V2-2/V2-3 in a small closing pass (with the
CHANGELOG entry and this doc committed, per items 1/2), then END the
scheduled full-read campaign. Replace it with event-driven review: (a) a
diff-scoped zero-trust review of any commit that touches `crates/vmem`
(which the workspace's existing per-phase review convention already
requires), (b) the infrastructure items above in whatever priority the
maintainer assigns — the Linux `bench-internals` row is the highest-leverage
single item, four beneficiaries for one CI row — and (c) one final
pre-publish pass tied to task #658's 0.2.0 go-ahead, which is a different
kind of review (packaging surface, docs.rs render, README-vs-crate parity)
than another read of `lib.rs`. A null result was declared a legitimate
outcome by this round's own brief; this is that result, honestly measured,
with the two follow-ons the null still owed.
