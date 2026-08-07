# `sefer-region` round-closing review (read-only, end-to-end)

**Date:** 2026-08-07
**Reviewed range:** `b5f7e7b~1..HEAD`, HEAD = `a980f58b88be16c5980c544a9a7261538c3e6483` (`main`)
**Scope:** the 14-commit `sefer-region` round — 9 fix commits (#664-672), 2 pre-round #656
commits, 1 #656 follow-up, the CHANGELOG commit (#675), and the markdown-artifact commit (#676).
**Mode:** read-only. No repository file was modified except this report; `git status --porcelain`
was empty before and after. Two throwaway probes were built and run in a scratch cargo project
under `%TEMP%` (path-dependency on `crates/region`, deleted after use) to independently test
claims that could not be settled by reading; their verbatim output is inlined below.
`slotmap 1.1.1` was read at source level from the registry cache
(`D:\system_artefact\cargo\registry\src\index.crates.io-1949cf8c6b5b557f\slotmap-1.1.1\src\basic.rs`).

---

## 1. Commit-by-commit: does each diff match its own message and its task's claim?

All nine fix commits were read line by line against their commit messages and against the
work-plan task each claims to close, and **nine of nine diffs do what their messages say** —
including the several self-reported "caught during zero-trust review" corrections, each of which
I verified is actually present in the landed diff rather than merely narrated. Spot-verifications
that carried real risk of divergence: **`395258e` (#664)** genuinely softens I2/I3 at all seven
cited sites and genuinely *declines* to propagate slotmap's `2^31` figure onto the unrelated M8
segment-substrate entry (the hedge-plus-warning text is in both `docs/ALLOC_PLAN.md` and
`docs/INVARIANTS.md`, as claimed); **`67062b3` (#665)** adds exactly three plan-1 churn arms and
relabels the README rows, with the `benches/region_bench.rs` hunk being purely additive (the six
existing `bench_batched` arms are byte-for-byte untouched) — which matches the commit message's
own "additive, not a replacement" wording but *not* the CHANGELOG's later paraphrase (finding E
below); **`6ace228` (#667)** adds the bare-metal `thumbv7em-none-eabi` build for the one workspace
member that actually has a `std` feature to disable, plus the dedicated comment correcting the
"no-op flag for both" framing directly above it; **`0931d35` (#666)** lands the partial-clear
caveat at all four claimed sites and the `mem::forget` removal it says it made; **`ecc5138`
(#669)** documents the release/debug divergence honestly and deliberately tests only the
profile-independent half; **`ffb6813` (#672)** fixes all three (not two) `docs/BENCHMARKS.md`
references and reuses #664's already-qualified `2^31` wording on `contains()` rather than minting
a fresh absolute — a genuinely good catch; **`185df1b` (#670)** and **`81290fd` (#671)** match
their messages structurally, but #670's substantive claim does not hold up under test (finding D)
and #664 left three residuals behind (findings A-C). Nothing in any of the nine diffs contradicts
its message; the defects found are all things the messages claim were *finished* and were only
finished partway.

## 2. Were any "fix this" findings silently dropped?

**No — coverage is complete, and #673's deferral is properly recorded.** I walked every finding
in all three source reports against the nine landed commits. Performance review: §1 → `67062b3`;
§2 → `81290fd` (both recommended workloads plus both recommended doc sites); §3 → task #673,
deferred; §4's optional layout static-assert → `185df1b` (the optional `Ord` impls were
explicitly declined in work-plan §3(c)); §5 and §6 were no-action by the work-plan's own
arbitration. Logic review: F1 → `395258e`; F2 → `6ace228`; F3 → `0931d35` (including the
`write()`-transaction half, which reached `sync_region.rs`'s poisoning-policy block verbatim);
F4 and F5 → `ecc5138`; F6.1/F6.2/F6.3 → `cec0333`; F6.4 → `185df1b` (landed, but see finding D —
this is the one finding whose *fix* does not achieve what the finding asked for); F6.5 →
`185df1b`; F6.6 → `ecc5138` (partial by explicit, stated design); F7.1/F7.2/F7.3 → `ffb6813`.
Safety review: §1.2, §3.3.2, §5, §6.1's loom question — all four are in the work-plan's
explicit "deliberately NOT filed" list with reasons that survive re-reading; §6.1's *alternative*
recommendation (the panicking-`clear` test) → `0931d35`; §6.2 → `6ace228`; §7.1 → `ffb6813`;
§7.2 → `0931d35`. The performance review's three open questions to the maintainer (Q1-Q3) are
each answered by a landed decision rather than left hanging. Task #673 appears in the CHANGELOG
under an explicit `**[deferred]**` bullet naming the work-plan's own "no defect claimed or
found… blocks nothing" justification, and remains `pending` in the TaskList — not silently
dropped, exactly as expected.

## 3. Independent re-run of the gates (run by me, now, on `a980f58`)

All four are green, re-run from scratch rather than trusted from any prior session's claim.
`cargo test -p sefer-region` — **28 passed, 0 failed, 1 ignored**, across seven binaries: lib
unittests 0, `bench_ids_isolatable` 1, `captrack_probe` 0/1-ignored (the telemetry probe, still
correctly `#[ignore]`d), `clear_partial_under_panic` 2, `coverage_gaps` 16,
`handle_static_asserts` 3, `smoke` 6, doctests 0 (consistent with CLAUDE.md's no-doctest rule).
`cargo clippy -p sefer-region --all-targets -- -D warnings` — clean, no diagnostics emitted.
`cargo fmt -p sefer-region --check` — clean, no output. `cargo doc -p sefer-region --no-deps` —
clean, generated with zero warnings (notably no broken-intra-doc-link warnings, which is the
gate that would have caught #672's dangling-reference class had those been rustdoc links rather
than bare code-spans). The working tree was clean before and after all four.

## 4. CHANGELOG accuracy

The new `#### sefer-region correctness/safety/performance sweep` subsection is placed correctly
(after "Post-sprint housekeeping", before `### BREAKING CHANGE`, preserving the heading-nesting
property this file has twice had to repair), carries the honest `**Runtime improvements: 0**`
line, and correctly discloses that six of the nine delegated fixes needed a real correction
during review. **All nine cited SHAs resolve to the right commits** — I spot-checked every one
against `git show <sha> --stat`, not the three the brief asked for: `395258e`→#664,
`67062b3`→#665, `6ace228`→#667, `0931d35`→#666, `cec0333`→#668, `ecc5138`→#669, `ffb6813`→#672,
`185df1b`→#670, `81290fd`→#671. Task #673's deferral is present and honest. Two accuracy defects
remain (E and F below): the #665 bullet says the harness misuse was "corrected to the harness's
intended usage", which the diff does not support — the batched arms were left exactly as they
were and still time the fixture teardown; what actually shipped is a relabel plus three new
steady-state arms — and the #668 bullet claims "16 tests total" where `cec0333` added 15, the
16th having landed in `ecc5138` (#669), whose own bullet also claims it. Both are honest-framing
regressions relative to the commit messages they summarize, which were accurate; neither
overstates the round's substance, but the #665 one does overstate what was fixed.

## 5. Out-of-scope check

**Clean, with two files outside the brief's named allow-list, both justified.** Across the nine
fix commits the only paths touched beyond `crates/region/`, `CHANGELOG.md`, `docs/ALLOC_PLAN.md`,
`docs/INVARIANTS.md`, `docs/PLAN.md` and `fuzz/` are `bench-iters.txt` (repo root; `67062b3` and
`81290fd`) and `.github/workflows/ci.yml` (`6ace228`). `bench-iters.txt` is bench-scale-tool's
pinned-iteration-count file, established by `b5f7e7b` earlier in the same round, and it
*necessarily* changes whenever a bench workload is added — both diffs to it are exactly the
expected new-id lines plus recalibrated counts, no unrelated churn. `ci.yml` is #667's entire
deliverable. No stray edits anywhere else in the workspace. **#664's cross-workspace reasoning
holds up on inspection, not just by assertion:** `src/lib.rs:377` re-exports
`sefer_region::{Handle, Region}` verbatim, and both `tests/region_invariants.rs` and
`fuzz/fuzz_targets/region_ops.rs` drive that re-export — so `docs/INVARIANTS.md`,
`docs/PLAN.md` and `fuzz/`'s I2/I3 text are restatements of the *same* slotmap-backed claim and
genuinely required the same softening. One judgement call is worth naming without calling it a
defect: `docs/ALLOC_PLAN.md`/`INVARIANTS.md`'s **M8** entry describes the allocator's own
segment-substrate generation scheme, which was not audited this round, and #664 nonetheless
softened its "never" to "within the segment substrate's own generation-reuse budget". That is
defensible (any finite generation counter makes an unqualified "never" an overclaim, and the
commit explicitly refuses to assert slotmap's `2^31` figure there, adding a do-not-copy warning),
but it is a weakening of an unaudited invariant by analogy rather than by measurement.

## 6. Task #671's median-of-3 claim — verified true

`81290fd`'s commit message states that the delegated `/crush` run's own report cited three
mutually inconsistent number sets (prose 2,226/10,546; its own pasted verification output
2,054/10,449; the number initially written into README 2,125/10,532) and that the published
numbers were therefore independently re-measured. **The claim checks out arithmetically and the
numbers look like a real median-of-3, not a fabrication.** The commit message reports
`st/holey_sweep` 2,227.78-2,510.10 (median 2,475.64) and `st/sparse_sweep` 10,955.34-11,844.99
(median 11,481.80); the README publishes `2,476 (2,228–2,510)` and `11,482 (10,955–11,845)` —
each is the message's own figure rounded to the table's established precision, with the range
being exactly the observed min and max. Both medians sit strictly inside their ranges and are
much closer to the max than the min (holey) and roughly centred (sparse), which is the
irregular shape a genuine three-sample median has and a fabricated one rarely does; neither is
a round number, and neither matches any of the three rejected delegated figures. The derived
ratios are also correct and independently recomputable: 2,476/1,319 = 1.877 → "~1.9×" and
11,482/1,319 = 8.705 → "~8.7×", both as published. One methodological note, not a defect: the
1,319 zero-holes baseline those ratios divide by was measured in an earlier session rather than
re-measured alongside the two new arms (the performance review's own fresh run of that same
workload read 1,369 ns/op, ~4% higher). The effect sizes are one and two orders of magnitude
larger than that drift, so the conclusion is unaffected. I also confirmed a real catch in this
commit that its message understates: the performance review recommended the ids
`st/iterate_holey`/`st/iterate_sparse`, which would have contained `st/iterate` as a substring
and violated `tests/bench_ids_isolatable.rs`'s no-substring rule — the landed ids
`st/holey_sweep`/`st/sparse_sweep` avoid this, and the test passes.

---

## Findings

**A — `docs/INVARIANTS.md:16` still carries the exact fabricated mechanism `395258e` says it
caught and corrected.** Severity: low-medium (factual error in a workspace spec doc).
The I3 entry reads "The slot's generation is bumped on removal (**incremented by 2 per
`remove_from_slot` in slotmap**)". `395258e`'s own commit message documents catching precisely
this claim in the delegated diff and states it was "corrected in both of `region.rs`'s two
occurrences" — but the identical claim in `INVARIANTS.md` was not corrected. Verified against
source: `slotmap-1.1.1/src/basic.rs:445` shows `remove_from_slot` calling
`slot.version.wrapping_add(1)` **exactly once**; the second `+1` comes from `insert`'s
`version | 1` on an even/vacant slot, a different operation in a different function. Verified
empirically (scratch probe, verbatim): a slot's version goes `1v1` → `1v3` → `1v5` across
successive occupy/free cycles — so the *net* `+2` per cycle is right and the `2^31` budget is
right, but attributing it to `remove_from_slot` is wrong.

**B — `docs/PLAN.md:459` has an orphaned text fragment left by the string replacement.**
Severity: low (cosmetic, but visibly broken prose in a spec doc). The replaced sentence now reads
`…(uses a 32-bit generation counter that wraps after ~2^31 reuse cycles of the same slot; memory
safety is never affected).` followed on the next line by a dangling `alias). The hand-rolled
retirement is removed;` — the tail of the old sentence the edit did not consume. Separately,
`docs/PLAN.md:172-173` has broken list-continuation indentation (`  the same slot;` at two
spaces inside a four-space continuation block).

**C — `tests/region_invariants.rs:24` is the last surviving absolute I2 claim in the workspace.**
Severity: low (a test doc comment, not published API — but it is the doc comment on the
*miri-executed invariant suite* for the very same `sefer_region::Region`). It still reads "a
removed handle is `` `None` forever ``". This is not a new miss: `0931d35`'s own zero-trust
review discovered that #664's verification grep used a plain `None forever` pattern that could
not match backtick-formatted occurrences, and re-swept with a tolerant regex — but scoped that
re-sweep to "the crate, `src/lib.rs`, or `README.md`", so the root `tests/` tree was never
covered by either pass. My sweep of `docs/`, `README.md`, `src/lib.rs` and `tests/` found this
as the only remaining instance; every "never"/"forever" left inside `crates/region/` is either a
genuinely absolute I5 drop-once claim or a structural statement about raw keys not escaping.

**D — #670's de-vacuuming of the I3 test replaced a vacuous assertion with a non-discriminating
one; the test still cannot detect the failure it claims to guard against.** Severity: medium in
context (this was the *entire point* of the task, and the finding it closes was itself "this test
can pass vacuously"). `crates/region/tests/smoke.rs` now asserts
`assert_eq!(r.capacity(), cap_after_first, "second insert reused freed slot (no capacity
growth)")`. Verified in a scratch project outside the repo that this does **not** discriminate
reuse from non-reuse (verbatim probe output):

```text
cap_after_first = 3
cap after reuse-insert = 3
h_old = Handle { key: DefaultKey(1v1) }
h_new = Handle { key: DefaultKey(1v3) }
no-remove: cap1=3 cap2=3 a=Handle { key: DefaultKey(1v1) } b=Handle { key: DefaultKey(2v1) }
no-remove: cap3=3 c=Handle { key: DefaultKey(3v1) }
```

`Region::capacity()` is 3 after the first insert (slotmap's `Vec` capacity 4 minus the sentinel)
and stays 3 through a second **and** third insert with no removal at all — so the assertion
passes identically whether slotmap reused the freed slot or allocated a fresh one. If slotmap's
freelist policy stopped reusing slots tomorrow, this test would still pass, which is exactly the
failure mode logic-review F6.4 filed. The counterfactual `185df1b`'s message cites ("temporarily
made the second insert go into a separate `Region` … confirmed the strengthened assertion fails
(left: 0, right: 1 on the len check)") failed on the *`len()`* assertion, which a separate region
trivially breaks — it never exercised the capacity-based reuse oracle at all, so the non-vacuity
evidence offered does not cover the claim in question. F6.4's own recommendation is the
discriminating oracle and was not used: `Handle`'s `Debug` exposes `DefaultKey`'s `{idx}v{version}`
form, and the probe above shows the index component is `1` for both `h_old` and `h_new` under
reuse versus `1` and `2` without it. Note the *rest* of #670 is sound — the
`handle_static_asserts.rs` `const _: () = assert!(...)` layout checks and the `assert_send_sync`
compile-time checks are genuine compile-time guards, and their `== 9`/E0080 counterfactual is a
real one.

**E — the CHANGELOG's #665 bullet overstates what shipped.** Severity: low-medium (honesty of the
public record, which is the specific thing this repo's conventions police). It says
`bench_batched`'s "per-iteration setup/routine split had been **misused** … **corrected to the
harness's intended usage**". Neither half is supported by `67062b3`'s diff: the six existing
`bench_batched` arms are untouched (the `region_bench.rs` hunk contains no deletions at all) and
still time the fixture teardown to this day, exactly as the README's own explanatory note now
says; and per the performance review §1 the in-window drop is `bench_batched`'s *inherent*
semantics (`routine` takes `state` by value), not a misuse of the API. What actually shipped is a
relabel of the batched rows plus three new plan-1 steady-state arms. The commit message itself
gets this right ("additive, not a replacement… the cold numbers are a real workload shape, the
defect was the missing label, not the measurement"); only the CHANGELOG paraphrase drifted.

**F — the CHANGELOG double-counts one test between the #668 and #669 bullets.** Severity: low.
The #668 bullet says "Added 16 tests total"; `git show cec0333:crates/region/tests/coverage_gaps.rs`
contains 15 `#[test]` functions (9 top-level + 6 in nested `mod`s), and `cec0333`'s own commit
message correctly says 15. The 16th, `region_reserve_overflow_panics`, landed in `ecc5138`
(#669), whose bullet also — correctly — claims it.

**G — a small unexplained numeric disagreement between `67062b3`'s message and the README it
wrote.** Severity: low. The commit message reports the `sync/churn` spread as "~69.6-84.2 ns/op
across multiple runs"; the README publishes `76.0 (72.1–84.2)`. If more than three runs were
taken and the table shows a median-of-3 subset that is fine, but neither the commit nor the
README says so, and under this repo's own derived-numbers convention a report's prose and its
published table should not disagree on their stated minimum without explanation.

**H — cross-run baseline for #671's ratios.** Severity: informational, no action needed. See §6
above: the ~1.9×/~8.7× ratios divide by a 1,319 ns/op baseline from an earlier measurement
session rather than one re-taken in the same median-of-3 pass; the ~4% observed drift on that
workload is negligible against effect sizes of 1.9× and 8.7×.

---

## Verdict

**GO-WITH-FIXES** — the round's substance is sound and every gate is independently green, but
four documentation residuals (A, B, C) and one CHANGELOG overclaim (E) contradict claims those
same commits make about themselves, and #670's replacement assertion (D) does not actually
discriminate the condition it was written to prove, leaving the very vacuity that task existed to
remove.
