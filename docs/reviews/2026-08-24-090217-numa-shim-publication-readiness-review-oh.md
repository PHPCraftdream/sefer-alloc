# Twelfth independent pre-publication review — `numa-shim` @ `1afed26`

**Author:** `@oh` (Opus, effort=high). **Reported:** 2026-08-24 09:02:17 Europe/Berlin.
**Revision reviewed:** `1afed26e7776126b09c91a098a049e5d19f53b08` (`main`, local **and**
`origin/main` — the two are identical for the first time in this campaign), working tree
clean apart from two untracked `docs/checkpoints/*.md` files.
**Mode:** READ-ONLY. No sub-agents. No file edited, no `git` write command run, no
`cargo` build/test/clippy/fmt invoked on this host.

**Two exceptions to "static", both read-only and both load-bearing for this verdict:**

1. I queried the GitHub Actions API directly (`gh run view 32696488144 --json …`,
   `gh run view --job <id> --log`) and **read the raw CI logs of all four `numa-shim-*`
   jobs**. This is what lets me settle, with receipts rather than by reading YAML, the two
   questions the eleventh review could only mark UNVERIFIED-BY-EXECUTION: did the
   converted mock rows actually run mock tests, and did the new Linux `vmem-integration`
   row actually execute the real `reserve_on_node` path. Both: **yes** — see §2 and §6.
2. Everything else is `git`/`grep`/file reads.

**Scope:** this is the review the campaign's standing loop calls for after tasks
#1288–#1291 landed against the eleventh review's C1/C2/C3 and E-findings
(`docs/reviews/2026-08-23-192230-numa-shim-publication-readiness-review-oh.md`, item 102).
It re-verifies each of those four tasks' claimed changes **against current source rather
than against their own commit messages**, hunts specifically for the incomplete-rollout /
doc-contract-mismatch / vacuous-green classes this campaign has repeatedly produced, and
looks for defects nobody has filed.

**Finding IDs:** ninth used `F1–F8`, tenth `N1–N12`, eleventh `E1–E11`. This one uses
**`T1–T16`** ("twelfth"). Severity uses this campaign's P0–P3 scale.

**Filename ASCII-only**, matching the convention the fifth and eighth audits adopted for
`scripts/verify-commit-prefixes.mjs` compatibility.

---

## 0. Verdict

**CONDITIONAL GO**, on **five conditions, none of which is a code change**.

The eleventh review's three conditions are settled:

- **C1 (nothing pushed, no CI run exists) — CLOSED, verified independently.**
  `git rev-parse HEAD` == `git rev-parse origin/main` == `1afed26e7776…`. CI run
  `32696488144` on **exactly that SHA** is `conclusion: success`; a job-level check
  (`gh run view --json jobs`) gives **38 `success`, 3 `skipped`, 0 `failure`, 0
  `cancelled`** across 41 jobs — the three skips are the schedule-only
  `numa-real-kernel`, `feature-powerset` and `fuzz-run` jobs, exactly as their `if:`
  guards require. Every "gates green" claim this campaign accumulated over ~40 unpushed
  commits now has a receipt.
- **C2 (Phase 1 stale again) — converted into #1262's ordering rule, unchanged.** Still
  correctly open by design, and now carrying one newly-broken detail — see **T4**.
- **C3 (no CI job executes the real Linux `reserve_on_node`) — CLOSED, with a log
  receipt.** `.github/workflows/ci.yml:2689` (`cargo test -p numa-shim --features
  vmem-integration`, ubuntu) ran, and job `97339434380`'s log contains
  `test reserve_on_node_returns_valid_span ... ok` and
  `test reserve_on_node_large_align_round_trip ... ok` — the crate's headline capability
  executed on its headline platform for the first time in this repository's history.

**No P0, no P1. No UB, no soundness hole, and no correctness defect.** Task #1288 — by far
the largest change in this wave, 24 files — is, at the code level, a **purely mechanical
1:1 attribute rewrite**: filtering `git show 48ab4e5 -- crates/numa-shim/src/lib.rs` to
non-comment lines yields exactly 21 attribute pairs, `feature = "mock"` → `numa_shim_mock`,
and **zero** semantic edits (§2). Tasks #1289 and #1290 are docs-only by `git show --stat`.
Task #1291 is CI-only and is **complete**: only two `windows-latest` jobs exist in this
workflow, and I checked every `run:` step in both.

The five conditions:

1. **T1 (P3) — one published-rustdoc site that #1288 missed.** `src/lib.rs:319` still
   says `NodeResolution::Unavailable` is returned "Under the **`mock` feature**" — a Cargo
   feature this very release REMOVES, on a type that renders unconditionally on docs.rs.
   Its sibling variant one screen up (`:287`) was correctly updated. Shipping 0.2.0 with
   this means publishing a docs.rs page that contradicts its own CHANGELOG's headline
   `### Removed` entry. **One clause.**
2. **T4 (P2) — the final Phase-1 gate re-run's recorded invocation is now dead.** Both
   gate-run reports and both index cards prescribe `cargo test -p numa-shim --features
   mock`, which post-#1288 **errors out**. #1262's own last step cannot execute as
   written. Correct invocation and expected count are in **T4**, both confirmed against
   the CI log.
