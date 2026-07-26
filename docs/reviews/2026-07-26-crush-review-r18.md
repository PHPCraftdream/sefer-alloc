# Independent read-only review — Round 18 (tasks #329–#336, commits `dc95d1a..3b8fdc0`)

**Reviewer:** Crush (independent pass). **Date:** 2026-07-26.
**Scope:** Round 18 only (9 commits). READ-ONLY — no build/test/bench run; all
numbers are taken from already-committed raw logs / CSVs / gate reports, verified
against `git show` and source.

**Method:** independent conclusions formed FIRST from `git log`, `git diff`,
`CHANGELOG.md`, `docs/perf/*.md`, and source — the three prior R13–R17 reviews
were read only afterward for cross-check (§6).

**Round 18 commit inventory (verified via `git log dc95d1a~1..3b8fdc0`):**

| commit | task | what | touches |
|---|---|---|---|
| `dc95d1a` | R18-1 / #329 | watchdog `abort()`→`exit(124)` + deadline env | `tests/race_repro.rs` |
| `912740f` | R18-3 / #330 | narrow R17-4 `kind_at` check to promotion-reachable domain | `src/registry/heap_core_free.rs`, `benches/perf_gate_iai.rs`, `scripts/iai.mjs` |
| `8833baa` | R18-2 / #331 | re-run R10-2 realloc kill-gate after R17-4/R18-3 — HONEST RED | `docs/perf/R14_4_*.md`, CSV, 3 raw logs |
| `290374b` | R18-7 / #332 | mimalloc-plan status: EXHAUSTED not dormant | `docs/perf/R18_7_*.md` |
| `4ba35dc` | #332 follow-up | correct R18-7's CI-gate claim (perf-gate.yml already exists) | `docs/perf/R18_7_*.md` |
| `1d2c9cd` | R18-4/5 / #333 | fix contradictory pad-target wording + class-aware-dirty framing | `CHANGELOG.md`, `docs/perf/R14_3_*.md`, `src/registry/heap_core_free.rs` (comment only) |
| `ed75b06` | R18-6 / #334 | design note for stale derived-literal comment guard | `docs/DESIGN_stale_literal_guard.md` |
| `60633e3` | R18-9 / #335 | unified adaptive Large policy design doc | `docs/perf/R18_9_*.md` |
| `3b8fdc0` | R18-8 / #336 | OPEN_ITEMS.md tracking convention + CLAUDE.md bullet | `CLAUDE.md`, `docs/perf/OPEN_ITEMS.md` |

**Headline character of the round:** of 9 commits, only **2 touch `src/`**
(one substantive correctness narrowing `912740f` + one 2-line comment fix
`1d2c9cd`) and **1 touches tests** (`dc95d1a`); the other **6 are docs/design/
process**. `Cargo.toml` is **untouched** (`git diff dc95d1a~1..3b8fdc0 --
Cargo.toml` = empty). This is a housekeeping + honest-negative-reporting round,
not a speedup round.

---

## 1. Did we actually speed up the code?

### 1.1 Verdict: NO — Round 18 shipped zero production-affecting changes

**OBSERVED.** `git diff dc95d1a~1..3b8fdc0 -- Cargo.toml` returns empty. The
`production` feature list is unchanged at `Cargo.toml:399`:

```
production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
              "alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]
```

None of the perf-relevant Large features (`medium-classes`, `exact-span-large`,
`large-reserved-capacity`, `large-cache-extended`, `hardened`) are in
`production`, and none were promoted this round. So nothing R18 did can move a
production-build number.

The only `src/` logic change (R18-3, `912740f`) narrows a `kind_at` check whose
two `#[cfg]` branches (A)/(B) **both compile out under plain `production`** —
branch (A) requires the promotion predicate (`medium-classes && ...`),
branch (B) requires `hardened`; `production` has neither. The commit's own iai
evidence confirms `small_churn_16b` is byte-identical (8,051 Ir) pre/post, and
the new `medium_class_dealloc_churn_16b` is Linux-gated and only meaningful
under `production,medium-classes`. So even the code change is a no-op on the
production hot path by construction.

### 1.2 R18-2's honest RED verdict is CORRECT — independently verified

