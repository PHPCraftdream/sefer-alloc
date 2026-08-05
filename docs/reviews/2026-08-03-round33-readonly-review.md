# Read-only review: Round 33 (tasks #506–#518)

Date: 2026-08-03

Reviewed range: `e3d01b2..HEAD` (20 commits), i.e. from task #506's
`fix(ci): green all five clippy --all-targets rows` through the round's
closing `docs: commit Round 33 session checkpoint`.
`HEAD` at review time = `40241b0810b42c672f3f7c507f21b2de762b782b`.

Review mode: read-only with respect to repository content — the only file
written is this report. Like the Round-32 review, this one **did** execute
verification commands (fmt, all five CI clippy rows, the full `production`
test suite, the loom `remote_free_ring` suite, both verifier scripts, and
**nineteen** of the round's derive scripts); every such command and its
result is quoted inline. Nine derive scripts were run in a way that
rewrites their own committed CSV; `git status` was checked after every one
and the tree returned clean each time (that is itself evidence — see §5).
No allocator code, test, doc, or artifact was modified.

---

## Executive verdict

**Round 33 is the cleanest round in this corpus on the axes it set out to
fix, and its central claims survive independent verification.** All 13
findings from `docs/reviews/2026-08-03-round32-readonly-review.md` were
addressed, `main` is genuinely green on all five CI clippy rows for the
first time since Round 31, the round's flagship claim — **"Runtime
improvements this round: 0"** — is *provably* true (every changed line in
`src/` across all 20 commits is a comment line; `Cargo.toml` gains no
feature-composition change), and the two highest-stakes correctness fixes
(R33-3's loom counterfactual, R33-4's write-site enumeration) are real
fixes, not paper ones.

The three specific claims I was asked to spot-check hardest all hold:

1. **R33-3's rewritten loom counterfactual is genuinely non-vacuous in
   both directions.** Verified: **CONFIRMED**;
2. **R33-4's 4-site `head` enumeration is exactly right** against the
   current source (`:886`, `:911`, `:936`, `:1186`), and matches the new
   drift test's `EXPECTED_HEAD_WRITE_SITE_COUNT = 4`. Verified:
   **CONFIRMED**, with one one-sidedness caveat (finding **G9**);
3. **R33-8's round-trippability claim holds for all 15 scripts** — I
   re-ran every one of them (plus four more) against committed raw data
   with the historical SHA as argv and got **zero** CSV drift across all
   19. Verified: **CONFIRMED**. This is the first time in this corpus a
   reviewer could mechanically re-derive-and-diff the whole gate-report
   CSV surface, and it worked.

**R33-12's backfilled R32-3 report reproduces the original commit
message's numbers exactly** (`realloc_grow` 492,694 → 492,574 = −120 Ir;
all four churn kill-gates byte-exact at 8,055/8,055/8,055/8,311), and the
two raw logs are genuinely two different runs (different worktree paths,
different dependency compile ordering) rather than the false-zero the
report's own §2.4 warns about. Verified: **CONFIRMED**.

Against that, this review raises **ten findings**, none of which blocks
anything. The two that matter:

- **G1 [P2] — `b3b18bb`'s stated justification is factually false, and the
  false fact propagated into `CHANGELOG.md` and the round checkpoint.**
  `docs/perf/paired_ab_runs/` has been force-committed by **eleven** prior
  commits going back to R14-3, including R32-12's own landing commit
  `e88390b` — the exact commit `b3b18bb` names as a precedent that "never
  committed paired_ab_runs/ files". The *action* is defensible; the
  *reason given* is refuted by `git log`.
- **G5 [P2] — Round 33 never touched `docs/perf/OPEN_ITEMS.md` at all**
  (`grep -c "R33" docs/perf/OPEN_ITEMS.md` → `0`). The Round-32 review's
  eleven findings were never entered into either open-items index (only
  F1 reached `docs/CORRECTNESS_OPEN_ITEMS.md`, as item 11), so the two
  items this round left genuinely open (see **G4**, **G6**) are now
  tracked nowhere durable. This is precisely the failure mode CLAUDE.md's
  "Round start: check BOTH open-items indexes" rule (R18-8 / R22-3
  precedents) exists to prevent.

### Did the allocator change for a default `production` user?

**No, and this is verifiable rather than asserted.** The complete `src/`
diff over the whole 20-commit range, filtered to non-comment lines, is
*empty*:

```
$ git diff e3d01b2..HEAD -- src/ | grep -E "^[+-]" | grep -vE "^[+-]{3}" \
    | grep -vE "^[+-]\s*(//|///|//!)"
(no output)
```

Only two `src/` files-worth of change exist at all, both in
`src/alloc_core/remote_free_ring.rs` (R33-4 `7d55209`, R33-10 `b928cfe`),
both doc-comment-only. `Cargo.toml`'s diff is three `required-features`
corrections plus two `[[example]]` registrations — no `[features]` change,
no `production` composition change. The CHANGELOG's
"**Runtime improvements this round: 0**" is accurate.

---

## Local verification actually run

All commands run at `HEAD` = `40241b0`, on Windows 10 Pro,
Intel Core i7-11800H.

| command | result |
|---|---|
| `cargo fmt --check` | **PASS** (exit 0, no output) |
| `cargo clippy --all-targets -- -D warnings` | **PASS** (exit 0) |
| `cargo clippy --all-targets --features experimental -- -D warnings` | **PASS** (exit 0) |
| `cargo clippy --all-targets --all-features -- -D warnings` | **PASS** (exit 0) |
| `cargo clippy --all-targets --features "hardened medium-classes" -- -D warnings` | **PASS** (exit 0) |
| `cargo clippy --all-targets --features production -- -D warnings` | **PASS** (exit 0) |
| `cargo test --features production` | **PASS** — 238 `test result:` blocks, every one `ok` with ` 0 failed;`; zero `FAILED`/`panicked at`/`error` lines; exit 0 |
| `RUSTFLAGS="--cfg loom" cargo test --features alloc-core,alloc-xthread --test loom_remote_ring` | **PASS** — `9 passed; 0 failed` in 0.34 s |
| `node scripts/verify-gate-report.mjs` | `PASS WITH 58 WARNINGS (d=31, e=9, h=3, identity=15) (100 report(s) scanned)`; global check (g) `PASS` |
| `node scripts/verify-commit-prefixes.mjs` | `PASS (with warnings)` — 58 commits linted, **7** direction-2 warnings |
| `node scripts/verify-commit-prefixes.mjs 3f7db16..HEAD` | `PASS (with warnings)` — 96 commits linted, **14** warnings |
| 19 derive scripts re-run against committed raw data | **PASS** — `git status --porcelain` clean (only `?? .claude/`) after every one; see §5 |

