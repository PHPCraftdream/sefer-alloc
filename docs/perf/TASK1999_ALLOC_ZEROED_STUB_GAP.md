# Task #1999 — close the iai stub gap on `alloc_zeroed`'s non-virgin path

**Two verdicts in this report:**

1. **Coverage gap closed (code change).** `alloc_zeroed_magazine_prefill_only_16b`
   / `alloc_zeroed_magazine_hit_only_16b` under `not(feature =
   "virgin-zero-skip")` now exercise the real calloc-shaped path plain
   `production` executes, instead of a `black_box(0u8)` stub.
2. **#1982 re-measured against its parent: NULL.** The classify-once split
   (`alloc_with_class`) changes the marginal per-hit `alloc_zeroed` cost by
   **0 Ir** (391 Ir for 16 hits, identical before and after, to the last
   digit). This is the measurement #1982's own commit message said it lacked
   and explicitly deferred to this follow-up; NULL is reported as NULL, not
   silently dropped.

## 0. Scope

Measurement-only for verdict 2; a small, real (not stub) benchmark body for
verdict 1 — `benches/perf_gate_iai.rs` gained no new counters, no new feature
gates, and `production`'s composition is unchanged. Per CLAUDE.md's
raw-log/summary-CSV rule (R22-14), this report cites measured numbers, so it
owes and includes raw logs + a summary CSV despite being a small follow-up
rather than a full gate.

## 1. The gap (before this task)