R18-2 (`8833baa`) re-ran the exact `scripts/r10_2_medium_gate.mjs` harness on
`main @ 912740f`, 20 A/B/B/A pairs (80 launches) per phase, and reported
**STILL RED** on the realloc kill-gate. I verified every headline number against
the committed raw logs and CSV rather than trusting the prose:

| claim (R14_4 §7.1 / commit msg) | my source | verdict |
|---|---|---|
| `production,medium-classes` realloc Δ = −66.06 ms, t=−38.137, sign 20/20 A-faster | `_raw_r18_2_combo12_off_vs_on.log:97` (`mean Δ=-66.055 ms ... t=-38.137 ... A-faster=20/20`) | **CONFIRMED** |
| `...,large-cache-extended` realloc Δ = −19.38 ms, t=−41.940, sign 20/20 | `_raw_r18_2_combo3_off_vs_onext.log:89` (`mean Δ=-19.378 ms ... t=-41.940`) | **CONFIRMED** |
| control (off vs off) t=0.364, sign 8/12 → honesty PASS | `R18_2_..._summary.csv:8` (`t=0.364 ... sign 8,12`) | **CONFIRMED** |
| leak gone: commit 1.3 GiB → 49 MiB | CSV `commit_kib_b`: 50518 (combo12) vs prior 1.3 GiB | **CONFIRMED** |
| per-op realloc ~67.6 µs (essentially unchanged from R14-4 ~72 µs) | sample `realloc_ns=58260700`/launch (`_raw_r18_2_combo12:30`) over ~864 reallocs ≈ 67 µs | **CONFIRMED** (order-of-magnitude) |

The report's own honesty about WHY the ratio dropped 1900×→1180× is the key
detail: it is **NOT** because medium got faster — it is because the *baseline*
(`production`) measured slower under this session's heavier host load (66–94%
CPU, disclosed in CSV `host_cpu_load_pct`). The per-op TIME barely moved
(~67.6 µs vs ~72 µs). This is a host-load artifact in the *ratio*, correctly
flagged rather than claimed as an improvement. **I agree with the RED verdict
and with its framing.**

**Nuance the headline omits but the data shows:** medium-classes **HELPS** the
alloc and free phases dramatically — alloc Δ≈+3.68 ms (B-faster 20/20, ~31×),
free Δ≈+14.89 ms (B-faster 20/20, ~200×) — but the realloc regression
(−66 ms) swamps both for the R10-2 workload. The net depends on realloc
intensity (R10-2 §5 break-even ≈205 reallocs/alloc-free-cycle). R18-2 §10 does
report the alloc/free preservation; the round-level framing justifiably
emphasizes the realloc RED because that is what the kill-gate measures.

### 1.3 Opt-in vs production — the distinction this round is careful about

Every perf-relevant feature R18 discusses is **opt-in**, not production:
`medium-classes` (`Cargo.toml:474`), `exact-span-large` (`:312`),
`large-reserved-capacity` (`:357`), `large-cache-extended` (`:371`). R18-2 and
R18-9 both correctly frame their results as opt-in measurements and explicitly
state "no production feature-list change" (R18-2 commit msg). This is the right
discipline and it is upheld.

**Bottom line for Q1:** R18 did not speed up production code. It honestly
confirmed a stubborn structural regression remains (realloc memcpy barrier) and
made no misleading speedup claim. The round's perf value is in *measurement
honesty* and *closing a stale open item*, not in shipped ns/op improvement.

---

## 2. What else can be significantly sped up?

### 2.1 R18-9 (unified adaptive Large policy) — agree with its core conclusion

R18-9's central boundary claim (§6/§8.1/§9): **a unified Large policy coordinates
existing levers but does NOT close the R10-2 realloc kill-gate.** The residual
~19–67 ms is structural promotion `memcpy` (16 objects × 256 KiB × 20 rounds ≈
80 MiB copied per the R18-2 data I verified). The only lever that could flip the
gate is R10-2 §5's in-place-medium-grow mechanism, which is **not designed**.

I independently checked the load-bearing facts:

- **The promotion copy is unavoidable by reserved-capacity headroom.** Promotion
  (`heap_core_free.rs:863` → `try_promote_to_large`) calls `alloc_large(new_size)`
  on a FRESH Large segment and copies the old prefix — the copy happens BEFORE
  the fresh segment's `reserved_capacity` is set. So `large-reserved-capacity`
  (which gives the *Large* segment growth headroom) cannot retroactively help the
  *first* promotion copy. R18-9 §3.3/§8.1 states this; R18-2 §10.7 leans the same
  way. **AGREE** — this is structurally sound, not just asserted.
- **C4 is the highest-information unmeasured cell.** `production,medium-classes,
  exact-span-large,large-reserved-capacity` on the R10-2 harness has never been
  run. R18-9 §3.1 marks C1/C3 as already-measured (R18-2) and C2/C4/C5 as gaps.
  C4's binary-ish outcome (headroom helps → gate may move; null → existing
  features already coordinated) makes it the cheapest high-value experiment.
  **AGREE** — and it needs no new harness (reuse `r10_2_medium_gate.mjs` with a
  different feature set).

**R18-9's plan-premise correction is accurate (verified):** the "five
independently-gating mechanisms" framing is wrong. `primordial-lazy-commit` is
already IN `production` (`Cargo.toml:399`, verified) and is not a Large mechanism
(affects only the one-time primordial segment + small-segment grow-on-carve, per
R12-9 §1–§2). The byte budget is `large-cache-extended`'s runtime dimension
(`resolved_budget_bytes()` at `large_cache_config.rs:373` branches on
`large-cache-extended`), not a sixth toggle. Honest re-count: **3 opt-in Large
features + 1 runtime knob**. A matrix built on the literal "five" would carry two
no-op rows. **AGREE** — verified each claim against Cargo.toml / source.

### 2.2 R18-7 (mimalloc gap status) — agree with its core correction

R18-7's headline finding (§0/§1): the `PERF_PLAN_beat_mimalloc_small_medium.md`
plan is **EXHAUSTED, not dormant** — all of Э1–Э5 landed in Round 7, plus Э6–Э11
across P6/P7.

I verified this independently rather than trusting the doc:

| eureka | cited commit | `git log -1` result | match? |
|---|---|---|---|
| Э1 (P3 bump-direct carve) | `671a81b` | `perf(#147): P3 bump-direct batched carve (Э1)...` | ✅ |
| Э2/Э4/Э5 (P1) | `38e1a44` | `perf(#145): P1 four quick wins — counter, classify-once, one-branch, 256 class` | ✅ |
| Э3 (P2 own-segment cache) | `3b9123e` | `perf(#146): P2 own-segment cache (Э3)...` | ✅ |
| P0 measurement foundation | `4908fce` | `perf(#144): P0 iai cold/churn benches...` | ✅ |
| P5 verdict/tables | `2dede7d` | `docs(#149): P5 — perf verdict...` | ✅ |

All five R7 commits exist with matching task numbers. **The plan's named task
chain #144→#149 is 100% landed.** This directly refutes the three prior reviews:
- `oh-review §231`: "it appears to have gone **dormant** since whenever Э1 landed"
- `crush-review §156`: "Часть уже приземлилась (Э1 ...) но README-цифра
  показывает: фронт **не закрыт**" (implied Э2–Э5 unfinished)
- `r18-plan §93/§107`: "**дормантный** root-caused план" / "дормантная с ~Round 6-9"

R18-7 is the first to actually `git log` the plan's tasks. **This is a genuine,
verified correction of all three prior reviews + the R18 plan.**

**Important fairness caveat:** the prior reviews were wrong about *where the
leverage lies* (not in un-started eurekas), but their *broader* point — the
cold-16B gap is a real open question — is NOT refuted. R18-7 §3b itself
acknowledges the cross-allocator `Ir` comparison was never done (no mimalloc arm
in `perf_gate_iai.rs`, verified: `rg mimalloc benches/perf_gate_iai.rs` → none).
So the gap is open; the plan just isn't the path to closing it.