All five CI clippy rows are green — **finding F1 [P1] from the Round-32
review is fully closed**, and I confirmed it by running each row's exact
`ci.yml` argv myself rather than trusting `npm run check`'s wrapper.

The known pre-existing flake
(`xthread_large_double_free_no_double_reclaim`,
`docs/CORRECTNESS_OPEN_ITEMS.md` item 12) did **not** fire in either of my
two full-suite runs. No re-run was needed.

---

## 1. Check-item — R33-3 (`3edce28`): is the rewritten loom counterfactual non-vacuous?

**Verdict: CONFIRMED. F2 [P2] is properly closed, and closed for the right
reason.**

I read `tests/loom_remote_ring.rs` at `HEAD` rather than the diff, and
traced the state machine by hand.

*The old defect.* The Round-32 review's complaint was not "the test is
deterministic" — it was that the test's **oracle was invalid**: in the
interleavings where the producer's reads happened before the drain
completed, the ring genuinely *was* full and rejecting was the *correct*
answer, yet the assertion still reported "spuriously overflowed". A
`#[should_panic]` whose assertion is false even when the behaviour is
right is a tautology.

*The fix addresses exactly that.* The rewritten test joins the drain
thread **before** the broken check runs
(`tc.join().unwrap();` preceding the check), so at check time the state is
pinned: `tail = 1`, `head = 1`, `cached_head = 0`, slot empty. The ring
**provably has room** (`1 - 1 = 0 < 1`). The broken check reads
`t = 1`, `ch = 0`, computes `1.wrapping_sub(0) = 1`, which is not `< 1`,
and rejects. The panic therefore now means one thing only: the shadow-only
check rejected a push the ring had room for. The oracle is valid.

*The non-vacuity direction, previously entirely absent, now exists.* The
new companion
`correct_shadow_recheck_admits_after_drain_no_spurious_overflow` puts the
**real** `RingModelShadow1::full_check` in the byte-identical position
(same prefill, same join-first sequencing, same stale `cached_head = 0`)
and asserts it returns `Ok`. It then additionally asserts
`ring.cached_head.load(Relaxed) == 1`, which pins that the *slow* path ran
and refreshed — so a future refactor that accidentally shortcuts the fast
path cannot silently re-vacuate the test. That second assertion is a
genuinely thoughtful addition; it is the piece that makes the pair a
proof rather than two anecdotes.

Both directions verified by me: `9 passed; 0 failed`, with
`counterfactual_shadow_trusts_stale_cache_spuriously_overflows - should
panic ... ok` and
`correct_shadow_recheck_admits_after_drain_no_spurious_overflow ... ok`
both present in the output.

