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
// `clippyRows.length` (task #587/F9, commit `650b818`):
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
//   12-26. aligned-vmem package gates — 15 steps (2 cache-invalidation
//      cleans + 5 clippy rows + 3 cross-target unix rows (1 check + 2
//      clippy) + 1 mock clippy row + 2 test rows + 1 doc row + 1 optional
//      semver row). NOTE the position: this group sits BEFORE the two
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
//      own comment), and the mock TEST row stays excluded because it
//      FAILS on a clean tree at HEAD (`decommit`'s debug_assert fires on
//      the contract-violating call that tests/mock.rs's
//      decommit_silently_skips_contract_violating_offsets expects to be
//      silently skipped — a pre-existing crate issue outside this gate
//      task's scope, recorded in the task #1071 commit). (3) CACHE REPLAY
//      — a cargo step's exit-0 could mean "cargo found a fresh cache",
//      not "the lints ran" (the failing push's gate log shows the
//      cross-target clippy row printing only `Finished ... 0.74s` before
//      its OK); the group is now preceded by two `cargo clean -p
//      aligned-vmem` steps — the HOST form plus a --target
//      x86_64-unknown-linux-gnu form, because the host form provably does
//      NOT invalidate the cross-target dir — and every cargo row in the
//      group carries `expectWork: 'aligned-vmem'`, which FAILS the step
//      unless its own output shows a real Checking/Compiling/Documenting
//      line (see the run-loop comment). STILL excluded (CI covers both):
//      the i686 cross-targets and the RUSTFLAGS `--cfg miri` check row
//      (item 51/task #1059 history), and mock-backend TEST execution
//      (see (2)). Rows: clippy (default, huge-pages, bench-internals,
//      lazy-commit+huge-pages+fault-injection+bench-internals,
//      --all-features); cross-target check + clippy
//      (x86_64-unknown-linux-gnu,
//      lazy-commit+huge-pages+fault-injection+bench-internals) and
//      clippy (x86_64-unknown-linux-gnu, bench-internals only); clippy
//      (--cfg aligned_vmem_mock via RUSTFLAGS, full feature set); test
//      (default, --all-features); doc (--all-features,
//      warnings-as-errors); semver-checks (optional, skipped if
//      cargo-semver-checks not installed).
//   27-28. the 2 remaining (non-clippy) PER_PR_ROWS rows — `cargo check --bench
//      perf_gate_iai --features "production bench-internals"` (R30-5:
//      scripts/iai.mjs's own DEFAULT_FEATURES and npm run check's own final
//      step — the exact command R29-16's 4x E0433 broke, now an
//      independent standalone check of its own), plus the internals-boundary
//      test (R34 review F1: runs r34_3_internals_boundary_api.rs WITHOUT
//      `internals` so the guard is non-vacuous)
//   29. node scripts/verify-internals-negative-boundary.mjs   (Sol-F1,
//      task #563, release-readiness review finding F1: the REAL compile-fail
//      oracle for the negative half of the `internals` boundary —
//      `AllocCore::dbg_carve_batch` must NOT compile without `internals` and
//      MUST compile with it; see that script's own header)
//   30. node scripts/verify-alloc-core-dbg-internals-exhaustive.mjs   (H2,
//      task #572, Sol-remediation review finding H2: the EXHAUSTIVE
//      structural complement to 25 — 25 only proves ONE method is gated;
//      this enumerates and checks EVERY `AllocCore::dbg_*` method across
//      `src/alloc_core/*.rs`; see that script's own header)
//   31. node scripts/verify-perf-gate-stubs.mjs   (R30-5: generated "feature
//      ABSENT" stub check for benches/perf_gate_iai.rs's library_benchmark_group!)
//   32. node scripts/verify-gate-report.mjs   (R31-5a: structural checks over
//      every docs/perf/R*_*.md gate report — companion CSV exists, valid
//      40-hex SHA/no placeholder, cited raw logs exist)
//   33. node scripts/verify-commit-prefixes.mjs   (R31-5c: commit-prefix
//      lint for CLAUDE.md's R30-12 perf(runtime)/perf(opt-in)/bench/
//      docs(config) taxonomy, local default range — see that script's own
//      header; the precise PR-scoped complement runs as ci.yml's
//      `commit-prefix-lint` job)
//   34. node scripts/vmem-doc-drift-guard.mjs   (R6, task #871; the guard
//      W5/task #854 asked for two rounds ago): guard against the doc-comment
//      drift class that has recurred 5 times (unconditional "over-reserve
//      / trim" statements without qualifying context). Heuristic: every
//      SENTENCE mentioning "over-reserv"/"trim" must contain a scope word
//      (if/when/fast-path/<=/>=/etc) in that same sentence; "unconditional"
//      is an outright failure regardless. See scripts/vmem-doc-drift-guard.mjs's
//      header for the full history and the per-sentence rewrite (task #878/Q8).
//   35. node scripts/verify-aligned-vmem-bench-internals-exhaustive.mjs   (task
//      #1067/F7: exhaustive completeness check that every
//      `pub fn ...() -> u64` counter accessor in
//      crates/aligned-vmem/src/bench_internals/ is CALLED by
//      tests/bench_internals_counters.rs — the hand-maintained list there
//      claims exhaustiveness and had silently drifted; see that script's
//      own header)
//   36. node scripts/stale-artifact-diagnosis.mjs --self-test   (task #1073:
//      prove the stale-cross-checkout-artifact output sniffer — imported
//      above and consulted on every failed step — still detects the literal
//      task #1073 panic signature and stays narrow on the three negative
//      fixtures; see that script's own header)
//   37. node scripts/verify-vmem-page-constant-call-sites.mjs   (task #1076:
//      static guard against compile-time PAGE constants in
//      page_size()-validated argument positions — the class that escaped
//      three times (#1067 mock.rs, #1074 bootstrap+siblings, #1075 smoke.rs
//      x4) and is invisible on this 4 KiB-page Windows host by construction;
//      runs its counterfactual fixture self-test first, then scans src/,
//      crates/, tests/, benches/, examples/. See that script's own header
//      for the scanned-position table, suppressions, and blind spots.)
//   38. npm run iai                                                (deterministic judge,
//      requires WSL + valgrind — see scripts/iai.mjs; skipped with a warning if
//      WSL is unavailable, since this is the one step that can't run on a bare
//      Windows/Linux CI runner without the WSL layer this repo's dev scripts use)
//
// This does NOT replace CI (CI additionally runs miri, loom, TSan, multi-arch,
// no_std, MSRV — see .github/workflows/ci.yml) — it is the FAST subset that
// catches the most common drift (fmt, clippy, the two main test combos, and
// an instruction-count regression) in a few minutes on the dev host, so most
// pushes never need a red CI run to discover a problem.

