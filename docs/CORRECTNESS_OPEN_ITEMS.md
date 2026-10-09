# Correctness / CI-debt open items — cross-round tracking index

**Purpose.** A single durable, session-surviving checklist of correctness,
flakiness, and CI-coverage-gap items that a commit message, code comment, or
review doc has flagged as *open / follow-up / "left for later"* — the sibling
to `docs/perf/OPEN_ITEMS.md`, which durably tracks the analogous class of item
but ONLY for `docs/perf/*.md` gate reports and perf design docs (see that
file's own `## Scope`). This file exists because R19-1 (task #337, commit
`46ea2db`)'s own commit message flagged TWO follow-ups — a flaky test and a
clippy dead-code combo — that then existed NOWHERE durable: not in
`OPEN_ITEMS.md` (out of its scope by design — it is not a perf gate report),
not in `CHANGELOG.md`, not anywhere else. Two independent reviews
(`docs/reviews/2026-07-26-crush-review-r19-r21.md` §4 P2 and
`docs/reviews/2026-07-26-oh-review-r19-r21.md` §4.1) both independently
rediscovered this gap, and the flaky item was then independently reproduced
TWICE MORE in Round 22 itself (once during task #352's CI verification, once
during task #356's test run) before this file existed to catch it. This file
is the fix: option (b) from both reviews (a sibling index), not a widening of
`OPEN_ITEMS.md`'s own scope — that file's perf-only narrowness is a deliberate,
working design choice for its own domain and stays intact.

**Scope.** This index covers correctness bugs, flaky tests, and CI-coverage
gaps that originate from ANY source — commit message follow-up notes, code
comments (`TODO`/`FIXME`), or review-doc findings — not just
`docs/perf/*.md` reports. It is the correctness/CI-debt counterpart to
`docs/perf/OPEN_ITEMS.md`, which stays scoped to perf gate reports and perf
design docs only; see that file's own `## Scope` for the boundary and its
cross-link back to this file. When in doubt which index an item belongs in:
if it is about wall-clock/Ir/memory numbers or a perf design's
CONDITIONAL-GO trigger, it belongs in `docs/perf/OPEN_ITEMS.md`; if it is
about a test that can fail spuriously, a lint/build combo that is not
clean, or a correctness contract, it belongs here.

**Convention (mandatory — see CLAUDE.md "Phased delivery").**

1. **Round start:** before forming a new round's task queue, read this
   index's tier files end-to-end (`docs/correctness-open-items/ACTIVE.md`
   then all nine thematic `docs/correctness-open-items/TRACKED_*.md`
   files, in any order — alongside `docs/perf/OPEN_ITEMS.md`) and decide, for
   each open item, whether this round closes it, defers it (with a
   one-line reason appended), or leaves it. An item must not be silently
   ignored — every round either moves it or explicitly re-defers it.
2. **When you close an item:** move its entry to
   `docs/correctness-open-items/RESOLVED.md`'s "Recently resolved" section
   with the closing round + task number + one-line evidence (commit / doc
   that records the resolution). Do NOT delete the entry — the closure
   trail is itself the artifact that lets a future reviewer confirm an item
   was actually addressed, not just forgotten again.
3. **When a new commit, comment, or review flags a correctness/CI-debt
   follow-up:** add it to `docs/correctness-open-items/ACTIVE.md` ([A]
   tier) or, for the `[T]` tier, the thematic `TRACKED_*.md` file whose
   subject the card matches (the nine filenames state their subjects;
   their criteria are in the "Structure" section below), in the same
   commit (or an immediate follow-up commit), with a citation back to
   its origin (commit SHA / file:line). A flag that lives only inside a
   single commit message body
   or code comment is exactly the failure mode this index exists to
   prevent.

**Tier key.** **[A]** active — a real next step a round should consider
taking. **[T]** tracked-not-actioned — genuinely reproduced/confirmed but
intentionally not yet scheduled for a fix (root-cause investigation or a
scoping decision is the pending step, not implementation).

**Round 13 src-review and remediation (2026-10-06).** R13-01…05 исправлены и
проверены; карточки 168/169 перенесены в `RESOLVED.md`. Остальные карточки
сохраняют текущий status/trigger: выбранные native-прогоны не закрывают
platform/model/publication acceptance. P1-box (164), interior-free residual
(166) и verification-only mirror (167) остаются как были. Evidence:
`docs/reviews/2026-10-06-src-review-sol-round-13.md`, раздел «Принятые исправления после ревью».