**R18-7's README-drift finding (§2):** the 2026-07-23 README "2.4–2.7×" headline
is a single host-drifted run (both allocators' absolute cold times ~doubled vs
07-14). This is **plausible and internally consistent** with the cited
ALLOC_BENCH tables, but I did NOT independently re-measure — so I mark it
INFERRED/plausible, not CONFIRMED. The methodological point (a single worst-case
snapshot shouldn't be the headline) is sound regardless.

### 2.3 The genuinely open speed-up levers (synthesized, all cited)

1. **R10-2 §5 #1 — in-place medium-class grow within a segment** (OPEN_ITEMS #1).
   NOT designed, NOT implemented. The real structural blocker for the realloc
   kill-gate. Highest long-term value; needs its own design task.
2. **C4 measurement** (OPEN_ITEMS #2, R18-9 §9). Cheapest binary-ish experiment;
   no new harness. Tests whether reserved-capacity headroom reduces the promotion
   memcpy (predicted: it cannot help the *first* copy, but unmeasured).
3. **Cross-allocator `Ir` comparison** (OPEN_ITEMS #4, R18-7 §3b). Would settle
   the 10-round cold-16B wall-clock argument deterministically. Requires a
   feasibility check (is a C-library `Ir` comparison meaningful under
   `iai-callgrind` via FFI?).

---

## 3. What needs improving in the code?

### 3.1 R18-3 `heap_core_free.rs` branch (A)/(B) split — SOUND (verified)

**The split + runtime size gate is correct.** I verified the three load-bearing
claims:

**(a) Branch (A)'s `#[cfg]` is byte-identical to the promotion call site's
`#[cfg]`.** The "1:1 realignment" claim holds — compare:

- Branch (A) cfg, `heap_core_free.rs:268-278`: `medium-classes && (!exact-span-large || (large-reserved-capacity && !numa-aware))`
- Promotion call-site cfg, `heap_core_free.rs:846-852`: identical expression.

So branch (A) compiles in exactly when promotion can actually fire. Correct.

**(b) The runtime size gate `cfg!(feature = "hardened") || layout.size() >=
MEDIUM_REALLOC_PROMOTION_THRESHOLD` is sound** (`heap_core_free.rs:294-296`,
threshold `= 256 * 1024` at `:75`).

The soundness argument: a *legit* Large-segment block whose dealloc layout
classifies small (so `class_for` returns `Some(c)` and we enter this arm) can
only arise via promotion + OPT-G in-place growth, because:
- An originally-Large allocation has size > `SMALL_MAX` (1 MiB under
  medium-classes) → `class_for` returns `None` → never enters the `Some(c)` arm.
- Promotion requires `new_size >= THRESHOLD` (`:854`) and only fires on a grow.
- OPT-G only grows (a shrink falls through to the move leg; the moved block lands
  in a small/medium segment, no longer Large).

Therefore a legit Large-with-small-layout dealloc always has `size >= THRESHOLD`.
The non-hardened gate (`size >= THRESHOLD`) soundly skips `kind_at` for
sub-threshold frees (16/32/64 B etc.) — those structurally cannot be Large.
**AGREE the placement is necessary and correct.**

**(c) The `hardened` bypass is necessary and correct.** Under `hardened`, a
`GlobalAlloc`-contract violation can fabricate ANY small layout on a Large
pointer (e.g. a 2 MiB Large freed with a 64-byte layout, well below 256 KiB). The
size-based soundness argument only covers *legit* allocations, so under
`hardened` the gate MUST be bypassed. The `cfg!` macro constant-folds: hardened
builds get `true || ...` (unconditional check, zero added branching), non-hardened
get the size gate. The counterfactual test
`tests/regression_hardened_large_kind_own_free.rs` (verified to exist, with a
documented RED-without-the-guard scenario) confirms a naive unconditional size
gate would let the alias through. **AGREE — the bypass is both necessary and
correctly verified.**

**(d) Branches (A) and (B) are mutually exclusive.** (A) requires the promotion
predicate true; (B) (`:325-337`) requires `hardened && NOT(promotion-predicate)`.
They cannot both compile. No double-routing. The broadening of (B) from
`not(medium-classes)` to `NOT(promotion-predicate)` correctly covers the
`medium-classes + exact-span-large-without-reserved-capacity` gap (e.g.
`--all-features` where `numa-aware` defeats `large-reserved-capacity`). For
non-`medium-classes` builds (B)'s cfg is equivalent to pre-R18-3 (promotion
predicate is false when `medium-classes` is absent). **AGREE.**

**One residual sharp edge (acknowledged in the commit, acceptable):** under
`production,medium-classes,exact-span-large` WITHOUT `hardened` and WITHOUT
`large-reserved-capacity`, NEITHER (A) nor (B) compiles — so a contract-violation
free of a Large pointer with a fabricated small layout is undefended. This is
consistent with the crate's stance that contract-violation defence is
`hardened`-opt-in (`heap_core_free.rs:319-321` documents it explicitly). Not a
bug, but worth knowing.

**Minor doc imprecision (low severity):** the iai bench comment
(`perf_gate_iai.rs:103-108`) says "this bench measures exactly the cost of that
gate on the common small-free path." The commit message is more honest: the bench
measures the WHOLE `production,medium-classes` dealloc path, not the gate's
marginal cost (a single `size >= 262144` compare that short-circuits before
`kind_at`). The gate's marginal cost is unisolatable from medium-classes' other
overhead (different `SIZE2CLASS` table, different `SMALL_MAX`). The bench
establishes a trackable baseline (8,275 Ir first measurement) — valuable — but
the comment slightly overstates what it isolates.

### 3.2 R18-1 `race_repro.rs` watchdog — correct, no problems found

The new watchdog logic (`tests/race_repro.rs`) is sound:

- **`exit(124)` vs `abort()`** (`:178`): correct distinction. `exit(124)` is the
  conventional `timeout(1)` code; on Windows/MSVC `abort()` → `__fastfail` →
  exception code `STATUS_STACK_BUFFER_OVERRUN` (0xC0000409), byte-identical to a
  genuine stack-corruption crash. Using `exit(124)` means a future watchdog
  firing is unambiguously identifiable. No destructor-leak concern (intentional
  fail-fast; the whole process is terminating).
- **`progress` closure** (`:251-263`, `:362-378`, `:441-458`): captures `Arc`s
  and reads `Ordering::Relaxed` atomics for an approximate snapshot. No lock
  acquisition, no deadlock risk; `Send + 'static` (only `Arc` captures). Reading
  atomics another thread is writing, with `Relaxed`, is fine for a diagnostic
  snapshot (no correctness requirement on exact values).
- **`RACE_REPRO_DEADLINE_SECS` env override** (`:128-133`): `.filter(|&n| n > 0)`
  guards against 0/negative parses — good defensive parsing; default 20s
  unchanged.
- **`join()` result no longer swallowed** (`:196-210`): recovers panic payload
  via `downcast_ref::<String>()` / `<&'static str>()`. Improvement over the old
  `let _ = h.join()`.
- **Pre-existing 100ms poll latency** (could fire just after `done` is set) is
  unchanged from before R18-1 and acceptable for a watchdog.

**Hypothesis framing is appropriately cautious.** The header cleanly separates
OBSERVED (the watchdog, the 20s deadline, the historical `abort()` call) from
INFERRED (the `abort()` → `__fastfail` → 0xC0000409 mapping, from documented
platform behaviour, NOT confirmed by a run in this repo). The refutation test
(grep preserved stderr for "TEST EXCEEDED") was NOT performed — honestly
disclosed. The checksum-oracle-stayed-green evidence is supporting but not
conclusive, stated as such. **No overclaiming.**

### 3.3 REAL ISSUE FOUND — R18-7 §7 self-correction is INCOMPLETE

This is the one concrete defect I found in a committed Round-18 artifact.

`docs/perf/R18_7_MIMALLOC_GAP_STATUS.md` §4 (the self-correction, landed in
`4ba35dc`) retracts the original claim that "the iai `Ir` gate is not in CI" —
`.github/workflows/perf-gate.yml` already exists and implements exactly that gate
(task #127/#128). I confirmed `perf-gate.yml` exists (via `ls
.github/workflows/`) and implements: nightly `schedule` + `workflow_dispatch` +
labeled-PR trigger, `ubuntu-latest`, `cargo bench --bench perf_gate_iai
--features production`, `IAI_CALLGRIND_REGRESSION: 'Ir=10'`
(`perf-gate.yml:21-41,96-117`).

**BUT §7 "Files inspected (evidence trail)" was NOT corrected.** Lines 300-301
still read:

> `.github/workflows/ci.yml:555–590` (miri jobs), `:978–1133` (the two existing
> weekly/dispatch jobs that an iai job would mirror); `rg iai|perf-gate|valgrind`
> **→ empty (no perf-gate job exists).**

This directly contradicts §4's correction AND the actual `perf-gate.yml` file.
The `4ba35dc` commit message itself names "Corrects sections **0, 4, 5, 6, and
8**" — §7 is not in that list, and the diff hunks (`@@ -22, @@ -171, @@ -211,
@@ -231, @@ -310`) confirm §7 (~285–307) was never touched. A reader who jumps to
§7's evidence trail (the natural place to audit what was inspected) is told "no
perf-gate job exists" — the exact error the correction was meant to retract.

**Severity:** low (a doc-internal inconsistency in an investigation report), but
it is a *live, verifiable* contradiction in a document whose entire purpose is
evidence-cited accuracy, and it is exactly the kind of leftover a grep-based
final pass (`rg "no perf-gate" docs/perf/R18_7*.md`) would have caught. The
`no_stale_doc_references` guard does not catch it (it checks external references,
not internal contradictions).

**Minimal fix:** delete or correct the "`rg ... → empty (no perf-gate job
exists)`" clause on lines 300-301 to reflect that `perf-gate.yml` exists and was
the subject of §4's correction.

### 3.4 Test coverage — adequate for R18's scope

- `tests/regression_hardened_large_kind_own_free.rs` — verified to exist, with a
  documented RED counterfactual (comment out the `#[cfg(feature="hardened")]`
  Large-kind block → alias through magazine). This is the test R18-3's `cfg!`
  bypass depends on. Non-vacuous.
- `tests/r17_4_inplace_grown_large_dealloc_routes_by_kind.rs` — verified to exist.
  Backs the legit-promotion-routing path.
- `benches/perf_gate_iai.rs::medium_class_dealloc_churn_16b` — Linux-gated
  (`#[cfg(target_os = "linux")]`), correct for iai-callgrind. First baseline for
  the `production,medium-classes` dealloc path. Not a correctness test (instruction
  count), but it's the right kind of guard for a hot-path-cost claim.

No coverage gap identified for the changes actually made. (A test exercising the
non-hardened size-gate *skip* path under `medium-classes` would be nice but is
not a correctness need — a wrong gate that always-checked would be slower, not
unsound, and the iai bench covers the perf aspect.)

---

## 4. What needs improving in the project (process / docs / CI / methodology)?

### 4.1 `OPEN_ITEMS.md` (R18-8) — well-implemented and genuinely valuable, NOT a formality

The convention is sound and the content is real:

- **The mechanism is the right one.** The index exists because R14-4's
  explicitly-flagged-open item ("re-run `r10_2_medium_gate.mjs` once R14-5 lands")
  hung unnoticed through rounds 15–17 — caught only by an accidental external
  re-read (closed as R18-2). The in-session TaskList does not survive a session
  boundary; a durable file does. The CLAUDE.md bullet (`:57-68`) makes the
  round-start read mandatory. This addresses a real, observed process failure.
- **The 14 items are real, not padding.** I verified the top-tier ones:
  - #1 (in-place medium grow) — the genuine blocker, reaffirmed by R18-9 §9.
  - #2 (C4 measurement) — the named next step, never measured.
  - #3 (stale "pending the Linux Ir gate" wording) — I did not independently
    verify the exact CHANGELOG/ALLOC_BENCH line numbers cited, but the claim is
    consistent with `perf-gate.yml` existing (this review) and low-stakes.
  - #4 (mimalloc Ir arm) — the genuinely open cross-allocator question.
- **The closure trail works.** §"Recently resolved" records R18-2 closing the
  R14-4 item, with commit + doc-pointer + one-line evidence. This is exactly the
  artifact that lets a future reviewer confirm an item was addressed, not
  forgotten. The "do NOT delete the entry" rule (`:21-24`) is the right call.
- **Evidence pointers are specific** (`file:line` + section), verifiable.

**Minor improvement suggestion:** the convention has no mechanical enforcement —
it relies on the agent remembering to (a) read at round start, (b) append on
flag, (c) move on close. Given the failure mode it exists to prevent is "an agent
forgot," a lightweight hook (e.g. a pre-commit grep that diffs
`docs/perf/*.md` "Open items"/"Follow-up" sections against OPEN_ITEMS.md entries)
would add a tripwire. But this is a nice-to-have, not a blocker; the current
checklist is a strict improvement over the prior state (nothing).

### 4.2 Process gap — no Round 18 CHANGELOG entry

Rounds 15/16/17 each got a consolidated CHANGELOG entry (`52bbb8a` / `daf36de` /
`a99314b`). Round 18 has **none** — the CHANGELOG's last round-section is Round
17. R18-4/R18-5 (`1d2c9cd`) edited *existing* entries but did not add an R18
section. `rg "R18-|Round 18|task #33[0-6]" CHANGELOG.md` → no matches.

**Severity:** low. R18 shipped no production-affecting change (no feature
promotion, no `production` list edit, no new `unsafe`), so a downstream consumer
is unaffected. But it is an inconsistency with the established per-round cadence,
and R18-3 (a real correctness narrowing of a dealloc-path defence) and R18-1 (a
test-hardening that changes crash-surface behaviour) are the kind of changes a
future reader scanning the CHANGELOG for "what happened when" would expect to
find. A 5–10 line R18 entry would close the gap.

### 4.3 The §7 residual (§3.3 above) is also a process finding

The zero-trust review caught the §4 error *before* the task was accepted (good —
that is the process working), but the *correction* commit (`4ba35dc`) did not
re-scan the whole document for the retracted claim. The lesson, already half-
known in this repo (R18-6's whole subject is stale literals surviving partial
edits): **a correction commit should grep the whole doc for the retracted claim,
not just the sections it explicitly names.** The `4ba35dc` message even lists
which sections it touched ("0, 4, 5, 6, and 8") — §7's absence from that list is
the tell.

### 4.4 R18-6 stale-literal-guard design — reasonable, correctly deferred

The design note (`docs/DESIGN_stale_literal_guard.md`) evaluates 4 variants
against this repo's own evidence and recommends: **convention (cite the const
name, not the resolved literal) as the standing rule + selective v4 tripwires**
(mirrored test-side const + `assert_eq`, already precedented at `dbg_max_segments`
/ `dbg_promotion_compiled`). It explicitly rejects building a new general lint
(variant 2 is unsound on historical prose; variant 1 is the same manual
discipline that failed 4×). This is the right call given the evidence. The flagged
live at-risk site (`dirty_by_class.rs:37-39` restates `49/3136/25088/...` derived
from `SMALL_CLASS_COUNT × WORDS_PER_CLASS`) is real doc-debt for a future round.
No action needed this round; the design is sound.

