# Thirteenth independent pre-publication review — `numa-shim` @ `a848408`

**Author:** `@oh` (Opus, effort=high). **Reported:** 2026-08-24 10:24:34 Europe/Berlin.
**Revision reviewed:** `a84840824f6e3b5a0a802fe27358489d3d7a68e3` (`main`, local **and**
`origin/main` — identical, verified by `git rev-parse` on both), working tree clean apart
from two untracked `docs/checkpoints/*.md` files.
**Mode:** READ-ONLY with respect to the repository. No sub-agents. No file in the repo
edited, no `git` write command run.

**Three exceptions to "static", all load-bearing for this verdict:**

1. **I ran `cargo` this time**, unlike the ninth through twelfth reviews — the brief for
   this round explicitly authorises it. Every invocation used an isolated
   `CARGO_TARGET_DIR` **outside the repository**
   (`%TEMP%/nsr13-target`, `%TEMP%/nsr13-root`, `%TEMP%/nsr13-vmemcheck`) and `--locked`,
   so the shared `target/` and `Cargo.lock` were not perturbed; `git status --short`
   after the whole run is byte-identical to before (two untracked checkpoint files, nothing
   else). This is what lets me settle **by execution** the two questions the twelfth review
   could only predict: whether T4's corrected invocation actually works and yields 33, and
   whether T2's corrected invocation actually runs tests rather than an empty binary.
2. I queried the GitHub Actions API (`gh run view 32702983897 --json …`, plus the raw log
   of job `97358141466`) to confirm CI on the exact landing SHA at job level and to
   cross-check the 33-test figure on Linux against my Windows run.
3. One read-only crates.io registry API query (`aligned-vmem` version list).

**Toolchain:** `rustc 1.97.0 (2d8144b78 2026-07-07)` / `cargo 1.97.0 (c980f4866 2026-06-30)`,
host `x86_64-pc-windows-msvc`.

**Scope:** the review the campaign's standing loop calls for after tasks #1293–#1298
landed against the twelfth review's five conditions and its informational findings
(`docs/reviews/2026-08-24-090217-numa-shim-publication-readiness-review-oh.md`, item 103).
It re-verifies each of T1/T2/T3/T4/T5/T6/T7/T11/T13/T15/T16 **against current source and
by execution, not against the fixing tasks' own commit messages**, hunts for the
incomplete-rollout / doc-contract-mismatch / stale-index classes this campaign keeps
producing, and asks whether #1262 is now both the correct next step and a sufficient one.

**Finding IDs:** ninth used `F1–F8`, tenth `N1–N12`, eleventh `E1–E11`, twelfth `T1–T16`.
This one uses **`U1–U12`**. Severity uses this campaign's P0–P3 scale.

**Filename ASCII-only**, matching the convention the fifth and eighth audits adopted for
`scripts/verify-commit-prefixes.mjs` compatibility.

---

## 0. Verdict

**CONDITIONAL GO**, on **three conditions, none of which is a code change** — down from
the twelfth review's five, and for the first time in this campaign **not one of them is a
finding about the crate's own source**. All three are about making the *release artefact*
tell the truth about the *release process* that produces it.

**No P0, no P1. No UB, no soundness hole, no correctness defect.** The whole wave
`1afed26..a848408` is provably free of semantic change to the crate:

```
git diff 1afed26 a848408 -- crates/numa-shim/src/lib.rs \
  | grep -E '^[-+]' | grep -vE '^[-+]{3}' | grep -vE '^[-+]\s*(///|//|$)'
→ (empty)
```

Ten added doc-comment lines, zero non-comment lines. The other four touched files are
`ci.yml` (one comment), two design docs, and one index file. There is no out-of-scope edit
anywhere in the wave.

**All eleven targeted findings are addressed. Nine of them completely; two partially:**

| Finding | Owning task | State at `a848408` | Verdict |
|---|---|---|---|
| T1 — `mock` feature named in shipped rustdoc | #1293 | `src/lib.rs:319` now "Under the `numa_shim_mock` cfg…", byte-parallel to `:287`'s "or under the `numa_shim_mock` cfg…" | **CLOSED** |
| T2 — `PHASE_NUMA_DESIGN.md` false gate/invocation | #1294 | `:435-442`; gate text matches `tests/numa_cache_invalidation.rs:33-37` exactly; **invocation executed: 2 tests ran, both passed** | **CLOSED** (execution-verified) |
| T3 — `NUMA_TESTING_OPTIONS.md` Phase-1 design | #1294 | `:216-219` SUPERSEDED banner; covers all four stale `feature = "mock"` sites (`:227`,`:244`,`:250`,`:254`), all inside §Phase 1 (`:214-276`) | **CLOSED** |
| T4 — dead Phase-1 gate invocation | #1295 | `:451`, `:485` corrected; **command re-derived and executed: exactly 33 tests, 6 binaries**; old command fails loud | **CLOSED** (execution-verified) |
| T5 — item 100's F5 stale | #1296 | addendum at `:422` records DECIDED/EXECUTED; **Status line `:396` and finding line `:402` unchanged and still say "decision pending"** | **PARTIAL** → **U2** |
| T6 — item 102's C1 stale | #1296 | addendum at `:489` records DISCHARGED; **Status line `:426` unchanged; item 100's Status line `:396` also asserts C1 unreached and has NO addendum correcting it** | **PARTIAL** → **U2** |
| T7 — no package-gates job | #1297 | `cargo publish --dry-run -p numa-shim` re-run by me: 15 files, verify-compile OK, aborts at upload. **Plus the gap #1297 left: I built and tested the packaged tarball with `--features vmem-integration` against REGISTRY `aligned-vmem 0.2.0` — clean** (§6) | **MITIGATED** → **U7** (re-run post-bump) |
| T11 — waiver SHA placeholder unlisted | #1296 | in the `:422` addendum **and** in #1262's TaskList description; the durable copy exists | **CLOSED** |
| T13 — `Resolved(u32)` `#[non_exhaustive]` | #1298 | `src/lib.rs:291-297`; precedent at `:149-163`/`:167-177` is genuine and architecturally parallel | **CLOSED** → **U8** (INFO) |
| T15 — last `feature = "mock"` string | #1294 | `ci.yml:2649`; sweep over all tracked `*.toml`/`*.rs`/`*.yml`/`*.mjs`/`*.json` now returns **zero** hits | **CLOSED** |
| T16 — stale `Cargo.toml:914` pin citation | #1296 | pin confirmed at `Cargo.toml:933`, recorded in the `:422` addendum and in #1262's description | **CLOSED** |

