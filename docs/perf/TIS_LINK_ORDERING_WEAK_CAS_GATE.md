# `tagged-index-stack` — P3-1/P3-2: link-cell ordering & CAS-kind weak-memory gate

Date: 2026-09-01. Measurement-only task (Sol-codex review run 4, findings P3-1
link-cell ordering and P3-2 strong-vs-weak CAS, branch `tis-sol4-w3-perf`
Wave 3). Driver:
`crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs`; companion CSVs and
raw logs are listed in §7. Every table in this report is copied verbatim from
those script-emitted artifacts; the percentages name numerator and denominator
inline.

## 0. Verdicts

- **P3-1 (link-cell `Acquire`/`Release` → `Relaxed` candidate): UNCHANGED.**
  x86-64 codegen is identical across base/links-Relaxed (zero cost either
  way, §3.1); the aarch64 static delta is real (link `ldar`/`stlr` present
  in base, removable under `Relaxed`, §3.2); but wall-clock on real
  weak-memory silicon is UNMEASURED. (Status clarified 2026-09-02, review
  run 6 P3-1: the STATIC multi-target A/B codegen comparison is COMPLETE —
  this report is its result; the only measurement still open is the NATIVE
  aarch64 WALL-CLOCK timing leg — §5, OPEN_ITEMS item 62.) Per CLAUDE.md
  and the review's own
  instruction ("do not change ordering blindly"), no ordering change lands
  without a measured wall-clock win.
- **P3-2 (`compare_exchange` → `compare_exchange_weak` candidate):
  UNCHANGED — measured NULL.** Codegen-identical on aarch64 under BOTH the
  outlined-atomics default lowering and the `+lse` lowering on rustc
  1.97.0/LLVM 22 (§3.2); the once-hypothesized inline-LL/SC win does not
  exist on this toolchain. The driver asserts the identity, so a toolchain
  change fails loudly ("P3-2 REOPENED") and reopens the question.
- **Infrastructure:** an arm64 wall-clock CI gate
  (`tis-weak-memory-wallclock-gate`, workflow_dispatch-only,
  `ubuntu-24.04-arm`) was AUTHORED in `.github/workflows/ci.yml` but never
  executed from this sandbox (no push). It is the named next trigger (§5).

## 1. Measurement identity and reproduction

**Identity captured BEFORE any build.** The driver's first action is
`node scripts/capture-measurement-identity.mjs --json`; the aarch64 codegen
raw log header records, verbatim:

```json
{"capturedAt":"2026-09-01T16:14:30.718Z","headSha":"0472f29383268b07d955d2d80dc8edc5e866fd56","treeSha":"c6bc313f3c879c231e004e335fced56d5f6bf380","dirty":true,"recoverCommand":"git show c6bc313f3c879c231e004e335fced56d5f6bf380:<path>  # or: git archive c6bc313f3c879c231e004e335fced56d5f6bf380 | tar -x"}
```

**On `dirty: true`:** the untracked measurement scripts under
`crates/tagged-index-stack/scripts/` made the capture script report a dirty
tree, but NO tracked file was modified. The source under test
(`crates/tagged-index-stack/src/imp.rs`) is byte-identical to the committed
tree at both `headSha` and `treeSha`;
`git show c6bc313f3c879c231e004e335fced56d5f6bf380:crates/tagged-index-stack/src/imp.rs`
recovers it exactly.

**Toolchain:** rustc 1.97.0 (2d8144b78 2026-07-07, LLVM 22.1.6), host
x86_64-pc-windows-msvc. Targets: `x86_64-unknown-linux-gnu` and
`aarch64-unknown-linux-gnu` (asm-only codegen leg) and
`x86_64-pc-windows-msvc` (wallclock smoke leg).

**A/B mechanism — out-of-tree variant materialization.** The driver writes
verbatim `lib.rs` + substituted `imp.rs` (base / `links_relaxed` /
`cas_weak`) under `target/tis_p3_ab/<target>/<variant>/` and compiles each
directly with `rustc --emit=asm` (codegen leg) or as a scratch cargo crate
(wallclock leg). Each substitution anchor is asserted to occur EXACTLY ONCE
in `src/imp.rs` before substitution (text-exact anchors, e.g.
`self.next[index as usize].load(Ordering::Acquire)` → `...load(Ordering::Relaxed)`).
The shipping `src/` is never touched and no scaffolding survives in the
committed diff: the driver itself is the committed, re-runnable scaffolding.
This contrasts with TIS_BACKOFF_CAP_SWEEP_GATE.md §1's revert-per-build
protocol (in-place edit, measure, revert, per cap value). Out-of-tree was
chosen for a publish-readiness campaign because it adds ZERO feature/cfg
surface to the crate — there is no hidden `#[cfg]` experiment lever left in
the shipping source for a reviewer or downstream user to trip over, and the
measured variants are defined entirely by the committed driver.