### 4.5 Overall methodology assessment for R18

R18 is a **response/triage round**, not a build round: 7 of 9 commits address
findings the three prior R13–R17 reviews surfaced (R18-3 narrows the `kind_at`
check all three flagged; R18-1 fixes the watchdog abort-surface all three
flagged; R18-2 closes the stale R14-4 re-run item; R18-7 corrects the
mimalloc-plan mischaracterization). The remaining 2 (R18-9 design, R18-8
OPEN_ITEMS) are forward-looking infrastructure. The round's *value* is in
closing open items, correcting prior-review errors, and establishing durable
cross-round tracking — honestly, not in shipping speedups. That is an appropriate
use of a round given the prior reviews' findings were triage-shaped.

The one place the discipline slipped is §4.3/§3.3 (the incomplete §7
correction) — small, but it is the same "partial edit leaves a stale claim"
defect class R18-6 itself catalogs.

---

## 5. Specifically-requested checks — direct answers

### 5.1 `heap_core_free.rs` (R18-3) — branch (A)/(B) + runtime size gate

**Is the size-gate placement (`cfg!(hardened) || layout.size() >= THRESHOLD`)
necessary and correct?** **YES.** See §3.1. The gate is sound for legit
allocations (promotion structurally requires `size >= THRESHOLD`), and the
`hardened` bypass is necessary because contract violations can fabricate any
small layout (the size argument only covers legit allocations). The `cfg!`
constant-folds, so hardened builds pay zero added branching. Verified by the
existing `regression_hardened_large_kind_own_free` test (2 MiB Large freed with
64-byte layout, below the 256 KiB threshold).