*One honest observation, not a finding.* The rewritten counterfactual no
longer exercises concurrency at the moment of the check (the drain is
joined first), so `loom::model::Builder`'s `preemption_bound = Some(3)`
explores interleavings that cannot change the outcome — it is a
deterministic unit test wearing a loom harness. That is *correct* here
(the counterfactual's whole point is a pinned post-drain state), the
commit message says so explicitly ("Rewritten so the drain thread is
JOINED before the broken check runs"), and the test's own doc comment
calls the join-first sequencing "load-bearing". No over-claim survives.
The genuine concurrency coverage for this mechanism lives in
`shadow_ring_never_loses_or_duplicates` /
`shadow_overflow_retry_concurrent_drain_never_loses_or_duplicates`, which
are unchanged and still pass.

The report correction (`R32_11_…GATE.md` §9) is append-only, §5's original
bullet untouched, and its "scratch-swap made the `should_panic` test fail"
claim is exactly what the new companion test now encodes permanently.

---

## 2. Check-item — R33-4 (`7d55209`): is the 4-site enumeration complete and accurate?

**Verdict: CONFIRMED on the enumeration; PARTIAL on the drift test's own
claim about what it pins (finding G9).**

I re-derived the write-site set from the current source myself:

```
$ grep -n "head().store(\|HEAD_OFF" src/alloc_core/remote_free_ring.rs
886:  self.head().store(head, Ordering::Release);        # dbg_set_cursors
888:  self.cached_head().store(head, Ordering::Relaxed); # (cached_head, not head)
911:  self.head().store(head, Ordering::Release);        # dbg_advance_head_only
936:  Node::write_u32(Node::offset(ring.base, HEAD_OFF) as *mut u32, 0);   # init_in_place
942:  Node::write_u32(Node::offset(ring.base, CACHED_HEAD_OFF) ..., 0);    # (cached_head)
954:  Node::atomic_u32_at(self.base, HEAD_OFF)           # READ accessor
1186: self.head().store(h, Ordering::Release);           # drain
```

Exactly **four** writes to `head`: `dbg_set_cursors`,
`dbg_advance_head_only`, `init_in_place`, `drain`. That is precisely what
the corrected module doc (`src/alloc_core/remote_free_ring.rs:105-131`)
enumerates, in the same order of significance, with `drain` correctly
identified as "the ONLY production write". The R32-3-era falsified "the
only OTHER write site" phrasing is gone.

The new `dbg_advance_head_only` precondition — *"MUST NOT regress `head`
below its current value"*, with the stale-HIGH-shadow consequence spelled
out — is the exact fix the Round-32 review asked for, and it matches
`dbg_set_cursors`'s existing precondition style.

*The drift test's filter is sound.* I checked the two false-positive
risks it documents: `self.cached_head().store(` does not contain
`self.head().store(` as a substring, and `", CACHED_HEAD_OFF)"` does not
contain `", HEAD_OFF)"` as a substring (the preceding characters are
`D_`, not `, `). The read accessor at `:954` uses `atomic_u32_at`, not
`write_u32`, so it is excluded. The count the test computes is genuinely
4.

`docs/ARCHITECTURE.md`'s test-file count bump 235 → 236 is also correct:
`ls tests/*.rs | wc -l` → `236`.

### G9 [P3] — the drift test is one-sided: it pins the source, not the doc, yet claims to pin both

`tests/remote_free_ring_head_write_sites.rs`'s module doc says it "fails
if a new site is added or an existing one removed **without updating both
this test's expected count AND the module doc's enumeration**", and cites
`tests/ci_clippy_matrix_consistency.rs` as its pattern. But
`ci_clippy_matrix_consistency.rs` cross-checks **two files** against each
other; this test reads only `src/alloc_core/remote_free_ring.rs` and
compares a line count against a hardcoded `4`. Consequences:

1. It cannot detect the failure that actually happened. If someone edits
   the module doc back to "the only OTHER write site" while leaving the
   code alone, the test still passes. The defect class this test was
   written to prevent is documentation drift, and documentation drift is
   the one thing it does not observe.
2. A same-count swap (delete one write site, add a different one) leaves
   the count at 4 and passes silently.

Cheap fix, in the same file and the same style: also scan the module doc's
own numbered list (e.g. count lines matching `^//! \d+\. \[` between the
F10 section markers) and assert that count equals
`EXPECTED_HEAD_WRITE_SITE_COUNT` too. That closes (1) at the cost of about
six lines. (2) is inherent to count-pinning and is fine to leave.

Severity is P3 because the *substantive* correction landed and the source
side is genuinely pinned; what is wrong is the strength of the claim the
test makes about itself — the same over-claiming class the Round-32 review
flagged as F2/F3.

---

## 3. Check-item — R33-5 (`81d24f9` + `b3b18bb`): the latency null and the gitignored-scratch self-correction

**Verdict: CONFIRMED on the measurement; PARTIAL on the self-correction —
its stated justification is false (G1) and it left two stale citations
behind (G2).**

### 3.1 The measurement itself is the strongest in the round

Everything the report claims is reproducible from the committed artifacts,
and I reproduced it:

- The derive script re-runs clean against the committed
  `_raw_r33_5_*.log` files and regenerates
  `docs/perf/R32_10_LATENCY_NULL_PAIRED_AB_summary.csv` **byte-identically**
  (`git status` clean afterwards).
- Every row of §11.1's Markdown table matches the CSV exactly, cell for
  cell, all 7 K values (`31.466/28.607/2.8591/-9.09/1.593` at K=4 through
  `28.585/29.137/-0.5517/1.93/-1.729` at K=64).
- The `t`-cross-check against the runner's own printed summary is
  genuinely active, not dead code:
  `docs/perf/_raw_r33_5_k64_before_after.log:1127` prints
  `t=-1.729  df=19  crit(p<0.05)=2.101` and `:1128` prints
  `sign test: before-faster=13/20  after-faster=7/20`, both of which the
  script parses and compares against its own recomputation.
- The path-activation claim "every one of the 1,680 launches passed
  oracle #2" is exact:
  `cat docs/perf/_raw_r33_5_k*.log | grep -c "RESULT oracle2_pass=1"` →
  **1680**, and each individual log has exactly 80.

The headline is honest: max `|t|` = 1.729 vs `crit` = 2.101, no K
significant, no sign test more lopsided than 13/7, and all 14 same-vs-same
controls non-significant. §11.1's "One directional observation worth
recording honestly" paragraph — noting that the K=4 direction *reversed*
relative to §4.1's original single-run numbers, and saying plainly that
§4.1's +8.8% was noise — is exactly the right way to report a null that
disagrees with your own prior table. This closes F4 [P2] properly.

The patch-hash provenance
(`9da1a54e83cec28adae585eeb1d2e55a93f44581f9471f13b268ff9fe85892ae`) is
**64 hex characters** — I counted it, precisely because R29-13's
equivalent was found to be 63 and unreproducible. Its recipe's premise
also checks out: `git show 7d55209:src/alloc_core/segment_table.rs`
contains `pub(crate) const OWN_CACHE_SIZE: usize = 16;` at `:180`. I did
not attempt to recompute the hash, because doing so requires mutating the
working tree and because `git diff` output is sensitive to index-blob
lines; that limitation is inherent to R29-6 option 3, not a defect of this
report.

### G1 [P2] — `b3b18bb`'s justification is factually false, and the false fact propagated into CHANGELOG.md and the checkpoint

`b3b18bb`'s commit message states, as the basis for untracking 21 JSON
files:

> "the commit force-added 21 JSON files (5.1MB) under that path, **the
> only commit in this repo's history to touch it**. R32-11/R32-12 (the
> precedent this task modeled) **never committed `paired_ab_runs/`
> files**."

Both sentences are false:

```
$ git log --oneline --all -- docs/perf/paired_ab_runs/
b3b18bb  (the untracking commit)
81d24f9  (R33-5)
e88390b  perf(runtime): large-cache occupancy bitmask …  (R32-12, task #503)
e6bbc6a  fix(alloc): trim_current_thread … (R32-1, task #492)
c72a27b  bench(perf): virgin-zero-skip cost-side …      (R32-0, task #490)
4f89723  bench(perf): resolve R31-3's segments_reserved …(task #488)
032048b  bench(perf): pool-cap sweep 8/16/32 …          (R31-2, task #465)
b5efe8c  perf(profiles): named rss/balanced/throughput … (R30-7, task #456)
97c2f07  perf(large-cache): headroom_bytes BENEFIT-side A/B (R30-6, task #455)
7d60ee4  perf(global): … REAL #[global_allocator]       (R27-4, task #422)
5537a20  perf(global): … pool cap 4->8 latency win      (R26-3, task #412)
5709c24  docs(perf): re-verify class-aware-dirty …      (R17-7)
6d85db4  docs(perf): honest sub-window vs full-round …  (R14-3, task #288)
```

Eleven prior commits touched the path, and `e88390b` **is** R32-12 — the
named precedent — which added
`docs/perf/paired_ab_runs/2026-08-02T21-17-17-448Z.json` and
`…T21-17-29-807Z.json`. `git ls-files docs/perf/paired_ab_runs/ | wc -l`
returns **33** at `HEAD`: the directory is `.gitignore`d *and* has 33
deliberately force-added files in it, which is the established
"`git add -f` the cited evidence" pattern CLAUDE.md's raw-log policy
describes.

Worse, those tracked JSONs are **load-bearing**:
`docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md:306-324` cites
`paired_ab_runs/2026-08-02T00-18-11-335Z.json` and
`…T00-19-14-627Z.json` as its evidence, and
`scripts/r31_10_derive_cost_report_data.mjs` takes exactly those two files
as **required positional arguments**. So the repo's convention is not just
"sometimes committed" — one gate report is not reproducible without them.

The false claim then propagated into two more durable artifacts:

- `CHANGELOG.md:10` — "the first time that path had ever been touched in
  this repo's history";
- `docs/checkpoints/2026-08-03-2255.md:11` — "never before committed in
  this repo's history".

**What I am and am not saying.** The *action* (untrack 5.1 MB, re-derive
from the already-committed `_raw_*.log` files) is defensible and arguably
better: the logs contain the same per-launch `RESULT` lines, the CSV's
numeric columns came out byte-identical, and 5.1 MB is a lot of repo for
one addendum. I would not reverse it. What I am flagging is that (a) the
stated reason is refuted by one `git log` invocation the zero-trust review
did not run, (b) the repo now has two contradictory conventions for the
same artifact class with no note reconciling them, and (c) the error is
now written into `CHANGELOG.md`, which is the durable record. The minimum
fix is a correction line in the CHANGELOG bullet plus one sentence in
`R32_10_…GATE.md` §11.4 acknowledging that `paired_ab_runs/` JSONs *are*
force-added elsewhere and explaining why this addendum chose the log route
instead.

### G2 [P3] — two stale citations survived `b3b18bb` inside the very section it rewrote

`b3b18bb`'s message says "§11's **three** `paired_ab_runs/` citations
updated". Three were. Two others in the same §11.4 bullet list were not,
and they now contradict the bullet immediately above them:

1. > **Checked derive script:** `scripts/r33_5_latency_null_addendum_summary.mjs`
   > (**reads the 21 JSON files**, recomputes t-test/sign-test from raw
   > samples …)

   It does not. The script's own header says "DATA SOURCE:
   `docs/perf/_raw_r33_5_*.log` … This script does NOT read from
   `docs/perf/paired_ab_runs/`", and its loader builds
   `` `_raw_r33_5_k${k}_${fileComp}.log` `` paths.

2. > … the measurement's source identity is captured in the prose above
   > (HEAD SHA + patch hash) **and in each JSON file's own `git_commit`
   > field — both more durable** than a CSV column …

   Those JSON files are untracked as of the same commit. Citing them as
   the durable identity carrier directly contradicts the preceding bullet
   ("… are NOT committed or cited as evidence").

Both are one-line edits. Neither affects a number.

### G8 [P3] — R33-5's summary CSV carries no source-identity fields at all

`docs/perf/R32_10_LATENCY_NULL_PAIRED_AB_summary.csv`'s header is:

```
k,comparison,arm_a,arm_b,n,mean_a_ns_per_op,mean_b_ns_per_op,delta_ns_per_op,
pct_change,t,crit,significant,sign_a_faster,sign_b_faster,sign_ties,provenance_file
```

CLAUDE.md's R14-10 summary-CSV rule asks the companion to hold "commit
SHA, active feature set, CPU/OS identification, **and** sample count". It
has `n` (sample count) and nothing else from that list. §11.4 argues
carefully for dropping `landing_commit` — and that argument is good, it is
the F6 lesson correctly applied — but it does not address the other three
fields, which have no chicken-and-egg problem at all. The same round's
other new CSV (`R33_6_DECAY_THROTTLE_RETENTION_COST_summary.csv`) carries
`cpu,os,feature_set,measurement_commit,base_commit`, so the round already
demonstrates the right shape one task over. A future cross-round grep for
"what hardware/feature set was this measured on" cannot answer the
question from this file.

### G7 [P3] — one tautological assertion and one soft-failing cross-check in `r33_5_latency_null_addendum_summary.mjs`

Two small defects in a script whose entire purpose is to make claims
mechanically enforced:

1. **Assertion 2 is vacuous by construction.** Under the comment
   `// 2. For before_vs_after: assert the significance verdict is correctly
   computed.`, the script computes
   `rederivedSignificance = Math.abs(r.tTest.t) > r.tTest.crit` and
   compares it against `r.tTest.significant` — which `pairedTTest` set,
   thirty lines earlier, as `crit != null && Math.abs(t) > crit`. The two
   expressions are identical, so the comparison can never fail. This is a
   miniature of the same "assertion that cannot fail" class the Round-32
   review raised as F2.
2. **The log cross-check is guarded but the claim is unconditional.** The
   cross-checks are `if (parsed.logT != null && …)` and
   `if (parsed.logSignA != null && …)`, so if the runner's output format
   ever drifts and the regexes stop matching, both checks silently become
   no-ops — while the script still prints
   `Cross-checked against each log's own printed t and sign-test: YES
   (asserted)`. I verified the regexes *do* currently match (see §3.1), so
   the published numbers are genuinely cross-checked; the defect is
   latent, not active. Fix: `throw` when `logT === null` rather than
   skipping.

---

## 4. Check-item — R33-6 (`5bd7c04` + `8a04452`): retention cost in the low-throughput regime

**Verdict: CONFIRMED. F9 [P2] is closed well, and this is the round's best
new measurement artifact.**

The harness is the right shape and I could not find a hole in it:

- **Subprocess-per-arm** (`run_one_child`), so cross-arm registry-state
  leakage is impossible by construction — the R26-4 rule's structural
  answer, not a promise.
- **Config evidence, hard-asserted in the parent**:
  `assert_eq!(m.get("verified_headroom"), profile.headroom_bytes as u64)`
  and `assert_eq!(m.get("config_conflicts_delta"), 0)` for every one of
  the 48 arms.
- **Path-activation oracle in two pieces** (R30-8/R31-0 rule):
  `headroom_crossed == 1` (hard-asserted in the parent) plus
  `guard_passed_delta` compared against `expected_calls`. The derive
  script `throw`s if the forced arm's guard delta ≠ `expected_calls`, and
  `throw`s if the unforced arm's is not strictly lower — i.e. it fails
  loudly if the throttle is a no-op. That second assertion is the one that
  makes the arm labels trustworthy.
- **The derive script asserts direction and bound**, not just prints:
  `throw` on negative retention cost, and `throw` if the cost exceeds one
  segment. Re-running it reproduced
  `R33_6_DECAY_THROTTLE_RETENTION_COST_summary.csv` byte-identically.
- Every §9.2 table cell matches the CSV (`LowHeadroom,1,288.00,252.00,
  288.00,37748736,36.00,2,0,2,3` ↔ the table's first row).

§9.2's framing deserves specific credit: it presents cost and benefit side
by side while stating explicitly that they were measured in *different*
regimes and refusing to combine them into a Pareto claim. That is CLAUDE.md's
R31-12 rule being followed *proactively*, in the report that would most
easily have violated it.

Two nits, neither a finding:

- The example's console `cost = median(uf).saturating_sub(median(fd))`
  would silently print `0` for a negative (unexpected-direction) result.
  It does not matter, because the derive script — which produces the CSV
  and the report's tables — recomputes signed and `throw`s on negative.
- The parent hard-asserts `headroom_crossed`, `verified_headroom` and
  `config_conflicts_delta`, but only *warns* on the composite
  `oracle_pass == 0`. Again covered downstream: the derive script `throw`s
  on `oracle_pass !== "1"` for every row.

The ~29-op stride-alignment threshold derivation (`7 + 2 × n_ops ≥ 64`) is
arithmetically consistent with the observed 8-vs-32 boundary, and the
report correctly labels it workload-specific while arguing the *bound*
(one segment per missed interval) is invariant.

---

## 5. Check-item — R33-8 (`b537770`): are the 15 derive scripts really round-trippable?

**Verdict: CONFIRMED, and verified far past the sample the brief asked
for. This is the round's highest-leverage change.**

I ran **19** derive scripts — all 15 R33-8 touched, plus `r497`
(the pre-existing positive precedent), `r33_5`, `r33_6`, and R33-12's new
`r32_3` — each against its own committed raw data with its historical SHA
supplied as argv, and checked `git status --porcelain` after every single
one:

| script | historical SHA passed | result |
|---|---|---|
| `r31_0_summary` | `dece4a7025f8…` | CLEAN |
| `r31_1_derive_report_data` | `fc11cf3c0391…` | CLEAN |
| `r31_8_derive_report_data` | `4f897237cf6e…` | CLEAN |
| `r31_10_derive_cost_report_data` | `e6bbc6acbc3f…` (+ 2 JSON args) | CLEAN |
| `r32_0_derive_report_data` | `c72a27ba408f…` | CLEAN |
| `r32_7_large_cache_hit_summary` | `eb2463a449ca…` | CLEAN |
| `r32_8_decay_clock_read_summary` | `74345b8b3323…` | CLEAN |
| `r32_9_derive_smoke_summary` | `2ea920b98fbf…` | CLEAN |
| `r32_10_own_cache_tier1_summary` | `5289c6618774…` | CLEAN |
| `r32_10_killgate_addendum_summary` | `2c825b2e7bce…` | CLEAN |
| `r32_11_shadow_head_summary` | `d38bf73c63fa…` | CLEAN |
| `r32_12_derive_report_data` | `e88390bc88c8…` | CLEAN |
| `r32_13_windows_decomposition_summary` | `f6c3a61e1e0a…` | CLEAN |
| `r495_stamp_removal_summary` | `cd5c634a29ab…` | CLEAN |
| `r496_perclass_repr_c_summary` | `5df56d376735…` | CLEAN |
| `r497_dualbitmap_summary` | (no SHA column) | CLEAN |
| `r33_5_latency_null_addendum_summary` | (no SHA column) | CLEAN |
| `r33_6_decay_throttle_retention_summary` | `5bd7c04c392a…` | CLEAN |
| `r32_3_realloc_redundant_contains_base_summary` | `f51ec37177b3…` | CLEAN |

Zero drift, 19 for 19. Round 32's F6 — "a future reviewer cannot
mechanically re-derive-and-diff a CSV to verify it" — is genuinely closed;
I just did it for the entire corpus in one pass, which was impossible one
round ago.

(`r31_10_derive_cost_report_data.mjs` takes the SHA as its **third**
positional argument, after two JSON paths — so R33-8's commit message
phrasing "run with the historical SHA as argv" is loose for that one
script. It does round-trip correctly once the two required JSON paths are
supplied. Not a finding.)

Global check (g) in `verify-gate-report.mjs` is a real hard `FAIL` (it
sets `allOk = false`), scans only the `^r\d.*\.mjs$` derive-script family
to avoid self-matching, and correctly does not flag the two sanctioned
shapes (live-derive, or no column at all). Sound.

### G3 [P3] — the live-`git rev-parse HEAD` route silently emits the PARENT commit, and R33-12's own CSV already shows it

R33-8 replaced the loud, obviously-wrong `'UNFILLED_PLACEHOLDER_40_HEX'`
with `process.argv[2] || execSync('git rev-parse HEAD')`. For **historical**
CSVs this is strictly better (proved above, 15/15). But for a **new**
report generated inside its own landing commit, the chicken-and-egg
problem is unchanged — and the new failure mode is quieter than the old
one, because a plausible-looking 40-hex SHA is emitted instead of an
obvious sentinel.

R33-12 is the first new report after the change, and it exhibits exactly
this:

```
$ awk -F, 'NR==2{print $NF}' docs/perf/R32_3_REALLOC_REDUNDANT_CONTAINS_BASE_GATE_summary.csv
f51ec37177b34cf7dd941dd352063a47249b1104
$ git rev-parse 96ae245^
f51ec37177b34cf7dd941dd352063a47249b1104
```

The column is named `doc_commit`, and
`scripts/r32_3_realloc_redundant_contains_base_summary.mjs:67` documents
it as "THIS documentation task's own landing commit". Its committed value
is the **parent** of that landing commit (`96ae245`). Nothing detects
this: check (b) validates 40-hexness only, and check (g) looks for
sentinels only.

This is small in consequence here (the parent is one commit away and the
report's substantive provenance — `f3020fd` / `5d72bc6` — is correct and
in separate, correct columns). But it is a real "fix introduced a smaller
new defect" instance, and it will recur silently on every future
same-commit report. Two clean options: keep passing the SHA explicitly in
a follow-up commit for *new* reports (the old workflow, now without the
sentinel), or follow R33-5's route and omit the column — noting that
R33-5's route has its own cost, see **G8**. The current state is the one
combination that is neither: a column that looks authoritative and is
off-by-one-commit.

R33-6 shows the pattern that actually works: commit the harness first
(`5bd7c04`), measure and derive at that HEAD, then commit the
report — `measurement_commit=5bd7c04…` is then genuinely correct. Worth
writing down as the recommended sequence.

---

## 6. Check-item — R33-11 (`998d373` + `f51ec37`) and R33-13 (`0ec15e1`)

**Verdict: R33-13 CONFIRMED. R33-11 PARTIAL (finding G4).**

*R33-13.* The taxonomy amendment is accurate and the linter change is
correct: `FIX_PERF_RE` is tested **before** the generic `fix(` fallthrough
(which would classify as `'other'` and skip the check), and `fix-perf` is
routed through the direction-1 branch alongside `perf-runtime`/`perf-opt-in`.
The commit's warning-count claim reproduces exactly:

```
$ node scripts/verify-commit-prefixes.mjs            → 58 linted,  7 warnings
$ node scripts/verify-commit-prefixes.mjs 3f7db16..HEAD → 96 linted, 14 warnings
```

I inspected all 7 default-range warnings. Five are Round-32 commits the
prior review already adjudicated legitimate (`f6c3a61`, `2ea920b`,
`c72a27b`, `4f89723`) plus `5bd7c04` (`Cargo.toml` example registration
only). The two new ones are `7d55209` and `b928cfe`, both flagged for
touching `src/alloc_core/remote_free_ring.rs` — and both are
comment-only, which I verified line by line (§"Did the allocator change"
above). All 7 are true-negatives of the "hidden runtime change" question.
No misuse found in the other direction either.

*R33-11.* The two renames landed, all citations were updated (I confirmed
`R32_4_…`, `R32_5_…`, and `R31_0_…`'s cross-reference all point at the new
names, and both derive scripts round-trip — see §5), and new check (h)
works: both previously-broken reports now report
`(h) … PASS — report cites its own-basename CSV`.

### G4 [P3] — check (h) surfaced a third genuine instance of the same defect, which was left unfixed, unindexed, and mislabelled in the CHANGELOG

Check (h) produces 3 warnings. I resolved each by hand:

| report | cited CSV | verdict |
|---|---|---|
| `R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` | `R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv` | legitimate — no own CSV exists, pure cross-reference |
| `R18_9_ADAPTIVE_LARGE_POLICY_DESIGN.md` | `R18_2_MEDIUM_REALLOC_GATE_RERUN_summary.csv` | legitimate — same |
| `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md` | `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_summary.csv` | **NOT legitimate — same defect class as R32_4/R32_5** |

`ls docs/perf/ | grep R30_7` shows exactly one report and exactly one CSV,
whose basenames differ only by the missing `_GATE` — i.e. this is the
report's *own* companion, misnamed, which is precisely what F8 was about.

`998d373`'s commit message is honest about it ("R30_7 (a known
pre-existing '_GATE' suffix mismatch)"), and I would not have expected
R33-11 to rename it unasked. Two things are wrong with the disposition,
though:

1. **It is indexed nowhere.** `grep -n "R30_7" docs/perf/OPEN_ITEMS.md`
   finds only unrelated mentions; `docs/CORRECTNESS_OPEN_ITEMS.md` has
   none. A commit that newly flags an open item must add it to an index in
   the same commit (CLAUDE.md, "Round start" bullet). See **G5** — this is
   the concrete casualty of that gap.
2. **`CHANGELOG.md` rounds it off in the wrong direction.** Both
   `CHANGELOG.md:10` ("3 further pre-existing (legitimate) naming drift
   instances") and the R33-11 bullet at `:26` ("3 further pre-existing
   (legitimate cross-reference) instances") describe all three as
   legitimate cross-references. One of the three is not. The commit
   message was accurate and the CHANGELOG summary lost the distinction —
   which inverts the usual direction (the durable record is normally the
   more careful one).

Fix is trivial either way: rename `R30_7_…_AB_summary.csv` →
`R30_7_…_AB_GATE_summary.csv` and update its two citations, or file it as
an open item and correct the CHANGELOG parenthetical.

---

## 7. Check-item — R33-12 (`96ae245`): does the backfilled R32-3 report reproduce the original numbers?

**Verdict: CONFIRMED, exactly, on every cited figure.**

The original commit `5d72bc6`'s own message cites:

```
realloc_grow:                   492,694 -> 492,574 Ir  (-120, ~7.5 Ir/step x 16)
small_churn_16b:                  8,055 ->   8,055 Ir
medium_class_dealloc_churn_16b:   8,055 ->   8,055 Ir
churn_256b:                       8,055 ->   8,055 Ir
churn_write_256b:                 8,311 ->   8,311 Ir
```

The committed raw logs give exactly those values:
`_raw_r32_3_realloc_before.log:454` → `Instructions: 492694`,
`_raw_r32_3_realloc_after.log:454` → `Instructions: 492574`, and
`:111` → `8055` in both. The committed CSV carries the identical set, and
re-running the derive script reproduces it byte-identically (§5). The
script's three assertions are real `throw`s and hard-code the published
values (`asserted == -120`, `asserted == -7.5` per step, all four
kill-gates `asserted == 0`) — CLAUDE.md's point-6 "assert the arithmetic
you print" rule applied as literally as it can be.

**The two logs are genuinely two different runs**, which is the thing
worth checking hardest given the report's own §2.4 warning about the
shared-`/tmp/sefer-iai`-target false-zero. `diff` between them is 716
lines: different worktree path on line 1
(`sefer-alloc-r517-before` vs `-after`), and a completely different
dependency compile ordering throughout — a cached-artifact reuse would
have produced neither. The only *numeric* difference is `realloc_grow`
(and its L1/L2/RAM companions), which is exactly the expected shape.

§2.4's documentation of that trap is a genuine contribution: it is the
kind of reproduction hazard that silently produces a "(No change)" verdict
and is nearly impossible to diagnose after the fact.

The report also satisfies checks (a), (b), (c), (e), (h) and (identity) in
`verify-gate-report.mjs`, including the entry-point-honesty rule
("**Entry point under test:** `HeapCore::realloc`, reached from
`SeferAlloc::realloc`"). Its only blemish is the `doc_commit` column
discussed as **G3**.

---

## 8. Check-item — the CHANGELOG (`be6552e`) and scope discipline

**Verdict: CONFIRMED on the round-level honesty claim; PARTIAL on two
details (G4 above, G6 below).**

I `git show --stat`'d all 20 commits against their messages. Every commit
touches only files its message names; no out-of-scope edit, no stray
`.gitignore` change, no accidental re-commit of the untracked `.claude/`
directory (which was untracked before the round and is untracked now).
The one genuinely accidental commit of the round (`81d24f9`'s 5.1 MB of
gitignored JSON) was caught and reversed by `b3b18bb` — see **G1** for
what went wrong in the *reasoning*, not the outcome.

"**Runtime improvements this round: 0**" is verified true, by the
empty-non-comment-diff test quoted at the top of this report. This is the
first round in this corpus where that claim is mechanically demonstrable
rather than asserted, and the CHANGELOG paragraph even names the check
("the one `src/` diff … is comments-only").

The Round-33 entry is placed correctly at the top of the file, is
detailed, and its per-task bullets carry accurate `[tag, Pn]` labels.

### G6 [P3] — F11 is only partially closed: Round 31's section still collides, and Rounds 31/32 are out of order

R33-7 correctly created a `#### Runtime improvements` subsection under
Round 32 with an accurate `Runtime improvements this round: 7` line, and
moved the seven bullets verbatim. But:

1. **The original F11 collision still exists one section down.**
   `CHANGELOG.md:36` reads "**Runtime improvements this round: 0.**", and
   `:38` is `#### Runtime improvements`, whose bullets include
   `- **[runtime improvement, P2] R31-10 (task #474) — promoted … to a
   documented public trim_current_thread() API …**`. That is the exact
   shape F11 described (a bolded zero-count two lines above a heading
   listing runtime improvements), just with 2 bullets instead of 8.
   R33-7's message says Round 31's line "is scoped to R31-0 specifically
   by its own text" — that is true of the sentence's *content*, but F11's
   whole point was that CLAUDE.md treats the literal string "Runtime
   improvements this round: N" as a **round-level** signal, and a skimming
   reader gets the wrong one. Round 31 shipped at least one runtime
   improvement (R31-10) by the CHANGELOG's own tagging.
2. **Section ordering is wrong, and R33-7 touched both sections without
   fixing it.** `grep -n "^### Round"` gives Round 33 (`:10`), Round 31
   (`:30`), Round **32** (`:65`), Round 30 (`:88`), … — newest-first
   everywhere except that 31 and 32 are swapped. This is pre-existing
   (identical at `e3d01b2` and at `ab6e0d6`), so it is not a Round-33
   regression; it is in scope only because R33-7 was the task that
   restructured precisely these two headings.

*Correction to the Round-32 review, for the record:* that review's F11
stated "`grep -n "^### Round" CHANGELOG.md` confirms no `### Round 32`
heading exists". That was **wrong** — the heading existed at `ab6e0d6`,
at line 52, merely out of order. `188b222`'s commit message says so
plainly ("Round 32's **existing** `### Round 32` heading"). Declining to
inherit a prior review's factual error is exactly the right behaviour and
deserves noting.

### G5 [P2] — Round 33 never touched `docs/perf/OPEN_ITEMS.md`; the Round-32 review's findings were never indexed anywhere durable

```
$ grep -c "R33" docs/perf/OPEN_ITEMS.md
0
$ grep -n "round32-readonly-review" docs/perf/OPEN_ITEMS.md docs/CORRECTNESS_OPEN_ITEMS.md
(no output)
```

The round's entire task queue came from a review document. Of its eleven
findings, exactly one (F1) ever reached an index — as
`docs/CORRECTNESS_OPEN_ITEMS.md` item 11, which R33-1/R33-2 then correctly
updated and half-resolved. F4, F5, F6, F8, F9, F10 and F11 are all
squarely in `docs/perf/OPEN_ITEMS.md`'s stated scope (perf gate reports
and perf design docs) and none was ever filed there — neither when opened
by the review nor when closed by this round.

CLAUDE.md's own rule states the requirement and the reason:

> "When a gate report / commit / review newly flags an open item, add it
> to the appropriate index in the same commit; when one is closed, move
> it to that index's 'Recently resolved' trail … the in-session TaskList
> does not survive a session boundary, so a fresh session inherits no
> memory of prior rounds' flagged-open items — these indexes do."

Nothing was lost *this* time, because the whole round ran inside one
session that held the TaskList. But the two items this round left open —
**G4**'s `R30_7` CSV mismatch, and F11's residual Round-31 collision
(**G6**) — are now recorded only in a commit message and in this report.
That is the R18-8/R22-3 situation reproducing itself: an item that
survives only in prose is an item a fresh session will not inherit.

I rate this P2 rather than P3 because it is a *process* regression in the
one process this project has repeatedly identified as its load-bearing
memory across sessions, and because it is cheap to fix (one
"Recently resolved" block listing F1–F11 with their closing commits, plus
one new open item for `R30_7`).

---

## 9. Things I checked and found sound (no finding)

- **R33-1 (`e526517`) is a better fix than the brief asked for.** The
  brief prescribed "one doc-indent + adding the missing `fn main`"; the
  agent found `fn main` already existed and correctly diagnosed that
  `examples/r31_10_trim_cost_gate.rs` was *auto-discovered* with no
  `[[example]]` entry, so its own `#![cfg(all(...))]` stripped the crate
  under feature sets lacking `alloc-decommit`. Registering it with
  `required-features` is the right fix and mirrors its sibling
  `r31_10_trim_rss_gate`. It also found three failures the brief did not
  know about. The `fast_after >= fast_before + 1` → `fast_after >
  fast_before` rewrite is semantically identical for `u64`.
- **R33-2's root-cause claim is verifiable and I verified it.**
  `scripts/check-all.mjs:65` does
  `PER_PR_ROWS.filter(r => r.kind === 'clippy').map(...)` and splices all
  five generated rows into its step list — so "the local gate has no hole"
  is true, and "procedural, not a coverage gap" is the correct diagnosis.
  The three compile errors (E0601/E0432/E0599) genuinely cannot be
  toolchain drift. Rejecting a mandatory pre-push hook as
  out-of-character, and adding a post-push CI-confirmation step instead,
  is a well-argued and appropriately-scoped disposition.
- **R33-9 (`454149e`) resolves the dangling cross-reference in the right
  place.** §5.2 says "see the provenance note in §8 below"; the new note
  was appended into §8 (`## 8. Provenance` spans `:531`–`:594`), so the
  cross-reference now lands on a note about the correct arm. Its
  four-channel recoverability check (`git stash list`, `git worktree
  list`, reflog-unreachability of a never-objectified working-tree change,
  no saved patch) is correct reasoning, and declining to manufacture a
  post-hoc hash is exactly what CLAUDE.md's point-7 rule requires. The two
  reproducible arms' SHAs (`ce3f44da…`, `5289c661…`) both resolve.
- **R33-10 (`b928cfe`) states the assumption honestly and declines the
  structural fix for a defensible reason.** The added paragraph names the
  precondition ("staleness lag strictly below `2^32` real head-advances"),
  the mechanism (`Acquire` load then `Relaxed` store, two adjacent
  instructions), the consequence at `k = 1`, and the practical weight. The
  rejection of a `fetch_max` refresh cites R32-11's own measured finding
  that a locked RMW on this path made the fix look like a regression
  (`t = -13.3`, reproduced 3×) — declining to add unmeasured RMW cost to a
  hot cross-thread path for a P3 hazard is consistent with this project's
  own cost discipline, and the reasoning is recorded rather than implied.
- **Every commit SHA I sampled from the round's docs resolves.** I ran
  `git cat-file -t` over `ce3f44da…`, `5289c661…`, `0985e22d…`,
  `4f897237…`, `e6bbc6ac…`, `d38bf73c…` (all `commit`) and
  `3dfa28c9…` (`tree`, correctly cited as a tree SHA). The one non-git
  hash (`9da1a54e…`) is a 64-char sha256, correct length.
- **No short-SHA regression anywhere in the round's new artifacts.** I
  scanned every `.csv`/`.md` file the range touched for 7–39-char hex
  tokens; the only hits are in prose contexts (`CHANGELOG.md`,
  `CLAUDE.md`, table column headers like `before Ir (f3020fd)`), never in
  a `landing_commit`/`base_commit`/`git_commit` field. `def1bd9` caught
  and fixed the one real instance mid-round, and the fix is correct.
- **No new `unsafe`, no new `pub fn` taking a raw pointer, no new
  benchmark hook.** The R25-1 hazard shape cannot be present: the round's
  `src/` diff contains zero non-comment lines.
- **`tests/remote_free_ring_head_write_sites.rs` is correctly designed as
  a link-free source-text guard** (`use std::fs` only), so it runs in
  every feature configuration, including the four clippy rows that do not
  enable `alloc-xthread`.
- **`scripts/_r33_5_own_cache_ab.json` is correctly kept committed** — it
  is a runner *input* config, not scratch output, matching R32-11's
  `docs/perf/r32_11_run.json` and R32-12's
  `scripts/_r32_12_free_slot_search_ab.json`.

### G10 [P4, nit] — `R32_10_OWN_CACHE_TIER1_THRASH_GATE.md` numbering skips §9 and §10

`grep -n "^## "` on that file gives `… ## 7 … ## 8 … ## 11`. R33-5 numbered
its addendum §11 with no §9 or §10 in existence (the sibling
`R32_11_…GATE.md` legitimately has §9 and §10, which is the likely source
of the confusion). Cosmetic; renumbering an append-only corrected report
after the fact is probably not worth the diff, but a one-line note at the
top of §11 would prevent a future reader hunting for missing sections.

---

## 10. Recommended dispositions

Ordered by what I would do first. **Nothing here blocks; nothing requires
reverting any of the round's 20 commits.**

1. **G1 [P2]** — correct the false "never before committed" claim. One
   sentence in `CHANGELOG.md`'s R33-5 bullet and one in
   `R32_10_…GATE.md` §11.4 acknowledging that `paired_ab_runs/` JSONs
   *are* force-added elsewhere (11 commits, incl. R32-12's own `e88390b`)
   and that `R31_10_…RSS_GATE.md` depends on two of them — then state
   affirmatively why the `_raw_*.log` route was preferred here (size;
   the logs carry the same per-launch data). This is the only finding
   that leaves a false statement in the durable record.
2. **G5 [P2]** — add a "Recently resolved" block to
   `docs/perf/OPEN_ITEMS.md` covering the Round-32 review's F4/F5/F6/F8/
   F9/F10/F11 with their closing commits, and file **G4**'s `R30_7`
   mismatch as a new open item. This restores the cross-session memory
   the round otherwise relied on the in-session TaskList for.
3. **G2 [P3]** — two one-line edits in `R32_10_…GATE.md` §11.4: the
   "reads the 21 JSON files" bullet and the "each JSON file's own
   `git_commit` field" clause.
4. **G4 [P3]** — either rename
   `R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_summary.csv` →
   `…_AB_GATE_summary.csv` (2 citations to update), or file it per (2);
   either way fix the CHANGELOG's "(legitimate cross-reference)"
   parenthetical, which is true of 2 of the 3 warnings, not 3.
5. **G3 [P3]** — decide and document the same-commit convention: either
   pass the SHA explicitly in a follow-up commit (old workflow, sentinel
   removed), or omit the column; and fix R33-12's `doc_commit` value,
   which currently holds `96ae245`'s parent. R33-6's
   commit-harness-first-then-measure sequence is the pattern worth
   writing down.
6. **G6 [P3]** — give Round 31 its own honest runtime-improvement count
   (it is not 0 by the section's own bullet tags), and move the
   `### Round 32` section above `### Round 31` so the file is uniformly
   newest-first.
7. **G7 / G8 / G9 [P3]** — small and independently cheap: drop or
   strengthen the tautological assertion in
   `r33_5_latency_null_addendum_summary.mjs`, make its log cross-check
   `throw` on a null parse, add `cpu`/`os`/`feature_set` columns to
   `R32_10_LATENCY_NULL_PAIRED_AB_summary.csv`, and teach
   `tests/remote_free_ring_head_write_sites.rs` to also count the module
   doc's own numbered list so it pins both sides of the pair it claims to
   pin.
8. **G10 [P4]** — optional one-line note about the §9/§10 numbering gap.

### On the `/crush` delegation experiment

Worth recording, since this round was the first to use an external CLI
sub-agent rather than the in-house Agent tool. From the outside — reading
only the artifacts — the sub-agent's output is of comparable quality to
Round 32's, and in two places (R33-1's re-diagnosis of the E0601 cause,
R33-7's refusal to inherit the review's "no `### Round 32` heading"
error) it was *more* accurate than its own brief. The two defects the
orchestrating session caught (short SHA, gitignored scratch commit) are
both recurring house bug classes, not novel failure modes.

The two P2 findings above are, however, both **verification-side**
failures rather than generation-side ones: **G1** would have been caught
by running `git log -- docs/perf/paired_ab_runs/` before believing the
sub-agent's "only commit in history" claim, and **G5** would have been
caught by the round-start checklist CLAUDE.md already mandates. Neither is
a `/crush` problem; both are the zero-trust review trusting a *stated
fact* where it rigorously re-ran every *stated command*. That asymmetry —
commands re-run, factual assertions accepted — is the one process gap this
round's own retrospective did not name.