**Src review round 14 (2026-10-06).** R14-01…05 filed as item170 before
remediation and are now CLOSED after source, type-boundary, and runtime
verification; the closure pointer is in `RESOLVED.md`, with full narrative
in `ARCHIVE.md`. A separate Miri failure on the existing experimental
`EpochRegion`/`crossbeam-epoch 0.9.20` path is tracked as item171; it is not
attributed to the R14 changes and is not hidden or called resolved. All
other prior-card statuses/triggers remain unchanged. Evidence:
`docs/reviews/2026-10-06-src-review-sol-round-14.md`, closure receipt.

**Src review round 15 (2026-10-07).** R15-01 and R15-02 are filed as active
items 172/173 after parent-run temporary witnesses confirmed the unwind-tail
drop leak and primordial rollback counter mismatch. R15-03 adds current prose
examples to item154. Item157 is CLOSED/SUPERSEDED: its legacy intrusive-spill
telemetry subject no longer exists in the current root allocator; its closure
record is in `RESOLVED.md`. Item158 remains open: all-bucket `publish_empty`
and current-node-only post-drain sync clearing are distinguished; no NUMA
runtime witness was run. All other prior-card statuses and triggers remain
unchanged; item158's next trigger was narrowed to the sync-only clear path.
Evidence: `docs/reviews/2026-10-07-src-review-xs-round-15.md`.

**Src review round 16 (2026-10-08).** R16-01 is filed as active item 174
(two release `expect`s on `GlobalAlloc` free/trim paths outside the no-panic
contract's enumeration); R16-02 as item 175 (the R15 x86_64 gate dropped the
R14-01 checks from native macOS arm64); R16-03 as item 176 (u64 class mask
without a static bound). Item 22 is CLOSED/SUPERSEDED: its `RemoteFreeRing`
subject no longer exists; its closure record is in `RESOLVED.md`. Items
172/173 stay open; R16 reproduced both with its own executed witness. Item 154
gains evidence only. All other statuses and triggers remain unchanged.
Evidence: `docs/reviews/2026-10-08-src-review-oxx-round-16.md`. The
independent verification of round 15 is appended to
`docs/reviews/2026-10-07-src-review-xs-round-15.md`; it corrects R15's CI
narrative (E0461 was already red in run `37644289617`) in item 13 and in
the R15 manifest, and adds re-confirmation notes to items 172/173.

**Round 16 follow-up close-out (2026-10-08).** Items 172, 173, 174, 175 and 176
are CLOSED (commits `486f5ace`, `32cacc97`, `9df9f6b8` + `b707d196`,
`6f568c25` + `eda25f97` + `b5247602`, `222e913d`); their narratives are in
`RESOLVED.md`. Item 175 closed only after two CI iterations: the first gate
turned the `cross test` aarch64 job red and was replaced; green CI (macOS arm64
and cross aarch64 both running the harness) confirmed the final one. Item 154 keeps its OPEN status with a
follow-up note: `c8b9344a` fixed the listed R15/R16 examples and removed the
`segment_route` `allow(dead_code)`, the structural prose debt remains. Perf
item 84 shipped (GO) and perf item 83 is a pre-registered NO-GO; see
`docs/perf/OPEN_ITEMS.md`. Evidence: `docs/perf/round-manifests/SRC_REVIEW_R16_MANIFEST.md`.

**R18 coverage reconciliation (2026-10-09).** Items 185–199 are filed and
mapped to their actual thematic cards; counts include retained closed records.
Item 199 owns only API-05's public feature-sensitive SegmentLayout geometry
contract. Existing statuses/triggers are unchanged; no new closure or runtime
verification is claimed. Evidence: `docs/reviews/2026-10-08-src-review-xxs-round-17-P4.md`, R18 appendix.

---

## Structure — this file is a thin index (split 2026-08-20, task #1217;
[T] tier RE-split by THEME, not item-number range, task #1222, same day)

**This file no longer holds card bodies.** It was split into a folder,
`docs/correctness-open-items/`, because its own single-file size had grown
past CLAUDE.md's R34-24 ~1,000-line threshold a second time (2,423 lines at
the task #1143 deferral that first declined this split — see item 86 in
`docs/correctness-open-items/TRACKED_process_record.md` for that decision
and its reversal). This split reverses that deferral, at the owner's
explicit request, following the R29-6/R34-24 mechanism this repo already
uses for `docs/perf/OPEN_ITEMS.md` — except one level deeper: instead of a
single main-file + single-archive pair, the OPEN portion itself is split by
tier, because tier is the axis this file already uses as its primary
organizing structure (see the "Tier key" above) and needed no new taxonomy
invented to cut along.

