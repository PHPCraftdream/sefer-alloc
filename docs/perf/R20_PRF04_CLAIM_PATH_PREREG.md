# R20 PRF-04 claim-path Ir gate — preregistration (phase: before the measuring B build)

**Status: UNMEASURED for the gate.** No gate measurement was taken, no runtime change, no verdict and no
runtime policy exist in this phase. Only A-only *harness-validity* runs were made (they decided the cell
design, see "Harness characterization"). Candidate B (`6951a9d0` on top of A) **was built, and its path
oracle was run once, during runner development** (compile check and claim-source counters only: no
instruction count of B was taken, and that run's artefacts were discarded, not used). No threshold below
was chosen or changed by looking at a B instruction count, and none exists. The historical R19 PRF-04
records (`R19_PRF04_B_VS_A_GATE.md`, run01/run02) are **NOT recomputed and NOT reinterpreted** by this
gate; the run02 NO-GO stands exactly as retained.

## Authority

Owner decision 2026-10-12: «делаем как @ox рекомендовал», implemented per
`docs/reviews/2026-10-12-prf-open-questions-reviewer-answers.md` §4. The V4 direction is accepted, but the
first reviewer's KILL-1 estimator (320 ABBA blocks per mode) is **REJECTED**: against the committed run02
data the block SD is 0.97–1.33 versus the required ≤ 0.20, giving 2.0–3.5 % power at n=320. The new primary
KILL-1 is the **deterministic Ir of the claim path** — this gate. The native axes (live latency, churn
wall, wave p99) become descriptive and are deferred to a quiet bare-metal host. Two independent reviewer
reads of this preregistration (2026-10-12) are incorporated below: the construction cell is direct (no
derived intercept), the oracle is bound to each arm, the free-set path has its own family, a layout-noise
ceiling bounds C0, and the frozen set covers every file the verdict depends on.

## Entry point and why

The measured path is **`HeapRegistry::dbg_claim_lease()`** — the `internals`-gated safe observer whose
body is exactly `claim_lease()` (`claim_impl(HeapCore::new, |_| {})`,
`src/registry/heap_registry/claim.rs`), the call `tls_heap::bind_slow_tagged` makes on a thread's first
allocating call; `HeapLease::drop` is the thread-exit release (LIVE → FREE plus `reuse_hint`). PRF-04
(V4/V4′) changes only code inside that claim/release path, so this is the layer the decision applies to; the
TLS wrapper around it (`finish_bind`, the abandon guard) is untouched by the candidate and constant across
arms. This is **not** the `#[global_allocator]` layer: a cell at that layer runs the claim on a spawned
thread, and under the default iai-callgrind toggle (entered by the benchmark function, which runs on the
main thread) callgrind records **0 Ir** in worker-thread output files (verified, see below), so such a cell
would measure spawn/join cost and miss the claim. Collecting a child thread needs a per-thread
`EntryPoint::Custom` toggle on a named child symbol with its own symbol/coverage receipts (as the PRF-01
producer cell has); that machinery is not needed here because the registry-layer cell executes the same
`claim_impl` on the collected thread. The true thread/`GlobalAlloc` path stays a **descriptive native**
measurement (deferred, below).

Feature sets: the Ir cells are built with `production internals` (no `bench-internals`: the claim-source
counters and the `live_config_matches` probe exist only under it and would change the measured path). The
path oracle is a separate example built with `production bench-internals internals`; path selection does
not depend on the counters. The oracle keeps the system allocator global and binds no Sefer heap to the main
thread, so its registry starts empty exactly as in each iai cell (which runs in a fresh process).

## Harness characterization (A-only validity runs, not gate data)

1. **Worker threads are not collected.** A first design (spawned threads calling `SeferAlloc::alloc`) was
   run once under iai-callgrind: the per-thread callgrind files `t2..tN` all report `summary: 0`; the
   reported Instructions equal the main thread's file. That design measured `pthread_create`/`join`, not the
   claim, and was discarded.
2. **Cold cost is not affine in the claim count.** `cold_first_claim(k)` for k = 1..40, 44, 48, 56, 60, 62,
   63, 64 shows a period-4 cost pattern (three claims of ≈ 47.1 k Ir, then one of ≈ 8–11 k Ir; within a run
   of periods the cheap one grows by 704 Ir per period). The retained profile
   (`docs/perf/_raw_r20_prf04_cold_profile.log`) attributes ≈ 86.8 % of the 16-claim cell to
   `RouteDirectory::register` → glibc `calloc` → `__memset_avx2_unaligned_erms`. The *sub-mechanism* — `calloc`
   skipping the `memset` when the arena top is freshly extended — is a **hypothesis** consistent with the
   period; the attribution to `calloc`/`memset` is observed, the skip is not. The non-affinity itself is
   observed and is what the exception below rests on. The recycled family is exactly affine (3 227 Ir per
   claim+release between every probed pair among k = 1, 2, 4, 8, 16, 32, 64). The raw per-k output is
   committed as `docs/perf/_raw_r20_prf04_harness_characterization.log` (the one-off probe bench was not
   committed; each row is `cold_first_claim(k)` / `recycled_first_claim(k)`, which the committed cells call
   at k = 16, 32, 64).
3. **Scale of the selection logic.** In the same profile the *self* cost of the inlined `dbg_claim_lease`
   (selection, slot-state transitions and inlined construction code) is ≈ 8.3 k Ir over 16 cold claims
   (≈ 0.5 k Ir per claim), and the recycled operation costs 3 227 Ir per claim+release pair. The relative
   5 % owner budget below is therefore ≈ 161 Ir per recycled claim+release and ≈ 1.9 k Ir per cold claim:
   **this gate bounds the user-visible cost of a claim; it cannot resolve a selection-logic delta of tens
   of Ir.** That is the owner's budget, not a gate defect, and it is recorded so that a GO is not read as
   "the selection logic is free". A finer instrument (a selector-only cell with an absolute Ir budget) is
   separate work, listed under "Not covered".

## Family design

- **start** — `start_first_claim()`: bootstrap plus one first claim from an empty registry. The direct
  construction cell (no intercept is ever derived from the other families).
- **cold** — `cold_first_claim(k)`: k leases are claimed and `mem::forget`ten, so every slot stays LIVE, no
  `reuse_hint` ever points at a FREE slot, and every claim is an EMPTY-first-claim bump with a fresh
  `HeapCore::new`.
- **recycled** — `recycled_first_claim(k)`: one warmup claim+drop leaves one initialised FREE slot with
  `reuse_hint` on it; each of the k iterations claims it back through the reuse path and drops it again
  (claim and release are both inside the slope).
- **wave** — `wave_first_claim(k)`: a warmup claims 64 leases and releases them all (64 FREE slots, hint on
  the last), then k leases are claimed and forgotten. This is the candidate's central state: main serves
  one claim by hint and **bumps** for the rest (its FREE slots stay stranded until the registry is
  saturated); candidate B serves the rest from its FREE-slot hint set. **The paths differ by design**, so
  this family proves the mechanism is exercised (oracle) and bounds that B's path is not costlier per claim;
  the saved `HeapCore::new` work is its descriptive benefit.

All families run on the collected thread. The main thread of the iai process has no heap bound (the system
allocator is global in the bench), so the registry sees exactly the cell's claims.

## Choice of N and K, and why

N = 16 claims, 2N = 32, 4N = 64; recycled warmup K = 1 claim+drop; wave left-behind FREE slots = 64 (the
largest measured claim count and one full 64-bit word of the free-slot hint set). All sizes lie inside the
first 64-slot registry chunk for the cold family (`CHUNK_SLOTS = 64`) and far below `MAX_HEAPS = 4096`.
The slope window spans 16 claims, far above the 10-Ir floor of `R20_CONTROL_GEOMETRY_RULE.md`. These
constants were fixed before any measuring B build existed.

## Arches

- **A** = the clean, committed HEAD of the worktree that holds the runner, the judge and this document
  (the runner refuses a dirty tree). **B** = A + one cherry-pick of `6951a9d0` (branch
  `r20-prf04-v4prime`: V4, free_set on its own cache line), committed on a temporary branch inside B's
  worktree; the runner asserts B = A + that single commit and that it touches neither the judge nor the
  runner.
- **C0** = A built from the **same source identity** in a third worktree under a different path label
  (layout noise), with the **registered** build difference `-C debuginfo=line-tables-only` (it changes no
  code; the judge requires a distinct binary). **A1/A2** = two runs of ONE A binary; **B1/B2** = two runs of
  ONE B binary.
- The FULL worktree paths of A, B and C have equal length and use one-letter labels (PRF-03 Amendment A3).
  Identity before any measurement (written by the runner's `--prepare`): `git write-tree` tree SHA, HEAD,
  patch and untracked hashes per arm, and the SHA256 of every frozen file (below). Commits are not
  manufactured after the fact.

## Frozen criteria

Notation per family F, arm X and size k: `Ir_X(k)` = the raw Instructions of the cell.

- **Windows (rule §2.3 / §3a).** *recycled* is affine and uses rule §2.3 unchanged: one window
  `w = Ir(2N) − Ir(N)` and `|(Ir(4N) − Ir(2N)) − 2·w| ≤ T_s` for every arm, violation → **INVALID**.
  *cold* and *wave* are not affine (finding 2 above), so under rule §3a they are judged as **three
  registered scenario windows**: `w0 = Ir(N)` (the whole N body: claims 1..16 plus the constant),
  `w1 = Ir(2N) − Ir(N)` (claims 17..32) and `w2 = Ir(4N) − Ir(2N)` (claims 33..64). This is a gate of
  these registered windows, **not** a model of a per-claim cost: it claims nothing for other k or other
  states, and it never derives a construction term from an intercept. Adjacent windows share an end
  point (`Ir(2N)` enters `w1` and `w2` with opposite signs), so they are NOT independent confirmations.
  All windows must pass.
- **(R-claim) primary KILL-1 — one-sided, per family and window:** `w_B ≤ w_A + 5 % × w_A` (numerator
  `w_B − w_A`, denominator `w_A`) for EVERY pair (B1,B2) × (A1,A2), in exact integer (BigInt) arithmetic
  `20·(w_B − w_A) ≤ w_A`. The 5 % budget is the owner's, expressed in relative Ir and frozen here. A single
  failing pair in any window of any family → **NO-GO**. A speedup (`w_B < w_A`) is never punished. A
  non-positive `w_A` → INCOMPLETE.
- **T_s** `= max(10, |w(A1) − w(A2)|, |w(C0) − w(A1)|)` per window (rule §2.2). It is used by the recycled
  linearity test and by the construction budget; it **never widens the 5 % R-claim budget**.
- **Layout-noise ceiling.** For every window (and for the start cell) `|A1 − A2|` and `|C0 − A1|` must each
  be ≤ `max(10, ⌊A1 / 200⌋)` (numerator the absolute difference, denominator the A1 value; 0.5 % = one tenth
  of the 5 % budget). Above the ceiling the verdict is **INCONCLUSIVE**; the ceiling is never raised after
  data. (A wide C0 can therefore not legitimise a wide tolerance.)
- **(R-construct)** the **start** cell: `Ir_B(start) − Ir_A(start) ≤ budget_K` with
  **`budget_K = T_s(start)`** for every (B1,B2) × (A1,A2) pair. Justification of "0 beyond noise": B only
  moves the `free_set` field onto its own cache line inside the existing static registry and by
  construction adds not one operation to bootstrap or the first EMPTY claim; any super-noise increase is
  unplanned work and needs investigation, not a budget. (`budget_K` is the owner's input per
  preregistration; for this gate it is the principle above.)
- **(R-benefit)** the usefulness criteria are carried over **UNCHANGED, verbatim thresholds**, from
  `docs/perf/R19_PRF04_GATE2B_PREREG.md` §Owner thresholds:
  - BENEFIT (median of 5 idle-commit samples at K=32, W=128):
    `commit_idle(B) ≤ commit_idle(A) − max(64 MiB, 25 %·commit_idle(A))`; savings percentage numerator
    `A−B`, denominator `A`.
  - KILL-1 (R-live): `|Δp50| ≤ 5 %`, paired t, numerator `p50B−p50A`, denominator `p50A`; significant
    positive slowdown kills even inside 5 %.
  - KILL-2 (R-churn): `Δwall ≤ max(2 %, AAnoise)`, numerator `median wallB − median wallA`, denominator
    `median wallA`.
  - KILL-3 (R-wave): hard p99 `B ≤ 1.10 × A`, median of five per-run nearest-rank p99s, never pooled;
    numerator `p99B−p99A`, denominator `p99A`.
  In THIS Ir gate those axes stay **native Windows measurements and are DESCRIPTIVE** — except the
  deterministic part: the wave family's heap census (oracle: B serves the wave from the free set with zero
  new heaps, A bumps `N−1` new heaps) and its Ir difference per claim, reported descriptively.
- **Mimalloc control:** none. mimalloc has no heap-slot registry, so there is no analogue of a registry
  claim to run as a ΔIr control, and none is invented. Wherever a future input supplies mimalloc proxy
  pairs, the judge requires `ΔIr = 0` exactly (the PRF-03 exact-zero rule).
- **Native rear guard (method description only, DEFERRED):** per-thread first-call CPU time, K ≤ hardware
  thread count, failed-CAS counters — deferred to a quiet bare-metal host with nanosecond clocks. NOT
  launched in this phase. Wall-clock K64 is descriptive only, never a verdict input.

## Oracles

`examples/r20_prf04_claim_path_oracle.rs` includes the SAME cell bodies by path
(`benches/support/r20_prf04.rs`) and runs once per **(arm, family, size)** — start, cold, recycled and wave
at N = 16, 32, 64 on A and on B — reading `HeapRegistry::dbg_registry_claim_sources()`
length-agnostically (main `[u64;4]` = [hint, bump, cap_scan, recovery]; B appends `free_set`) and
`heaps_claimed_high_water`. Each record is bound by the runner to its arm's source identity, the oracle
binary's SHA256 and the environment, and the judge verifies that binding. Required over the measured phase
(N claims):

- start / cold: `bump_delta == N`, `reclaim_delta == 0`, `new_heaps == N`;
- recycled: `reclaim_delta == hint_delta == N`, `bump_delta == 0`, `new_heaps == 0`;
- wave on A: `hint_delta == 1`, `bump_delta == N−1`, `free_set_delta == 0`, `new_heaps == N−1`;
- wave on B: `hint_delta == 1`, `free_set_delta == N−1`, `bump_delta == 0`, `new_heaps == 0`;
- the arm label must agree with `claim_sources_len` (A = 4, B = 5) and `missing_fifth_explicit_zero ==
  (len < 5)`.

Any violation → **INVALID**; a missing or unbound record → **INCOMPLETE**.

## Verdicts, stopping, integrity

- **GO** — all judged checks pass and the noise ceiling holds;
  **NO-GO** — R-claim, R-construct or mimalloc violated;
  **INCONCLUSIVE** — layout noise above the ceiling;
  **INCOMPLETE** — missing or inconsistent data/identity/oracle;
  **UNJUDGEABLE** — a family or the start cell without its siblings: excluded from the other checks, listed
  in the table, never treated as a pass, and the gate verdict is then never GO;
  **INVALID** — recycled 4N non-linearity or an oracle path violation.
  Priority: INCOMPLETE > INVALID > NO-GO > INCONCLUSIVE > UNJUDGEABLE > GO. Non-GO exits nonzero.
- **Nothing is raised after an observation**: no threshold, budget, ceiling, window, family or arm
  definition changes after any number is seen. Later edits require an explicit re-freeze of the hashes
  below BEFORE measurement, never a retroactive adjustment.
- Commit types follow the R30-12 taxonomy (`bench` / `perf(runtime)` / `perf(opt-in)` / `docs`); this
  delivery is measurement-only.

## Not covered (stated, not hidden)

- Concurrency: contention and CAS losses on the free-slot set, hint/stale-summary races, saturation and
  capacity states, and reused heaps carrying a previous owner's state are not exercised by a single-thread
  Ir cell; they belong to the native axes and to the Loom model.
- A selector-only cell with an absolute Ir budget (finding 3) and a function-level Callgrind decomposition
  (full claim minus materialisation, with inlining and clone handling) are separate work.
- Ir does not cover cache footprint, RSS or latency.
- If PRF-01 changes sidecar layout or initialisation, the A-only characterization above no longer qualifies
  that combination; a new identity and a new preregistration are required.

## Deriver, runner, freeze

`scripts/r20_prf04_claim_path_gate.mjs` is the read-only JSON deriver; `scripts/r20_prf04_claim_path_run.mjs`
is the executor (`--prepare`, `--build`, `--oracle`, `--measure`, `--rehearsal`, `--cleanup`). `--input FILE`
emits tables and the verdict; non-GO exits 1; no arguments → INCOMPLETE. `--self-test` runs the synthetic
fixture with negative controls only (a synthetic GO is never printed as a real result). `--hashes` prints the
digests; `--check-hashes` compares the companion `R20_PRF04_CLAIM_PATH_PREREG.sha256`; `--input` verifies it
first and the runner refuses to start on drift.

The frozen set is the judge's `files` list, one SHA256 per raw file in that order: this document, the
judge, the runner, `docs/perf/R20_CONTROL_GEOMETRY_RULE.md`, `benches/r20_prf04_claim_iai.rs`,
`benches/support/r20_prf04.rs`, `examples/r20_prf04_claim_path_oracle.rs`, and the two A-only evidence logs
`docs/perf/_raw_r20_prf04_harness_characterization.log` and `docs/perf/_raw_r20_prf04_cold_profile.log`.
The companion excludes itself (no self-hash). Any later edit requires an explicit re-freeze before
measurement. The R19 run01/run02 numbers are never recomputed or reinterpreted.
