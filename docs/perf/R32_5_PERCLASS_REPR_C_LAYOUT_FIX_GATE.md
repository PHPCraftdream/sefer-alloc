# R32-5 (task #496) — `PerClass` gains `#[repr(C)]`; restores the documented one-cache-line magazine layout

Date: 2026-08-02.

## 0. What this is

`src/registry/tcache.rs`'s `PerClass` (per-size-class magazine: a depth
counter `count` plus a fixed pointer stack `slots`, and — under
`virgin-zero-skip` — a virginity bitmask `virgin_mask`) carries a doc
comment (PERF-PASS-5, task #53) claiming the whole point of bundling these
fields into one struct is so a magazine push/pop touches ONE 64-byte cache
line instead of two, by keeping `count` "directly adjacent to (in front of)"
`slots`.

The struct had **no `#[repr(C)]`** — it used Rust's unspecified default
layout (`repr(Rust)`). This task (1) re-derives the actual field offsets
independently (not trusting the F4 survey finding blindly), (2) confirms the
claimed locality was NOT being delivered, (3) fixes it with `#[repr(C)]` +
declaration reorder, (4) pins the fix with compile-time `offset_of!`
asserts so it cannot silently regress again, and (5) measures the Ir impact
on the magazine hot path with the project's standard WSL/callgrind
worktree-isolation technique.

This is task #496, tracking finding **F4** in
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` (§"F4 — `PerClass` is
missing `#[repr(C)]`...", lines 426-518), which itself predicted "expect a 0
`Ir` delta (same instructions, different addresses)" for the churn benches
and explicitly flagged the benefit as **unproven** — this report exists to
verify that prediction empirically, not to manufacture a win.

## 1. Independent layout re-verification

A scratch `rustc -O` probe (outside the repo, `std::mem::offset_of!`) on the
struct's PRE-FIX shape, both without and with the `virgin_mask` field:

```rust
#[derive(Clone, Copy)]
struct PerClassNoMask { count: u8, slots: [*mut u8; 16] }
#[derive(Clone, Copy)]
struct PerClassMask { count: u8, slots: [*mut u8; 16], virgin_mask: u16 }
```

Result (rustc 1.97.0):

| variant | `size_of` | `align_of` | `offset_of(count)` | `offset_of(slots)` | `offset_of(virgin_mask)` |
|---|---:|---:|---:|---:|---:|
| no `virgin-zero-skip` | 136 | 8 | **128** | **0** | — |
| with `virgin-zero-skip` | 136 | 8 | **130** | **0** | **128** |

This independently confirms the survey's claim: rustc's default
field-reordering heuristic (sort by descending alignment) placed the
8-aligned `slots` array FIRST and the small fields LAST — the exact opposite
of what the struct's own doc comment describes. For the documented common
case (shallow magazine, `count` 1-3 — see R22-17 §1.2 / the `Э6` comment at
`heap_core_free.rs:601-602`), `count` at byte offset 128 and `slots[0]` at
byte offset 0 are **always in different 64-byte cache lines** (128 ≥ 64).
The one-cache-line optimization task #53 documented was not in effect.

## 2. The fix

`src/registry/tcache.rs`:

1. Added `#[repr(C)]` to `PerClass`.
2. Reordered the DECLARED fields to `count`, `virgin_mask` (cfg-gated),
   `slots` — under `#[repr(C)]`'s declaration-order guarantee, this puts the
   small fields first and pads `slots` up to its own 8-byte alignment
   starting right after them, instead of rustc reordering them away.
3. Added three `const _: () = assert!(core::mem::offset_of!(...) == N, ...)`
   compile-time pins (mirroring the pre-existing `TCACHE_CAP <= 16` assert
   in the same file) — one for `count == 0`, one for `virgin_mask == 2`
   (only under `virgin-zero-skip`), one for `slots == 8` — so a future field
   reorder or an accidental removal of `#[repr(C)]` fails the BUILD instead
   of silently regressing the documented locality claim a second time.
4. Left `TCACHE_CAP <= 16`'s existing assert (the `virgin_mask` bit-per-slot
   invariant) untouched — only declaration order changed, not its logic.

Post-fix offsets, re-derived the same way (both configurations now give
`size_of == 136`, `align_of == 8`, unchanged from before):

