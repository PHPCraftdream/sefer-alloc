# R20 PRF-04 claim-path Ir gate — preregistration (phase: before any B build)

**Status: UNMEASURED for the gate.** No B build exists, no gate measurement was
taken, no runtime change, no verdict and no runtime policy exist in this phase.
Only A-only *harness-validity* runs were made (they decided the cell design, see
"Harness characterization"); no threshold below was chosen or changed by looking at
a B number, and no B number exists.
The historical R19 PRF-04 records (`R19_PRF04_B_VS_A_GATE.md`, run01/run02)
are **NOT recomputed and NOT reinterpreted** by this gate; the run02 NO-GO
stands exactly as retained.

## Authority

Owner decision 2026-10-12: «делаем как @ox рекомендовал», implemented per
`docs/reviews/2026-10-12-prf-open-questions-reviewer-answers.md` §4. The V4
direction is accepted, but the first reviewer's KILL-1 estimator (320 ABBA
blocks per mode) is **REJECTED**: against the committed run02 data the block
SD is 0.97–1.33 versus the required ≤ 0.20, giving 2.0–3.5 % power at n=320
(needing ≈ 6 310–11 860 blocks per mode). The new primary KILL-1 is the
**deterministic Ir of the claim path** — this gate. The native axes (live
latency, churn wall, wave p99) become descriptive and are deferred to a quiet
bare-metal host.

## Entry point and why

The measured path is **`HeapRegistry::dbg_claim_lease()`** — the `internals`-gated
safe observer whose body is exactly `claim_lease()` (`claim_impl(HeapCore::new,
|_| {})`, `src/registry/heap_registry/claim.rs`), the call `tls_heap::bind_slow_tagged`
makes on a thread's first allocating call; `HeapLease::drop` is the thread-exit
release (LIVE → FREE plus `reuse_hint`). PRF-04 (V4/V4′) changes only code inside
that claim/release path, so this is the layer the decision applies to; the TLS
wrapper around it (`finish_bind`, the abandon guard) is untouched by the candidate
and constant across arms. This is **not** the `#[global_allocator]` layer: a cell
at that layer runs the claim on a spawned thread, and callgrind records **0 Ir** in
worker-thread output files (only the thread that enters the benchmark function is
collected — verified, see below), so such a cell would measure spawn/join cost and
miss the claim entirely. The true thread/`GlobalAlloc` path stays a **descriptive
native** measurement (deferred, below).

Feature sets: the Ir cells are built with `production internals` (no
`bench-internals`: the claim-source counters and the `live_config_matches` probe exist
only under it and would change the measured path). The path oracle is a separate
example built with `production bench-internals internals`; path selection does not
depend on the counters. iai-callgrind runs every `#[library_benchmark]` cell in a
**fresh process**, so cross-cell state never accumulates and the registry starts empty.

## Harness characterization (A-only validity runs, not gate data)

Two facts were established on arm A alone, before any B build existed, and they fix
the design below:

1. **Worker threads are not collected.** A first design (spawned threads calling
   `SeferAlloc::alloc`) was run once under iai-callgrind: the per-thread callgrind
   files `t2..tN` all report `summary: 0`; the reported Instructions equal the main
   thread's file. That design measured `pthread_create`/`join` and not the claim, so
   it was discarded.
2. **Cold cost is not affine in the claim count.** `cold_first_claim(k)` for k =
   1..40, 44, 48, 56, 60, 62, 63, 64 shows a period-4 cost pattern (three claims of ≈ 47.1 k
   Ir, then one of ≈ 8–11 k Ir; within a run of periods the cheap one grows by 704 Ir per period). The profile
   attributes ≈ 86 % of the cold cell to `RouteDirectory::register` → glibc `calloc` →
   `memset`: `calloc` skips the `memset` when the arena top is freshly extended, so the
   cost follows the host allocator's growth pattern, deterministically per binary but
   not linearly in k. The recycled family is exactly affine (3 227 Ir per claim+release
   between every probed pair among k = 1, 2, 4, 8, 16, 32, 64). The raw per-k output is committed as
   `docs/perf/_raw_r20_prf04_harness_characterization.log` (the one-off probe bench was
   not committed; each row is `cold_first_claim(k)` / `recycled_first_claim(k)`, which
   the committed cells call at k = 16, 32, 64).

Consequence for sensitivity (stated before any B number): the recycled operation costs
about 3.2 k Ir, so the 5 % owner budget is about 161 Ir per claim+release; the cold
operation costs about 37.5 k Ir, so 5 % is about 1.9 k Ir per claim. The gate bounds the
**user-visible cost of a claim**; it cannot resolve a selection-logic delta smaller than
those budgets. That is the owner's budget, not a gate defect, and it is recorded here so
a GO is not read as "the selection logic is free".

## Family design

- **cold** — `cold_first_claim(k)`: k leases are claimed and `mem::forget`ten, so every
  slot stays LIVE, no `reuse_hint` ever points at a FREE slot, and every claim is an
  EMPTY-first-claim bump with a fresh `HeapCore::new`.
