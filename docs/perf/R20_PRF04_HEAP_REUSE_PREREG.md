# R20 PRF-04 heap reuse (P4-1) — preregistration

**Status: UNMEASURED. Candidate: `TBD`.** The runner candidate identity is fixed in its own constant; preparation
must
fail if unavailable. No candidate build or result is claimed. This preregistration records canonical exports
from
`scripts/r20_prf04_heap_reuse_support.mjs`: `CELLS`, `N`, `ORACLE_COUNTERS`, `RELEASE_LAYER`, `BUDGETS`,
`FROZEN_FILES`
and `validateActivation`. It maintains no duplicate inventory, values table or activation
implementation.

## 1. Scope and frozen owner decisions

A is the `origin/main` merge-base snapshot; B is the candidate source; C0 is an isolated A layout control. No Gate Z.
`ReuseFirst` is the default candidate without a user policy API, Cargo feature, runtime flag, `heap_reuse_kind`, seen
mask, or custom `claim_as` API. Native latency/RSS/throughput remain deferred; when deterministic axes pass and native
evidence is deferred the verdict is `GO (deterministic; native deferred)`, not a wall-clock claim.

`RELEASE_LAYER` remains frozen at `L2`. Guard-drop-only `L1` fallback does not waive missing first-bind receipts. If new
target symbol/count/bind-path collection fails, L2 allocation evidence is INVALID/INCOMPLETE before B; there is no
automatic L1 downgrade. The current L1 collector remains unimplemented; matched inventory and CI are required (PARTIAL).
Any fallback receipt has fields `reason`, `rehearsal`, `source`, `symbolReceipt`, `profileReceipt`, and `rawLogReceipt`,
for example:

```text
{reason: 'guard-drop-uncollectable', rehearsal: true, source, symbolReceipt, profileReceipt, rawLogReceipt}
```

The runner supports `--rehearsal` and `--out-dir` and loads `out-dir/fallbackReceipt.json` into the manifest/input;
`source` is bound to the base-head identity by gate verifiers. The shared r20 gate config currently rejects their
combination; CI configuration presently uses rehearsal without out-dir. Rehearsal uses A only, does not access
candidate, and cannot emit a gate verdict.

## 2. Identity, builds and evidence contract

A/B/C0 use isolated source snapshots/build directories inside the current worktree/`target`, never `git worktree add`,
cherry-pick, `git write-tree`, or any git write. Capture source identity read-only with `git rev-parse HEAD` and `git
rev-parse HEAD^{tree}`. A/B candidate source identity does not include new harness files: CI applies the frozen bench
overlay with its own immutable hash/manifest and must not label dirty A as clean source. Hash every executable
immediately before each launch, including each oracle and each individual benchmark process. Bind stage receipts by
`builds.manifestSha256`, source and binary identities, `receiptHashes`, and out-dir. Preserve stdout/stderr, arguments,
environment, profile records, exit status and timestamps.

The collector preserves dispatcher `--iai-run` group and cell index. Boundary receipts prove inclusive target-frame Ir,
exact incoming call count, caller context, effective toggle and exact `nm`/disassembly identity. No weakening when a
combined toggle cannot collect both sides: L2 recycled allocation and release are separate named cells/profiles. Each
verifies exactly N incoming allocation calls and N guard drops in its own target profile; their combined cost is judged
only after both receipts validate.

## 3. Fixed cell inventory and entry points

The canonical `CELLS` array supplies all names, layer/family/N, group/index and symbol metadata; both Rust group member
lists must match it byte-for-byte. It contains start, cold, recycled, wave, wave-release, L2 recycled
allocation/release, and L2 wave claim/release cells at N/2N/4N. N and wave capacity are exported constants.

