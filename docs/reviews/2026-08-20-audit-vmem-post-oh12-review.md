# Independent review — post-OH12 wave, `a1554aa..fc42ea9` (task #1245)

Reviewer role: `@oh` — fresh, skeptical, no attachment to prior work. Range: the 16
commits `8972810, 1f8b366, 8114949, 2f65eb1, db248bb, 93f573f, 896342d, 1847c86,
a968577, 14bf368, 6f22c79, 7cd0f52, c36aad1, d4509bd, d50fda2, fc42ea9` (all
2026-08-20), i.e. everything after the fifth-audit filing `a1554aa` through the
`static SC` red-main fix. Reviewed on the tree at `fc42ea9` == `origin/main`.

**Verdict: the load-bearing technical work in this range is sound — every
mechanical claim I could re-derive either reproduced exactly or held up. What I
found wrong is concentrated in the prose layer: one incomplete fix (#1242 left the
stale `308`/`--dry-run` facts standing in the very file it edited), one false
exhaustive enumeration in `fc42ea9`'s own commit body, one imprecise summary in a
shipped ci.yml comment (#1242 again), and one leftover unjustified SERIAL lock that
#1241's own corrected criterion condemns but its edit did not remove. Plus two
false premises in this review's own task brief, reported below per this repo's
rule that a task's own false claims be named.**

---

## Findings, by severity

### F1 (medium) — #1242 corrected the stale `308` and `--dry-run` in CLAUDE.md but left the SAME stale facts standing in the same file, same job, ~35 lines above its own edit

`.github/workflows/ci.yml`'s `feature-powerset` job header still asserts, as
undated present-tense fact:

- ci.yml:2907-2909 — "against this crate's ~26 top-level Cargo.toml features
  resolves to **308** distinct `cargo check` invocations (confirmed via
  `cargo hack ... --dry-run` locally"
- ci.yml:2913 — "but 308 of them is still substantial per-PR cost"
- ci.yml:2487 (ASan job comment) — "governs `feature-powerset` (308 `cargo
  check` invocations)"

Measured now (cargo-hack 0.6.45, this host):

```text
$ cargo hack check --feature-powerset --depth 2 --no-dev-deps --print-command-list | grep -c '^cargo check'
365
$ cargo hack check -p aligned-vmem --dry-run
info: running `cargo check --dry-run` on aligned-vmem (1/1)
error: unexpected argument '--dry-run' found
```

Root top-level feature definitions number 29 today (`awk '/^\[features\]/{f=1;next} /^\[/{f=0} f && /^[a-z0-9-]+ =/{c++} END{print c}' Cargo.toml`), not ~26. So the header
carries: a count stale by 57 invocations, a feature count stale by 3, a combinatorial
formula `C(26,2)+26+1` keyed to the stale count, and a "confirmed via" method that
**cannot work on any current cargo-hack**.

The reason this is a finding rather than frozen history: task #1242 (commit
`d50fda2`) edited this exact job. Its own new comment block at ci.yml:2935-2965
states "`--dry-run`, named in this comment before task #1242, no longer exists in
current cargo-hack — it is forwarded to cargo, which rejects it" (ci.yml:2943-2946)
and its CLAUDE.md correction replaced 308 with "365 as of 2026-08-20 … re-derive …
rather than quoting either figure". The commit knew the number, knew the flag was
dead, cited both facts in its own body — and left three `308`s and a live-looking
`--dry-run` instruction ~35 lines above the edit, in the same file. #1240
(`d4509bd`) measured the 365 and corrected CLAUDE.md's line reference
("CLAUDE.md:405-411 does carry both stale facts the report names") without naming
the ci.yml copies either. This is the precise "#1236 class" — two passes over a
paragraph in one day, the false statement survives both — reproduced one level
over, inside the correcting commit itself.

### F2 (medium) — `fc42ea9`'s commit body carries a false exhaustive enumeration: "they touched only … and checkpoint files"

The #1244 body says of the 22 pushed commits (`5979275..d50fda2`): "None of the 22
pushed commits touched src/alloc_core/size_classes.rs at all (verified: they
touched only crates/aligned-vmem/, CLAUDE.md, docs/, .github/workflows/ci.yml,
scripts/, and checkpoint files)".

The load-bearing half is TRUE — I verified the 22 commits touched nothing under
root `src/`:

```text
$ git diff --name-only 5979275..d50fda2 -- 'src/'      # (empty)
$ git rev-list --count 5979275..d50fda2                 # 22
```

But the parenthetical "verified: only" list is FALSE in both directions:

- **Five real files are omitted**: `crates/numa-shim/CHANGELOG.md`,
  `crates/racy-ptr-cell/CHANGELOG.md`, `crates/size-classes/CHANGELOG.md`,
  `crates/tagged-index-stack/CHANGELOG.md` (all from `a968577`/#1220 — *inside my
  review range*) and `tests/no_stale_doc_references.rs` (from `8114949`/#1236,
  also in range). `git diff --name-only 5979275..d50fda2 | sed 's|/[^/]*$||' |
  sort -u` shows `crates/numa-shim`, `crates/racy-ptr-cell`, `crates/size-classes`,
  `crates/tagged-index-stack`, and `tests` as touched directories.
- **"checkpoint files" has zero members in the push**: `git diff --name-only
  5979275..d50fda2 | grep -ci checkpoint` → `0`. The session's checkpoint files
  were untracked scratch, never part of the 22 commits.

So the enumeration includes a category with no files and misses five. The
conclusion (toolchain drift, not a wave-caused break) stands — none of the missed
files is root `src/` — but the "verified:" prefix asserts an exhaustiveness the
command behind it did not have. This is the campaign's own most-reproduced defect
class (a false exhaustive-composition claim — cf. #1181/F6, which this repo has
already filed once before against a closing line of exactly this shape), in the
one commit of the range whose entire subject is diagnosis precision.

### F3 (low-medium) — ci.yml's new #1242 comment says "two wrongly-gated test files"; card 95, written by the same commit, establishes that one of the two is UNGATED

ci.yml:2961: "the root crate is born-red under --all-targets (two wrongly-gated
test files plus a deliberate compile-fail example tripwire)".

Card 95 (`docs/correctness-open-items/TRACKED_ci_gate_coverage.md`, item 95,
Evidence bullet 1) says of `tests/concurrent_stress.rs`: "with NO `#![cfg]` guard
anywhere in the file … This one is genuinely UNGATED — the fix shape is adding a
gate or adapting the test to `no_std`, not re-gating." I re-verified on the current
tree: `use sefer_alloc::SyncRegion;` sits at tests/concurrent_stress.rs:19 and
`grep -n '#!\[cfg' tests/concurrent_stress.rs` exits 1 (no guard).

The same commit therefore corrected #1240's "ungated `SeferAlloc` use" imprecision
in the card ("gated on the WRONG feature, not ungated") while introducing the
mirror-image imprecision ("wrongly-gated" for a file with no gate at all) in the
shipped ci.yml comment summarizing the same decision. A reader implementing the
preconditions from the ci.yml comment alone would go looking for a wrong gate in
`concurrent_stress.rs` and find nothing to re-gate.

### F4 (low) — #1241's corrected criterion leaves one unexplained, unjustified SERIAL lock standing, absent from the file's own census

`crates/aligned-vmem/tests/reservation_decommit_contract.rs` — the SERIAL doc
(lines 29-78, rewritten by #1241) states the criterion "takes the lock iff it
moves state some reader in THIS binary reads" and enumerates: the two
`huge_decommit_attempts` readers (locked, justified), the mock-log reader (lock
kept, explicitly documented as "harmless … not what makes that test sound"), the
two #1224 locks removed, the debug trio (unlocked), and the release-counter note.
`method_try_decommit_reports_violations_and_never_panics` (line 237, lock at
line 238) is **not mentioned anywhere in the census**, and by the census's own
criterion it needs no lock:

- its reservation is ordinary (`reserve_aligned`, so `is_huge()` is false and the
  `HUGE_DECOMMIT_ATTEMPTS` increment at `src/reservation.rs:918-919` is never
  reached — verified by reading `Reservation::try_decommit`,
  src/reservation.rs:902-925);
- its mock-log entries are thread-local (the very fact #1241 established);
- the file's own doc says "No test in this binary reads the
  munmap/VirtualFree-release counters" (line 75).

#1241's commit body lists it among "the 4 remaining locks" with no reason — the
only one of the four not given one. This is the same over-application class #1241
removed for the other two tests, one instance left, plus a census that is
incomplete against the file it documents (the mock-log reader's kept lock is
excused in-text; this one is simply absent). Harmless at runtime; it contradicts
the corrected criterion the same doc block asserts.

### F5 (report, per repo rule) — this review's own task brief contains two false premises

The task text says: "#1224 added `SERIAL.lock()` calls to `tests/mock.rs` and
`tests/mock_reentrancy.rs` reasoning the mock call log needed protection; #1241
removed those same two locks". Verified false on both halves:

- `git show 6f22c79 --stat` touches exactly three files:
  `tests/lazy_commit.rs`, `tests/reservation_decommit_contract.rs`,
  `tests/smoke.rs`. #1224 never touched `tests/mock.rs` or
  `tests/mock_reentrancy.rs` — its body *reported* them as suspicious and filed
  #1241. The two locks #1241 removed were in
  `tests/reservation_decommit_contract.rs`
  (`lazy_method_silently_skips_a_violated_range_on_every_profile`,
  `method_silently_skips_a_violated_range_in_release`), as #1241's own body
  correctly states.
- The brief also asks whether "#1224's OTHER lock additions (in `smoke.rs`,
  `reservation_decommit_contract.rs`, `huge_pages.rs`, `lazy_commit.rs`)" needed
  to stay — but #1224 added locks only in the first two. `huge_pages.rs` was
  untouched ("unchanged" in #1224's own table; `git show 6f22c79 --stat` confirms)
  and `lazy_commit.rs` received a comment correction only (its 9-line diff is one
  doc-comment reword; verified by reading the diff).

Noted because this repo's rule is that a task's own false claims be named, and
because the brief's framing would send a future auditor hunting for lock
additions in files that never had any.

---

## The seven requested checks — what was verified and how

### 1. Commit bodies vs actual diffs

All 16 bodies read in full. Spot-verified mechanical claims, with results:

| Commit (task) | Claim | Re-verified |
|---|---|---|
| 6f22c79 (#1224) | smoke.rs 13→46 locks; contract 4→6 | **exact**: `grep -c "SERIAL.lock()"` at `6f22c79^`/`6f22c79` → 13/46 (smoke), 4/6 (contract) |
| c36aad1 (#1241) | removed exactly the 2 #1224 locks; mock files doc-only | **exact**: contract now 4; `c36aad1`'s mock.rs/mock_reentrancy.rs deltas are `//!`-only |
| c36aad1 (#1241) | mock.rs 15 tests, 14 run in debug | **exact**: 15 `#[test]`; `decommit_release_silently_skips_contract_violating_offsets` is `#[cfg(not(debug_assertions))]` at mock.rs:374-375 |
| c36aad1 (#1241) | `bench_internals_counters.rs` single-test binary | **exact**: 1 `#[test]` |
| 8114949 (#1236) | nine headers byte-identical, hash `57c5a5e3c637be3807fe8de02fdc0542` | **exact hash reproduced**: `sed -n '17,49p' … md5sum` over all nine TRACKED files → 9 × `57c5a5e…` |
| 8114949 (#1236) | card counts agree; index "(9 cards)"→10 | holds today (see §4 below) |
| 1847c86 (#1219) | test passes under `--features "fault-injection lazy-commit"` | **run by me**: 3 tests, `refused_variant_…` ok (see §2) |
| 1f8b366 (#1235) | `git log -S 'base.add(end)'` → `1522d25` | **exact**; only surviving `add(end)` in src/ is the quote at `api/decommit.rs:50`; zero in `src/os/` |
| 2f65eb1 (#1237) | zero "out-of-bounds pointer" left | **exact** (`grep -rn … src/` → 0) |
| 8972810 (#1190) | four places record the owner's NO; `not yet answered` gone; `674d3b7` added no CHANGELOG entry | **all hold**: reservation.rs:1263 + 1306, CHANGELOG.md:333, item 90 (TRACKED_publish_readiness.md:300 names #1190); greps → 0; `git show 674d3b7 -- …/CHANGELOG.md` empty |
| a968577 (#1220) | four CHANGELOGs exist; numa-shim dated 2026-06-29 | files exist; `## 0.1.0 - 2026-06-29` + `## Unreleased` present (crates.io 200/404s NOT re-verified — no need) |
| 14bf368 (#1231) | historical E0425: gated static + ungated use at `1b72e73` | **read from `git show 1b72e73:…/decommit_capability.rs`**: `#[cfg(feature = "bench-internals")] static SERIAL` at head, bare `SERIAL.lock()` use ~line 1043; new CI step appears exactly once |
| db248bb/#1218, 93f573f/#1238 | guards green | **run by me**: `node scripts/verify-commit-prefixes.mjs 5979275..fc42ea9` → PASS, 1 grandfathered (`2f9d7b9`, reason line matches #1238's body); self-test 13/13; `node scripts/verify-ci-sentinels.mjs` → 47/47 |
| 7cd0f52 (#1232) | heartbeat properties | verified by reading `scripts/lib.mjs:42-166`: stdout-only (never `out`), TTY-gated with `SEFER_HEARTBEAT`/`opts.heartbeat` overrides, observable-stdio condition, interval cleared on BOTH `close` and `error` (lines 149-165), marker-swept wording; `dllInitFailedDiagnosis` exact-unsigned equality, no retry (lines 195-197). Behavior runs NOT re-executed. |
| d4509bd (#1240) | report exists; 14/14/365; CLAUDE.md:405-411 carried 308+`--dry-run` | report exists; counts re-measured (14/14/365 — see §6); `git show d50fda2^:CLAUDE.md` lines 405-411 carry both stale facts. The 11s→16s / 68s→339s timings were **not** re-measured (expensive; accepted as the report's measurements, clearly labelled MEASURED there). |
| fc42ea9 (#1244) | blame, CI colours, no-root-src touch, static SC | see §5 — all verified; the body's "touched only" enumeration is F2 above |

### 2. The #1219 fault-injection seam

- `arm_fail_next_decommit` is real and wired: `crates/aligned-vmem/src/fault_injection.rs:221-223`;
  `should_fail_decommit` (fault_injection.rs:237-248) has exactly ONE call site —
  `dispatch_try_decommit`'s `#[cfg(not(aligned_vmem_mock))]` branch,
  `crates/aligned-vmem/src/api/decommit.rs:400`, inside
  `#[cfg(feature = "fault-injection")]`. The injected
  `Err(VmemError::os_refusal_unknown_code())` flows through the existing
  `Err(e) => DecommitOutcome::Refused(e)` arm at decommit.rs:419, not an early
  return. Both fallible entry points route through `dispatch_try_decommit` (free
  `try_decommit` at decommit.rs:500; `Reservation::try_decommit` at
  reservation.rs:913/925 — the only two callers, per grep). The infallible
  `decommit`/`decommit_lazy` do not consult it, as documented.
- The test exercises both entry points with exact-`PartialEq` asserts plus the
  one-shot/real-backend coexistence assert
  (tests/decommit_outcome.rs:305-360). Ran the named command myself:

```text
$ cargo test -p aligned-vmem --features "fault-injection lazy-commit" --test decommit_outcome
running 3 tests
test advised_variant_is_produced_by_a_genuinely_accepted_decommit ... ok
test refused_variant_is_produced_by_the_fault_injection_seam_on_both_fallible_entry_points ... ok
test skipped_variant_is_produced_by_an_empty_range_on_the_free_function ... ok
test result: ok. 3 passed; 0 failed
```

3 tests — matching both the commit body and the module doc's row arithmetic
(default 2, `fault-injection` alone 3, `huge-pages` alone 3, both 4). The row
exists in CI (ci.yml:1725, `cargo test -p aligned-vmem --features
"fault-injection lazy-commit"`). The commit's counterfactual (neutered hook →
test fails) was NOT re-run by me — it requires editing `src/`, which this review
may not do; the seam's wiring makes the counterfactual's mechanism
(the assert observes `Advised` without the hook) self-evident.
- Not previously noted anywhere I could find, and worth recording: the `SERIAL`
  in `decommit_outcome.rs` (line 127) is *genuinely* required by the #1224 class —
  `FAIL_NEXT_DECOMMIT` is process-global (`static AtomicU32`,
  fault_injection.rs:193), so the `advised`/`skipped` tests' concurrent
  `try_decommit` calls could otherwise consume or be failed by the armed fault
  from test 4. #1219 got this right a few hours before #1224 generalized the
  class.

### 3. The SERIAL class, #1224 vs #1241

- **#1241 is correct on the current tree.** `crates/aligned-vmem/src/mock.rs:228-240`
  puts `CALLS`, `RESERVE_FAILS`, `COMMIT_FAILS`, `RECORDING` inside one
  `std::thread_local!`; `drain()` (line 258) and `reset()` (264-268) act on the
  calling thread only. libtest runs each `#[test]` on its own thread (including
  under `--test-threads=1`), so #1224's "mock log is process-global" premise —
  inherited from #1079's SERIAL doc — was false, and removing the two locks was
  correct.
- **Did #1224's other locks need to stay?** In `smoke.rs`, yes in substance: the
  readers there are the `bench-internals` process-global statics
  (`UNIX_MUNMAP_ATTEMPTS`, `bench_internals/unix.rs:128`;
  `WINDOWS_VIRTUALFREE_RELEASE_ATTEMPTS`, `bench_internals/windows.rs:112`;
  genuine `static AtomicU64`s), so movers in the same binary genuinely race the
  delta readers — the thread-local refutation does not reach them. #1224's
  documented trade-off (paying the serialization in rows without
  `bench-internals`, where the counters don't exist) is stated in SERIAL's doc and
  is a cost choice, not a soundness error. In `huge_pages.rs`/`lazy_commit.rs`
  there were no additions to assess (F5). The one genuine leftover is F4.

### 4. The open-items indexes, on this tree

Method: card bodies counted as `grep -c '^[0-9]\+[a-z]*\. \*\*'` per file (the
index's own census pattern).

- **Per-file counts: 9/9 agree.** Declared (index) vs actual: hook_safety 4/4,
  verification_coverage 5/5, platform_contracts 13/13, ci_gate_coverage 19/19,
  test_flakiness 4/4, correctness_residuals 4/4, publish_readiness 10/10,
  process_record 11/11, misc 2/2. Sum 72.
- **Both "must agree" command pairs, run verbatim from the file's fences
  (docs/CORRECTNESS_OPEN_ITEMS.md:287-290, 383-387):**

```text
grep -hE '^[0-9]+[a-z]?\. \*\*' docs/correctness-open-items/TRACKED_*.md | wc -l   → 72
grep -cE '^\| *[0-9]+[a-z]? *\|' docs/CORRECTNESS_OPEN_ITEMS.md                    → 72
grep -cE '^[0-9]+[a-z]*\. \*\*' …/ACTIVE.md                                        → 6
grep -hE '^[0-9]+[a-z]*\. \*\*' ACTIVE.md TRACKED_*.md | wc -l                     → 78  (6+72)
```

All agree — `d50fda2`'s "72 = 72" and "6 + 72 = 78" hold now.
- **Lookup table**: rather than 5 random items, I resolved ALL 72 rows
  mechanically: every `| N | file |` row matches a `^N\. \*\*` card in the named
  file, with 0 missing and 72 distinct item numbers across the TRACKED files — a
  bijection, stronger than the requested sample.
- **ACTIVE tier**: 6 cards, exactly (1, 2, 11, 13, 42, 62) as the census sentence
  enumerates.
- **Card 95** (TRACKED_ci_gate_coverage.md:306-314): follows the sibling
  Status / Current-number-or-verdict / Next trigger / Evidence structure
  (lines 308/309/310/311). Every static citation re-verified on the current tree:
  concurrent_stress.rs:19 (`use sefer_alloc::SyncRegion;`) with no `#![cfg]`
  (grep exit 1); the re-export `#[cfg(feature = "std")] pub use
  sefer_region::SyncRegion;` at src/lib.rs:386-387;
  regression_r4_3_teardown_trim.rs:90 `#![cfg(feature = "alloc-decommit")]` and
  :95 `use sefer_alloc::SeferAlloc;`; `#[cfg(feature = "alloc-global")] pub use
  global::{AllocStats, SeferAlloc};` at src/lib.rs:413-414;
  `alloc-decommit = ["alloc-core"]` (Cargo.toml:283) vs `alloc-global =
  ["alloc-core", "dep:tagged-index-stack"]` (Cargo.toml:187) — implication
  direction as stated; the example's `required-features = ["alloc-core"]`
  (Cargo.toml:2358-2359); `#[cfg(feature = "internals")]` above the impl block in
  `src/alloc_core/alloc_core_small_diag.rs:33`; `scripts/
  verify-internals-negative-boundary.mjs` exists. The twelve-features-`alloc-core`
  count re-derived: 10 single-line + 2 multi-line arrays
  (`primordial-lazy-commit`, `small-segment-lazy-commit`) = **12 features** ✓. The
  "365 invocations" figure re-derived = 365 ✓.

### 5. #1244's toolchain-drift diagnosis and the `static SC` conversion

Every element re-verified independently, not from the body:

- **Blame**: at `fc42ea9^`, line 181 is `const SC: SizeClassesImpl<TABLE_LEN,
  S2C_LEN> = SizeClassesImpl::build(PARAMS);` introduced by `121d6578`,
  **2026-07-17** — `git blame fc42ea9^ -L 179,183 -- src/alloc_core/size_classes.rs`.
  "Over a month before this session" ✓ (34 days).
- **CI colours** (`gh run list`): `5979275` CI **success** (the
  immediately-prior origin/main commit), `d50fda2` CI **failure** (the red),
  `fc42ea9` CI **success** (the fix green). Matches the body's narrative. The
  "~7 hours earlier" is 6h05m by commit timestamps (12:15→18:20) — immaterial
  rounding in the body, noted for completeness.
- **The 22-commit push touched no root `src/`** ✓ (F2 gives the commands and the
  body's enumeration error).
- **Toolchain**: local `cargo clippy --version` = 0.1.97 (rustc 1.97.0,
  2026-07-07); CI cites the rust-1.98.0 lint docs per the body. Consistent with
  drift; the repo has no rust-toolchain.toml (the body's own citation of
  CLAUDE.md's documented risk).
- **Soundness of `static SC`** (size_classes.rs:188): `SizeClassesImpl` derives
  only `Debug, Clone, Copy` (crates/size-classes/src/lib.rs:412), embeds
  `size2class` (line 455) — plain data, no interior mutability, no `Drop` →
  auto-`Sync`, so a `static` is sound. It is used at exactly three sites
  (size_classes.rs:216/226/234), all method calls inside `SizeClasses`'
  `const fn` forwarders; nothing takes `&SC`'s address into storage, nothing
  uses it in a const context (which would be a compile error, not silent), and
  `cargo check -p sefer-alloc` is green (run by me). The opposite-direction
  concern from `SIZE2CLASS` — a caller relying on per-reference duplication —
  would also be a compile-visible change; none exists.

### 6. #1242's ci.yml change

- Current step (ci.yml:2966): `cargo hack check --feature-powerset --depth 2 -p
  aligned-vmem --all-targets` — the `--no-dev-deps` → `--all-targets`
  **replacement** is what the file shows ✓; root step (ci.yml, `feature-powerset`
  job, first `run:`) still `--no-dev-deps` ✓.
- Flag-pair rejection re-run by me: `error: --no-dev-deps may not be used
  together with --all-targets` ✓.
- `crates/aligned-vmem/Cargo.toml`'s `[dependencies]` is literally empty ✓.
- Invocation counts re-measured: aligned-vmem `--all-targets` → 14; root
  `--no-dev-deps` → 365 ✓ (both match the commit and card 95).
- The root-step-leave-alone explanation exists but is imprecise — F3.

### 7. CLAUDE.md's cargo-hack note

CLAUDE.md:404-413 now reads "…resolves to a few hundred `cargo check` invocations
(308 at adoption, 365 as of 2026-08-20 — the count drifts upward as features
accrete, so re-derive it via `--print-command-list | grep -c '^cargo check'`
rather than quoting either figure, per this repo's no-hardcoded-counts
convention…)". **No rotting hardcoded count**: both figures are explicitly dated
snapshots with a re-derivation command and a do-not-quote instruction — the same
sanctioned pattern card 95's Current-number-or-verdict field uses. The
`--dry-run` mention is itself dated and declared dead in place. This is compliant.
The failure mode the question feared does exist — but in ci.yml, not CLAUDE.md
(F1).

---

## Near-miss recorded so it is not re-filed

The index (CORRECTNESS_OPEN_ITEMS.md:87-91) says the task-#1217 split's typed
"42" was stale because "running the census against the split commit itself yields
44", while #1236's body says "the census against the split commit gives 43, not
42". Both are correct — different referents. Measured: at `4f4d9f4` (the #1217
split, which typed "42") the census yields **44**; at `1b72e73` (the #1222
thematic re-split, which typed "42+") it yields **43**. Also re-confirmed: the
census today is 43 (`git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' |
wc -l`), so #1236's "43 at HEAD today" still holds. No defect.

Also pre-existing and already on the record (not re-filed): the census sentence's
"78-95, plus 85" is non-additive (85 ∈ 78-95) — observed and deliberately left at
#1239; and cards 46/49 are `**CLOSED**` stubs counted as open by both agreement
commands — pre-existing semantics, also recorded at #1239.

## Out-of-scope observations (reported, not fixed)

- **Item 90's closure condition is met but the card is OPEN** — its Status says
  "This card closes when #1190 is answered" and #1190 is answered (owner NO,
  2026-08-20). `8972810`'s body pre-documents the deliberate non-closure
  (sequencing assigned to the #1208-coordinated refresh, items 90/91/93/94).
  Consistent and self-explaining; flagging only because a round-start reader
  applying rule 1 literally will keep deferring a card whose condition is met.
- **The pre-existing `#[cfg]` on `SERIAL` in `tests/decommit_capability.rs`**
  (gated on `bench-internals`, uses gated per #1223/#1231) is outside my range;
  current tree compiles clean on the trap rows per #1224/#1241's sweeps, which I
  did not re-run in full (`cargo check -p aligned-vmem --all-targets
  --features=huge-pages` was verified transitively by the decommit_outcome build
  only — not independently re-run for every row; not verified by me for all rows).

## Commands actually run (vs read-only)

Run: the `decommit_outcome` test under `--features "fault-injection lazy-commit"`
(pass); `cargo check -p sefer-alloc` (default); `cargo test -p sefer-alloc
--features "production internals" --test no_stale_doc_references` (16 passed);
`node scripts/verify-commit-prefixes.mjs` (default range: PASS trivially, 0
commits; `5979275..fc42ea9`: PASS with the 2f9d7b9 grandfather) and its self-test
(13/13); `node scripts/verify-ci-sentinels.mjs` (47/47); the two flag-rejection probes (`--no-dev-deps`+`--all-targets`, `--dry-run`); two `--print-command-list` invocation counts (14 for aligned-vmem `--all-targets`, 365 for root `--no-dev-deps`); every census/grep/agreement
command quoted above; `git log/show/blame/grep/diff` archaeology; `gh run list`
for CI colours on `1b72e73/5979275/d50fda2/fc42ea9`.

Not run (stated as such where relied upon): `npm run check` (forbidden by the
brief); #1240's timing measurements (11s→16s, 68s→339s) and its
`--partition 1/10` red-root sampling (expensive; accepted as clearly-labelled
measurements in the report); #1219's and #1218's scratch-edit counterfactuals
(they require editing files this review may not touch); #1238's scratch-copy
guard-integrity probes; crates.io API checks for #1220 (unnecessary — the
in-tree artifacts and their dates verify); the historical-tree builds of #1231
(re-verified by reading `1b72e73`'s file content instead).

Read-only verification (no command): the mock `thread_local!` declaration and
its consequences; `dispatch_try_decommit`'s wiring; `Reservation::try_decommit`'s
counter reachability (F4's core argument); `scripts/lib.mjs`'s heartbeat
implementation; `SizeClassesImpl`'s derives; the #1240 report's internals; all
nine TRACKED headers; card 92 and card 95 in full.