- **recycled** — `recycled_first_claim(k)`: one warmup claim+drop materialises a slot and
  leaves it FREE with `reuse_hint` pointing at it; each of the k measured iterations
  claims it back through the reuse path and drops it again (claim and release are both
  inside the slope). The warmup count K is identical in the N/2N/4N cells, so it cancels
  in the slope.

Both run on the collected thread. The main thread of the iai process has no heap bound
(the system allocator is global in the bench), so the registry sees exactly the cell's
claims.

## Choice of N and K, and why

N = 16 claims, 2N = 32, 4N = 64; warmup K = 1 claim+drop for the recycled family. All
three sizes lie inside the first 64-slot registry chunk (`CHUNK_SLOTS = 64`), far below
`MAX_HEAPS = 4096`. The slope window spans 16 claims, far above the 10-Ir floor of
`R20_CONTROL_GEOMETRY_RULE.md`. These constants were fixed before any B build existed.

## Arches

- **A** = main `8d6ae58d` + this harness (this worktree's gate additions).
- **B** = A + cherry-pick `6951a9d0` (branch `r20-prf04-v4prime`: V4, free_set
  on its own cache line).
- **C0** = A built from the **same source identity** under a different build
  path label (layout noise), following PRF-03 Amendment A3's equal-paths rule:
  the FULL worktree paths of the A and B runs have equal length, and the
  build/observation directories use one-letter labels (A, B, C).
- **A1/A2** = two runs of ONE A binary; **B1/B2** = two runs of ONE B binary.
- Identity before any measurement: `git write-tree` tree SHA of the working
  parties A and B, plus SHA256 of this prereg and of the judge script.
  Commits are NOT manufactured.

## Frozen criteria (frozen before B exists)

Notation per family F ∈ {cold, recycled} and arm X: `Ir_X(k)` = the raw
Instructions of cell `r20_prf04_F_first_claim_k`, `s_X = Ir_X(2N) − Ir_X(N)`,
`K_X = Ir_X(N) − s_X`.

- **(R-claim) primary KILL-1 — one-sided slope neutrality, per family:**
  `s_B − s_A ≤ +5 % × s_A` (numerator `s_B − s_A`, denominator `s_A`) for
  EVERY pair (B1,B2) × (A1,A2) and every window of the family (one for recycled, two for
  cold). The 5 % budget is an owner decision, expressed
  in relative Ir and frozen here. KILL (NO-GO) if any pair in either family
  exceeds it. Speedup (`s_B < s_A`) is never punished: one-sided means only an
  excess kills, a reduction proves nothing and harms nothing. If `s_A ≤ 0` or
  any value is non-finite → INCOMPLETE.
- **T_s** `= max(10, |s_A1 − s_A2|, |s_C0 − s_A1|)` per
  `docs/perf/R20_CONTROL_GEOMETRY_RULE.md`. Because T_s contains the A/A term
  by construction, the A/A agreement rule is enforced against the C0-based
  floor: `|s_A1 − s_A2| ≤ max(10, |s_C0 − s_A1|)`, otherwise **INCONCLUSIVE**
  (a wide A/A never becomes GO). A wide A/A still widens T_s for the
  linearity and construction checks — conservative, never permissive. The
  owner's 5 % R-claim budget is NEVER widened by noise.
- **Windows (rule §2.3a).** *recycled* is affine and uses rule §2.3 unchanged:
  `|(Ir(4N) − Ir(2N)) − 2·s| ≤ T_s` for every arm, violation → the family is **INVALID**
  (no verdict, never a pass). *cold* is known from the A-only characterization (identified
  mechanism: glibc `calloc` skip, above) not to be affine, so the affine test is replaced,
  declared here before any B build, by **two disjoint windows**:
  `w1 = Ir(2N) − Ir(N)` (claims 17–32) and `w2 = Ir(4N) − Ir(2N)` (claims 33–64). Every
  neutrality check below (R-claim and A/A) is applied to **each** window with that window's
  own `T_s`; a violation in either window decides. The construction cell uses window 1.
  `T_s` for window `k` is `max(10, |w_k(A1) − w_k(A2)|, |w_k(C0) − w_k(A1)|)`.
- **(R-construct)** rule §2.4: `K_B − K_A ≤ budget_K` with
  **`budget_K = T_s`**, checked for every (B1,B2) × (A1,A2) pair per family.
  Justification of the "0 beyond noise" budget: candidate B only moves the
  `free_set` field onto its own cache line inside the existing heap-slot
  structure and by construction adds **not one new operation** to the
  construction / first-claim path; any super-noise construction increase means
  unplanned work and requires investigation, not a budget.
- **(R-benefit)** the usefulness criteria are carried over **UNCHANGED, verbatim
  thresholds**, from `docs/perf/R19_PRF04_GATE2B_PREREG.md` §Owner thresholds:
  - BENEFIT (median of 5 idle-commit samples at K=32, W=128):
    `commit_idle(B) ≤ commit_idle(A) − max(64 MiB, 25 %·commit_idle(A))`;
    savings percentage numerator `A−B`, denominator `A`.
  - KILL-1 (R-live): `|Δp50| ≤ 5 %`, paired t, numerator `p50B−p50A`,
    denominator `p50A`; significant positive slowdown kills even inside 5 %.
  - KILL-2 (R-churn): `Δwall ≤ max(2 %, AAnoise)`, numerator
    `median wallB − median wallA`, denominator `median wallA`.
  - KILL-3 (R-wave): hard p99 `B ≤ 1.10 × A`, median of five per-run
    nearest-rank p99s, never pooled; numerator `p99B−p99A`, denominator `p99A`.
  In THIS Ir gate those axes stay **native Windows measurements and are
  DESCRIPTIVE** — except the deterministic part: the wave's heap-claim census
  is checked by the oracle example (cold: exactly N bumps, zero reclaims,
  high-water +N; recycled: all N reclaims, zero bumps, high-water +0).
- **Mimalloc control:** none. mimalloc has no heap-slot registry, so there is no
  analogue of a registry claim to run as a ΔIr control, and none is invented (the
  existing mimalloc twins in `benches/perf_gate_iai.rs` are allocation-churn cells,
  not claim cells). Wherever a future input supplies mimalloc proxy pairs, the judge
  requires `ΔIr = 0` exactly (the PRF-03 exact-zero rule). No mimalloc bootstrap proxy
  is declared for this gate.
- **Native rear guard (method description only, DEFERRED):** per-thread
  first-call CPU time, K ≤ hardware thread count, failed-CAS counters —
  deferred to a quiet bare-metal host with nanosecond clocks. NOT launched in
  this phase. Wall-clock K64 is descriptive only, never a verdict input.

## Oracles

`examples/r20_prf04_claim_path_oracle.rs` includes the SAME cell bodies by path
(`benches/support/r20_prf04.rs`), runs per family at N=16, and reads
`HeapRegistry::dbg_registry_claim_sources()` length-agnostically (main
`[u64;4]` = [hint, bump, cap_scan, recovery]; branch B `6951a9d0` appends
`free_set` fifth) plus `heaps_claimed_high_water`. Required:

- cold: `bump_delta == 16 && reclaim_delta == 0 && new_heaps == 16`
  (high-water +N);
- recycled: `reclaim_delta == 16 && bump_delta == 0 && new_heaps == 0`
  (high-water +0 in the measured phase);
- `claim_sources_len` ∈ {4, 5} and
  `missing_fifth_explicit_zero == (claim_sources_len < 5)`;
- `environment`, `binarySha256`, `source` identity fields and
  `capturedAt < startedAt` as in PRF-03.

Violation of an oracle assert → the family is **INVALID**.

## Verdicts, stopping, integrity

- **GO** — all judged checks pass and A/A is inside noise;
  **NO-GO** — R-claim, R-construct or mimalloc violated;
  **INCONCLUSIVE** — A/A noise too wide;
  **INCOMPLETE** — missing or inconsistent data/identity;
  **UNJUDGEABLE** — a family without its 2N sibling: excluded from the
  other checks, listed in the table, never treated as a pass, and the gate
  verdict is then never GO (it is UNJUDGEABLE unless a worse verdict applies);
  **INVALID** — recycled 4N non-linearity or an oracle violation.
  Priority: INVALID / NO-GO over INCONCLUSIVE over GO; INCOMPLETE data rules
  everything out first. non-GO exits nonzero.
- **Nothing is raised after an observation**: no threshold, budget, family or
  arm definition changes after any number is seen. Later edits require an
  explicit re-freeze of the hashes below BEFORE measurement, never a
  retroactive adjustment.
- Commit types follow the R30-12 taxonomy (`bench` / `perf(runtime)` /
  `perf(opt-in)` / `docs`); this delivery is measurement-only.
- Source identity before any measurement: `git write-tree` tree SHA of the A
  and B working parties + SHA256 of this prereg and the judge script; no
  commits are manufactured. R19 run01/run02 numbers are never recomputed or
  reinterpreted.

## Deriver

`scripts/r20_prf04_claim_path_gate.mjs` is a read-only JSON deriver, NOT a
runner. `--input FILE` emits tables and the verdict; non-GO exits 1. No
arguments → INCOMPLETE. `--self-test`/`--selftest` runs the synthetic fixture
with negative controls only; a synthetic GO is never printed as a real result.
`--hashes` prints the doc+script digests; `--check-hashes` compares the
companion `R20_PRF04_CLAIM_PATH_PREREG.sha256` (doc then script, companion
excluded); `--input` verifies the companion first. Schema `version: 1`:
`hashes`, `environmentSha256`, `inventoryReceipt`, `oracleReceipt`,
`cells` (name/family/size/arm/ir/receipt/identity fields), optional
`mimalloc` rows, `oracle` per family.

## Freeze hashes (no self-hash)

Companion `R20_PRF04_CLAIM_PATH_PREREG.sha256` holds the SHA256 of the raw
document bytes and raw script bytes, in that order. Recipe:
`node scripts/r20_prf04_claim_path_gate.mjs --hashes`. Any later edit requires
an explicit re-freeze before measurement.