L2 uses installed `SeferAlloc` behind an identical bench-only forwarding `GlobalAlloc` wrapper in A/B/C0.
`FirstAlloc<false>` is the bench wrapper and `FirstAlloc<true>` the oracle. Its TLS const `Cell` one-shot latch is
consumed before forwarding and armed only after parent warmup. Named `#[inline(never)] hr::first_global_alloc` wraps
real `SeferAlloc::alloc` or `alloc_zeroed`, including bind, and captures child std startup allocation before
user-closure entry; it replaces the old child-first-allocation toggle. Canonical target:
`r20_prf04_heap_reuse_l2_iai::hr::first_global_alloc` for cold, recycled allocation, and wave. Its strict incoming
caller is `r20_prf04_heap_reuse_l2_iai::hr::FirstAlloc<_>::alloc_inner`, never the target itself or an arbitrary
closure. Run/gate/verifier and fixtures enforce this boundary. L1 callgrind toggles actual workload symbols (`hr::cold`,
`hr::recycled`, `hr::wave_claim`, `hr::wave_release_measured`), not a broad default toggle. Release profiles toggle
the exact TLS destructor `std::sys::thread_local::native::eager::destroy::<sefer_alloc::global::tls_heap::AbandonGuard>`:
`AbandonGuard::drop` is inlined into it and its out-of-line copy is never called (rehearsal disassembly), while
the destructor itself is registered by address (`lea`) and run by `__call_tls_dtors`. No literal `GUARD` toggle.

Scenario bodies are in the single canonical Rust support module. Bench bodies contain no instrumentation reads. Oracle
executables are separate and capture activation immediately around target allocation/claim; snapshots must not allocate.
The candidate observer pair uses observed P4-2 claim.rs APIs: `dbg_free_slot_hint_popcount() -> u32` and
`dbg_registry_free_set_claims() -> u64`; the four source counters remain `dbg_registry_claim_sources() -> [u64; 4]`.
`heap_reuse_spill_bits_delta` is signed free-slot-hint popcount difference across the window, **not an exact
spill-publication call count**; positive is net increase. `heap_reuse_take_hits` is unsigned free-set-claim counter
delta, computed with checked subtraction after monotonicity validation. Each child must have exactly one passive TLS
target-call receipt; source-counter deltas plus free-set-claim delta must sum exactly one. Each actual target profile
must independently show dynamic descent through `tls_heap::bind_slow_tagged` or `_with_config` and registry claim;
warm-only descendants do not qualify. Candidate-only observations are available only in B oracle cfg. A emits
`observer=false` and null candidate counters, not measured zero. B activation: recycled take hits are zero; wave take
hits are positive; spill-release has positive net bit delta during its release-only, non-concurrent-claim window; cold
is zero. `config_conflicts_total` delta must be zero. No `src/**` changes are in this task.

`validateActivation` is the canonical activation validator. Missing candidate observer is not a zero measurement.

The wrapper and latch overhead are shared identically by A/B/C0 and included in calibrated noise; no wrapper overhead is
subtracted or attributed to the allocator. std startup input/layout may differ from the explicit 8-byte workload. The
old profile window excluded startup; the exact std startup allocator frame was not independently proven locally
(rust-src unavailable), so no cause is asserted.

## 4. Budgets, paths and verdict

**Version 2 (2026-10-15, before any v2 measurement) supersedes the gating rules of this section; see §7.**

Canonical values are exported as `BUDGETS`: cold/pair +4, spill-release +14, start `T_s+4`, L2 +3, relative ceiling 5%,
absolute noise 10. `T_s` is qualified A/C0 noise and never widens a budget. If an absolute noise difference exceeds 10,
that window is INCONCLUSIVE. Fixed work budgets are one-sided; the relative bound is exact integer and negative deltas
pass. Wave benefit requires B<A plus positive take activation. Cold and wave use 0→N, N→2N, 2N→4N windows;
recycled uses
claim/release pair slopes and linearity; spill-release uses N/2N/4N release-only slopes and linearity.

Priority: INCOMPLETE > INVALID > NO-GO > INCONCLUSIVE > UNJUDGEABLE > GO. Missing
identity/hash/profile/count/cell/oracle is INCOMPLETE; incorrect path, activation or receipts are INVALID; exceeded
fixed budget is NO-GO. Complete deterministic pass with native axis deferred is `GO (deterministic; native deferred)`.

## 5. Rehearsal and frozen inputs