| variant | `offset_of(count)` | `offset_of(virgin_mask)` | `offset_of(slots)` |
|---|---:|---:|---:|
| no `virgin-zero-skip` | **0** | — | **8** |
| with `virgin-zero-skip` | **0** | **2** | **8** |

Struct size is **byte-identical to before in both configurations** (136
bytes either way — `#[repr(C)]` only stopped the small fields being pushed
to the tail; the padding budget is the same 7 or 5 bytes either way). `count`
now sits 8 bytes from `slots[0]` — the survey's arithmetic
(`(c*136) mod 64 == (c*8) mod 64`, nonzero only for `c ≡ 7 (mod 8)`) means 7
of every 8 classes now have `count` and `slots[0..6]` on the SAME 64-byte
line, versus 0 of 8 before.

## 3. Ir measurement — magazine hit/push path, worktree-isolated

### 3.1 Which benches

No new bench code was needed: the pre-existing `alloc_magazine_prefill_only_16b`
/ `alloc_magazine_hit_only_16b` pair (R23-3) and
`alloc_zeroed_magazine_prefill_only_16b` / `alloc_zeroed_magazine_hit_only_16b`
pair (F7/R32-4) in `benches/perf_gate_iai.rs` already isolate EXACTLY the
magazine push/pop path this fix targets, via the established shared-prefix-
subtraction technique (fill the magazine to 16 resident blocks, then time a
16-hit drain; subtracting the prefill arm's Ir from the hit arm's Ir isolates
16 hits' worth of magazine-pop cost with the fill cost cancelled). Reusing
them means BEFORE and AFTER are measured through byte-identical bench
source — no bench-file diff to account for.

### 3.2 Immutable source identity (CLAUDE.md's R29-6 rule)

- **BEFORE**: `git worktree add ../sefer-alloc-r496-before
  62e217fa1ca599d5903fb519c16ab9f0af55a7e0` (this task's base commit — `main`
  HEAD at task start), no changes applied (`PerClass` still `repr(Rust)`).
- **AFTER**: the main working tree at the same base commit +
  this task's full diff to `src/registry/tcache.rs` only (no bench-file
  changes). Patch hash (`git diff -- src/registry/tcache.rs | sha256sum`):
  `64d6a1ab3c0a0d861e8d52574bdcd2610ea003bf4744d9e90df1d44b8a54cbc9`.

### 3.3 Reproduction

```
# Plain-alloc + kill-gates, production feature set:
node scripts/iai.mjs alloc_magazine_prefill_only_16b alloc_magazine_hit_only_16b \
  small_churn_16b churn_256b aligned_churn_640b_a128 cold_alloc_free_256x16b
# BEFORE -> docs/perf/_raw_r496_repr_c_before_production.log (run in the isolated worktree)
# AFTER  -> docs/perf/_raw_r496_repr_c_after_production.log  (run in the main tree)

# alloc_zeroed + kill-gates, virgin-zero-skip feature set:
node scripts/iai.mjs --features "production bench-internals virgin-zero-skip" \
  alloc_magazine_prefill_only_16b alloc_magazine_hit_only_16b \
  alloc_zeroed_magazine_prefill_only_16b alloc_zeroed_magazine_hit_only_16b \
  small_churn_16b churn_256b large_alloc_free_cycle
# BEFORE -> docs/perf/_raw_r496_repr_c_before_virginzeroskip.log
# AFTER  -> docs/perf/_raw_r496_repr_c_after_virginzeroskip.log

# Derive the summary CSV (asserts the arithmetic, per CLAUDE.md's checked-script rule):
node scripts/r496_perclass_repr_c_summary.mjs [landing_commit_sha]
```

Raw logs (full `npm run iai`-style reports, not truncated):
`docs/perf/_raw_r496_repr_c_before_production.log`,
`docs/perf/_raw_r496_repr_c_after_production.log`,
`docs/perf/_raw_r496_repr_c_before_virginzeroskip.log`,
`docs/perf/_raw_r496_repr_c_after_virginzeroskip.log`. Summary CSV:
`docs/perf/R496_PERCLASS_REPR_C_LAYOUT_FIX_GATE_summary.csv`, produced by
`scripts/r496_perclass_repr_c_summary.mjs` — the one checked script; it
hard-asserts (a) the isolated 16-hit magazine-pop delta is exactly 0 Ir in
BOTH feature configurations, and (b) the plain-churn kill-gate benches
within each feature set move by the exact same constant as each other
(proving a uniform shift, not a concentrated regression), before writing the
CSV or printing a number.

