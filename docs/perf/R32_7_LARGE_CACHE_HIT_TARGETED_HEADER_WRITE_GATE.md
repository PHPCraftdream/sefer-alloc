# R32-7 (task #498) — large-cache HIT arm: targeted `SegmentHeader` field writes instead of a full-struct rewrite

Date: 2026-08-02.

## 0. What this is

This task tracks finding **F12** in
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` ("the large-cache HIT
path rewrites the entire ~130-byte `SegmentHeader` when only ~5 words
actually changed"). `AllocCore::alloc_large`'s large-cache hit arm
(`src/alloc_core/alloc_core_large.rs`) used to build a full fresh
`SegmentHeader` via `SegmentHeader::large(..)` and overwrite the WHOLE
header (144 bytes, confirmed by this task — see §2) with
`Node::write_struct`, even though 4 of that constructor's 8 arguments
(`span_usable`, `reserved_capacity`, `reservation`, `reservation_len`) are,
by the pre-existing code's own comments, carried forward BYTE-IDENTICAL
from the cached slot. Only `magic`, `large_size`, `large_align`, and `bump`
genuinely change (`segment_id` is handled by its own separate 1-word patch,
unaffected by this change).

**This was implemented, correctness-verified (falsification assert never
fired; the UBFIX-6 unregistered-window argument was independently restated
and holds for the new shape), and measured. The result is a real, small,
reproducible `Ir` improvement (−32 Ir per large-cache hit, ~8.5% of the
hit's own marginal cost) confined exactly to the modified arm, with every
kill-gate bench Ir-identical before/after.** This is a `perf(runtime)`
change: the large-cache hit arm is reachable through `production`'s
always-on default feature set (`alloc-decommit` is one of `production`'s
seven components — `Cargo.toml`).

## 1. Falsification (step 1, per the survey's own recipe — run BEFORE any edit)

A `debug_assert_eq!` group was added immediately before the (then still
full-struct) write, reading the header ALREADY at `slot.base` via
`SegmentHeader::read_at` and comparing its `span_usable`/`reserved_capacity`/
`reservation`/`reservation_len` against `slot.usable_size`/
`slot.reserved_capacity`/`slot.reservation`/`slot.reservation_len` — the
exact "carried forward unchanged" premise the whole optimization depends on.

Run under `#[cfg(debug_assertions)]` (dev profile), the full `cargo test
--features production` suite passed with **zero assertion failures** —
including the two tests that most directly exercise cache-hit reuse:
`regression_large_cache_span_usable_stable.rs`'s
`cache_hit_reuse_preserves_physical_span_usable` (asserts the exact same
invariant this falsification pin checks, independently, via a different
route) and `regression_xthread_large_free_layout_mismatch.rs`'s
`xthread_large_free_stale_align_after_cache_hit_reuse_is_dropped`. Also
re-run under `--features "production large-cache-extended"` (the
base+extension combined index space, `regression_large_cache_multi_size_cycle`
and the seven `large_cache_extended_*` tests) — all pass, no assertion
fired.

**Verdict: the falsification assert never fired. The "carried forward
unchanged" premise is TRUE.** The assert is kept as a PERMANENT correctness
pin (not removed after the one-shot check) — it is cheap relative to the
segment-registration work around it, and it guards an invariant a future
field addition to `slot`/`CachedLarge` could silently break without this
pin catching it. See `src/alloc_core/alloc_core_large.rs`, the block
immediately preceding the 4 targeted-write calls.

## 2. Exact-size compile-time pin

The survey's own text notes `size_of::<SegmentHeader>()` has drifted three
times (104 → 120 → 128 → 136) with only a coarse `<= PAGE` bound guarding
it — no exact value pinned anywhere. Confirmed empirically via a
mismatched-array-length compiler-error probe (`[u8; 0] = [0u8;
size_of::<SegmentHeader>()]`, read the "found one with a size of N" error):
**144 bytes**, confirmed identical under `--features production`,
`--all-features`, and `--features experimental`.

Added as a permanent compile-time assert in `src/alloc_core/segment_header.rs`:

```text
const _: () = assert!(size_of::<SegmentHeader>() == 144);
```

A future field addition/removal that changes this value now fails the
build at this line instead of silently invalidating this task's targeted-write
optimization's field-coverage assumption.

## 3. Correctness: restating UBFIX-6 for the targeted-write shape

**UBFIX-6 (M-2, `docs/reviews/2026-07-10-ub-audit-final-synthesis.md`)**
deliberately reordered this call site so the (then full-struct) header
write happens while `slot.base` is still UNREGISTERED — not yet inserted
into `SegmentTable`'s `contains_base` hash table by `register()` (confirmed
by reading `segment_table.rs:257-266`: `register`'s `hash_insert(base)` is
the ONLY thing that makes a base reachable via `contains_base`). Before
that call, no cross-thread reader (`SegmentHeader::magic_at`/`kind_at`/
`large_size_at`/`span_usable_at`, all in `segment_header_views.rs`) can
address this segment at all — the race is closed by construction, not by
per-field atomics.

**Restated for the targeted-write shape:** all 4 new field writes
(`set_magic_at`, `set_large_size_at`, `set_large_align_at`, `set_bump_at`)
run in the SAME unregistered window, strictly before the SAME
`self.table.register(slot.base)` call UBFIX-6 already reasoned about — no
code was inserted between the writes and `register()`. Since no reader can
observe ANY of these 4 fields until `register()` returns, the argument that
made the OLD full-struct write's non-atomicity sound applies identically to
the NEW narrower write: (a) a reader cannot observe a torn/partial write
between the 4 separate field stores (there is no reader at all in this
window), and (b) the RELATIVE ORDER of the 4 writes among each other is
immaterial for the same reason. This is not a new argument invented for
this task — it is the exact UBFIX-6 argument, re-derived to confirm it
still holds after narrowing the write's shape, per the task brief's
explicit instruction not to assume it carries over.

**One subtlety independently checked and confirmed sound:** `magic` is
read CROSS-THREAD via an ATOMIC Acquire load in the steady-state case
(`magic_at`, pairing the recycler's atomic Release store when a Large
segment is deposited into the cache — see that accessor's own doc). The
new `set_magic_at` call is deliberately a PLAIN (non-atomic) store, not an
atomic one. This is sound specifically BECAUSE this call site is the
unregistered-window case, not the steady-state case `magic_at`'s atomicity
exists for — see `set_magic_at`'s doc comment in
`src/alloc_core/segment_header_views.rs` for the full restatement. The
pre-existing full-struct `Node::write_struct` this replaces was ALSO a
plain, non-atomic write at this exact call site (confirmed by reading the
removed code) — this task's targeted write is no less atomic than what it
replaces.

**`kind`/`segment_id` are correctly left untouched.** `kind` was already
`Large` from this segment's PRIOR header construction — a large-cache slot
is, by construction, always a former Large segment (confirmed:
`AllocCore::dealloc`'s Large-branch deposit and `reclaim_large_segment`,
the only two paths that ever call `large_cache_slot_set`, are both gated on
`kind == SegmentKind::Large`, `alloc_core.rs:1498`/`:2021`) — so it does not
need rewriting. `segment_id` keeps its own pre-existing separate 1-word
patch (`Node::write_u32` at `segment_id`'s offset, immediately after
`register()` returns the real slot index), unaffected by this task.

**No reviewer-found gap.** The falsification assert (§1), the correctness
restatement above, and independent re-verification against
`segment_header_views.rs`'s pre-existing top-of-file discipline note (which
already documents the exact `magic`-is-the-atomic-exception distinction
this task relies on) all agree: the change is sound.

## 4. The fix

`src/alloc_core/alloc_core_large.rs`, the large-cache hit arm: replaced

```text
let hdr = SegmentHeader::large(u32::MAX, size, align, slot.usable_size,
    slot.reserved_capacity, bump, slot.reservation, slot.reservation_len);