`CANDIDATE='TBD'` is runner-local, not a support-module export. The ordered `FROZEN_FILES` list has ten files, including
this companion preregistration. Its `.sha256` companion is excluded from its own inventory. Freeze it with the companion
hash manifest before measurement.

## 6. Re-freeze log

- **2026-10-15 — pre-measurement re-freeze 1 (P4-3 dispatch binding).** B candidate = `942360b96bda0f159e49bb9dd05c7728bd5b10a8` (PR #28 head: the P4-2 ReuseFirst hint-spill candidate merged with `main`). A = `merge-base(B, origin/main)` = `3cc870a9b09353a96cf8c5250d9f479936f5e954` (`main` with this harness, PR #30). The runner binds B through its frozen `CANDIDATE` constant (no dispatch argument), and `docs/perf/R20_PRF04_HEAP_REUSE_PREREG.md.sha256` freezes the ten `FROZEN_FILES` at this commit. No candidate build or measurement precedes this entry; the earlier rehearsal was A-only harness validation.
- **2026-10-15 — official dispatch record, run01 (outcome).** Run 38652419070 (PR #28 head `3cd84197`, B = `942360b9`, A = `3cc870a9`) wrote official input. It returned **INVALID**, with flags invalid and noGo both set.
  - INVALID came from the absolute-arm linearity rule, which failed identically in every arm, A and C0 included: L1 wave_release 43, L2 recycled 487, L2 wave_release 2384, against T_s 10.
  - NO-GO came from the per-work allowances:
    - L1 cold +160 / 16 claims;
    - L1 recycled +112 / 16;
    - L1 wave_release +208 / 16, which breaks the relative 5% bound of its 460-Ir window;
    - L2 cold and L2 recycled +144 / 16.
  - The wave benefit passed strictly: L1 wave 558283 → 13026 Ir, L2 wave 740979 → 82165 Ir.
  - The result stands as the v1 record. Data: Actions artifact `r20-gates-28` of that run.

## 7. Version 2 (2026-10-15)

On 2026-10-15 the owner decided to improve the protocol rather than keep rules that measure something other than the decision. v2 is a new protocol version. It was written after run01, and this section says so openly. Its rules follow from two principles that do not depend on the candidate's numbers.

1. **Price is judged at the shipping layer.**
   - The candidate ships as the claim/release step of the real `#[global_allocator]` thread lifecycle: TLS bind, registry claim, thread exit. The L2 cells measure exactly that step.
   - L1 calls `HeapRegistry::dbg_claim_lease` directly, below that layer. L1 windows are therefore kept as diagnostics with their v1 per-work allowances and never set the verdict (the CLAUDE.md entry-point rule).
   - The gating L2 rule is `100 * (delta - T_s) <= BUDGETS.lifecyclePercent * base`, with `lifecyclePercent = 1`: an added fixed cost may not exceed 1% of the A lifecycle window it is paid in.
2. **Linearity is a property of the candidate's added cost, not of the allocator.**
   - The run01 profiles show that the absolute-arm nonlinearity is shared allocator state: per-heap `flush_class_internal`, `drain_sidecar_ingress` and `trim_cold_retention` work falls as N grows, identically in A, B and C0.
   - The L1 wave_release harness also front-drained its lease vector, and that memmove shrinks as n grows.
   - v2 gates `|(B_2N - A_2N) - 2 * (B_N - A_N)| <= T_s` for each B/A pair. Absolute-arm linearity is kept as a diagnostic.

Unchanged: identity, receipts, the activation oracle, the start check, the strict wave benefit and the priority INCOMPLETE > INVALID > NO-GO > INCONCLUSIVE > UNJUDGEABLE > GO.

Changes made before the v2 measurement:
- `wave_release_measured` and `l2_wave_release_measured` now drain the tail of their vectors, so no memmove happens inside the window.
- The candidate's bench-only claim-source counter is branch-free: `CLAIM_SOURCES` gained a fifth slot for FREE-set claims.

Disclosure: running the v2 judge offline on the run01 input gives GO. That is a consistency check, not gate evidence. Only an official v2 run, re-frozen with the new candidate SHA, decides.
