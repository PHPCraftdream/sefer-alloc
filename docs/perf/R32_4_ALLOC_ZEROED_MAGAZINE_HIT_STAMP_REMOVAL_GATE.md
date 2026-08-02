# R32-4 (task #495) — remove `alloc_zeroed`'s magazine-hit `stamp_segment_owner` redundancy

Date: 2026-08-02.

## 0. What this is

`HeapCore::alloc_small_zeroed_via_magazine`'s magazine-HIT arm
(`src/registry/heap_core_alloc.rs`, the path `alloc_zeroed` takes for a
small class under `virgin-zero-skip` + `fastbin`) called
`self.stamp_segment_owner(issued)` right before returning a popped block.
Plain `alloc`'s own magazine-hit arm, immediately above it in the same file,
does NOT do this — its own comment says so explicitly: "P4: NO stamp here —
the block's source segment was already stamped during the refill that
originally pulled it." The P4 justification applies verbatim to BOTH arms:
both are fed by a refill (`refill_magazine_slow` /
`refill_magazine_slow_virgin`) whose stamp-dedupe loops are line-for-line
identical and stamp every distinct source segment before any block lands in
the magazine. There was no stated reason for the asymmetry.

This task (1) enumerates every producer of a magazine-resident block to
confirm the stamp is genuinely redundant (not assumed), (2) removes it,
(3) measures the Ir delta under WSL/callgrind with a path-activation oracle
proving the hit arm actually ran, and (4) appends a dated correction to
`docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md` naming the
(now-fixed) asymmetry, per this project's append-only correction convention.

`virgin-zero-skip` is NOT in `production`'s default feature bundle
(`Cargo.toml`: `production = ["alloc-global", "alloc-xthread",
"alloc-decommit", "fastbin", "alloc-segment-directory",
"primordial-lazy-commit", "class-aware-dirty"]` — no `virgin-zero-skip`), so
per CLAUDE.md's commit-prefix taxonomy this is `perf(opt-in)`, not
`perf(runtime)`.

## 1. Enumeration: is the stamp genuinely removable?

A magazine-resident block (`self.tcache.classes[c].slots[..]`) can only be
placed there by one of exactly three producer sites:

1. **`refill_magazine_slow`** (`heap_core_alloc.rs`, plain `alloc`'s miss
   path). Its stamp-dedupe loop (lines ~718-732, pre-existing code) walks
   every refilled block `i in 0..n`, computing each block's segment base and
   calling `stamp_segment_owner` exactly once per distinct base BEFORE any
   of those `n` blocks are marked magazine-resident or issued. Every block
   that lands in the magazine through this path is stamped first.

2. **`refill_magazine_slow_virgin`** (`heap_core_alloc.rs`, the
   `virgin-zero-skip`-only sibling `alloc_small_zeroed_via_magazine`'s own
   miss leg calls). Its stamp-dedupe loop (lines ~443-453 pre-existing,
   unaffected by this task) is line-for-line identical in structure to
   (1)'s — same walk, same base-dedupe compare, same
   `stamp_segment_owner` call — before any block is marked
   magazine-resident via `mark_magazine` or returned to the caller. Every
   block retained in the magazine (indices `0..new_cnt`) or issued
   (index `new_cnt`) by this path is stamped first.

3. **The free path's magazine push**, `dealloc_own_thread`
   (`heap_core_free.rs` ~line 712-724 non-overflow arm, ~line 796-802
   overflow arm). This path does NOT itself call `stamp_segment_owner` — its
   own comment says "Legit free → push. NO key stamp, NO block-body write,"
   the free-side mirror of the alloc-side P4 argument. It does not need to:
   `dealloc_own_thread` is reached only through `dealloc_routing`'s
   `contains_base(base)` check resolving `true` for THIS heap's own
   `AllocCore` (`src/registry/heap_core_free.rs`; the identical
   `self.core.contains_base(base)` check gates the own-segment leg of
   `realloc` too, `:922`). A segment can only be registered in a heap's own
   `AllocCore` segment table if that same heap previously carved/reserved
   it — which is exactly the alloc-side event that producer (1) or (2)
   above already stamped. So any block reaching the free-path push was
   necessarily issued from an already-stamped segment on a PRIOR alloc
   (either a direct fresh-alloc stamp at `heap_core_alloc.rs`'s Large
   fallthrough / non-fastbin virgin branch, or via one of the two refill
   dedupe loops above) — the free path re-parks an already-stamped block,
   it never introduces a new unstamped one.

**Conclusion: the stamp is removable.** No producer of a magazine-resident
block can ever place an unstamped segment's block into the magazine. Every
path a `HeapCore::alloc_zeroed` caller can take to reach
`alloc_small_zeroed_via_magazine`'s hit arm pops a block whose segment was
already stamped by `self.id` before this call. This is exactly the same
guarantee plain `alloc`'s own hit arm already relies on and has relied on
since the P4 hoist — `alloc_small_zeroed_via_magazine`'s stamp was carried
over from the non-`fastbin` sibling branch
(`heap_core_alloc.rs`'s `virgin-zero-skip`-without-`fastbin` arm, which
DOES need its own explicit stamp because it bypasses the magazine entirely
and calls `AllocCore::alloc_small_with_virgin` directly — a genuinely
different, magazine-free code path where the P4 guarantee does not apply)
by symmetry, not by necessity.