Node::write_struct(slot.base as *mut SegmentHeader, hdr);
```

with

```text
SegmentHeader::set_magic_at(slot.base, SEGMENT_MAGIC);
SegmentHeader::set_large_size_at(slot.base, size);
SegmentHeader::set_large_align_at(slot.base, align);
SegmentHeader::set_bump_at(slot.base, bump);
```

Three new field-specific accessors were added to
`src/alloc_core/segment_header_views.rs`, following the file's existing
`set_large_size_at` naming/shape pattern exactly:

- `set_large_align_at` — plain `usize` store at `large_align`'s offset.
- `set_bump_at` — plain `usize` store at `bump`'s offset (a `*_at(base)`
  sibling of the pre-existing owner-only `SegmentMeta::set_bump`, needed
  because this call site only has a raw `slot.base` pointer, not a
  `SegmentMeta` handle — the segment is not yet registered).
- `set_magic_at` — plain `u32` store at `magic`'s offset, re-establishing
  `SEGMENT_MAGIC` after the deposit path's atomic zero (see §3's atomicity
  discussion).

All three are `#[cfg_attr(not(feature = "alloc-decommit"), allow(dead_code))]`
(their sole call site lives inside `alloc_large`'s
`#[cfg(feature = "alloc-decommit")]` block — the `large_cache` mechanism
does not exist without that feature).