`benches/perf_gate_iai.rs`'s `alloc_zeroed_magazine_{prefill,hit}_only_16b`
pair had a REAL body only under `all(target_os="linux", alloc-xthread,
fastbin, virgin-zero-skip)`; the `not(virgin-zero-skip)` arm compiled a
`black_box(0u8)` no-op stub. `npm run iai` runs under `production
bench-internals internals` (`scripts/iai.mjs`'s `DEFAULT_FEATURES`), and
`production` does **not** include `virgin-zero-skip` — so the gate that
actually guards this repo measured `Ir=3` (a `black_box`, not an allocator
call) for both rows, discovered while implementing #1982 and confirmed here
by re-running the pre-task binary (`docs/perf/_raw_task1999_...` inline
excerpt below cites the stub numbers from the earlier committed baseline,
reproduced in the summary CSV's `stub_baseline` rows from the last full `npm
run iai` run before this task's bench-file edit).

## 2. Entry point and mechanism

**Entry point:** `HeapCore::alloc_zeroed` (`src/registry/heap_core/alloc/hot.rs`)
— the same layer `SeferAlloc`'s `#[global_allocator]` `alloc_zeroed` reaches,
per CLAUDE.md's entry-point rule (not `AllocCore::alloc_zeroed`, which
bypasses the magazine).

**Mechanism, `not(virgin-zero-skip)` small-classified branch:** delegates
entirely to `alloc_with_class` (the classify-once split #1982 introduced —
same magazine pop `alloc_magazine_hit_only_16b` exercises), then
unconditionally calls `Node::zero(ptr, size)`. There is no virgin/non-virgin
split in this branch at all (that distinction only exists under
`virgin-zero-skip`) — every non-null result is zeroed, always, matching the
byte-identical pre-R12-10 behaviour this branch preserves.

**Bench design — shared-prefix subtraction (R23-3's established technique,
reused unchanged):** `alloc_zeroed_magazine_prefill_only_16b` runs
`PREFILL_CYCLES` fill cycles (carve 16 blocks via plain `alloc`, free all 16
via plain `dealloc`, leaving the magazine populated with exactly 16 resident
blocks after the last cycle) with no drain. `alloc_zeroed_magazine_hit_only_16b`
is byte-identical plus ONE added step: drain those 16 resident blocks via
`alloc_zeroed` instead of `alloc`. Subtracting the prefill arm's Ir from the
hit arm's Ir isolates exactly 16 hits' cost (fill cost, common to both arms,
cancels exactly).

**Why no separate path-activation-oracle counter was added:** `TCACHE_CAP`
(the magazine's capacity) is 16, matching `MAGAZINE_FILL`; the prefill loop's
final cycle pushes exactly 16 blocks and the hit arm's drain pops exactly 16
— a miss is structurally impossible here (the same deterministic-residency
argument the `virgin-zero-skip` sibling pair already relies on, in the same
file, reviewed and shipped under R23-3/F7). Adding a counter-based oracle for
a mechanism that cannot structurally miss would duplicate, not strengthen,
that existing argument.

## 3. Measured results

Three `npm run iai` runs, `production bench-internals internals` (this
project's default iai feature set), WSL Ubuntu-24.04,
`iai-callgrind-runner 0.14.2`, valgrind 3.22.0:

| measurement | source | prefill Ir | hit Ir | hit − prefill (16-hit marginal) |
|---|---|---:|---:|---:|
| stub (pre-task) | `production bench-internals internals`, any commit before this one | 3 | 3 | n/a (both `black_box`) |
| before #1982 | `105d89e2` (parent of #1982) + this commit's bench-file diff | 8,561 | 8,952 | **391** |
| after #1982 | `7bbdbd57` (task #1999's base) + this commit's bench-file diff | 8,707 | 9,098 | **391** |

**Verdict 2 (the #1982 re-measurement): NULL.** The 16-hit marginal cost is
**identical to the last digit, 391 Ir, before and after #1982's
classify-once split.** The raw per-row Ir DID shift by a constant +146 on
BOTH rows (8,561→8,707 prefill, 8,952→9,098 hit) — since the prefill arm
never calls `alloc_zeroed` at all, this +146 reflects a codegen-shape change
to `alloc` itself (now a classify-then-delegate wrapper around
`alloc_with_class` instead of one monolithic function), not a per-hit
`alloc_zeroed` cost change. The MARGINAL figure (what the bench pair is
actually designed to isolate) nets that shift out exactly. This matches
#1982's own commit message verbatim: "NO SPEEDUP MEASURED OR CLAIMED" —
now confirmed by real numbers, not just argued from the diff's shape,
exactly the measurement that commit deferred to this follow-up.

**Verdict 1 (the coverage gap): closed.** Both rows moved from `Ir=3`
(`black_box`) to real allocator-path numbers (8,707 / 9,098) — a large
apparent jump that is **not a regression**: it reflects the stub being
replaced with a real body, not the allocator getting slower. See
`docs/perf/IAI_BASELINE.md`'s new section for the same note in the file
future `npm run iai` diffs are read against.

## 4. Immutable source identity

- **Stub baseline / after-#1982 measurement:** this task's own commit
  (introducing this report and the real bench bodies) IS the exact source —
  `git log -1 --format=%H -- benches/perf_gate_iai.rs` at the commit that
  lands this report resolves it.
- **Before-#1982 measurement:** base commit `105d89e2` (verify: `git log -1
  --format=%H 68fe92ef^`, i.e. the parent of #1982's landing commit
  `68fe92ef`) + this task's own committed bench-file diff applied on top
  (`git diff 7bbdbd57..<this-commit> -- benches/perf_gate_iai.rs`, which is
  base-commit-independent — it only touches the two function bodies, not
  anything #1982 changed). Reproduce: `git worktree add --detach <dir>
  105d89e2 && git -C <dir> apply <(git diff 7bbdbd57..<this-commit> --
  benches/perf_gate_iai.rs) && cd <dir> && npm run iai`.

## 5. Reproduction

```
cargo build --release --features "production bench-internals internals"
npm run iai   # from repo root; requires WSL + valgrind, see scripts/iai.mjs
```

## 6. Raw logs / summary CSV

- `docs/perf/_raw_task1999_alloc_zeroed_stub_gap_after_1982.log` (full `npm
  run iai` output at this task's HEAD)
- `docs/perf/_raw_task1999_alloc_zeroed_stub_gap_before_1982.log` (full `npm
  run iai` output at `105d89e2` + this task's bench-file diff)
- `docs/perf/TASK1999_ALLOC_ZEROED_STUB_GAP_summary.csv`

Both raw logs are ~62 KiB, under the 200 KiB force-add ceiling (R34-24) —
committed verbatim, no truncation needed.