## 2. The fix

`src/registry/heap_core_alloc.rs`, `alloc_small_zeroed_via_magazine`'s hit
arm: deleted the `self.stamp_segment_owner(issued);` line, replaced with a
comment recording the enumeration above (so a future reader does not
re-introduce the stamp "for symmetry" without re-deriving §1).

No other line changed in that function. The non-fastbin sibling branch
(`heap_core_alloc.rs`'s `virgin-zero-skip`-without-`fastbin` arm, which
calls `self.core.alloc_small_with_virgin` + `self.stamp_segment_owner`
directly) is untouched — it does not go through the magazine at all, so the
P4 guarantee does not apply there and its stamp is load-bearing.

## 3. Path-activation oracle + Ir measurement

Per CLAUDE.md's R30-8 rule, a judge must prove the arm actually took the
HIT path, not a refill miss, and must measure at the layer the feature
actually ships at (`HeapCore::alloc_zeroed`, the chain
`SeferAlloc`'s `#[global_allocator]` uses — not bare `AllocCore`, R31-0's
own corrected layer).

### 3.1 New benches

Two new `#[library_benchmark]` arms added to `benches/perf_gate_iai.rs`,
directly mirroring the pre-existing `alloc_magazine_prefill_only_16b` /
`alloc_magazine_hit_only_16b` pair's shared-prefix-subtraction design
(R23-3):

- `alloc_zeroed_magazine_prefill_only_16b` — `PREFILL_CYCLES` rounds of
  carve-16-via-plain-`alloc` + free-16-via-plain-`dealloc` through
  `HeapCore::alloc`/`HeapCore::dealloc` (the `#[doc(hidden)]` test-only
  export surface, same surface the pre-existing pair already uses). Ends
  with the magazine holding 16 resident, NON-virgin blocks (a freed block
  is never virgin — `dealloc_own_thread`'s own R13-3 comment). Control arm:
  untouched by this task's code change (never calls `alloc_zeroed`).
- `alloc_zeroed_magazine_hit_only_16b` — byte-identical prefill, plus ONE
  final drain of the 16 resident blocks via `HeapCore::alloc_zeroed`
  instead of `HeapCore::alloc`. `count` is 16 and every popped block is
  non-virgin at that point (by construction — see the prefill loop above),
  so all 16 calls are guaranteed to take
  `alloc_small_zeroed_via_magazine`'s HIT arm (never
  `refill_magazine_slow_virgin`'s miss leg) AND `Node::zero` runs every
  time (`is_virgin` reads `false`) — this construction IS the
  path-activation oracle: no separate hit/miss counter is needed because
  the workload's own shape makes a miss structurally impossible for these
  16 calls, exactly the technique the pre-existing `alloc_magazine_hit_only_16b`
  already established for plain `alloc`.

Both are gated `#[cfg(all(target_os = "linux", feature = "alloc-xthread",
feature = "fastbin", feature = "virgin-zero-skip"))]` with no-op stubs for
`not(feature = "virgin-zero-skip")` (mirroring the R24-8 stub pattern
already used elsewhere in this file), so
`cargo check --bench perf_gate_iai --features "production bench-internals"`
(the exact command `npm run check`'s final step and `scripts/iai.mjs`'s
default run use) still resolves without `virgin-zero-skip`.

### 3.2 Immutable source identity (CLAUDE.md's R29-6 rule)

- **AFTER** (stamp removed): the main working tree at commit
  `5d72bc633193938181e2d06f8c584617ebaecf42` (`main` HEAD at task start) +
  this task's full diff (stamp removal in `heap_core_alloc.rs` + the new
  bench pair in `perf_gate_iai.rs`). Landing commit SHA fills the
  `UNFILLED` placeholder in
  `docs/perf/R495_STAMP_REMOVAL_GATE_summary.csv` in a follow-up commit
  (chicken-and-egg — the SHA cannot cite itself), mirroring the R31-0/R31-1
  precedent for the same problem.
- **BEFORE** (stamp present): `git worktree add ../sefer-alloc-r495-before
  5d72bc633193938181e2d06f8c584617ebaecf42` (isolated worktree, base commit,
  detached HEAD), with ONLY `git diff -- benches/perf_gate_iai.rs` (the
  bench-only half of this task's diff — the new bench pair, NOT the stamp
  removal) applied via `git apply`. `git write-tree` of that state:
  `3dfa28c9532427cbf00e4204f98c80d783056608`. This measures the OLD
  (stamp-present) hit arm through the EXACT SAME bench source the AFTER run
  uses — the only difference between the two measured trees is the single
  deleted `stamp_segment_owner` call.

### 3.3 Reproduction