**CI is green on the exact landing SHA, verified at job level, not from the run-level tick.**
`gh run view 32702983897` → `headSha: a84840824f6e3b5a0a802fe27358489d3d7a68e3`,
`conclusion: success`; job-level enumeration → **38 `success`, 3 `skipped`, 0 `failure`,
0 `cancelled`**. The three skips are the schedule-only jobs. All four `numa-shim-*` jobs
ran; the Linux job's mock rows produced 7 `mock_dispatch` + 5 `node_resolution` tests with
the sentinel greps matching, and its `vmem-integration` row ran `reserve_on_node_returns_valid_span`.

### The three conditions

1. **U1 + U5 (P2) — the release's own last step falsifies text that ships to crates.io,
   and #1262's checklist does not cover it.**
   `crates/numa-shim/CHANGELOG.md:21-23` — inside the consumer-facing "NUMA gate
   verification caveat" the owner created specifically to be honest — states
   *"Phase 1 (mock dispatch) passed (31/0 at `c427dd6`, task #1279; **the final pre-tag
   re-run is still owed** …)"*. #1262's mandated last step is precisely that re-run, at
   33/0 on the release SHA. So the published 0.2.0 CHANGELOG would understate its own
   verification and describe an obligation it had already discharged. The same 31/0 figure
   and the same "still owed" sentence are in the waiver the CHANGELOG points consumers at
   (`docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md:38-44`), whose SHA placeholder
   #1262 is already required to fill. **Fold both into #1262: fill the SHA, update the
   Phase-1 line, update the CHANGELOG caveat — all in the same commit that runs the gate.**
2. **U6 (P3) — the final Phase-1 re-run has no recording plan**, although both prior
   Phase-1 runs produced a dated gate-run report *and* a raw log
   (`docs/NUMA_GATE_RUN_2026-08-23_task1270.md` + `docs/perf/_raw_numa_gate_p1_2026-08-23.log`;
   `…_task1279_phase1_rerun.md` + `_raw_numa_gate_p1_rerun_2026-08-23.log`). #1262's
   description says only "run it, expect 33". Without a third record, F8's Phase-1 limb
   closes on an unrecorded run and CLAUDE.md's raw-log rule ("any report whose verdict
   rests on measured numbers owes raw logs + a summary") is unmet for the one gate phase
   this release actually passes.
3. **U2 + U3 + U4 (P3) — index staleness, fifth consecutive occurrence, and the addendum
   remedy did not stop it.** Item 100's Status line (`:396`) still asserts BOTH that F5 is
   pending AND that C1 is unreached — the C1 half is corrected on **item 102's** card only,
   so on item 100 it stands uncorrected anywhere. Item 103's Status line (`:493`) reads
   *"OPEN — filed fresh, no refresh cycle yet"* while 11 of its 16 findings landed in this
   very wave and its Next-trigger steps (1)–(6) have all executed. And
   `docs/NUMA_RELEASE_GATE.md:220` states as fact *"0.2.0 published 2026-08-23"* — it is
   not published, #1262 has not started, and the date will be wrong too. **Fix by editing
   the Status lines IN PLACE**, which is what #1278 and #1286 did on these same cards, not
   by a fourth appended addendum — see U2 for why the addendum shape is the wrong tool for
   this specific job.

**Still owner-gated, unchanged and correctly so:** F1/#1262 (the version bump) and F8's
Phase 3 in-guest remainder. F8 phases 2/4 stay formally waived for 0.2.0 by #1290's record.

**Is #1262 the correct next step? Yes. Is it sufficient? Not as currently described** —
conditions 1 and 2 are literally about #1262's own final step and belong inside it;
condition 3 is separable bookkeeping that can land before or after. Once conditions 1–3
are folded in, I see nothing else standing between this tree and the tag.

---

## 1. What I read and ran

Full current source of `crates/numa-shim/` (`src/lib.rs`, `Cargo.toml`, `README.md`,
`CHANGELOG.md`). Full diffs of `0db7e75`, `3988935`, `fed749e`, `a848408`, plus
`git diff --stat 1afed26..a848408` and the non-comment filter on `src/lib.rs`.
`docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md`; `docs/NUMA_RELEASE_GATE.md`;
`docs/NUMA_TESTING_OPTIONS.md` §Phase 1; `docs/PHASE_NUMA_DESIGN.md` §Test coverage;
`docs/FEATURE_PROMOTION_STATUS.md:75`. Items 100/101/102/103 in
`docs/correctness-open-items/TRACKED_publish_readiness.md` end to end, plus
`docs/CORRECTNESS_OPEN_ITEMS.md`'s tier list and lookup table. Root `Cargo.toml`'s numa
wiring and pin; `tests/numa_cache_invalidation.rs`;
`.github/workflows/release.yml`'s changelog guard, CI-green guard, test gate and publish
step; `.github/workflows/ci.yml`'s `numa-shim-*` jobs. Task #1262's TaskList description.