### 5.2 `race_repro.rs` (R18-1) — watchdog logic

**Are there problems?** **No problems found.** See §3.2. `exit(124)` is distinct
from `abort()`/`__fastfail`/0xC0000409; the env override is defensively parsed;
the progress closure is deadlock-free (Relaxed atomic reads only); the join
result is no longer swallowed. The hypothesis (OBSERVED vs INFERRED) is framed
honestly, and the not-performed refutation test is disclosed.

### 5.3 R18-7 §4 self-correction — is it complete and accurate?

**The §4 correction itself is accurate** (perf-gate.yml exists and does what it
claims — verified). **But the correction is INCOMPLETE:** §7 (lines 300-301)
still carries the original error's "`rg ... → empty (no perf-gate job exists)`"
claim, contradicting §4 and the real file. The `4ba35dc` commit touched sections
0/4/5/6/8 but not §7. This is a live, verifiable residual. See §3.3 for the
minimal fix.

---

## 6. Cross-check against the three prior reviews + R18 plan

I formed the above conclusions before reading the prior reviews. Reconciliation:

**Agreement:**
- All three prior reviews flagged R17-4's unconditional `kind_at` check
  (`r17-readonly §P2`, `crush §2.4/§3.7`, `oh §1a`). **R18-3 closes exactly this
  finding** — and does it correctly (§3.1). The review→fix loop worked.
