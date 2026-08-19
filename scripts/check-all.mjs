// One command, every pre-push gate. Runs the exact checks CI runs (fmt,
// clippy across the full feature matrix, tests across the main feature
// combos) plus the deterministic iai judge, so a red CI run is caught here
// FIRST — not after a push.
//
// Why this exists: this session pushed 17 commits and only discovered CI
// was broken (rustfmt drift from the PERF-3 phases, two workflow jobs
// pointing at test files deleted in task #204) by watching the Actions run
// AFTER pushing. `npm run check` is the single command that should have
// caught all of that beforehand — run it before every push.
//
// Usage (from repo root):
//   node scripts/check-all.mjs
//   npm run check
//
// What it runs, in order (fails fast — stops at the first red step). Step
// numbers below are kept in sync with the actual runtime step list by hand;
// a numbering drift here (a doubled "7" between the last clippy row and the
// first `cargo test` row) was finding F5 of
// `docs/reviews/2026-08-05-hs-new-waves-release-readonly-review.md`, fixed
// in the same pass that made the runtime banner derive its row count from
// `clippyRows.length` (task #587/F9, commit `650b818`). Task
// #1157/F14: this file's own claim to "reproduce the 'aligned-vmem
// package gates' job" had drifted false — commit `3e8c3fe` (task #1141/L2)
// added a real-backend `--all-features --release --no-fail-fast` test row to
// that ci.yml job and this file gained no matching row, so every step number
// from the new row's insertion point onward shifted by +1 in this pass
// (steps 32-49 -> 33-50); every reference below was re-derived against the
// live array, not incremented by hand guesswork, per this file's own
// standing numbering-drift caution above:
//   0. node scripts/argv-roundtrip-test.mjs   (shell:false argv regression; R27-9)
//   1. cargo fmt --all -- --check           (rustfmt gate)
//   2-7. the 6 `clippy` rows from scripts/check-matrix.mjs's PER_PR_ROWS
//      (R30-5: GENERATED, not hand-written — default / experimental /
//      --all-features / hardened medium-classes internals / production /
//      production internals)
//   8. cargo test --features "production internals"                (default prod suite;
//      `internals` required — this repo's own tests/ suite reaches
//      alloc_core/global/registry directly, R34-3/task #522. There is no
//      bare `--features production` test step anywhere in this file —
//      `production` alone appears only as a clippy row above. Finding S9,
//      docs/reviews/2026-08-06-sprint-closing-readonly-review.md: this
//      line previously said "cargo test --features production", which
//      does not exist as a step here.)
//   9-11. cargo test x3 more feature combos (production alloc-stats
//      bench-internals internals, pinning, --all-features)
//   12-30. aligned-vmem package gates — 19 steps (2 cache-invalidation
//      cleans + 5 clippy rows + 3 cross-target unix rows (1 check + 2
//      clippy) + 1 mock clippy row + 5 test rows (2 real-backend debug +
//      1 mock debug + 1 mock-release, task #1086/L10 + 1 real-backend
//      release, task #1141/L2 via #1157/F14) + 2 doc rows
//      (--all-features plus the docs.rs feature set, task #1142) + 1
//      optional semver row). NOTE the position:
//      this group sits BEFORE the two
//      remaining PER_PR_ROWS rows in the runtime array, not after them
//      (task #1047 checked the order against the array after the header
//      had claimed the opposite since the group's introduction, commit
//      66b8508/task #1024). Reproduces the 'aligned-vmem package gates'
//      job from ci.yml. Task #1071 closed three holes in this group, all
//      three of which let the 2026-08-17 push reach CI red despite a
//      green local gate: (1) FEATURE COVERAGE — the cross-target rows ran
//      one hardcoded set with `huge-pages` always on, so the
//      bench-internals-without-huge-pages unix shape (task #1070's
//      Breakage A: imports whose only use sites need huge-pages or
//      32-bit) was unreachable by construction; there is now a
//      bench-internals-ONLY cross clippy row — the minimal lattice
//      addition, because the crate's only compound feature gates are
//      bench-internals x (huge-pages | 32-bit) (the breakage just fixed)
//      and fault-injection x not(lazy-commit) (an explicit in-code
//      allow(dead_code) in src/fault_injection.rs, not a lint surface),
//      so {bench-internals alone, full set} reaches every pairwise
//      feature interaction compilable on x86_64-linux. (2) MOCK — the
//      `--cfg aligned_vmem_mock` arm was excluded "to keep the local
//      check fast" with no measurement behind it; the mock clippy row is
//      now included (~2.5 s after invalidation, measured — see the row's
//      own comment), and since task #1082 the mock TEST row is included
//      too (measured ~3.8 s after invalidation / ~1.5 s warm — see the
//      row's own comment): task #1071 had kept it out because it FAILED
//      on a clean tree at HEAD, task #1072 resolved that blocker
//      (profile-gated silent-skip tests; the eager `decommit`'s rustdoc
//      and README split), and task #1082 closed the dangling follow-up
//      that #1072's commit body had left recorded only inside closed
//      correctness item 51. (3) CACHE REPLAY
//      — a cargo step's exit-0 could mean "cargo found a fresh cache",
//      not "the lints ran" (the failing push's gate log shows the
//      cross-target clippy row printing only `Finished ... 0.74s` before
//      its OK); the group is now preceded by two `cargo clean -p
//      aligned-vmem` steps — the HOST form plus a --target
//      x86_64-unknown-linux-gnu form, because the host form provably does
//      NOT invalidate the cross-target dir — and every cargo row in the
//      group carries `expectWork: 'aligned-vmem'`, which FAILS the step
//      unless its own output shows a real Checking/Compiling/Documenting
//      line (see the run-loop comment). STILL excluded (CI covers it):
//      the i686 cross-targets and the RUSTFLAGS `--cfg miri` check row
//      (item 51/task #1059 history). Rows: clippy (default,
//      huge-pages, bench-internals,
//      lazy-commit+huge-pages+fault-injection+bench-internals,
//      --all-features); cross-target check + clippy
//      (x86_64-unknown-linux-gnu,
//      lazy-commit+huge-pages+fault-injection+bench-internals) and
//      clippy (x86_64-unknown-linux-gnu, bench-internals only); clippy
//      (--cfg aligned_vmem_mock via RUSTFLAGS, full feature set); test
//      (default, --all-features, mock via RUSTFLAGS — expectTest-asserted
//      since task #1101 — and since task #1086 mock via RUSTFLAGS
//      --release --test reservation_decommit_contract — and since
//      task #1157/F14 the real-backend --all-features --release
//      --no-fail-fast row ci.yml gained in task #1141/L2, commit `3e8c3fe`);
//      doc (--all-features, warnings-as-errors); semver-checks (optional,
//      skipped if cargo-semver-checks not installed).
//   31-32. the 2 remaining (non-clippy) PER_PR_ROWS rows — `cargo check --bench
//      perf_gate_iai --features "production bench-internals"` (R30-5:
//      scripts/iai.mjs's own DEFAULT_FEATURES and npm run check's own final
//      step — the exact command R29-16's 4x E0433 broke, now an
//      independent standalone check of its own), plus the internals-boundary
//      test (R34 review F1: runs r34_3_internals_boundary_api.rs WITHOUT
//      `internals` so the guard is non-vacuous)
//   33. node scripts/verify-internals-negative-boundary.mjs   (Sol-F1,
//      task #563, release-readiness review finding F1: the REAL compile-fail
//      oracle for the negative half of the `internals` boundary —
//      `AllocCore::dbg_carve_batch` must NOT compile without `internals` and
//      MUST compile with it; see that script's own header)
//   34. node scripts/verify-alloc-core-dbg-internals-exhaustive.mjs   (H2,
//      task #572, Sol-remediation review finding H2: the EXHAUSTIVE
//      structural complement to 33 — 33 only proves ONE method is gated;
//      this enumerates and checks EVERY `AllocCore::dbg_*` method across
//      `src/alloc_core/*.rs`; see that script's own header)
//   35. node scripts/verify-perf-gate-stubs.mjs   (R30-5: generated "feature
//      ABSENT" stub check for benches/perf_gate_iai.rs's library_benchmark_group!)
//   36. node scripts/verify-gate-report.mjs   (R31-5a: structural checks over
//      every docs/perf/R*_*.md gate report — companion CSV exists, valid
//      40-hex SHA/no placeholder, cited raw logs exist)
//   37. node scripts/verify-commit-prefixes.mjs   (R31-5c: commit-prefix
//      lint for CLAUDE.md's R30-12 perf(runtime)/perf(opt-in)/bench/
//      docs(config) taxonomy, local default range — see that script's own
//      header; the precise PR-scoped complement runs as ci.yml's
//      `commit-prefix-lint` job)
//   38. node scripts/vmem-doc-drift-guard.mjs   (R6, task #871; the guard
//      W5/task #854 asked for two rounds ago): guard against the doc-comment
//      drift class that has recurred 5 times (unconditional "over-reserve
//      / trim" statements without qualifying context). Heuristic: every
//      SENTENCE mentioning "over-reserv"/"trim" must contain a scope word
//      (if/when/fast-path/<=/>=/etc) in that same sentence; "unconditional"
//      is an outright failure regardless. See scripts/vmem-doc-drift-guard.mjs's
//      header for the full history and the per-sentence rewrite (task #878/Q8).
//   39. node scripts/vmem-linux-android-pairing-guard.mjs   (task #1131:
//      the Linux/Android pairing guard, created in the M1/task-#1105 wave
//      as a NEW sibling of vmem-doc-drift-guard (38) and then hardened by
//      SEVEN tasks (#1105, #1114, #1121, #1122, #1128, #1129, #1130) while
//      wired into NO gate anywhere — the creating agent's report flagged
//      the missing wiring at creation time ("one <1s step ... needs
//      adding") and the note reached no index, the R22-3 class recurring;
//      see docs/CORRECTNESS_OPEN_ITEMS.md item 80. Until this step existed
//      the guard's "self-cleaning allowlist" / "debt stays visible" header
//      claims could not hold in practice — nothing ran the check. Also a
//      ci.yml check-matrix row (same task): this file is the LOCAL gate
//      only and is not invoked by CI, so the local step alone would leave
//      the guard gate-dead in every push that skips `npm run check`.
//      Pure text scan, no cargo; measured 0.289 s clean-tree. See the
//      script's own header for the rule, the 13-entry known-drift
//      allowlist, and the documented limitations.)
//   40. node scripts/verify-aligned-vmem-bench-internals-exhaustive.mjs   (task
//      #1067/F7: exhaustive completeness check that every
//      `pub fn ...() -> u64` counter accessor in
//      crates/aligned-vmem/src/bench_internals/ is CALLED by
//      tests/bench_internals_counters.rs — the hand-maintained list there
//      claims exhaustiveness and had silently drifted; see that script's
//      own header)
//   41. node scripts/stale-artifact-diagnosis.mjs --self-test   (task #1073:
//      prove the stale-cross-checkout-artifact output sniffer — imported
//      above and consulted on every failed step — still detects the literal
//      task #1073 panic signature and stays narrow on the three negative
//      fixtures; see that script's own header)
//   42. node scripts/verify-vmem-page-constant-call-sites.mjs   (task #1076:
//      static guard against compile-time PAGE constants in
//      page_size()-validated argument positions — the class that escaped
//      three times (#1067 mock.rs, #1074 bootstrap+siblings, #1075 smoke.rs
//      x4) and is invisible on this 4 KiB-page Windows host by construction;
//      runs its counterfactual fixture self-test first, then scans src/,
//      crates/, tests/, benches/, examples/. Task #1080 adds the production
//      provenance rule for src/: every scanned argument must fold, be an
//      approved rounding form (lazy_initial_commit / page_size /
//      align_up(.., page_size())), or resolve to one through parameter→
//      caller and let-binding chains — closing the "every src/ arg is a
//      variable, so nothing folds" blind spot that made the fold-only
//      guard silent on production; the static complement to the forced-page
//      runtime regression in tests/lazy_initial_commit_forced_page.rs.
//      See that script's own header for the scanned-position table,
//      suppressions, and blind spots.)
//   43. cargo test --features "production internals bench-internals
//      small-segment-lazy-commit" --test segment_state_reconciliation_oracle
//      (task #1087: the segment-state reconciliation oracle, LAZY arm on
//      the host's real page. Carries expectTest because that file
//      deliberately compiles ZERO tests in the (no-override, no-lazy)
//      combination — see the step's own comment. Sits BEFORE the
//      forced-page rows so the RUSTFLAGS flip below happens once and
//      stays.)
//   44. cargo test --features "production small-segment-lazy-commit internals"
//      --test lazy_initial_commit_forced_page   (task #1080: the forced-page
//      lazy-commit call-site regression under RUSTFLAGS
//      "--cfg aligned_vmem_page_size_override" — the task #962 cfg-flag
//      conversion pattern, mirroring the ci.yml `test-windows` row. Placed
//      at the END of the array steps (with steps 45-46 directly after it)
//      so the RUSTFLAGS change's cache invalidation is paid once at the
//      end, not between the existing test/clippy rows. This single feature
//      set covers BOTH lazy legs: `production` implies
//      `primordial-lazy-commit`, and the explicit
//      `small-segment-lazy-commit` adds the small-segment leg; since task
//      #1086 (M6) the row carries expectTest — the forced test's name must
//      appear in the output — so a typo'd or lost cfg fails the step
//      instead of passing green-and-dead.)
//   45. cargo test --features "production internals bench-internals"
//      --test decomp_hooks_forced_page   (task #1081: the forced-page
//      decomp-hook page-alignment regression, same
//      "--cfg aligned_vmem_page_size_override" RUSTFLAGS as step 44, so
//      the pair shares one invalidation boundary at the array's end.
//      Numbering-drift note (task #1082, kept verbatim as the historical
//      record of the general problem — see the F14/task #1157
//      note at the header's very top for the LATEST such shift): this
//      row's header entry was MISSING from task #1081's own commit — the
//      header jumped straight from the forced-page lazy-commit row
//      immediately above (this row's predecessor) to "npm run iai",
//      under-counting the step list by one until renumbered here. Every
//      step number in this header has shifted multiple times since
//      (task #1131's new step, task #1142's new doc row,
//      task #1157/F14's new release-test row are three
//      concrete examples) — see task #1137's inventory in this file's own
//      header for the general form of this problem. Carries expectTest
//      since task #1086/M6, same as step 44.)
//   46. cargo test --features "production internals bench-internals"
//      --test segment_state_reconciliation_oracle   (task #1087: the
//      reconciliation oracle's FORCED-page arm, same override RUSTFLAGS
//      and same feature set as step 45 so the pair shares one fingerprint
//      boundary; carries expectTest — without the cfg this feature set
//      compiles ZERO tests in that file and the row would pass green on
//      an empty binary.)
//   47. cargo test -p aligned-vmem --all-features --test page_size_override
//      under RUSTFLAGS "--cfg aligned_vmem_page_size_override"   (task
//      #1095/H1: the task #1085 real-page-floor test — the only test
//      pinning that publication blocker, and before this row it executed
//      in NO gate row anywhere: steps 44-46 build root-crate test
//      targets, which never compile the aligned-vmem package's own
//      integration tests, and the -p aligned-vmem rows in the group
//      above never set the cfg. Carries expectTest — the file is
//      cfg-gated at the top level, so a lost cfg means an empty green
//      binary, the steps-43-45 M6 class.)
//   48. cargo test -p aligned-vmem --all-features --test page_size_query_failure
//      under RUSTFLAGS "--cfg aligned_vmem_page_size_override"   (task
//      #1139: the failed-OS-page-size-query fail-CLOSED oracle. Same
//      top-level cfg gate and therefore the same green-and-dead hazard as
//      the row above, so it carries expectTest for the same reason.
//      Unlike the floor test it is NOT host-adaptive: it drives the
//      failure through the raw-query seam, so its discriminating
//      assertions run on a 4 KiB host too.)
//   49. node scripts/verify-ci-sentinels.mjs   (task #1150: static guard that every
//      `grep -F "test <NAME> ... ok"` postcondition in .github/workflows/ci.yml
//      stays byte-identical to what libtest actually prints for the named `fn`
//      — a `#[should_panic]`/`#[ignore]` attribute added to a sentinel-tracked
//      test with no matching ci.yml edit is exactly the task #1146 defect class
//      (commit 77efd3a: six sentinels stayed in the plain form after their
//      targets gained #[should_panic], failing two loom jobs on every push
//      despite every test in them passing). Pure text scan, no cargo
//      invocation — appended at the array's END so every step number above it
//      is unchanged; see the script's own header for the full design and its
//      documented does/does-NOT-cover split.)
//   50. npm run iai                                                (deterministic judge,
//      requires WSL + valgrind — see scripts/iai.mjs; skipped with a warning if
//      WSL is unavailable, since this is the one step that can't run on a bare
//      Windows/Linux CI runner without the WSL layer this repo's dev scripts use)
//
// This does NOT replace CI (CI additionally runs miri, loom, TSan, multi-arch,
// no_std, MSRV — see .github/workflows/ci.yml) — it is the FAST subset that
// catches the most common drift (fmt, clippy, the two main test combos, and
// an instruction-count regression) in a few minutes on the dev host, so most
// pushes never need a red CI run to discover a problem.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { REPO_ROOT, run } from './lib.mjs';
import { PER_PR_ROWS, rowToCargoArgs, rowLabel } from './check-matrix.mjs';
import { staleArtifactDiagnosis } from './stale-artifact-diagnosis.mjs';