**Ran:**

| Command | Result |
|---|---|
| `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim --locked` | **33 passed / 0 failed** across 6 binaries + 0 doc-tests |
| `cargo test -p numa-shim --features mock --locked` | `error: the package 'numa-shim' does not contain this feature: mock` |
| `RUSTFLAGS="--cfg numa_shim_mock" cargo test --features "numa-aware-mock alloc-global internals" --test numa_cache_invalidation --locked` | **2 passed / 0 failed** — not an empty binary |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p numa-shim --features vmem-integration --no-deps --locked` | clean (the docs.rs feature set, per CLAUDE.md's exact-feature-set rule) |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p numa-shim --all-features --no-deps --locked` | clean |
| `cargo package --list -p numa-shim --locked` | 15 files |
| `cargo publish --dry-run -p numa-shim --locked` | packaged, verify-compiled, aborted at upload |
| `cargo build/test --features vmem-integration` **in the packaged tarball** | compiled and passed against **registry** `aligned-vmem 0.2.0` + `bench-scale-tool 0.1.0` |
| `gh run view 32702983897` (+ job log `97358141466`) | 38 success / 3 skipped / 0 failure |
| crates.io API, `aligned-vmem` | `0.2.0` published 2026-08-23T15:13:51Z, not yanked |

---

## 2. Re-verification of #1293–#1298 against source and by execution

### T1 (#1293) — VERIFIED; the two sibling wordings now match

`crates/numa-shim/src/lib.rs:319`:

```
/// - Under the `numa_shim_mock` cfg when the scripted node is [`NO_NODE`].
```

Its sibling, `:286-288`:

```
/// or under the `numa_shim_mock` cfg when the scripted node is not
/// [`NO_NODE`]. …
```

Same mechanism, same phrase (`the `numa_shim_mock` cfg`), correct polarity on each
variant. The brief asked specifically whether the fix *matches* its sibling: it does,
modulo the negation, which is semantically required. Sweep confirmation: zero
`feature = "mock"` strings remain in the tracked build tree —

```
git ls-files '*.toml' '*.rs' '*.yml' '*.mjs' '*.json' | xargs grep -ln 'feature = "mock"'
→ (no output)
```

— closing T15 in the same pass. The surviving `--features mock` strings elsewhere are all
correct: migration instructions for 0.1.0 consumers (`crates/numa-shim/CHANGELOG.md:151,192,193`,
`README.md:102`), a past-tense root manifest comment (`Cargo.toml:715`), and historical
records (the two gate-run reports, `ARCHIVE.md`, `docs/perf/_raw_*`, `docs/reviews/`).

### T2 (#1294) — VERIFIED, and the new invocation was EXECUTED, not merely read

The rewritten `docs/PHASE_NUMA_DESIGN.md:435-442` quotes the gate as
`all(all(feature = "numa-aware-mock", feature = "alloc-global"), feature = "internals", numa_shim_mock)`.
`tests/numa_cache_invalidation.rs:33-37` is exactly that, token for token. The invocation
it now gives is the test file's own header (`:28`).

I ran it. **2 tests, both passed** (`cached_node_amortises_within_a_claim`,
`cached_node_invalidates_across_slot_recycle`). This matters more than the textual match:
T2's whole point was that the *old* instruction produced a silent green on an empty binary,
so a doc fix that merely swapped one unexercised string for another would have reproduced
the defect one level up. It did not.

One prose defect introduced by the rewrite → **U9**.

### T3 (#1294) — VERIFIED, and the banner's coverage is complete for that section

`docs/NUMA_TESTING_OPTIONS.md:216-219` is a four-line SUPERSEDED banner naming task #1288
and the `--cfg numa_shim_mock` replacement. All four stale `feature = "mock"` occurrences
in the file (`:227`, `:244`, `:250`, `:254`) sit inside §"Phase 1" (`:214` to `:276`, the
next `## ` header being `:277`), so the banner governs every one of them. The manifest
pointer this finding was really about — `crates/numa-shim/Cargo.toml:41`, *"see
docs/NUMA_TESTING_OPTIONS.md Phase 1 for the original design note"* — now lands a reader on
a section that announces itself as design history, which is the twelfth review's own
preferred (lowest-cost) resolution.

### T4 (#1295) — VERIFIED three ways: source, execution, and CI cross-check

**Source.** Both index-card sites carry the corrected command
(`TRACKED_publish_readiness.md:451`, `:485`), with the "expect 33, not 31" text preserved.

**Historical reports genuinely untouched** — I did not take the commit message's word:

```
git log --oneline -- docs/NUMA_GATE_RUN_2026-08-23_task1270.md \
                     docs/NUMA_GATE_RUN_2026-08-23_task1279_phase1_rerun.md
→ 663394a (#1279), 9275125 (#1270), bf7e1cb (#1270)
```

No commit from this wave touches either file.

**Execution — I re-derived the invocation rather than trusting the citation.** The gate's
purpose is "run the mock-dispatch suite"; post-#1288 the only activation path is
`RUSTFLAGS="--cfg numa_shim_mock"` (there is no Cargo feature, and `Cargo.toml:52-53`'s
`check-cfg` declares exactly that cfg name). Running it:

| Binary | Tests |
|---|---|
| `unittests src/lib.rs` | 0 |
| `tests/cpumap_parser.rs` | 17 |
| `tests/mock_dispatch.rs` | 7 |
| `tests/node_resolution.rs` | 5 |
| `tests/node_resolution_linux.rs` | 0 (empty on Windows by design) |
| `tests/smoke.rs` | 4 |
| doc-tests | 0 |
| **total** | **33 passed / 0 failed** |