**Reproduction commands** (exact, from the repo root):

```
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode codegen --target x86_64-unknown-linux-gnu
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode codegen --target aarch64-unknown-linux-gnu
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode wallclock --target x86_64-pc-windows-msvc --smoke
node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode summary
```

## 2. Oracles (per CLAUDE.md's path-activation rule)

**Static (codegen leg):**

- Substitution-count asserts: each of the four text anchors must occur
  exactly once in `src/imp.rs` before substitution (`applyAnchors` /
  `verifyAllAnchorsOnce`); a count other than 1 is a hard failure.
- x86 identity oracles: every studied function's normalized instruction
  text must be sha-identical (`sha256_16`) between each non-base variant
  and base, and base push/pop must contain `cmpxchg >= 1`.
- aarch64 delta oracles, exact formulas (all derived from the base run at
  runtime, never hardcoded):
  - `links_relaxed pop ldar == base pop ldar − base load_next ldar`,
    with residual `>= 1` — the residual IS `pop_index_impl`'s own 64-bit
    Acquire HEAD load (`head_ref.load(Ordering::Acquire)`), positively
    proving the HEAD ordering was untouched;
  - `links_relaxed push stlr == 0` with `base push stlr >= 1`;
  - a plain `ldr`/`str` must appear in relaxed load_next/store_next
    (the link access is now a relaxed access, not gone);
  - CAS instruction counts (`cas8` under default features, `cas` under
    `+lse`) unchanged by the links substitution;
  - `cas_weak` sha-identity asserted for push/pop under BOTH feature sets.
    These asserts are DELIBERATE tripwires: if a future toolchain
    reintroduces an inline-LL/SC lowering where weak differs from strong,
    they FAIL LOUDLY (message contains "P3-2 REOPENED") instead of silently
    hiding the change.
- Base-shape oracles: under default features, base push/pop must contain
  `>= 1` `__aarch64_cas8_` outlined call (the observed lowering); under
  `+lse`, `>= 1` single-instruction `casl`/`casa` and exactly 0 outlined
  calls.

**Wallclock leg (wallclock oracles):**

- Retry-counter oracle: per-variant sum of `push_retries + pop_retries`
  over all samples must be `> 0` (the CAS-retry path was actually
  exercised). Actual smoke values from the x86 wallclock log (threads=4,
  window_ms=100, samples=1, smoke=true): base push_retries=19445 /
  pop_retries=32744; links_relaxed 9618 / 41002; cas_weak 14248 / 38363.
- Lateness guard: measured `elapsed_ms >= 0.5 × window_ms` per sample.
- `ops_per_sec` re-derivation assert: harness-reported `ops_per_sec` must
  match `ops_total / (elapsed_ms / 1000)` within 2%, and the driver's
  median-ratio arithmetic is asserted against itself before emission.

**Known oracle limitation, stated honestly:** the static oracles pin the
BASELINE rustc's lowering (1.97.0 / LLVM 22.1.6). Other toolchains and
lowerings are covered only by re-running the gate — which the
`tis-weak-memory-wallclock-gate` CI job does on each dispatch, re-verifying
the static oracles (including the P3-2 tripwires) on the runner's own
toolchain before any timing is recorded.

## 3. Results

### 3.1 x86-64 codegen (`x86_64-unknown-linux-gnu`, default features)

Derived table, VERBATIM from
`docs/perf/_raw_tis_p3_ab_x86_64-unknown-linux-gnu_codegen.log`:

| target | features | function | variant | sha256_16 | instr_count | ldar | stlr | ldaxr | stlxr | cmpxchg | cas | cas8 | delta% vs base |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| x86_64-unknown-linux-gnu | default | load_next | base | 1821380ada9f7f95 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | — |
| x86_64-unknown-linux-gnu | default | load_next | links_relaxed | 1821380ada9f7f95 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| x86_64-unknown-linux-gnu | default | load_next | cas_weak | 1821380ada9f7f95 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| x86_64-unknown-linux-gnu | default | store_next | base | ab75145a1d29d654 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | — |
| x86_64-unknown-linux-gnu | default | store_next | links_relaxed | ab75145a1d29d654 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| x86_64-unknown-linux-gnu | default | store_next | cas_weak | ab75145a1d29d654 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| x86_64-unknown-linux-gnu | default | push_index_impl | base | b609f094b9c5928b | 51 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | — |
| x86_64-unknown-linux-gnu | default | push_index_impl | links_relaxed | b609f094b9c5928b | 51 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 |
| x86_64-unknown-linux-gnu | default | push_index_impl | cas_weak | b609f094b9c5928b | 51 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 |
| x86_64-unknown-linux-gnu | default | pop_index_impl | base | 0f0d1b99ddeaa2fd | 58 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | — |
| x86_64-unknown-linux-gnu | default | pop_index_impl | links_relaxed | 0f0d1b99ddeaa2fd | 58 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 |
| x86_64-unknown-linux-gnu | default | pop_index_impl | cas_weak | 0f0d1b99ddeaa2fd | 58 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0 |

Verdict: all four functions are sha-identical across all three variants —
on x86-64 TSO, both candidates cost exactly zero (identity by construction,
a stronger statement than any wall-clock null could be). The `delta% vs
base` column is derived as: delta% = (variant instr_count − base
instr_count) / base instr_count × 100, numerator and denominator being the
instr_count cells of the same (target, features, function) row group.

### 3.2 aarch64 codegen (`aarch64-unknown-linux-gnu`, default and `+lse`)

Derived table, VERBATIM from
`docs/perf/_raw_tis_p3_ab_aarch64-unknown-linux-gnu_codegen.log`:

| target | features | function | variant | sha256_16 | instr_count | ldar | stlr | ldaxr | stlxr | cmpxchg | cas | cas8 | delta% vs base |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| aarch64-unknown-linux-gnu | default | load_next | base | 9a650d8e501255d1 | 13 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | — |
| aarch64-unknown-linux-gnu | default | load_next | links_relaxed | e16aee9e62bc42a0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | -7.7 |
| aarch64-unknown-linux-gnu | default | load_next | cas_weak | 9a650d8e501255d1 | 13 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| aarch64-unknown-linux-gnu | default | store_next | base | da86556ea0e3823a | 13 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | — |
| aarch64-unknown-linux-gnu | default | store_next | links_relaxed | a0893bbe063c93d5 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | -7.7 |
| aarch64-unknown-linux-gnu | default | store_next | cas_weak | da86556ea0e3823a | 13 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| aarch64-unknown-linux-gnu | default | push_index_impl | base | aafab9e307837b83 | 68 | 0 | 2 | 0 | 0 | 0 | 0 | 2 | — |
| aarch64-unknown-linux-gnu | default | push_index_impl | links_relaxed | c73d026865f9c201 | 68 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| aarch64-unknown-linux-gnu | default | push_index_impl | cas_weak | aafab9e307837b83 | 68 | 0 | 2 | 0 | 0 | 0 | 0 | 2 | 0 |
| aarch64-unknown-linux-gnu | default | pop_index_impl | base | 3c6f69664f51da36 | 75 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | — |
| aarch64-unknown-linux-gnu | default | pop_index_impl | links_relaxed | 4ed2950953948022 | 75 | 1 | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| aarch64-unknown-linux-gnu | default | pop_index_impl | cas_weak | 3c6f69664f51da36 | 75 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| aarch64-unknown-linux-gnu | lse | load_next | base | 9a650d8e501255d1 | 13 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | — |
| aarch64-unknown-linux-gnu | lse | load_next | links_relaxed | e16aee9e62bc42a0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | -7.7 |
| aarch64-unknown-linux-gnu | lse | load_next | cas_weak | 9a650d8e501255d1 | 13 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| aarch64-unknown-linux-gnu | lse | store_next | base | da86556ea0e3823a | 13 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | — |
| aarch64-unknown-linux-gnu | lse | store_next | links_relaxed | a0893bbe063c93d5 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | -7.7 |
| aarch64-unknown-linux-gnu | lse | store_next | cas_weak | da86556ea0e3823a | 13 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| aarch64-unknown-linux-gnu | lse | push_index_impl | base | 5964cf9ee6f5617e | 59 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | — |
| aarch64-unknown-linux-gnu | lse | push_index_impl | links_relaxed | 1185c4b5d1b11c1e | 58 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | -1.7 |
| aarch64-unknown-linux-gnu | lse | push_index_impl | cas_weak | 5964cf9ee6f5617e | 59 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 |
| aarch64-unknown-linux-gnu | lse | pop_index_impl | base | 65d7ba154d856a73 | 67 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | — |
| aarch64-unknown-linux-gnu | lse | pop_index_impl | links_relaxed | e3b753391e720f07 | 67 | 1 | 0 | 0 | 0 | 0 | 2 | 0 | 0 |
| aarch64-unknown-linux-gnu | lse | pop_index_impl | cas_weak | 65d7ba154d856a73 | 67 | 2 | 0 | 0 | 0 | 0 | 2 | 0 | 0 |