import { REPO_ROOT, run } from './lib.mjs';
import { PER_PR_ROWS, rowToCargoArgs, rowLabel } from './check-matrix.mjs';
import { staleArtifactDiagnosis } from './stale-artifact-diagnosis.mjs';

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
    // stays excluded: it FAILS on a clean tree at HEAD
    // (tests/mock.rs's decommit_silently_skips_contract_violating_offsets
    // calls the contract-violating decommit that api/decommit.rs's
    // debug_assert rejects — a pre-existing crate issue, not a gate
    // issue; see the task #1071 commit), so mock-backend RUNTIME behavior
    // stays CI-only.
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
    // where PAGE == page_size() by construction. Runs the fixture self-test
    // (the permanent counterfactual: must-flag and must-pass shapes) before
    // scanning the tree. See the script's own header.
    name: 'verify-vmem-page-constant-call-sites (task #1076 page-constant guard)',
    cmd: 'node',
    args: ['scripts/verify-vmem-page-constant-call-sites.mjs'],
  },
];

console.log(`[check-all] repo: ${REPO_ROOT}`);
console.log(`[check-all] running ${steps.length + 1} step(s) (argv-roundtrip, fmt, clippy x${clippyRows.length} [generated], test x4, aligned-vmem x15 [2 clean + 5 clippy + 3 cross-target unix (1 check + 2 clippy) + 1 mock clippy + 2 test + 1 doc + 1 optional semver], perf-gate check + internals-boundary test [generated], verify-internals-negative-boundary, verify-alloc-core-dbg-internals-exhaustive, verify-perf-gate-stubs, verify-gate-report, verify-commit-prefixes, vmem-doc-drift-guard, verify-aligned-vmem-bench-internals-exhaustive, stale-artifact-diagnosis [self-test], verify-vmem-page-constant-call-sites, iai) — fails fast\n`);

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
