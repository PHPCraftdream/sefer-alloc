# R20 PRF-04 heap reuse (P4-1) — preregistration

**Status: UNMEASURED. Candidate: `TBD`.** The runner candidate identity is fixed in its own constant; preparation
must fail if unavailable. No candidate build or result is claimed. This preregistration records canonical exports
from `scripts/r20_prf04_heap_reuse_support.mjs`: `CELLS`, `N`, `ORACLE_COUNTERS`, `RELEASE_LAYER`, `BUDGETS`,
`FROZEN_FILES` and `validateActivation`. It maintains no duplicate inventory, values table or activation
implementation.

## 1. Scope and frozen owner decisions

A is the `origin/main` merge-base snapshot; B is the candidate source; C0 is an
isolated A layout control. No Gate Z. `ReuseFirst` is the default candidate
without a user policy API, Cargo feature, runtime flag, `heap_reuse_kind`, seen
mask, or custom `claim_as` API. Native latency/RSS/throughput remain deferred;
when deterministic axes pass and native evidence is deferred the verdict is
`GO (deterministic; native deferred)`, not a wall-clock claim.

`RELEASE_LAYER` is frozen at `L2`. A-only rehearsal requires nonzero Ir for first allocation and exact
`AbandonGuard::drop` symbol receipts. If guard-drop collection is unavailable, the owner re-freezes
`RELEASE_LAYER` at `L1` before B; release is then judged L1-only. The current L1 collector remains
unimplemented; matched inventory and CI are required (PARTIAL). The fallback receipt fields are
`reason`, `rehearsal`, `source`, `symbolReceipt`, `profileReceipt`, and `rawLogReceipt`, for example:

```text
{reason: 'guard-drop-uncollectable', rehearsal: true, source, symbolReceipt, profileReceipt, rawLogReceipt}
```

The runner supports `--rehearsal` and `--out-dir` and loads
`out-dir/fallbackReceipt.json` into the manifest/input; `source` is bound to the
base-head identity by gate verifiers. The shared r20 gate config currently
rejects their combination; CI configuration presently uses rehearsal without
out-dir. Rehearsal uses A only, does not access candidate, and cannot emit a gate verdict.

## 2. Identity, builds and evidence contract

A/B/C0 use isolated source snapshots/build directories inside the current
worktree/`target`, never `git worktree add`, cherry-pick, `git write-tree`, or
any git write. Capture source identity read-only with `git rev-parse HEAD` and
`git rev-parse HEAD^{tree}`. A/B candidate source identity does not include
new harness files: CI applies the frozen bench overlay with its own immutable
hash/manifest and must not label dirty A as clean source. Hash every executable
immediately before each launch, including each oracle and each individual
benchmark process. Bind stage receipts by `builds.manifestSha256`, source and
binary identities, `receiptHashes`, and out-dir. Preserve stdout/stderr,
arguments, environment, profile records, exit status and timestamps.

The collector preserves dispatcher `--iai-run` group and cell index. Boundary
receipts prove inclusive target-frame Ir, exact incoming call count, caller
context, effective toggle and `nm`/disassembly identity. No weakening when a
combined toggle cannot collect both sides: L2 recycled allocation and release
are separate named cells/profiles. Each verifies exactly N incoming allocation
calls and N guard drops in its own target profile; their combined cost is
judged only after both receipts validate.

## 3. Fixed cell inventory and entry points

The canonical `CELLS` array supplies all names, layer/family/N, group/index and
symbol metadata; both Rust group member lists must match it byte-for-byte. It
contains start, cold, recycled, wave, wave-release, L2 recycled
allocation/release, and L2 wave claim/release cells at N/2N/4N. N and wave
capacity are exported constants. L2 uses an installed real
`#[global_allocator] SeferAlloc`. L1 callgrind entry-point toggles name the
actual workload symbols (`hr::cold`, `hr::recycled`, `hr::wave_claim`, and
`hr::wave_release_measured`), not a broad default toggle. L2 allocation
profiles toggle the named `child_first_alloc`; release profiles toggle the
exact `AbandonGuard::drop` symbol, validated by A-only symbol receipts. No
literal `GUARD` toggle.

Scenario bodies are in the single canonical Rust support module. Bench bodies
contain no instrumentation reads. Oracle executables are separate and capture
activation immediately around the target allocation/claim; snapshots must not
allocate. The candidate observer pair uses the observed P4-2 claim.rs APIs: `dbg_free_slot_hint_popcount() -> u32`
and `dbg_registry_free_set_claims() -> u64`; the four existing source counters remain `dbg_registry_claim_sources()
-> [u64; 4]`. `heap_reuse_spill_bits_delta` is the signed free-slot-hint popcount difference across the window,
**not an exact spill-publication call count**; positive is a net increase. `heap_reuse_take_hits` is the unsigned
free-set-claim counter delta, computed with checked subtraction after monotonicity validation. In the L2 child
binding witness, the sum of the four source-counter deltas plus free-set-claim delta must equal one, covering
claims not included in the four-source breakdown. Candidate-only observations are available only in the B oracle
cfg. A emits `observer=false` and null candidate counters, not measured zero. B activation: recycled take hits are
zero; wave take hits are positive; spill-release has positive net bit delta during its release-only,
non-concurrent-claim window; cold is zero. No `src/**` changes are in this task.

`validateActivation` is the canonical activation validator. Missing candidate
observer is not a zero measurement. `config_conflicts_total` delta must be zero.

## 4. Budgets, paths and verdict

Canonical values are exported as `BUDGETS`: cold/pair +4, spill-release +14,
start `T_s+4`, L2 +3, relative ceiling 5%, absolute noise 10. `T_s` is the
qualified A/C0 noise and never widens a budget. If an absolute noise difference
exceeds 10, that window is INCONCLUSIVE. Fixed work budgets are one-sided; the
relative bound is exact integer and negative deltas pass. Wave benefit requires
B<A plus positive take activation. Cold and wave use 0→N, N→2N, 2N→4N windows;
recycled uses claim/release pair slopes and linearity; spill-release uses
N/2N/4N release-only slopes and linearity.

Priority: INCOMPLETE > INVALID > NO-GO > INCONCLUSIVE > UNJUDGEABLE > GO.
Missing identity/hash/profile/count/cell/oracle is INCOMPLETE; incorrect path,
activation or receipts are INVALID; exceeded fixed budget is NO-GO. Complete
deterministic pass with native axis deferred is `GO (deterministic; native deferred)`.

## 5. Rehearsal and frozen inputs

`CANDIDATE='TBD'` is a runner-local constant, not a support-module export. The ordered
`FROZEN_FILES` list has ten files, including this companion preregistration. Its
`.sha256` companion is excluded from its own inventory. Freeze it with the
companion hash manifest before measurement.