**Why the main file survives as an index, rather than being deleted
outright:** a large and drifting set of code/CI/script files (`src/`,
`tests/`, `crates/`, `scripts/`, `.github/workflows/ci.yml`, and
`CLAUDE.md` itself) cite this exact path, `docs/CORRECTNESS_OPEN_ITEMS.md`
-- never a line number, never a filename. The count is deliberately NOT
typed here: the task-#1217 split commit typed "42", and that figure was
already stale on arrival (running the census against the split commit
itself yields 44). Compare against this command's output, never a
hardcoded count:

```text
git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l
```

Every citation that points at ONE SPECIFIC ITEM carries that item's
number, in the form `` `docs/CORRECTNESS_OPEN_ITEMS.md` item N ``. That
was FALSE as first written here: at the task-#1217 split nine
item-pointing citations carried no number (seven in `aligned-vmem`, plus
the two elsewhere named below), and the split had silently
upgraded that from a cosmetic gap into a navigation dead end — before
the split, following the path landed on a file that CONTAINED the card
(findable by Ctrl-F on any surrounding phrase); after it, the same
citation landed on this index, whose only per-item navigation aid is the
very number the citation lacked. Task #1227 repaired all seven: item 6
x2 (`commit_range.rs`/`recommit.rs` rustdoc), item 48 x2 (`README.md` +
`decommit.rs` rustdoc), item 62 x3 (`README.md`, the `os/unix.rs` MIPS
comment, and the MIPS `compile_error!` diagnostic), five of the seven on
publish-facing surfaces. Citations that point at the FILE as a whole, at
a named SECTION, or at a CLASS of items rather than one item (CLAUDE.md's
round-start rule, the "Recently resolved" pointers, the ci.yml
guard-class comment) carry no item number and never needed one. Two
card-pointing citations outside `aligned-vmem` still carried no number
as of task #1227 — outside that task's file scope, recorded here for the
next round: `crates/sefer-region/benches/region_bench.rs` ("for the
tracking entry") and `scripts/verify-ci-sentinels.mjs` ("'s new card for
the durable record"). As long as this filename resolves and this section
stays a complete, accurate item-N -> file lookup, every numbered
citation (several of them in *published* `aligned-vmem` rustdoc that
ships to docs.rs) keeps resolving without editing a single one of them.
Deleting this file outright would have forced touching every citing
site, including the publish-facing citations task #889 already had to
repair once, for zero reader benefit.

**Task #1222 (2026-08-20): the `[T]` tier's four number-range files
(task #1221, same day) are REPLACED by nine THEMATIC files.** The owner
rejected balancing-by-line-count and asked for a category split instead --
grouping the then-70 `[T]`-tier cards by what they are actually ABOUT, derived
by reading every card rather than assumed from category names supplied in
the task brief (three of the five candidate axes suggested at task time --
platform/OS contracts, CI coverage, test hygiene — turned out real and are
below; "documentation/indexes/publish-facing surface" and "audits and their
verdicts" turned out to be TWO different real axes each, not one, once the
cards were actually read — see the category table below for the split each
one became).

**The cost this split has to justify, and how it is paid:** a thematic
filename is NOT a one-hop lookup by item number the way `TRACKED_044_093.md`
was — a reader who knows only "item 61" cannot derive its filename from the
number alone. The table below is the complete
item-N → file map for every `[T]`-tier number (including `59a`/`59b`) and
**`ACTIVE.md`** holds current `[A]` cards; `RESOLVED.md` keeps item 170's
closure pointer. The `[T]` table includes item 171's Miri investigation
card. A reader or script citing a `[T]` item by number looks it up here (or greps
`docs/correctness-open-items/*.md` directly).

**The nine `[T]`-tier files, their criteria and numbered-card totals (including retained closed records):**

- **`docs/correctness-open-items/TRACKED_hook_safety.md`** (4 cards) --
  live `dbg_*` hook safety and scanner coverage, plus historical records of
  the former Rust tripwire. Criterion: the safety/soundness of a
  `dbg_*`/`bench-internals` measurement hook that touches live allocator
  state, its `unsafe`/feature-gating correctness, or scanner coverage of
  that hazard class — the R25-1 lineage
  (R29-7/8/17, R30-1/2, R31-4/14b, R31-15). The old
  `tests/dbg_hook_safety_tripwire.rs` was removed at the terminal
  sidecar/owner-maintenance cutover (commit `a4245965`), but the reviewed
  allowlists and scanner live in `scripts/verify-dbg-hook-safety.mjs`, wired
  into `scripts/check-all.mjs` and `.github/workflows/ci.yml`. Old Rust
  scanner line references and run receipts are historical, not current
  execution evidence. Scanner presence does not close the hook-safety
  residual class or prove individual hook invariants. Evidence this is a real, cohesive axis: items 5/7/8/9 are four
  consecutive rounds (R29 through R31) of the SAME bug class recurring and
  being re-fixed — `dbg_decomp_full_cycle`'s dangling cursor (item 5),
  `dbg_decomp_reserve_and_keep`/`_release`'s mint-then-redeem hazard
  (item 7), `has_bench_internals_cfg`'s `cfg_attr` gap and
  `dbg_large_cache_hits`'s gating (item 8), and `ReservedSmallSegment`'s
  scoping/`needs_drop`/scanner-name-prefix follow-ups (item 9) — not four
  unrelated findings that happen to mention `dbg_`.
- **`docs/correctness-open-items/TRACKED_verification_coverage.md`**
  (12 numbered records: 9 open + 3 retained closed) — miri / loom / kani
  proof coverage. Criterion: whether an `unsafe` seam or algorithmic invariant
  has (or lacks) interpreter/model-checker PROOF coverage — distinct from
  ordinary CI gate wiring (a test exists but does not run under some job) and
  from platform empirical verification (real hardware, not a formal tool).
  Evidence: items 17/18 name specific seams/proofs miri/kani never reached;
  41/61 are closed miri-job wiring gaps; 84 records two ignored source-text
  guards; 167 records a verification-only protocol/runtime mismatch; 171
  records an observed Miri failure in the existing experimental epoch path.
- **`docs/correctness-open-items/TRACKED_platform_contracts.md`**
  (17 cards) — per-OS/arch runtime contracts (aligned-vmem, numa-shim).
  Criterion: whether code behaves correctly on a specific OS/architecture
  (HugeTLB, Darwin `madvise`, Windows large pages, BSD/Android/tvOS/
  watchOS/MIPS, page-size constants, numa-shim syscalls), or whether that
  OS-specific behavior has been empirically verified on real hardware
  versus only reasoned-from-spec. Evidence: this cross-platform cluster
  includes items 43/47/60, explicitly framed
  by their own filing task as "REASONED-FROM-SPEC, never empirically
  executed" for a named OS family; 48/52/53/58/59/59a/59b are one
  continuous HugeTLB/Darwin-decommit investigation across many rounds; 6/26
  are single confirmed platform-divergence bugs (Windows decommit crash;
  numa-shim macOS+miri fix unconfirmed on real macOS) of the identical
  shape.
- **`docs/correctness-open-items/TRACKED_ci_gate_coverage.md`** (28
  cards; item 175 was added in R16 and closed the same day, now in `RESOLVED.md`) — local/CI gate wiring & sentinel/guard-script coverage.
  Criterion: whether an existing test, oracle, or guard script actually
  RUNS under some gate (`npm run check` and/or a CI job) — wiring, dead
  scripts, missing feature/profile rows, sentinel-guard scope — as opposed
  to whether the underlying OS behavior is platform-verified (category 3)
  or proof-verified (category 2). This recurring category captures the
  R22-3 "flagged in a commit body, reached no index" failure mode:
  items 80/82/87 are explicitly filed
  as records of exactly that recurrence for three different scripts;
  50/51/54/55/64/65/70/72/73/74/76/88/92 are each "a real test exists, but
  no gate runs it, or runs it under the wrong profile/feature set"; 19/25
  are the same shape for MSRV and a compile-fail harness specifically.
- **`docs/correctness-open-items/TRACKED_test_flakiness.md`** (3
  open cards + 8 retained resolved records) — flaky /
  order-dependent / scheduler-sensitive tests. Criterion:
  a test that fails intermittently because of timing, thread ordering, or
  shared process-wide state — an ACTUALLY-OBSERVED nondeterministic
  failure, not a coverage gap (no test exists) or a platform gap (no
  runner exists). Open items 146–147 document allocator-test observations and
  item 153 local build processes killed by a stale-PID
  `taskkill` from host tooling;
  resolved entries remain as pointers, including item 150's captured
  Windows linker-path failure and verified short-path fix. This separates
  test execution nondeterminism from "nothing runs this test" (category 4).
- **`docs/correctness-open-items/TRACKED_correctness_residuals.md`**
  (4 open cards + 3 retained closed/resolved records; item 22 moved to
  `RESOLVED.md` in R16) — documented-but-unproven panic-/unwind-safety residuals in
  shipping code. Criterion: a known, honestly-recorded gap in a
  panic-safety or unwind-safety guarantee of shipping (non-hook,
  non-platform-specific) code — a residual the code's OWN doc comments
  already name, not yet a proven live bug. Evidence: 23 originated in a
  doc-comment limitation of the fallback init guard; its former sibling
  22 (the retired ring guard) was closed as superseded in R16.
  16 is its resolved release-notes counterpart; 66 records the resolved
  Reservation committed-length contract. Current open statuses remain explicit
  in the tier file rather than inferred from these historical examples.
- **`docs/correctness-open-items/TRACKED_publish_readiness.md`** (55
  numbered cards) — crates.io publish-readiness: metadata, naming, dependencies,
  NO-GO audits. Criterion: a decision or blocker that gates a crate's
  crates.io publication — naming/description/license/dependency
  one-way-door decisions, semver-coupling decisions, or a NO-GO verdict
  (and its blocking findings) from an independent pre-publication audit.
  Evidence: 90/91/93 are three of the four independent `aligned-vmem`
  publication-readiness audits, each an explicit NO-GO verdict with M1-M4
  blockers; 24/28/29/85 are one-way-door metadata/dependency decisions
  (README crate-count claim, `racy-ptr-cell` naming, `deny(missing_docs)`,
  `captrack` supply-chain pin) that all explicitly "become permanent the
  moment the crate first publishes"; 27 and 46 are a compile-error UX
  tradeoff and a semver-coupling acceptance, both pre-publish decisions
  for a crate about to ship; 100 is the first independent audit of this
  campaign to target `numa-shim` rather than `aligned-vmem`, another
  explicit NO-GO verdict.
- **`docs/correctness-open-items/TRACKED_process_record.md`** (11
  cards) — commit-message / count / citation record corrections.
  Criterion: the card's entire content is a RECORD correction — a wrong
  count, a wrong citation, a commit-prefix taxonomy mis-slot, or a "filed
  as a follow-up" claim that was never actually filed — needing no code,
  test, or script change, only an accurate durable record. Evidence: 10 of
  the 11 cards (all but 86) are explicitly tagged "record correction" or
  "closed on filing" in their own card header, the strongest possible
  signal this is a real, self-declared category rather than an imposed
  one; 78/79/81/83 are R30-12 taxonomy mis-slots and commit-body count
  errors; 67/68/89 are citation/claim corrections; 20/21 are CHANGELOG/
  taxonomy record gaps; 86 is this very index's own split-deferral
  decision and its reversal — a record about the index, not about code.
- **`docs/correctness-open-items/TRACKED_misc.md`** (7 open cards + 2
  retained closed/resolved pointers; item157 moved to `RESOLVED.md` in R15;
  item176 was added in R16 and moved to `RESOLVED.md` at the R16 close-out) --
  residual, does not fit any category above. Per this task's brief: a
  card that does not fit is collected here, NOT forced into the
  closest-sounding bucket. Item 45 (numa-shim `RefCell`-vs-`Cell`
  defensive-coding/panic-safety nit) is not an OS-CONTRACT question (it
  never claims the OS behaves differently than documented), not a
  `dbg_*` hook, and not flakiness. Item 49 (aligned-vmem edition-2021-
  vs-2024 explicit-`unsafe{}`-block hygiene, ten FFI call sites) is about
  unsafe-ANNOTATION style at ordinary FFI call sites, not about a
  `bench-internals` measurement hook (category 1's actual criterion) or a
  CI-wiring gap (both hooks in item 49 already compile and run today; the
  gap is only that edition 2024 would make the implicit form a hard error).
  The remaining cards have their individually stated subjects and triggers.
- **`docs/correctness-open-items/ACTIVE.md`** — the **[A]** tier: active
  cards, a real next step a round should consider taking. 7 numbered cards
  currently (re-derived at the R16 close-out with the census command below:
  items 1, 2, 11, 13, 62, 162, 163; items 172, 173, 174 moved to
  `RESOLVED.md` as CLOSED; R15 recorded "8", but its own count was 9, and R16
  recorded 10 before those closures).
- **`docs/correctness-open-items/RESOLVED.md`** — the "Recently resolved
  (closure trail)" section: current review closures may carry their full
  narratives inline; older moved entries have one-line pointers to
  `ARCHIVE.md`.
- **`docs/correctness-open-items/ARCHIVE.md`** — the full dated historical
  closure narratives. Consulted on demand, not part of the mandatory
  round-start read. Unchanged by this task.

**What to read at round start:** `ACTIVE.md` then all nine `TRACKED_*.md`
files (any order — unlike the retired number-range split, there is no
natural reading sequence across themes) — together they are the full
OPEN-item content this file's own "Round start" convention rule (above)
already requires reading end-to-end; `RESOLVED.md` and `ARCHIVE.md` are
consulted on demand, exactly as before.

**Item-number -> file lookup table.** It covers every `[T]`-tier number,
including `59a`/`59b`, plus retained closure pointers. Item157 moved to
`RESOLVED.md` in R15 and item22 in R16; items168–170 remain R13/R14 closure
pointers, item171 is the `[T]` verification card, items172–176 are R16 closures,
and items177–184 are R17 closures in `RESOLVED.md`. New items185–199
are thematic `[T]` entries in the destinations below. Other `[A]` citations
resolve directly in `ACTIVE.md`. The total is deliberately not typed here.
Compare the two `[T]` numbered-record counts; the second excludes
`RESOLVED.md` and counts only `TRACKED_*.md` destinations:

```text
grep -hE '^[0-9]+[a-z]?\. \*\*' docs/correctness-open-items/TRACKED_*.md | wc -l
grep -cE '^\| *[0-9]+[a-z]? *\| `TRACKED_[^`]+\.md` \|' docs/CORRECTNESS_OPEN_ITEMS.md
```

| Item | File |
| --- | --- |
| 5 | `TRACKED_hook_safety.md` |
| 6 | `TRACKED_platform_contracts.md` |
| 7 | `TRACKED_hook_safety.md` |
| 8 | `TRACKED_hook_safety.md` |
| 9 | `TRACKED_hook_safety.md` |
| 10 | `TRACKED_process_record.md` |
| 12 | `TRACKED_test_flakiness.md` |
| 14 | `TRACKED_test_flakiness.md` |
| 16 | `TRACKED_correctness_residuals.md` |
| 17 | `TRACKED_verification_coverage.md` |
| 18 | `TRACKED_verification_coverage.md` |
| 19 | `TRACKED_ci_gate_coverage.md` |
| 20 | `TRACKED_process_record.md` |
| 21 | `TRACKED_process_record.md` |
| 22 | `RESOLVED.md` |
| 23 | `TRACKED_correctness_residuals.md` |
| 24 | `TRACKED_publish_readiness.md` |
| 25 | `TRACKED_ci_gate_coverage.md` |
| 26 | `TRACKED_platform_contracts.md` |
| 27 | `TRACKED_publish_readiness.md` |
| 28 | `TRACKED_publish_readiness.md` |
| 29 | `TRACKED_publish_readiness.md` |
| 41 | `TRACKED_verification_coverage.md` |
| 43 | `TRACKED_platform_contracts.md` |
| 44 | `TRACKED_platform_contracts.md` |
| 45 | `TRACKED_misc.md` |
| 46 | `TRACKED_publish_readiness.md` |
| 47 | `TRACKED_platform_contracts.md` |
| 48 | `TRACKED_platform_contracts.md` |
| 49 | `TRACKED_misc.md` |
| 50 | `TRACKED_ci_gate_coverage.md` |
| 51 | `TRACKED_ci_gate_coverage.md` |
| 52 | `TRACKED_platform_contracts.md` |
| 53 | `TRACKED_platform_contracts.md` |
| 54 | `TRACKED_ci_gate_coverage.md` |
| 55 | `TRACKED_ci_gate_coverage.md` |
| 58 | `TRACKED_platform_contracts.md` |
| 59 | `TRACKED_platform_contracts.md` |
| 59a | `TRACKED_platform_contracts.md` |
| 59b | `TRACKED_platform_contracts.md` |
| 60 | `TRACKED_platform_contracts.md` |
| 61 | `TRACKED_verification_coverage.md` |
| 63 | `TRACKED_test_flakiness.md` |
| 64 | `TRACKED_ci_gate_coverage.md` |
| 65 | `TRACKED_ci_gate_coverage.md` |
| 66 | `TRACKED_correctness_residuals.md` |
| 67 | `TRACKED_process_record.md` |
| 68 | `TRACKED_process_record.md` |
| 69 | `TRACKED_test_flakiness.md` |
| 70 | `TRACKED_ci_gate_coverage.md` |
| 72 | `TRACKED_ci_gate_coverage.md` |
| 73 | `TRACKED_ci_gate_coverage.md` |
| 74 | `TRACKED_ci_gate_coverage.md` |
| 76 | `TRACKED_ci_gate_coverage.md` |
| 78 | `TRACKED_process_record.md` |
| 79 | `TRACKED_process_record.md` |
| 80 | `TRACKED_ci_gate_coverage.md` |
| 81 | `TRACKED_process_record.md` |
| 82 | `TRACKED_ci_gate_coverage.md` |
| 83 | `TRACKED_process_record.md` |
| 84 | `TRACKED_verification_coverage.md` |
| 85 | `TRACKED_publish_readiness.md` |
| 86 | `TRACKED_process_record.md` |
| 87 | `TRACKED_ci_gate_coverage.md` |
| 88 | `TRACKED_ci_gate_coverage.md` |
| 89 | `TRACKED_process_record.md` |
| 90 | `TRACKED_publish_readiness.md` |
| 91 | `TRACKED_publish_readiness.md` |
| 92 | `TRACKED_ci_gate_coverage.md` |
| 93 | `TRACKED_publish_readiness.md` |
| 94 | `TRACKED_publish_readiness.md` |
| 95 | `TRACKED_ci_gate_coverage.md` |
| 96 | `TRACKED_test_flakiness.md` |
| 97 | `TRACKED_publish_readiness.md` |
| 98 | `TRACKED_publish_readiness.md` |
| 99 | `TRACKED_publish_readiness.md` |
| 100 | `TRACKED_publish_readiness.md` |
| 101 | `TRACKED_publish_readiness.md` |
| 102 | `TRACKED_publish_readiness.md` |
| 103 | `TRACKED_publish_readiness.md` |
| 104 | `TRACKED_publish_readiness.md` |
| 105 | `TRACKED_publish_readiness.md` |
| 106 | `TRACKED_publish_readiness.md` |
| 107 | `TRACKED_ci_gate_coverage.md` |
| 108 | `TRACKED_publish_readiness.md` |
| 109 | `TRACKED_publish_readiness.md` |
| 110 | `TRACKED_publish_readiness.md` |
| 111 | `TRACKED_publish_readiness.md` |
| 112 | `TRACKED_publish_readiness.md` |
| 113 | `TRACKED_publish_readiness.md` |
| 114 | `TRACKED_publish_readiness.md` |
| 115 | `TRACKED_publish_readiness.md` |
| 116 | `TRACKED_publish_readiness.md` |
| 117 | `TRACKED_publish_readiness.md` |
| 118 | `TRACKED_publish_readiness.md` |
| 119 | `TRACKED_publish_readiness.md` |
| 120 | `TRACKED_publish_readiness.md` |
| 121 | `TRACKED_publish_readiness.md` |
| 122 | `TRACKED_publish_readiness.md` |
| 123 | `TRACKED_publish_readiness.md` |
| 124 | `TRACKED_publish_readiness.md` |
| 125 | `TRACKED_publish_readiness.md` |
| 126 | `TRACKED_publish_readiness.md` |
| 127 | `TRACKED_publish_readiness.md` |
| 128 | `TRACKED_publish_readiness.md` |
| 129 | `TRACKED_publish_readiness.md` |
| 130 | `TRACKED_publish_readiness.md` |
| 131 | `TRACKED_publish_readiness.md` |
| 132 | `TRACKED_publish_readiness.md` |
| 133 | `TRACKED_publish_readiness.md` |
| 134 | `TRACKED_publish_readiness.md` |
| 135 | `TRACKED_publish_readiness.md` |
| 136 | `TRACKED_publish_readiness.md` |
| 137 | `TRACKED_publish_readiness.md` |
| 138 | `TRACKED_publish_readiness.md` |
| 139 | `TRACKED_publish_readiness.md` |
| 140 | `TRACKED_ci_gate_coverage.md` |
| 141 | `TRACKED_publish_readiness.md` |
| 142 | `TRACKED_publish_readiness.md` |
| 143 | `TRACKED_test_flakiness.md` |
| 144 | `TRACKED_publish_readiness.md` |
| 145 | `TRACKED_test_flakiness.md` |
| 146 | `TRACKED_test_flakiness.md` |
| 147 | `TRACKED_test_flakiness.md` |
| 148 | `RESOLVED.md` |
| 149 | `RESOLVED.md` |
| 150 | `TRACKED_test_flakiness.md` |
| 151 | `TRACKED_ci_gate_coverage.md` |
| 152 | `TRACKED_platform_contracts.md` |
| 153 | `TRACKED_test_flakiness.md` |
| 154 | `TRACKED_misc.md` |
| 155 | `TRACKED_correctness_residuals.md` |
| 156 | `TRACKED_ci_gate_coverage.md` |
| 157 | `RESOLVED.md` |
| 158 | `TRACKED_misc.md` |
| 159 | `TRACKED_misc.md` |
| 160 | `TRACKED_misc.md` |
| 161 | `TRACKED_misc.md` |
| 164 | `TRACKED_correctness_residuals.md` |
| 165 | `TRACKED_correctness_residuals.md` |
| 166 | `TRACKED_correctness_residuals.md` |
| 167 | `TRACKED_verification_coverage.md` |
| 168 | `RESOLVED.md` |
| 169 | `RESOLVED.md` |
| 170 | `RESOLVED.md` |
| 171 | `TRACKED_verification_coverage.md` |
| 172 | `RESOLVED.md` |
| 173 | `RESOLVED.md` |
| 174 | `RESOLVED.md` |
| 175 | `RESOLVED.md` |
| 176 | `RESOLVED.md` |
| 177 | `RESOLVED.md` |
| 178 | `RESOLVED.md` |
| 179 | `RESOLVED.md` |
| 180 | `RESOLVED.md` |
| 181 | `RESOLVED.md` |
| 182 | `RESOLVED.md` |
| 183 | `RESOLVED.md` |
| 184 | `RESOLVED.md` |
| 185 | `TRACKED_platform_contracts.md` |
| 186 | `TRACKED_verification_coverage.md` |
| 187 | `TRACKED_ci_gate_coverage.md` |
| 188 | `TRACKED_verification_coverage.md` |
| 189 | `TRACKED_ci_gate_coverage.md` |
| 190 | `TRACKED_ci_gate_coverage.md` |
| 191 | `TRACKED_misc.md` |
| 192 | `TRACKED_verification_coverage.md` |
| 193 | `TRACKED_verification_coverage.md` |
| 194 | `TRACKED_platform_contracts.md` |
| 195 | `TRACKED_ci_gate_coverage.md` |
| 196 | `TRACKED_platform_contracts.md` |
| 197 | `TRACKED_verification_coverage.md` |
| 198 | `TRACKED_ci_gate_coverage.md` |
| 199 | `TRACKED_misc.md` |

**Citing an item going forward:** the established convention --
`` `docs/CORRECTNESS_OPEN_ITEMS.md` item N `` -- is UNCHANGED and remains
correct; this file stays the canonical citation target precisely so nothing
downstream needs to learn a new path. A reader resolves an `[A]` card in
`ACTIVE.md`; a `[T]` card or retained closure pointer resolves through the
table. If its tier or number is unknown, grep `docs/correctness-open-items/*.md`
directly.

**Card census (task #1217, 2026-08-20; re-derived after task #1221's
number-range re-split; re-derived again, unchanged in total, after task
#1222's thematic re-split, same day).** Re-derived during review from the
committed files themselves, not from the split's own working copy — see
the paragraph below (task #1217's own finding) for why that distinction
mattered. Reproduce with:

```text
grep -cE '^[0-9]+[a-z]*\. \*\*' docs/correctness-open-items/ACTIVE.md
grep -hcE '^[0-9]+[a-z]*\. \*\*' docs/correctness-open-items/TRACKED_*.md
grep -hE '^[0-9]+[a-z]*\. \*\*' docs/correctness-open-items/ACTIVE.md docs/correctness-open-items/TRACKED_*.md | wc -l
```

The three commands count **numbered records**, including retained CLOSED /
RESOLVED records; they do not calculate an open-only total. The `[A]` tier
currently contains 1, 2, 11, 13, 62, 162, and 163. The `[T]` IDs are the
table rows targeting `TRACKED_*.md`, not its resolution pointers or the
historical 78–97 range from the initial split. For actionable state, read
each card's current Status / Next trigger. The complete R14 round-start
inventory is preserved in `docs/reviews/2026-10-06-src-review-sol-round-14.md`
appendix B, including the publish-readiness tail.
Items 1-4's original flaky-test cards are separately already-resolved
stub pointers inside the `[T]` tier's own intro text (now duplicated
verbatim at the top of `TRACKED_test_flakiness.md`, the file whose theme
they most resemble, so a reader landing there first still sees the
pointer), superseded by real cards later reusing numbers 1 and 2 for
unrelated `[A]`-tier findings — a pre-existing, intentional renumbering
documented at task #1143, not a defect introduced by any split.

**One card was lost by the task-#1217 split and restored in review --
recorded because the loss mechanism is reusable, not because it survived
into this split.** The task-#1217 split was performed in a git worktree
branched from a commit that did not yet contain `105cf53` (task #1209), so
its source copy of the pre-split file was 2,555 lines where the shared
checkout held 2,566. Those 11 lines were exactly **item 93**, the card
filing the fourth independent audit's NO-GO verdict — the single newest
card in the file at that time. The split's own card census PASSED while
item 93 was missing, because it compared the output against that same
stale source: a census that re-derives both sides from one snapshot cannot
detect a card the snapshot never had. This is the same class as task
#1116, where the FIRST split of this index truncated 9 pointers
mid-heading and lost the verdict on 19 of 32 — that one was caught by an
independent reader, item 93's loss by re-running the census against
`git show HEAD:docs/CORRECTNESS_OPEN_ITEMS.md` instead of against the
worktree. **Census both sides from committed history, not from the working
copy the transformation itself read.** Task #1222 (this split) re-applied
that lesson: its own census (above) was run against `git show
main:<old-path>` for all four number-range files, not against this
worktree's own copies, before any new file was written.