Narration (all numbers from the table above):

- **Base pop** carries `ldar=2` — the head's own 64-bit Acquire load plus
  the link's 32-bit Acquire load. **Relaxed pop** carries `ldar=1` (head
  only): the oracle formula `relaxed pop ldar == base pop ldar − base
  load_next ldar` = 2 − 1 = 1 held, and the residual `>= 1` positively
  proves the head Acquire load was untouched.
- **Push `stlr` 2 → 0** in links_relaxed (both feature sets); a plain
  relaxed link `str` replaces it (asserted `str >= 1`).
- **`load_next`/`store_next`: 13 → 12 instructions** (delta% = −7.7:
  numerator 1 instruction removed, denominator 13 base instructions — 1 of
  13 in each). The sha changes (e.g. `9a650d8e501255d1` →
  `e16aee9e62bc42a0`) while `cas_weak` keeps base's sha everywhere.
- **CAS counts unchanged** by the links substitution in every row.
- **`+lse` axis:** each CAS lowers to a single `casl`/`casa` instruction
  (2 casa + 2 casl across push+pop; `cas=2` per function), zero
  `__aarch64_cas8_` calls, zero `ldaxr`/`stlxr` anywhere.
- **The "outlined atomics" discovery:** under the DEFAULT feature set
  (baseline armv8-a) on this toolchain, BOTH `compare_exchange` and
  `compare_exchange_weak` lower to OUTLINED atomic helper calls
  (`bl __aarch64_cas8_acq` / `bl __aarch64_cas8_rel`; `cas8=2` per
  function) — there is ZERO inline `ldaxr`/`stlxr` anywhere in the file.
  That is why the weak-CAS question VANISHES on this toolchain: there is no
  inline LL/SC loop for `compare_exchange_weak` to shorten, and the helper
  call sequence is byte-identical for strong and weak (sha-identical rows
  above under both feature sets).

As in §3.1, delta% = (variant instr_count − base instr_count) / base
instr_count × 100, numerator and denominator being the instr_count cells of
the same (target, features, function) row group.

### 3.3 Wall-clock smoke (`x86_64-pc-windows-msvc`) — VALIDATION-ONLY, not evidence

Derived summary table, VERBATIM from
`docs/perf/_raw_tis_p3_ab_x86_64-pc-windows-msvc_wallclock.log`
(threads=4 window_ms=100 samples=1 smoke=true):

| target | variant | median_ops_per_sec | ratio vs base | push_retries | pop_retries |
|---|---|---|---|---|---|
| x86_64-pc-windows-msvc | base | 16707840.00 | 1.0 | 19445 | 32744 |
| x86_64-pc-windows-msvc | links_relaxed | 16574080.00 | 0.992 | 9618 | 41002 |
| x86_64-pc-windows-msvc | cas_weak | 16528640.00 | 0.989 | 14248 | 38363 |

Medians sit ~16.3–16.7 M ops/s with ratios 0.989–1.0 (each ratio's
numerator is that variant's median, denominator is base's median) — noise,
and consistent with the x86 codegen identity of §3.1. This leg exists to
validate the harness/oracle plumbing, NOT to answer the P3-1 question: **no
wall-clock evidence exists for aarch64 yet.**

**What static codegen CANNOT tell:** whether aarch64 `ldar`/`stlr` versus
`ldr`/`str` is a measurable cycle delta on real weak-memory silicon. That
is exactly what the arm64 CI job exists to measure.

## 4. Infrastructure decision

Options weighed for the wall-clock axis:

1. **QEMU-user via `cross` — DECLINED for timing.** QEMU-user TCG on an x86
   host does not reproduce weak-memory TIMING; acquire/release lowering
   effects are invisible under TCG, so an A/B would measure nothing while
   looking plausible. (It remains usable for correctness only.)
2. **Real ARM64 runner — CHOSEN.** `ubuntu-24.04-arm` is a free standard
   hosted runner for public repositories (this repo is public); the job
   `tis-weak-memory-wallclock-gate` in `.github/workflows/ci.yml` is
   workflow_dispatch-only, following the feature-powerset job's cadence
   pattern, because it is an evidence-gathering gate (~5 min arm64 minutes
   per run), not a standing regression check.
3. **Bare decline (close P3-1 unmeasured) — superseded by (2) plus the
   static legs**, but the ARM RUN itself is still pending and is filed as
   the open item (docs/perf/OPEN_ITEMS.md item 62).

**aarch64 CORRECTNESS coverage** already exists in CI's `multi-arch` job:
workspace-wide `cross test --target aarch64-unknown-linux-gnu` rows run the
crate's non-loom tests under QEMU (note: this is INFERRED from
workspace-wide `cargo test` semantics — no tis-specific step exists there).

## 5. Decision and consequences

**Keep Acquire/Release links + strong CAS** — `crates/tagged-index-stack/src/`
is unchanged by this task. The loom-model + counterfactual obligation (task
contract) fires ONLY if a future measured wall-clock win justifies landing
Relaxed; none exists today.

Next trigger: dispatch `tis-weak-memory-wallclock-gate`. Then either:
(a) measured win → land the Relaxed ordering + loom model + counterfactual +
`perf(runtime)` commit; or (b) NULL → record the result and close
docs/perf/OPEN_ITEMS.md item 62.

## 6. Verification run (this task)

- `node crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs --mode summary`
  → exit 0; emitted
  `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE_summary.csv` (96 data rows:
  2 identity, 80 codegen incl. per-family metrics, 12 codegen_identity,
  6 wallclock incl. 3 median + 3 ratio rows; all ratios asserted in-script
  against their own re-derivation from the CSVs' sample rows).
- Static legs (this session's committed logs): both codegen runs exited 0
  with all oracle asserts passing (x86 identity table; aarch64 delta
  formulas and cas_weak sha-identity tripwires under default and `+lse`).
  The wallclock smoke run exited 0 with the retry-counter oracle satisfied
  on all three variants (§3.3) and the lateness/`ops_per_sec` guards
  passing.
- Every table in this report was diffed against its raw log's derived table
  (12-row x86, 24-row aarch64, 3-row wallclock): all three diffs are empty —
  verbatim-verified.
- `git status --porcelain` after the run shows exactly: the modified driver,
  the new report, the new summary CSV, and the pre-existing artifacts

## 7. Companion artifacts

Committed evidence files (exact names, all under `docs/perf/`):

- `TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_x86_64-unknown-linux-gnu.csv`
- `TIS_LINK_ORDERING_WEAK_CAS_GATE_codegen_aarch64-unknown-linux-gnu.csv`
- `TIS_LINK_ORDERING_WEAK_CAS_GATE_wallclock_x86_64-pc-windows-msvc.csv`
- `TIS_LINK_ORDERING_WEAK_CAS_GATE_summary.csv` (this task's new compact
  machine-readable companion; regenerated by `--mode summary`)
- `_raw_tis_p3_ab_x86_64-unknown-linux-gnu_codegen.log` and
  `_raw_tis_p3_ab_aarch64-unknown-linux-gnu_codegen.log` (raw codegen logs,
  header identity + oracles + derived tables)
- `_raw_tis_p3_ab_x86_64-pc-windows-msvc_wallclock.log` (raw wallclock log)
- `_raw_tis_p3_ab_x86_64-unknown-linux-gnu_codegen.s.all` and
  `_raw_tis_p3_ab_aarch64-unknown-linux-gnu_codegen.s.all` — the full
  emitted assembler for every (features, variant) compilation, committed as
  the audit basis for the extraction and oracle counts.
- Driver: `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs`.
- CI job: `tis-weak-memory-wallclock-gate` in `.github/workflows/ci.yml`.
- OPEN_ITEMS: items 61 (infra status) and 62 (the pending arm64 wall-clock
  run) in `docs/perf/OPEN_ITEMS.md`.
