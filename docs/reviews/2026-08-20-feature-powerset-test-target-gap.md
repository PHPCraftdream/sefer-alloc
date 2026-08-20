# The `feature-powerset` job never compiles a test target — measurement and recommendation

Date: 2026-08-20
Task: #1240 (MEASURE-then-RECOMMEND; no CI change made by this task)
Author context: cargo-hack 0.6.45 was already installed locally (`cargo hack --version` →
`cargo-hack 0.6.45`) — nothing was installed for this task.

---

## 1. Verified facts (each re-established here, not taken from the task brief)

### 1.1 The job and its steps, quoted

`.github/workflows/ci.yml`, job `feature-powerset` (starts at the line `  feature-powerset:`),
gated at job level by:

```yaml
    if: ${{ github.event_name == 'schedule' || github.event_name == 'workflow_dispatch' }}
```

with the workflow-level trigger (top of the same file):

```yaml
  schedule:
    - cron: '0 6 * * 1'   # NUMA real-kernel job (weekly Monday 06:00 UTC)
  workflow_dispatch:
```

The job's two build steps, verbatim:

```yaml
      - name: cargo hack check --feature-powerset --depth 2
        run: cargo hack check --feature-powerset --depth 2 --no-dev-deps
```

```yaml
      - name: cargo hack check --feature-powerset --depth 2 (aligned-vmem)
        run: cargo hack check --feature-powerset --depth 2 -p aligned-vmem --no-dev-deps
```

So the job covers exactly two crates: the root crate `sefer-alloc` (no `-p`; the default
package of this non-virtual workspace root) and `aligned-vmem`.

### 1.2 What `--no-dev-deps` does

From `cargo hack --help` (0.6.45), verbatim:

> `--no-dev-deps` — Perform without dev-dependencies.
> Note that this flag removes dev-dependencies from real `Cargo.toml` while cargo-hack is
> running and restores it when finished.

One precision the task brief's phrasing glosses: `--no-dev-deps` does not itself exclude
test targets — plain `cargo check` (without `--all-targets`) already checks only lib+bins.
The two flags compose the blind spot: `cargo check` never compiles `tests/*.rs`, and
`--no-dev-deps` additionally strips the dev-dependencies those files need, so no
combination of the current step ever typechecks an integration test, bench, or example.
The job comment's own rationale (ci.yml: "keeps the powerset scoped to the library's own
feature graph") is accurate as written — it is a lib-level job by design.

### 1.3 `--all-targets` and `--no-dev-deps` cannot coexist

MEASURED, not assumed — passing both to cargo-hack 0.6.45:

```
$ cargo hack check --feature-powerset --depth 2 -p aligned-vmem --all-targets --no-dev-deps
error: --no-dev-deps may not be used together with --all-targets
```

cargo-hack rejects the pair outright. Any "add `--all-targets`" option is therefore a
flag *replacement*, never an addition.

### 1.4 Task #1231's history, re-verified statically

