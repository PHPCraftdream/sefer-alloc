# `sefer-region` round-2 closing review (read-only, end-to-end)

**Date:** 2026-08-08
**Reviewed range:** `ea52f85~1..HEAD`, HEAD = `9f35ada442d424fdfd2a45d83e9377d0a6e9f47a` (`main`)
**Scope:** the 5-commit round-2 chain — 3 fix commits (#694, #695, #696), the CHANGELOG
gap-closure commit `b463825` (#733), and the checkpoint-files commit `9f35ada` (#734).
**Mode:** read-only. No repository file was modified except this report; `git status
--porcelain` was empty before and after. One throwaway probe was built and run in a scratch
cargo project under `%TEMP%` (path-dependency on `crates/region`, deleted after use) to
independently settle two claims that could not be settled by reading; its verbatim output is
inlined below. `slotmap 1.1.1` was read at source level from the registry cache
(`D:\system_artefact\cargo\registry\src\index.crates.io-1949cf8c6b5b557f\slotmap-1.1.1\src\basic.rs`).

**One scoping note up front:** the brief described *three* post-work commits (a checkpoint
commit, `b463825`, and a checkpoint-files commit). Only **two** exist: `b463825` and
`9f35ada`. Task #732's `/checkpoint` produced `docs/checkpoints/2026-08-08-2340.md` but did
not itself commit — that file (plus the previously-uncommitted `2026-08-07-2221.md`) landed
in `9f35ada` under task #734. The checkpoint file's own "Repo state" block records
`?? docs/checkpoints/2026-08-07-2221.md`, which is exactly consistent with that sequence.
Not a defect; the round chain is 5 commits, not 6.

---

## 1. Commit-by-commit: does each diff match its own message and its task's claim?

**Two of three match fully; one (`ea52f85`) delivers roughly half of what its own subject
line claims.** All three diffs were read line by line against their messages and against the
audit finding each closes.

**`ea52f85` (#694) — the order-agnosticism claim does not hold for the assertions it
retained.** The half of the rewrite that concerns *which* ids were dropped is genuinely
order-agnostic and genuinely computed, not assumed: `survivor_ids` is collected from
`r.iter()`/`sr.read().iter()`'s actual output into a `HashSet`, the bomb is asserted absent
from that set, every member of the set is resolved through `handles[survivor_id]` and checked
to carry the matching `id`, and the dropped set is derived as the *complement* (the
`for (i, handle) in handles.iter().enumerate() { if !survivor_ids.contains(&i) { … } }` loop),
not hardcoded. No positional assumption survives anywhere in that block. Both scenarios
(`Region` and `SyncRegion`) got the identical treatment; the total-accounting strength the
brief asked about is preserved exactly (`drop_count == 3`, `len() == 2`, plus the unchanged
end-of-test `== 6` totals and the reusability checks).

But `drop_count == 3` and `len() == 2` **are themselves the drain-order oracle** — they are a
direct function of the bomb's *ordinal position* in the visitation sequence, which is the
precise thing §D1a filed as unspecified. Verified at source: `SlotMap::clear` is
`{ self.drain(); }` (`basic.rs:615`), `drain()` returns `Drain { cur: 1, … }` (`:640`), and
`Drain::next` walks `cur` upward through the slot array (`:1109-1124`) with `Drain::drop`
draining the remainder via `for_each` (`:1133`). Today the bomb is the 3rd occupied slot, so
2 clean drops precede it and 2 values survive. Verified empirically (scratch probe, verbatim
— all three arms use the *same* slotmap 1.1.1, varying only the slot layout, which is a
faithful proxy for "the drain visits values in a different order"):

```text
A committed-test layout : insert_order=[0, 1, 2, 3, 4] clear_panicked=true drop_count=3 len=2 survivors=[3, 4]
   -> committed test's assertions would be: drop_count==3 ? true ; len()==2 ? true
B bomb in LAST slot     : insert_order=[0, 1, 3, 4, 2] clear_panicked=true drop_count=5 len=0 survivors=[]
   -> committed test's assertions would be: drop_count==3 ? false ; len()==2 ? false
C bomb in FIRST slot    : insert_order=[2, 0, 1, 3, 4] clear_panicked=true drop_count=1 len=4 survivors=[0, 1, 3, 4]
   -> committed test's assertions would be: drop_count==3 ? false ; len()==2 ? false
```

In arms B and C the crate's own order-free partial-clear contract is *perfectly upheld* —
everything visited before the bomb dropped, the bomb dropped, nothing after it did, the
region stayed consistent — and the committed test still goes red. That is verbatim the
false-red failure mode §D1a exists to eliminate. The audit's own suggested invariant,
`drop_count + len() == 5`, holds in all three arms (3+2, 5+0, 1+4). See finding **A**.

**`0373b28` (#695) — complete, no strays.** All three occurrences named in the message are
qualified with consistent wording (`crates/region/README.md:13`, `src/lib.rs:8-11`,
`src/region.rs:14-17`). A workspace-wide sweep for `never escape` and for `DefaultKey` across
`*.rs` and `*.md` found no fourth occurrence of the claim anywhere: the only other
`never escape` hits are the new CHANGELOG bullet describing this very fix, three checkpoint
files, the audit report itself, and `src/concurrent/hand.rs:232` ("the reference is bound to
`f` and never escapes" — the root allocator, unrelated). `handle.rs`'s own type doc never made
the absolute claim, so it needed no edit. Doc-only as claimed; `cargo doc` re-run below.

**`1f962e5` (#696) — matches, and its counterfactual reproduces exactly.** The new
`region_reserve_reuses_freed_slots_on_churn` is **not** `#[ignore]`d and genuinely runs — it
appears by name in my own `cargo test` output in both profiles (see §3), inside
`coverage_gaps.rs`'s 19-test binary. The probe was correctly left observational: both
`assert!`s were removed while the `captrack::registry::record_sample` calls stayed, and both
removal sites gained a pointer to where the assertion now lives. The commit's counterfactual
numbers are not merely plausible — I reproduced them exactly outside the repo:

```text
churn(refill=500): after_remove=1023 after_refill=1023 assertion(after_refill<=after_remove)=true
churn(refill=700): after_remove=1023 after_refill=2047 assertion(after_refill<=after_remove)=false
churn(refill=600): after_remove=1023 after_refill=2047 assertion(after_refill<=after_remove)=false
churn(refill=523): after_remove=1023 after_refill=1023 assertion(after_refill<=after_remove)=true
churn(refill=524): after_remove=1023 after_refill=2047 assertion(after_refill<=after_remove)=false
```

`after_remove=1023, after_refill=2047` at refill=700 is byte-for-byte the pair the commit
message, the CHANGELOG bullet, and the checkpoint all report. The test is genuinely
discriminating for the filed bug class (a free-list bypass), with a measured slack of 23
inserts before it fires — see finding **C**, and finding **B** on the in-code comment, which
describes a *different* counterfactual from the one that was actually run.

**The two deleted runtime tests were genuinely redundant; nothing extra was lost.**
`handle_is_send_even_when_t_is_not_send` compiled `thread::spawn(move || { let _ = h; })` for
`h: Handle<NonSendType>`, which requires exactly `Handle<NonSendType>: Send`;
`handle_is_sync_even_when_t_is_not_sync` compiled a `thread::spawn` over
`Arc<Handle<NonSyncType>>`, requiring `Handle<NonSyncType>: Send + Sync`. Both are strictly
implied by the surviving `const _: () = assert_send_sync::<Handle<NonSendType>>()` and
`::<Handle<NonSyncType>>()` (`handle_static_asserts.rs:50,57`), which additionally fire at
*compile* time rather than as a runtime green tick. Their runtime bodies asserted nothing
(`let _ = h;` / `let _ = *h_arc_clone;`). The only thing exercised nowhere else is the
incidental `Region::<NonSendType>::new().insert(…)` construction — `Region<T>` carries no
`Send`/`Sync` bound on `T`, so this was never a guarded claim. `NonSendType`/`NonSyncType`
both remain live through the consts (clippy `-D warnings` is clean, so no `dead_code` was
introduced). **`handle_layout_matches_expectations` is present and byte-for-byte unchanged**
— `1f962e5`'s diff of that file is a pure 38-line deletion with zero additions, and the
function still sits at `handle_static_asserts.rs:77-87` with its original body and comment.

## 2. Were any rust-intel audit findings for this crate silently dropped?

**No — all 12 findings are accounted for, and 11 of 12 are fully discharged.** I walked the
audit end-to-end (7 MEDIUM, 5 INFO, plus its 20-item post-flight 🔴 inventory) against the
landed commits, and verified each landing in the tree rather than trusting the audit's own
task-filing note:

| Audit finding | Task | Landed | Verified in tree |
| --- | --- | --- | --- |
| MEDIUM §B17 + §F2(reentrancy) + §B1b | #687 | `5e4244f` | `sync_region.rs:45-58` `## Reentrancy` section covers both the `Clone`/`Drop`-under-lock case and the guard-held one-shot nesting, and cites std's reacquisition behavior; §B1b's doc-line option taken verbatim at `:101-102` and `:111-112` ("a deliberate, stable API commitment; migrating the internal lock implementation in the future would be a breaking change") |
| MEDIUM §B26 + §B7 (`reserve`) | #690 | `df16693` | `region.rs:137` `.checked_add(additional)` |
| MEDIUM §C1 (`repr(transparent)`) | #691 | `a243c38` | `handle.rs:22` `#[repr(transparent)]` |
| MEDIUM §D1 (`catch_unwind` payload) | #692 | `ed008a5` | `coverage_gaps.rs:487,503` use the `catch_panic_message` helper instead of bare `is_err()` |
| MEDIUM §D3 (single-threaded only) | #693 | `89913c6` | `coverage_gaps.rs:632-663` — `THREADS = 4`, `OPS_PER_THREAD = 200`, `thread::scope` |
| MEDIUM §D3 (no release-profile run) | #693 | `89913c6` | `.github/workflows/ci.yml:774` `cargo test -p sefer-region --release --no-fail-fast` |
| MEDIUM §F1 (poisoning policy) | #688 | `ec59520` | reworded to write-mode-only poisoning |
| INFO §B26 (`with_capacity`) | folded into #690 | `df16693` | `region.rs:90` `.checked_add(1)` — the fold the audit asked for ("align `with_capacity` and `reserve` on one policy in the same pass") actually happened |
| INFO §D1 (probe-only churn assertion) | #696 | `1f962e5` | ✅ |
| INFO §D1 (redundant runtime Send/Sync) | #696 | `1f962e5` | ✅ |
| INFO §F2 (Debug renders the key) | #695 | `0373b28` | ✅ |
| INFO §D1a (drain-order pin) | #694 | `ea52f85` | **partial** — see §1 and finding A |

The audit's post-flight 🔴 inventory is 20 entries of "N/A / justified / pattern absent" with
no action requested; nothing in it is an unfiled item. Nothing in the report is left dangling
without a task. The only shortfall is §D1a, which is addressed-but-not-closed rather than
dropped.

One **documentation-discoverability residual** worth naming without calling it a round-2
defect: §B17's fix text asked for the deadlock hazard to be documented on `SyncRegion`'s
type-level doc *"and to the one-shot methods"*. The type-level `## Reentrancy` section landed
and is thorough, but of the seven one-shot methods (`insert`, `remove`, `contains`, `len`,
`is_empty`, `clear`, `get_cloned`) only `remove` cross-references it — and it does so because
it is the documented *exception* (drop-outside-lock), not as a hazard warning. That belongs to
#687 (prior round), is a discoverability gap rather than a factual error, and is out of
round 2's scope; recorded here so it is not lost. See finding **F**.

## 3. Independent re-run of the gates (run by me, now, on `9f35ada`)

All four green, re-run from scratch rather than trusted from any commit message's claim.

- **`cargo test -p sefer-region` (debug) — 29 passed, 0 failed, 1 ignored**, across eight
  binaries: lib unittests 0, `bench_ids_isolatable` 1, `captrack_probe` 0 passed / 1 ignored
  (the telemetry probe, still correctly `#[ignore]`d after having its assertions lifted out),
  `clear_partial_under_panic` 2, `coverage_gaps` 19, `handle_static_asserts` 1, `smoke` 6,
  doctests 0 (consistent with CLAUDE.md's no-doctest rule).
- **`cargo test -p sefer-region --release` — 29 passed, 0 failed, 1 ignored**, identical
  per-binary breakdown. This is the profile CI gained in #693; it exercises `#696`'s new
  churn test and `#692`'s overflow-message tests with overflow-checks off.
- **`cargo clippy -p sefer-region --all-targets -- -D warnings`** — clean, zero diagnostics
  emitted (exit 0).
- **`cargo fmt -p sefer-region --check`** — clean, no output (exit 0).
- **`cargo doc -p sefer-region --all-features --no-deps`** — clean, generated with zero
  warnings, including no broken-intra-doc-link warnings from #695's rewritten prose or
  #687's `Self#reentrancy` anchor.

Delta vs. the prior closing review's 28-passed/1-ignored at `a980f58`: +2 from #692/#693
(`sync_region_concurrent_insert_remove_get_is_consistent` and the second overflow-message
test), then #696's net −1 (two runtime tests deleted, one churn test added), then +1 from
`region_with_capacity_overflow_panics` — netting 29. `1f962e5`'s "Net −1 test count" claim is
consistent with `handle_static_asserts` dropping from 3 tests to 1 and `coverage_gaps` gaining
one. Working tree clean before and after all five invocations.

I also ran the project's own doc-consistency gate that `b463825` cites:
**`cargo test --test no_stale_doc_references` — 13 passed, 0 failed**, matching that commit's
claimed count exactly.

## 4. CHANGELOG accuracy

**The gap `b463825` claims to have found is real, and I verified it independently rather than
accepting the commit's self-report.** `git log --follow -- CHANGELOG.md` shows exactly three
commits touching that file across this entire multi-round effort: `8dfd041` (#675, the
original sweep section), `aa24f84` (#682-684), and `b463825` itself. `aa24f84`'s CHANGELOG
diff is **4 lines / 2 changed** — an in-place reword of the #665 bullet (finding E) and a
count fix in the #668 bullet (finding F); finding G was handled in `crates/region/README.md`
(+10 lines), not the CHANGELOG. Grepping `aa24f84`'s CHANGELOG hunks for any trace of
`#679`/`#680`/`#681`/`INVARIANTS`/`PLAN.md`/`region_invariants` returns nothing. So tasks
#678-681 and #685-696 genuinely had **zero** CHANGELOG representation before `b463825`. The
claim is not an overstated finding manufactured to justify a large commit.

**Every SHA cited in the new text resolves to the commit it is attributed to.** I checked all
of them, not a sample:

`39704e1`→#678 · `9fcbbf1`→#679-681 · `aa24f84`→#682-684 · `df16693`→#690 · `a243c38`→#691 ·
`ed008a5`→#692 · `89913c6`→#693 · `ea52f85`→#694 · `0373b28`→#695 · `1f962e5`→#696 — plus the
pre-existing citations the new text reuses (`185db1b`→#670, `8dfd041`→#675, `cec0333`→#668,
`ecc5138`→#669, `81290fd`→#671, `67062b3`→#665). Zero mismatches. Five bullets (#685-#689)
cite no SHA at all despite having commits — see finding **E**.

**The corrected #670 bullet's substantive claim about #678 is TRUE.** I read `185db1b` and
`39704e1` in full rather than accepting the summary. `185db1b` added
`assert_eq!(r.capacity(), cap_after_first, "second insert reused freed slot (no capacity
growth)")` as its reuse oracle; `39704e1` removed exactly that assertion and replaced it with
`assert_eq!(slot_index(h_old), slot_index(h_new), …)` built on a new `slot_index()` helper
that parses `{idx}v{version}` out of `Handle`'s `Debug` output. The bullet's three load-bearing
sub-claims all check out: (a) `slotmap` pre-allocates, so `capacity()` is flat across a second
insert with no prior removal — independently confirmed by the prior review's probe
(`cap1=3 cap2=3 cap3=3`) and re-confirmed by mine below; (b) parsing `Debug` is indeed the
only external route to slot identity, since `Handle::key` is `pub(crate)` (`handle.rs:26`);
(c) #678 re-verified non-vacuity in isolation, and its diff's in-code comment says so at the
exact `remove()` call site.

**The one sub-claim that does not survive verification is a positional detail.** The bullet
says #670's counterfactual "only ever exercised an unrelated `len()` assertion **positioned
earlier in the same test**". I reconstructed `185db1b`'s exact assertion block outside the
repo and applied #670's own stated counterfactual ("temporarily made the second insert go
into a separate `Region`"):

```text
A1 len-after-remove : left=0 right=0 -> true
A2 capacity-oracle  : left=3 right=3 -> true
A3 len-after-second : left=0 right=1 -> false
```

`left: 0, right: 1` — the exact pair `185db1b`'s message reports — comes from A3, the
`assert_eq!(r.len(), 1, "length after second insert")` positioned **one line after** the
capacity oracle, not before it. The bullet's *substance* (the capacity oracle itself never
fired under that counterfactual) is confirmed correct: A2 passes. Only the word "earlier" is
inverted, and it is inherited verbatim from `39704e1`'s own commit message ("positioned before
it in the test"). See finding **D**.

**Nothing else in the three new blocks is falsifiable against the commits they cite.** I
spot-verified every mechanically-checkable claim in the #685-693 subsection: #685's
`crates/region/examples/contended_reads.rs` exists; #690's `checked_add` guards are at
`region.rs:90` and `:137`; #691's `#[repr(transparent)]` is at `handle.rs:22`; #692's
`catch_panic_message` helper is used at `coverage_gaps.rs:487,503`; #693's test is
`4 × 200` under `thread::scope` and the release CI step is `ci.yml:774`. The round-2
subsection's #696 bullet reproduces its counterfactual numbers correctly (§1 above), and its
"**Runtime improvements: 0**" header is accurate — no `src/` file changed in any of the three
round-2 commits except #695's doc comments. The relocated "Deferred, unchanged by this round"
paragraph still sits immediately before `### BREAKING CHANGE`, preserving the heading-nesting
property this file has had to repair twice. One phrasing looseness, not worth a numbered
finding: the commit message says "Only #682-684 … had ever been logged", when in fact
`aa24f84` applied E and F as *in-place corrections* to other bullets and handled G in the
README — none of the three had a bullet of its own until `b463825` created one. The CHANGELOG
text itself does not repeat this looseness.

## 5. Out-of-scope check

**Clean — the tightest round in this series.** File-level inventory of all five commits:
`ea52f85` touches one file (`crates/region/tests/clear_partial_under_panic.rs`); `0373b28`
touches three, all named in its own message (`crates/region/README.md`, `src/lib.rs`,
`src/region.rs`); `1f962e5` touches three, all named (`tests/captrack_probe.rs`,
`tests/coverage_gaps.rs`, `tests/handle_static_asserts.rs`); `b463825` touches `CHANGELOG.md`
only; `9f35ada` touches only the two `docs/checkpoints/*.md` files. Nothing outside
`crates/region/`, `CHANGELOG.md`, and `docs/checkpoints/`. Grepping every added line across
`ea52f85~1..HEAD` for `TODO|FIXME|XXX|HACK|dbg!|eprintln!|println!` returns **zero** matches.
No half-wired feature, no placeholder, no new `pub fn` of any shape (the crate remains
`#![forbid(unsafe_code)]` with no API surface change in this round at all — #695 changed only
doc comments). The two checkpoint files were read and contain no claim that contradicts the
commits: their "recently completed" list cites `1f962e5`/`0373b28`/`ea52f85` correctly, and
their record of #696's counterfactual ("over-inserting to force real capacity growth") matches
the commit message rather than the in-code comment, which corroborates finding **B**'s
diagnosis of *which* artifact drifted.

---

## Findings

**A — #694's headline claim is only half true: the `drop_count == 3` / `len() == 2` pair it
kept IS the drain-order oracle §D1a filed, and the false-red hazard therefore survives.**
Severity: **medium-low** (the task's entire stated purpose is ~50% achieved, and the test is
still a false-red trap for the exact event it was rewritten to survive; but nothing regressed
— the test is no weaker than before, and the *which-ids* half is a genuine improvement).
`clear_partial_under_panic.rs:83-88` and `:198-203` assert exact counts that are a direct
function of the bomb's ordinal position in the visitation sequence: verified at source that
`SlotMap::clear` → `drain()` → ascending `Drain::next` from `cur: 1`, and verified empirically
(§1's three-arm probe) that moving the bomb to the last slot yields `drop_count=5, len=0` and
to the first slot `drop_count=1, len=4` — both making the committed assertions fail while the
crate's order-free contract holds perfectly. The in-code comment added by the same commit is
actively misleading about this: *"slotmap's drain order is unspecified, so we don't assert
WHICH IDs were dropped/survived — only that the total accounting is correct"* — the total
accounting as written is not order-free. The audit offered two acceptable resolutions and the
commit took neither cleanly: it did not adopt the order-free invariant (`drop_count + len() ==
5`, which holds in all three of my arms), and its comment does not "explicitly accept the
slotmap-order dependence as a deliberate pin" — it denies the dependence exists. The same
"order-agnostic … keeping the same total-accounting strength" framing propagated into
`CHANGELOG.md`'s #694 bullet, so the public record carries it too. Cheapest honest fix, either
direction: (i) add `assert_eq!(drop_count + len(), 5)` as the order-free invariant and demote
the `== 3`/`== 2` pair to an explicitly-labelled deliberate order pin, or (ii) drop the exact
pair in favour of `drop_count + len() == 5` plus the already-correct complement logic. Either
also needs the comment and the CHANGELOG bullet reworded.

**B — #696's in-code counterfactual comment describes an experiment that was not the one
performed.** Severity: **low** (the counterfactual itself is real and reproduces; only the
durable in-tree description of it is wrong). `coverage_gaps.rs:446-448` states the test *"has
been verified to catch that class of bug by temporarily bypassing the slotmap free list
(inserting fresh handles instead of reusing freed slots) and confirming the assertion fails."*
The commit message, the CHANGELOG bullet, and the checkpoint all describe a *different*
experiment: raising the refill loop from 500 to 700 so it over-runs the 500 freed slots. I
reproduced the 700 variant and got `after_remove=1023, after_refill=2047` — exactly the
reported pair — so the work was genuinely done; but the in-code note is the artifact a future
maintainer will read and trust, and it names an experiment nobody ran. Reword it to match what
was actually done.

**C — #696's new test tolerates a partial free-list regression of up to ~23 inserts (≈4.6% of
the refill) before it fires.** Severity: **low, informational** (it does catch the bug class
the audit filed; this is a sensitivity note, not a defect). Measured threshold: refill=523
passes, refill=524 fails, because 1000 initial slots + N non-reused inserts must exceed the
1023 capacity high-water mark before `Vec` doubling shows up. The test's own comment frames
the caught bug as *"ignoring the free list and always allocating fresh slots"* — which it
does catch decisively (500 fresh inserts → 2047) — so the comment is not wrong; it is simply
silent about the slack. Worth one clause in the comment if anyone touches that test.

**D — the new CHANGELOG #670 addendum inverts the position of the assertion that actually
failed under #670's counterfactual.** Severity: **low** (a positional detail inside an
otherwise-correct and well-verified correction; the substantive claim is confirmed true). The
text says the counterfactual "only ever exercised an unrelated `len()` assertion positioned
**earlier** in the same test". Reconstructed and run (§4): the failing assertion is
`assert_eq!(r.len(), 1, "length after second insert")`, positioned one line **after** the
capacity oracle, and its `left: 0, right: 1` matches `185db1b`'s reported output exactly; the
assertion positioned *before* the oracle (`len() == 0` after remove) passes under the
counterfactual. Inherited verbatim from `39704e1`'s own commit message ("positioned before it
in the test"). A charitable reading exists — "earlier in the same test" could mean earlier
than the test's actual I3 subject (`get(h_old).is_none()`), which is true — so this is
imprecision rather than a falsified claim, but the ambiguity is exactly the kind this repo's
derived-numbers convention exists to foreclose.

**E — SHA-citation inconsistency inside the new #685-693 block.** Severity: **low** (honesty
of the public record's traceability, which the prior closing review made its primary CHANGELOG
gate). Bullets for #690/#691/#692/#693 cite their commits; bullets for #685/#686/#687/#688/
#689 cite none — yet all five have commits, which I resolved by grep: `25de4cd` (#685),
`127545b` (#686), `5e4244f` (#687), `ec59520` (#688), `5985a61` (#689). Every other bullet in
this CHANGELOG section, across both prior rounds, cites its SHA. Since `b463825`'s entire
purpose was to make the record complete and independently checkable, leaving five of nine
bullets unresolvable-without-grep is a gap in exactly the property that commit set out to
establish.

**F — §B17's per-method deadlock documentation is a residual from #687, not round 2.**
Severity: **informational, no round-2 action.** The audit asked for the hazard on the type doc
*and* on the one-shot methods; only the (thorough) type-level `## Reentrancy` section landed.
Of the seven one-shots, only `remove` cross-references it, and only because it is the
exception. A reader who lands on `SyncRegion::get_cloned`'s or `clear`'s rustdoc page from a
search engine sees no deadlock warning. Recorded so it survives past this review; the type-level
section does satisfy the finding's core requirement.

**G — #695's fix is complete and #696's structural claims all hold.** Severity:
**informational, no action.** Recorded as an explicit negative result because both were called
out in the brief as risk areas: the "never escape" qualification reaches all three occurrences
with no fourth anywhere in the workspace; the new churn test is not `#[ignore]`d and runs in
both profiles; the two deleted runtime tests were provably subsumed by the surviving const
assertions with no residual coverage loss; and `handle_layout_matches_expectations` is present
and byte-for-byte unchanged (the diff of that file is a pure deletion, zero additions).

---

## Verdict

**GO-WITH-FIXES** — every gate is independently green in both profiles, the diffs are the
tightest and most in-scope of this whole series, `b463825`'s self-reported CHANGELOG gap is
real and its SHA citations all resolve, and #695/#696 fully deliver what they claim (with
#696's counterfactual reproducing to the byte). The one substantive shortfall is **A**:
`ea52f85`'s subject line, its in-code comment, and its CHANGELOG bullet all assert
order-agnosticism that the retained `drop_count == 3` / `len() == 2` assertions demonstrably
do not have — the §D1a false-red hazard is reduced in surface but not removed, and the new
comment denies a dependence that is still there. **B**, **D** and **E** are small accuracy
residuals in artifacts (an in-code note, one word in a CHANGELOG addendum, five missing SHAs)
that each contradict or under-serve a claim the same commit makes about itself; **C**, **F**
and **G** need no action.