`set_large_size_at` (pre-existing, already used by the OPT-G realloc grow
path) was reused as-is, no changes.

## 5. Path-activation oracle (R30-8) + layer (R31-0's corrected-layer rule)

Two new `#[library_benchmark]` arms were added to `benches/perf_gate_iai.rs`,
mirroring the pre-existing `alloc_magazine_prefill_only_16b`/
`alloc_magazine_hit_only_16b` shared-prefix design (R23-3):

- `large_cache_prefill_only_4mib` — `LARGE_HIT_CYCLES` (8) rounds of
  alloc(4 MiB)+free(4 MiB) through `HeapCore::alloc`/`HeapCore::dealloc`
  (the SAME `#[doc(hidden)]` test-only export the pre-existing magazine-hit
  pair already uses — `HeapRegistry::claim()`), NOT bare `AllocCore` — the
  real `#[global_allocator]` dispatch chain `SeferAlloc` uses, per R31-0's
  corrected-layer rule (the exact meta-pattern R31-0 fixed after R30-3
  measured the wrong layer).
- `large_cache_hit_only_4mib` — byte-identical prefix, plus ONE more
  terminal alloc(4 MiB), guaranteed by construction (same size as every
  prior alloc in the prefix, single-threaded, single-allocator-instance
  bench body — nothing else can evict or admit a competing slot) to be a
  large-cache HIT, never `alloc_large_slow`.

