# R19-PRF02-R preregistration (before new measurements)

Status: REVIEW ACCEPTED for Stage C activation ONLY; IAI is not authorized.
No real measurement has started. Historical preparation status: no PRF02R cargo,
activation execution or measurement had been run during preparation. A was reconstructed by read-only git show;
its hashes and backup snapshots were verified, NOT independently executed.
B runtime must remain byte-exact to original identity `arms.B`.
Original receipts and original scripts remain historical evidence, not R receipts.

## Fixed decision protocol

Two axes, two repeats of each A/B arm, sequential WSL only. Pure features:
`production internals`; instrumented: `production internals bench-internals`.
Compare within axes, never across axes. First absolute Instructions column only.
Both repeats must match all required raw counts (including diagnostic bodies).
Any prediction/count/path/repeat/control discrepancy is final **NO-GO**; do not
repredict, tune B or selectively discard samples. Failed activation forbids iai.

- push = n9 - n8; B-A pure push <= -6 Ir.
- B-A instrumented push <= -8 Ir; churn = (2N-N)/64, <= -8 Ir/pair.
- Large raw one-sided B-A Delta Ir <= +10:
  **переписана после наблюдения, доказательством не является**.
  This explicitly replaces the original symmetric Large control; no claim that
  the original B NO-GO became GO retroactively.
- Other first-gate controls unchanged: mimalloc raw delta = 0 for N and 2N;
  magazine-hit-minus-prefill numerator delta within +/-10;
  raw churn N and 2N delta <= +10 kill gate on both axes.
- free-average = (n16-prefix)/16; body diagnostic = (body-base-probe)/64,
  reported without a new threshold.

## Immutable code predictions and isolation proof

B-A mutable root resolutions: Large own free -1/free; realloc move free tail
-1; promotion free tail -1; all other paths 0. Small own free separately remains
original oracle -1/free (16 frees after prealloc64: A32, B16, tcache16, scan0).
These are path-local claims, not a claim every composite operation has delta0.
Fastbin is required for the removed body lookup; no-fastbin remains unchanged.

Existing hits+misses counts exactly one per canonical_base_of_mut invocation:
`segment_table_impl.rs` increments hits before cached return, otherwise misses
before hash_find, including unsuccessful lookup. Read-only canonical_base_of
and canonical_root_for are not counted. No runtime instrumentation is added.

A ingress canonical_block_of and B owned_block_of both do one mutable lookup.
A dealloc_own_thread_with_base repeats canonical_block_of; B consumes witness.
Both Large bodies then delegate to AllocCore::dealloc, which does one additional
lookup. Thus Large own free A3/B2, not A2/B1.

`tests/r19_prf02r_realloc_tail.rs` is a single-test isolated process with no
installed Sefer global allocator. Bootstrap/prealloc/priming/diagnostics/printing
are outside measured windows. No foreign publishers or unrelated tests run.

- In-place 16->16 preserves address and tcache count0; total A1/B1.
  Realloc returns before either tail: absence of tail is explicitly asserted.
- Move 16->32 uses a primed magazine hit and verifies destination identity,
  different source address, no promotion and old class tcache +1. P4 magazine
  hit has NO stamp (hot.rs hit arm), independently asserted alloc total0.
  Realloc total A2/B1 minus ingress1 minus alloc0 isolates tail A1/B0.
  Old class cache is below overflow/budget limits: no flush lookup.
- Promotion 16->256KiB verifies promotion counter +1, physical Large tag2,
  different address and old tcache +1. alloc_large has no canonical mutable
  lookup; explicit stamp_segment_owner calls canonical_block_of exactly once
  even on stamp cache hit (ownership.rs). No published ingress exists to drain.
  Total A3/B2 minus ingress1 minus allocation stamp1 isolates tail A1/B0.
- Separate 2MiB Large free asserts A3/B2 as above.

Read implementations and callers: mem_impl canonical_block_of/owned_block_of/
dealloc; segment_table canonical_base_of_mut/contains_base/read-only lookup;
HeapCore routing, alloc hit/refill, owner stamp, realloc in-place/move/promotion,
own-free magazine guard/substrate fallthrough and alloc_large. Reintroducing
A body lookup makes B tail assertions fail; adding a tail to in-place makes
its ingress-only assertion fail. The test has been formatted, NOT compiled/run.
An unanticipated counted allocation/drain path is a final NO-GO, not permission
to subtract an empirically fitted baseline. Stop if isolation cannot be proved.

