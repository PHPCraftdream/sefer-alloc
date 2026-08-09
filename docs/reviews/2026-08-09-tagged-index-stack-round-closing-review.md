# `tagged-index-stack` round-closing review (read-only, end-to-end)

**Date:** 2026-08-09
**Reviewed range:** `1485bb6~1..HEAD`, HEAD = `7a993ce5b239c3c0a8aebfbb571bbab70c4b06f6` (`main`)
**Scope:** the 7-commit `tagged-index-stack` `rust-intel` remediation round — 5 fix/test commits
(`1485bb6` #698, `c552704` #702, `17185d6` #703, `96e28a7` #704, `d156e5c` #705), the CHANGELOG
commit (`28686dc` #737), and the checkpoint-artifact commit (`7a993ce` #738).
**Mode:** read-only. No repository file was modified except this report; `git status --porcelain`
was empty before writing it. Four throwaway probes were built and run in a scratch cargo copy of
`crates/tagged-index-stack` under `%TEMP%` (workspace-detached, deleted after use) to settle
claims that could not be settled by reading; their verbatim output is inlined below.

**Bottom line:** the round's *code* is correct — the `#698` concurrency fix is right, sufficient,
and does not move the bug elsewhere (independently counterfactual-verified), the `#703` `assert!`
promotion and its message-pinned test are genuinely non-vacuous, and the `#705` `_CHECK_BITS`
widening does exactly what it claims (probed against all six associated items). The round's
*evidence* has one real hole: **task #702's HIGH audit finding is not actually closed** — the
untagged ABA counterfactual still `#[should_panic]`s for the wrong reason, just a *different*
wrong reason than the one it was fixed for. That is finding **F1** below and is the only
high-severity item.

---

## 0. Current-state green check (re-run personally, not trusted from commit messages)

| Command | Result |
| --- | --- |
| `cargo test -p tagged-index-stack` | **17 passed** (4 proptest + 4 counter-wrap + 9 stack_unit), 0 failed |
| `RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --test loom_aba --release` | **8 passed**, 0 failed |
| `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` | clean |
| `RUSTFLAGS="--cfg loom" cargo clippy -p tagged-index-stack --all-targets -- -D warnings` | clean |
| `cargo doc -p tagged-index-stack --no-deps` | **0 warnings** (matches `d156e5c`'s claim) |
| `cargo test --test no_stale_doc_references` | 13 passed (matches `28686dc`'s claim) |

All the numeric claims in the commit messages check out against the current tree.

---

## 1. Commit-by-commit: does each diff match its own message?

All seven commits were read line by line (`git show`) against their messages. **Seven of seven
diffs do what their messages say.** Specific spot-verifications that carried real risk of
divergence:

- **`1485bb6` (#698)** genuinely changes `pop`'s failure ordering
  (`crates/tagged-index-stack/src/lib.rs:484`) and genuinely adds an end-to-end loom test that
  calls the *real* `pop`/`push` (`tests/loom_aba.rs:528`, `:533` — `stack_a.pop(&*links_a)` /
  `stack_b.push(&*links_b, 0)`, no hand-inlining). The self-reported zero-trust catch (correcting
  the delegated draft's false "this test does NOT catch the regression" comment) is present in
  the landed doc comment at `tests/loom_aba.rs:513-516`, and the correction is itself true — see
  §2.1.
- **`c552704` (#702)** lands both claimed harness changes and the companion test; the two
  self-reported review catches (the scheduling-dependent `!popped.contains(&1)` assertion
  replaced with a dedup conservation invariant, and the missing `is_empty` guard) are both in the
  landed diff (`tests/loom_aba.rs:290-292` and `:345-353`). But the replacement oracle was applied
  only to the **companion** test, not to the untagged counterfactual it was mirroring — F1.
- **`17185d6` (#703)** promotes `debug_assert!`→`assert!` at `src/lib.rs:413` and rewrites both
  doc sites (`src/lib.rs:392-403`, `:405-411`); the `# Panics` section no longer says "never in
  release".
- **`96e28a7` (#704)** adds `#[doc(hidden)]` at `src/lib.rs:504`. Independently verified: after
  `cargo doc -p tagged-index-stack --no-deps`, `grep -c raw_head
  target/doc/tagged_index_stack/struct.TaggedIndexStack.html` returns **0**.
- **`d156e5c` (#705)** lands all five sub-items. The only manifest change is a new
  `[dev-dependencies] proptest = "1"` plus the matching `Cargo.lock` line — no version bump of
  anything existing, consistent with the per-manifest `proptest = "1"` pattern already used by the
  root crate, `size-classes`, and `globalalloc-model` (there is no `[workspace.dependencies]`
  table in this workspace, so this is not new drift; the `loom` drift item is already filed as
  task #711).
- **`28686dc` / `7a993ce`** are docs-only and touch nothing else.

**Out-of-scope edits, TODO/placeholder code, half-wired features: none found.** Every hunk in
every commit is inside `crates/tagged-index-stack/` except the CHANGELOG/Cargo.lock/checkpoint
files, which are the round's declared bookkeeping. No `TODO`, `FIXME`, `unimplemented!`,
`todo!`, or dead feature gate was introduced. `grep -n "TODO\|FIXME\|unimplemented\|todo!"` over
the crate returns nothing.

## 2. Were any audit findings silently dropped?

**No — coverage is complete.** Walking every finding in
`docs/reviews/2026-08-07-tagged-index-stack-rust-intel-audit.md` (0 critical / 2 high / 2 medium /
7 info) against the landed commits: §B13 → #698; §D1a → #702; §B26 (`push` guard) + §D1 (F1
regression test) → #703; §A3 (`raw_head`) → #704; §B26 (`_CHECK_BITS`) + §C1 (`Links` posture) +
§F2 (`pack` doc) + §F2 (README tag budget) + §F4 (proptest) → #705; §C10 (`loom = "0.7"` in three
manifests) → task #711, a workspace-scope item that correctly stayed out of this crate's round and
is still `pending` in the TaskList. Nothing is unaccounted for. The audit's §D1 fix suggestion had
a second half ("gate the TAIL-push half with `#[cfg(debug_assertions)]` … documenting the guard is
debug-only") that #703 deliberately made moot by promoting to `assert!` — a strictly better
resolution, correctly not carried out literally.

---

## 3. Findings

### F1 — HIGH — the untagged ABA counterfactual still panics for the wrong reason; task #702's audit finding (§D1a) is **not** closed

**`crates/tagged-index-stack/tests/loom_aba.rs:248-253`** (the `assert!(!popped.contains(&1), …)`
oracle inside `counterfactual_untagged_head_lets_aba_corrupt_free_list`).

The audit's §D1a defect was: *"the `#[should_panic]` the crate cites as its headline
non-vacuousness proof for the ABA tag proves a harness bug, not that the tag is load-bearing."*
`c552704` fixed the specific accounting bug it named (`popped.push(0)` → `popped.push(idx)`) and
redesigned the scenario, but replaced the oracle with `!popped.contains(&1)` — a
**schedule-dependent** predicate, which is precisely the defect class the same commit found and
fixed in the *companion* test (`tests/loom_aba.rs:328-353`, the dedup conservation invariant) and
then did not port back here.

**Verified, not inferred.** Instrumenting the test in a scratch copy to print each loom iteration's
state shows it panics on the **very first iteration**, on a completely benign, race-free schedule:

```text
ITER: a_result=Ok(0) b_first=Some(1) b_held=None
ITER: popped=[0, 1]

thread 'counterfactual_untagged_head_lets_aba_corrupt_free_list' panicked at tests\loom_aba.rs:250:9:
free-list corrupted by ABA: index 1 appears in drain [0, 1] when only vec![0] is correct. ...
```

Decoded: thread A ran to completion first, legitimately popped index 0 (`head 0→1`, no
interposition, no stale CAS). Thread B then popped 1, found the stack empty on its second pop
(`b_held=None` — the "live consumer holds a slot" resurrection setup the redesign is built around
**never engaged**), and pushed 1 back. The final drain correctly yields 1. A holds 0, the free
list holds 1: **zero duplication, zero loss, textbook-correct behaviour** — and the test declares
it "corrupted by ABA". Because loom aborts model-checking at the first panic, the genuine ABA
interleaving is never reached by the shipped test.

The *scenario* is sound — only the oracle is wrong. Substituting the companion test's own
scheduling-independent conservation oracle (A's result + B's held item + the drain must be
pairwise disjoint) into this same untagged model, corruption **is** reachable, at loom iteration
15:

```text
ITER: a_result=Ok(0) b_first=Some(0) b_held=Some(1)
thread '…' panicked at tests\loom_aba.rs:239:9:
assertion `left == right` failed: free-list corrupted (duplicate): [0, 1]
  left: 2
 right: 3
```

— A and B both popped index 0 (a genuine double-allocated slot) while B held 1 and the drain
returned a third item. That is the real corruption the counterfactual is supposed to exhibit.

**Concrete consequence if left unfixed.** The test cannot distinguish "the untagged model
corrupted its free list" from "thread A won the race cleanly", so it will stay green under any
future edit that destroys the model's ability to corrupt at all (a narrower `preemption_bound`, a
change to B's shape, a different `N`). More immediately, three *published* claims are already
false as shipped: `crates/tagged-index-stack/README.md:79-82`, `src/lib.rs:102-106`, and — the one
that goes to crates.io — `crates/tagged-index-stack/Cargo.toml:8`'s `description` field
("`#[should_panic]` counterfactuals (untagged corruption + empty-transition tag-reset ABA)" …
"proving the harness is non-vacuous"). The untagged half of that pair does not currently
demonstrate untagged corruption. Note the H-2 counterfactual
(`counterfactual_empty_transition_tag_reset_lets_aba_recur`, `tests/loom_aba.rs:494`) is **not**
affected — it asserts `a_result.is_err()` on a rendezvous-pinned schedule and is a valid negative
control.

Suggested close (for a follow-up task, not applied here): replace `:248-253` with the same
`accounted`/`dedup` conservation oracle the companion test at `:335-353` uses, keeping
`#[should_panic(expected = "corrupted")]` — verified above to still panic, and for the right
reason.

### F2 — LOW — CHANGELOG.md quotes a panic message that the code path it describes cannot produce

**`CHANGELOG.md:196`** — the task #702 bullet states the missing `is_empty` guard caused *"a real
out-of-bounds panic (`the len is 4 but the index is 4294967295`)"*.

The code path described is `tagged_stack_survives_the_same_resurrection_pattern`, which runs
`ArrayLinks::<2>` at `INDEX_BITS = 16`, so its empty sentinel is `0xFFFF = 65535`, not
`u32::MAX = 4294967295`, and the array length is 2, not 4. Reproduced in the scratch copy by
deleting the `is_empty` guard at `tests/loom_aba.rs:290-292`:

```text
thread 'tagged_stack_survives_the_same_resurrection_pattern' panicked at src\lib.rs:348:9:
index out of bounds: the len is 2 but the index is 65535
```

The quoted string is task **#703**'s diagnostic (`ArrayLinks::<4>` at width 32, where
`INDEX_MASK == TAIL == 4294967295`) — it appears verbatim in commit `17185d6`'s message and was
cross-contaminated into #702's CHANGELOG bullet. Commit `c552704`'s own message does *not* make
this error (it says only "index out of bounds"). Consequence: a reader trying to reproduce #702's
described failure from the CHANGELOG will not be able to, and the two tasks' evidence is
conflated.

### F3 — LOW — `_CHECK_BITS`'s doc overclaims: `TAG_BITS` still reaches an out-of-range width unguarded

**`crates/tagged-index-stack/src/lib.rs:197-198`** — *"There is no remaining path to an
out-of-range width reaching any of these items unchecked."*

The `INDEX_MASK`-initializer widening itself is **correct and does what it claims**. Probed each
associated item at the out-of-range width `INDEX_BITS = 40` in a scratch copy:

| Item | Result |
| --- | --- |
| `TaggedIndex::<40>::INDEX_MASK` | guard fires (`INDEX_BITS must be in 1..=32`) |
| `TaggedIndex::<40>::unpack(0)` | guard fires |
| `TaggedIndex::<40>::empty_index()` | guard fires |
| `TaggedIndex::<40>::is_empty(0)` | guard fires |
| `TaggedIndex::<40>::empty()` | guard fires |
| `TaggedIndex::<40>::pack(1, 1)` | guard fires |
| **`TaggedIndex::<40>::TAG_BITS`** | **compiles clean, evaluates to `24`** |

`TAG_BITS` (`src/lib.rs:222`, `64 - INDEX_BITS`) is a `pub` associated const that never touches
`INDEX_MASK`, so it is reachable at any `INDEX_BITS <= 64`. Concrete scenario: a downstream user
writing `let tag_range = 1u64 << TaggedIndex::<40>::TAG_BITS;` (exactly the shape this crate's own
`tests/proptest_pack_unpack.rs:22` uses) gets a silently meaningless `2^24` with no diagnostic, and
only discovers the width is illegal when they later touch the mask. Severity is low because
`TAG_BITS` alone cannot corrupt anything and every real stack/packing use forces the guard — but
the doc sentence as written is false. The precise, true statement is the one already in the
preceding clause: *every **mask-touching** associated item forces this guard*.

### F4 — LOW-MEDIUM — the `assert!` promotion's release-profile claim has no CI run

**`crates/tagged-index-stack/tests/stack_unit.rs:131-133`** — *"The guard is now a full `assert!`
… so this holds identically in both debug and release builds."*

`.github/workflows/ci.yml:724` runs `cargo test -p tagged-index-stack --no-fail-fast` (debug
only). The only `--release` invocation of this crate anywhere in CI is
`.github/workflows/ci.yml:1110-1113`, which runs `--test loom_aba` under `RUSTFLAGS="--cfg loom"`
— a target selection that excludes `stack_unit.rs` twice over (by `--test` and by its own
`#![cfg(not(loom))]`). So the release half of the sentence is asserted in a comment and verified
nowhere.

This is the identical gap class that task **#693** closed for `sefer-region` ("reserve's
'profile-independent' panic claim has no release-profile CI run"), and it was not filed for this
crate. Concrete scenario: if the `assert!` at `src/lib.rs:413` is ever reverted to
`debug_assert!` (or wrapped in `#[cfg(debug_assertions)]`), CI stays fully green — the
release-mode behaviour change is invisible to every job in `ci.yml`. One extra step
(`cargo test --release -p tagged-index-stack`) closes it.

### F5 — LOW — the crate's advertised `no_std` / `no-std::no-alloc` claim has zero CI proof

`crates/tagged-index-stack/Cargo.toml:12` declares `categories = [… "no-std::no-alloc"]` and
`src/lib.rs:125` sets `#![cfg_attr(not(test), no_std)]`, but `.github/workflows/ci.yml`'s
`test-workspace` job cross-builds `size-classes` (`:753`), `racy-ptr-cell` (`:754`), and
`sefer-region` (`:762`) for `thumbv7em-none-eabi` and **not** this crate. It cannot use
`thumbv7em-none-eabi` (that target has no 64-bit atomics — the crate's own `compile_error!` at
`src/lib.rs:134-143` fires there, correctly), which is presumably why it was skipped; a
64-bit-atomic bare-metal target (`x86_64-unknown-none` / `aarch64-unknown-none`) would work.

Two concrete consequences: (a) a future edit that pulls in `std` (e.g. a `String` in a panic
message, a `std::` import behind a cfg) ships to crates.io under a `no-std::no-alloc` category
claim that no job checks; (b) the `compile_error!` guard itself — a documented, README-advertised
behaviour — has no test proving it fires with the intended message on an unsupported target.

### F6 — LOW — the crate now ships **three** `#[should_panic]` counterfactuals, but every doc site still says two

`crates/tagged-index-stack/README.md:76-79`, `src/lib.rs:102-106`, and `Cargo.toml:8`'s published
`description` all enumerate the counterfactuals as "(untagged corruption + the H-2 tag-reset
ABA)". `1485bb6` added a third, `counterfactual_relaxed_cas_failure_corrupts_free_list`
(`tests/loom_aba.rs:649`), and no doc site was updated. `Cargo.toml`'s `description` is the one
that ships to crates.io on the pending first publish (task #661), so this is worth fixing before
that, not after.

### F7 — LOW — the companion conservation oracle detects duplication but not loss

**`crates/tagged-index-stack/tests/loom_aba.rs:345-353`** — `tagged_stack_survives_the_same_
resurrection_pattern` compares `accounted.len()` before and after `dedup()`.

In every legal schedule of this test, `accounted` must be exactly `{0, 1}` — both indices are
always accounted for (A's popped item, B's never-repushed held item, and the drain partition the
two slots). The dedup check therefore leaves half the conservation property unasserted. Concrete
scenario: a corruption in which the head jumps *past* a live index (chain truncation rather than
resurrection — e.g. a stale CAS installing `empty` where `next` was a real index) leaves
`accounted = [0]` with no duplicate, and the test stays green while an index has permanently
leaked out of the free list. The sibling test at `:119-124` gets this right
(`assert_eq!(popped, vec![0, 1])`, catching both loss and duplication); `assert_eq!(accounted,
vec![0, 1])` here would be the same strength and still fully schedule-independent.

### F8 — INFORMATIONAL — the two hand-inlined CAS-retry harnesses do not mirror `pop`'s tag discipline

**`crates/tagged-index-stack/tests/loom_aba.rs:584`, `:586`, `:609`, `:611`** (and the mirrored
lines at `:669`, `:671`, `:693`, `:695`) hardcode `Tag::pack(…, 0)`, discarding the observed
running tag — unlike the real `pop` (`src/lib.rs:478-481`) and unlike the file's other
hand-inlined harnesses (`:90-93`, `:296-300`), which preserve it. The inline comment at `:584`
("tag value doesn't matter for failure path") is true of the *first* CAS, which is expected to
fail, but the **second** CAS at `:615` is expected to *succeed* and installs a tag-0 head — i.e.
the harness resets the ABA tag mid-scenario. Harmless here (nothing pushes afterwards, and the
tests' assertions do not depend on the tag), and both tests are explicitly labelled exposition
harnesses subordinate to the real-type test at `:518`. Flagged only so a future reader does not
mistake them for faithful `pop` transcriptions the way `aba_repush_forces_stale_cas_retry_and_
stays_consistent` explicitly claims to be.

### F9 — INFORMATIONAL — `tests/loom_aba.rs`'s module-level property list was not extended this round

`crates/tagged-index-stack/tests/loom_aba.rs:19-32` still enumerates properties (a)–(d). The round
added a section `(e)` (`:498-502`) and a fifth scenario (`tagged_stack_survives_the_same_
resurrection_pattern`) that the top-of-file list does not mention. Separately, listed property (a)
— *"A's stale-tag CAS is FORCED to fail (retry) rather than succeeding onto a stale chain"* — is
not actually asserted by `aba_repush_forces_stale_cas_retry_and_stays_consistent`, which asserts
only the conservation property; it *cannot* assert (a) unconditionally, because A's CAS
legitimately succeeds when A wins the race. The only test that asserts a forced CAS failure is
the H-2 harness at `:446-452`, which pins the schedule with a two-flag rendezvous.

### F10 — INFORMATIONAL — two small residuals in the new proptest file

**`crates/tagged-index-stack/tests/proptest_pack_unpack.rs:21`** — `index in
0u64..TaggedIndex::<1>::INDEX_MASK` is `0..1`, so the "degenerate width" property exercises
exactly one index value (0); only its tag axis is actually randomized. Worth a one-line comment so
a reader does not over-read what width 1 proves.

**Coverage gap:** `pack`'s truncation semantics — the exact claim `d156e5c` rewrote at
`src/lib.rs:224-231` — have no test at any width. All four properties draw `index` strictly below
`INDEX_MASK`, so the `& Self::INDEX_MASK` mask is never exercised. Worth noting because the
truncation's sharpest failure mode is stronger than the corrected doc states: at width 16,
`pack(0x1_FFFF, tag)` truncates to `0xFFFF`, which is the **empty sentinel**, so
`is_empty(pack(0x1_FFFF, tag)) == true`. The doc's "the failure mode is a wrong index
round-tripping out of `unpack`" is accurate but understates this specific case (the word reads as
*empty*, not merely as a *wrong index*). Not reachable through `TaggedIndexStack::push` — its
`assert!` forecloses it — but `pack` is public.

### F11 — INFORMATIONAL — one wrong word in the CHANGELOG's #698 bullet

**`CHANGELOG.md:195`** — *"permitting a stale/torn read on architectures weaker than x86"*. An
`AtomicU32::load` cannot tear, on any architecture; the defect is staleness (missing
happens-before) only. `1485bb6`'s own commit message gets this right ("could observe stale data").

---

## 4. Category-by-category verdicts on the questions asked

### 4.1 The `Acquire` failure-ordering fix (#698) — **correct, sufficient, and not moved elsewhere**

**Correct.** `pop`'s `compare_exchange(head, new_head, Acquire, Acquire)` (`src/lib.rs:484`) makes
the failure path an atomic *acquire load*. `push` publishes with a `Release` CAS (`:442`) that is
sequenced-after its `store_next` (`:433`). Because *every* modification of `head` is an RMW
(`push`'s CAS or `pop`'s CAS), each observed head value lies in the release sequence headed by the
`push` that wrote the relevant link, so the retry's acquire observation synchronizes-with that
`push` and the subsequent `links.load_next` at `:475` is guaranteed to see the pushed link. `pop`'s
`Acquire`-only (not `AcqRel`) success ordering does not break this: release-sequence continuation
requires only that intervening modifications be RMWs, regardless of their own ordering.

**Sufficient / not moved elsewhere.** `push`'s remaining `Relaxed` failure ordering (`:442`) is
genuinely fine and is *not* the same bug relocated: on `Err(actual) => head = actual`, `push`
dereferences nothing — it uses the observed word only as a plain integer (`:424-429`) which it
then *writes* as its own `next_link`, and the following CAS atomically revalidates that the head
still equals the observed word. The audit reached the same conclusion independently
(`docs/reviews/2026-08-07-tagged-index-stack-rust-intel-audit.md:96`). No other read-through-a-
relaxed-observation site exists in `src/`.

**Empirically confirmed non-vacuous.** I re-ran the counterfactual myself rather than trusting the
commit message: reverting `src/lib.rs:484`'s failure ordering to `Ordering::Relaxed` in a scratch
copy makes `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type` **fail**, with
exactly the shape the commit cites:

```text
test pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type ... FAILED
assertion `left == right` failed: free-list corrupted (loss or duplication) via the real pop/push: got [0, 0, 1]
  left: [0, 0, 1]
 right: [0, 1]
```

The other seven loom tests stayed green under the reverted ordering, which also confirms this
test is the *only* thing protecting that line — i.e. the commit message's characterization of it
as "the test that actually protects the shipped source" is accurate, not marketing.

**Does the end-to-end test really call the real functions?** Yes — `tests/loom_aba.rs:528` and
`:533` call `stack_a.pop(&*links_a)` / `stack_b.push(&*links_b, 0)` directly, with no
`cas_head_for_test` and no transcription, and its oracle (`assert_eq!(popped, vec![0, 1])`,
`:547-551`) is a tight both-directions conservation check (the stack is seeded non-empty and
nothing but A ever pops, so A always returns `Some`, and both indices must always be accounted
for).

### 4.2 The redesigned ABA counterfactual (#702) — **scenario sound, oracle broken**

Covered in full as **F1**. Summary: the redesigned B-shape (two pops, re-push only the first,
hold the second) *is* capable of producing genuine free-list resurrection in the untagged model —
I demonstrated it directly. But the shipped assertion `!popped.contains(&1)` is
schedule-dependent and fires on loom's very first, entirely benign iteration, so the shipped test
never witnesses the corruption it claims to prove. The companion test
`tagged_stack_survives_the_same_resurrection_pattern` **is** sound and its dedup-based oracle does
catch real duplication (see F7 for its one-sidedness) — the fix that was applied there is the fix
the untagged counterfactual still needs.

### 4.3 The `assert!` promotion (#703) — **verified correct, no issues**

The guard at `src/lib.rs:413` is an unconditional `assert!`, so it is release-active by
construction (F4 is about CI *proving* that, not about whether it is true). The panic-message check
at `tests/stack_unit.rs:143-149` is genuinely tight: the pinned substring `"index must be <
INDEX_MASK"` appears nowhere else in the crate or in `core`, and the specific false-pass the audit
named — `ArrayLinks::store_next`'s slice OOB — produces `"index out of bounds: the len is 4 but the
index is 4294967295"`, which does not contain it. The payload downcast (`&str` first, then
`String`) is correct for `assert!(cond, "literal")`, whose payload is a `&'static str`. One note
for the `no_std` posture: the promoted `assert!` carries a static message with no format arguments,
so it lowers to a `core::panicking::panic` call and pulls in no formatting machinery — the
`no_std`/allocation-free claims are unaffected.

### 4.4 The `_CHECK_BITS` widening (#705) — **verified correct; one doc overclaim (F3)**

`INDEX_MASK`'s block initializer (`src/lib.rs:215-218`) does force `_CHECK_BITS` for every
associated item that references the mask — probed and confirmed for all six such items (table in
F3). The mechanism is sound: a `const` item is evaluated at every monomorphized use, and
`let () = Self::_CHECK_BITS;` inside the initializer makes that evaluation transitively force the
assertion. The only remaining unguarded associated item is `TAG_BITS` (F3), which is low severity
but makes the doc's absolute closing sentence untrue as written.

### 4.5 Doc accuracy (#705) — **both spot-checks pass**

- **Tag budget.** `2^32 / 100_000 / 3600 = 4_294_967_296 / 100_000 / 3600 = 11.93` hours — "≈ 12
  hours" is correct. The unchanged 48-bit figure also checks out:
  `2^48 / 100_000 / 31_536_000 = 89.25` years. The old "~43 s" was indeed ~1000× low (the true
  value in seconds is ~42,950), so the commit's "~1000x too small" framing is accurate, and both
  figures now sit at the same stated 100k pushes/sec rate. README (`:51-53`) and crate doc
  (`src/lib.rs:94-98`) agree with each other.
- **`pack`'s "truncated, not collided".** Accurate. `src/lib.rs:236` is
  `(tag << INDEX_BITS) | (index & Self::INDEX_MASK)` — the mask is applied to `index` *before* the
  `|`, so an over-wide index can never set a tag bit; the tag half round-trips intact. The
  corrected doc's stated failure mode ("a wrong index round-tripping out of `unpack`") is right,
  though see F10 for the sharper sub-case it does not name.

### 4.6 CHANGELOG.md accuracy — **structurally accurate; two wording defects**

All five commit SHAs cited in `CHANGELOG.md:195-199` are correct and in landing order
(`1485bb6`, `c552704`, `17185d6`, `96e28a7`, `d156e5c` — each verified against `git show`). The
"**Runtime improvements: 1**" header at `:193` is honest: only #698 changed shipping behaviour,
and it is framed as a correctness fix rather than a speedup, which matches CLAUDE.md's R30-12
taxonomy intent. Each bullet's substantive description matches its diff. Two defects: the
mis-attributed panic-message quote (**F2**) and the "torn read" wording (**F11**). One framing
caveat worth naming: the #702 bullet's narrative reads as though the scheduling-independent
conservation invariant fixed the round's oracle problem generally; it fixed it only in the
companion test (**F1**).

### 4.7 Test-coverage gaps a `/rust-intel` audit would still flag before first publish (task #661)

Ranked by what a pre-publish audit would actually care about:

1. **F1** — the headline non-vacuousness proof does not prove non-vacuousness, and the claim is in
   the crates.io `description`.
2. **F4** — no release-profile test run; the newly-promoted `assert!`'s release behaviour is
   unverified in CI.
3. **F5** — no bare-metal build despite the `no-std::no-alloc` category claim, and the
   `compile_error!` portability guard is untested.
4. **F7** — the conservation oracle in the newest loom test is one-sided (duplication only).
5. **F10** — `pack`'s truncation contract (rewritten this round) has zero tests.
6. Pre-existing and honestly self-documented, not a regression from this round: no `trybuild`
   compile-fail test pins `INDEX_BITS > 32` failing to compile (`tests/stack_unit.rs:152-158`
   records this explicitly). Now that `_CHECK_BITS` is forced from `INDEX_MASK` too, a
   `trybuild`/`compiletest` case would be cheap and would also pin F3's boundary.
7. Also pre-existing: there is no plain multi-threaded (non-loom) stress/soak test of
   `push`/`pop` — loom covers 2-thread interleavings up to `preemption_bound = 4`, but nothing
   exercises the real type under many threads on real hardware. Low value given loom's coverage of
   the actual hazard, but a `/rust-intel` audit typically names it.

---

## 5. Categories with nothing to report

Stated explicitly rather than omitted:

- **Out-of-scope edits:** none. Every non-doc hunk is inside `crates/tagged-index-stack/`.
- **TODO / placeholder / half-wired features:** none.
- **Version bumps:** none (the only manifest change is a new dev-dependency, which the round's task
  explicitly called for).
- **New unsafe surface:** none — the crate remains `#![forbid(unsafe_code)]` (`src/lib.rs:126`) and
  no `pub fn` taking a raw pointer was added (the CLAUDE.md benchmark-hook rule is not engaged).
- **`raw_head` posture (#704):** verified end-to-end — `#[doc(hidden)]` present, rustdoc output
  confirmed clean of it, both README and rustdoc policy statements present and mutually
  consistent, and the loom/unit tests that depend on it still compile and pass.
- **`Links` trait stability statement (#705):** present at `src/lib.rs:292-298`, accurate (the
  trait is genuinely unsealed and has two required methods), and consistent with the crate's design
  rationale.
- **Commit-message claims vs. diffs:** every self-reported "zero-trust catch" in every commit
  message was checked against the landed diff and is genuinely present. None is narrated-only.
- **Clippy / fmt / doc warnings:** clean in all four configurations re-run above.

---

## 6. Suggested follow-up tasks

| Priority | Finding | One-line action |
| --- | --- | --- |
| P1 | F1 | Reopen #702: port the companion test's `accounted`/`dedup` conservation oracle into `counterfactual_untagged_head_lets_aba_corrupt_free_list` (`tests/loom_aba.rs:248-253`); re-verify it still panics, and for the right reason. |
| P2 | F4 + F5 | Add `cargo test --release -p tagged-index-stack` and a `cargo build -p tagged-index-stack --target x86_64-unknown-none` step to `ci.yml`'s `test-workspace` job. |
| P2 | F6 | Update `Cargo.toml`'s `description`, README, and crate doc to say three counterfactuals — before the first publish, since `description` is immutable per published version. |
| P3 | F2 + F11 | Two CHANGELOG wording corrections (`:196` panic quote, `:195` "torn"). |
| P3 | F3 | Soften `src/lib.rs:197-198`'s closing sentence to the mask-touching scope that is actually true, or reference `_CHECK_BITS` from `TAG_BITS` too. |
| P3 | F7 | Strengthen `tests/loom_aba.rs:348-353` to `assert_eq!(accounted, vec![0, 1])`. |
| P4 | F8 + F9 + F10 | Comment/doc hygiene in `tests/loom_aba.rs` and `tests/proptest_pack_unpack.rs`; optionally one `pack` truncation test. |