**Discovered during this task, not assumed from the survey:** the
pre-existing `large_alloc_free_cycle` bench — which the survey's own text
claimed "already exercises exactly this alloc→cache-deposit→alloc-hit
cycle" — does NOT. Reading its body confirms it is a SINGLE alloc+free on
a fresh `SeferAlloc`; it never issues a second `alloc`, so it can only ever
take `alloc_large_slow` (a fresh OS reservation), never the large-cache HIT
arm. This is exactly the kind of survey claim CLAUDE.md's R30-8 rule
requires verifying empirically rather than trusting — confirmed wrong here,
corrected by building the new bench pair instead of reusing the existing
one. `large_alloc_free_cycle` itself is left UNCHANGED and used as one of
the kill-gate benches below (it is provably untouched by this task's diff).

**Independent confirmation via a public-API oracle
(`examples/r32_7_large_cache_hit_activation_oracle.rs`):** reproduces the
exact same workload shape through `SeferAlloc`'s public `GlobalAlloc`
surface (not the doc-hidden `HeapCore` seam the iai bench itself uses) and
reads `SeferAlloc::stats().large_cache_hits` (the public, `alloc-stats`-gated
hit-rate counter) before/after the terminal alloc:

```
hits_before_prefill  = 0
hits_after_prefill   = 7
hits_after_terminal  = 8
terminal_hit_delta   = 1
F12 path-activation oracle PASSED: the large_cache_hit_only_4mib bench's
terminal alloc is a genuine large-cache HIT.
```

(`hits_after_prefill = 7`, not 0, is expected and consistent: every alloc
in the 8-cycle prefill loop after the very first one also hits the slot the
PRIOR cycle's free just deposited — only cycle 1's alloc is a genuine
miss/fresh-reservation.) Run:
`cargo run --example r32_7_large_cache_hit_activation_oracle --features "alloc-global alloc-xthread alloc-decommit fastbin alloc-stats"`.

This satisfies R30-8's requirement twice over: the iai bench's own workload
shape is a structural (by-construction) activation oracle, independently
confirmed by an out-of-band counter read through the public API.

## 6. Measurement

### 6.1 Immutable source identity (CLAUDE.md's R29-6 rule)

- **Base commit:** `2dfeaa30944fb73dedd2365bb90c41ff4c198c5d` (`main` HEAD
  at task start).
- **AFTER** (targeted write): the base commit's working tree + this task's
  full uncommitted diff at measurement time (targeted-write change +
  falsification assert + size pin + new bench pair + activation oracle
  example). `git write-tree` of the staged tree at measurement time:
  `8fa61fd1a4aabd11296607bb878951afb728d79e`.
- **BEFORE** (full write): `git worktree add ../sefer-f12-before
  2dfeaa30944fb73dedd2365bb90c41ff4c198c5d` (isolated worktree, base commit,
  detached HEAD), with ONLY `git diff HEAD -- benches/perf_gate_iai.rs` (the
  bench-only half of this task's diff — the new bench pair, NOT the
  targeted-write change) applied via `git apply`. `git write-tree` of that
  state: `b19c6e6f0b5bcf7d41438143fe8bc8e318a5cb29`. This measures the OLD
  (full-write) hit arm through the EXACT SAME bench source the AFTER run
  uses — the only difference between the two measured trees is the header-
  write call site itself.

### 6.2 WSL/callgrind shared-target-dir caveat (discovered during this task)

`scripts/iai.mjs` uses a single hardcoded `CARGO_TARGET_DIR`
(`/tmp/sefer-iai` inside WSL) regardless of which source tree (main repo vs.
an isolated worktree) invokes it. A first attempt at the BEFORE/AFTER pair,
run without cleaning this directory between the two trees, produced
IDENTICAL numbers for both trees (a false "no change" result) — traced to
cargo/callgrind reusing a stale build from the OTHER tree's prior
invocation rather than rebuilding from the tree actually being measured.
**Every number cited in §6.3 below was captured with `/tmp/sefer-iai`
explicitly removed (`rm -rf`) immediately before each run**, and the AFTER
measurement was independently reproduced THREE times (all three:
`large_cache_prefill_only_4mib` = 6,770 Ir, `large_cache_hit_only_4mib` =
7,115 Ir, byte-identical) and the BEFORE measurement TWICE (both:
prefill = 6,994 Ir, hit = 7,371 Ir, byte-identical) before being trusted.
This caveat is recorded in `scripts/r32_7_large_cache_hit_summary.mjs`'s
own header comment so a future BEFORE/AFTER measurement using this
project's worktree-isolation pattern does not repeat it.

### 6.3 Reproduction

```
# AFTER (main tree, targeted write):
wsl.exe -d Ubuntu-24.04 -- rm -rf /tmp/sefer-iai
node scripts/iai.mjs large_alloc_free_cycle large_cache_prefill_only_4mib \
  large_cache_hit_only_4mib small_churn_16b churn_256b \
  aligned_churn_640b_a128 cold_alloc_free_256x16b
# -> docs/perf/_raw_r32_7_after.log

# BEFORE (isolated worktree at the base commit + bench-only patch):
git worktree add ../sefer-f12-before 2dfeaa30944fb73dedd2365bb90c41ff4c198c5d
cd ../sefer-f12-before
git diff 2dfeaa30944fb73dedd2365bb90c41ff4c198c5d -- benches/perf_gate_iai.rs | git apply
wsl.exe -d Ubuntu-24.04 -- rm -rf /tmp/sefer-iai
node ./scripts/iai.mjs large_alloc_free_cycle large_cache_prefill_only_4mib \
  large_cache_hit_only_4mib small_churn_16b churn_256b \
  aligned_churn_640b_a128 cold_alloc_free_256x16b
# -> docs/perf/_raw_r32_7_before.log

# Derive the summary CSV (asserts the arithmetic, per CLAUDE.md's checked-script rule):
node scripts/r32_7_large_cache_hit_summary.mjs [landing_commit_sha]
```

Raw logs: `docs/perf/_raw_r32_7_before.log`, `docs/perf/_raw_r32_7_after.log`
(both the full `npm run iai`-style report, not truncated). Summary CSV:
`docs/perf/R32_7_LARGE_CACHE_HIT_TARGETED_HEADER_WRITE_GATE_summary.csv`,
produced by `scripts/r32_7_large_cache_hit_summary.mjs` — the one checked
script; it hard-asserts (a) every kill-gate bench's delta is exactly 0,
(b) both the prefill and treatment arms' deltas are negative, and (c) the
per-hit marginal-cost delta (shared-prefix-subtracted) is in a plausible
`[-100, -5]` Ir/hit sanity range, before writing the CSV or printing a
number.

## 7. Result

| bench | before Ir | after Ir | Δ Ir | note |
|---|---:|---:|---:|---|
| `large_cache_prefill_only_4mib` | 6,994 | 6,770 | **−224** | shared prefix (8 alloc+free rounds; 7 of the 8 allocs are themselves hits) |
| `large_cache_hit_only_4mib` | 7,371 | 7,115 | **−256** | prefix + 1 more terminal large-cache HIT |
| `large_alloc_free_cycle` | 4,080 | 4,080 | 0 | kill-gate — never touches the large_cache (fresh OS reservation only) |
| `small_churn_16b` | 8,810 | 8,810 | 0 | kill-gate |
| `churn_256b` | 8,810 | 8,810 | 0 | kill-gate |
| `aligned_churn_640b_a128` | 8,746 | 8,746 | 0 | kill-gate |
| `cold_alloc_free_256x16b` | 50,968 | 50,968 | 0 | kill-gate |

**Per-hit marginal cost (R23-3 shared-prefix subtraction, isolating ONE
hit's own cost from the shared prefill prefix):**

- BEFORE (full-struct write): `hit.before − prefill.before` = 7,371 − 6,994
  = **377 Ir/hit**.
- AFTER (targeted write): `hit.after − prefill.after` = 7,115 − 6,770 =
  **345 Ir/hit**.
- **Δ = −32 Ir/hit removed, an 8.5% reduction of the hit arm's own marginal
  cost** (32 / 377 = 8.49%).

All five kill-gate benches are byte-identical before/after (exactly 0 Ir
delta), confirming the fix is confined exactly to the large-cache hit arm
and does not perturb any other allocation path — including
`large_alloc_free_cycle`, which the survey mistakenly assumed already
exercised this code path (see §5).

**Honesty note on magnitude.** The survey's own estimate ("~10-20% of a
~45 ns cache hit, at most") predicted a SMALL win, and that is what was
measured: 32 Ir out of a ~377 Ir/hit marginal cost is real but modest — not
a headline speedup, a small confirmed correctness-preserving instruction-count
reduction on a hot-ish path. No wall-clock claim is made here (Ir is the
judge per this project's convention for a change this size — see
CLAUDE.md's raw-log-policy notes on when a sub-window/Ir-only judge is
sufficient vs. when a full wall-clock criterion run is additionally
required; this change is small enough, and the mechanism direct enough —
fewer store instructions on the identical code path, no reordering, no
new allocation — that Ir alone is sufficient evidence here, unlike the
R14-3 sub-window-vs-full-round gap that rule exists to prevent).

## 8. Correctness verification

`cargo test --features production`: full suite green (734 lines of test
binary output, zero failures), INCLUDING the falsification assert active
throughout (dev profile, `debug_assertions` on) and the two large-cache-hit
tests named in §1. Also re-run under `--features "production
large-cache-extended"` (covers the base+extension combined index space):
green.

`cargo clippy --features production -- -D warnings`,
`cargo clippy --all-features -- -D warnings`,
`cargo clippy --features experimental -- -D warnings`,
`cargo clippy -- -D warnings` (default/no features): all clean.
`cargo clippy --features "hardened medium-classes" -- -D warnings`
(library-only, no `--all-targets`): clean — see §9 for the pre-existing
`--all-targets` gap this row hits, unrelated to this task's diff.

`cargo fmt --check`: clean.

## 9. Pre-existing CI gap discovered (NOT introduced by this task, NOT fixed here)

While verifying `cargo clippy --all-targets --features "..." -- -D warnings`
for each of the 5 real CI clippy rows, 3 of the 5 (`hardened medium-classes`,
`production`, `--all-features`) failed to compile — traced to pre-existing
bugs in `examples/r31_10_trim_cost_gate.rs` and
`examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`,
entirely unrelated to any file this task touched. Independently
re-verified against `main` @ `2dfeaa3` (this task's own base commit) in an
isolated `git worktree`, BEFORE this task's diff existed — the same
failures reproduce there. Not fixed here (out of scope); filed as item 11
in `docs/CORRECTNESS_OPEN_ITEMS.md` per this file's own convention, so a
future round picks it up.

## 10. Files changed

- `src/alloc_core/alloc_core_large.rs` — the fix (targeted writes +
  falsification assert + restated UBFIX-6 comment).
- `src/alloc_core/segment_header.rs` — the `size_of::<SegmentHeader>() ==
  144` compile-time pin.
- `src/alloc_core/segment_header_views.rs` — 3 new accessors
  (`set_large_align_at`, `set_bump_at`, `set_magic_at`).
- `benches/perf_gate_iai.rs` — 2 new bench arms (`large_cache_prefill_only_4mib`,
  `large_cache_hit_only_4mib`) + stubs for `not(alloc-decommit)`, plus a
  corrective doc-comment on `large_alloc_free_cycle` recording that it does
  NOT cover the large-cache hit arm (see §5).
- `examples/r32_7_large_cache_hit_activation_oracle.rs` (new) — public-API
  path-activation oracle.
- `Cargo.toml` — registers the new example with `required-features`.
- `scripts/r32_7_large_cache_hit_summary.mjs` (new) — the checked
  summary-derivation script.
- `docs/perf/_raw_r32_7_before.log`, `docs/perf/_raw_r32_7_after.log` (new,
  committed with `git add -f` per the raw-log policy) — cited raw evidence.
- `docs/perf/R32_7_LARGE_CACHE_HIT_TARGETED_HEADER_WRITE_GATE_summary.csv`
  (new) — machine-readable companion to this report.
- `docs/CORRECTNESS_OPEN_ITEMS.md` — item 11 (the pre-existing `--all-targets`
  clippy gap discovered in §9).
- `docs/perf/OPEN_ITEMS.md` — F12 marked resolved (see that file's own
  entry for the exact wording).