```
# AFTER (current working tree, stamp removed):
node scripts/iai.mjs --features "production bench-internals virgin-zero-skip" \
  alloc_zeroed_magazine small_churn_16b churn_256b aligned_churn_640b_a128 cold_alloc_free_256x16b
# -> docs/perf/_raw_r495_stamp_removal_after.log

# BEFORE (isolated worktree at the base commit + bench-only patch):
git worktree add ../sefer-alloc-r495-before 5d72bc633193938181e2d06f8c584617ebaecf42
cd ../sefer-alloc-r495-before
git diff 5d72bc633193938181e2d06f8c584617ebaecf42 -- benches/perf_gate_iai.rs | git apply   # (or apply the bench-only hunk directly)
node scripts/iai.mjs --features "production bench-internals virgin-zero-skip" \
  alloc_zeroed_magazine small_churn_16b churn_256b aligned_churn_640b_a128 cold_alloc_free_256x16b
# -> docs/perf/_raw_r495_stamp_removal_before.log

# Derive the summary CSV (asserts the arithmetic, per CLAUDE.md's checked-script rule):
node scripts/r495_stamp_removal_summary.mjs [landing_commit_sha]
```

Raw logs: `docs/perf/_raw_r495_stamp_removal_before.log`,
`docs/perf/_raw_r495_stamp_removal_after.log` (both the full `npm run
iai`-style report, not truncated). Summary CSV:
`docs/perf/R495_STAMP_REMOVAL_GATE_summary.csv`, produced by
`scripts/r495_stamp_removal_summary.mjs` — the one checked script; it
hard-asserts (a) the control arm's delta is exactly 0, (b) all four
plain-`alloc` kill-gate benches' deltas are exactly 0, and (c) the treatment
arm's delta is negative and within the task brief's predicted [-30, -5]
Ir/hit sanity range, before writing the CSV or printing a number.

## 4. Result

| bench | before Ir | after Ir | Δ Ir | note |
|---|---:|---:|---:|---|
| `alloc_zeroed_magazine_prefill_only_16b` | 7,832 | 7,832 | 0 | control — never calls `alloc_zeroed` |
| `alloc_zeroed_magazine_hit_only_16b` | 8,623 | 8,431 | **−192** | treatment — 16 magazine hits via `alloc_zeroed` |
| `small_churn_16b` | 8,437 | 8,437 | 0 | kill-gate |
| `churn_256b` | 8,437 | 8,437 | 0 | kill-gate |
| `aligned_churn_640b_a128` | 8,373 | 8,373 | 0 | kill-gate |
| `cold_alloc_free_256x16b` | 51,942 | 51,942 | 0 | kill-gate |

Treatment delta over 16 hits: **−192 Ir / 16 = −12.00 Ir/hit removed**,
matching the task brief's predicted "roughly 12-18 Ir plus one extra
metadata cache-line touch per `alloc_zeroed` magazine hit" range. The
control arm and all four plain-`alloc` kill-gate benches are byte-identical
(exactly 0 Ir delta), confirming (a) the prefill/kill-gate arms are
genuinely untouched by this change and (b) plain `alloc`'s own hit arm
(never touched by this task) did not regress or improve as a side effect —
the fix is confined to exactly the one arm it targets.

## 5. Correctness verification

`cargo test --features "production,virgin-zero-skip"` run on the full tree
after the fix: all test binaries pass except
`tests/r31_10_trim_current_thread_api.rs`'s
`ac4c_trim_on_never_allocated_thread_claims_no_slot`, which fails ONLY under
the default multi-threaded `cargo test` runner and passes both in isolation
(`cargo test --test r31_10_trim_current_thread_api`) and single-threaded
(`--test-threads=1`) — reproduced IDENTICALLY on the pre-change tree
(commit `5d72bc633193938181e2d06f8c584617ebaecf42`, no working-tree changes)
under the same parallel-runner conditions. This is a pre-existing test-order
sensitivity in that test's own process-global
`heaps_claimed_high_water` assertion (racing against OTHER tests' registry
claims in the same test binary process, not against this task's diff) —
confirmed NOT caused by this task's change, and out of scope for it. Not
filed as a new `docs/CORRECTNESS_OPEN_ITEMS.md` entry here because it is a
test-isolation flake already exhibited by the unmodified base commit, not a
regression this task introduced; a future task auditing that test file's
process-global-state assumptions may want to file it separately.

## 6. R31-0 correction

See `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE.md`'s new dated
addendum (§9), which names this asymmetry and its direction of bias against
the `virgin-zero-skip` ON arm in that report's A/B, per this project's
append-only correction convention.

## 7. Verdict

**GO** — the stamp was genuinely redundant (enumeration in §1), the removal
measures a real, if small, per-hit Ir saving (§4) with zero blast radius
outside the targeted arm (kill-gates flat), and correctness is unaffected
(§5). `perf(opt-in)`: `virgin-zero-skip` is not in `production`'s default
bundle, so this is real, if opt-in, shipping code — not a measurement-only
change.