The old command fails loud, confirming this was never a vacuous-green hazard:
`error: the package 'numa-shim' does not contain this feature: mock`. (Minor: that is the
actual message on cargo 1.97.0; both the card and #1295's commit body quote the older
phrasing *"none of the selected packages contains these features: mock"*. Harmless — the
command still fails — but the quoted string is not what this toolchain prints.)

**CI cross-check.** Job `97358141466` on run `32702983897` shows the same six binaries with
the same 0/17/7/5/0/4 split on ubuntu: **33 on Linux too**, for a different reason in the
`node_resolution_linux.rs` cell (skipped by the cfg rather than by the OS). The cards'
predicted 33 is correct on both hosts.

### T5/T6 (#1296) — PARTIAL, and the brief's suspicion is correct

The brief asks directly: *does the addendum pattern actually resolve the staleness, or does
it just add more text?* **On this specific defect, it adds text without fixing the problem
the finding named.** The reasoning, not an impression:

- CLAUDE.md's R34-24 rule makes the Status card **the first visible block** the current-state
  contract, precisely because "the in-session TaskList does not survive a session boundary,
  so a fresh session inherits no memory of prior rounds' flagged-open items — these indexes
  do." The failure mode it names verbatim is "a closed / null / rejected item must NOT look
  active due to a stale header."
- After #1296, item 100's Status line (`:396`) still reads **"F5 writeup landed, decision
  pending"** and still closes with *"What remains open on this item is fully owner-gated:
  F1/#1262 …, F5/#1267 …, F8's final Phase-1 re-run plus phases 2/4, and C1 — push +
  CI-green confirmation on the landing SHA, explicitly NOT owned by any TaskList task
  (requires the human owner's explicit authorization to `git push`, which this session has
  not received for numa-shim)."* Both halves are false. The correction sits 26 lines lower.
- **The C1 half of that sentence is corrected nowhere on item 100.** #1296 put its C1
  addendum on item 102 (`:489`) only. Item 100's Status line independently asserts the same
  false fact and no addendum on that card mentions C1.
- The commit body justifies the append-only shape as "per this file's own established
  convention (matching the existing F5-writeup and F8-waiver addenda)". That conflates two
  different things. Those addenda record **new information** (a writeup; an owner decision)
  — append-only is right for them. Correcting a **stale current-state Status line** has its
  own precedent on these very cards, and it is in-place editing: the Status line itself
  says *"refreshed 2026-08-23 by task #1278 … and refreshed AGAIN by task #1286"*. The
  history-is-not-rewritten convention governs reports and archive narratives, not
  Status cards, which R34-24 defines as current-state by construction.

Net: the durable record now exists (good — a fresh session that reads the whole card
learns the truth), but the round-start reading path R34-24 was written to protect is still
wrong. → **U2**. And the same class recurred on a third card during this very wave →
**U3**.

### T7 (#1297) — VERIFIED, and I closed the one gap the dry-run left

I re-ran it rather than trusting the report: `cargo publish --dry-run -p numa-shim`
packages **15 files, 149.4 KiB (46.6 KiB compressed)**, verify-compiles the tarball, and
aborts at upload. `cargo package --list` shows the expected set (both licences, README,
CHANGELOG, `src/lib.rs`, five test files, the bench, `Cargo.lock`, `.cargo_vcs_info.json`).

**Two things #1297's run did not cover, both named in T7's own text:**

- The verify step builds **default features only**, so the optional
  `aligned-vmem = { version = "0.2", path = "../aligned-vmem" }` — whose `path` is stripped
  at package time (confirmed: the packaged `Cargo.toml:77-79` is `version = "0.2"`,
  `optional = true`, no path) — is never compiled against the **registry** copy. That is
  the exact failure mode T7 §1 described. I closed it: in the extracted tarball, outside
  the workspace, `cargo build --features vmem-integration` pulled and compiled
  `aligned-vmem v0.2.0` from crates.io, and `cargo test --features vmem-integration` then
  compiled `bench-scale-tool v0.1.0` and passed (17 `cpumap_parser`, 7 `smoke` incl. the
  three vmem tests). This exercises the most API-fragile call site in the crate —
  `Reservation::from_raw_parts` at `src/lib.rs:1454`, Windows-only — against the published
  dependency rather than the local path. **Clean.**
- The dry-run ran at **version 0.1.0** (`warning: crate numa-shim@0.1.0 already exists on
  crates.io index`), i.e. not the tree #1262 will publish. → **U7**, non-blocking, because
  `release.yml`'s own `cargo publish` step runs with verify ON and re-does this check on
  the real tree.

### T13 (#1298) — VERIFIED; the framing is defensible, with one caveat worth recording

The brief asks whether "precedent-application, not a new decision" is sound or whether an
owner decision was bypassed. **Sound, on three grounds:**