3. **T7 (P2) — `numa-shim` has no package-gates CI job**, unlike both of its sibling
   publish-candidates (`sefer-region-gates` `:53`, `aligned-vmem-gates` `:136`, each with
   `cargo publish --dry-run` + `cargo semver-checks check-release`). Packaging is first
   exercised at real publish time. Mitigation is one manual `cargo publish --dry-run -p
   numa-shim` (or the release workflow's own dry-run checkbox) before the tag.
4. **T11 + T16 (INFO) — #1262's checklist is incomplete and partly stale.** The waiver
   record carries a live `[TO BE FILLED: task #1262's landing SHA]` placeholder that
   appears in no card's #1262 site list, and every card cites the root pin as
   `Cargo.toml:914` when it is now at `:933`.
5. **T5 + T6 (P3) — the index cards are stale again, for the fourth consecutive time.**
   #1289 DECIDED F5 and touched no index file; #1291 discharged C1 and touched no index
   file. A fresh session reading item 100 today concludes F5 is undecided, and reading
   item 102 concludes nothing has been pushed.

**Still owner-gated, unchanged and correctly so:** F1/#1262 (the version bump itself) and
F8's Phase 3 in-guest remainder. F8 phases 2/4 are now formally waived for 0.2.0 by an
explicit, correctly-scoped record (#1290, verified §2). F2 (item 42) and F5 are both
DECIDED and EXECUTED in source; only their index bookkeeping lags (T5).

**Is #1262 the correct and sufficient next step?** Correct — yes. **Sufficient — no.**
T1 should land before or with it (it is a published-surface statement about the exact
feature #1262's release removes); T4's invocation correction is a precondition for
#1262's own final step to run at all; T11/T16 must be folded into its site list first.
T7's dry-run is strongly recommended before the tag, not after.

### Disposition of the eleventh review's conditions and E-findings at `1afed26`

| Item | Owning task | State in source / CI | Verdict |
|---|---|---|---|
| C1 — nothing pushed, no CI run | (owner) | `origin/main` == `HEAD` == `1afed26`; run `32696488144` 38 success / 3 skipped / 0 failure | **CLOSED** (log-verified, §6) |
| C2 — Phase 1 stale again (E1) | #1262 | ordering rule folded into #1262; still un-run by design | **OPEN by design** → **T4** |
| C3 — no real Linux `reserve_on_node` row (E2) | #1282 | `ci.yml:2689`; both `reserve_on_node_*` tests `... ok` in job `97339434380` | **CLOSED** (log-verified) |
| E3 — mock-log recording convention docs | #1283 | `src/lib.rs:141-148` (raw pre-remap) + `:354-368` ("intentionally a DIFFERENT convention") | **CLOSED** |
| E4 — README omits `NodeResolution` | #1284 | `README.md:113-126` | **CLOSED** |
| E5 — dangling README reference link | #1284 | `README.md:59-60`, now an inline crates.io link | **CLOSED** |
| E6 — stale Status cards | #1286 | refreshed as the wave's last commit — then re-staled by #1289/#1291 | **CLOSED then RE-OPENED** → **T5/T6** |
| E7 — mock-surface CHANGELOG bullets | #1285 | `CHANGELOG.md:110-118` | **CLOSED** |
| E8 — MSRV never checks numa-shim | #1282 | `ci.yml:1916-1917` | **CLOSED**; mock arm still unchecked → **T12** |
| E9 — N11 residuals | partly #1287 | `dbg_*` hook classified in `tests/dbg_hook_safety_tripwire.rs`; 2 residuals remain | **PARTIAL** → **T13** |
| E10 — three README `0.1` pins | #1262 | `README.md:33`, `:36`, `:64` unchanged, correctly | **OPEN by design** |
| E11 — nothing pushed | (owner) | same as C1 | **CLOSED** |
| F2 / item 42 (`mock` feature) | #1288 | feature removed; `--cfg numa_shim_mock` throughout | **EXECUTED** → residuals T1/T2/T3/T15 |
| F5 (doc-hidden semver policy) | #1289 | exemption text in both modules + CHANGELOG + README | **EXECUTED**; index stale → **T5** |
| F8 phases 2/4 | #1290 | dated waiver, correctly scoped | **WAIVED for 0.2.0** |
| pwsh `VAR=val` CI break | #1291 | `ci.yml:2753-2755`, `:2756-2758`, `:2772-2774` | **CLOSED**, complete (§2) |

---

## 1. What I read and ran

Full current source of `crates/numa-shim/` (`src/lib.rs`, `Cargo.toml`, `README.md`,
`CHANGELOG.md`, `benches/numa_bench.rs`, all five test files). The full diffs and
`--stat` of `48ab4e5`, `04b115c`, `8f9bdf9`, `b6db2ac`, `099a4cf`, `1afed26`, plus
`99ecef4` (#1287). `docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md` end to end;
`docs/NUMA_RELEASE_GATE.md`; both `docs/NUMA_GATE_RUN_2026-08-23_*.md`;
`docs/NUMA_TESTING_OPTIONS.md`'s Phase-1 section; `docs/PHASE_NUMA_DESIGN.md`'s test
coverage section. Items 100/101/102 and the F5 addendum in
`docs/correctness-open-items/TRACKED_publish_readiness.md`; item 42's entries in
`RESOLVED.md` and `ARCHIVE.md`; `docs/CORRECTNESS_OPEN_ITEMS.md`'s tier lists and lookup
table; `tests/no_stale_doc_references.rs:1465`'s pointer guard. The whole of
`.github/workflows/ci.yml`'s `numa-shim-*`, `test-windows`, `test-macos`, `msrv`,
`sefer-region-gates`, `aligned-vmem-gates` jobs and the root mock row at `:1235-1247`;
`release.yml`'s changelog and CI-green guards. Root `Cargo.toml`'s numa wiring and pin;
all seven re-gated root test files.

**Ran (read-only):** `git rev-parse`/`log`/`show`/`status`; `grep` sweeps; `gh run view
32696488144 --json {jobs,headSha,conclusion}`; `gh run view --job {97339434380,
97339434414, 97339434453, 97339434523} --log`.

---

## 2. Re-verification of #1288–#1291 against source

### #1288 (`48ab4e5` + `04b115c` + `8f9bdf9`) — VERIFIED; rollout complete except three prose sites

**The code change is mechanical, and I checked that rather than assuming it.** Filtering
the `src/lib.rs` diff to non-comment lines
(`git show 48ab4e5 -- crates/numa-shim/src/lib.rs | grep -E '^[-+]' | grep -vE '^[-+]{3}' | grep -vE '^[-+]\s*(///|//|$)'`)
yields **exactly 21 attribute pairs and nothing else**: 10 `#[cfg(feature = "mock")]`/
`#[cfg(not(feature = "mock"))]` → `#[cfg(numa_shim_mock)]`/`#[cfg(not(numa_shim_mock))]`
(the `mod mock` gate, four `pub fn`s × 2 arms, the `pub mod linux` gate) and 11
`#[cfg_attr(feature = "mock", allow(dead_code))]` → `#[cfg_attr(numa_shim_mock, …)]`. No
function body, no signature, no cfg *shape* changed. The commit body's own claim of "21
active cfg sites" is exact.

**Rollout sweep.** Across every tracked `*.toml`/`*.rs`/`*.yml`/`*.mjs`/`*.json` in the
workspace there is **exactly one** remaining `feature = "mock"` string, and it is a
past-tense comment (`ci.yml:2649`, → **T15**). Zero `features = ["mock"]`, zero
`numa-shim/mock` outside `docs/reviews/` and `docs/correctness-open-items/ARCHIVE.md`
(both historical by construction). Verified:

```
grep -rn 'feature = "mock"' $(git ls-files '*.toml' '*.rs' '*.yml' '*.mjs' '*.json')
→ .github/workflows/ci.yml:2649
```

**Manifest side.** `crates/numa-shim/Cargo.toml`: `[features]` is now `default = []` +
`vmem-integration` only (`:27-29`); `[lints.rust] unexpected_cfgs = { level = "warn",
check-cfg = ['cfg(numa_shim_mock)'] }` added (`:52-53`); `[[bench]]`'s
`required-features = ["mock"]` removed (`:62-72`). Root `Cargo.toml`:
`numa-aware-mock = ["numa-aware"]` (`:740`, no longer forwards), `'cfg(numa_shim_mock)'`
added to the workspace check-cfg list (`:108`). The check-cfg declarations are **proven,
not assumed**: `ci.yml:2704` and `:2772` run `cargo clippy -p numa-shim --all-targets --
-D warnings` under the cfg and are green in run `32696488144`, which they could not be if
`unexpected_cfgs` fired.

**The bench guard is genuinely fail-loud and does not create a red row.**
`benches/numa_bench.rs:16-35`: `main()` panics under `#[cfg(not(numa_shim_mock))]` with
the exact rerun command; `run()` is cfg'd in only under the cfg. Because the bench uses
`harness = false`, `cargo test` builds but does not RUN it — confirmed empirically from
the CI logs, where every `cargo test -p numa-shim` step lists exactly six test binaries
(`unittests src/lib.rs`, `cpumap_parser`, `mock_dispatch`, `node_resolution`,
`node_resolution_linux`, `smoke`) and never `numa_bench`. Dropping `required-features`
was therefore safe, and the runtime panic (not `compile_error!`) is the right choice for
exactly the reason the comment gives.

**Root wiring.** All six mock-using root test files gained a `numa_shim_mock` conjunct
(`tests/numa_cache_invalidation.rs:33-37`, `numa_periodic_refresh.rs:27-31`,
`segment_directory_numa.rs:22-27`, `…_bucket_reuse.rs:84-89`, `…_high_node_ids.rs:40-45`,
`segment_directory_clear_bit_no_register.rs:65-70`), and
`tests/alloc_core_reentrancy.rs:50` inverted from a feature-based skip to
`#![cfg(not(numa_shim_mock))]` — which is a **coverage gain**: that test now RUNS under
plain `--all-features`, where it previously skipped.

**Item 42's index bookkeeping.** The card is out of `ACTIVE.md` (grep: zero hits), the
`[A]`-tier count line in `docs/CORRECTNESS_OPEN_ITEMS.md:400` went 6 → 5 with `42` dropped
from the enumeration, the full narrative is in `ARCHIVE.md` with a trailing
`**CLOSED 2026-08-23, task #1288**` paragraph (`:1690`), and `8f9bdf9` repaired the
`RESOLVED.md` pointer. I read `tests/no_stale_doc_references.rs:1465`'s guard to
understand what `8f9bdf9` was actually satisfying: it requires the pointer's
pre-boilerplate headline to be **byte-identical** to the archive entry's first paragraph
and to carry a whole-word verdict token. `8f9bdf9`'s fix is correct against that contract
— with one consequence worth naming, → **T10**.

### #1289 (`b6db2ac`) — VERIFIED, consistent across all four surfaces

`git show --stat` = 3 files, +41/−2, **zero code**. The semver-exemption text is present
verbatim in **both** modules — `src/lib.rs:601-607` (`pub mod cpumap`) and `:751-757`
(`pub mod linux`) — and both keep `#[doc(hidden)]` + `pub` exactly as the option-(d)
decision specifies. Mirrored in `CHANGELOG.md:134-146` (`### Changed`, docs-only, "zero
code, zero visibility change" — true) and `README.md:142-146`. The `serde::__private`
framing is accurate, and the supporting claim that `cargo-semver-checks` excludes
`#[doc(hidden)]` items is correct. I checked the modules' actual visibility surface is
unchanged: `pub mod cpumap` is unconditional (`:608-609`), `pub mod linux` keeps its
`#[cfg(all(target_os = "linux", not(miri), not(numa_shim_mock)))]` gate (`:758`), so
docs.rs (Linux, `features = ["vmem-integration"]`, no cfg) compiles both and renders
neither.

### #1290 (`099a4cf`) — VERIFIED; the scoping is correct, and tighter than it had to be

`git show --stat` = 4 files, +113/−0, docs-only. I read the waiver against the specific
scope-creep/scope-gap question in the brief. It is clean on **all four** axes:

- **Does not waive Phase 1's final re-run:** `:38-44` states the PASS is "Known-stale per
  item 102's finding E1", names the two commits that superseded it, restates the
  run-LAST-after-the-version-bump ordering rule, and closes "**This record does not waive
  Phase 1.**"
- **Does not waive Phase 3's remainder:** `:45-50` — "the actual in-guest Hyper-V
  procedure never ran. **This record does not waive completing Phase 3**".
- **Does not amend the policy:** `:63-68` — future releases still owe Phases 1–4.
- **Does not waive C1/E1:** `:69-71` names both explicitly.

It is also scoped to "the 0.2.0 release only" (`:51-52`), names the decider and date
(`:17-18`), gives an infrastructure (not judgment) reason with the two gate-run reports as
its evidence (`:23-31`), and records that no synthetic substitute was attempted. Linked
from `docs/NUMA_RELEASE_GATE.md:220` and mirrored consumer-facing in
`CHANGELOG.md:15-34`. One live placeholder — `:19` — → **T11**.

### #1291 (`1afed26`) — VERIFIED, and I checked it is complete, not just correct

The diagnosis in the commit body is right: `windows-latest` `run:` steps default to
`pwsh`, this workflow sets no `defaults.run.shell`, and pwsh rejects bash's `VAR=val cmd`
prefix. The fix converts all three rows to step-level `env:` blocks (`:2753-2755`,
`:2756-2758`, `:2772-2774`), which is shell-agnostic — Actions sets the variable on the
child process before invoking any shell.

**Completeness check, done independently of the commit's claims.** There are exactly two
`windows-latest` jobs in the file: `test-windows` (`:1452`) and `numa-shim-windows`
(`:2733`). I read every `run:` step in both.

- `test-windows` was already immune: its five multi-line steps each carry **both**
  `shell: bash` and an `env:` block for `RUSTFLAGS` (`:1493-1497`, `:1503-1507`,
  `:1517-1520`, `:1521-1526`, `:1561-1567`). Not one inline `VAR=val` prefix.
- `numa-shim-windows` now has none either.

The seven surviving inline-prefix rows in the whole file (`:65`, `:314`, `:329`, `:2704`,
`:2714`, `:2789`, `:2827`) are all on `ubuntu-latest` or `macos-latest`, where `run:`
defaults to bash. **No other job carries this latent bug.**

**And the fix is confirmed to have worked, from the logs rather than from the green tick.**
Job `97339434414` (numa-shim real Windows kernel):

| Step | `mock_dispatch.rs` | `node_resolution.rs` |
|---|---|---|
| `cargo test -p numa-shim` (06:15:27, no cfg) | 0 tests | 0 tests |
| `… --features vmem-integration` (06:15:28, no cfg) | 0 tests | 0 tests |
| `cargo test -p numa-shim` + `env: RUSTFLAGS` (06:15:30) | **7 passed** | **5 passed** |
| `… --features vmem-integration` + `env:` (06:15:32) | **9 passed** | **5 passed** |

The mock backend genuinely activated on Windows through the `env:` block. Not vacuous.

---

## 3. Regression hunt — did any of the four changes break something?

This campaign's live pattern is "a fix introduces a regression" (twice: #1269→N4/N5,
#1276→E3). I hunted for it deliberately.

**#1288's cfg conversion: no functional change is even possible.** The diff is 21
attributes; every cfg predicate is a 1:1 rename of the same shape (`X` → `Y`,
`not(X)` → `not(Y)`, `cfg_attr(X, …)` → `cfg_attr(Y, …)`). There is no site where a
conjunct was added, dropped, or re-associated. The four public functions' dispatch, the
`bind_range` short-circuit, the Windows reserve/commit/release ownership chain the
eleventh review walked exit-by-exit, the cpumap parser, and the `OnceLock` topology cache
are **byte-identical** to `1043b0e`.

**#1288's manifest changes: one behavioural consequence, and it is a gain not a loss.**
Dropping `required-features = ["mock"]` means the bench target is now BUILT by every
`cargo test`/`cargo clippy --all-targets` row that previously skipped it. That is the
riskiest part of the change (a compile error there would redden rows that never compiled
it). It compiled on all three OSes in run `32696488144`, including under
`clippy … -- -D warnings` on two of them.

**#1288's root re-gating: one file changed skip-polarity.**
`tests/alloc_core_reentrancy.rs` moved from "skipped whenever `numa-aware-mock` is on" to
"skipped only under `--cfg numa_shim_mock`", so it now runs under plain `--all-features`.
Its M5 no-global-alloc invariant therefore executes in a configuration where it never did
before — green in run `32696488144`'s `test (--all-features)` step.

**#1289/#1290: docs-only by `--stat`.** No `.rs` outside doc comments; no visibility, cfg,
or signature touched.

**#1291: CI-only**, 1 file, and the two `env:`-carrying `cargo test` rows produce strictly
MORE executed tests than before (they previously failed the step outright).

**Net: no functional regression. The three residuals this wave leaves are all prose**
(T1, T2, T3) plus index bookkeeping (T5, T6). The fix-breaks-something pattern did not
recur.

---

## 4. What the eleventh review's conditions left open

- **C2 → T4.** The ordering rule survived; the *command* it will be executed with did not.
- **E6 → T5/T6.** #1286 correctly ran the refresh as the wave's LAST commit — and then
  four more commits landed after it, three of which changed the disposition of something
  a card asserts. The structural fix E6 proposed addresses ordering *within* a wave; this
  wave shows the failure mode also occurs *between* waves.
- **E9 → T13.** One of three residuals is now closed, by a task nobody assigned to E9:
  #1287 discovered that a repo-wide tripwire (`tests/dbg_hook_safety_tripwire.rs`) already
  enforces the CLAUDE.md R25-1 rule that both the tenth and eleventh reviews had filed as
  "a convention point, not a defect". Its commit body says so bluntly, and it is right:
  **two consecutive independent reviews under-rated a finding because neither knew the
  automated guard existed.** Worth carrying forward as a lesson about this campaign's own
  method, not just about numa-shim.
- **E10 → still open by design**, and now with a stale line number (**T16**).

---

## 5. New findings

### T1 (P3) — `NodeResolution::Unavailable`'s published rustdoc still names the `mock` **feature** that this release removes

`crates/numa-shim/src/lib.rs:319`:

```
/// - Under the `mock` feature when the scripted node is [`NO_NODE`].
```

Its sibling variant, 32 lines earlier, WAS converted — `:287`: *"or under the
`numa_shim_mock` cfg when the scripted node is not [`NO_NODE`]"*. So the same enum
documents the same mechanism two different ways, one of them naming a Cargo feature that
`crates/numa-shim/CHANGELOG.md:184-195` records as **removed** in this very release.

Why this is worth a condition rather than a shrug: `NodeResolution` is unconditionally
public (no `#[cfg]`), so this line renders on the docs.rs landing page for 0.2.0, and it
is the *only* mechanism-naming sentence a reader of that variant sees. A consumer
following it would run `cargo test --features numa-shim/mock` and get
`error: none of the selected packages contains these features: mock`. This is exactly the
doc/contract-mismatch class F3, N7 and E3 have each been raised about — the fourth
instance, one function over, each time in a doc site adjacent to the one just fixed.

**Fix: one clause**, matching `:287`'s wording.

### T2 (P3) — `docs/PHASE_NUMA_DESIGN.md` tells a reader to reproduce a mock test with an invocation that now yields an empty binary

`docs/PHASE_NUMA_DESIGN.md:435-436`:

> `tests/numa_cache_invalidation.rs` (gated on the `numa-aware-mock` feature, **which
> enables `numa-shim/mock`** for deterministic control of `current_node()`'s return value)

Both halves are now false. `numa-aware-mock = ["numa-aware"]` (root `Cargo.toml:740`) — it
enables nothing in `numa-shim`; and the test's real gate is
`all(all(feature = "numa-aware-mock", feature = "alloc-global"), feature = "internals", numa_shim_mock)`
(`tests/numa_cache_invalidation.rs:33-37`).

The consequence is the specific one #1288's own CI sentinels exist to prevent: a reader
who runs `cargo test --features numa-aware-mock --test numa_cache_invalidation` gets
"running 0 tests … ok" — a **silent** green on an empty binary, not an error. #1288
correctly updated `docs/FEATURE_PROMOTION_STATUS.md:75` and root `Cargo.toml`'s comment
block; this doc was missed. It is also the design document this crate's NUMA behaviour is
specified in, so it is not a low-traffic file.

**Fix: one sentence**, ideally quoting the full four-conjunct gate and the
`RUSTFLAGS="--cfg numa_shim_mock"` invocation the test file's own header (`:28`) already
gives correctly.

### T3 (P3) — `docs/NUMA_TESTING_OPTIONS.md`'s Phase-1 section still prescribes `mock = []`, and the published manifest routes readers to it

`docs/NUMA_TESTING_OPTIONS.md:219` (`mock = []` inside a `[features]` block), `:222`/`:239`/
`:245` (`#[cfg(feature = "mock")]` / `#[cfg(not(feature = "mock"))]`), `:249` ("Tests in
`crates/numa-shim/tests/mock_dispatch.rs` (gated on `feature = "mock"`)").

Mitigating: the section is headed "Phase 1 — mock-shim concrete design (for follow-up
task)", i.e. historical-by-framing. Aggravating: **`crates/numa-shim/Cargo.toml:41` — a
file that ships to crates.io — points at it**: "see docs/NUMA_TESTING_OPTIONS.md Phase 1
for the original design note." A reader who follows that pointer lands on a design that
contradicts the manifest they came from.

**Fix: either a one-line "SUPERSEDED by task #1288 — the mock is now `--cfg
numa_shim_mock`" banner on §"Phase 1", or drop the cross-reference from the manifest.**
Lowest-cost is the banner.

### T4 (P2) — the final Phase-1 gate re-run cannot be executed as recorded; #1262's own last step is blocked on a stale command

Every place that records how to run Phase 1 prescribes `cargo test -p numa-shim --features
mock`:

- `docs/NUMA_GATE_RUN_2026-08-23_task1270.md:8` and `:16`
- `docs/NUMA_GATE_RUN_2026-08-23_task1279_phase1_rerun.md:12`
- `docs/correctness-open-items/TRACKED_publish_readiness.md:450` and `:484`
  ("F8 Phase 1 re-run LAST … expect 33 tests under `--features mock`, not 31")

Post-#1288 that command fails with `error: none of the selected packages contains these
features: mock`. **Fail-loud, not vacuous-green** — so this is a scheduling/record defect,
not a correctness hazard — but it will stop #1262 at its final step, which by E1's own
ordering rule is the last thing that happens before the tag.

The two gate-run reports are historical records and should stay as they are (this repo's
history-is-not-rewritten convention). The **index cards** are current-state documents and
must be corrected.

Correct current invocation and expectation, both derived from run `32696488144`'s
`numa-shim-mock` job log rather than predicted:

```
RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim
```

| Binary | Tests |
|---|---|
| `unittests src/lib.rs` | 0 |
| `tests/cpumap_parser.rs` | 17 |
| `tests/mock_dispatch.rs` | 7 |
| `tests/node_resolution.rs` | 5 |
| `tests/node_resolution_linux.rs` | 0 (correctly skipped under the cfg) |
| `tests/smoke.rs` | 4 |
| doc-tests | 0 |
| **total** | **33** |

The cards' predicted **33** is therefore CORRECT — only the command is wrong. Note that a
Windows host (where the prior Phase-1 runs were taken) yields the same 33, since
`node_resolution_linux.rs` is empty there for a different reason.

### T5 (P3) — item 100's F5 disposition is stale: #1289 decided F5 and updated no index file

Three sites in `docs/correctness-open-items/TRACKED_publish_readiness.md` still present F5
as undecided:

- `:396` (the Status card, the R34-24 "first visible block") — "**F5 writeup landed,
  decision pending** (#1267 — see this card's F5 addendum below)"
- `:402` — "**F5 (P2) — writeup landed, owner decision pending (task #1267 …)**"
- `:418` — "**Status:** F5 = RECOMMENDATION RECORDED, awaiting owner decision"

Meanwhile the decision is DONE and recorded in three places in the shipping crate:
`CHANGELOG.md:44-55` ("**DECIDED** (task #1289, owner-confirmed)"), `src/lib.rs:601-607`
and `:751-757`, `README.md:142-146`. `git show --stat b6db2ac` = 3 files, none under
`docs/correctness-open-items/`.

CLAUDE.md's R34-24 rule makes the Status card the current-state contract for round-start
reading precisely because the in-session TaskList does not survive a session boundary. A
fresh session reading item 100 today would re-open a decision the owner already made and
that is already published in the crate's own CHANGELOG.

### T6 (P3) — item 102's card still says C1 is unreached, and that nothing has been pushed

`TRACKED_publish_readiness.md:425` and `:450`: *"C1 — push + CI-green confirmation on the
landing SHA — explicitly NOT owned by any TaskList task; requires the human owner's
explicit authorization to `git push`, which this session has not received for numa-shim"*,
and `:425`'s per-finding line adds "though the row has not executed anywhere pending C1"
about #1282's CI addition.

All of that is now false: `origin/main` == `HEAD` == `1afed26`, run `32696488144` is
green on that SHA, and #1282's row demonstrably executed (§0, C3). #1291 landed after
#1286's refresh and touched only `ci.yml`.

**T5 and T6 together are the fourth consecutive occurrence of this class** (N9 → E6 →
now, twice). E6's remedy — put the card refresh in the wave's last commit — was followed
by #1286 and still did not hold, because the wave did not end there. The durable version
is probably a mechanical one: a guard test that fails when a card asserts a task is
"pending"/"not started" while that task's commit exists in `git log`. I raise the idea;
I am not filing it as a requirement.

### T7 (P2) — `numa-shim` is the only publish-candidate crate in this workspace with no package-gates CI job

`.github/workflows/ci.yml` has `sefer-region-gates` (`:53`) and `aligned-vmem-gates`
(`:136`), each of which runs, per PR:

- `cargo publish --dry-run -p <crate>` (`:81`, `:355`)
- `cargo semver-checks check-release --package <crate>` (`:104`, `:389`)
- a sparse-index availability check (`:106-109`)

There is **no `numa-shim-gates` job**. The crate's four jobs (`:2630`, `:2733`, `:2776`,
`:2791`) run tests, clippy and rustdoc only. Consequences specific to this release:

1. **Packaging is first exercised at real publish time.** `cargo publish -p numa-shim`
   strips the `path` from `aligned-vmem = { version = "0.2", path = "../aligned-vmem", optional = true }`
   (`crates/numa-shim/Cargo.toml:44`) and must resolve `aligned-vmem 0.2` **and** the
   dev-dependency `bench-scale-tool = "0.1"` (`:60`) from the registry. The eleventh
   review verified both are published and unyanked, and `crates/aligned-vmem/Cargo.toml:3`
   is `0.2.0` locally, so I expect this to pass — but "expect" is the word this campaign
   exists to eliminate.
2. **No semver check against published 0.1.0.** For the record, one would now PASS: under
   Cargo's 0.x rules 0.1 → 0.2 *is* the major bump, so all four recorded breaks are legal.
   That makes adding the job cheap and non-blocking, not merely aspirational.

`release.yml` does guard the two worst release-time failure modes — a dated-CHANGELOG
check (`:300-310`) and a hard "main CI workflow must be green for THIS commit" poll
(`:311+`, keyed on `head_sha`, not "latest on main") — but neither exercises packaging.

**Minimum mitigation before the tag: run the release workflow once with its `dry-run`
checkbox ticked, or `cargo publish --dry-run -p numa-shim` locally.** Adding the job is
the better fix and mirrors two existing templates.

### T8 (P3) — three of the four `numa-shim-*` jobs' mock rows have no sentinel grep; the macOS-miri one is the only miri×mock cell anywhere

`ci.yml:2663-2675` (Linux) carries the task-#1101 `tee` + `grep -F` sentinels, and its own
comment states the rule: *"the greps require one named test per FILE … the vacuous-green
hazard this guards against is task #1070 'Breakage B'"* (`:2656-2662`). The three
converted rows on the other jobs — `:2753-2755` and `:2756-2758` (Windows), `:2789`
(macOS), `:2827` (macOS-miri) — have none. A cfg typo confined to one of those rows
compiles `mock_dispatch.rs` and `node_resolution.rs` to zero tests and exits 0.

**This is not a live defect** — I verified from the raw logs that all four jobs' mock rows
really ran the mock tests in run `32696488144`:

| Job | id | `mock_dispatch` | `node_resolution` |
|---|---|---|---|
| numa-shim-mock (ubuntu) | `97339434380` | 7 | 5 |
| numa-shim-windows | `97339434414` | 7 (9 with vmem) | 5 |
| numa-shim-macos | `97339434453` | 7 | 5 |
| numa-shim-macos-miri | `97339434523` | 7 | 5 |

— versus 0 and 0 on the same jobs' non-mock rows. The Windows job's own comment is honest
about the asymmetry ("the Linux job is the primary mock-coverage home"). I flag it anyway
because **`numa-shim-macos-miri:2827` is the only place in this workflow where miri and
the mock backend intersect**, so a typo confined to that row is caught by nothing.

### T9 (P3) — the root-crate mock row proves one of six files while its comment claims the per-file pattern

`ci.yml:1244-1247` greps a single sentinel, `cached_node_invalidates_across_slot_recycle`,
from `tests/numa_cache_invalidation.rs`. Its comment (`:1242`) says "The grep sentinel
(task #1101 pattern) proves the mock **files** actually RAN", and the sibling numa-shim
row states the pattern as "one named test per FILE" (`:2660`). Five files are unproven:
`numa_periodic_refresh.rs`, `segment_directory_numa.rs`, `…_bucket_reuse.rs`,
`…_high_node_ids.rs`, `segment_directory_clear_bit_no_register.rs`.

Low risk today — all six share the `numa_shim_mock` conjunct, so the one sentinel does
prove the cfg took effect — but a per-file gate edit (e.g. someone adds a
`not(feature = "X")` conjunct to one of the five) would go unnoticed, which is the exact
scenario the per-file rule was written for. **Fix: four more `grep -F` lines, or soften
the comment to match what the row actually asserts.**

### T10 (INFO) — item 42's `RESOLVED.md` pointer headline reads as if the item were still open, and its verdict word describes the other half

`docs/correctness-open-items/RESOLVED.md:112` bolds: *"`numa-shim`'s `mock`
Cargo-feature-unification hazard **remains a Cargo feature (deliberately deferred)** — the
aligned-vmem half of this item is CLOSED"*. The numa-shim closure appears only in the
trailing prose after the boilerplate.

This is **not** a mistake by `8f9bdf9`: `tests/no_stale_doc_references.rs:1465` requires
the pre-boilerplate headline to be byte-identical to the archive entry's first paragraph,
and the archive keeps the original wording per the history-is-not-rewritten convention.
Two consequences worth recording rather than fixing:

- the guard's whole-word verdict check is satisfied here by a `CLOSED` that refers to the
  **aligned-vmem** half, not the numa-shim half that actually closed;
- CLAUDE.md's R34-24 "a closed item must NOT look active due to a stale header" and the
  byte-identity requirement pull in opposite directions for any item closed in halves.

No action requested. Flagged so a future reader does not "fix" one rule by breaking the
other.

### T11 (INFO) — the waiver's release-SHA placeholder is not in any #1262 site list

`docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md:19`:

> **Release/tag commit:** **[TO BE FILLED: task #1262's landing SHA]**. … whoever executes
> #1262 MUST come back and fill this in so the acceptance is bound to the exact published
> tree.

The instruction is correct and self-documenting. But I checked every #1262 site list in
the index (`TRACKED_publish_readiness.md:396`, `:408`, `:450`, `:484`): they enumerate the
crate manifest, the root pin, three README pins, the dated CHANGELOG header, the tag, and
E1's ordering rule — **not the waiver placeholder**. An unfilled `[TO BE FILLED: …]`
string in a shipped risk-acceptance record would materially weaken it (the whole point of
the record is that it is bound to a specific published tree). Fold it into #1262 now,
while it is visible.

### T12 (INFO) — no MSRV row compiles the mock arm

`ci.yml:1916-1917` adds `cargo check -p numa-shim` and
`cargo test -p numa-shim --no-run --all-features` on pinned 1.88, correctly closing E8's
actual ask (the default `cargo add numa-shim` configuration). Post-#1288, `--all-features`
cannot reach the mock, so `pub mod mock` and the four functions' mock arms are compiled by
**no** 1.88 row. Real risk is near zero — that module uses only `std::thread_local!`,
`RefCell`, `Vec` and a `const` — and it is test-support code, not shipped behaviour.
One optional row closes it: `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim --no-run`.

### T13 (INFO) — E9's residuals: one closed by an unrelated task, two remain and both freeze at publish

- **CLOSED:** `linux::dbg_node_resolution_for_cpu`'s R25-1 gating question. Task #1287
  (`99ecef4`) classified it as a pure observer in `tests/dbg_hook_safety_tripwire.rs` — a
  workspace-wide scanner that had turned `main` red on the campaign's push. Its commit
  body notes that **neither the tenth nor the eleventh review realised an automated
  tripwire enforces the rule they were both discussing**; both rated it "a convention
  point, not a defect". A method lesson for this campaign, not just a closed item.
- **OPEN:** `NodeResolution::Resolved(u32)` (`src/lib.rs:290`) carries no variant-level
  `#[non_exhaustive]` and no recorded decision either way, while both single-field
  `MockCall` variants document theirs at length (`:150-159`, `:169-174`). The enum-level
  `#[non_exhaustive]` (`:278`) does not cover the tuple field.
- **OPEN:** the Windows/macOS/miri/fallback `current_node_impl` and
  `current_node_resolution_impl` remain copy-paste parallels, so the published three-row
  mapping table (`:335-339`) is an unenforced claim rather than true by construction.

Both open residuals are frozen by the publish and are cheap to decide now.

### T14 (INFO) — `npm run check` contains no `numa-shim` row at all

`scripts/check-all.mjs` and `scripts/check-matrix.mjs` yield **zero** matches for `numa`.
Every numa-shim gate this campaign added is CI-only. That is consistent with the local
gate's documented "fast subset" scope and is moot now that CI is green on the landing SHA
— recorded only so nobody assumes a local pre-push run covers this crate. If a numa-shim
package-gates job is added (T7), the same asymmetry will apply to it, exactly as
`docs/correctness-open-items/RESOLVED.md`'s item 65 already recorded for `aligned-vmem`.

### T15 (INFO) — the single surviving `feature = "mock"` string in the tracked build tree

`.github/workflows/ci.yml:2649` quotes `node_resolution_linux.rs`'s gate as
`all(target_os = "linux", not(miri), not(feature = "mock"))`; the real gate is
`all(target_os = "linux", not(miri), not(numa_shim_mock))`
(`crates/numa-shim/tests/node_resolution_linux.rs:9`, `src/lib.rs:758`). The sentence's
verb is past tense ("*were* compiled by no CI job on ANY platform"), so it reads as a
historical citation and no reader is actively misled. Recorded because it is
grep-verifiably the **last** one: `grep -rn 'feature = "mock"'` over every tracked
`*.toml`/`*.rs`/`*.yml`/`*.mjs`/`*.json` returns exactly this one hit.

### T16 (INFO) — every #1262 checklist cites the root pin at a line number that moved

The cards and the eleventh review all say `root Cargo.toml:914`. #1288 grew the
`numa-aware-mock` comment block by ~19 lines, and the pin is now at **`Cargo.toml:933`**:

```
933:numa-shim = { path = "crates/numa-shim", version = "0.1", features = ["vmem-integration"], optional = true }
```

Purely mechanical, but it is the one site whose omission breaks the build outright (the
`version = "0.1"` requirement stops matching a `0.2.0` member), so the checklist should
point at the right place.

---

## 6. What I checked and found clean

Stated so this report does not read as if only defects were looked for.

- **C1 is genuinely discharged, verified two independent ways.**
  `git rev-parse HEAD` = `git rev-parse origin/main` = `1afed26e7776126b09c91a098a049e5d19f53b08`;
  `gh run view 32696488144` reports `headSha` equal to that SHA and
  `conclusion: success`; and a job-level enumeration returns 38 `success`, 3 `skipped`,
  0 `failure`, 0 `cancelled`. I did not take the run-level conclusion on trust, per the
  brief.
- **C3's row did what it exists to do.** Job `97339434380`, step
  `cargo test -p numa-shim --features vmem-integration`, log at 06:14:49:
  `test reserve_on_node_returns_valid_span ... ok`,
  `test reserve_on_node_large_align_round_trip ... ok`. The Linux
  `platform::reserve_on_node_impl` chain (`aligned_vmem::reserve_aligned` +
  `bind_range_impl_linux`, i.e. real `mbind(2)` on a real fresh `mmap`) executed.
- **The Linux job's sentinel greps executed and matched**, visible in the log as the
  echoed `grep -F` commands followed by
  `test current_node_records_scripted_value ... ok`,
  `test resolution_matches_current_node_resolved_zero ... ok`, and (on the vmem row)
  `test reserve_on_node_chains_and_records ... ok`. Not a green-and-dead row.
- **`node_resolution_linux.rs` runs, and only where it should.** 1 test on ubuntu's plain
  row; 0 under the mock cfg; 0 on Windows/macOS. Its host-independent oracle
  (`dbg_node_resolution_for_cpu(1_000_000)` → `FellBackToZero`) passed on a real
  single-node runner, confirming the eleventh review's paper analysis.
- **The mock conversion is complete in every buildable file.** Zero `feature = "mock"`,
  zero `features = ["mock"]`, zero `numa-shim/mock` across all tracked
  `*.toml`/`*.rs`/`*.yml`/`*.mjs`/`*.json`, except the one past-tense comment (T15).
- **The `unexpected_cfgs` declarations are proven, not assumed** — two `-D warnings`
  clippy rows run under the cfg and are green.
- **`#1291`'s fix is complete workspace-wide** — both `windows-latest` jobs audited step
  by step; `test-windows` was already using the `shell: bash` + `env:` shape throughout.
- **The bench is not silently skipped and not accidentally run.** Fail-loud panic under
  the wrong build; `harness = false` keeps `cargo test` from running it; no CI log shows a
  `numa_bench` binary.
- **`#1290`'s waiver is correctly scoped on all four axes** (Phase 1, Phase 3, policy,
  C1/E1) — §2.
- **`#1289` is consistent across all four surfaces** and changes no visibility — §2.
- **E3/E4/E5/E7 re-verified closed against source**, not against their commit messages:
  `src/lib.rs:141-148` now says `CurrentNode` records "the RAW pre-remap slot value" and
  `:354-368` explicitly says the resolution recording is "intentionally a DIFFERENT
  convention", correcting E3's "mirroring" claim by name; `README.md:113-126` documents
  `NodeResolution`/`current_node_resolution`; `README.md:59-60` uses an inline link and
  the file needs no reference definitions; `CHANGELOG.md:110-118` records both mock-surface
  additions.
- **Root wiring is coherent.** `numa-aware-mock` is a pure marker; six test files carry
  the four-conjunct gate; `alloc_core_reentrancy.rs` gained coverage rather than losing it;
  the root check-cfg entry exists.
- **`release.yml`'s two structural guards are in place** — dated-CHANGELOG (`:300-310`)
  and CI-green-for-this-exact-SHA (`:311+`) — so a red or absent CI run cannot reach
  `cargo publish`, and an un-consolidated `## Unreleased` heading cannot either.
- **Registry preconditions unchanged.** `crates/aligned-vmem/Cargo.toml:3` = `0.2.0`,
  matching numa-shim's `version = "0.2"` requirement; the eleventh review's crates.io
  confirmations for `aligned-vmem 0.2.0` and `bench-scale-tool 0.1.0` need no re-check.
- **No `cargo` invocation was run on this host**, so nothing in this report can have
  perturbed a sibling agent's `target/` or `Cargo.lock`.

---

## 7. Recommended order before publish

1. **T1** — fix the one published-rustdoc site (`src/lib.rs:319`). One clause. Do this
   first; it is the only item that would otherwise ship to docs.rs.
2. **T2, T3, T15** — the three remaining prose sites (`PHASE_NUMA_DESIGN.md:435-436`, a
   SUPERSEDED banner on `NUMA_TESTING_OPTIONS.md`'s Phase-1 section, and optionally
   `ci.yml:2649`). All one-line.
3. **T4** — correct the Phase-1 invocation in the two index cards (`:450`, `:484`) to
   `RUSTFLAGS="--cfg numa_shim_mock" cargo test -p numa-shim`; keep the "33 tests"
   expectation, which is confirmed correct. Leave the two historical gate-run reports
   alone.
4. **T5, T6, T11, T16** — one card-refresh commit: F5 → DECIDED/EXECUTED (#1289), C1 →
   DISCHARGED at `1afed26` with the run id, plus fold the waiver SHA placeholder and the
   corrected `Cargo.toml:933` pin line into #1262's site list.
5. **T7** — run `cargo publish --dry-run -p numa-shim` once (or dispatch `release.yml`
   with `dry-run` ticked). Adding the `numa-shim-gates` job is the better long-term fix
   and can follow the release.
6. **T13** — decide `NodeResolution::Resolved(u32)`'s `#[non_exhaustive]` question and
   record it; the publish freezes it.
7. **#1262 — the version bump**, with the complete site list: `crates/numa-shim/Cargo.toml:3`,
   root `Cargo.toml:933`, `README.md:33`/`:36`/`:64`, the dated CHANGELOG header
   (consolidating `## Unreleased` and resolving the now-empty `### Owner decisions
   pending` heading, both of whose bullets read DECISION MADE / DECIDED), the waiver's
   release-SHA placeholder, and the tag.
8. **F8 Phase 1 — re-run LAST**, on the final pre-tag revision, with the T4 invocation.
   Expect 33.
9. **Then publish.** Phases 2/4 are waived for 0.2.0 by #1290's record; Phase 3's in-guest
   remainder and the consumer-facing caveat are already stated in `CHANGELOG.md:15-34`.
10. **T8, T9, T12, T14** — the CI-hardening set. None blocks the release; all are one to
    four lines each and are best done in a follow-up so they do not churn the release
    commit.

---

**Summary verdict: CONDITIONAL GO.** All three of the eleventh review's conditions are
settled, two of them (C1, C3) with CI-log receipts rather than inference, and the wave's
largest change is provably a mechanical attribute rewrite with no semantic edit. **No P0,
no P1, no UB, no soundness hole, no correctness defect — and, for the first time in this
campaign, no functional regression introduced by the fixes either.** What stands between
this tree and a publish is one clause of published rustdoc that still names the Cargo
feature this release removes (T1), a dead command in the release's own last step (T4), a
packaging path that no gate has ever exercised (T7), and index bookkeeping that has now
gone stale four consecutive times (T5/T6). #1262 is the correct next step but not a
sufficient one until T1, T4, T11 and T16 are folded in ahead of it.