// Task #1142: the feature set docs.rs ACTUALLY builds with, read out of
// `crates/aligned-vmem/Cargo.toml`'s `[package.metadata.docs.rs]` rather than
// hand-copied here. A second hand-maintained copy of that list is a drift
// surface, and this gate exists precisely because a doc build under the WRONG
// feature set proves nothing: `--all-features` (the row below this one, and
// until #1142 the only doc row anywhere) is the one set in which the crate's
// four `crate::bench_internals::*` intra-doc links resolve, because it turns
// `bench-internals` on — while docs.rs deliberately excludes it. Those four
// links were dead on the published docs and invisible to every gate.
// Throws (failing the whole run) rather than returning a default if the key
// cannot be read: a silently-empty feature list would make this row build the
// DEFAULT feature set and quietly re-open the same blind spot.
function docsRsFeatures() {
  const manifest = join(REPO_ROOT, 'crates', 'aligned-vmem', 'Cargo.toml');
  const text = readFileSync(manifest, 'utf8');
  const section = text.split('[package.metadata.docs.rs]')[1];
  if (section === undefined) {
    throw new Error(
      `[check-all] ${manifest} has no [package.metadata.docs.rs] section — ` +
        'the docs.rs doc row cannot know which features docs.rs builds with. ' +
        'If the section was intentionally removed, remove this row too.',
    );
  }
  const match = /^\s*features\s*=\s*\[([^\]]*)\]/m.exec(section);
  if (match === null) {
    throw new Error(
      `[check-all] [package.metadata.docs.rs] in ${manifest} has no ` +
        '`features = [...]` key; cannot derive the published doc feature set.',
    );
  }
  const features = match[1]
    .split(',')
    .map((f) => f.trim().replace(/^["']|["']$/g, ''))
    .filter((f) => f.length > 0);
  if (features.length === 0) {
    throw new Error(
      `[check-all] [package.metadata.docs.rs].features in ${manifest} is empty.`,
    );
  }
  return features.join(',');
}

// R30-5 (task #454): every row in `scripts/check-matrix.mjs`'s `PER_PR_ROWS`
// (the single source of truth also consumed by `scripts/run-check-matrix.mjs`,
// which `.github/workflows/ci.yml`'s `check-matrix` job runs) is generated
// into a step here — each row runs EXACTLY ONCE in this script, split into
// `clippyRows` (spliced in among the hand-written steps below, at the same
// position the 3 hardcoded clippy steps used to occupy, so the step order
// this script has always documented stays intuitive) and `otherRows` (the
// non-clippy rows — currently just the perf-gate `check` row — appended
// near the end, since there is no pre-existing hand-written step for them
// to slot in next to).
const clippyRows = PER_PR_ROWS.filter((r) => r.kind === 'clippy').map((row) => ({
  name: `[matrix] ${rowLabel(row)}`,
  cmd: 'cargo',
  args: rowToCargoArgs(row),
}));
const otherRows = PER_PR_ROWS.filter((r) => r.kind !== 'clippy').map((row) => ({
  name: `[matrix] ${rowLabel(row)}`,
  cmd: 'cargo',
  args: rowToCargoArgs(row),
}));

const steps = [
  {
    // R27-9 (task #427): tooling-correctness precondition for every later
    // step — they all spawn through the same run() in lib.mjs, so a
    // regression in run()'s shell:false argv preservation would corrupt
    // every multi-word --features value before cargo ever saw it. Runs in
    // milliseconds and fails fast. Wired in explicitly (not left as a
    // direct-invocation-only script) so it cannot silently rot — the same
    // failure mode tests/no_stale_loom_files.rs (R13-5) was created to catch.
    name: 'argv-roundtrip (shell:false regression test)',
    cmd: 'node',
    args: ['scripts/argv-roundtrip-test.mjs'],
  },
  {
    name: 'rustfmt',
    cmd: 'cargo',
    args: ['fmt', '--all', '--', '--check'],
  },
  // R30-5: the 6 PER_PR_ROWS clippy rows (default / experimental /
  // --all-features / hardened medium-classes internals / production /
  // production internals), generated — see the comment above `clippyRows`.
  // Byte-identical argv to the pre-R30-5 hand-written steps for the first
  // 3; `hardened medium-classes internals` and plain `production` were NEW
  // as of R30-5 (they previously ran only in CI's `clippy` job / not at
  // all, respectively); `production internals` was added later still
  // (R34-3's `internals` feature).
  ...clippyRows,
  {
    // R34-3 (task #522, finding B1): `internals` added — this repo's own
    // `tests/` suite reaches `alloc_core`/`global`/`registry` directly, and
    // that module path now requires the `internals` feature (additive over
    // `alloc-core`/`alloc-global`, NOT implied by `production`).
    name: 'test (--features "production internals")',
    cmd: 'cargo',
    args: ['test', '--features', 'production internals'],
  },
  // C2 (bug-hunt review 2026-07-09): kept in lockstep with the CI `test`
  // job's feature matrix (.github/workflows/ci.yml). These tiers carry
  // tests whose bodies are `#[cfg(feature = "...")]`-gated, so only a run
  // WITH the feature actually exercises them.
  {
    // R24-6 (task #384): `bench-internals` added so
    // `tests/class_aware_dirty_oom_latch.rs` (which now additionally gates on
    // that feature — see `Cargo.toml`'s `bench-internals` doc) keeps running
    // under this step instead of silently being skipped by its own
    // `#![cfg]`. `internals` added (R34-3/task #522) — same whole-suite
    // rationale as the row above.
    name: 'test (--features "production alloc-stats bench-internals internals")',
    cmd: 'cargo',
    args: ['test', '--features', 'production alloc-stats bench-internals internals'],
  },
  {
    name: 'test (--features pinning)',
    cmd: 'cargo',
    args: ['test', '--features', 'pinning'],
  },
  {
    name: 'test (--all-features)',
    cmd: 'cargo',
    args: ['test', '--all-features'],
  },
  // --- aligned-vmem package gates (mandatory local coverage) ---
  // Reproduces the 'aligned-vmem package gates' job from ci.yml.
  // INCLUDES the mock (--cfg aligned_vmem_mock) clippy row and is preceded
  // by two cache-invalidation cleans (task #1071 — the old "excludes mock
  // (RUSTFLAGS) ... to keep the local check fast" rule was an unmeasured
  // assumption, and the vacuous cache-replay OKs it also permitted are
  // what let the 2026-08-17 push ship red to CI; see the header's task
  // #1071 block for all three holes). Still excluded: the i686
  // cross-targets and the RUSTFLAGS `--cfg miri` check row (CI covers
  // both), and mock TEST execution (pre-existing local failure — see
  // header). The x86_64-unknown-linux-gnu cross-target stays (item
  // 51/task #1059): on a Windows host the unix cfg surface is invisible
  // to every host-target row — see the cross-target steps below.
  {
    // Task #1071, hole 3: force real work in every row below. A green
    // cargo step is NOT proof its lints ran — cargo's freshness check is
    // mtime-based, and on a tree whose bytes changed but whose mtimes did
    // not advance (observed live in the 2026-08-17 failing push and
    // reproduced deterministically in task #1071's counterfactual: a
    // restored-mtime edit of src/os/unix.rs makes the cross-target clippy
    // row print only `Finished ... 0.6s` and exit 0 while the file
    // contains a -D warnings error), the step replays the cache and the
    // gate records a vacuous OK. `cargo clean -p` is the invalidation
    // mechanism because `touch` was shown insufficient for the
    // cargo-test profile (task #1070). This HOST form invalidates host
    // clippy/check/test/doc artifacts including the RUSTFLAGS-mock
    // variants (verified: each profile prints a fresh
    // Checking/Compiling/Documenting line after it) but provably does NOT
    // touch the --target dir — hence the --target-scoped companion step
    // immediately below. Measured cost per run: the clean itself ~1.1-1.7
    // s, plus a once-per-run rebuild surcharge on the rows below (~2-2.5
    // s per clippy row, ~4 s per test row, ~13 s for the doc row) and
    // ~2.7 s of ripple into the two later [matrix] rows (task #1071
    // commit has the full table).
    name: 'clean (aligned-vmem host artifacts — cache invalidation for the rows below)',
    cmd: 'cargo',
    args: ['clean', '-p', 'aligned-vmem'],
  },
  {
    // Task #1071, hole 3: cross-target companion of the clean above — the
    // host form leaves everything under target/x86_64-unknown-linux-gnu
    // untouched (measured: the cross rows replay `Finished` in 0.69 s
    // with no Checking line after a host-only clean); only this scoped
    // form actually invalidates them. ~1.2 s per run.
    name: 'clean (aligned-vmem x86_64-unknown-linux-gnu artifacts — cross-target companion)',
    cmd: 'cargo',
    args: ['clean', '-p', 'aligned-vmem', '--target', 'x86_64-unknown-linux-gnu'],
  },
  {
    name: 'clippy (aligned-vmem default)',
    cmd: 'cargo',
    args: ['clippy', '-p', 'aligned-vmem', '--all-targets', '--', '-D', 'warnings'],
    expectWork: 'aligned-vmem',
  },
  {
    name: 'clippy (aligned-vmem --features "huge-pages")',
    cmd: 'cargo',
    args: ['clippy', '-p', 'aligned-vmem', '--features', 'huge-pages', '--all-targets', '--', '-D', 'warnings'],
    expectWork: 'aligned-vmem',
  },
  {
    name: 'clippy (aligned-vmem --features "bench-internals")',
    cmd: 'cargo',
    args: ['clippy', '-p', 'aligned-vmem', '--features', 'bench-internals', '--all-targets', '--', '-D', 'warnings'],
    expectWork: 'aligned-vmem',
  },
  {
    name: 'clippy (aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals")',
    cmd: 'cargo',
    args: ['clippy', '-p', 'aligned-vmem', '--features', 'lazy-commit huge-pages fault-injection bench-internals', '--all-targets', '--', '-D', 'warnings'],
    expectWork: 'aligned-vmem',
  },
  {
    name: 'clippy (aligned-vmem --all-features)',
    cmd: 'cargo',
    args: ['clippy', '-p', 'aligned-vmem', '--all-features', '--all-targets', '--', '-D', 'warnings'],
    expectWork: 'aligned-vmem',
  },
  {
    // Item 51 (task #1059): cross-target unix compile gate. On this Windows
    // host every #[cfg(unix)] / #[cfg(all(unix, not(miri)))] item in the
    // crate compiles to NOTHING under the host-target rows above — the gap
    // that let genuinely missing imports hide in src/os/unix.rs (cfg(unix))
    // and src/os/miri.rs (cfg(miri)) until task #1055's cross-checks caught
    // them (commit a4b8e50). This row closes the cfg(unix) half locally; the
    // cfg(miri) half stays with CI's RUSTFLAGS row (excluded locally along
    // with mock, same keep-it-fast rule). CI also covers the unix half
    // natively (aligned-vmem-gates runs on ubuntu-latest) — this row exists
    // so the LOCAL pre-push gate stops being blind to it. Feature set
    // matches the lazy-commit clippy row above / CI's native-linux row for
    // the same combination; --all-targets is load-bearing (without it,
    // tests/ is not built and unix-only tests go unchecked).
    // x86_64-unknown-linux-gnu is already installed via rustup — ~3-6 s per
    // cached run (measured task #1059).
    name: 'check (aligned-vmem --target x86_64-unknown-linux-gnu --features "lazy-commit huge-pages fault-injection bench-internals")',
    cmd: 'cargo',
    args: ['check', '-p', 'aligned-vmem', '--target', 'x86_64-unknown-linux-gnu', '--features', 'lazy-commit huge-pages fault-injection bench-internals', '--all-targets'],
    expectWork: 'aligned-vmem',
  },
  {
    // Item 51 (task #1059): clippy half of the cross-target unix gate above —
    // same rationale (unix cfg code is invisible to the host clippy rows;
    // linting it only on CI means drift lands one push late).
    name: 'clippy (aligned-vmem --target x86_64-unknown-linux-gnu --features "lazy-commit huge-pages fault-injection bench-internals")',
    cmd: 'cargo',
    args: ['clippy', '-p', 'aligned-vmem', '--target', 'x86_64-unknown-linux-gnu', '--features', 'lazy-commit huge-pages fault-injection bench-internals', '--all-targets', '--', '-D', 'warnings'],
    expectWork: 'aligned-vmem',
  },
  {
    // Task #1071, hole 1: the feature-lattice companion of the cross rows
    // above. The 2026-08-17 CI breakage (task #1070's Breakage A) lived in
    // a cfg shape those rows cannot reach: bench-internals ON with
    // huge-pages OFF on 64-bit unix, where UNIX_EXACT_RESERVE_*'s only
    // use sites vanish and the imports go unused under -D warnings. The
    // rows above always ran with huge-pages on, so this defect class was
    // unreachable by construction — the gate added to catch unix drift
    // could not catch the unix drift that actually shipped.
    // bench-internals ALONE is the minimal addition that closes the
    // lattice (see the header's task #1071 block for the full
    // justification). clippy, not check: the breakage class is a lint
    // (unused imports), which bare `cargo check` warns about but exits 0
    // on. Measured ~2.6 s after the clean steps / ~1.1 s warm (task
    // #1071 commit).
    name: 'clippy (aligned-vmem --target x86_64-unknown-linux-gnu --features "bench-internals")',
    cmd: 'cargo',
    args: ['clippy', '-p', 'aligned-vmem', '--target', 'x86_64-unknown-linux-gnu', '--features', 'bench-internals', '--all-targets', '--', '-D', 'warnings'],
    expectWork: 'aligned-vmem',
  },
  {
    // Task #1071, hole 2: the mock arm. Excluded since the group's
    // introduction "to keep the local check fast" — a decision made
    // without a measurement, and the reason task #1070's Breakage B (the
    // whole --cfg aligned_vmem_mock arm failing to compile: E0433 on a
    // bare mock:: path plus six unused imports) shipped through a green
    // local gate. Measured cost: ~2.5 s after the clean steps — the same
    // order as the 0.72 s/0.64 s warm cross-target rows that item
    // 51/task #1059 justified adding, so "for speed" is not a defensible
    // reason to keep the crate's mock backend unverifiable locally.
    // Mirrors CI's aligned-vmem-gates mock clippy row exactly (features +
    // --all-targets + -D warnings + the RUSTFLAGS cfg). The mock TEST row
    // that task #1071 excluded here (it failed on a clean tree at HEAD:
    // tests/mock.rs's decommit_silently_skips_contract_violating_offsets
    // vs api/decommit.rs's debug_assert) was added back by task #1082 —
    // task #1072 resolved that blocker and #1082 closed the follow-up
    // (see the mock test row's own comment).
    name: 'clippy (aligned-vmem --cfg aligned_vmem_mock --features "lazy-commit huge-pages fault-injection bench-internals")',
    cmd: 'cargo',
    args: ['clippy', '-p', 'aligned-vmem', '--features', 'lazy-commit huge-pages fault-injection bench-internals', '--all-targets', '--', '-D', 'warnings'],
    env: { RUSTFLAGS: '--cfg aligned_vmem_mock' },
    expectWork: 'aligned-vmem',
  },
  {
    name: 'test (aligned-vmem --all-features)',
    cmd: 'cargo',
    args: ['test', '-p', 'aligned-vmem', '--all-features'],
    expectWork: 'aligned-vmem',
  },
  {
    // Task #1082 (F9): the mock TEST row — the last mock-arm exclusion
    // left standing, closed on the merits. CI has run this exact row
    // (ci.yml aligned-vmem-gates) since task #962; locally task #1071
    // kept it out because it FAILED on a clean tree at HEAD, and task
    // #1072 resolved that blocker (profile-gated silent-skip tests;
    // eager `decommit` rustdoc/README split) but recorded "adding that
    // row remains the planned follow-up" only inside closed correctness
    // item 51's Update block — a flag living one storey below the index
    // this file's own convention exists to surface. Measured cost:
    // ~3.8 s after `cargo clean -p aligned-vmem` invalidation / ~1.5 s
    // warm (task #1082 commit) — the same order as the mock clippy row
    // (~2.5 s, #1071), so "for speed" is not a defensible exclusion
    // here either. This is the ONLY local row that EXECUTES the mock
    // backend: the mock clippy row compiles it without running it, and
    // every real-backend row compiles a different cfg arm entirely —
    // counterfactually verified (a one-line mock-recording regression
    // in api/reserve.rs turned this row red with 2 failed tests while
    // every other local row stayed green; see the task #1082 commit
    // body). Mirrors CI's row exactly (features + the RUSTFLAGS cfg).
    //
    // Task #1101: the row now also carries expectTest — an ARRAY of two
    // names, one per cfg-gated test FILE this full-suite row compiles
    // (tests/mock.rs and tests/mock_reentrancy.rs are BOTH
    // `#![cfg(aligned_vmem_mock)]`-gated at the top level). Before this,
    // the row's only postcondition was expectWork, which a typo'd cfg
    // still satisfies (the crate recompiles under any RUSTFLAGS — only
    // the mock files compile to ZERO tests): verified on this tree, the
    // real cfg runs 14 + 2 tests, the one-character typo
    // `--cfg aligned_vmem_mok` runs 0 + 0, both exit 0. ONE name PER
    // FILE is the right granularity because the two files share this
    // row's single RUSTFLAGS cfg (a lost/typo'd flag kills both gates,
    // so either name catches it) while per-FILE drift (a renamed test,
    // an edited #![cfg] gate, a deleted file) is independent — a single
    // sentinel would leave the other file silently droppable, the exact
    // #1095/H1 asymmetry this row closes for the mock debug arm.
    // reservation_decommit_contract.rs (the mock-release row's
    // expectTest above) is NOT file-level cfg-gated and needs no
    // sentinel here.
    name: 'test (aligned-vmem --cfg aligned_vmem_mock --features "lazy-commit huge-pages fault-injection bench-internals")',
    cmd: 'cargo',
    args: ['test', '-p', 'aligned-vmem', '--features', 'lazy-commit huge-pages fault-injection bench-internals'],
    env: { RUSTFLAGS: '--cfg aligned_vmem_mock' },
    expectWork: 'aligned-vmem',
    expectTest: [
      'records_reserve_and_decommit',
      'reentrancy_is_silently_dropped_with_outer_record_intact',
    ],
  },
  {
    // Task #1086 (L10): the mock RELEASE test row — the release half of
    // tests/reservation_decommit_contract.rs's decommit-contract oracle.
    // `method_records_nothing_for_a_violated_range_in_release_mock` is
    // gated `all(aligned_vmem_mock, not(debug_assertions))`, and before
    // this row NO row anywhere combined the mock cfg with --release: the
    // mock row above (debug) cannot compile it, and correctness item 72's
    // recorded plain-`--release` estimate covers a row that does not
    // activate the mock cfg — so the test executed nowhere. Scoped to the
    // one test target so the cost is a single release compile of the crate
    // plus one test binary: measured 16.0s cold / ~0.5s warm on the dev
    // host (task #1086). Deliberately carries `expectTest` (NOT
    // `expectWork`): libtest prints its per-test lines on every run
    // cached or not, but the Compiling line expectWork needs appears only
    // cold — and measured, `cargo clean -p aligned-vmem` (the group's
    // invalidation steps above) does NOT remove the release-profile
    // artifacts, so an expectWork here would false-RED on every warm run.
    // The expectTest assertion is what makes this row self-verifying: a
    // typo'd `--cfg aligned_vmem_mock` compiles the mock half OUT and
    // turns this row red instead of green-and-dead. Mirrors CI's
    // aligned-vmem-gates mock-release row (added in the same task).
    name: 'test (aligned-vmem --release --cfg aligned_vmem_mock --features "lazy-commit huge-pages fault-injection bench-internals" --test reservation_decommit_contract)',
    cmd: 'cargo',
    args: ['test', '--release', '-p', 'aligned-vmem', '--features', 'lazy-commit huge-pages fault-injection bench-internals', '--test', 'reservation_decommit_contract'],
    env: { RUSTFLAGS: '--cfg aligned_vmem_mock' },
    expectTest: 'method_records_nothing_for_a_violated_range_in_release_mock',
  },
  {
    // F14 (task #1157): the REAL-backend release test row. Commit
    // `3e8c3fe` (task #1141/L2) added `cargo test -p aligned-vmem
    // --all-features --release --no-fail-fast` as its own row in ci.yml's
    // 'aligned-vmem package gates' job (this file's header, above, already
    // claimed to "reproduce" that job) — before that commit, the ONLY
    // release-profile coverage anywhere (local or CI) was the mock-release
    // row immediately above, which exercises a single mock test target, not
    // the real Unix/Windows backends. This crate's contract tripwires are
    // `debug_assert!` — precisely the code whose behaviour DIFFERS between
    // profiles — and several public methods document a "silent no-op in
    // release" half that no debug row can ever reach, so a release gate
    // that only ever ran the mock arm left the real backends' release
    // behaviour completely unverified, locally or in CI, until #1141.
    // Unscoped (no `--test <target>`, matching ci.yml's row exactly) rather
    // than scoped to one target: the whole point of #1141 was release
    // coverage of the WHOLE real-backend suite, not one file.
    // `reserve_is_aligned_and_writable` exists in EVERY feature
    // configuration (same sentinel ci.yml's row asserts via `grep -F`), so
    // a feature change cannot silence this row either.
    name: 'test (aligned-vmem --all-features --release --no-fail-fast)',
    cmd: 'cargo',
    args: ['test', '-p', 'aligned-vmem', '--all-features', '--release', '--no-fail-fast'],
    expectTest: 'reserve_is_aligned_and_writable',
  },
  {
    // Item 64 (task #1030 follow-up): default-feature runtime test —
    // ci.yml's separate 'test workspace members' job runs this, and the local
    // gate should match it. The clippy row above validates compile-time, but
    // runtime tests (OOM, panic) are invisible to --all-targets alone.
    name: 'test (aligned-vmem default)',
    cmd: 'cargo',
    args: ['test', '-p', 'aligned-vmem'],
    expectWork: 'aligned-vmem',
  },
  {
    // Item 65 (task #1039): doc warnings check — matches the aligned-vmem-gates
    // job's first step. Uses process.env.RUSTDOCFLAGS rather than inline
    // environment syntax to avoid Windows shell quoting issues.
    name: 'doc (aligned-vmem --all-features, warnings-as-errors)',
    cmd: 'cargo',
    args: ['doc', '-p', 'aligned-vmem', '--all-features', '--no-deps'],
    env: { RUSTDOCFLAGS: '-D warnings' },
    expectWork: 'aligned-vmem',
  },
  {
    // Task #1142: the row above is NOT the configuration that ships. docs.rs
    // builds with `[package.metadata.docs.rs].features`, which is narrower
    // than `--all-features` — so a warning-free `--all-features` build says
    // nothing about the pages users actually read. This row mirrors ci.yml's
    // 'docs.rs feature-set doc build' step, deriving the list from Cargo.toml
    // (see `docsRsFeatures()` above) so the two cannot drift.
    name: `doc (aligned-vmem docs.rs feature set: ${docsRsFeatures()}, warnings-as-errors)`,
    cmd: 'cargo',
    args: [
      'doc',
      '-p',
      'aligned-vmem',
      '--no-deps',
      '--features',
      docsRsFeatures(),
    ],
    env: { RUSTDOCFLAGS: '-D warnings' },
    expectWork: 'aligned-vmem',
  },
  {
    // Item 65 (task #1039): semver check — matches the aligned-vmem-gates job's
    // third step. Skipped with a clear diagnostic if cargo-semver-checks is not
    // installed; this is NOT a silent error swallow (the check is explicitly
    // documented as optional when the tool is absent).
    name: 'semver-checks (aligned-vmem, optional)',
    cmd: 'node',
    args: ['scripts/aligned-vmem-semver-check-optional.mjs'],
  },
  // --- end aligned-vmem package gates ---
  // R30-5: the remaining (non-clippy) PER_PR_ROWS rows — currently
  // `check-perf-gate-iai-default` (`cargo check --bench perf_gate_iai
  // --features "production bench-internals"`, scripts/iai.mjs's own
  // DEFAULT_FEATURES and the exact command R29-16's 4x E0433 broke), plus
  // `test-internals-boundary-no-internals` (R34 review F1: the `internals`
  // boundary guard must run WITHOUT `internals` to be non-vacuous).
  ...otherRows,
  {
    // Sol-F1 (task #563, release-readiness review finding F1): the REAL
    // compile-fail oracle for the NEGATIVE half of the `internals` boundary
    // — `test-internals-boundary-no-internals` above only proves the
    // POSITIVE half (stable re-exports resolve without `internals`); this
    // step proves `AllocCore::dbg_carve_batch` genuinely does NOT compile
    // without `internals` (and DOES compile with it). See
    // scripts/verify-internals-negative-boundary.mjs's own header.
    name: 'verify-internals-negative-boundary (Sol-F1 compile-fail oracle)',
    cmd: 'node',
    args: ['scripts/verify-internals-negative-boundary.mjs'],
  },
  {
    // H2 (task #572, Sol-remediation review finding H2): the single-method
    // oracle immediately above (`verify-internals-negative-boundary.mjs`)
    // proved exactly ONE `AllocCore::dbg_*` method is `internals`-gated; it
    // does not prove the other 127. This step is the exhaustive structural
    // complement — enumerates EVERY `AllocCore::dbg_*` method across
    // `src/alloc_core/*.rs` and asserts each is gated or explicitly
    // allowlisted. See scripts/verify-alloc-core-dbg-internals-exhaustive.mjs's
    // own header for the full rationale (this is the exact gap that let 31
    // methods across 3 files stay reachable without `internals` for a full
    // remediation wave, undetected).
    name: 'verify-alloc-core-dbg-internals-exhaustive (H2 exhaustive gating check)',
    cmd: 'node',
    args: ['scripts/verify-alloc-core-dbg-internals-exhaustive.mjs'],
  },
  {
    // R30-5: generated "feature ABSENT" compile-check enumeration for every
    // conditionally-registered iai arm in benches/perf_gate_iai.rs — the
    // mechanical, automatic form of the stub rule R29-16 violated by
    // omission. See scripts/verify-perf-gate-stubs.mjs's header for the full
    // rationale.
    name: 'verify-perf-gate-stubs (generated feature-ABSENT check)',
    cmd: 'node',
    args: ['scripts/verify-perf-gate-stubs.mjs'],
  },
  {
    // R31-5a (task #480): structural checks over every docs/perf/R*_*.md gate
    // report — companion summary CSV cited actually exists, its commit/SHA
    // field(s) are a real 40-hex SHA (not a prose placeholder, the exact
    // defect class R30-6's placeholder and R29-13's invalid 63-char hash
    // both were), and every cited `_raw_*.log` filename actually exists on
    // disk. Zero cargo invocations, pure text/regex scan — see
    // scripts/verify-gate-report.mjs's header for the full rationale and its
    // documented, individually-verified retroactive-exemption list for
    // reports predating the relevant CLAUDE.md rule.
    name: 'verify-gate-report (structural gate-report checks)',
    cmd: 'node',
    args: ['scripts/verify-gate-report.mjs'],
  },
  {
    // R31-5c (task #482): commit-prefix lint for CLAUDE.md's R30-12
    // taxonomy (perf(runtime)/perf(opt-in)/bench/docs(config)) — see
    // scripts/verify-commit-prefixes.mjs's header for the full rule. Local
    // default range is `@{u}..HEAD` (or the last 40 commits if no upstream
    // is configured) — a reasonable local approximation of "this session's
    // unpushed work"; the precise PR-scoped complement
    // (`base.sha..head.sha`) runs as its own `commit-prefix-lint` CI job
    // (needs `fetch-depth: 0`, which this project's other jobs deliberately
    // don't pay for — see that job's own comment in
    // .github/workflows/ci.yml). Pure `git log`/`git show --stat` text
    // scanning, zero cargo invocations — runs in well under a second.
    name: 'verify-commit-prefixes (R30-12 taxonomy lint, local default range)',
    cmd: 'node',
    args: ['scripts/verify-commit-prefixes.mjs'],
  },
  {
    // R6 (task #871; the guard W5/task #854 asked for two rounds ago):
    // guard against the doc-comment drift class that has recurred 5 times
    // (unconditional "over-reserve size + align" / "trim" statements without
    // qualifying context). Heuristic: every SENTENCE mentioning
    // "over-reserv"/"trim" must contain a scope word (if/when/fast-path/
    // <=/>=/etc) in that same sentence; "unconditional" is an outright
    // failure regardless. See scripts/vmem-doc-drift-guard.mjs's header for
    // the full history and the per-sentence rewrite (task #878/Q8).
    name: 'vmem-doc-drift-guard (unconditional over-reserve/trim detection)',
    cmd: 'node',
    args: ['scripts/vmem-doc-drift-guard.mjs'],
  },
  {
    // Task #1131: the Linux/Android pairing guard — created in the M1/
    // task-#1105 wave as a NEW sibling of vmem-doc-drift-guard above, then
    // edited by SEVEN tasks (#1105, #1114, #1121, #1122, #1128, #1129,
    // #1130) while wired into NO gate anywhere: the creating agent's own
    // report flagged the missing wiring at creation time ("one <1s step
    // needs adding"), but the note lived only in a report body and reached
    // no index — the R22-3 class. Until this step existed, the guard's
    // "self-cleaning allowlist" / "debt stays visible" header claims could
    // not hold: nothing ran the check, so stale entries rotted silently and
    // the printed 13-entry debt list was invisible. Pure text scan, no
    // cargo — measured 0.289 s on the clean tree. CI coverage: ci.yml's
    // check-matrix job runs the same script (added in the same task) —
    // this file is NOT invoked by CI, so the local step alone would have
    // left the guard gate-dead in every push that skips `npm run check`.
    // See scripts/vmem-linux-android-pairing-guard.mjs's header for the
    // rule and its documented limitations.
    name: 'vmem-linux-android-pairing-guard (Linux-only phrasing of Linux/Android-pair-gated mechanisms)',
    cmd: 'node',
    args: ['scripts/vmem-linux-android-pairing-guard.mjs'],
  },
  {
    // task #1067 (F7): the accessor-existence test
    // (crates/aligned-vmem/tests/bench_internals_counters.rs) imports each
    // accessor by name — an EXISTENCE check with no completeness direction.
    // R7-3 added windows_large_page_plain_fallback_successes without the
    // test gaining it, and nothing noticed across several rounds of gate
    // runs. This step enumerates EVERY `pub fn ...() -> u64` accessor in
    // crates/aligned-vmem/src/bench_internals/ and requires each to be
    // CALLED by that test — see the script's own header.
    name: 'verify-aligned-vmem-bench-internals-exhaustive (vmem accessor completeness)',
    cmd: 'node',
    args: ['scripts/verify-aligned-vmem-bench-internals-exhaustive.mjs'],
  },
  {
    // Task #1073: self-test for the stale-artifact output sniffer above —
    // feeds it the literal observed failing output (the task #1073
    // `read scripts/check-matrix.mjs: Os { code: 3, kind: NotFound }`
    // panic) plus three negative fixtures, and fails unless all four
    // expectations hold. Wired in as a step (not left as a
    // direct-invocation-only script) for the same reason
    // scripts/argv-roundtrip-test.mjs is: a detector that has never fired
    // is not proven, and an unwired detector rots silently. Milliseconds.
    name: 'stale-artifact-diagnosis (task #1073 self-test)',
    cmd: 'node',
    args: ['scripts/stale-artifact-diagnosis.mjs', '--self-test'],
  },
  {
    // Task #1076: static guard against passing the compile-time 4 KiB `PAGE`
    // constant (or any non-64 KiB multiple) into argument positions that
    // aligned-vmem validates against the RUNTIME page_size() — the class that
    // escaped three times in one campaign (task #1067 tests/mock.rs, task
    // #1074 bootstrap.rs + 5 siblings in production code, task #1075 smoke.rs
    // x4) and that NO local test can catch on this 4 KiB-page Windows host,
    // where PAGE == page_size() by construction. Task #1080 adds the
    // production provenance rule for src/: every scanned argument must fold,
    // be an approved rounding form (lazy_initial_commit / page_size /
    // align_up(.., page_size())), or resolve to one through parameter→caller
    // and let-binding chains — the static complement to the forced-page
    // runtime regression in tests/lazy_initial_commit_forced_page.rs. Runs
    // the fixture self-test (the permanent counterfactual: must-flag and
    // must-pass shapes) before scanning the tree. See the script's own
    // header.
    name: 'verify-vmem-page-constant-call-sites (task #1076 page-constant + task #1080 production provenance guard)',
    cmd: 'node',
    args: ['scripts/verify-vmem-page-constant-call-sites.mjs'],
  },
  {
    // Task #1087: the segment-state reconciliation oracle, LAZY arm —
    // drives the REAL dbg_force_decommit_retain_for retain leg and the
    // dbg_segment_state_reconciliation committed-bytes accounting on the
    // host's real page under `small-segment-lazy-commit` (the task #1081
    // meta_bytes fix's runtime oracle; see
    // tests/segment_state_reconciliation_oracle.rs). expectTest is
    // load-bearing: this feature combination compiles EXACTLY ONE test,
    // and the oracle file is deliberately built so the (no-override,
    // no-lazy) combination compiles ZERO tests — so dropping
    // `small-segment-lazy-commit` from this row's features (or any future
    // gate drift) makes the binary pass green with 0 tests unless the
    // test's name is required to appear. Placed BEFORE the two forced-page
    // rows below so the RUSTFLAGS flip into
    // `--cfg aligned_vmem_page_size_override` happens once and stays (the
    // fingerprint-boundary rationale those rows' own comments document).
    name: 'test (segment-state reconciliation oracle, lazy real page)',
    cmd: 'cargo',
    args: ['test', '--features', 'production internals bench-internals small-segment-lazy-commit', '--test', 'segment_state_reconciliation_oracle'],
    expectTest: 'retain_decommitted_retained_meta_bytes_oracle_lazy_real_page',
  },
  {
    // Task #1080: the forced-page lazy-commit call-site regression — the
    // RUNTIME complement to the static guard immediately above. Drives the
    // REAL bootstrap.rs/alloc_core_small.rs lazy-reservation call sites
    // under a simulated 64 KiB runtime page so the task #1074 compile-time-
    // PAGE bug class fails loudly on this 4 KiB-page host. The page-size
    // override is the test-only --cfg aligned_vmem_page_size_override build
    // flag (the task #962 conversion pattern — a Cargo feature would unify
    // across the dependency graph; a cfg flag is explicit per-build only),
    // passed via RUSTFLAGS exactly like the mock clippy row earlier in this
    // file. Deliberately among the LAST array steps, immediately before the
    // post-loop iai run: a RUSTFLAGS change invalidates cargo's fingerprint
    // cache, and paying that invalidation once at the end is cheaper than
    // re-running the existing test/clippy rows above under a different
    // RUSTFLAGS. The feature set covers BOTH lazy legs: `production` implies
    // `primordial-lazy-commit`, and the explicit `small-segment-lazy-commit`
    // adds the small-segment leg (the runtime module's cfg requires at least
    // one). Mirrors the ci.yml `test-windows` forced-page row.
    name: 'test (forced-page lazy-commit call-site regression, --cfg aligned_vmem_page_size_override)',
    cmd: 'cargo',
    args: ['test', '--features', 'production small-segment-lazy-commit internals', '--test', 'lazy_initial_commit_forced_page'],
    env: { RUSTFLAGS: '--cfg aligned_vmem_page_size_override' },
    // Task #1086 (M6): without this, a typo'd RUSTFLAGS cfg name compiles
    // the forced module OUT and this row still passes green on the file's
    // one ungated pure test (exit 0, 1 passed).
    expectTest: 'forced_page::lazy_initial_commit_call_sites_survive_a_forced_64k_page',
  },
  {
    // Task #1081: the decomp-hook forced-page regression — drives the REAL
    // R29-3 dbg_decomp_decommit_payload/dbg_decomp_recommit_payload hooks
    // (and the two F10b accessor hooks) under simulated 16/64 KiB runtime
    // pages, same override seam and same RUSTFLAGS as the forced-page row
    // above (so the two rows share the fingerprint invalidation). The F6
    // defect (payload start computed from the const small_meta_end(), 4
    // KiB-aligned only) is invisible on this 4 KiB-page host by construction
    // — forcing a larger page is the only way the hooks' boundary violation
    // fails loudly here. `production` implies alloc-decommit (the hooks'
    // gate); deliberately WITHOUT small-segment-lazy-commit so the reserved
    // segment is eager (fully committed), matching the hooks' documented
    // # Safety precondition. Mirrors the ci.yml test-windows forced-page row.
    name: 'test (forced-page decomp-hook page-alignment regression, --cfg aligned_vmem_page_size_override)',
    cmd: 'cargo',
    args: ['test', '--features', 'production internals bench-internals', '--test', 'decomp_hooks_forced_page'],
    env: { RUSTFLAGS: '--cfg aligned_vmem_page_size_override' },
    // Task #1086 (M6): same class as the row above — without this the row
    // passes green on the file's two ungated pure tests when the cfg is
    // lost (exit 0, 2 passed).
    expectTest: 'forced_page::decomp_hooks_decommit_recommit_page_aligned_under_forced_pages',
  },
  {
    // Task #1087: the segment-state reconciliation oracle, FORCED-page arm
    // — same `--cfg aligned_vmem_page_size_override` seam and the SAME
    // feature set as the decomp-hook row immediately above (so the pair
    // shares one fingerprint boundary), driving the eager-policy
    // reconciliation accounting under simulated 16/64 KiB pages. This arm
    // is the only non-vacuous eager-policy oracle for the meta_bytes fix
    // (on a 4 KiB host the tight and page-rounded boundaries coincide —
    // see the oracle file's own doc). expectTest is load-bearing for the
    // same reason as the two rows above, only more so: without the cfg
    // this feature set compiles ZERO tests in that file, so a lost cfg
    // would otherwise pass green on an empty binary (0 tests, exit 0).
    name: 'test (segment-state reconciliation oracle, forced pages, --cfg aligned_vmem_page_size_override)',
    cmd: 'cargo',
    args: ['test', '--features', 'production internals bench-internals', '--test', 'segment_state_reconciliation_oracle'],
    env: { RUSTFLAGS: '--cfg aligned_vmem_page_size_override' },
    expectTest: 'retain_decommitted_retained_meta_bytes_oracle_forced_pages',
  },
  {
    // Task #1095 (H1): the page-size-override FLOOR test — the only test
    // pinning the task #1085 fix (set_page_size_override must reject a
    // page below the host's real one; a declared publication blocker).
    // The whole file is gated `#![cfg(aligned_vmem_page_size_override)]`
    // at the top level, so a lost cfg compiles ZERO tests and the target
    // passes green-and-dead on an empty binary — the exact M6 class the
    // three rows above carry expectTest for. Why this row had to be NEW
    // rather than folded into them: those rows build ROOT-crate test
    // binaries (--test lazy_initial_commit_forced_page et al.) — RUSTFLAGS
    // reaches the aligned-vmem LIBRARY there, but cargo never builds the
    // aligned-vmem PACKAGE's own integration-test targets for them, and
    // every row that DOES build those targets (-p aligned-vmem
    // --all-features / mock / mock-release / default) runs without the
    // cfg — so before this row the test executed in NO row anywhere
    // (counterfactual on record: the plain command prints "running 0
    // tests", exit 0). expectTest only, not expectWork: libtest prints
    // its per-test line on every run cached or not, and unlike the
    // aligned-vmem group above no `cargo clean -p aligned-vmem`
    // invalidation precedes this tail-of-array row. Placed after step 45
    // so all four `--cfg aligned_vmem_page_size_override` rows share the
    // array tail; the fingerprint is separate from 43-45 anyway (the
    // aligned-vmem package's own --all-features test target, not a
    // root-crate target). Mirrors the ci.yml aligned-vmem-gates row added
    // in the same task. On this 4 KiB host the test is host-adaptive
    // (prints the real page, pins the boundaries; the discriminating
    // floor arm goes live only on >4 KiB hosts) — the row's job is to
    // keep the file's override-cfg compilation live in the mandatory
    // local gate everywhere.
    name: 'test (aligned-vmem page-size-override floor, --cfg aligned_vmem_page_size_override)',
    cmd: 'cargo',
    args: ['test', '-p', 'aligned-vmem', '--all-features', '--test', 'page_size_override'],
    env: { RUSTFLAGS: '--cfg aligned_vmem_page_size_override' },
    expectTest: 'override_floor_is_the_real_os_page_size',
  },
  {
    // Task #1139: the failed-OS-page-size-query oracle — the only test
    // pinning the fail-CLOSED poison (a failed query must NOT be folded to
    // 4 KiB and cached as if real; the fold would let the OS round a
    // decommit LENGTH up across live data on a 16/64 KiB-page host). Shares
    // the row above's `#![cfg(aligned_vmem_page_size_override)]` top-level
    // gate and therefore its green-and-dead hazard, so it carries
    // expectTest for the same reason: without the cfg the target compiles
    // ZERO tests and passes on an empty binary. A separate row rather than
    // an extra `--test` on the row above, mirroring the ci.yml pair added
    // in the same task. Unlike the floor test this one is NOT
    // host-adaptive — it drives the failure through the raw-query seam, so
    // its discriminating assertions run on every host including this 4 KiB
    // one (counterfactual on record: restoring the pre-fix fold makes it
    // fail at "try_page_size must report the failed query: 4096").
    name: 'test (aligned-vmem failed-page-size-query fail-closed oracle, --cfg aligned_vmem_page_size_override)',
    cmd: 'cargo',
    args: ['test', '-p', 'aligned-vmem', '--all-features', '--test', 'page_size_query_failure'],
    env: { RUSTFLAGS: '--cfg aligned_vmem_page_size_override' },
    expectTest: 'failed_os_page_size_query_fails_closed',
  },
  {
    // Task #1150: static guard over every `grep -F "test <NAME> ... ok"`
    // postcondition in .github/workflows/ci.yml — the sentinel string must
    // stay byte-identical to what libtest actually prints for the named
    // `fn`, which depends on that fn's real `#[should_panic]`/`#[ignore]`
    // attributes. Task #1146 (commit 77efd3a) found and fixed six sentinels
    // that named `#[should_panic]` tests in the plain "... ok" form — two
    // loom jobs went red on every push while every test in them passed —
    // and nothing before that task would have caught the drift
    // automatically; this step is that automatic signal, going forward.
    // Appended at the END of the array (not inserted mid-list) specifically
    // to minimise the step-numbering shift this file's own header documents
    // as unavoidable on every insertion (see the header's task #1137 note):
    // every step ABOVE this one keeps its existing number. Pure text scan
    // of ci.yml + tests/*.rs / crates/*/tests/*.rs, no cargo invocation —
    // see scripts/verify-ci-sentinels.mjs's own header for the full design
    // and its documented does/does-NOT-cover split (also recorded in
    // docs/CORRECTNESS_OPEN_ITEMS.md item 87).
    name: 'verify-ci-sentinels (ci.yml grep -F sentinel / #[should_panic] / #[ignore] consistency)',
    cmd: 'node',
    args: ['scripts/verify-ci-sentinels.mjs'],
  },
];

console.log(`[check-all] repo: ${REPO_ROOT}`);
console.log(`[check-all] running ${steps.length + 1} step(s) (argv-roundtrip, fmt, clippy x${clippyRows.length} [generated], test x8, aligned-vmem x21 [2 clean + 5 clippy + 3 cross-target unix (1 check + 2 clippy) + 1 mock clippy + 5 test (2 real-backend debug + 1 mock debug + 1 mock-release + 1 real-backend release, task #1157/F14) + 2 doc (--all-features + the docs.rs feature set, task #1142) + 1 optional semver + 2 override-cfg tests at the array tail (page-size-override floor, task #1095; failed-query fail-closed oracle, task #1139)], perf-gate check + internals-boundary test [generated], verify-internals-negative-boundary, verify-alloc-core-dbg-internals-exhaustive, verify-perf-gate-stubs, verify-gate-report, verify-commit-prefixes, vmem-doc-drift-guard, vmem-linux-android-pairing-guard, verify-aligned-vmem-bench-internals-exhaustive, stale-artifact-diagnosis [self-test], verify-vmem-page-constant-call-sites, verify-ci-sentinels [task #1150], iai) — fails fast\n`);

let allOk = true;
for (const step of steps) {
  console.log(`\n============================================================`);
  console.log(`  ${step.name}`);
  console.log(`============================================================`);
  // `step.env`, when present, is MERGED over the inherited environment rather
  // than replacing it — `spawn`'s `env` option replaces wholesale, which would
  // strip PATH and break every subsequent `cargo` lookup.
  //
  // This pass-through is load-bearing, not decoration (task #1047 review): the
  // step that needs it is `doc (aligned-vmem --all-features,
  // warnings-as-errors)`, whose ONLY teeth are `RUSTDOCFLAGS=-D warnings`.
  // The delegated version of that step declared `env` on the step object while
  // this loop still called `run(cmd, args, { cwd })` — so the variable never
  // reached rustdoc, doc warnings did not fail the build, and the step's own
  // name asserted a guarantee it did not provide. A step that cannot fail is
  // worse than no step, because it reads as coverage.
  const { code, out } = await run(step.cmd, step.args, {
    cwd: REPO_ROOT,
    ...(step.env ? { env: { ...process.env, ...step.env } } : {}),
  });
  if (code !== 0) {
    // Task #1073: if the failure output carries the stale cross-checkout
    // artifact signature (a test panicking with io::Error NotFound while
    // reading a repo file), print the real cause next to the failure —
    // advisory only, never a replacement for the step's own red.
    const staleArtifact = staleArtifactDiagnosis(out);
    if (staleArtifact) {
      console.log(`\n[check-all] DIAGNOSIS (task #1073): ${staleArtifact}`);
    }
    console.log(`\n[check-all] FAIL at step: ${step.name} (exit ${code})`);
    allOk = false;
    break;
  }
  // Task #1071, hole 3: `expectWork` turns "exit 0" into "exit 0 AND
  // provably did something". A step carrying `expectWork: '<crate>'` must
  // show a real Checking/Compiling/Documenting line for that crate in its
  // own output — the two `clean` steps ahead of the aligned-vmem group
  // guarantee such a line appears whenever the row actually runs, so its
  // absence means cargo replayed a cache the cleans were supposed to
  // invalidate (wrong target dir, changed `cargo clean` semantics, a
  // duplicate-fingerprint row inserted ahead, ...) and the step's OK is
  // vacuous. A vacuous green is worse than no step: it reads as coverage
  // (the failing push's gate printed `Finished ... 0.74s` + OK for a row
  // that had checked nothing).
  if (
    step.expectWork &&
    !new RegExp(`^\\s*(Checking|Compiling|Documenting) ${step.expectWork}\\b`, 'm').test(out)
  ) {
    console.log(
      `\n[check-all] FAIL at step: ${step.name} (exit 0 but no evidence of real ` +
        `work — expected a 'Checking|Compiling|Documenting ${step.expectWork}' line ` +
        `in the step output; cargo served a cached result, so the ` +
        `cache-invalidation steps ahead of the aligned-vmem group did not take ` +
        `effect. See the task #1071 hole-3 notes in this file's header.)`,
    );
    allOk = false;
    break;
  }
  // Task #1086 (M6): `expectTest` is expectWork's RUNTIME twin — a step
  // carrying `expectTest: '<full libtest name>'` must show that exact
  // `test <name> ... ok` line in its own output, turning "exit 0" into
  // "exit 0 AND the cfg-gated test provably compiled in and ran". Built
  // for the forced-page rows: their forced modules compile only under
  // `#[cfg(aligned_vmem_page_size_override)]`, so a typo'd RUSTFLAGS
  // `--cfg aligned_vmem_page_size_overide` (the exact scenario of task
  // #1086) makes the module vanish with NO warning and NO non-zero exit —
  // the binary still passes green on the file's ungated pure tests, and
  // the runtime half of the compile-time-PAGE defence is silently gone.
  // The test-name line is printed by libtest on EVERY run (cached or
  // not), so unlike expectWork this cannot false-RED on a warm cache. A
  // FAILING test never reaches this check (its non-zero exit fails the
  // step above); this check catches only the silent-absence case.
  //
  // Task #1101: `expectTest` may now be an ARRAY of names — every listed
  // name must appear — because one cargo row can compile SEVERAL
  // independently cfg-gated test FILES (the mock test row runs the whole
  // `-p aligned-vmem` suite, in which BOTH tests/mock.rs and
  // tests/mock_reentrancy.rs are `#![cfg(aligned_vmem_mock)]`-gated; a
  // single name would leave the second file's gate silently droppable).
  // A bare string is normalized to a one-element array, so every
  // pre-#1101 row's pass/fail behavior is unchanged.
  if (step.expectTest) {
    const expectedTestNames = Array.isArray(step.expectTest)
      ? step.expectTest
      : [step.expectTest];
    const missingTestNames = expectedTestNames.filter(
      (name) => !out.includes(`test ${name} ... ok`),
    );
    if (missingTestNames.length > 0) {
      console.log(
        `\n[check-all] FAIL at step: ${step.name} (exit 0 but the expected ` +
          `${missingTestNames.length === 1 ? 'test did not run' : 'tests did not all run'}: ` +
          `no ${missingTestNames.map((name) => `'test ${name} ... ok'`).join(' and no ')} ` +
          `line${missingTestNames.length === 1 ? '' : 's'} in the step ` +
          `output. For a forced-page or cfg-gated row this means the ` +
          `--cfg aligned_vmem_page_size_override / --cfg aligned_vmem_mock ` +
          `RUSTFLAGS was lost or typo'd, a required feature was dropped, or the ` +
          `test/module was renamed — the gated half compiled OUT (see task ` +
          `#1086/M6)`,
      );
      allOk = false;
      break;
    }
  }
  console.log(`\n[check-all] OK: ${step.name}`);
}

if (allOk) {
  console.log(`\n============================================================`);
  console.log(`  npm run iai (deterministic instruction-count judge)`);
  console.log(`============================================================`);
  const { code } = await run('node', ['scripts/iai.mjs'], { cwd: REPO_ROOT });
  if (code !== 0) {
    console.log(`\n[check-all] FAIL at step: iai (exit ${code}) — if this is "WSL not found" ` +
      `or similar environment failure (not a real regression), treat iai as a manual ` +
      `follow-up rather than blocking on it here.`);
    allOk = false;
  } else {
    console.log(`\n[check-all] OK: iai`);
  }
}

console.log(`\n============================================================`);
console.log(allOk ? '[check-all] ALL GREEN — safe to push' : '[check-all] FAILED — fix before pushing');
console.log(`============================================================`);
process.exit(allOk ? 0 : 1);