1. **The precedent is real and cited accurately.** `MockCall::CurrentNode` at
   `src/lib.rs:149-163` carries a long task-#778/F13 note refusing variant-level
   `#[non_exhaustive]` for exactly this reason ("a single scalar return with no plausible
   second field to grow into"), and `MockCall::CurrentNodeResolution` at `:167-177` already
   *extends* that precedent once, in the same words the new paragraph uses ("carries no
   field-level `#[non_exhaustive]` (same reasoning as task #778/F13's note)"). #1298 is the
   second extension, not the first.
2. **The outcome is the conservative one.** It documents the status quo; no attribute was
   added or removed, and `git show --stat a848408` is 1 file, +8/−0, doc comment only.
   A decision that changes nothing and freezes nothing new is not the kind that needs fresh
   sign-off.
3. **The substance holds independently.** `NodeResolution` derives `PartialEq`/`Eq`/`Copy`
   and is a *returned* value; a variant-level `#[non_exhaustive]` would make
   `NodeResolution::Resolved(n)` unconstructible outside the crate, breaking every
   downstream `assert_eq!(res, NodeResolution::Resolved(0))`. The enum-level attribute at
   `:278` still reserves the real growth path — a fourth variant, which is precisely what
   the F4/§4(a) discussion contemplates.

Caveat: the precedent was set for a type inside `#[cfg(numa_shim_mock)] pub mod mock`
(`:111`), which never ships, whereas `NodeResolution::Resolved` renders on docs.rs and
freezes at publish. The conclusion is unaffected; the doc's phrasing has a small
consequence → **U8**.

### T11 / T16 (#1296) — VERIFIED complete

`Cargo.toml:933` confirmed by grep as the live pin (`numa-shim = { path =
"crates/numa-shim", version = "0.1", features = ["vmem-integration"], optional = true }`).
Both the corrected line number and the waiver's `[TO BE FILLED: task #1262's landing SHA]`
placeholder appear in the `:422` addendum **and** in #1262's TaskList description. The
addendum is the load-bearing copy — CLAUDE.md's own note that the TaskList does not survive
a session boundary means a TaskList-only fold would not have counted. It did not have to.

---

## 3. Regression hunt — did this wave break anything?

This campaign's live pattern is "a fix introduces a regression" (#1269→N4/N5; #1276→E3).
Checked deliberately:

- **The crate's code is untouched.** The non-comment filter above returns empty across the
  whole wave. Every dispatch function, the `bind_range` short-circuit, the Windows
  reserve/commit/release chain, the cpumap parser and the `OnceLock` topology cache are
  byte-identical to `1afed26`, which the twelfth review had already proven byte-identical
  to `1043b0e`. The 21 cfg sites (10 `#[cfg(…)]` + 11 `#[cfg_attr(…)]`) are unchanged in
  count and shape.
- **The two new doc paragraphs do not warn.** `RUSTDOCFLAGS="-D warnings" cargo doc` is
  clean under **both** `--features vmem-integration` (the exact
  `package.metadata.docs.rs.features` list, per CLAUDE.md's exact-feature-set rule) and
  `--all-features`. The `MockCall::…` references in the new T13 paragraph are plain
  backticks, not intra-doc links, so no `broken_intra_doc_links` fires — deliberate or
  lucky, it is correct.
- **The ci.yml edit is one comment character-range** inside a `#` line; the file still
  parses (CI ran).
- **The two index insertions shifted line numbers**, which item 103's own citations did not
  follow → **U10**, trivia.
- **Test counts did not move.** 33 under the cfg here and on Linux CI; 2 for the root
  invalidation test; the packaged tarball's suite passes against registry deps.

**Net: no functional regression, and no out-of-scope edit.** For the second consecutive
wave, the fix-breaks-something pattern did not recur. What this wave *did* introduce is one
dangling clause (U9) and one docs.rs cross-reference to a non-rendered type (U8), both
INFO.

---

## 4. What the twelfth review's conditions left open

- **T1, T4, T11, T16 — fully closed**, the first three with execution or grep receipts.
- **T5/T6 — the remedy was applied in a shape that does not discharge the finding's own
  stated concern**; and the class recurred a third time within the same wave (U2, U3).
  The twelfth review floated, without filing, a mechanical guard: "fail a test when a card
  asserts a task is 'pending'/'not started' while that task's commit exists in `git log`."
  Five consecutive occurrences (N9 → E6 → T5 → T6 → U2/U3) is, I think, enough evidence
  that the human-discipline remedy does not hold at this cadence. I still do not file it as
  a requirement — it is a repo-wide process change outside this crate's release — but I
  record that the empirical case for it is now considerably stronger than when it was
  raised.
- **T7 — mitigated and then extended** (§2, §6). The residual is a re-run after the bump.
- **T13 — closed**, with the reasoning independently checked rather than accepted.
- **T8/T9/T12/T14 (#1299) — correctly deferred.** I looked for new evidence that any of
  them should block, per the brief. There is none: run `32702983897`'s four `numa-shim-*`
  jobs all ran real mock tests again (the Linux log shows the sentinel greps echoing and
  matching); the MSRV rows are green; `npm run check` still has no numa-shim row but CI is
  green on the landing SHA. **#1299 stays non-blocking.**

---

## 5. New findings

### U1 (P2) — the release's own final step falsifies a paragraph that ships to crates.io, and #1262's checklist does not include it

`crates/numa-shim/CHANGELOG.md:21-23`:

> …Phase 1 (mock dispatch) passed (31/0 at `c427dd6`, task #1279; **the final pre-tag
> re-run is still owed** per the eleventh review's E1 ordering rule)…

Two statements that #1262 is required to falsify, by its own mandated last step:

- "31/0 at `c427dd6`" — the re-run will be **33/0** at the release SHA (verified by me and
  by CI, §2).
- "the final pre-tag re-run is still owed" — it will have been done, *before the tag*, by
  construction of E1's ordering rule.

This is not an internal note. It sits inside the `### NUMA gate verification caveat (owner
risk acceptance, 2026-08-23)` block — the section whose entire purpose is to tell users
honestly how much verification this release actually got — and it ships in the packaged
crate (`CHANGELOG.md` is in `cargo package --list`). A 0.2.0 that understates its own gate
coverage while pointing users at a "still owed" obligation it already discharged is the
same doc/contract-mismatch class as T1, one file over.

I checked #1262's TaskList description in full: its site list is crate manifest, root pin,
three README pins, the dated CHANGELOG **header**, the waiver's SHA placeholder, the tag,
and the ordering rule. The CHANGELOG's gate-caveat **body** is not in it.

**Fix: fold into #1262, in the same commit that runs the gate** — update `:21-23` to the
re-run's real figure and SHA and drop the "still owed" clause.

### U2 (P3) — item 100's Status line asserts two false facts, and the C1 one is corrected nowhere on that card

`docs/correctness-open-items/TRACKED_publish_readiness.md:396` (the R34-24 first visible
block) still contains, verbatim:

> **F5 writeup landed, decision pending** (#1267 — see this card's F5 addendum below)

and, in the same line:

> …and C1 — push + CI-green confirmation on the landing SHA, explicitly NOT owned by any
> TaskList task (requires the human owner's explicit authorization to `git push`, which
> this session has not received for numa-shim)

F5 was decided by #1289 and is shipped in `CHANGELOG.md:44-55`, `src/lib.rs:601-607`/`:751-757`,
`README.md:142-146`. C1 was discharged by #1291's push and CI run `32696488144`. The F5 half
is corrected by #1296's addendum at `:422`. **The C1 half is corrected only on item 102's
card (`:489`)** — nothing on item 100 mentions it. The finding line `:402` and the F5
addendum's own Status line `:418` likewise still read "pending"/"awaiting owner decision".

**Fix: edit `:396`, `:402`, `:418` in place**, exactly as tasks #1278 and #1286 edited this
same Status line in place (the line documents both refreshes). Keep the `:422` addendum —
it carries the decision's *substance* (that the owner chose a fourth path, not the writeup's
option (c)), which is genuinely new information and belongs appended.

### U3 (P3) — item 103's own Status card is stale, in the wave that acted on it — the fifth consecutive occurrence

`TRACKED_publish_readiness.md:493`: **"Status: OPEN — filed fresh, no refresh cycle yet."**
Its Next-trigger line enumerates steps (1) T1, (2) T2/T3/T15, (3) T4, (4) the T5/T6/T11/T16
card refresh, (5) T7's dry-run, (6) T13 — **all six executed**, by #1293–#1298. Eleven of
its sixteen findings are closed.

This is the same class as T5/T6, on the card that *reported* T5/T6, one wave later. E6's
remedy (refresh in the wave's last commit) failed for item 100/102 because the wave did not
end at the refresh; here it failed because nobody scheduled a refresh of the *reporting*
card at all. I note the orchestrator's stated plan is to file a new item 104 — filing 104
does not fix 103's Status line, and a fresh session reading the `[T]` tier top-down hits
103 before 104.

**Fix: fold into the same in-place card-refresh commit as U2.**

### U4 (P3) — `docs/NUMA_RELEASE_GATE.md:220` states as fact that 0.2.0 has been published

> - [`NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md`](…) — dated owner risk-acceptance
>   record: **0.2.0 published 2026-08-23** with Phases 2/4 outstanding …

0.2.0 is not published; `crates/numa-shim/Cargo.toml:3` is still `version = "0.1.0"`, #1262
is `pending`, and today is 2026-08-24. The waiver itself is scrupulously careful about this
(`:19` carries the SHA placeholder precisely because the release had not happened) — only
the index line in the gate policy overstates. Pre-existing from #1290, not from this wave;
the twelfth review cited `:220` as a link-existence check without reading its wording.
**Fix: one line, "risk-acceptance record for the queued 0.2.0 release"** — and note the
publish date, when it happens, will not be 2026-08-23.

### U5 (P3) — the waiver's Phase-1 line will be superseded by the re-run it demands, and #1262 only fills its SHA

`docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md:38-44` records "Phase 1 (mock dispatch):
PASS — 31/0 passed/failed at `c427dd6`… the FINAL Phase-1 re-run is still owed… **This
record does not waive Phase 1.**" Correct when written. But this is the record *bound to the
published tree* (that is what the SHA placeholder is for) and the record the shipped
CHANGELOG points consumers at (`CHANGELOG.md:31-32`). After the re-run, its Phase-1 line
describes a superseded measurement while claiming to describe the released tree.

**Fix: in the same #1262 commit that fills `:19`, update `:38-44`** with the re-run's
figure, SHA, and the fact that Phase 1 is now satisfied on the released tree. Same edit
class as U1; they should travel together.

### U6 (P3) — the final Phase-1 re-run has no recording plan, unlike both prior runs

Both previous Phase-1 executions produced a dated gate-run report **and** a raw log:

- `docs/NUMA_GATE_RUN_2026-08-23_task1270.md` + `docs/perf/_raw_numa_gate_p1_2026-08-23.log`
- `docs/NUMA_GATE_RUN_2026-08-23_task1279_phase1_rerun.md` + `docs/perf/_raw_numa_gate_p1_rerun_2026-08-23.log`

#1262's description says only *"expect exactly 33 tests"*. Nothing produces a third record.
Consequences: F8's one passing phase would close for 0.2.0 on an unrecorded run; U1's and
U5's corrections would have no artefact to cite; and CLAUDE.md's rule that "any report whose
verdict rests on a number obtained by running something owes raw logs + a summary" is unmet
for the one gate phase this release actually passes. The cost is one short markdown file and
one `tee`d log — the same shape #1279 already produced, well under the 200 KiB tier-1
ceiling.

**Fix: add "record the run as `docs/NUMA_GATE_RUN_<date>_task1262_phase1_final.md` +
`docs/perf/_raw_numa_gate_p1_final_<date>.log`" to #1262's last step.**

### U7 (INFO) — the dry-run that discharged T7 measured version 0.1.0, not the tree that will publish

`cargo publish --dry-run -p numa-shim` emits `warning: crate numa-shim@0.1.0 already exists
on crates.io index`. #1262 will change `Cargo.toml`, `README.md` and `CHANGELOG.md` — all
three are packaged files — so the dry-run's evidentiary value does not transfer to the
post-bump tree. Non-blocking, because `release.yml`'s `cargo publish` step deliberately runs
with **verify ON** ("re-builds the packaged crate standalone — catches packaging mistakes…
before the upload is final"), which repeats the check on the real tree. Recommended anyway:
one `cargo publish --dry-run -p numa-shim` after #1262 lands, before the tag — it costs
seconds and moves the failure from the irreversible path to a reversible one.

### U8 (INFO) — the new `Resolved` rationale cites, on a docs.rs page, two types docs.rs does not render

`src/lib.rs:291-297` (unconditionally public, renders on the 0.2.0 landing page):

> Deliberately carries no field-level `#[non_exhaustive]`, following task #778/F13's
> precedent for `MockCall::CurrentNode`/`CurrentNodeResolution`…

`mod mock` is `#[cfg(numa_shim_mock)]` (`:111`), and docs.rs builds with
`features = ["vmem-integration"]` and no `RUSTFLAGS`, so **neither `MockCall` nor either
variant exists on the published docs**. A reader of `NodeResolution::Resolved` is pointed at
a precedent they cannot look up. (No rustdoc warning fires — the references are plain
backticks, not intra-doc links, and `cargo doc -D warnings` is clean under both feature
sets.) Also worth recording for the file's own history: the cited precedent governs a type
that does not ship, so the extension to a shipping type is slightly wider than "completing
an existing policy" — the conclusion still holds (§2, T13), but a future reader should know
the analogy is not exact. Two cheap options: state the reason self-containedly ("a single
scalar node id has no plausible second field to grow into"), or keep the task reference and
drop the type names. The crate already sets the precedent for bare task references in public
docs (`:121`, "See task #1266, audit finding F4 for background").

### U9 (INFO) — T2's rewrite leaves a dangling clause in the design doc

`docs/PHASE_NUMA_DESIGN.md:435-442` now reads:

> …run it via `RUSTFLAGS="--cfg numa_shim_mock" cargo test --features "numa-aware-mock
> alloc-global internals" --test numa_cache_invalidation` **— for deterministic control of
> `current_node()`'s return value)** proves the invalidation fires at `claim()`…

The trailing "for deterministic control of…" was the tail of the *deleted* phrase
("…which enables `numa-shim/mock` **for deterministic control of**…"); the inserted gate and
invocation orphaned it. The content is correct; the sentence no longer parses. One clause.

### U10 (INFO) — item 103's citations into its own file are off by one after #1296

Item 103's T4/T5/T6 lines cite `TRACKED_publish_readiness.md:450`, `:484`, `:425`. #1296
inserted two lines (at 422 and 497), so those sites are now `:451`, `:485`, `:426`. Purely
mechanical, and the citations were accurate when filed. Recorded because U2's in-place
refresh will touch these cards anyway.

### U11 (INFO, outside numa-shim's scope) — a hand-maintained card count in the index drifted and was silently corrected by accident

`docs/CORRECTNESS_OPEN_ITEMS.md:225` says `TRACKED_publish_readiness.md` holds "(16 cards)".
At `1afed26` the file held **15**; the line already said 16. Item 103's filing made it 16,
so the count is correct today — by coincidence, not by maintenance. No test pins it
(`grep -rn 'publish_readiness' tests/*.rs scripts/*.mjs` → nothing), and CLAUDE.md's own
no-hardcoded-counts convention (task #776/F10) says such a figure should be a self-verifying
command. Flagged once, not filed; it belongs to the index's own hygiene, not this release.

### U12 (INFO, pre-existing) — `docs/FEATURE_PROMOTION_STATUS.md:75` cites `numa-aware-mock` at `Cargo.toml:616`

The definition is at `Cargo.toml:740`. The row's *substance* is correct and current
(#1288 updated it: "the feature alone no longer activates any mock"); only the line number
is stale. Same drift class as T16, in a file nobody has re-checked since.

---

## 6. What I checked and found clean

Stated so this report does not read as if only defects were looked for.

- **The crate's source did not change this wave**, proven by the non-comment filter, not
  by `--stat` alone. Ten doc lines, zero semantic lines, in a 1,600-line file.
- **CI green on the exact landing SHA**, at job level: 38 success / 3 skipped / 0 failure /
  0 cancelled on `a84840824f6e3b5a0a802fe27358489d3d7a68e3`. `HEAD` == `origin/main`.
- **The mock rows are still not vacuous** after the wave: job `97358141466`'s log shows the
  `RUSTFLAGS`-carrying rows producing 7 `mock_dispatch` + 5 `node_resolution` tests with the
  task-#1101 sentinel greps echoing and matching, versus 0 and 0 on the same job's non-mock
  rows, and the `vmem-integration` row running `reserve_on_node_returns_valid_span`.
- **Packaging works, and works further than anyone has checked before.** 15 files; the
  packaged tarball's `Cargo.toml` correctly strips the `aligned-vmem` path to a bare
  `version = "0.2"`; and, built **outside the workspace against the registry**, the crate
  compiles and its tests pass with `--features vmem-integration` on `aligned-vmem 0.2.0` +
  `bench-scale-tool 0.1.0`. That covers `Reservation::from_raw_parts` (`src/lib.rs:1454`),
  `reserve_aligned`, `PAGE` and `page_size` — every symbol numa-shim imports from its
  optional dependency — against the published copy rather than the local path.
- **Registry preconditions confirmed live**, not inherited: crates.io reports
  `aligned-vmem 0.2.0` published 2026-08-23T15:13:51Z, `yanked=false`; `0.1.0` also
  unyanked. `bench-scale-tool 0.1.0` resolved and compiled during the packaged-tarball test.
- **Doc-lint clean in the exact published feature set**, not just a superset — CLAUDE.md's
  fifth-instance rule. `RUSTDOCFLAGS="-D warnings" cargo doc -p numa-shim --features
  vmem-integration --no-deps` (the literal `package.metadata.docs.rs.features` list at
  `Cargo.toml:24-25`) is clean, as is `--all-features`.
- **`release.yml`'s guards would do their job.** The changelog guard resolves the manifest
  via `cargo metadata` (fail-closed on unknown package or missing file), requires exactly
  one `^## \[?0\.2\.0\]?(\]|$| )` section and rejects a header containing "unreleased"
  case-insensitively. `crates/numa-shim/CHANGELOG.md` has exactly two `## ` headers today
  (`:7` `## Unreleased`, `:197` `## 0.1.0 - 2026-06-29`), so #1262's consolidation yields
  exactly one match and no ambiguity. The CI-green guard keys on `github.sha`, not "latest
  on main". The tag-vs-manifest version guard and the test gate both run before the
  irreversible upload, and the publish step keeps verify ON.
- **The root wiring is coherent post-#1288.** `numa-aware-mock = ["numa-aware"]`
  (`Cargo.toml:740`) is a pure marker with a comment that says so; the root's check-cfg list
  declares `cfg(numa_shim_mock)`; `tests/numa_cache_invalidation.rs`'s four-conjunct gate
  runs 2 tests under the documented invocation and compiles to an empty binary without it —
  the fail-quiet-but-documented behaviour #1288 intended.
- **The three README `0.1` pins are still at `:33`, `:36`, `:64`**, unchanged and correctly
  so (they are #1262's).
- **No `cargo` invocation touched the shared workspace.** All three target directories were
  outside the repo, every command carried `--locked`, and `git status --short` after the run
  shows the same two untracked checkpoint files and nothing else — no `Cargo.lock` drift, no
  perturbation of a sibling agent's `target/`.

---

## 7. Recommended order before publish

1. **Condition 3 first, because it is independent and cheap** — one card-refresh commit:
   edit items 100 (`:396`, `:402`, `:418`) and 102 (`:426`) Status lines **in place** to the
   true current state (F5 DECIDED/EXECUTED; C1 DISCHARGED at `1afed26`, CI `32696488144`),
   refresh item 103's Status line (`:493`) and Next-trigger to record #1293–#1298, fix
   item 103's three off-by-one self-citations (U10), and correct
   `docs/NUMA_RELEASE_GATE.md:220` (U4). Keep #1296's `:422`/`:489` addenda — their
   substance is new information and belongs appended.
2. **U9, U8, U12 — three one-line prose fixes**, optional and non-blocking; best folded
   into the same commit so they do not churn the release.
3. **#1262 — the version bump**, with the site list now complete:
   `crates/numa-shim/Cargo.toml:3`, root `Cargo.toml:933` (re-verify — it moved once
   already), `README.md:33`/`:36`/`:64`, the dated CHANGELOG header (consolidating
   `## Unreleased` and resolving the now-empty `### Owner decisions pending` heading, whose
   two bullets both read DECIDED), and the waiver's `:19` SHA placeholder.
4. **F8 Phase 1 — re-run LAST**, on the final pre-tag revision, with
   `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim`. **Expect exactly 33** — now
   confirmed by execution on Windows and by CI log on Linux, not predicted.
5. **Conditions 1 and 2, in the same commit as step 4** — record the run
   (`docs/NUMA_GATE_RUN_<date>_task1262_phase1_final.md` + a `tee`d
   `docs/perf/_raw_numa_gate_p1_final_<date>.log`), then update
   `crates/numa-shim/CHANGELOG.md:21-23` and
   `docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md:38-44` to cite that run instead of the
   superseded 31/0 at `c427dd6`, and drop the "the final pre-tag re-run is still owed"
   clause. This is the step that makes the published caveat true.
6. **U7 — one `cargo publish --dry-run -p numa-shim`** on the post-bump tree, before the
   tag. Seconds; moves any packaging surprise off the irreversible path.
7. **Tag `numa-shim-v0.2.0`**, confirm CI green on the landing SHA read from the remote,
   **then publish.** Phases 2/4 are waived for 0.2.0 by #1290's record; Phase 3's in-guest
   remainder stays open and is already a consumer-facing caveat.
8. **#1299 (T8/T9/T12/T14)** — the CI-hardening set. Unchanged: none blocks the release; I
   looked for new evidence to promote any of them and found none. A `numa-shim-gates` job
   mirroring `aligned-vmem-gates` remains the right long-term fix for T7 and should follow
   the release, not precede it.

---

**Summary verdict: CONDITIONAL GO.** Four of the twelfth review's five conditions are fully
discharged, three of them with execution receipts rather than inference — T4's corrected
invocation really does produce exactly 33 tests, T2's really does run 2 tests instead of an
empty binary, and the packaged crate really does build and pass against the **registry**
`aligned-vmem 0.2.0`, which no gate in this campaign had ever exercised. The fifth (T5/T6)
was answered in a shape that records the truth but leaves the Status lines R34-24 designates
as the current-state contract still asserting the opposite — and the same class recurred, in
this wave, on a third card. **No P0, no P1, no UB, no soundness hole, no correctness defect,
no functional regression, and no out-of-scope edit: the crate's source did not change at all
this wave.** What stands between this tree and a tag is not a defect in `numa-shim` — it is
that #1262's own last step will falsify a paragraph that ships to crates.io (U1), will leave
the gate record it is bound to describing a superseded measurement (U5), and has no plan to
record itself the way both prior runs did (U6). Fold those three into #1262 and refresh the
three stale cards, and I expect the next review to be able to return an unconditional GO.
