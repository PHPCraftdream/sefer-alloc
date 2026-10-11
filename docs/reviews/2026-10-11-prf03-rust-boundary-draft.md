# PRF03 harness/runner review — DRAFT (A source only)

Not a featureExactReceipt and not acceptance evidence. B source was never read by
this worker. Independent owner review must cover both arms and exact retained
source/binary/raw linkage before real GO. No runtime speedup is claimed.

## A cache-exit table

Read src/alloc_core/large/alloc_core_large.rs, alloc_large lines 170–523.
Signature order: occupied-slot delta, reserve-count delta, release-count delta.
Successful slow fallback is distinguished from null/OOM alternatives.

| Exit | Source | Signature |
|---|---|---|
| needed overflow before cache | 193–196 | (0,0,0), null; no slot taken |
| alignment bypass/no compatible slot, successful slow allocation | 250, 519–522 | (0,+1,0) |
| header generation exhausted, successful slow fallback | 283–299 | (-1,+1,+1) |
| register_payload fails, successful slow fallback | 456–470 | (-1,+1,+1) |
| successful selected-slot reuse | 488–518 | (-1,0,0), nonnull cached pointer |

The two selected-slot fallback exits can fail slow allocation instead: slot -1,
release +1, reserve depends on failure stage; returned null is rejected. Invalid
terminal transitions abort at 295/467. Only the successful hit combines slot -1,
reserve/release 0, same cached pointer. Decay releases would break the signature.
R2 is unchanged: 41 allocations/frees, eight tail hits, 33 tail reserves/free
evictions — eviction-dominated 33/41, not zero-hit. R3 requires the same root and
eight changed offsets over its nine-allocation sequence.

## Feature transfer and unsafe invariants

Read large_cache_config.rs entirely: DEFAULT/new and policy resolvers contain no
bench-internals/internals conditional resolution. Budget branches only on
large-cache-extended, excluded from these builds. Lifecycle construction lines
88–290 resolve DEFAULT through new_with_config_and_owner; budget/decay/mode/pool
assignments have alloc-decommit gates, not diagnostic gates. Diagnostic owner-ID
counter does not determine policy. Cargo internals=[]; bench-internals forwards
aligned-vmem bench-internals/lazy-commit; production already enables lazy-commit.
Actual build receipts must still record the transitive diagnostic forwarding.

Native/twin use the same source and installed SeferAlloc::new(). Twin slots8
proves Own before None budget means unbounded. Native cache observations stay
null and require immediately preceding same-arm twin provenance. Exact UTF-8
Debug DEFAULT text is hashed without quotes/newline. No new hash dependency.
Exit barrier keeps worker TLS heaps alive through done snapshot. Public counters
prove claims N, ready/done reserve/release0, conflict/fallback0, maintenance off.
A worker panic before a barrier still needs the external process timeout.

Unsafe sites have SAFETY comments: oracle System allocation/free, same-layout
HeapCore free and direct GlobalAlloc calls; native installed allocation and
initialized first-byte volatile write/read followed by same-layout free. The
owner-only probe is not Send/Sync, forms no borrowed span references, and drops
registrations before census/System free. Sentinel owner is nonexistent. No new
library unsafe sites or hooks. Bounded rust-intel unsafe/testing/semantics review;
not a full delegated multi-module audit.

## Current CLI and pipeline

```text
node scripts/r19_prf03_large_route_retain_run.mjs --self-test
node scripts/r19_prf03_large_route_retain_run.mjs --run OUTDIR --arm-a A --arm-b B [--work COUNT] [--review-receipt FILE]
node scripts/r19_prf03_large_route_retain_run.mjs --aggregate OUTDIR [--review-receipt FILE]
```

--out DIR is an alternative to positional OUTDIR, not permitted together.
Default work=8192 is explicitly recorded; --work must be positive/divisible by8.
Injectable Windows WSL bash-lc transport: --transport-executable EXE and
--transport-prefix-json JSON. Unknown/conflicting options are rejected.
Future explicit arms may name ROOT or other authorized clean Git worktrees;
output may be outside ROOT. This worker read only its authorized worktree.