## 4. Result

### 4.1 Isolated per-op magazine-hit cost (the actual thing under test)

| bench pair | before Ir | after Ir | isolated Δ (16 hits) | Δ/hit |
|---|---:|---:|---:|---:|
| `alloc_magazine_{prefill,hit}_only_16b` (plain `alloc`, production) | 358 | 358 | **0** | 0.00 |
| `alloc_zeroed_magazine_{prefill,hit}_only_16b` (`alloc_zeroed`, virgin-zero-skip) | 599 | 599 | **0** | 0.00 |

**The magazine hit/push path's own Ir cost is unchanged — exactly 0 Ir
delta, in both feature configurations.** This matches the survey's own
explicit prediction ("expect 0 `Ir` delta — same instructions, different
addresses"). `#[repr(C)]` + field reorder changes WHERE `count`/`slots`
live in memory, not how many instructions the pop/push sequence executes;
Ir (an instruction *count*) is blind to that by construction — a genuine
locality win, if any, would show up in `Estimated Cycles`/cache-miss counts,
not Ir (see §4.3).

### 4.2 A real, but uniform and workload-independent, absolute-Ir shift

Every SeferAlloc-side bench's ABSOLUTE Ir number moved between BEFORE and
AFTER — this was investigated rather than dismissed, since a naive read
("`small_churn_16b`, a supposed kill-gate, changed by 755 Ir!") would look
like a regression:

| bench | feature set | before Ir | after Ir | Δ |
|---|---|---:|---:|---:|
| `small_churn_16b` | production | 8,055 | 8,810 | +755 |
| `churn_256b` | production | 8,055 | 8,810 | +755 |
| `aligned_churn_640b_a128` | production | 7,991 | 8,746 | +755 |
| `cold_alloc_free_256x16b` | production | 50,468 | 50,968 | +500 |
| `large_alloc_free_cycle` (bootstrap proxy) | production | 3,312 | 4,080 | +768 |
| `small_churn_16b` | virgin-zero-skip | 8,437 | 9,241 | +804 |
| `churn_256b` | virgin-zero-skip | 8,437 | 9,241 | +804 |
| `large_alloc_free_cycle` (bootstrap proxy) | virgin-zero-skip | 3,312 | 4,129 | +817 |

Diagnosis: `small_churn_16b`, `churn_256b`, and `aligned_churn_640b_a128`
(three structurally different plain-`alloc` churn benches) move by the
EXACT SAME constant within each feature set (755 in production, 804 under
virgin-zero-skip — asserted equal by the derive script). mimalloc's own
benches in the SAME log runs (`mimalloc_small_churn_16b`,
`mimalloc_churn_256b`, ...) — an independent allocator this change cannot
touch — are byte-identical between BEFORE and AFTER (verified in the raw
logs). This is consistent with exactly one thing: a **process-wide
one-time initialization cost change**, not a per-operation cost change.
`HeapCore::new()` constructs `Tcache::new()`, which zero-initializes
`SMALL_CLASS_COUNT` (49) `PerClass` structs via `PerClass::new()`'s `const
fn` — reordering that struct's fields changes the codegen for this
one-time zero-init loop (different immediate offsets, different
instruction selection/scheduling), and that fixed one-time cost is baked
into every bench's raw Ir (each `#[library_benchmark]` runs in its own
fresh process, so the bootstrap cost is never amortized away — the same
"every bench pays `B` once" structure `docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE.md`
already documents for this bench suite).

`large_alloc_free_cycle` (the dedicated bootstrap-proxy bench: one 4 MiB
alloc+free, structurally different from the small-class churn benches) also
shifted, but by a slightly different amount (768 vs. the churn benches' 755
in production; 817 vs. 804 under virgin-zero-skip — a ~13-17 Ir residual).
This is plausible and not investigated further: `large_alloc_free_cycle`'s
own body never touches the small-class `Tcache`/`PerClass` array beyond the
same one-time `HeapCore::new()` zero-init the churn benches share, so its
delta should track theirs closely but need not match to the instruction —
the two workload shapes exercise different amounts of adjacent codegen
around the allocator's init path. `cold_alloc_free_256x16b`'s smaller +500
delta (a carve/refill-heavy workload that touches `PerClass` differently
under the hood) is reported as-is, not forced to match.