Commits exist and say what the brief said: `2828e04` (introduced the test), `5979275`
(fixed; its body: "a fifth `#[cfg]` in this commit closes a PRE-EXISTING break … `SERIAL`
is gated on `bench-internals`. So `cargo check -p aligned-vmem --tests
--features=huge-pages` for a Linux target has been an E0425 on `main`, verified by
re-running that exact command against an unmodified `1b72e73` checkout"), and `14bf368`
(added the per-PR row `cargo check -p aligned-vmem --all-targets --features huge-pages`,
ci.yml line 179).

I additionally confirmed the break by inspection at `2828e04` without building a tree
(`git show`, read-only): in `tests/decommit_capability.rs` at that commit,
`static SERIAL` is gated `#[cfg(feature = "bench-internals")]`, while
`fn ci_hugetlb_real_pool_decommit_actually_zeroes_memory_on_reaccess` is gated
`all(any(target_os = "linux", target_os = "android"), feature = "huge-pages")` — no
`bench-internals` — and its body takes `SERIAL.lock()`. On a Linux target under
`huge-pages`-without-`bench-internals` that is E0425 by inspection. In today's tree the
fix is on the use site (`#[cfg(feature = "bench-internals")]` on the `let _serial = …`
line inside that test).

### 1.5 A flag-name fact for anyone re-deriving numbers

`--dry-run` — the flag CLAUDE.md's note records using — no longer exists in cargo-hack
0.6.45. It is forwarded to cargo, which rejects it. The equivalent today is
`--print-command-list`. All counts below used `--print-command-list`.

---

## 2. Measurements

Environment for every wall time below: Windows 10 x86_64, host target
`x86_64-pc-windows-msvc`, one shared warm `target/` directory, variants run sequentially.
CI runs on `ubuntu-latest` cold — absolute times will differ; the *ratios* are the
transferable quantity, and even those are indicative only.

### 2.1 Invocation counts

| Command (prefix `cargo hack check --feature-powerset --depth 2`) | Count | Label |
|---|---:|---|
| `-p aligned-vmem --no-dev-deps` (CURRENT) | **14** | MEASURED — `--print-command-list` piped to `rg -c '^cargo check'`; cargo-hack's own progress line confirmed `(14/14)` |
| `-p aligned-vmem --all-targets` (PROPOSED) | **14** | MEASURED — same method |
| `--no-dev-deps` (CURRENT, root) | **365** | MEASURED — same method |
| `--all-targets` (PROPOSED, root) | **365** | MEASURED — same method |

The count is identical between variants — `--all-targets`/`--no-dev-deps` change what each
invocation builds, not how many combinations are enumerated. **The invocation-count ratio
is exactly 1.0 for both crates.**

DERIVED sanity check (closed form, not a measurement): depth-2 powerset of N features is
1 (no-default) + 1 (default) + N + C(N,2) + 1 (all-features). aligned-vmem: N=5 → 18
naive vs 14 measured; the four dropped are the default run (aligned-vmem has no `default`
key, so it resolves identically to the no-default run), two pairs made redundant by
implication (`alloc-lazy-commit = ["lazy-commit"]`, `fault-injection = ["lazy-commit"]`),
and the `--all-features` run, which cargo-hack 0.6.45 did not emit in any configuration I
tried (depth 2, depth 3, unbounded) despite its help text describing one. Root: N=28 →
409 naive vs 365 measured; of the 44 dropped combinations, one is the missing
`--all-features` run and the other 43 are attributable to feature implications (e.g.
`production = ["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
"alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]`).

Two consequences worth knowing: (a) CLAUDE.md's recorded **308** for this step is stale —
today it enumerates **365** (features accreted since R14-10); (b) the depth-2 job builds
no full-feature-set combination at all (no `--all-features` run; no depth-2 pair reaches
the union) — that combination is covered per-PR by the `--all-features` clippy rows, not
by this job.

### 2.2 Wall times

**aligned-vmem, CURRENT step, full run** — MEASURED:

```
$ time cargo hack check --feature-powerset --depth 2 -p aligned-vmem --no-dev-deps
… (14/14) … Finished `dev` profile …
11 seconds
```

**aligned-vmem, PROPOSED (`--all-targets`, no `--no-dev-deps`), full run** — MEASURED:
same session, immediately after, so the second run inherits the first's artifacts:

```
$ time cargo hack check --feature-powerset --depth 2 -p aligned-vmem --all-targets
… (14/14) … Finished `dev` profile …
16 seconds
```

**Ratio: 16/11 ≈ 1.45×** (14 invocations either way). All 14 combinations passed on this
host — with the platform caveat of §2.4.

**Root crate**: 365 invocations was too long to run twice in full, so I timed a
deterministic `--partition 1/10` (the first ~37 enumerated combinations) of each variant:

- CURRENT (`--no-dev-deps`): **68 s** — MEASURED (1/10 partition, full completion).
- PROPOSED (`--all-targets`, with `--keep-going` because combinations fail): **339 s** —
  MEASURED (1/10 partition; note the run's console display was truncated by `head -60`
  after the 14th combination, but the pipeline itself ran to completion at 339 s, which is
  consistent with ~9 s × 37 combinations).
- DERIVED full-run extrapolations (partition × 10; both slight overestimates because the
  one-time dependency build amortizes over 10× more combinations):
  CURRENT ≈ **11 min**; PROPOSED ≈ **56 min and a lower bound** — failing combinations
  abort early, so a hypothetical green run costs more per combination.

**Ratio (root, partition-measured): 339/68 ≈ 5.0×**, on a run that is failing throughout
(§2.3) — a healthy `--all-targets` run would be slower still, since it typechecks up to
249 test files + 83 examples + 25 benches per combination instead of the lib alone.

### 2.3 The root crate is born-red under `--all-targets` — live evidence

The proposed root step does not merely cost more; it fails immediately and systemically.
From the measured partition run, every one of the first 14 sampled combinations failed,
on three distinct defects (all are the #1231 class — test/example targets that do not
compile under feature combinations the library itself supports):

1. `tests/concurrent_stress.rs` — `use sefer_alloc::SyncRegion;` with no
   `#![cfg(feature = "std")]` guard; the re-export is `#[cfg(feature = "std")]`
   (src/lib.rs). E0432 under `--no-default-features` (default = `["std"]`).
2. `tests/regression_r4_3_teardown_trim.rs` — uses `SeferAlloc`, re-exported under
   `#[cfg(feature = "alloc-global")]`. E0432 in every combination lacking `alloc-global`
   (observed in sampled combinations 4, 8, 11; the rest follows structurally from the
   gate — each test target compiles independently, so no masking).
3. `examples/sol_f1_dbg_carve_batch_negative_probe.rs` — **deliberate compile-fail
   bait**. `dbg_carve_batch` is `#[cfg(feature = "internals")]`
   (src/alloc_core/alloc_core_small_diag.rs, whole impl block), and this example
   deliberately does not list `internals` in its `required-features = ["alloc-core"]`
   ("internals-off is exactly the configuration this probe exists to prove fails" — its
   own header). Because 10 features imply `alloc-core` (`alloc-xthread`, `alloc-global`,
   `batch-api`, `page-map-diag`, `alloc-decommit`, `exact-span-large`, `alloc-stats`,
   `medium-classes`, `alloc-segment-directory`, `numa-aware` — counted in the
   `[features]` section of Cargo.toml), the example is *built* — and fails — in
   a large fraction of all combinations. This one is not fixable by adding
   `required-features`: failing to compile without `internals` is its documented purpose.
   Today's green per-PR rows dodge it only because every root-crate row that builds
   tests/examples either enables no `alloc-core`-implying feature at all (default,
   `experimental`, `pinning` — none of them or their implicants turn `alloc-core` on,
   so the example's `required-features` is unmet and cargo skips the target), or
   includes `internals` / is `--all-features` (enumerated over every root-crate
   `cargo test`/`--all-targets` row in ci.yml, including the multi-line `run: |` blocks
   and the cross-target rows: clippy at 741–785, test at 1092–1871 and 2578–2590;
   note `hardened` implies `fastbin` → `alloc-global` → `alloc-core`, and every
   `hardened` row carries `internals` too). Rows that enable `alloc-core` without
   `internals` do exist (the `alloc-core page-map-diag` row, line 1344) but select
   explicit `--test` targets, which does not build examples — that is how narrowly the
   current matrix threads this needle.

The existing per-PR rows are in fact carefully balanced on exactly this edge — a sample
(`--all-targets` clippy rows: default, `experimental`, `--all-features`,
`"hardened medium-classes internals"`, `"production internals"`; full `cargo test` rows:
default, `experimental`, `"alloc-core internals"`, `"production internals"`, …; the
complete enumeration is item 3's parenthetical above). That is the same
hand-maintained-row fragility CLAUDE.md's note describes — now demonstrated at the test
level, on this crate, with three live instances.

### 2.4 Platform caveat on my green aligned-vmem result

My host is Windows. The #1231 defect lived in a test gated
`any(target_os = "linux", target_os = "android")`, which compiles to nothing on Windows —
so my 14/14 green `--all-targets` run measures COST only; it cannot measure the Linux
coverage the step would add. The coverage argument rests on #1231's own Linux-verified
history (§1.4), not on my run. A CI runner is `ubuntu-latest`, where those tests compile.

---

## 3. Options weighed

### (a) Replace `--no-dev-deps` with `--all-targets` on the existing weekly steps

- **aligned-vmem step**: cost MEASURED trivial — same 14 invocations, 1.45× wall locally
  (11 s → 16 s; in CI, minutes at most, weekly). Because cargo-hack hard-rejects the flag
  pair (§1.3), this drops `--no-dev-deps` — which for THIS crate buys nothing:
  `crates/aligned-vmem/Cargo.toml` has an **empty `[dependencies]` section** (verified),
  so there is no dependency graph whose features dev-dependencies could pollute; the
  flag's documented purpose is structurally moot here.
- **Root step**: cost DERIVED ≈ 56+ min weekly AND permanently red — not because of one
  latent bug but by design (the `sol_f1` compile-fail tripwire, §2.3 item 3) plus two
  genuinely ungated test files, on a test surface whose gating is demonstrably
  incomplete (249 test files; Cargo.toml's 120 `required-features` entries cover only
  the explicitly listed targets). Making this step green would require either excluding
  the tripwire (defeating its documented purpose) or redesigning it as a real compile-fail
  harness (e.g. `trybuild`), plus a per-target `required-features` campaign across the
  root crate's test targets. That is a project, not a flag change.
- Newly catches: E0425/E0432/E0599-class breaks in `tests/`, `benches/`, `examples/` for
  every enumerated feature combination — the exact #1231 class, on a Linux runner where
  the linux/android-gated tests actually compile (for aligned-vmem).
- Still misses: everything below in (b)'s residual list; and for the root crate it is
  not deployable today.

### (b) Narrow the change to aligned-vmem: `--all-targets` on that one step only

The brief framed this as "keep the existing step, add a second"; because cargo-hack
hard-rejects the flag pair (§1.3) the concrete shape is a *replacement on the aligned-vmem
step* (for the owner to apply or not — not applied by this task): change the second
step of `feature-powerset` from

```yaml
        run: cargo hack check --feature-powerset --depth 2 -p aligned-vmem --no-dev-deps
```

to

```yaml
        run: cargo hack check --feature-powerset --depth 2 -p aligned-vmem --all-targets
```

(a replacement, not an addition — the flag pair cannot coexist, and with zero normal
deps the lib-only row adds no information the `--all-targets` row does not already
contain. If the owner prefers preserving the lib-only row verbatim, an additional step
costs the same 14 invocations again — 28 total — buying no observable extra check.)

- **Cost**: MEASURED 14 invocations, 16 s vs 11 s locally (§2.2); identical cadence
  (weekly + on-demand), no per-PR impact. Residual risk: with dev-deps present, a
  `bench-scale-tool` breakage on the runner would redden this step for an unrelated
  reason — acceptable for a weekly job, stated here so nobody is surprised.
- **Newly catches, on top of what per-PR rows already build.** Complete inventory of
  aligned-vmem test-level feature coverage in ci.yml (every `cargo test`/`check`/`clippy`
  row naming the package, including multi-line `run: |` blocks): `{}` (default clippy,
  line 158), `{huge-pages}` (commit `14bf368`'s row, line 179), `{lazy-commit,
  fault-injection}` (line 1725), `{huge-pages, bench-internals}` (the
  `aligned-vmem-hugetlb-real` job's six test steps, lines 575–631 — that job carries no
  `if:` gate, so it runs per push/PR), and the full set / `--all-features` union (many
  rows; note the union resolves to the same cfg as the named full set and is *not* one
  of the 14 depth-2 combinations). NOT built at test level by any row anywhere today —
  and therefore newly covered, weekly, on Linux: `{lazy-commit}`, `{bench-internals}`,
  `{lazy-commit, bench-internals}`, `{bench-internals, fault-injection}`,
  `{huge-pages, lazy-commit}`, `{huge-pages, fault-injection}` — 6 distinct feature
  states, 9 of the 14 depth-2 invocations (the `alloc-lazy-commit` alias spellings
  resolve identically). Concretely, a future test gated `fault-injection` referencing a
  `bench-internals`-gated item, or one gated `huge-pages, lazy-commit` referencing a
  `fault-injection`-gated item, is today invisible to every gate; the per-PR `huge-pages`
  row only compiles tests whose gates `{huge-pages}` alone satisfies, so it caught
  #1231's shape but cannot see anything inside `all(huge-pages, fault-injection)`-style
  gates. The `bench-internals,huge-pages` pair would also build the
  `v1189_windows_large_page_native_profile` example in its *minimal* configuration
  (`required-features = ["bench-internals", "huge-pages"]`) — per-PR it is built only
  as part of the `--all-features` union.
- **Still misses** (residual gap, stated honestly):
  - the root crate entirely — including the three live defects of §2.3, which remain
    invisible to every current gate;
  - runtime failures — `check` typechecks, it does not run tests;
  - the green-and-dead file-level `#![cfg]` class (a typo'd cfg compiles a test binary
    with zero tests and exits 0) — check cannot see test counts; the existing tee+grep
    sentinel rows remain the only defence;
  - the `--cfg` flag space (`aligned_vmem_mock`, `aligned_vmem_page_size_override`) —
    outside cargo-hack's feature space entirely since task #962;
  - link/codegen errors (`check` produces no binaries);
  - and the weekly cadence itself: up to 7 days of lag, by design (for scale: the
    per-PR `14bf368` row delivers its one combination immediately; this step delivers
    the other nine weekly).

### (c) Change nothing in CI; correct CLAUDE.md's cargo-hack note

The note (CLAUDE.md, "`cargo-hack` feature-powerset CI — ADOPTED…") is not false in what
it says — it motivates the job with R13-12, a *library-level* E0599, and never claims
test coverage — but it does claim the decision "closes the actual gap" for a bug class it
describes generically ("compiles under every combination a human happened to write
down"), and a reader can reasonably take that as covering the #1231 shape. It does not.
Corrections the owner may want in the same edit:

1. Scope the claim: add one sentence stating the job builds lib+bins only
   (`--no-dev-deps` + plain `check`), never `tests/`/`benches/`/`examples/`, and that
   test-target feature combinations are covered only by hand-written rows (see ci.yml's
   `aligned-vmem-gates` and `test-workspace`).
2. 308 → 365 (as of 2026-08-20, `--print-command-list`) or, better, drop the hardcoded
   count in favour of "re-derive via `--print-command-list`" per this repo's own
   no-hardcoded-counts convention (task #776/F10, already applied in ci.yml's own
   comment).
3. `--dry-run` → `--print-command-list` (the old flag no longer exists in cargo-hack
   0.6.45; §1.5).
4. Optionally record that depth 2 emits no `--all-features` run, so the job does not
   build the full feature union at all.

- **Cost**: zero CI minutes.
- **Catches**: nothing new — this is the honesty-only option. Its value is that the
  project's own guidance stops implying a coverage the job does not deliver (this task
  exists because the note was read that way).
- **Still misses**: everything (a)/(b) would catch; the 14-invocation test-target space
  for aligned-vmem keeps relying on rows humans remember to write.

### Options considered and rejected

- **`--all-targets` at `--depth 1`** to shrink cost: the interesting interactions
  (#1231's was a *gate* mismatch visible even in a single-feature row — `huge-pages`
  alone — on Linux) are mostly single-feature visible, but pairs like
  `bench-internals,huge-pages` (which the #1223-era file leans on heavily) are not; and
  the measured cost of depth 2 is already trivial for aligned-vmem. Rejected.
- **`cargo hack test`** (actually run tests per combination): strictly more than the
  brief's `--all-targets` question, 249 test binaries × combinations on the root crate, and
  runtime behaviour under partial feature sets is a different campaign. Not recommended
  now; noted as a possible future step for aligned-vmem only if a runtime-under-
  combination defect class ever materialises.

---

## 4. Recommendation

**Adopt (b) for the aligned-vmem step, and the (c) documentation corrections. They are
complementary, not alternatives.** Leave the root crate's step exactly as it is.

Rationale, in one paragraph each:

- *aligned-vmem*: the measured marginal cost is 14 unchanged invocations and ~1.45× wall
  on a weekly job (MEASURED: 11 s → 16 s locally); the flag being replaced
  (`--no-dev-deps`) is protecting a dependency graph this crate does not have (empty
  `[dependencies]`, verified); and the coverage added is exactly the class that already
  cost this project a multi-commit blind spot (#1231), on the platform (Linux) where it
  manifests, across the 6 feature states / 9 of the 14 depth-2 invocations listed in
  §3(b) that no per-PR row builds — the per-PR row from
  `14bf368` covers one combination of that space and this task does not re-argue for it.
- *root crate*: measured born-red (§2.3), by design in one case (the `sol_f1` tripwire)
  and by debt in the others; a green `--all-targets` powerset for `sefer-alloc` requires
  owner-level decisions (redesign or exempt the tripwire; a `required-features`/
  `#![cfg]` campaign over 249 test files) that should not be smuggled in through a CI
  flag change. The honest interim state is (c): say in CLAUDE.md what the job does and
  does not cover. The three §2.3 defects are recorded below for the owner; fixing them is
  outside this task's scope.

---

## 5. Findings outside this task's scope (reported, not fixed)

1. `tests/concurrent_stress.rs` (root crate) does not compile under
   `--no-default-features` (ungated `use sefer_alloc::SyncRegion`). MEASURED (E0432,
   partition run, combination 1).
2. `tests/regression_r4_3_teardown_trim.rs` (root crate) does not compile without
   `alloc-global` (ungated `SeferAlloc` use). MEASURED (E0432, combinations 4, 8, 11).
3. `examples/sol_f1_dbg_carve_batch_negative_probe.rs` — the deliberate compile-fail
   tripwire makes EVERY `--all-targets` build with `alloc-core`-implying features but
   not `internals` fail (MEASURED, E0599 in all 14 sampled combinations). If the owner
   ever wants (a) for the root crate, this probe needs redesign first (e.g. `trybuild`).
4. CLAUDE.md staleness items: 308 → 365; `--dry-run` → `--print-command-list` (§1.5,
   §2.1). Also, ci.yml's job comment cites "C(26,2)+26+1" for a "~26 top-level features"
   count that is today 28 non-default features enumerating 365 — the formula's shape is
   right but its inputs drifted.
5. The `--depth 2` job builds no full-feature-union combination for either crate (no
   `--all-features` run emitted; §2.1) — covered per-PR by other rows, but worth knowing
   when reading this job's name.

## 6. Reproduction commands

```
cargo hack --version                                  # 0.6.45
cargo hack check --feature-powerset --depth 2 -p aligned-vmem --no-dev-deps --print-command-list | rg -c '^cargo check'   # 14
cargo hack check --feature-powerset --depth 2 -p aligned-vmem --all-targets --print-command-list | rg -c '^cargo check'   # 14
cargo hack check --feature-powerset --depth 2 --no-dev-deps --print-command-list | rg -c '^cargo check'                   # 365
cargo hack check --feature-powerset --depth 2 --all-targets --print-command-list | rg -c '^cargo check'                   # 365
# wall times: wrap the same commands without --print-command-list in timestamps;
# root-crate slices via --partition 1/10 (--keep-going for the --all-targets variant)
```

Not verified here: actual GitHub Actions wall times (no access to run history from this
machine); whether main is currently green on the `14bf368` per-PR row (same reason).
Every local wall time in this document carries the §2.2 environment caveat.