Clean preflight rejects tracked patches/untracked inputs, captures relevant
ignored build inputs and exact unfiltered source bytes before builds. All role
binaries are hashed/retained before observations. Targets are
 target/gate-prf03-<arm>-<role>; C0 uses identical flags and fails closed if bytes
match A. Compiler/profile/runner environment overrides are rejected. CPU identity
uses stable model/topology/ISA fields, not changing MHz.

PRIMARY probes; paired regime oracles; complete A/B primary/paired R1/R2/R3
negative matrix. R1 miss outputs valid data; R2 skip-tail and R3 unchanged-offset
must fail specific activation assertions, not unrelated CLI errors. Raw objects,
logs, envelopes and derived iai/native rows are persisted exclusively as each
matched observation becomes valid, before final bundle assembly.

Native/twin pairs are adjacent and alternating. Aggregate rechecks raw bytes,
configuration/provenance, debug SHA, structural equality, completeness, unique
PID/runId, source cleanliness and all-capture ordering. receiptSha256 is canonical
raw-receipt hash. A-only note bytes/hash plus A source hash enter reviewed linkage.
DRAFT blocks GO; reviewed receipt must attest inventory/cache exits/feature
transfer and match all source/binary/raw hashes exactly. Never fake reviewed status.

## Actual iai path and verification limits

Future --run invokes each hashed bench normally once per A1/B1/A2/B2/C0 with unique
--home/--save-summary=json; installed iai executes the full group. Only frozen
inventory enters the judge; unrelated outputs retained as audit. Observation
homes use target/gate-prf03-<arm>-iai-observations-<id>. Per-cell startedAt is the
collector's group-launch boundary, not a fabricated exact OS child start time;
it is after every capture/build. PID comes from retained ==PID== log and must
match profile pid. Actual emitted Instructions must match parsed profile Ir.

Locked runner 0.14.2 source: mod.rs 244/283 filters file meta.bench_name;
lib_bench.rs 459–467 dispatches group/index/module; mod.rs38 default wrapper toggle;
tool/mod.rs549–578 names exact cell directory, 773–779 filenames;
callgrind/args.rs87–103 ignores user log-file overrides (runner sets its own).
Profile parser uses totals first, summary fallback, not equality between them.
No unused direct iai executor or fake-consistency branch remains.

Authorized one-cell mimalloc_bootstrap_proxy raw Callgrind smoke passed genuine
log/profile PID agreement and actual wrapper-symbol shape. Scratch files removed;
no instruction/timing value retained/cited. Normal full-group integration was not
run during this task and remains for independent execution. No real gate GO.

## Checks and scope

Prior sequential --locked release primary/paired/native/twin builds and tiny A
smokes passed; R2/R3 rejection controls passed. Rustfmt passed. Authorized bench
--no-run build passed; proc-macro-error2 2.0.1 emitted existing future-incompat
warning. No Rust changes in runner correction chunks.

Synthetic tests use the actual pipeline/aggregate/judge and fake output transport,
plus real clean mini Git trees/index/loose zlib objects constructed through fs.
Only read-only Git commands; temporary fixtures are removed. Rehashed semantic
mutants repair raw/envelope/process-stdout linkage before validation; direct
validator controls isolate semantics. Pipeline controls test C0-identical actual
bytes, build-time source drift, unrelated actual ancestry, config absence,
wrong rejection error and failed transport. No hash-only oracle claims.

Final Node selftest/hash/judge/syntax receipts are reported by the worker's final
boundary message. Full cargo test/npm check/powerset/clippy/real gate/full iai
runs were not performed by this worker. No lock/version/frozen files/src edits,
new example/test targets beyond the two named, other-worktree reads or project Git writes;
exactly one authorized worker sub-agent performed the edits and its diff was re-checked.