**No SeferAlloc bench moved DOWN, and none moved by an amount inconsistent
with "one fixed one-time constant plus zero per-op change"** — this is a
process-bootstrap-codegen shift, not a functional regression, confirmed by
§4.1's isolated-delta-zero result on the actual operation under test.

### 4.3 Cache-level signal (informational, not the pass/fail judge)

RAM-miss counts dropped materially in the AFTER runs (production feature
set): `small_churn_16b` 442→283 RAM hits, `cold_alloc_free_256x16b`
560→402, `alloc_magazine_hit_only_16b` 425→375. This is directionally
consistent with the intended cache-locality improvement, but is NOT relied
on as this report's verdict basis — Callgrind's cache simulation on these
small, mostly-L1-resident benches is a coarse, best-effort signal (per this
suite's own long-standing convention — see `scripts/iai.mjs`'s module doc,
"a missing column must NOT fail the run"), and a 16-way churn loop is
already small enough to mostly fit in L1 regardless of the exact intra-line
offset. It is noted here as an honest observation, not promoted to a
headline claim.

## 5. Correctness verification

- `cargo test --features production` (full tree): **all green**, 0
  failures.
- `cargo test --features "production,virgin-zero-skip"` (full tree): **all
  green**, 0 failures.
- `cargo test --features "alloc-global,virgin-zero-skip" --lib`: compiles
  and passes (0 tests in the lib target itself, confirming the feature
  combination is buildable).
- `cargo test --features "virgin-zero-skip"` ALONE (no `alloc-global`):
  `cargo check --lib` succeeds, but the integration test tree
  (`tests/regression_r4_3_config_conflict.rs` and others) fails to compile
  with `E0432: unresolved import sefer_alloc::registry` /
  `unresolved import sefer_alloc::SeferAlloc` — both gated on
  `alloc-global` (`src/lib.rs:315-317`, `:348-349`), which `virgin-zero-skip
  = ["alloc-decommit"]` (`Cargo.toml:744`) does not pull in. **Not a valid
  standalone feature combination for this crate's test tree** — confirmed
  pre-existing (unrelated to this task's diff: the lib itself compiles
  fine under `virgin-zero-skip` alone; only the test binaries that need
  `SeferAlloc`/`registry` fail, and they need `alloc-global` regardless of
  this task).
- `cargo fmt --check`: clean.
- `cargo clippy --features production --lib --tests --benches -- -D
  warnings`: clean.
- `cargo clippy --features "production,virgin-zero-skip" --lib --tests
  --benches -- -D warnings`: clean.
- `cargo clippy --all-features --lib --tests --benches -- -D warnings`:
  clean.
- `cargo clippy --features production --all-targets -- -D warnings`: fails
  on a **pre-existing, unrelated** `clippy::doc_lazy_continuation` lint in
  `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs:257`
  (introduced by commit `4f897237cf6e4bcbe6a722f5c124890e15f07e82`, not
  touched by this task's diff — confirmed via `git diff --stat`, which
  shows only `src/registry/tcache.rs` changed). Out of scope for this task;
  `--lib --tests --benches` (excluding that one broken example target) is
  clean under every feature combination this task touches.

## 6. Verdict

**GO on correctness/documentation-honesty grounds; NULL on measured
per-op Ir.** The struct now delivers the layout its own doc comment has
claimed since task #53: `count` (and `virgin_mask`, when present) at
offset 0, `slots` at offset 8, both inside the same leading region of a
136-byte stride — 7 of every 8 classes now share a 64-byte line between
`count` and `slots[0..6]`, versus 0 of 8 before. The isolated magazine
hit/push cost is measured at **exactly 0 Ir delta** in both feature
configurations (§4.1) — no measurable per-op win, exactly as the survey's
own F4 entry predicted ("benefit unproven... expect 0 Ir delta"), and this
report does not inflate that into a claim it cannot support. The fix is
landed anyway because (a) it costs nothing (struct size and per-op Ir both
unchanged), (b) it closes a real "doc comment says X, code does Y"
divergence against an already-decided prior optimization (task #53), and
(c) it is now enforced at compile time (§2 point 3) rather than
aspirational, per the survey's own recommendation. `fix(perf)`, not
`perf(runtime)`/`perf(opt-in)`: no runtime algorithm or default changed,
and no measurable speedup is claimed — this is a layout-correctness fix
restoring a documented invariant.