## C and conditional C-prime

C measured NO-GO: push +8 Ir, churn +512 raw. No new C measurement planned.
Only after B GO: exactly ONE C-prime attempt, original B BlockKind::of and
payload_start byte-exact; Unknown-only cold rejection plus
FOREIGN_OR_UNROUTABLE_FREES. C-prime-B push [-3,+1] on BOTH axes; raw churn
N/2N +10 kill gate. No C-prime implementation or launch authorization here.
M6 is unexpressible through pub(super), non-blocker; do not widen visibility.

## Identity, restoration and evidence

Base `042e88fb67ab48dd6bef7cf8bc416d8b0103ea54`.
Original B patch SHA256
`1730350cd2f8120f61043a37154b8abab88a045ec329064c8a3b9a3dac4307df`.
The separate `R19_PRF02R_identity.json` pins every B source byte to original
arms.B, verified git-show A bytes, full source/input hashes, generated harness
hashes, and reconstructed B patch hash. The reconstructed patch includes the
untracked witness and is not assumed identical to the lost original patch file.
Exact hash inventory is incorporated here by reference to that identity; hash
all inputs BEFORE activation/measurement, then fail closed on any drift.

Missing old external store: A reconstructed ONLY by read-only git show; B copied
from verified current source to fresh external backup snapshots. External writes
ONLY backups, no git checkout/add/commit/config. Arm installation uses cp + fresh
timestamps + cmp. Every execution restores B in finally with cp/cmp/hash check.
Transient harness and reconstructed patch live under target/r19-prf02r.
New receipts `_raw_r19_prf02r_*`; exclusive creation, never overwrite originals.
No private machine paths in identities or receipts. Exclusive driver lock;
operator must ensure no other cargo/iai process anywhere uses this worktree.
All new cargo uses WSL, `RUSTC_WRAPPER= CARGO_BUILD_JOBS=3
CARGO_TARGET_DIR="$PWD/target"`, `--locked`, one cargo/iai at a time. No r19-linux.
Preinstalled matching iai runner required; no installation by driver.

## Stage B static validation and hashes

PASS: original/new PRF02 driver and gate-table self-tests; actual hook scanner
(208 reviewed safe, 33 reviewed unsafe, 102 bench-gated safe); WSL rustfmt
edition2021 formatting/check of the new test; git diff --check; all arms.B and
source/input hashes. Empty Unknown test physically removed (observed size0).
Hook scanner **self-test FAILED** at its pre-existing r18 fixture:
`reviewed hook missing r18_sidecar_scan_bench gate` in queries.rs fixture.
Actual scanner separately PASSED. Not repaired: outside PRF02 scope. This is
not an all-checks-green claim; review must acknowledge that validation failure.

SHA256 pinned before any new cargo:

| Artifact | SHA256 |
|---|---|
| reconstructed runtime B patch | 9e4b867e58413b6bf3e110a50d11f8532133928d2f15ecf15250440d7514f8f6 |
| R identity | 76b4f9d0ed144b1803409f6ecb4f01c52bb3346b51a5a336cd5a91f4052b849c |
| tests/r19_prf02r_realloc_tail.rs | 14a33627f14382d9ce0e410bc5314d4413b38dc6f2dcc292551786e08b77f46e |
| scripts/r19_prf02r_iai.mjs | d1e186aad602be598f3ee3cddc2241530d8ae071add04666520f6e94db2f1a44 |
| scripts/r19_prf02r_gate_table.mjs | d5b316991010c275be83697b3618957be96860ef8b8f8fdbfb5dd6b20580d604 |

Identity contains the full per-file source/input inventory, not only this table.
The driver was repinned after adding mandatory activation-receipt preflight;
no measurements intervened. No runtime source was changed during Stage B.

## Exact next-stage recipe (not executed during Stage B)

From this repository root, after review approval (A reconstruction/hash and
snapshot verification is not an execution confirmation):