- All three flagged the watchdog `abort()` crash-surface confusion
  (`r17-readonly`, `crush`, `oh`). **R18-1 addresses it** (§3.2).
- All three prioritized re-running the R10-2 gate after R17-4's leak fix
  (`crush §2.3`, `oh §457`, `r17-readonly §1`). **R18-2 did exactly this** and
  reported an honest RED (§1.2).

**Where R18-7 corrects the prior reviews (verified by me, §2.2):**
- All three + the R18 plan characterized the mimalloc plan as **"dormant"**
  (oh §231, crush §156/§442, r18-plan §93/§107). R18-7 is the first to `git log`
  the plan's tasks and show it is **EXHAUSTED** (Э1–Э11 all landed in R7). I
  independently verified the 5 cited R7 commits. **The prior reviews were wrong
  that there was unfinished work in this plan** — though right that the cold-16B
  gap remains an open question (R18-7 §3b acknowledges the Ir comparison was
  never done).

**New findings this review adds (not in the prior three):**
- The R18-7 §7 residual error (§3.3) — a concrete, verifiable doc inconsistency
  the self-correction missed.
- The absent R18 CHANGELOG entry (§4.2).
- The iai-bench comment imprecision (§3.1, minor).

---

## 7. Summary card

| Q | verdict | confidence |
|---|---|---|
| 1. Did we speed up code? | **No.** Zero production-affecting changes (Cargo.toml untouched); R18-2's honest RED is correct (verified against raw logs). | CONFIRMED (observed) |
| 2. What else to speed up? | R10-2 §5 in-place-medium-grow (the real blocker, undesigned); C4 measurement (cheapest, binary); cross-allocator Ir arm. R18-9/R18-7 conclusions agreed with independently. | CONFIRMED (structural) |
| 3. Code improvements? | R18-3 sound (verified); R18-1 sound (no issues); **one real defect: R18-7 §7 residual "(no perf-gate job exists)"** contradicts §4 + actual file. | CONFIRMED (verifiable) |
| 4. Project improvements? | OPEN_ITEMS.md is genuinely valuable (not a formality); missing R18 CHANGELOG entry (low severity); §7 residual is a "partial-edit leaves stale claim" instance of the R18-6 defect class. | CONFIRMED |

**No evidence found of:** unsound `unsafe`, broken `#[cfg]` mutual exclusivity,
a vacuous/missing test, an architectural-rule violation (one-export-per-file,
mod.rs-reexports-only, no-inline-tests all upheld across the 2 src/test diffs),
or an overclaimed speedup. The round is honest and technically careful; its one
slip is a documentation-internal leftover, not a code defect.