```sh
export R19_PRF02R_REVIEWED=yes
# BACKUPS is the fresh external backup directory created at preparation.
# No private path is persisted in repository; pass its actual location locally.
for arm in A B; do
  for repeat in 1 2; do
    node scripts/r19_prf02r_iai.mjs --run "$BACKUPS" "$arm" activation "$repeat" || exit 1
  done
done
for axis in pure instrumented; do
  for arm in A B; do
    for repeat in 1 2; do
      node scripts/r19_prf02r_iai.mjs --run "$BACKUPS" "$arm" "$axis" "$repeat" || exit 1
    done
  done
done
node scripts/r19_prf02r_gate_table.mjs --derive docs/perf
# On any activation failure, stop launches and run --derive immediately:
# it emits final NO-GO without requiring or inventing iai measurements.
```

Driver activation runs sequential `cargo run --locked --example
r19_prf02_activation --features 'production internals bench-internals' --
A 32` (B 16), then `R19_PRF02R_ARM=A` (or B) `cargo test --locked
--features 'production internals bench-internals medium-classes'
--test r19_prf02r_realloc_tail -- --test-threads=1 --nocapture`, under the
exact WSL environment above. Medium classes is activation-only, not an iai axis.
Driver iai launches the two target harnesses with `cargo bench --locked` and the
axis feature set. Output and compiler command metadata stay in new raw receipts.
Original instrumented source was formatted after its first measurement;
current fixture bytes are repinned, no old-count byte-identity claim.

## Code path references (B source; no execution claim)

- Mutable count: `src/alloc_core/segment/segment_table/segment_table_impl.rs:743`,
  hit increment `:747`, miss increment `:752`.
- Ingress/witness: `src/alloc_core/alloc_core/mem/mem_impl.rs:35`, `:46`;
  own-free routing `src/registry/heap_core_xthread/routing.rs:11`.
- Witness consumer `src/registry/heap_core/free/dealloc_own_base.rs:347`;
  Large substrate fallthrough `:550`; additional substrate canonical lookup
  `src/alloc_core/alloc_core/mem/mem_impl.rs:237`.
- Realloc ingress `src/registry/heap_core/free/realloc.rs:143`, in-place return
  `:219`, move tail `:408`, promotion alloc/stamp `:593`/`:597`, tail `:657`.
- Stamp lookup `src/registry/heap_core/state/ownership.rs:40`/`:42`;
  magazine-hit no-stamp path `src/registry/heap_core/alloc/hot.rs:207`/`:239`.
- Test default arm `tests/r19_prf02r_realloc_tail.rs:23`.

## Boundary B review correction (before activation)

The targeted realloc test defaults to B; explicit `R19_PRF02R_ARM=A` remains
available. Runtime src B and original R thresholds are unchanged. Only the new
transient R identity/input pins are replaced; no old receipts are replaced.
No cargo, activation or measurement happened during this correction.

The judge exclusively generates `R19_PRF02R_REGATE.md` and
`R19_PRF02R_REGATE_summary.csv` from new raw receipts. CSV includes raw counts,
all derived metrics and B-A deltas with numerator/denominator, repeat, receipt,
identity/prereg hashes, features and environment. The activation table records
all four expected receipts; a missing/failed activation is final NO-GO and
suppresses iai parsing. Driver receipts pin metadata and command exit status;
failed commands are preserved, never reinterpreted as successful activation.
`R19_PRF02R_prereg.sha256` separately pins this preregistration before activation
(to avoid a circular identity/prereg hash); driver and judge assert it.


Boundary B correction validation: original and R driver/judge self-tests PASS;
WSL rustfmt edition2021 format/check PASS. All unchanged input hashes and
original B source hashes verified; read-only git-show A hashes verified.
Only three new input hashes and the new R identity were repinned; preregistration
SHA256 was pinned last in R19_PRF02R_prereg.sha256. No measurement happened;
no R raw receipts exist. No runtime src B or git state was written.

## Stage C activation-only approval and preflight correction

Review accepted for activation A1, A2, B1, B2 strictly sequential; no IAI authorization. Before first activation, judge body diagnostic corrected to (body-base-probe)/64 only; synthetic selftest moved under repository target/r19-prf02r; report title identifies perf(runtime)-кандидат B without a predeclared GO. Judge selftest asserts numerator 2000 for synthetic body3000/baseprobe1000, without subtracting prealloc1000. No runtime source or predictions changed. Judge input and identity repinned, this preregistration pinned last.
