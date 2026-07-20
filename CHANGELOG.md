# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### BREAKING CHANGE — `alloc-runfreelist` feature removed

The `alloc-runfreelist` experimental performance feature (PERF-3, the
run-encoded freelist / `RunStack`) has been **removed entirely** — the feature
flag, the source module, the cfg-gated branches in shared hot-path files, the
specialized test files, and the CI job that exercised the gated test bodies.
This is a semver-breaking feature removal, the same treatment the
abandon/adopt substrate got in round4.

**Why.** The feature reached a documented NO-GO verdict (Ф5 honest-reject):
it **regressed every one of the 11 iai benches**, including the four
cold/recycle targets it was designed to improve, by **+23 %–+31 % (Ir)**
instead of the predicted ≥5 % improvement. The wall-clock judge confirmed the
regression direction and magnitude (**+40 %/+43 %** on the 16 B/64 B cold
storm). See `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md` §Verdict for the full
measurement. The feature was never added to `production`, never recommended for
use, and was not under active development; retaining it as a "ready starting
point for a future re-run" was pure maintenance drag — every
small-segment-lifecycle change since had to keep accounting for a
known-losing implementation with its own metadata layout, hot/cold branches
in shared hot-path files, and hundreds of lines of specialized tests. See
`docs/agent_reviews_round5/code_quality_review.md` (finding #5) and
`docs/reviews/2026-07-13-round4-remediation-plan.md` (#97 / R4-5, never done).

**What was removed:**
- The `alloc-runfreelist = ["alloc-core"]` feature declaration (`Cargo.toml`).
- `src/alloc_core/run_stack.rs` (the `RunStack` type, `RunDesc`, `FOOTPRINT`,
  `RUNSTACK_CAPACITY`, and all six accessors `init_in_place`/`push`/`pop`/
  `peek`/`is_empty`/`clear_all`) and its `pub mod run_stack;` wiring in
  `src/alloc_core/mod.rs`.
- The `#[cfg(feature = "alloc-runfreelist")]` arms in `drain_freelist_batch`
  (`alloc_core_small.rs`), `flush_run` (`alloc_core_small_magazine.rs`),
  `decommit_empty_segment` (`alloc_core_small_pool.rs`), the bootstrap init
  (`bootstrap.rs`), the recycle init (`alloc_core_small.rs`), and
  `small_meta_end`/`run_stack_off` (`segment_header.rs`) — collapsed to just
  the shipped (classic linked-list) path.
- The tests `regression_r2_3_run_stack_class_guard.rs`,
  `regression_run_stack_decommit.rs`, `regression_run_stack_drain.rs`,
  `regression_run_stack_flush.rs`, `regression_run_stack_layout.rs`.
- The `cargo test --features "production alloc-runfreelist"` step in
  `scripts/check-all.mjs` and `.github/workflows/ci.yml` (`test-gated-bodies`).

**What was kept (NOT removed):** `docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`
(the experiment's negative RESULT stays as institutional memory per this
project's honest-reject convention) and `docs/design/RUN_ENCODED_FREELIST_PLAN.md`
(the design plan that led to the experiment). The confined-`unsafe` count
dropped by 12 (6 in `run_stack.rs` + 3 in `alloc_core_small.rs` + 1 in
`alloc_core_small_magazine.rs` + 1 in `bootstrap.rs` + 1 in
`alloc_core_small_pool.rs`).

**Migration.** This feature was experimental and never recommended for use;
it was not part of `production` and had no non-test consumer. There is no
migration path because nothing depended on it. Any downstream `Cargo.toml`
listing `alloc-runfreelist` in its feature list will get a Cargo error
("unknown feature `alloc-runfreelist`") and should simply drop the feature.

### BREAKING CHANGE — `AllocCore`/`HeapCore::dbg_push_to_ring` narrowed to `unsafe fn`

`AllocCore::dbg_push_to_ring` and its `HeapCore` thin-delegation wrapper were
safe `#[doc(hidden)]` test hooks — the PRODUCER side of the cross-thread free
simulation — so fully-safe Rust could drive a deterministic stale-note→double-
issue chain under the `production` feature set (round5 `memory_safety_review`
R5-MS-4, HIGH): `alloc` a block, `dbg_push_to_ring` a "remote free" note for it
(no liveness/uniqueness check), `dealloc` it (own-thread free), `alloc`-re-issue
the same address (the hot path pops the freelist before draining the ring), then
`dbg_drain_all_rings` processes the STALE note — the re-issued block's bitmap
reads "allocated", the magazine predicate is always-false on a bare `AllocCore`,
and the generational guard is compiled out under `production`, so drain does
`write_next`/`mark_free` on the LIVE re-issue, yielding two live owners of one
range. No threads, no `unsafe` blocks, no type-system violation downstream — the
unsoundness was in the seam's contract, not any one caller's misuse (R5-F1 had
already fixed a `heap_xthread.rs` caller that misused this seam; this fix closes
the seam itself).

**Why.** The obligation the producer must uphold — "this push is at most one
logical remote free; the block is not freed/re-issued between the push and the
consuming drain" — is exactly the class of caller obligation Rust expresses via
`unsafe fn` + a `# Safety` doc, the same reasoning as R6-MS-1/2
(`dealloc`/`realloc`) and R6-MS-3 (`flush_class`). Under `production` the drain's
own guards are insufficient on their own, so the boundary moved from prose to the
compiler.

**What changed:** both `dbg_push_to_ring` entry points are now `pub unsafe fn`
with full `# Safety` docs and a tier-2 item-level `#[allow(unsafe_code)]` (the
`HeapCore` wrapper is `unsafe fn` too, so the chain is not left reachable
through it — mirroring R6-MS-1/2 making both `AllocCore` and `HeapCore`
`dealloc`/`realloc` unsafe). Every call site across `tests/`/`benches/` got an
`unsafe {}` block and a per-site `// SAFETY:` comment; the honoring callers
(single remote free) state the contract, the defensive callers (deliberate
contract-stress of the drain's `is_free`/magazine/generation guards) state which
guard recovers benignly. The drain side (`dbg_drain_all_rings` and the
`_checked`/`_impl` siblings) is LEFT safe — it is the consumer, and with the
producer now `unsafe fn` a contract-honoring caller can never produce a stale
note, so drain can never hit the chain; its reclaim guards remain defence-in-
depth. The `hardened`-only generational guard is NOT made unconditional — a
contract-honoring caller cannot hit the wrap-1/256 residual, so it stays a
probabilistic misuse backstop, not the primary soundness mechanism. New
`tests/regression_push_to_ring_unsafe_boundary.rs` proves the compile boundary
and the contract-honoring single-owner path. The two-tier confined-unsafe
inventory (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`) grew by
two item-level sites (54 → 56).

### BREAKING CHANGE — `AllocCore`/`HeapCore::{dealloc,realloc}` narrowed to `unsafe fn`

`AllocCore::dealloc`, `AllocCore::realloc`, `HeapCore::dealloc`, and
`HeapCore::realloc` were safe `pub fn`s accepting a caller-supplied raw
pointer and `Layout` with no way to verify the pointer is a live allocation
start, that the `Layout` matches the actual block, or that the block hasn't
already been freed — so fully safe Rust could trigger real memory-safety
bugs (round5 memory_safety_review R5-MS-1/MS-2, CRITICAL, the fifth time this
class of finding was raised, this time with concrete counterexamples):
resurrecting a freed block via `realloc`'s same-class in-place branch,
overlapping `copy_nonoverlapping` UB via a `realloc` racing a LIFO re-issue,
releasing a live `Large` segment via an interior-pointer `dealloc`, and
double-freeing a stale-after-reuse pointer.

**Why.** These preconditions (valid live allocation start, matching layout,
freed at most once) are exactly the class of caller obligation Rust expresses
via `unsafe fn` + a `# Safety` doc, not prose a safe caller can violate
without a compiler warning — the same reasoning as the prior raw-memory-hook
narrowing above, applied to the allocator's two most load-bearing entry
points.

**What changed:** all four methods are now `pub unsafe fn` with full
`# Safety` docs. The `#[global_allocator]` adapter (`SeferAlloc`'s
`GlobalAlloc` impl) is unaffected at the API level — `GlobalAlloc::dealloc`/
`realloc` were already `unsafe fn`; they now call the core methods inside
their existing unsafe context. Every internal call site across `src/`,
`tests/`, `benches/`, `fuzz/`, and `examples/` was updated with an `unsafe {}`
block and a per-site `// SAFETY:` comment. The two-tier confined-unsafe
inventory (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`,
CLAUDE.md/README/ARCHITECTURE/`src/lib.rs`) grew by these four new
item-level sites.

The `#[ignore]`d residual test `regression_xthread_double_free_no_corruption`
(which pinned a known cross-thread double-free residual as RED, tracked
under task #164/X7) was removed: its scenario is a genuine caller-side
double-free, which is now documented caller UB under the `unsafe fn`
contract rather than a soundness gap a safe caller could trigger. The
defence-in-depth regression coverage for the retained M2/X7 defensive
drain logic (which must still degrade a contract-violating double-free
benignly rather than corrupting memory) is preserved via the hardened
sibling test in the same file.

**Migration.** `AllocCore`/`HeapCore` are `#[doc(hidden)]` re-exports, never
stable public API. Any downstream call site now needs an `unsafe {}` block
around `dealloc`/`realloc`; the safety contract itself (valid live
allocation, matching layout, freed once) is unchanged — only its enforcement
moved from prose to the compiler. Code going through the public
`#[global_allocator]`/`GlobalAlloc` surface is unaffected.

### BREAKING CHANGE — registry control-plane fields narrowed to `pub(crate)`

`HeapSlot::state`, `HeapSlot::generation`, and `Registry`'s `slots`/`count`/
`free_slots`/`abandoned_segs` fields were `pub` (reachable through the
doc-hidden `pub mod registry` surface). Narrowed to `pub(crate)` to close
R4-MS-4 (CRITICAL) — a public field let safe downstream code force a
`LIVE → FREE` transition and re-push a slot onto `free_slots`, letting a
second thread's ordinary `claim()` steal a slot a first thread still had
cached, breaking the single-writer invariant `unsafe impl Sync for HeapSlot`
depends on.

**Why.** These fields were never intended as stable public API (every one
carries a "NOT stable public API" doc note and exists only because the
crate's `#[doc(hidden)] pub mod` test-only-export pattern requires the
enclosing module to be `pub`). The narrowing closes a real capability-boundary
gap; it does not change any documented, supported behavior.

**What was removed:** direct field access to the items above from outside the
crate. Replaced with narrow `#[doc(hidden)]` accessors on `Registry`
(`dbg_slot_state`, `dbg_slot_generation`, and one `unsafe fn
dbg_slot_preset_generation` for the one legitimate test that presets a
generation) for the tests that legitimately needed to observe this state.

**Migration.** No production code referenced these fields directly (they were
never part of the crate's supported public API). A downstream crate that was
relying on direct field access — an unsupported use of a `#[doc(hidden)]`
surface — will fail to compile (E0616) and should route through
`SeferAlloc`'s supported API instead; there is no supported use case this
narrowing removes.

### BREAKING CHANGE — public raw-memory test hooks narrowed to `unsafe fn`

Eight doc-hidden `pub fn` hooks (`RemoteFreeRing::{init,over}_test_buffer`,
`RunStack::{push,pop,peek,is_empty,init_in_place,clear_all}`,
`segment_header::{gen_at,bump_gen,init_gen_table_in_place}`,
`alloc_core_small.rs`'s `dbg_corrupt_freelist_head_next`/
`dbg_drain_freelist_batch`/`dbg_alloc_bitmap_bytes_for`/
`dbg_magazine_bitmap_bytes_for`/`dbg_payload_start_for`,
`alloc_core.rs`'s `dbg_unregister`/`dbg_recycle`, `numa::bind_segment`)
accepted a caller-supplied raw pointer/base with an unenforceable prose-only
safety contract — a safe downstream call with an invalid pointer could
trigger a library-side invalid read/write with zero `unsafe` at the call
site (R4-MS-3).

**Why.** The validity/size/alignment/lifetime/exclusivity of a caller-supplied
pointer is fundamentally unverifiable by the callee; that contract belongs in
the function signature (`unsafe fn` + `# Safety`), not in prose a caller can
ignore without a compiler warning.

**What changed:** each hook above is now `pub unsafe fn` with a `# Safety`
doc section. This introduced a second, item-level tier of confined `unsafe`
(alongside the existing 13 module-level seams) — see the source-of-truth
inventory command in `CLAUDE.md`/`README.md`/`docs/ARCHITECTURE.md`/
`src/lib.rs`, now `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`.

**Migration.** These are `#[doc(hidden)]` items, never stable public API. Any
downstream call site now needs an `unsafe { }` block; the safety contract
itself is unchanged (only its enforcement moved from prose to the compiler).

### BREAKING CHANGE — removal of the abandon/adopt substrate

The abandoned-segments / adoption substrate (the unreachable segment-transfer
protocol that predated Phase 12.5's whole-slot reuse) has been **removed
entirely**. This is a semver-breaking API removal. It mirrors the
`LargeCacheMode::{Background, Both}` removal precedent ("make invalid states
unrepresentable"); git history preserves the code if a future
decommit-when-empty policy ever needs to reintroduce segment transfer.

**Why.** The substrate was unreachable on every production path: whole-slot
reuse (Phase 12.5) recycles a slot's `HeapCore` whole on thread exit, so
`abandon_segments` / `try_adopt` were never called. It was also internally
inconsistent even on its own terms — `try_adopt` ignored the result of
`register_segment_internal` (silently proceeding even if registration failed),
`reset_stamp_cache` (documented as required on cross-heap segment transfer)
was never called, and its intrusive linked-list field (`SegmentHeader::next_abandoned`)
was shared with the LIVE `deferred_large` cross-thread-free stack (a separate,
production feature). Retaining it as a "loom-proven basis for a future policy"
was therefore an illusion: the documented future scenario already diverged
from the code's live invariants, and a naive reactivation would clobber the
`deferred_large` stack. See `docs/agent_reviews_round4/code_quality_review.md`
(finding #4) and `docs/reviews/2026-07-13-round4-remediation-plan.md` (#97 /
R4-5).

**What was removed:**
- `HeapRegistry::{abandon_segments, push_abandoned_segment,
  pop_abandoned_segment, try_adopt}` and the private helpers
  `push_abandoned_segment_into` / `abandon_one_segment`
  (`src/registry/heap_registry.rs`).
- `Registry::abandoned_segs` field and the abandoned-head packing helpers
  `pack_abandoned_head` / `unpack_abandoned_head` / `abandoned_head_is_empty` /
  `ABANDONED_HEAD_EMPTY` / `ABANDON_TAG_MASK` / `ABANDON_TAG_BITS` /
  `ABANDON_SEG_SHIFT` / `ABANDON_SEG_SIZE` (`src/registry/bootstrap.rs`).
- `OWNER_STATE_ABANDONED`, `unpack_owner_state`, `unpack_owner_gen`, and
  `OWNER_GEN_MASK` (`src/alloc_core/segment_header.rs`) — used only by the
  abandon/adopt CAS. (`owner_state`, `OWNER_STATE_LIVE`, `pack_owner`,
  `unpack_owner_id`, `OWNER_ID_NONE` are RETAINED: the LIVE owner-id
  resolution path for cross-thread free routing still uses them.)
- The dead adoption forwarders `register_segment_internal` /
  `set_small_current_internal` (`src/registry/heap_core.rs`) and
  `register_segment` / `set_small_current` (`src/alloc_core/alloc_core.rs`)
  — their sole caller was `try_adopt`.
- The tests `loom_abandoned_segs_aba.rs`,
  `regression_abandoned_stack_safe_api_uaf.rs`,
  `regression_abandon_a1_next_abandoned_field_sharing.rs`, and
  `loom_registry.rs` (entirely); the abandon-specific tests in
  `registry_basic.rs` and `regression_gen_table_lifecycle_seams.rs` (Seam 3);
  and the abandoned-head packing Kani proofs in `src/kani_proofs.rs`.

**What was kept (NOT removed):** `SegmentHeader::next_abandoned` (the field)
and `next_abandoned_atomic()` (the accessor), the `ABANDONED_TAIL` sentinel,
and the entire `src/alloc_core/deferred_large/` module — these are the LIVE
`deferred_large` cross-thread-free stack, a separate production feature that
reuses the same header field. Its tests (`loom_deferred_large`,
`regression_xthread_large_free_no_leak`) pass unchanged.

**Migration.** No production code referenced the removed items. Downstream
code that reached the `#[doc(hidden)] pub mod registry` surface and called
`HeapRegistry::abandon_segments` / `push_abandoned_segment` /
`pop_abandoned_segment` / `try_adopt` will fail to compile (E0425/E0061) and
should drop the call — whole-slot reuse (the only production teardown path)
makes segment abandonment unnecessary.

### BREAKING CHANGE — removal of `Default for AllocCore`

The `Default` impl on `AllocCore` (feature = `alloc-core`) has been **removed
entirely**. This is a semver-breaking API removal.

**Why.** `AllocCore::new()` / `AllocCore::new_with_config()` return
`Option<Self>` because the very first thing construction does is a real
multi-MiB OS memory reservation for the primordial segment, which can fail
under memory pressure / OOM / `rlimit`. The `Default` impl hid that
fallibility behind `.expect(...)`, i.e. a panic. Generic code across the
ecosystem treats `T::default()` / `T: Default` as a conventionally-cheap,
infallible operation (`Option::<T>::unwrap_or_default()`, `#[derive(Default)]`
on a containing struct, `mem::take`, collection `resize_with(Default::default)`,
etc.) — none of those call sites expect a multi-MiB syscall plus a latent
panic. The implementation had no internal callers (verified by grepping the
whole tree), so the impl was a footgun for hypothetical generic-bound users
rather than a load-bearing convenience. See
`docs/reviews/2026-07-12-round3-remediation-plan.md` (R3-C / N3).

**What was removed:**
- The `impl Default for AllocCore` block in `src/alloc_core/alloc_core.rs`
  (and its doc comment).

**Migration.** Replace any `AllocCore::default()` (or `T: Default`-driven
construction) with an explicit `AllocCore::new().expect("...")` or
`AllocCore::new_with_config(cfg).expect("...")` — making both the fallibility
and the panic visible *at the call site*, where they belong, rather than
hidden inside a trait impl elsewhere. If you want to preserve the exact old
message, use `AllocCore::new().expect("AllocCore::new: primordial segment
reservation failed (OOM)")`.

### BREAKING CHANGE — removal of `LargeCacheMode::{Background, Both}`

The `LargeCacheMode` enum (feature = `alloc-decommit`) has been reduced to
its single implemented variant, `Lazy`. The `Background` and `Both`
variants — placeholders for a background scavenger thread that was never
implemented — have been **removed entirely**. This is a semver-breaking
API removal.

**Why.** `Background` and `Both` had no implemented behaviour: they were
stored by the builder and silently degraded to `Lazy` at runtime. An
earlier fix (T5) made materialising a heap with either variant `panic!`
at resolution time, but that panic was reachable lazily through the
`GlobalAlloc::alloc` entry point (first-bind materialises the per-thread
heap), which conflicted with the crate's "never panics" guarantee on its
allocation entry points. Removing the variants outright ("make invalid
states unrepresentable") is safer than either the silent no-op or the
panic: there is no longer an unrepresentable promise to reject. See
`docs/reviews/2026-07-12-round3-remediation-plan.md` (решение №2).

**What was removed:**
- The `Background` and `Both` variants of `LargeCacheMode`.
- The resolution-time `panic!` match in `AllocCore::new_with_config`
  (T5's eager rejection) — nothing left to reject.
- The two `should_panic` regression tests
  (`background_mode_panics_at_materialisation`,
  `both_mode_panics_at_materialisation`) in `tests/large_cache_mode.rs`.

**Forward compatibility.** `LargeCacheMode` is now marked
`#[non_exhaustive]`. Reintroducing a variant alongside a real future
background-scavenger implementation will be a *non-breaking* addition,
not another breaking change. Code that constructs `LargeCacheMode::Lazy`
is unaffected; any code that referenced `Background`/`Both` will fail to
compile (E0599 — no such variant) and should drop the reference.

**Migration.** Remove any `.mode(LargeCacheMode::Background)` or
`.mode(LargeCacheMode::Both)` call — `Lazy` (the default, and the only
mode that ever had implemented behaviour) is what both were already
doing.

### BREAKING CHANGE — removal of the `Heap` / `with_heap` public face and the `alloc` feature

The explicit `Heap` type (`src/heap/heap.rs`), its TLS binding `with_heap` /
`with_heap_try` (`src/heap/tls.rs`), and the `alloc` Cargo feature that gated
them have been **removed entirely**. This is a semver-breaking API removal.

**Why.** `Heap` was a thin wrapper around `AllocCore` with no per-thread
magazine cache. The production `#[global_allocator]` face (`SeferAlloc`, backed
by the registry-resident `HeapCore`) already has the magazine fast path and
does not use `Heap` at all. A head-to-head benchmark
(`docs/HEAP_BENCH.md`, preserved as a historical record) showed `Heap` running
~9–12x slower than mimalloc on the steady-state alloc/dealloc hot path — the
gap that triggered the decision to remove `Heap` rather than invest in speeding
it up, since the magazine-backed `SeferAlloc` path supersedes it entirely.

**What was removed:**
- The `Heap` struct and its `impl` (`new`/`alloc`/`dealloc`/`realloc`/
  `alloc_zeroed`/`dealloc_any_thread`/`Drop`).
- The `with_heap` and `with_heap_try` TLS bindings and the
  `RefCell<Option<Heap>>` thread-local.
- The `alloc` Cargo feature (it gated only `Heap`/`with_heap`).
- The `src/heap/` module entirely (`heap.rs`, `tls.rs`, `thread_free.rs`,
  `mod.rs` — all existed solely for `Heap`).
- The `benches/heap_alloc.rs` bench and its `[[bench]]` entry.
- The `regression_with_heap_no_panic` test (tested the `with_heap` no-panic
  contract — coverage of `with_heap` is removed by design).
- The `regression_heap_xthread_large_free_no_leak` test (the `Heap`-face A1
  regression; the parallel `HeapCore`-face regression
  `regression_xthread_large_free_no_leak` remains and covers the same fix).
- The `heap_cross_thread` and `heap_miri_xthread` tests (exercised
  `Heap::dealloc_any_thread`; `HeapCore` does not expose a public cross-thread-
  free entry point, so these cannot be faithfully rewritten without inventing
  new public API. Cross-thread coverage lives on against `SeferAlloc`/
  `HeapCore` via `global_alloc_mt.rs`, `concurrent_stress.rs`, etc.).

**What was rewritten (coverage preserved):**
- `heap_cross_segment`, `heap_diag`, `heap_differential`, `heap_invariants`,
  `heap_soak`: rewrote from `Heap` to `AllocCore` directly (faithful 1:1
  substitution — under the single-thread `alloc` feature `Heap` was a pure
  pass-through to `AllocCore`). The two `with_heap` TLS tests in
  `heap_invariants` were removed (they tested `with_heap` specifically).
- `numa_alloc`: tests 1 and 3 already used `AllocCore` directly (unchanged);
  test 2 (`cross_node_handoff_safe`, which used `Heap::dealloc_any_thread`)
  was removed (cross-thread NUMA-handoff coverage lost — `HeapCore` does not
  expose `dealloc_any_thread`; see "coverage lost" below).
- `stamp_cache` test 3: rewrote from `Heap::dealloc_any_thread` end-to-end
  cross-thread free to a direct `dbg_owner_id_for` stamp readback (preserves
  the OPT-C stamp-cache coverage; loses the end-to-end cross-thread-free leg).
- `regression_xthread_large_free_layout_mismatch`: deleted only the `heap_face`
  submodule (the `HeapCore`-face tests remain).
- `regression_hardened_interior_ptr`: both tests already used `HeapCore`/
    `AllocCore` (not `Heap`); only a doc comment was updated.

**Coverage lost (cannot be faithfully rewritten without new public API):**
- `Heap::dealloc_any_thread` cross-thread free via the explicit-`Heap` face:
  `HeapCore` does not expose a public `dealloc_any_thread` equivalent (cross-
  thread routing lives inside the private `dealloc_routing`, reachable only
  via `SeferAlloc::dealloc`). The miri-targeted `heap_miri_xthread` and the
  `numa_alloc::cross_node_handoff_safe` tests exercised this path directly.
  Miri coverage of the substrate continues via `decommit_miri_cycle.rs`; cross-
  thread NUMA coverage is a decision point for a human (whether to expose a
  `HeapCore::dealloc_any_thread`-shaped public API or accept the loss).
- `with_heap` no-panic reentrancy contract: removed by design (the API is
  gone). The production `SeferAlloc` face has its own reentrancy-safe TLS
  binding (`global::tls_heap`) which is structurally reentrancy-free (raw
  `Cell<*mut HeapCore>`, no `RefCell` borrow state).

**Migration.** Users of `Heap`/`with_heap` should switch to `SeferAlloc`
(`#[global_allocator] static A: SeferAlloc = SeferAlloc;`) or, for direct
substrate access, `AllocCore` (`alloc-core` feature). There is no `Heap`-
shaped replacement with `dealloc_any_thread`; cross-thread free is reached via
the `SeferAlloc` global face.

**Feature rewiring.** `alloc-xthread` and `alloc-global` previously depended
on `alloc`; they now depend on `alloc-core` directly (the `alloc` feature's
only content was `Heap`/`with_heap`, so depending on it would be a no-op).
The `production` feature bundle (`alloc-global + alloc-xthread + alloc-decommit
+ fastbin`) is unchanged in effect.

### Security & compliance remediation (SEC-1 through SEC-6)

A `/fxx` security/compliance audit
([`docs/security/SECURITY_COMPLIANCE_AUDIT_2026-07-06.md`](docs/security/SECURITY_COMPLIANCE_AUDIT_2026-07-06.md),
research-only — no source touched) found the unsafe-confinement, dependency,
secrets, and MSRV claims all VERIFIED as advertised, and ten lower-severity
process/documentation gaps. SEC-1 through SEC-6 close six of them (three
MEDIUM, three LOW). No code defect was found or fixed — the pass hardens
disclosure, CI supply-chain posture, and the user-facing hardened-tier docs.

- **SEC-1 (`c3389de`, #198) — `SECURITY.md` shipped with a non-functional
  e-mail fallback.** The fallback section carried the literal placeholder
  `REPLACE_WITH_REAL_EMAIL` plus a `<!-- PLACEHOLDER: ... -->` banner, and no
  real maintainer address exists anywhere in the repo to source a genuine one
  from (`Cargo.toml` has no `authors`/email field). Rather than invent a
  plausible-looking placeholder, the e-mail fallback channel is **removed
  entirely** (−15 lines); private disclosure now relies solely on **GitHub
  Security Advisories**, which was already the preferred channel and remains
  fully functional.
- **SEC-2 (`94fc4f4`, #199) — `SECURITY.md` supported-versions table was
  stale.** It declared "`0.1.x` (current) — Yes" while the published crate is
  `0.3.0` — literally promising patches only for the `0.1.x` line. Reworded to
  "**Latest `0.x` release (see crates.io)**" so the table does not go stale
  again on the next patch/minor bump.
- **SEC-3 (`c81246f`, #200) — README's X7 residual disclosure was stale.**
  The README "documented residual" paragraph (≈line 701) still cited #164 as
  the pending fix and the `hardened` feature-matrix row (≈line 778) described
  only the H1 interior-pointer guard, with no mention of the X7 generational-
  ring arc that closed the re-issue-before-drain leg under `--features
  hardened`. (The X7 closure and its 1/256 wrap were fully documented
  internally — `DURABILITY.md`, this CHANGELOG, the X7 plan — but absent from
  the surface a security-conscious consumer evaluating `hardened` would
  actually read; audit finding §1.5.) Both passages now state the residual
  taxonomy correctly: two of three legs closed on plain `production` (X2/#164,
  R1), the third closed under `hardened` **except the 1/256 wrap**, which is
  named explicitly as the accepted probabilistic residual-of-the-residual.
  The plain-production residual disclosure is not weakened.
- **SEC-4 (`fd05274`, #201) — `permissions: contents: read` added to all
  three workflows.** `.github/workflows/{ci,release,perf-gate}.yml` previously
  ran with the repository-default `GITHUB_TOKEN` scope (legacy read/write on
  older repos). Traced every job/step in all three files: no job needs
  contents-write — `ci.yml` is checkout+cargo; `release.yml` publishes via the
  separate `CARGO_REGISTRY_TOKEN` secret, not `GITHUB_TOKEN`; `perf-gate.yml`
  caches/uploads via its own scoped backends. Workflow-level `contents: read`
  applied to all three; no job needed a broader override.
- **SEC-5 (`d70cd19`, #202) — new `deny.toml` + CI `deny` job
  (cargo-deny).** Closes audit gaps §1.3 (cargo-audit never run, tool absent
  locally) and §1.6#3/§2.2 (license compatibility manually assessed, not
  machine-checked). `cargo-deny` was chosen over `cargo-audit`-alone because
  it covers both RustSec advisories **and** license compatibility in one
  tool/one job. New `deny.toml` at the repo root: `[advisories]` with a
  narrow per-ID-documented ignore list; `[licenses]` allow-list built from
  cargo-deny's actual report against the current full-feature tree (MIT /
  Apache-2.0 / Zlib — narrower than the audit's manual §2.2 inventory,
  reconciled in the task report; no copyleft found either way); `[bans]`
  permissive (duplicate-version = warn); `[sources]` crates.io-only. At the
  time, two narrowly-scoped dev-only ignore entries: **RUSTSEC-2025-0141**
  (`bincode` 1.3.3 unmaintained; reaches this workspace ONLY through
  `iai-callgrind`, the Linux-only CI perf-gate bench — NOT in the published
  runtime tree) and **RUSTSEC-2026-0173** (`proc-macro-error2` 2.0.1
  unmaintained; same `iai-callgrind` dev-only chain). A third was added later
  this session — see the "CI fixes" subsection below.
- **SEC-6 (`91a6dac`, #203) — SHA-pinned `actions/checkout@v5` in
  `release.yml`.** Scoped to the token-bearing workflow per audit finding
  §1.6#2 (this is the only workflow carrying `CARGO_REGISTRY_TOKEN`, so
  tag-rewrite supply-chain risk matters most here). `actions/checkout@v5` →
  pinned to the exact commit SHA `v5` currently resolves to (verified via
  `git ls-remote`), with a trailing `# v5` comment for readability.
  `dtolnay/rust-toolchain@stable` was **deliberately left tag-pinned** — it is
  a moving branch by design (tracks the latest stable toolchain), and pinning
  it to a SHA would defeat its purpose; the conscious decision is recorded in
  the commit message.

### PERF-1 — README bench-doc sync (`650a3ed`, #205)

The README carried two disagreeing cold-direct tables: the dedicated "Cold
first-touch" section still showed P3-era numbers (16 B 1.60× slower, 64 B
1.15× slower, 256 B parity, 1024 B 1.84× faster), while the main dated
"Performance" table already had the correct post-X-arc re-measurement. A
full-file grep found **five** total occurrences of the stale ratios (the intro
bullet, the P0–P6 narrative, the "where we still trail" callout, the dedicated
Cold first-touch table + prose, and the Honest verdict bullets). All five were
synced to the post-X-arc measured ratios — **2.5× / 2.1× / 1.8× slower on
16 B / 64 B / 256 B cold-direct, 1.12× faster on 1024 B** (measured
2026-07-06 post-X-arc) — each explicitly labeled as post-X-arc vs preserved
P3-era history (the P3-era history is not erased; it carries a provenance
note). Docs-only; no source change.

### PERF-2 — TCACHE_CAP / FLUSH_N sweep: honest-reject (`e6f5112`, #206)

**REJECT (all three candidates).** A `/fxx` research hypothesis (#206 / PERF-2)
proposed that a larger per-class magazine (`TCACHE_CAP`, default 16) would
amortize refill/flush orchestration cost on storm-shaped alloc/free patterns
(the cold first-touch gap vs mimalloc). Tested `TCACHE_CAP = 32 / 64 / 128`
against the default `16` on **both** judges: the 11-bench iai
instruction-count gate and the wall-clock `global_alloc` criterion bench (the
exact 1024-op cold-storm shape the hypothesis targeted). Every candidate
**regressed every bench, including the explicit targets** (cold / recycle /
the `global_alloc` storm), and the regressions grew **monotonically and
super-linearly** with CAP. Pure experiment — **zero source changes survived**
(`git diff` to `src/` empty at the end; this doc is the only new file).
Recorded per the project's reject-with-numbers precedent so the next reader
does not re-run the same sweep blind. Full tables and mechanism in
[`docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md`](docs/perf/PERF2_TCACHE_CAP_SWEEP_EXPERIMENT.md).

- **CAP=32 reproduces X4-A** (the 2026-07-05 reject) within binary-layout
  noise: recycle +32,779 Ir (+18%), churn +22,863 (+28%), cold +25,763 (+21%),
  every other bench regressed too. Mechanism (X4-A's, re-confirmed): each
  refill/flush doubled in size (bigger carve/flush batches, larger `Tcache`
  zero-init at heap claim, longer M2 in-magazine scan); the benches don't
  refill-miss enough to amortize the larger batches.
- **CAP=64** is strictly worse on every bench (monotonic): recycle +88,949 Ir
  (+50%), churn +56,033 (+69%), cold +66,881 (+53%).
- **CAP=128 — super-linear regression; the decisive signal.** The `Tcache`
  struct footprint grows from **6.4 KiB → 50.2 KiB/thread** for `slots` alone
  (`49 × 128 × 8 B`) and **spills L1** — visible in the L2-hit column jumping
  ~160 → ~1000 on `small_churn_16b`: the magazine metadata itself stopped
  being L1-resident. Wall-clock confirmed on the exact storm shape: the sefer-
  vs-mimalloc gap on `global_alloc/16B` **WIDENED from 2.5× to 4.9×** at
  CAP=128 instead of narrowing (64B 2.3×→5.1×, 256B 1.46×→3.9×, 1024B
  0.90×→2.0×). The storm hypothesis's own arithmetic ("1024/16 = 64 refills →
  1024/128 = 8 refills, an 8× amortization win") is overwhelmed by the
  per-refill cost growth (8× larger carve batch + L1-spill). The companion
  predictions also failed against measurement: `churn_256b`/`small_churn_16b`
  were predicted CAP-insensitive but regressed monotonically (the first alloc
  of each iteration triggers a full refill — larger CAP = larger refill batch
  + larger `Tcache` zero-init); `large_alloc_free_cycle` regressed too
  despite doing NO small-block magazine work (pure `Tcache` zero-init at heap
  claim).

**Verdict.** mimalloc's advantage is **NOT a deeper magazine — it is a
structurally cheaper refill** (a `mmap`/page free list with no per-refill
orchestration equivalent), which a larger CAP cannot replicate and in fact
punishes. The CAP parameter is already at its optimum (16); CAP=64 and CAP=128
are the two never-before-measured values and are strictly worse. The shape that
could win is a **cheaper refill, not a rarer refill** — exactly the family
PERF-3 (below) then attempted on the recycle flush→drain path.

### PERF-3 — run-encoded freelist arc (Ф0–Ф5): IMPLEMENTED then honest-rejected

PERF-2 named "cheaper per-block work on the hot recycle path" as the winning
family of attack. PERF-3 was the concrete realization of that family for the
recycle flush→drain path: encode contiguous same-class runs as compact
`(start_off, count)` descriptors so the drain side reconstructs member
addresses by stride arithmetic (`start_off + i*block_size`) instead of pointer-
chasing `Node::read_next` per block. Design:
[`docs/design/RUN_ENCODED_FREELIST_PLAN.md`](docs/design/RUN_ENCODED_FREELIST_PLAN.md).
Verdict (Ф5):
[`docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md`](docs/perf/PERF3_RUN_FREELIST_EXPERIMENT.md).
Five phases, each committed between phases with a zero-trust review by @o46m
(GO each on Ф1–Ф4); the Ф5 measurement is the honest-reject.

- **Ф0 (`2732dfc`, #207) — design doc.** No src/ code; mirrors the X7 plan's
  structure (key insight → fixed decisions → phases Ф0–Ф6 → risks →
  readiness). Targets the "cheaper refill, not rarer refill" family PERF-2
  identified.
- **Ф1 (`5c5b6af`, #208) — `RunStack` storage + Layout.** New module
  `src/alloc_core/run_stack.rs` (`RunStack`, `RunDesc { start_off, count }`
  compact descriptors for contiguous-offset same-class runs,
  `RUNSTACK_CAPACITY = 8` per class, `Layout::run_stack_off` /
  `small_meta_end` shift to carve the RunStack region into segment metadata).
  New **`alloc-runfreelist`** Cargo feature (`= ["alloc-core"]`, **opt-in,
  NOT in `production`**). Storage only — no allocator behavior wired up yet.
  Production-judge 11/11 byte-identical (neutrality gate).
- **Ф2 (`7d5bada`, #209) — flush-side contiguous-run detection in `flush_run`.**
  Under the feature, `flush_run` collects accepted (guard-passing) freed
  offsets and detects contiguous sub-runs to divert into RunStack descriptors.
  **Empirical finding:** the magazine's LIFO refill returns blocks in
  **descending** address order, so a flush batch built from magazine order is
  descending, not ascending, and **in-place adjacency detection found ~0%
  contiguity** on the target `bench_direct_alloc` pattern. **Sorting
  (ascending) recovered 100% adjacency** on the target pattern — so the
  landed detector is sort-then-detect, not in-place. (This finding is load-
  bearing for the Ф5 mechanism analysis below.)
- **Ф3 (`f13ec4b`, #210) — drain-side stride reconstruction in
  `drain_freelist_batch` — "the heart."** Full `#[cfg(feature =
  "alloc-runfreelist")]` / `#[cfg(not(...))]` body split. Feature-on: drain
  RunStack for the class FIRST (pop descriptors, reconstruct member offsets by
  `start_off + i*block_size` instead of pointer-chasing `read_next`, guard
  `bm.is_free(off)` before `mark_alloc` + hand-out — fail-safe skip, never
  panics), THEN drain the classic linked list for remaining `out` capacity.
  The `is_free` guard is **mandatory defense-in-depth** (plan §2.3): the M2
  bitmap stays the ground truth, not the descriptor — a reconstructed offset
  that is somehow not free is skipped, never mis-linked.
- **Ф4 (`3e097be`, #211) — `decommit_empty_segment` clears RunStack +
  drain-overflow fix.** (a) On decommit, `RunStack::clear_all(base)` runs
  before `set_decommitted` — stale descriptors would otherwise reconstruct
  addresses into unmapped payload memory on a later drain (opposite policy
  from X7's gen table, which is deliberately NOT re-zeroed: RunStack
  descriptors are address hints into payload, so stale hints are unsafe, not
  merely stale). (b) Also fixed a narrow **descriptor-overflow-on-drain leak**
  found during Ф3's review: classes with `block_size > 8192 B` could have a
  descriptor larger than a single drain call's `out` capacity — fixed via a
  truncated-remainder pushback (`RunStack::push` of the un-drained tail when
  `out` fills mid-descriptor).
- **Ф5 (`154d1fa`, #212) — THE VERDICT: NO-GO / honest-reject.** Measurement-
  only phase (no source changes). Applied the pre-declared GO/NO-GO gate
  (design doc §3-Ф5) mechanically. The feature **REGRESSED every one of the 11
  iai benches**: the 4 cold/recycle targets (the feature's whole design goal)
  regressed **+23% to +31% Ir** (needed ≥5% **improvement** — `cold_16b`
  +23.04%, `cold_64b` +23.89%, `recycle_16b` +31.03%, `recycle_64b` +31.03%);
  the other 7 regressed **+0.75% to +4.33%** (6 of 7 breach the ≤+1% ceiling;
  only `realloc_grow` +0.75% sits inside it, because its hot path is the
  large-block realloc copy, not the small-block recycle path). Wall-clock
  confirmed on the exact storm shape: **+40.5%** on `global_alloc/16B`,
  **+42.5%** on `64B`, +43.2% on `256B`, +68.9% on `1024B` (criterion's own
  paired `change:` field p = 0.00 < 0.05 on every row). All three NO-GO
  triggers fire simultaneously — not a close call.

  **Root cause (confirmed by @o46m's independent code review):** the landed Ф2
  implementation **AUGMENTS** the classic per-block `write_next` chain-build
  rather than **diverting** from it — every accepted block still pays the full
  classic linked-list cost, and the sort/detect/push/rebuild machinery runs as
  an **ADDITIONAL** pass on top, not instead of it. The single `read_next`
  load the drain side saves per run-member block is dwarfed by this added
  flush-side cost. The **L1-hits column is the smoking gun**: for
  `recycle_alloc_free_256x16b`, ON L1 hits = 336,531 vs OFF 260,773 — a rise
  of **+75,758 L1 hits**, almost exactly matching the +55,593 rise in Ir (the
  new instructions are predominantly L1-resident memory ops: the offset
  array, the sort permutation, the RunStack slots). There is no level of the
  cache hierarchy where the feature wins (L2 flat ~174→~176, RAM flat-to-
  slightly-up 5,335→5,419). The "eliminate the dependent-load pointer chase"
  hypothesis is **refuted**: the pointer chase was already prefetcher-covered
  and cheap (the design doc's own §5 readiness note had flagged this as the
  failure mode). The design doc §1's honesty caveat — "this plan introduces a
  different representation, where hoist is *possible*" — was correct that the
  hoist is possible; the measurement shows it is not *profitable*.

  **Disposition: feature stays OFF / opt-in-only** (`alloc-runfreelist`, NOT
  in `production`; Ф6 is not triggered). Source is **KEPT, not reverted** —
  (1) **zero production cost**: the feature-off build is byte-identical to the
  pre-PERF-3 build (the neutrality gate, verified again by Ф5's baseline
  reproducing the 11-bench reference digit-for-digit); (2) the code is
  **correct, reviewed, and tested** — Ф1–Ф4 each passed @o46m zero-trust
  review, each has dedicated regression tests (`tests/regression_run_stack_*`),
  and the M2-double-free-through-run and decommit-clears-runstack safety cases
  are explicitly covered; (3) the loss is an **algorithmic-cost loss, not a
  correctness loss**, and the algorithm can be revisited — a future "PERF-3.5"
  reworking `flush_run` to genuinely **DIVERT** (skip `write_next` for detected
  run-members — write the descriptor instead of the chain link) rather than
  augment could in principle tip the trade; the storage (Ф1), drain-side
  reconstruction (Ф3), and lifecycle seams (Ф4) are reusable as-is, only Ф2's
  flush-side algorithm would need rework. (Precedent: PERF-2 left no source
  because it temp-edited a constant — nothing reusable to keep; PERF-3 landed
  four phases of real reviewed implementation, and the honest-reject is of the
  *measured outcome*, not the *code quality*.)

**Combined with PERF-2, this establishes:** sefer's remaining small-size gap
vs mimalloc is **not closeable by either a deeper magazine OR a cheaper-per-
block recycle representation of this shape** — the gap is structural in the
refill/flush orchestration itself (`find_segment_with_free` / latch /
carve-batch machinery), which is where a future PERF-4 should look.

### New dev scripts

- **`scripts/bench-table.mjs` — `npm run bench:table` (`73a6b2b`).** Runs the
  comparative wall-clock bench and **always prints the SAME canonical tables**
  (ns/op units, fixed bench set, vs-mimalloc ratio column). Written because
  ad-hoc benchmark tables varied in units/subset/format run to run — once
  causing a spurious apparent "20 ns → 40 ns regression" that was actually a
  µs-per-1024-op-batch vs ns-per-op unit mixup. The canonical table is now the
  single source of truth whenever comparative numbers are asked for.
- **`scripts/check-all.mjs` — `npm run check` (`29087c5`).** Single pre-push
  gate: `cargo fmt --check`, `clippy -D warnings` across all three CI feature-
  matrix entries (`""`, `--features experimental`, `--all-features`), `cargo
  test` under `production` and `production alloc-runfreelist`, then `npm run
  iai` (the deterministic judge). Fails fast at the first red step. Written
  after a push this session shipped 17 commits with a red CI (rustfmt drift
  accumulated across the PERF-3 phases, plus two ci.yml jobs pointing at a
  Cargo feature and test files deleted by task #204 — see the next section)
  that this command would have caught locally in under 5 minutes. It does NOT
  replace CI (CI additionally runs miri, loom, TSan, multi-arch, no_std, MSRV)
  but catches the most common drift class before a push, not after.

### CI fixes — found and fixed via a red CI run this session

A push mid-session went red on CI (Actions run 28846975468); the fixes below
landed in the same session. All are style/lint/drift — zero behavior change
(verified via judge byte-identical + full test suite green on each).

- **`d9767fe` — `cargo fmt --all` + clippy fixes across the CI feature
  matrix.** The PERF-3 arc (Ф1–Ф5) landed real code without a final
  `cargo fmt --all` + full clippy-matrix pass, so CI's fmt and clippy gates
  were red on push. `cargo fmt --all`: mechanical reformat (line-wrapping) in
  `alloc_core.rs`'s Ф2–Ф4 additions and the new `regression_run_stack_*.rs`
  test files. `clippy -D warnings` across all three CI matrix entries:
  `needless_return` (`return k;` → `k` in the `alloc-runfreelist` branch of
  `drain_freelist_batch`, tail position under `--all-features`),
  `manual_is_multiple_of` (`off % MIN_BLOCK as u32 == 0` → `.is_multiple_of(…)`
  in `remote_free_ring.rs`), `bool_assert_comparison` +
  `nonminimal_bool` (`assert_eq!(expr, true)` → `assert!(expr)` in
  `regression_gen_wrap_boundary.rs` / `regression_run_stack_layout.rs` — same
  assertions, same failure messages), `doc_lazy_continuation` (a blank `//!`
  line to split a markdown-list lazy continuation in
  `regression_gen_wrap_boundary.rs` /
  `regression_refill_window_double_issue.rs`), `assertions_on_constants`, and
  `iter_cloned_collect`. Purely style/lint; zero semantic change.
- **`ad1d533` — two CI workflow jobs referenced code deleted by task #204
  (Heap removal).** The `loom (loom_thread_free)` matrix entry passed
  `--features "alloc"` (a Cargo feature that no longer exists — it only ever
  gated the removed `Heap` type; the test's synthetic `Node` model never
  actually depended on it — feature set changed to `""`). The `thread-
  sanitizer` job ran `--test heap_cross_thread` and
  `--test regression_heap_xthread_large_free_no_leak` — **both test files were
  deleted in task #204** (no faithful `HeapCore` substitute existed; see the
  Heap-removal section above). A drift the removal task's own CI runs hadn't
  caught until this session's push.
- **`e1ff1e9` — added RUSTSEC-2026-0204 (crossbeam-epoch) to `deny.toml`'s
  ignore list.** A **new** advisory, unrelated to any change in this session —
  discovered via the `cargo-deny` CI job (SEC-5) failing on push (Actions run
  28848487484). `crossbeam-epoch` 0.9.18's `fmt::Display` impl dereferences a
  raw pointer that can be a null `Shared`/`Atomic` sentinel (fixed upstream in
  ≥0.9.20). Unlike the two existing dev-only ignore entries (bincode,
  proc-macro-error2, both via `iai-callgrind`), this one is **NOT** purely a
  dev-dependency chain: `cargo tree -i crossbeam-epoch` shows both the
  dev-only `criterion → rayon → crossbeam-deque → crossbeam-epoch` path AND a
  direct optional dependency via `Cargo.toml`'s `experimental` feature
  (`dep:crossbeam-epoch`, backing `src/concurrent/hand.rs`'s epoch-reclaimed
  slot). Verified this crate's own code does **not** trigger the vulnerable
  path: grepped `src/` for any `fmt::Display`/`format!`/`{}`-style formatting
  of a `crossbeam_epoch::Shared`/`Atomic` value — none exists; `hand.rs` only
  dereferences these via `.as_ref()`/pointer-load APIs, not the affected
  Display path. The ignore is therefore **sound for current usage**, but
  flagged in the `deny.toml` comment that a future addition of Display/format
  logging on an epoch pointer would silently reintroduce the bug under this
  ignore — re-grep before trusting the note stays valid. A
  `cargo update -p crossbeam-epoch` bump (≥0.9.20) is the proper fix,
  deferred as a dependency-version decision per project convention.

### Round 6 tail — cleanup, module splits, OPT-P0 perf batch (R6-CQ-5..7, R6-OPT-A1..A6, R6-OPT-P0-1..4, R6-REGRESSION)

The tail of round 6 — 21 commits (`e73dbec`..`461fe8f`), 2026-07-14..16 —
applies the same judge-driven methodology as the PERF-2 / PERF-3 arcs above
to three new *axes* the existing benches did not measure: **OS commit
charge**, **cross-thread-free tail latency**, and **the SMALL_MAX
fragmentation cliff**. Each candidate source change was preceded by a
dedicated diagnostic judge (the A-series harnesses), and every change was
measured against the deterministic `npm run iai` instruction-count gate so
zero of these wins came at a throughput cost (confirmed by the
cross-version wall-clock report at the end of the round). Two genuine
regressions the P0 work introduced were found and fixed in-flight and are
documented below as such, not spun — this project's honest-reject
convention.

**R6-CQ-5 — remove future-only dead scaffolding (`e73dbec`).** The
abandon/adopt removal left three executable-but-unreachable scaffolds kept
under `#[allow(dead_code)]` "in case the substrate returns": `HeapCore::
reset_stamp_cache`, the full-reset `AllocCore::decommit_empty_segment`
variant (every production caller uses the `_for_release`/`_impl` pair), and
`HeapSlot::new_uninit` (plus the `HeapSlotRemote::new_uninit` it transitively
dead-coded). All three confirmed zero real callers via whole-tree grep and
deleted. The load-bearing finding: `HeapSlot::new_uninit` *deliberately
diverged* from the real bootstrap — it set `next_free = NEXT_FREE_TAIL
(u32::MAX)` while the real bootstrap relies on the OS-zeroed reservation
and lets `push_free_slot` write `next_free` lazily (RAD-1); the scaffold's
own doc called this an "intentional, observationally-equivalent divergence,"
but a future caller trusting it as documentation would get the wrong initial
state. The actual lazy-`next_free` invariant is already preserved in prose at
`bootstrap.rs:39-49`, so nothing was lost. (Investigated and *not* removed:
`HeapOverflow::new_uninit`, kept alive by `new_boxed_for_test`'s real callers.)

**R6-CQ-6 — purge stale abandon/adopt architecture text (`139d4eb`).**
Docs/comments still described the *removed* abandon/adopt lifecycle,
referencing functions that no longer exist (`abandon_segments` et al.). The
real teardown path is whole-slot reuse (`tls_heap.rs`), not abandon/adopt.
Fixed across `Cargo.toml` (description + `alloc-xthread` feature doc), the
field rename `SegmentHeader::next_abandoned` → `deferred_next` (the field is
actually the live `deferred_large` cross-thread-free stack's link, rippled
through 14 source files + the tests that name it), `HeapCore::id`'s doc, and
README/`ARCHITECTURE.md`/`src/global/sefer_alloc.rs`/`src/registry/mod.rs`.
New guard `tests/no_stale_doc_references.rs::
no_stale_abandon_adopt_substrate_references` bans the removed API's exact
identifiers (`try_adopt`, `abandon_segments`, `push_abandoned_segment`,
`pop_abandoned_segment`, `abandoned_segs`, `OWNER_STATE_ABANDONED`) outside
the two files that legitimately name them in past tense — scoped to exact
identifiers, not the bare word-stems, so it doesn't false-positive on the
live `AbandonGuard` name, the `ABANDONED_TAIL` sentinel, or unrelated prose.
Counterfactual-verified by injecting a forbidden token and watching the guard
fail with the exact injected line.

**R6-CQ-7a/b/c — split the three remaining monoliths (`13a1f86`, `49f3a29`,
`fd2c770`).** Continues round 4's already-precedented "split one type's
`impl { .. }` block across thematic sibling files" pattern, applied to the
last three monoliths (`alloc_core.rs`, `heap_core.rs`, `segment_header.rs` —
round4 R4-10 / round5 code_quality_review #7, both only partially remediated
until now). **Pure code movement, zero behaviour change:** `npm run iai`
shows `Instructions: "(No change)"` on all 12 perf-gate benches against the
persisted baseline after each of the three splits; the two-tier
confined-`unsafe` grep count stayed at 46 (moved `unsafe fn`s keep their
`#[allow(unsafe_code)]` + `# Safety` docs, just relocated). The two
highest-risk moves per split (`dealloc`/`realloc` for 7b, `magic_at`/
`BinTable::head` for 7c) were byte-diffed against the pre-move source. New
sibling files: `alloc_core_core_diag.rs` (391 lines, the non-small-subsystem
`dbg_*` cluster); `heap_core_{alloc,free,tcache,ownership}.rs` (heap_core.rs
1987 → 606 lines); `segment_header_{layout,views,meta_fields,gen_table}.rs`
(segment_header.rs 1759 → 1041 lines). A handful of `private fn` →
`pub(super)`/`pub(crate)` widenings were the only mechanical adjustments
needed to compile.

**R6-OPT-A1..A6 — six new diagnostic judges (the "Stage A" harnesses).**
The round's design rule was *measure before you change*: each P0 source
change was preceded by the dedicated harness that would honestly prove
whatever win it claimed, so a result that isn't visible on the right axis is
not claimed. All six are `harness = false` custom timing-loop binaries or
process-per-sample runners (Criterion's `b.iter()` model cannot express
"alloc N, hold live, batch-time the free with percentiles" nor read this
crate's `dbg_*` counters at precise checkpoints) — measurement-only,
no allocator source touched, confined-unsafe count unchanged at 46.

- **R6-OPT-A1 — Windows commit-charge probe (`6d1b7ce`).**
  `examples/first_alloc_process.rs` measured only Working Set (RSS), never
  Windows commit charge — so a real cost was invisible: on Windows `crates/
  vmem` commits the full exact size of the Registry + inline `HeapOverflow`
  in one `VirtualAlloc(MEM_COMMIT)` (~125 MiB predicted), demand-zero and
  therefore absent from RSS until pages are touched. Added `commit_kib()`
  (reads `PagefileUsage` from the *same* `GetProcessMemoryInfo` call already
  made for RSS; the field was already declared, just never surfaced).
  Measured finding: RSS Delta 1 heap = 120 KiB vs Commit Delta 1 heap =
  132,620 KiB (**129.51 MiB**) — a ~129.4 MiB gap completely invisible to
  the pre-existing RSS-only judge, closely matching the review's ~125 MiB
  prediction. This gap is the quantity R6-OPT-P0-2 is meant to shrink.
- **R6-OPT-A2 — persistent-thread fan-in throughput judge (`6fa6776`).**
  `benches/heap_fanin_persistent.rs` spawns producer threads *once* per cell
  and reuses them (the existing `heap_fanin_production.rs` re-spawned/joined
  per iteration, so thread lifecycle dominated). Matrix T × burst ×
  {active,slow,paused,exited} owner state; reports p50/p99/max per-op
  wall-clock + `DBG_RING_*` overflow/retry/exhausted-per-op. A real
  cross-cell state-leak bug (recycling a heap then re-claiming it inherited
  the prior cell's `RemoteFreeRing`/`HeapOverflow` state via whole-slot
  reuse) was found during the orchestrator's zero-trust re-run and fixed;
  a `verify_repeated_cell_consistency()` regression guard is wired into
  `main()` so the class can't silently return.
- **R6-OPT-A3 — SMALL_MAX cliff independent-alloc sweep (`b6bcaa2`).**
  `benches/medium_size_sweep.rs`: there is a sharp architectural cliff at
  258,752 B (`SMALL_MAX`, last small class) vs 262,144 B (one byte over):
  below it, many objects share one 4 MiB segment via the per-segment
  freelist; above it, *every* object gets its own dedicated 4 MiB span +
  one `SegmentTable` slot. No existing bench measured this. Confirmed the
  cliff directly: at n=64, **258,752 B reserves +4 segments (fragmentation
  0.9871) vs 262,144 B +64 segments (fragmentation 0.0625)** — a 16×
  segment-count ratio matching the ~15-usable-blocks-per-4-MiB-segment
  theory. The harness handles real allocator OOM at n=1024 for post-cliff
  sizes (~4 GiB VA exhaustion on this host) cleanly — the OOM-at-scale is
  itself evidence of the cliff's cost.
- **R6-OPT-A4 — deterministic multi-segment directory judge (`3686412`).**
  `benches/segment_directory_sweep.rs`: `find_segment_with_free_impl` is
  O(segments) on a free-list miss, but the only existing judge
  (`multiseg_cold_256k`) builds just 3 segments — deep in the flat region.
  Three prior optimization attempts (X5, T10, R5-R1) measured ~zero
  improvement against that judge not because the scan is secretly O(1), but
  because nobody ever measured it far enough into S. Confirmed the
  flat-then-rising curve: S=1/3/16 in the 36–140 ns range, S=64 climbs to
  652–3,749 ns, S=256 to 18,590–25,668 ns, S=1023 to **92,716–169,875 ns**
  (kill-gate ratio 3742× post/S=3 vs 1.13× S=3/S=1, so divergence is
  concentrated at high S and the existing small-S IAI judge stays neutral).
- **R6-OPT-A5 — dealloc-only unbound-thread judge (`8248cb0`).**
  `examples/dealloc_only_unbound_thread.rs` + `scripts/dealloc-only-bench.mjs`:
  a worker that only ever receives a pointer and frees it (never allocates
  itself) is a common pattern no existing bench measured. Pre-fix the
  commit-charge ratio (treatment free-only / control alloc-then-free) sat at
  **1.00×** at every cell — both bind identically via `current_heap()`
  regardless of which call triggered it, exactly the pre-fix convergence
  this harness exists to make P0-1's win visible against.
- **R6-OPT-A6 — real-installed-allocator paired A/B/B/A runner (`57bf118`).**
  `examples/paired_ab_{sefer,mimalloc,system}.rs` (three binaries each
  genuinely installing its *own* `#[global_allocator]` — `bench:table`'s
  direct-call comparison is honest but not the codegen shape of a real
  production binary where every allocation routes through one
  `#[global_allocator]`) + `scripts/paired-ab-runner.mjs` (`npm run
  paired-ab`). A/B/B/A ordering per block (not A/B/A/B) specifically cancels
  linear host-drift/thermal trends; default N=20 paired blocks (the
  threshold for resolving <20% claims, matching R5-R2's actual N). The
  mandated same-vs-same control (`--arms sefer,sefer`) was independently
  re-verified at N=12 (t=-0.018 vs crit=2.228, sign test 7/12) — cleanly
  "NOT statistically distinguishable from noise," proving the runner doesn't
  manufacture a false signal.

**R6-OPT-P0-1 — dealloc without binding a heap (`09fe56f`).** `SeferAlloc::
dealloc` unconditionally called `current_heap()`, which for a thread whose
TLS is null (a worker that only ever frees foreign pointers) *claimed a full
registry slot* (`HeapCore::new` → reserve/commit a 4 MiB primordial segment)
just to free one foreign pointer. Extracted `HeapCore::dealloc_foreign_slow`'s
heap-instance-independent routing body into `dealloc_foreign_routing(..,
our_head: Option<…>)`; a new `tls_heap::current_for_dealloc()` maps both
null and TORN states to a `ForeignNoBind` variant that never binds, never
touches the fallback lock, and routes via `dealloc_foreign_routing(.., None)`
(any pointer reaching `dealloc` on a bind-less thread is foreign by
construction). Deliberate, documented trade-off: a TORN thread freeing a
pointer that happens to be fallback-owned now pushes onto the fallback's own
ring instead of taking its direct free path — still correct (ring-push is
safe regardless of caller identity; the fallback drains its own ring
lazily), just marginally less optimal in this already-rare corner. Verified
via RED-counterfactual (reverting to old routing fails both new tests
`dealloc_only_no_bind.rs` / `dealloc_only_no_bind_torn.rs` for the right
reason).

**R6-OPT-P0-2 — chunk the Registry + lazy HeapOverflow sidecar (two rounds,
`e4b3e1d` + `8dc6fe8`).** The Registry held `slots: [HeapSlot; 4096]` inline
as ONE giant `reserve_aligned` reservation, paid in full by every process's
first heap claim — ~125 MiB under `production` (inline `HeapCore` magazine
+ decommit state + inline `HeapOverflow`), committed in one `VirtualAlloc`
call with no OS-level commit-only-touched-pages for a reservation of this
shape. **Round 1 (`e4b3e1d`):** split the slot array into 64 chunks of 64
slots (`RegistryChunk`, new `src/registry/registry_chunk.rs`), materialised
lazily per chunk via the same `CAS(null→SENTINEL)→reserve→publish(Release)
/spin(Acquire)` protocol the old whole-registry ensure used. Commit Delta 1
heap: **~129.5 MiB → ~5.98 MiB (~21.7×)**. **Round 2 (`8dc6fe8`):** the
remaining dominant cost was `HeapOverflow` — a `[AtomicUsize; 2048] +
[AtomicU32; 2048]` pair inline in *every* `HeapSlot` (24 KiB/slot), paid by
every claimed slot whether or not it ever overflows. Split into a small
always-inline "emergency" tier (`INLINE_CAP = 64` entries, 768 B/slot) plus
a lazily-materialised sidecar (`HeapOverflowSidecar`, M5-clean reserve in
the existing `os.rs` seam mirroring round 1's chunk pattern) covering the
remaining 1984 entries, null until first genuine overflow past the inline
tier. Commit Delta 1 heap: ~5.98 MiB → **~4.52 MiB**; combined round 1 + 2:
**~129.5 MiB → ~4.52 MiB (~28.6×)**. The wedge hazard (a producer winning
the tail-CAS for a sidecar index ≥ `INLINE_CAP` and *then* discovering OOM
would strand that index forever) is fixed by calling
`ensure_overflow_sidecar` *before* the tail CAS — on failure, push returns
false without advancing tail, identical externally to "ring full," which
every caller already treats as the documented-sound bounded leak.

**R6-OPT-P0-3a — exact medium size classes, feature-gated (`b98f082`).** Six
exact "medium" size classes (256 / 320 / 384 / 512 / 768 KiB, 1 MiB) added
to `SIZE_CLASS_TABLE` behind a new purely-opt-in **`medium-classes`** feature
(additive over `alloc-core`, **NOT** part of `production` or any default-on
bundle). Eliminates the ~16× segment-count/fragmentation cliff at the old
`SMALL_MAX` boundary for allocations in this range — they now share a 4 MiB
segment with ~4–15 same-class siblings instead of each claiming a dedicated
Large segment. Reuses the existing small-segment substrate verbatim (one
segment, one size class, `BinTable`/`PageMap`/bump-carve) — no new segment
kind, no page-run layer. This is the "cheap first experiment" (radical-optimization
review SS4 sub-task 3a); the larger page-run redesign (3b) is deferred pending
evidence this substrate reuse isn't sufficient. The R6-OPT-A3 judge confirms
the fix at the exact predicted boundary: **16.00× segments/reserved-bytes at
n=64 (258,752 B vs 262,144 B) collapses to 1.00×** with the feature on. Ten
pre-existing regression tests that hardcoded byte sizes (usually 512 KiB)
that silently flipped from "Large" to "Small" under `medium-classes` were
bumped to sizes that stay genuinely Large in every feature combination;
`SIZE2CLASS` went `const → static` (`large_const_arrays` lint once the table
grew ~16 → ~64 KiB), a storage-class fix not a behaviour change.

**R6-OPT-P0-4 — overflow-first remote-free retry inversion (`345fa9b`).**
Inverted the cross-thread-free fallback order in `HeapCore::
push_with_overflow_retry`: try the heap-level `HeapOverflow` second-chance
ring *immediately* on a full segment ring, before any spinning, instead of
exhausting the whole `RING_PUSH_RETRY_SPINS` (8,192) budget against the ring
first. Each failed counted push ticked two locked-RMW diagnostic counters,
so the old policy could burn up to 8,193 checks / 16,386 RMWs on a single
logical free before ever trying the already-provisioned overflow ring (8×
the capacity). New policy: (1) one counted `RemoteFreeRing::push`; (2) on
failure, an immediate `push_to_heap_overflow`; (3) only if *both* fail (rare
double-saturation), a bounded spin-retry against both tiers using new
*uncounted* variants so failed polls inside the loop no longer tax either
ring's diagnostic counters. Common case: **2 checks total instead of up to
8,193**. On the R6-OPT-A2 judge (T=32/63, saturated ring), p99 tail latency
dropped from **tens of ms to hundreds-to-low-thousands of ns (~10,000×)**,
overflow/op near zero. This commit is also the *source* of the two
regressions below — the retry-loop reshape it introduced had a pathological
busy-spin budget that the A-series judges (which own a draining owner) did
not exercise.

**R6-REGRESSION — bound `push_with_overflow_retry`'s wall-clock cost
(`ba34fd5`).** P0-4's bounded retry loop scaled its iteration budget from
`RING_PUSH_RETRY_SPINS` (8,192) to `RETRY_LOOP_ITERATIONS` (2,097,152 =
8,192 × 256) as a *flat, uninterrupted* `core::hint::spin_loop()` busy-spin.
Under sustained double-saturation (both the segment ring and the heap-level
overflow full) with a live-but-never-draining owner (the `owner=paused`
shape A2 models), nearly every push burned most/all of its 2,097,152-
iteration budget purely on CPU — `spin_loop()` is a CPU-level hint (e.g.
`PAUSE`), never an OS-level yield, so it gave the scheduler no chance to run
the stalled owner. Confirmed independently: A2's `--reduced`
T=32/burst=100,000/`owner=paused` cell **burned 4,420 CPU-seconds over ~4
minutes with zero output before being killed**. A first fix attempt (same
total budget reshaped into 8,192-iteration rounds with `yield_now()`
between rounds) did *not* resolve it — `yield_now()` is a scheduling hint
with no other runnable work to hand the CPU to when every contending thread
is itself spin-then-yield-looping (~9 CPU-seconds/wall-second at 32 threads/
16 cores). **Fix adopted:** cap to `RETRY_ROUND_MAX_ROUNDS = 8` rounds of
8,192 tight-spin polls each with a real `std::thread::sleep(200 µs)` OS-level
block between rounds (native only; miri keeps a single pure-spin round).
Round 1 stays a pure tight spin with no sleep before it, so the
moderately-contended actively-draining-owner workload task #136's
high-contention judge exercises resolves within round 1 and pays no new
latency. Only a push that survives 8 full rounds (a push that can genuinely
never succeed once the fixed 2,304 combined ring+overflow capacity is
exhausted with a permanently-stalled owner) concedes to the documented
bounded leak after ~1.4 ms of sleep instead of a multi-second CPU burn. New
`tests/regression_paused_owner_wallclock.rs`; RED-counterfactual (pre-fix
source) lands all 3 attempts at ~20–21 s, GREEN with the fix at 0.7–1.9 s.

**R6-REGRESSION-2 — progress-detected stop condition (`1da4497`).**
R6-REGRESSION fixed the CPU-burn near-livelock but, by cutting the retry
budget to a fixed 8 rounds, *reintroduced* the task #136 throughput
regression under host load — the #136 judge went flaky, `exhausted_delta` up
to 821 during load spikes. Root tension: a *paused* owner (never drains)
needs a FAST give-up, while a *live-but-CPU-starved* owner (IS draining,
slowly) needs PATIENCE. No fixed round/iteration budget can distinguish
them — tuning the number only moves the failure between the two judges.
**Fix:** the retry loop's stop condition is now **drain-progress detection,
not a round count.** Both tiers' drain cursors (advanced *only* by the
owner's drain) are snapshotted before round 1 and re-read after every
fully-failed probe round via two new read-only `pub(crate)` accessors
(`RemoteFreeRing::head_relaxed()`, `HeapOverflow::head_relaxed()` — cheap
Relaxed loads of monotonic owner-advanced cursors; no ring protocol, layout,
or Ordering touched; no new `unsafe`). If either cursor moved, the owner
drained something — the stall counter resets and the push keeps waiting.
Only after `RETRY_STALLED_ROUNDS_GIVE_UP = 128` *consecutive* zero-progress
rounds (~0.3–2 s of continuously observed zero drain progress) does it
concede; `RETRY_ROUND_SAFETY_CAP = 4096` total rounds hard-bounds the wait.
The load-bearing 200 µs between-round sleep is kept unchanged. Each
concession memoizes its `(segment base, ring head, overflow head)` snapshot
in a per-thread const-init TLS `Cell` so a sustained stall does not re-pay
the full patience per push; the memo is written *only* on concession, so any
judge run that satisfies `exhausted_delta == 0` never populates it. K
calibration (measured): K=4 → 6/10 judge failures even on an idle host;
K=32 → 10/10 calm but 3/5 under a 16-thread CPU hog; **K=128 → 10/10 calm +
8/8 under the hog, all `exhausted_delta = 0`**. RED-counterfactual #2
(emulated pre-R6-REGRESSION flat 2,097,152-iteration spin) → paused-owner
wallclock test fails all 3 at 15.2–15.7 s; restored → 95–210 ms calm, 7.9 s
under the hog. The `tests/remote_fanin.rs` harness-1/2.5 liveness fix (the
owner loop now runs until every producer has finished via an `Arc<AtomicBool>`
handshake, draining every 4096 allocs) closes the remaining flake — every
prior failing run's concessions occurred strictly *after* the owner's fixed
N×2-alloc loop had completed, i.e. the paused-owner shape where conceding is
the documented-correct outcome.

**R6-REVIEW residuals — N-way stall memo + doc accuracy (`f27d060`).**
Address the non-blocking findings from an independent `@fl` review of the
P0 wave; no behaviour change on any already-green path. **F2 (perf
robustness):** the fast-concede memo was single-entry — a paused owner of
2+ saturated segments with a producer whose frees interleave across them
(A,B,A,B,…) overwrote the memo every push with the other segment's tuple,
so the memo never matched and every push re-paid the full 128-round patience
(a linear-in-push-count cost the memo exists to bound). Replaced with a
per-thread 4-way cache (`STALL_CONCESSION_WAYS = 4`): const-init, `Copy`,
no allocation; lookup fast-concedes iff *any* slot matches; soundness
unchanged (written only on concession, so a zero-concession run never
populates it; the post-round progress check still resets to full patience
the moment either drain cursor advances). New
`tests/regression_paused_owner_multisegment.rs`: passes in ~0.7 s with the
4-way cache; RED-counterfactual (forced to 1 way) fails all 3 attempts at
**77–105 s** — the exact single-entry thrash F2 fixes. F3/F5/F1/F4 are doc
fixes: `DBG_RING_PUSH_RETRY_EXHAUSTED`'s doc rewritten to the actual control
flow; dead `RETRY_LOOP_ITERATIONS` constant + its references scrubbed;
`ARCHITECTURE.md`'s loom-model count corrected (13 → 16, the 3 missing
entries added); a self-contradicting comment in `registry_chunk.rs` rewritten.

**Cross-version wall-clock report (`461fe8f`).** New
[`docs/perf/R6_CROSS_VERSION_BENCH.md`](docs/perf/R6_CROSS_VERSION_BENCH.md)
+ a README "Cross-version comparison" subsection: a same-harness three-way
comparison of sefer-alloc across published **0.2.1** (tag `sefer-alloc-v0.2.1`),
the tree **immediately before the round-6 wave** (`57bf118`), and current
HEAD (`f27d060`) — full per-family tables with the vs-mimalloc-ratio
methodology (host-drift-normalised) and the 0.2.1 not-apples-to-apples
caveats. **Headline:** every *large* wall-clock win landed between 0.2.1 and
the pre-round-6 tree (OPT-G in-place realloc → ms-scale copy-and-free to
µs-scale; Э6 churn → 256 B/1024 B throughput), **NOT** in the round-6 wave
itself. **The round-6 wave (before-wave → now) is flat-to-slightly-better on
wall-clock throughput and regresses no family beyond host noise**, by design:
it targeted **OS commit charge (≈7.4× lower for the first heap — 33.3 MiB →
4.5 MiB on the `bench:table` harness)**, **cross-thread-free tail latency**,
and **the SMALL_MAX fragmentation cliff** — axes `bench:table` does not
measure (see the A-series judges above). Probable modest wins on the 4 MiB
large-alloc/free path (~30–35% faster, 78/85 ns → 53/56 ns) and the 1024 B
teardown/decommit diagnostic, both inside this host's noise band. The 0.2.1
column was produced by porting the current bench harness onto the release
tag, preserved as the local `bench/0.2.1` branch so 0.2.1 stays
re-measurable. (Note on the commit-charge figure: the A1/P0-2
`first_alloc_process.rs` probe measures a stricter "genuinely fresh single
process" baseline and reports the larger **~129.5 MiB → ~4.52 MiB (~28.6×)**
reduction above; the cross-version doc's 33.3 → 4.5 MiB figure is the same
axis measured in the `bench:table` harness context.)

## [0.3.0] - 2026-07-04

0.3.0 is the first `0.3.x` release (the current crates.io live version is
`0.2.1`; see the yank notes below). It bundles four workstreams, each
implemented with line-by-line zero-trust review, per-fix counterfactual
verification, and a commit between phases: the **P0–P7 perf arc**
(#144–#163, beat `mimalloc` on small/medium), a **reliability, stress &
release-doc pass** (R1–R4 / S1–S3 / D1, #153–#168), **two post-tag review
passes** (#164–#178 — a hardening/H1 pass then a perf/reliability/CI pass
W1–W6, both driven by fresh `/fxx` audits with per-fix counterfactuals), the
**post-review hardening pass** (#129–#143), and the **initial phase A–F pass**.
Sections below are grouped per workstream.

### Performance & correctness — the X-arc (#182–#188, 2026-07-05/06)

The post-W7 arc that attacked the last "cardinal" costs found by a fresh
audit. Judge-driven end to end: every change was measured by the
deterministic callgrind judge (`npm run iai`) against a pinned reference
table, adversarially reviewed, and either kept with numbers or
honest-rejected with numbers (four experiments were rejected — the ledger in
[`docs/perf/IAI_BASELINE.md`](docs/perf/IAI_BASELINE.md) records all
tables so no experiment is re-run blind).

- **X1 — OPT-G in-place Large→Large realloc growth (#182).** When the grown
  size (clamped to `MIN_BLOCK`, symmetric with the #138 consistency check)
  still fits the segment's committed `span_usable`, `realloc` updates the
  header's `large_size` and returns the SAME pointer — zero alloc/copy/
  dealloc. Large reservations round up to whole 4 MiB segments and `vmem`
  commits the entire span, so growth cannot fault; `dealloc` routes Large
  frees by segment kind, so the grown block frees correctly. Shrinks still
  take the slow path (RSS reclaim preserved). An adversarial review caught
  (and a counterfactual test now pins) a MIN_BLOCK-clamp leak the first cut
  had. `realloc_grow`: **1,520,714 → 617,859 Ir**.
- **X2 — #164 narrowed: drain-side magazine check (#183).** The ring↔magazine
  cross-thread double-free residual was closed on its *in-magazine leg*: the
  owner's ring drain now consults an `is_in_magazine` predicate (generic
  closure threaded from `HeapCore` via split borrows) immediately before
  linking, on ALL production drains — refill-miss, the realloc alloc-leg
  (rerouted through the magazine-aware `HeapCore::alloc`; the blind path was
  found by adversarial review), and the dbg seam. A magazine-resident block's
  ring entry is dropped; the magazine copy stays canonical. The *re-issue-
  before-drain* leg is **proven** information-theoretically indistinguishable
  from a delayed genuine cross-thread free (design doc §8 impossibility
  postscript) — full closure needs generational ring entries (X7, hardened,
  future arc). Costs accepted and documented: +~630 Ir one-time bootstrap
  per heap claim, ~+30 Ir per refill-miss; hot magazine push/pop untouched.
  Bonus: `realloc_grow` → **561,912 Ir** (the alloc-leg now hits the
  magazine). loom green model + two new counterfactual regression tests.
  - **Correction (R1, 2026-07-06):** the X2 fix as originally shipped left a
    SECOND, decidable leg open — the **refill-window in-out-buffer** leg.
    `refill_class_bump_impl` pulls freelist blocks into `out[0..filled]`
    BEFORE draining rings; the predicate's `if k == c { return false; }`
    shortcut (justified only by count[c]==0 borrow-safety) was blind to those
    magazine-destined blocks, so a stale ring note was reclaimed → relinked →
    the SAME refill loop re-pulled the block → double-issue at consecutive
    positions. Task R1 closed it by wrapping the predicate with an
    out-membership guard (`is_in_magazine(ptr,k) || (k == c &&
    out[..filled].contains(ptr))`) — zero cost when the ring is empty.
    Counterfactual regression test:
    `refill_window_does_not_double_issue_in_out_buffer_resident_block`
    (reverting the guard → P double-issued at positions [14, 15]). The §8
    impossibility theorem is now correctly scoped to leg 3 only (re-issue-
    before-drain); the taxonomy is three legs, not two.
  - **Cleanup (R2, 2026-07-06):** the X-arc retrospective (C2) flagged
    `AllocCore::realloc` as production-dead yet carrying a full duplicate of
    the OPT-F/OPT-G in-place logic also present in `try_realloc_inplace` —
    an unmarked divergence hazard. Resolved by extracting the shared
    detection into one private helper, `realloc_inplace_fast_path`, called
    by both `AllocCore::realloc` (substrate-level fallback to its own
    alloc+copy+dealloc) and `try_realloc_inplace` (the `alloc-global`-gated
    thin wrapper `HeapCore::realloc` consumes). Single source of truth; no
    behaviour change. The same pass rewrote `HeapCore::realloc`'s doc
    comment, which still described the pre-#164 "delegate to
    `self.core.realloc`" flow, to match the actual body (try_realloc_inplace
    → `HeapCore::alloc` + copy + `HeapCore::dealloc`), and replaced a dead
    `if p != ptr { stamp }` branch (unreachable: `try_realloc_inplace`
    always returns the same pointer) with a `debug_assert_eq!`. MUST-1/A1
    and #169 stamp semantics unchanged; both invariant-guarding suites
    (`regression_realloc_xthread_stamp`, `regression_inplace_large_realloc`)
    stayed green without assertion edits.
- **X3 — judge upgrade (#184).** `scripts/iai.mjs` now surfaces the full
  callgrind metric set (Ir | L1 | L2 | RAM | Estimated Cycles) — Ir counts a
  `udiv` and a cache-missing load identically, cycles do not; the X-arc's own
  memcpy story is the proof (realloc_grow Ir −63% but cycles −47% with RAM
  hits 92,240 → 74,963). New `multiseg_cold_256k` bench (3-segment scan
  judge, seeded for future segment-queue work). `docs/perf/FAULT_PROBE.md`
  records the honest negative verdict on a WSL2 page-fault judge.
- **X4/X5/X6 — four honest-rejects with full tables (#185–#187).**
  Magazine CAP 16→32 (every bench regressed, recycle +32,305 — the target
  itself); a 64-bit bloom gating the M2 in-magazine scan (recycle −19k but
  churn +980 — the won front is not traded); clz `class_for` vs the 16 KiB
  SIZE2CLASS LUT (bitwise-identical over 8.28M pairs, but Estimated Cycles
  regressed on 10/11 benches); a per-segment free-classes bitmap for the
  segment scan (every bench regressed incl. the designated judge). All four
  experiments' mechanisms and revisit-triggers are in the ledger.
- **X-arc headline:** `realloc_grow` **1,520,714 → 561,912 Ir (−63 %)** and
  **7,206,236 → 3,817,567 Estimated Cycles (−47 %)**; all other benches within
  documented cold constants of their pre-arc values; every M2/D1 guarantee
  intact and one double-free leg newly closed. X7 (hardened generational ring
  entries — the only path to the remaining, proven-undetectable double-free
  leg) landed as a follow-up arc; see the "X7" subsection below.

### Hardening — the X7 generational-ring arc (#188–#193, 2026-07-06)

The X-arc closed the *in-magazine* and *refill-window* legs of the cross-thread
double-free residual (X2 #164, R1). The third and final leg — *re-issue-before-
drain* (a block popped from the magazine and re-issued before the owner's lazy
drain catches a stale cross-thread-free note) — is information-theoretically
indistinguishable from a genuine delayed free on the bare `GlobalAlloc`
interface. X7 closes it under `--features hardened` via a per-granule
generation counter: the ring note now carries the block's generation at
remote-free time, and the drain drops a note whose generation no longer matches
the block's current life. Delivered in five phases (Ф1–Ф5), each committed
between phases with a zero-trust review and a production-judge gate.

- **Ф1 (`cdc3361`, #189) — gen table in segment metadata.** A 256 KiB table of
  `AtomicU8` (one byte per `MIN_BLOCK = 16` granule, `#[cfg(feature =
  "hardened")]`-gated) carved into the segment metadata region, below
  `small_meta_end`. Not decommitted with the payload → numbering is continuous
  across decommit-reset; dies only with full segment release. Byte-level
  `gen_at`/`bump_gen` accessors (Relaxed load / `fetch_add(1, Relaxed)`). Miri-
  clean (exposed-provenance standalone-buffer tests). Production-judge 11/11
  byte-identical.
- **Ф2 (`345a2ce`, #190) — hardened ring-entry repack.** The ring's `u32` slot
  entry repacks under hardened to `[gen:8|class:6|off16:18]` (was
  `[off:22|class:10]`). Const-asserts pin the bit layout (sum == 32, gen == 8);
  the `RING_SLOT_EMPTY = u32::MAX` non-collision is structurally guaranteed
  (`class=63` is unreachable: `SMALL_CLASS_COUNT = 49 < 64`). Round-trip +
  field-independence + misalignment-guard regression tests. Non-hardened path
  byte-identical.
- **Ф3 (`d1e91ff`, #191) — the three touches.** (a) issue bumps the gen
  (`bump_gen` at magazine pop + `pop_free`); (b) remote free stamps the current
  gen into the note (`dealloc_routing` Variant-2); (c) drain compares, AFTER all
  existing guards, BEFORE `write_next`: mismatch ⇒ drop. The pinned-red
  `#[ignore]` test `residual_xthread_double_free_no_corruption` (scenario
  A→B→I→D) turns GREEN under `hardened` — the pinned bug becomes the feature
  proof. loom model + `should_panic` counterfactual; production-judge 11/11
  byte-identical.
- **Ф4 (`3b0ed2c`, #192) — lifecycle-seam tests.** Pins the three seams the gen
  table touches: (1) decommit-reset continuity (the table is NOT re-zeroed —
  numbering persists; fresh segments ARE zeroed by `init_gen_table_in_place`);
  (2) recycle/release drops stale notes via the EXISTING `contains_base`/
  `magic_at` guards (the gen table is unmapped before any post-recycle read);
  (3) adopt/abandon — the table travels with the segment unchanged (`abandon`
  touches only `owner_state`, never metadata bytes).
- **Ф5 (#193) — honest costs, wrap boundary, docs sync, final runs.** This
  phase. (a) Published the hardened-tier cost in
  [`docs/perf/IAI_BASELINE.md`](docs/perf/IAI_BASELINE.md): marginal per-op
  cost is **+0.2–0.8% Ir** on the magazine hot path (the per-issue `bump_gen`
  RMW), **+2.6%** on refill-miss paths, plus a one-time **~262k Ir bootstrap**
  per heap-claim (gen-table zeroing) — the published price of the defence-in-
  depth feature (plan §5: "порога 'не хуже' нет — это осознанная плата за
  защиту"). (b) Wrap-1/256 boundary test
  (`tests/regression_gen_wrap_boundary.rs`): pins the EXACT 256-modulus of the
  accepted residual — `stamped_gen == current_gen` is TRUE at k=256 bumps
  (collision), FALSE at k=255/257, const-derived from `ENTRY_GEN_BITS == 8`.
  (c) Docs sync: `DURABILITY.md` (+gen counter inventory row, accepted-residual
  verdict category), `RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md` §8.4 (→
  IMPLEMENTED), `FASTBIN_DESIGN.md` residual banner (→ CLOSED under hardened).
  (d) Final loom/miri runs green across both profiles; TSan deferred to CI on
  push (Linux-only, not runnable on the Windows dev host).

**Residual after X7:** leg 3 (re-issue-before-drain) is closed under
`--features hardened`. The only remaining leak is the **1/256 wrap** (≥256
re-issues of one block without an intervening drain → the stamped gen
coincidentally matches the current gen mod 256) — an accepted probabilistic
residual by design (plan §2.5 rejected doubling the ring footprint for a `u64`
note), pinned to its exact modulus by the Ф5 boundary test. The production hot
path is byte-for-byte untouched (every X7 code path is behind the hardened
cfg). Full phased account:
[`docs/design/X7_GENERATIONAL_RING_PLAN.md`](docs/design/X7_GENERATIONAL_RING_PLAN.md).

### Performance — the P0–P7 "beat mimalloc on small/medium" arc (#144–#163)

A seven-phase perf campaign against `mimalloc` on the two fronts where 0.3.0
lost: cold first-touch of tiny blocks (16–64 B) and 256 B churn. The governing
rule was **every speedup removes a *tautology*, never a *guard*** — no
correctness guarantee was surrendered (M2 exact double/foreign-free no-op, D1
live-count accuracy, A1 cross-thread reclaim, `#![forbid(unsafe_code)]` by
default with `production` = `#![deny(unsafe_code)]` + 8 named seams — all
intact — M2's exact-no-op scope being the live/mapped,
single-legged free, with the cross-thread-double-free ring-in-flight case a
pre-existing documented residual limit, #164); in P6 the M2 guard was
**strengthened for the two own-thread resting places** (magazine + BinTable,
see Э6 below). Each phase was implemented, line-by-line zero-trust reviewed,
counterfactually verified, and committed between phases. See
[`docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md`](docs/perf/PERF_PLAN_beat_mimalloc_small_medium.md)
for the full diagnosis and
[`docs/ALLOC_BENCH.md`](docs/ALLOC_BENCH.md) for the P0→P5 measurement tables.

The six eurekas that landed (P1–P3, P6):

- **Э1 (P3) — bump-direct batched carve — front A's main lever (#147).** A
  freshly bump-carved block already satisfies the M2 bitmap invariant
  (`bit 0 = allocated`); the old refill drove every virgin block on a
  `carve → write_next → bitmap RMW → head-store → pop → read_next → bitmap RMW`
  round-trip through the `BinTable` only to move it to "free" and instantly
  back to "allocated" — a tautology (~40 instructions/block). New
  `AllocCore::refill_class_bump` carves a batch straight from the bump cursor
  into the magazine (`bump += n·block_size`, `live_count += n`) **without
  touching the bitmap** (bit 0 is already correct), ~6–8 instructions/block.
  Source order preserved: freelist / cross-thread ring-drain are still tried
  BEFORE bump-carve, so freed blocks never go stale (no RSS drift). M2
  byte-identical (a double-free of such a block still `mark_free`s, and the
  second free still sees "already free" → no-op); D1 exact (same batch inc).
  The P7 alloc-side bulk-bypass became unnecessary and was retired (the
  dealloc-side bulk-flush is kept). This roughly halved the cold tiny-block
  gap and brought cold 256 B to parity.
- **Э2 (P1) — one-branch teardown resolver (#145).** After #129 every alloc
  compared `p == TORN` (`usize::MAX`) and `p == null` (`0`) — two branches on
  the process's hottest path for a once-per-thread teardown case. Since the
  two sentinels are the range ends, one compare
  (`p.addr().wrapping_sub(1) < usize::MAX − 1`) catches both; the cold split
  (`0 → bind_slow`, `MAX → Fallback`) only runs off the fast path. Semantics
  identical (same #129 counterfactual test), minus a branch.
- **Э4 (P1) — classify once (#145).** `class_for` was recomputed 2–3× per
  alloc and 2× per free; the class `c` (a pure function of size+align) is now
  threaded once through the path (the magazine miss resolves `c` and hands it
  straight to `refill_class_bump(c, …)`; the dealloc overflow resolves `c` once
  and passes it to `flush_class` / `dealloc_small(base, ptr, c)`), removing 1–2
  loads from the 16 KiB `SIZE2CLASS` table plus branches per op. (P1 introduced
  thin `alloc_small_class` / `dealloc_small_class` wrappers for the bulk-bypass
  callers; P3 retired those wrappers with the P7 bypass, but the classify-once
  threading they enabled survives on the live refill/dealloc paths.)
- **Э5 (P1) — a counter that doesn't count (#145).** The per-hit
  `tcache_hits.fetch_add` was a `lock xadd` even after #133 removed the
  *contention* (the owner is the sole writer). Replaced with a
  `load(Relaxed); store(+1, Relaxed)` pair — same atomic visibility for
  `stats()`, no lock prefix. TSan/miri-clean.
- **Exact 256 B size class (P1, #145).** `SMALL_CLASS_COUNT` 48 → 49 adds an
  exact-256 B class (the public size-class type has been a `&'static [..]`
  slice since #136, so this is not a breaking change). This narrows — but does
  not close — the 256 B churn gap.
- **Э6 (P6) — oracle-in-metadata: the 256 B churn loss is ELIMINATED, and M2
  got STRONGER (#150–#152).** The P5 docs blamed the residual 256 B loss on
  "the M2 bitmap price"; that framing was incomplete. The real cost was a
  stale per-heap key (`TCACHE_KEY`) stamped into the freed block's **body**
  (word1) and read back as a magazine double-free fast-path filter. On the
  non-writing churn bench the key survived across the free, forcing a
  slow-path scan on every free AND touching a cold/conflict cache line at the
  256 B stride (the "256 B churn loss" — never the bitmap itself). Э6 removes
  `TCACHE_KEY` entirely: the two exact oracles (in-magazine array scan + the
  `BinTable` `is_free` bitmap line — both hot metadata) now run on every free
  with no block-body filter, and **the free path never touches the block
  body**. This is not a trade — M2 is **strengthened for the two own-thread
  resting places (magazine + BinTable)**: the pre-Э6 flushed-double-free-
  after-user-write hole (a double-free after the user overwrote word1 could
  double-issue) is now CLOSED, because the oracle no longer depends on
  block-body contents. **The cross-thread-double-free ring-in-flight case
  remains a documented residual limit (#164):** the oracles are blind to a
  block whose cross-thread free is still undrained in its segment's
  `RemoteFreeRing` (the ring push sets neither oracle), so an own-thread free of
  such a block still slips through — pre-existing since fastbin, neither opened
  nor closed by Э6, pinned RED by
  `tests/regression_xthread_double_free_residual.rs`. Counterfactual proof: `tests/regression_magazine_oracles.rs`
  test (c) is RED pre-Э6, GREEN on Э6. Bonus: our free path is now cheaper than
  mimalloc's on this pattern — mimalloc writes `next` into the block body on
  every free; we write nothing to it. Cold carve is untouched (Э6 targets only
  the churn free path).

The P7 arc (P7.0–P7.4, #159–#163) — an **instruction-count** optimization of
the steady-state cold recycle path (the freelist round-trip P7.0 isolated —
NOT page faults; at criterion steady state the instance is reused, so the cost
is per-block metadata ceremony on the refill/flush path). Five more eurekas,
each proven **byte-identical** by counterfactual regression tests:

- **Э7 (P7.2) — batch freelist drain in `refill_class_bump`, the main cold
  lever (#161).** One segment's freelist is drained in a **single walk**: the
  head-read, `set_head`, and `inc_live` are hoisted out of the per-block loop
  (one head-store + one live-count update for the whole run). The genuinely
  per-block work stays per block: the dependent `read_next` load and the
  `mark_alloc` bitmap RMW (the M2/D1 guards) still run once per block. The
  drained blocks are byte-identical to the per-block loop's output.
- **Э8 (P7.3) — batch flush in `flush_class` (#162).** Symmetric on the dealloc
  side: same-segment runs flush in one pass with `set_head` and the bump-load
  hoisted out of the loop. Every guard stays per block: `is_free`, `off >= bump`,
  and `dec_live` all still run once per flushed block — no guard collapsed,
  only shared head/bump bookkeeping pulled out.
- **Э9 (P7.1) — classify-once + base-once on the `HeapCore` alloc/free faces
  (#160).** A duplicate `class_for` and `segment_base_of` per op were removed —
  both are resolved once and threaded through. Same values, fewer loads; both
  sides win, risk ~0.
- **Э10 (P7.4) — branchless chunked in-magazine M2 scan (#163).** The
  in-magazine double-free oracle (the Э6 array scan) is now a branchless
  chunked scan — same exact membership test, no per-element branch. M2
  membership is byte-identical; the scan bounds are counterfactually pinned.
- **Э11 (P7.2) — stamp-dedupe (#161).** A redundant owner-stamp on the batched
  drain path was de-duplicated (stamped once for the drained run, not per
  block). Same stamp result.

Э3 (P2, own-segment cache) was implemented and gated but is honestly modest
(the win is skipping the probe arithmetic + a likely L1 miss; `contains_base`
was already O(1)); it does not move the headline tables.

### Measured result (single noisy Windows dev host, criterion FAST profile — ratios are the signal)

- **Cold tiny blocks (front A) — the big win.** 16 B `2.6× → 1.60× slower`;
  64 B `2.0× → 1.15× slower`; cold 256 B reached **parity** (1.03×). Not full parity
  on the tiniest cold sizes, but the tautological carve→BinTable→pop round-trip
  is gone — what remains is honest per-block work (page-map writes, page faults
  on genuinely fresh pages).
- **Churn tiny blocks — lead widened.** 16 B `1.26× → 1.63× faster`; 64 B
  `1.23× → 1.69× faster` (Э2 + Э4 + Э5 compounding on the hit path).
- **256 B churn (front B) — the loss is ELIMINATED (Э6, P6).** Through P5 the
  exact-256 B class only narrowed this from `1.25× → 1.16× slower` and never
  overtook. Э6 removed the real cause (the stale block-body key, not the
  bitmap): on the artificial **non-writing** pattern 256 B churn reached
  **≈ parity** (`~1.03×`, was 1.16–1.25× SLOWER), and on the realistic
  **writing** pattern (`global_alloc_churn_write`, new in P6.0 — real code
  writes to what it allocates) sefer-alloc now **leads at every size**:
  16 B 1.63×, 64 B 1.69×, **256 B 1.14× faster**, 1024 B 5.42× faster. The
  earlier "honest ceiling" framing (256 B is the M2 bitmap price) is retired —
  the price was a per-heap key in the block body, and it is gone.
- **Cold tiny (16–64 B) — unchanged, still trails 1.15–1.60×.** Э6 does not
  touch the cold carve path (page-fault-bound honest per-block work); no claim
  of improvement there.
- **Large (≥1 KiB) — the crushing lead is retained.** Cold 1.84× faster,
  churn 5.42× faster (writing) / retained; the OPT-E large-cache headline
  (13–34× at 4/16/64 MiB) is unchanged.
- **P7 cold recycle — an instruction-count reduction; wall-clock MODEST and
  within noise on this host (no overclaim).** P7 batches the freelist
  drain/flush (Э7/Э8), classifies once (Э9), and makes the M2 scan branchless
  (Э10) on the steady-state cold recycle path. On this noisy single-host
  wall-clock the cold-tiny numbers moved only within run-to-run noise: 16 B
  `1.60× → ~1.5× slower`, cold 256 B `parity → ~1.06× faster`, 64 B unchanged
  (`~1.15×`) — the 16 B row alone spanned 18–24 µs across samples. **We do NOT
  claim the plan's projected ~1.1–1.2× cold-tiny figure as achieved** — the
  wall-clock on this machine cannot cleanly resolve the per-op instruction
  savings. The real, DETERMINISTIC proof is the iai `Ir` gate on Linux CI (see
  the `recycle_*` benches below); the P7 cold verdict is **pending that gate**.
  Churn (the won front) is **UNREGRESSED** (16 B still ~1.6× faster, 256 B
  still ≈ parity). Guarantees intact: the batching removed only shared-
  bookkeeping tautologies and kept every per-block guard (`is_free`,
  `off >= bump`, `mark_alloc`, `dec_live`); M2 / D1 / A1 /
  `#![forbid(unsafe_code)]` by default (`production` = `#![deny(unsafe_code)]`
  + 8 named seams) all hold.

The rigorous, DETERMINISTIC proof is the `perf_gate_iai` instruction-count
gate (Valgrind, Linux-only CI): the P0 benches
(`cold_alloc_free_256x16b` / `_256x64b`, `churn_256b`, #144), the P6
`churn_write_256b` bench (#150), and the P7.0 two-round
`recycle_alloc_free_256x16b` / `_256x64b` benches (#159 — round 2 drains what
round 1 freed, isolating exactly the Э7/Э8 recycle path the single-round
`cold_*` benches are blind to) exist for exactly this and confirm the per-op
`Ir` deltas; their `Ir` baseline is captured on the first Linux perf-gate run.
The P7 cold verdict specifically is **pending this Linux Ir gate** — the
wall-clock numbers above are noisy comparative measurements from a single
noisy Windows dev host, not a statistical suite.

### Reliability, stress & release-doc pass (R1–R4, S1–S3, D1 — #153–#168)

A post-perf pass that hardens the guarantees, adds adversarial boundary
coverage, and reconciles the release docs — strictly from the safe
`GlobalAlloc` envelope (each block freed exactly once, same layout; misuse
from `unsafe` callers is out of scope). No correctness guarantee was
weakened; M2 was *strengthened* in R1.

#### Fixed

- **R1 — the magazine-push `off >= bump` guard closes a real M2 gap.** The
  Э6 in-magazine free path could push a not-yet-carved (`off >= bump`)
  offset into the per-thread magazine, from which a later alloc could hand
  out a block the substrate never carved. The push now rejects any
  `off >= bump` offset (byte-identical to the flush-side guard).
  Counterfactual-pinned by `tests/regression_magazine_bump_guard.rs` (RED
  without the guard).

#### Changed — honesty of the M2 scope

- **R2 — the ring↔magazine cross-thread double-free residual is documented,
  pinned, and modelled (real fix tracked as #164).** A block whose
  cross-thread free is still in-flight in a segment's `RemoteFreeRing` (not
  yet drained by the owner) sets neither own-thread oracle (it is in neither
  the magazine `slots` scan nor the `BinTable` `is_free` bitmap), so a
  concurrent own-thread double-free of it is not detected. This is a
  pre-existing limit (present in the live 0.2.1 `fastbin` too), NOT
  introduced by the perf arc. Pinned by
  `tests/regression_xthread_double_free_residual.rs` (`#[ignore]`), modelled
  by `tests/loom_magazine_ring_compose.rs` (loom also showed the naive
  "own-free reads the ring" fix is itself holed — the real fix must let the
  drain see the magazine, hence #164). `docs/INVARIANTS.md` / README now
  qualify "never UB" to live/mapped memory and reference this residual.

#### Internal — verification

- **R3 — `production` is now covered by sanitizers in CI:** a ThreadSanitizer
  job on the `production` feature set plus `miri` over the `fastbin` magazine
  tests (and loom variants). Zero races, zero UB.
- **R4 — code-doc hygiene:** stale `40`→`49` size-class counts, the slot-0
  FIFO wording, the unsafe-seams comment, and stale `realloc` / no-`Box`-on-
  bind notes corrected across the substrate source.
- **S1 — bounded concurrent boundary-stress harness**
  (`tests/stress_concurrent_boundaries.rs`): multi-thread hammering of the
  class / align / segment seams with allocation canaries + distinctness +
  M2/D1 assertions, all from the safe envelope. Bounded to ~1 s by default; a
  heavier run is opt-in via `SEFER_STRESS_HEAVY` / `SEFER_STRESS_OPS` /
  `SEFER_STRESS_MAX_THREADS`.
- **S2 — deterministic single-thread exhaustive boundary sweep**
  (`tests/stress_boundary_sweep.rs`): every class/align seam × a realloc
  matrix (~2100 cases in ~0.5 s; the grid auto-reduces under `cfg(miri)`).
- **S3 — the stress harnesses run under sanitizers in CI:** S1 under TSan,
  S2 under miri, with reduced budgets so CI stays fast. Neither S1/S2 nor the
  sanitizers found any new bug.
- **D1 — release-doc accuracy pass** (docs-only): the unsafe-seam inventory
  (+`registry::bootstrap`), the M2 scope, purged env-vars, `production` =
  `+fastbin`, the 1024-segment-ceiling reframe, and every verification
  counter were reconciled against verified ground truth before the tag.

### Post-tag-review pass (#167 H1, #164 design, C2-regression fix)

A second four-agent review of the fully-composed 0.3.0 tree, each finding
verified by a personal counterfactual before commit.

#### Added

- **`hardened` feature (#167 / H1) — opt-in defence-in-depth against
  UNSAFE-CALLER misuse, default OFF, NOT in `production`.** Adds an
  interior-pointer free guard on **both** own-thread free faces — the
  `SeferAlloc` per-thread magazine (`HeapCore`) and the substrate
  (`AllocCore::dealloc_small`, which the explicit `Heap`/`with_heap` face and
  any direct `AllocCore` user reach): a free of a pointer that is not a block
  start (`off % block_size(class) != 0`) becomes a detected no-op instead of a
  mis-indexed bitmap read → magazine double-issue / free-list corruption. The
  check is a modulo-per-free (a real division), so it is honestly a paid check
  and stays behind the feature — the `production` hot path is byte-identical.
  The cross-thread leg is already covered unconditionally by `reclaim_offset`.
  Other misuse vectors were cost-evaluated and honestly rejected (mismatched-
  layout free needs a per-block size word — reintroduces the block-body write
  Э6 removed). Pinned by `regression_hardened_interior_ptr`.

#### Fixed

- **C2 realloc regression (a tag blocker, found by the review):
  `HeapCore::realloc`'s own-segment branch bypassed segment-ownership stamping
  and the A1 deferred-large drain.** The 0.3.0 C2 optimization delegated
  own-thread realloc straight to `AllocCore::realloc`, so a Vec grown via
  realloc lived in an UNSTAMPED Large segment (`owner_thread_free == null`); a
  cross-thread free of it silently no-op'd → the 4+ MiB segment and its slot
  leaked → cumulative `MAX_SEGMENTS` exhaustion → abort. The resurrected
  A1/#114 leak-to-abort, on the everyday "Vec grows on one thread, freed on
  another" pattern. Fixed by mirroring `alloc`'s two hooks (stamp the result
  when it relocated; drain when the new size is Large). Pinned by
  `regression_realloc_xthread_stamp`.
- **`AllocCore::reclaim_offset` panicked instead of skipping on a garbled ring
  entry.** The class field carries 10 bits (0..1023) while only 49 classes
  exist; a corrupted entry indexed `SIZE_CLASS_TABLE` out of bounds → panic
  inside the `#[global_allocator]` → abort, violating the function's own
  documented "no abort — just skip" contract. Fixed with a class-bounds check.
  Pinned by `regression_reclaim_offset_garbled_class`.

#### Internal

- **CI blind spots closed:** a `windows-latest` `production` job (the
  aligned-vmem `VirtualAlloc`/`MEM_DECOMMIT` path was only tested locally),
  the workspace member crates' own suites (`aligned-vmem` etc.), an MSRV
  (1.88) `cargo check`, a `production hardened` matrix row, aarch64 gains a
  `production` cross run, and `release.yml` gains a tag==version guard + a
  pre-publish test gate (and a fix to the root-crate `cargo pkgid` version
  parse). The `loom_magazine_ring_compose` model and the `hardened` row were
  also wired into CI.
- **SAFETY / doc-rot corrections** (docs-only): the `TORN` (#129) SAFETY
  comment no longer rests on the false "reverse TLS declaration order"
  guarantee (rewritten to the three real reasons); the stale
  `install_thread_free` "Box-allocates" claim corrected.

### Second review pass — perf, reliability & CI hardening (W1–W6)

A second `/fxx` review of the fully-composed tree. A **deterministic
instruction-count (`Ir`) judge was built first** (W1) so every perf change is
proven on the noisy Windows dev host *before* push, not left to Linux CI. Each
change was verified by a personal counterfactual and committed between phases.

#### Performance

- **W4 — `carve_batch` + batched `dec_live`: the cold 16–64 B path drops
  ~6.3k `Ir`.** A cold refill carved blocks one at a time through `carve_block`,
  paying a runtime `align_up` division on the loop-carried `bump` dependency
  chain plus a per-block `SegmentMeta` view, `bump` load/store, `is_decommitted`
  check, `inc_live`, and page-map probe — most of them tautological after the
  first block of a run. `AllocCore::carve_batch` carves a whole run from the
  bump cursor in one shot (one `align_up`, one `set_bump`, one `add_live(n)`,
  one recommit check, page-map marking only on a page change), byte-identical to
  the per-block loop (the alloc bitmap is never touched — a bump-carved block is
  already `bit0 = allocated`, so M2 is untouched; D1 exact `+n`; same SEGMENT
  boundary, page dedication, and decommit recommit-on-reuse). `refill_class_bump`
  also drops a now-dead redundant freelist re-read after the `free_exhausted`
  latch. `flush_run`'s per-block `dec_live_and_maybe_decommit` becomes one
  `sub_live(k)` + a single decommit check (`live` reaches 0 only at the last
  accepted block, so the decommit decision is identical). Measured (W1 Ir judge,
  `production`): `cold_alloc_free_256x16b` 129,863 → 123,516 (−6,347),
  `cold_alloc_free_256x64b` −6,350, `recycle_alloc_free_256x16b` −6,254; churn
  is unregressed. Two candidates were **honest-rejected with numbers**: a
  `REFILL_N` const LUT regressed cold +32 Ir vs the inlined `udiv`; a
  `heap_core` branch-fold was not a self-contained `match`. Pinned by
  `regression_carve_batch` (+ `alloc_core_differential` M1–M4 and
  `regression_magazine_oracles` M2).
- **W3 — `alloc-stats` gate: `production` lands *below* the pre-W3 baseline on
  the hit-heavy benches.** The per-hit `tcache_hits` / `large_cache_hits`
  increments are now gated behind a new `alloc-stats` feature (default OFF, NOT
  in `production`). With it off, the magazine (churn) and large-cache hit fast
  paths carry no counter bookkeeping and those two `stats()` fields read `0`
  (all other `stats()` fields unaffected; the counter storage always exists in
  the slot, so toggling never changes layout/ABI). Gating the bump out brings
  `production` below baseline: `small_churn_16b` −59, `churn_256b` −59,
  `recycle_256b` −477, `cold_256b` −236 Ir. Enable
  `--features "production alloc-stats"` to poll the counters.

#### Fixed

- **W3 — closed a Stacked-Borrows aliasing gap in the stats aggregators.** The
  process-wide `stats().tcache_hits` / `.large_cache_hits` aggregators read each
  heap's counter through `(*heap_ptr).…`, materialising a shared
  `&HeapCore`/`&AllocCore` over a struct the OWNING thread concurrently holds a
  protected `&mut` into — a foreign read of a protected `Unique`: UB under
  Stacked Borrows (miri's default model), fine under Tree Borrows. The two hit
  counters now live in the shared, `Sync` `HeapSlot` (already read by the
  aggregator via `&HeapSlot` for `initialised`); the owner increments them
  through a stable `&'static AtomicU64` planted at `HeapRegistry::claim`, and the
  aggregators read the slot's atomic directly — no `&HeapCore` is ever formed.
  Personally verified under miri: the old shape is SB UB, the new shape is
  SB-clean (`regression_w3_stats_aliasing_miri`).
- **W2 — `SegmentTable` tombstone-rebuild: killed a long-horizon probe cliff.**
  The open-addressing `contains_base` hash tombstoned deleted entries but never
  converted them back to empty, so `#empty` was monotonically non-increasing:
  every register/unregister cycle with a fresh base (large-cache eviction,
  decommit-recycle, ASLR) consumed one empty slot forever. Once `#empty` hit 0,
  a `contains_base` MISS — the hot case, since every cross-thread free begins
  with one on the caller's own table — probed the ENTIRE table. A long-running
  server (the DBMS/async profile the crate targets) degraded to ~`HASH_CAPACITY`
  metadata loads per cross-thread free. Fixed with an exact tombstone counter
  that rebuilds the hash from the live slot set once tombstones exceed
  `HASH_CAPACITY/4` (O(1) amortised per delete; the read path stays branch-free).
  Membership is transparent across rebuilds. Ir byte-identical on all hot benches
  (zero instructions added to the measured paths). Pinned by
  `regression_segment_table_tombstone_rebuild`.

#### Internal — tooling, CI, docs

- **W1 — a deterministic WSL `Ir` judge (`npm run iai`, `scripts/iai.mjs`).**
  Drives the Linux-only `benches/perf_gate_iai.rs` through WSL under
  `valgrind --tool=callgrind` and tables the per-bench instruction count.
  Instruction counts are byte-deterministic run-to-run, which makes this a judge
  on the noisy Windows host where wall-clock is not. `docs/perf/IAI_BASELINE.md`
  records the reference table.
- **W5 — MSRV / macOS / fuzzing.** Silenced a `cargo +1.88 check --all-features`
  dead-code false-positive on `ABANDON_SEG_SIZE` (an MSRV-invisible `const _`
  assert reference); added a `macos-latest` allocator job (real Darwin runs the
  `madvise(MADV_DONTNEED)` decommit path) plus an honesty note that XNU
  `MADV_DONTNEED` is lazy (RSS reclaim best-effort; correctness unaffected as
  `alloc_zeroed` zeroes explicitly); widened the fuzz align corridor to
  `2^0..2^21` (exercising #130's large-align math), added a third fuzz target
  `heap_core_ops` (the fastbin magazine via the `SeferAlloc` `GlobalAlloc`
  face), seed corpora, and a build-only `fuzz-build` CI job.
- **W6 — sanitizer coverage gaps.** Added a plain-provenance `miri-plain` CI job
  for the exposed-provenance intrusive stacks (A1 `deferred_large` /
  `abandoned_segs`), which the strict-provenance miri jobs cannot validate by
  design; and added the two Large cross-thread tests
  (`regression_realloc_xthread_stamp`, `regression_heap_xthread_large_free_no_leak`)
  to the ThreadSanitizer list.

### Long-run durability pass — counter-wrap hardening (W7)

Auditing what happens on ultra-long runs (days/weeks of uptime, billions of
ops): every monotonic/wrapping counter was enumerated and its wrap boundary
either made unreachable (widen/repack, at proven-zero hot-path cost) or pinned
and tested across the boundary. **Honest framing: none of these was a live bug
today** — the pass makes long-run robustness auditable and future-proof. The
full inventory is [`docs/DURABILITY.md`](docs/DURABILITY.md).

- **W7a — `HeapSlot::generation` → `AtomicU64`; `TaggedPtr` repacked to
  `index:16 | tag:48`.** Generation wrapped at 2^32 thread-deaths (reachable on
  a thread-per-request server over months) — though it turned out to be consumed
  only by a `== 1` first-materialise gate, with the stale-TLS hazard actually
  guarded by the `TORN` sentinel, so the wrap was defence-in-depth, not a live
  ABA. The `free_slots` ABA tag was 32-bit (the documented probabilistic wrap);
  repacking the index half from 32 to 16 bits (MAX_HEAPS = 4096 needs 13, pinned
  by a `const` assert) gives the tag 48 bits → wrap at ~89 years. Generation is
  Ir byte-identical; the repack is a uniform −4 Ir (a *decrease*, from the
  cheaper bootstrap `empty()` constant — cold path). Boundary tests in
  `regression_counter_wrap` preset each counter near its limit and cross it.
- **W7b — pinned the `RemoteFreeRing` u32 cursor wrap.** The per-segment ring's
  `head`/`tail` genuinely wrap on a long run (2^32 cross-thread frees on one hot
  segment — reachable), but the ring is wrap-SAFE by design (`wrapping_sub`
  occupancy + `i % RING_CAP` indexing, whose continuity across `u32::MAX` needs
  `2^32 % RING_CAP == 0`). That power-of-two dependency was unstated — now a
  `const` assert — and `regression_ring_cursor_wrap` drives the real ring across
  the boundary (FIFO order, full-ring overflow, occupancy, and a concurrent
  hammer). Counterfactuals confirm both guards bite. Ir byte-identical.
- **W7c — `docs/DURABILITY.md`.** The authoritative counter inventory (width /
  wrap semantics / reachability arithmetic / verdict / covering test) and the
  rule that a new monotonic counter lands only with a row here + a
  boundary-crossing test, proven Ir-neutral.

### Post-review hardening pass (#129–#143)

This and the phase A–F pass below hardened 0.3.0 before its first publish:
the post-review pass (#129–#143, 2026-07-02/03) driven by a four-agent audit
with per-fix counterfactual verification, and the phase A–F pass
(2026-06-30). Entries are grouped per pass.

#### Fixed

- **#129 — BLOCKER: `tls_heap`'s stale-LOCAL TLS resolver could hand out two
  `&mut HeapCore` for the same recycled registry slot.** `tls_heap`'s `LOCAL`
  (a `Cell`, no `Drop`) and `GUARD` (`AbandonGuard`, has `Drop`) are declared
  in an order where `GUARD` drops FIRST on thread teardown — recycling the
  registry slot — while `LOCAL` survives holding its now-stale pre-recycle
  pointer. Every resolver treated any non-null `LOCAL` as "my own live slot";
  the documented generation-guard was never actually read on the alloc path.
  Reachable from correct code: an application `thread_local` with a `Drop`
  impl that allocates, first touched before the thread's first `sefer-alloc`
  allocation, is destroyed after `GUARD` — its `Drop` could resolve to the
  stale, already-recycled slot, handing out a second live `&mut HeapCore`
  concurrently with whoever re-claimed it (a data race / UAF). Fixed with a
  `TORN` sentinel (`usize::MAX`, never dereferenced): `AbandonGuard::drop`
  stamps `LOCAL = TORN` before recycling; all three TLS resolvers check
  `TORN` before treating a non-null `LOCAL` as live, and route post-teardown
  deallocs through the always-live fallback heap instead.
- **#130 — BLOCKER: `alloc_large` with `align >= SEGMENT` leaked to abort or
  returned a misaligned pointer (UB).** `alloc_large` places a large block at
  `base + align_up(header, align.max(PAGE))`, but `base` is only
  `SEGMENT`-aligned (4 MiB). For `align == SEGMENT`, the block itself landed
  `SEGMENT`-aligned at `base + SEGMENT` — an address `dealloc`'s
  `base & !(SEGMENT-1)` computation never resolves back to the registered
  `base`, so every such `dealloc` silently no-op'd, leaking the segment and
  its `SegmentTable` slot until `MAX_SEGMENTS` (1024) exhausted and the
  process aborted. For `align > SEGMENT`, the returned pointer inherited only
  4 MiB alignment roughly half the time — violating the `GlobalAlloc`
  contract (UB in the caller). Both reachable from a valid `Layout` (e.g.
  `#[repr(align(4194304))]`, huge-page buffers). Fixed by rejecting
  `align >= SEGMENT` up front with a null return (a legal, documented alloc
  failure) — exotic alignments at or above the segment size are unsupported
  by the dedicated-segment large path.
- **#131 — `ensure_slow`'s OOM path panicked without rolling back the
  bootstrap sentinel, livelocking every future registry access.** The CAS
  winner publishes `SENTINEL_INITIALIZING` before reserving VM for the
  `Registry`; on OOM the old code called `.expect(..)`, which panicked
  without ever restoring a real pointer or rolling the sentinel back to
  null. Every loser thread spinning on the sentinel spun forever, and every
  future `ensure()` call also spun forever (CAS(null, SENTINEL) never
  succeeds against a non-null stuck sentinel) — a process-wide livelock on
  the next registry touch. Worse, unwinding the panic itself allocates,
  reentering `ensure()` against the same stuck state before the panic even
  finished. Fixed: on reservation failure, roll `REGISTRY_PTR` back to null
  (Release) before terminating via `std::process::abort` (not `panic!` —
  `abort` performs no unwind and no allocation, so it cannot reenter
  `ensure()`).
- **#134 — `large_cache`'s `usable_size` was recomputed from mutable header
  fields, corrupting the RSS byte-budget.** At deposit time (both the
  own-thread `dealloc` Large branch and `reclaim_large_segment`),
  `usable_size` was recomputed from the header's `large_size`/`large_align`.
  On a large-cache HIT, a larger cached span can be reused for a smaller
  request, and the hit path rewrites the header's logical size/align to the
  smaller request — so on the segment's NEXT free, the recomputed
  `usable_size` under-reports the segment's true physical span. This let
  `large_cache_used_bytes` under-count real RSS, admitting more spans than
  the configured budget should allow (unbounded RSS amplification), and
  corrupted the cache-hit size-ratio matching. Fixed by adding a new
  `SegmentHeader::span_usable` field — the segment's PHYSICAL committed span,
  set once at the original OS reservation and carried forward verbatim
  (never recomputed) through every subsequent cache-hit reuse. Both deposit
  sites now read `header.span_usable` instead of recomputing from
  `large_size`/`large_align`.
- **#139 — miri could not validate the `registry` module: the ~22 MB
  `Registry` reservation was uninitialised under miri's `std::alloc`
  fallback.** `bootstrap::ensure_slow` relies on OS zero-pages
  (`VirtualAlloc`/`mmap`) for every `Registry` field it does not explicitly
  write. Under miri, `aligned-vmem`'s reservation falls back to
  `std::alloc`, which does NOT zero memory — so reads of `count`,
  `abandoned_segs`, and friends hit uninitialised memory (UB), aborting miri
  before it could validate anything in the registry module (including the
  #133 per-heap-counter aggregation and the #131 sentinel rollback). Fixed
  with a `#[cfg(miri)]`-only `write_bytes(base, 0, REGISTRY_SIZE)` right
  after the reservation — compiled out entirely on real targets (zero
  production cost). Full strict-provenance cleanliness of the tagged-pointer
  infrastructure is separately tracked as #140.

- **#142 — cross-thread `thread_free` access violated the aliasing model
  (Stacked AND Tree Borrows).** Expanding miri to the A1 cross-thread path
  showed the deferred-free push's `head.load` was UB under both experimental
  borrow models: the `owner_thread_free` stamp inherited the owner's
  `&mut self`-rooted reference provenance, so one remote thread's
  `compare_exchange` through it was a "foreign write" that Disabled the
  shared parent tag and forbade a second remote's read. Fixed with the same
  exposed-provenance discipline as #140: the stamp sites `expose_provenance()`
  the atomic's address (taken via `addr_of!`, no intermediate `&` retag) and
  `Node::atomic_ptr_ref` reconstructs the remote's `&AtomicPtr` via
  `with_exposed_provenance_mut` — a wildcard pointer outside the owner's
  borrow tree. Verified under miri with BOTH models on both faces' A1 tests
  and `heap_cross_thread` (all were UB before this fix).
- **#143 — `push_large_deferred_free` silently dropped a push (permanent
  leak) under concurrent head contention.** Found by the new
  `loom_deferred_large` model (#141) and confirmed by a 2M-trial
  `std::thread` reproduction: the double-push claim-CAS lived INSIDE the
  head-CAS retry loop, so after losing the head CAS to a concurrent pusher
  of a DIFFERENT base, the retry's claim always failed (the link word had
  already left `ABANDONED_TAIL`) and the function returned through the guard
  bail-out without ever winning `head` — the segment never entered the
  deferred-free stack (an A1-class permanent leak). Fixed by hoisting the
  claim CAS to run exactly once, before the head-CAS retry loop.
- **Full-review follow-up — the #138 layout-consistency mitigation
  over-rejected legitimate tiny-size frees.** The alloc path clamps every
  request to `MIN_BLOCK` (16) before it reaches the header's `large_size`,
  but the mitigation compared the freeing caller's RAW `layout.size()` — so
  a legitimate cross-thread free of a `size < 16`, `align > SMALL_MAX` block
  (a valid `Layout` via the raw alloc API) always mismatched, was dropped,
  and permanently leaked the segment + its table slot (the #114/#130
  leak-to-abort class, narrow trigger). `large_layout_consistent` now clamps
  the caller's size symmetrically before comparing.

#### Performance

- **#133 — per-heap hit counters replace a contended global-lock `fetch_add`
  on the hot path.** `DBG_TCACHE_HITS` (magazine-hit) and
  `LARGE_CACHE_HITS` (large-cache-hit) were process-global `AtomicU64`s
  bumped by every thread on otherwise fully-per-thread hot paths — a
  contended cache line that ping-ponged across cores. Moved to per-heap
  fields (`HeapCore::tcache_hits`, `AllocCore::large_cache_hits`),
  incremented `Relaxed` by the owning thread only; the process-wide view
  (`stats()`, tests) is reconstructed by summing every minted heap slot's
  counter, gated by a new `HeapSlot::initialised: AtomicBool` (Release-set
  after the heap is fully constructed; the aggregator Acquire-loads it to
  avoid reading a not-yet-initialised slot). Measured: churn −20.9 % (16 B),
  −19.6 % (64 B).
- **#135 — `SegmentTable::register`/`unregister`/`recycle` and
  `HeapCore::realloc`'s ownership test are now O(1), not O(segment count).**
  `register` used to scan `[0, count)` for a NULL slot; `unregister`/
  `recycle` scanned for a matching base. All three are now O(1) via a
  free-list stack of recycled slot indices (carved in the primordial
  segment) plus a field-specific `segment_id_at` header read that indexes
  the slot directly. `HeapCore::realloc`'s ownership check switched from
  `segment_bases().any(...)` (O(count)) to `AllocCore::contains_base` (O(1)
  hash probe, same semantics). Also hardens `dealloc_routing`'s M2 routing:
  `self.core.contains_base(base)` is now checked FIRST (O(1), reads only the
  caller's own table, no cross-thread memory read) — proven equivalent to
  the prior `owner_tf.is_null() || owner_tf == our_head` branch for every
  segment the caller owns; only a miss falls through to the field-specific
  cross-thread header reads.

#### Changed

- **#136 — public API polish before the first 0.3.0 publish (pre-release, not
  a breaking change for any published version).**
  - `SegmentLayout::SIZE_CLASS_TABLE` / `SIZE2CLASS` are now `&'static [..]`
    slices instead of fixed-size arrays (`[usize; 48]` / `[u8; N]`). The
    class-count grew silently 40→48 in 0.3.0; a fixed-length public type would
    have made every future class re-tune a breaking change. A slice view has
    no length in its type.
  - `LargeCacheConfig::budget_bytes(0)` now means "cache disabled" (every
    deposit released to the OS), stored verbatim as `Some(0)`. Previously `0`
    was silently remapped to `None` ("unbounded") — the opposite of what `0`
    intuitively suggests. Unbounded is still the default (don't call
    `budget_bytes`).
  - `LargeCacheMode` is now `#[non_exhaustive]` (adding a variant in a future
    release is no longer breaking).
  - Internal-but-`pub` items reachable only through `#[doc(hidden)]` modules
    (e.g. `AllocCore::segment_bases`, `HeapCore::segment_bases`) are now
    `#[doc(hidden)]`, and stale `SMALL_ALIGN_MAX`/`SMALL_MAX` docs were
    corrected to match the #114/B1 divisibility-aware small path (align > 16
    is served by the small path up to `SMALL_MAX`, not routed to Large).
  - rustdoc builds clean (0 warnings) under both the default and `production`
    feature sets; docs.rs is configured to render with `production`.

- **#132 — the explicit `Heap`/`with_heap` public face lacked the A1
  cross-thread Large-segment reclaim fix.** `SeferAlloc` (via `HeapCore`) got
  the A1 fix in 0.3.0; `Heap::dealloc_any_thread` did not — a cross-thread
  free of a Large/huge segment through the explicit `Heap` API still no-op'd
  and leaked the segment permanently until the owning `Heap` dropped. Both
  faces now share the same extracted deferred-free primitive
  (`alloc_core::deferred_large`), including the double-push guard hardening,
  so a remote free of a Large segment is reclaimed on the owner's next large
  allocation regardless of which public face is used.
- **#132 — `with_heap` panicked on a reentrant borrow or TLS teardown.**
  `with_heap`'s documented `# Panics` behaviour (`RefCell::borrow_mut`
  panicking on a reentrant call, or on TLS-destructor-already-ran) was a
  footgun for a public allocator API — e.g. a `Drop` impl that allocates via
  `with_heap` during thread teardown would abort instead of degrading
  gracefully. `with_heap` now uses the same no-panic
  `try_with`/`try_borrow_mut` mechanics as the crate-internal
  `with_heap_try` and returns `None` (its signature has always been
  `Option<R>`) instead of panicking.
- **#138 — A1 post-reuse defensive mitigation for cross-thread Large-segment
  double-free.** A1's deferred-free stack fully closes the PRE-reuse
  double-free window (a double-free of a Large segment not yet reclaimed is
  a sound no-op, guarded by `push_large_deferred_free`'s double-push CAS
  guard). The POST-reuse window remained: a stale free arriving after the
  segment was already reclaimed and handed to a brand-new allocation is, by
  address alone, indistinguishable from a legitimate free of that new
  occupant. Both cross-thread Large-free routing paths
  (`HeapCore::dealloc_routing`, `Heap::dealloc_any_thread`) now check that
  the freeing `Layout`'s size matches the CURRENT occupant's `large_size`
  header field (`alloc_core::deferred_large::large_layout_consistent`)
  before queuing the segment for reclaim; a mismatch is dropped as a no-op
  instead of corrupting the reused segment. **Honest scope: this is a
  mitigation, not a full fix** — a reuse that happens to request the
  bit-identical size is not caught (double-free remains UB by the
  `GlobalAlloc` contract). New regression tests:
  `tests/regression_xthread_large_free_layout_mismatch.rs`
  (`xthread_large_free_mismatched_layout_is_dropped`,
  `xthread_large_free_consistent_layout_is_reclaimed`, plus a `Heap`-face
  counterpart), counterfactual-verified against both call sites.

#### Internal

- **#137 — CI never exercised the `fastbin` (magazine/tcache) path or the
  flagship `production` feature bundle**, and `loom_fallback_init` (the
  fallback-heap lazy-init state machine) existed but was absent from the
  loom CI matrix (model-checked locally, never gated in CI). Added
  `--features "alloc-global alloc-xthread fastbin"` and
  `--features production` to the test matrix, `--no-fail-fast` to the test
  runner (a failure in one test binary no longer masks failures in later
  ones), and `loom_fallback_init` to the loom matrix.
- **#138 — loom-model honesty audit.** Every `tests/loom_*.rs` file's doc
  comment now states whether it models a currently-live production code
  path, a removed/superseded one, or a dead (currently-unreachable) one:
  `loom_thread_free.rs` models the Phase 10 intrusive-TFS push/drain of
  individual freed blocks, which was superseded by the non-intrusive
  per-segment `RemoteFreeRing` (modelled separately, faithfully, in
  `loom_remote_ring.rs`) — retained for its generic CAS-push counterfactual,
  not as a validator of any current path. `loom_registry.rs` models the
  Phase 12.4 segment-adoption CAS protocol, whose only producer
  (`HeapRegistry::abandon_segments`) is unreachable from any production path
  today (Phase 12.5 replaced thread-exit abandonment with whole-heap slot
  reuse) — retained as a pre-validated substrate for a future
  decommit-when-empty policy. `tagged_ptr.rs`'s doc comment referenced a
  push-pop-repush ABA loom model in `loom_registry.rs` that was never
  actually written (that file models a different protocol entirely); the
  reference is corrected and the missing ABA model for the `free_slots`
  `TaggedPtr` stack is tracked as follow-up debt, not written in this pass.
  A loom model for the A1 `deferred_large` push/drain (Large-segment
  reclaim) is also tracked as follow-up debt — judged out of scope for this
  hardening pass (see the task report for the full audit table).

- **#140 — explicit provenance APIs for the registry's lock-free stacks.**
  The `REGISTRY_PTR` sentinel is now constructed with
  `core::ptr::without_provenance_mut` (strict-provenance-clean; it is only
  ever compared, never dereferenced), and every cross-allocation packed-word
  store/load pair in `abandoned_segs` and the A1 deferred-large stack calls
  `expose_provenance` / `with_exposed_provenance_mut` explicitly, with a
  documented "Provenance model" section explaining why full
  `-Zmiri-strict-provenance` is structurally unreachable for
  cross-allocation intrusive stacks (an exposed-provenance shape by design,
  not a bug). No lock-free semantics changed.
- **#141 — the two missing loom models were written**, closing the debt the
  #138 audit recorded above: `loom_deferred_large.rs` (the A1 push/drain
  Treiber stack including the double-push guard — the model that found
  #143) and `loom_free_slots_aba.rs` (the `free_slots`/`TaggedPtr`
  push-pop-repush ABA scenario). Both ship `should_panic` counterfactuals
  proving non-vacuity and are wired into the CI loom matrix.

### Initial pass — phases A–F (2026-06-30)

Post-0.2.1 hardening pass — six phases (A–F), each independently reviewed,
counterfactual-verified, and committed.

#### Fixed

- **A1 — permanent leak: cross-thread free of a Large/huge segment.** A
  remote free of a Large segment no-op'd instead of reclaiming it — the
  segment (≥4 MiB) and its `SegmentTable` slot leaked forever under any
  allocate-here/free-there workload (the canonical case: an async runtime
  migrating a task holding a large buffer to a different worker thread). Now
  reclaimed via a per-heap deferred-free stack, drained lazily on the
  owner's next large allocation.
- **A2 — `fastbin` buildable without `alloc-xthread` (unsound).** A
  cross-thread free with `fastbin` alone had no ownership-checked routing
  path — a data race into another thread's private magazine. `fastbin` now
  requires `alloc-xthread` (Cargo feature unification + a `compile_error!`
  guard).
- **B1 — page-aligned allocations (512 B – 16 KiB, `align` a multiple of
  512/1024/2048/4096) still burned a dedicated Large segment**, the last gap
  in #114's fix. Eight page-aligned size classes added to the table.
- **Latent `realloc` cross-class-shrink bug**, exposed by B1: `AllocCore::realloc`'s
  in-place fast path aliased a shrink across size classes, corrupting the
  smaller class's free list on a later layout-derived free. Restricted to
  same-class in-place; a cross-class shrink now relocates.
- **F1 — fallback-heap init livelock.** If the CAS winner initialising the
  process-global fallback heap hit primordial OOM, every other thread
  spun forever waiting for a `READY` that would never come. Losers now
  observe the rollback and re-race the CAS.

#### Changed — performance

- **C1 — the per-thread magazine (`fastbin`) now serves `align > 16`
  requests** (tokio task cells, page-aligned buffers), not just the
  historical `align <= 16` case — the main remaining hot-path gap for the
  workload #114/B1 targeted.
- **C2 — `realloc`'s in-place fast path is now reachable through the
  `#[global_allocator]` face**, not just the lower-level `AllocCore` API; a
  same-class resize through `SeferAlloc` no longer pays a redundant
  alloc+copy+dealloc.
- **D1 — `LARGE_CACHE_SLOTS` raised 2 → 8**, with a correctness fix: eviction
  now uses a true insertion-order FIFO (a monotonic sequence number) instead
  of an index-order assumption that only held at 2 slots. A workload cycling
  more than two distinct large sizes now gets real cache reuse instead of
  thrashing to an OS round-trip on every allocation.
- **D3 — magazine refill is now a per-class byte budget** (≈64 KiB) instead
  of a fixed 16-block count for every class; a large size class no longer
  parks several MiB in one idle thread's cache after a single refill.

#### Added

- **`SeferAlloc::stats() -> AllocStats`** — a cheap, lock-free, process-wide
  diagnostic snapshot (cache hits, decommit calls, cross-thread reclaims,
  ring overflows, segments reserved/released, heaps claimed). Previously
  every one of these counters was crate-internal and invisible in
  production; `segments_reserved_total - segments_released_total` is the
  single most useful field for spotting a segment leak before it escalates
  to an OOM abort. `#[non_exhaustive]`, stable field set across every
  feature combination.
- **D2 — process-wide `RemoteFreeRing` overflow counter**, feeding
  `AllocStats::ring_overflows`.
- Rustdoc: a "Multi-thread safety" section on `SeferAlloc` spelling out the
  `alloc-global`-without-`alloc-xthread` footgun (cross-thread frees leak
  monotonically), and a "std-only" note.

#### Internal

- CI: `-D warnings` restored on the clippy gate after a warnings-cleanup
  pass; miri matrix extended to the task-#114 align-regression tests; a
  process-global-state test flake in `heap_core_bulk_bypass` fixed at its
  real root cause (whole-heap slot reuse carrying stale P7 state across
  tests in one binary).

## [0.2.1] - 2026-06-30

> ⚠️ **Superseded by `0.3.0`; to be yanked from crates.io once `0.3.0` is
> published.** `0.2.1` ships `fastbin = ["alloc-global"]`, which is buildable
> *without* `alloc-xthread` — a cross-thread free with `fastbin` alone has no
> ownership-checked routing path and races into another thread's private
> magazine (data race / UB). Fixed in `0.3.0` (phase A2: `fastbin` now
> requires `alloc-xthread`, enforced by Cargo feature unification + a
> `compile_error!` guard). Upgrade to `0.3.0`.

### Fixed — `align > 16` allocations no longer burn a dedicated segment each

`SizeClasses::class_for(size, align)` unconditionally returned `None` for
any `align > SMALL_ALIGN_MAX` (= `MIN_BLOCK` = 16). Every allocation with
a larger alignment — including the `tokio::runtime::task::core::Cell<T,S>`
shape (≈640 B, `#[repr(align(128))]` against false sharing) — was routed
to the dedicated-segment Large path, consuming a full ~4 MiB segment and
one `SegmentTable` slot per request.

Under concurrent task-spawning workloads (canonical reproducer: the
shamir-db `duplex_throughput/duplex_cap32/32` bench — 32 in-flight
tokio tasks × 55 iterations), cumulative live segments exceeded
`MAX_SEGMENTS = 1024`, then `alloc_large_slow → SegmentTable::register`
returned `None`, then the `GlobalAlloc` face returned null, then
`std::alloc::handle_alloc_error` aborted the process with
`memory allocation of 640 bytes failed`.

`class_for` now searches for the smallest small class whose
`block_size >= max(size, align)` AND `block_size % align == 0`. M4
(alignment fidelity) is preserved: the segment base is `SEGMENT`-aligned,
the offset within is a multiple of `block_size`, and `block_size` is a
multiple of `align`, so the returned pointer is naturally `align`-aligned
without any per-block padding. The fast path for `align ≤ MIN_BLOCK = 16`
(the typical case) is byte-identical to the previous behaviour — one
`SIZE2CLASS` load. The slow path is a forward walk over at most
`SMALL_CLASS_COUNT = 40` entries; in practice it settles in 0–3 steps
for power-of-two alignments common in async runtimes (32 / 64 / 128 / 256).

For `(640, align=128)` the resolver picks the existing class with
`block_size = 768` (768 % 128 == 0). Per-allocation memory cost drops
from ~4 MiB to ~768 B, and the per-process `SegmentTable` is no longer
touched on the hot path.

Regression test: `tests/regression_large_align_no_segment_exhaustion.rs`
(2048 sequential `(640, 128)` allocations + 1500 sequential allocations
each for 4 representative `(size, align)` shapes). Counterfactual
verified — reverting the fix makes the test fail on iteration 1023
(= `MAX_SEGMENTS − 1`, primordial segment holds the first slot).

Single-threaded substrate change; no concurrency-protocol or wire-format
implications. Full test suite under `features = ["production"]` —
including loom (`loom_bootstrap_cas`, `loom_xthread_protocol`,
`loom_thread_free`) — green.

## [0.2.0] - 2026-06-29

> ⚠️ **Yanked from crates.io.** Superseded by `0.2.1`, which fixes the #114
> `align > 16` segment-exhaustion bug: an `align > 16` allocation (e.g. the
> `tokio` task-cell shape, `#[repr(align(128))]`) burned a full ~4 MiB
> segment each and could exhaust `MAX_SEGMENTS = 1024` and abort the process
> under ordinary async workloads. Upgrade to `0.2.1` or later.

### Changed — BREAKING: `SeferMalloc` renamed to `SeferAlloc`

The headline `#[global_allocator]` type is renamed from `SeferMalloc` to
`SeferAlloc`. The "malloc" suffix was a libc convention inherited from
C-wrapper allocators (`mimalloc`, `jemalloc`, `tcmalloc`) and clashed
with sefer-alloc's positioning as a pure-Rust allocator with no C deps.
The new name aligns the type with the crate name and the Rust ecosystem's
modern `*-alloc` convention.

**Migration:** rename every occurrence of `SeferMalloc` in your code to
`SeferAlloc`. The constructors (`new()`, `with_config(...)`) and the
public API surface are otherwise unchanged.

```rust
// Before (0.1.x):
use sefer_alloc::SeferMalloc;
#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

// After (0.2.0):
use sefer_alloc::SeferAlloc;
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
```

`LargeCacheConfig`, `LargeCacheMode`, `Region`, `Handle`, `SyncRegion`,
`AllocCore`, and every other public type are unchanged.

Internal: `src/global/sefer_malloc.rs` → `src/global/sefer_alloc.rs`
(module file rename). User-facing docs (`README.md`, `docs/INTEGRATION.md`,
`docs/ARCHITECTURE.md`) updated to use "alloc face" terminology consistently;
historical / planning docs (`ALLOC_PLAN.md`, `FINDINGS_PHASE12.md`, etc.)
keep their original "malloc face" language as historical record.

`0.1.0` is yanked from crates.io to direct fresh installs to `0.2.0`;
existing `Cargo.lock` references continue to work.

### Changed — const-builder config API replaces env vars (alloc-decommit)

- **`LargeCacheConfig` const builder** — new type (re-exported from
  `sefer_alloc::` under `alloc-core + alloc-decommit`). All five knobs
  that were previously set via environment variables are now expressed at
  compile time via a `const fn` builder chain:

  ```rust
  use sefer_alloc::{SeferMalloc, LargeCacheConfig, LargeCacheMode};

  const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
      .budget_bytes(512 * 1024 * 1024)
      .headroom_bytes(64 * 1024 * 1024)
      .decay_interval_ms(200)
      .decay_rate_percent(25)
      .mode(LargeCacheMode::Lazy);

  #[global_allocator]
  static GLOBAL: SeferMalloc = SeferMalloc::with_config(CONFIG);
  ```

- **`SeferMalloc::with_config(config: LargeCacheConfig) -> Self`** (`const fn`,
  only under `alloc-decommit`) — constructs the allocator with a custom
  large-cache config. The config is plumbed into each per-thread `AllocCore`
  on first TLS bind.

- **`SeferMalloc::new()`** unchanged — equivalent to
  `SeferMalloc::with_config(LargeCacheConfig::DEFAULT)`.

- **`AllocCore::new_with_config(config: LargeCacheConfig) -> Option<Self>`**
  (`alloc-decommit` only) — new constructor for direct `AllocCore` users.

- **Env vars removed entirely** — `SEFER_LARGE_CACHE_BUDGET`,
  `SEFER_LARGE_CACHE_HEADROOM_BYTES`, `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS`,
  `SEFER_LARGE_CACHE_DECAY_RATE`, `SEFER_LARGE_CACHE_MODE` are no longer read.
  The allocation-free env-var parser in `src/alloc_core/os.rs` is deleted.
  Default values are byte-identical to what the parsers produced when no variable
  was set (headroom=256 MiB, interval=1000 ms, rate=10 %, budget=unbounded,
  mode=Lazy).

- **Tests updated** — `tests/large_cache_budget.rs`, `tests/large_cache_decay.rs`,
  and `tests/large_cache_mode.rs` no longer use `std::env::set_var`. The
  env-var test cases are replaced with equivalent `AllocCore::new_with_config`
  tests that are deterministic and safe to run in parallel.

## [0.1.0] - 2026-06-28

### Changed — workspace extraction (tasks #74–#86)

Four independently-publishable companion crates extracted from sefer-alloc
into `crates/`. Each is a real crates.io package someone can `cargo add`
on its own:

- **`sefer-region`** (`crates/region/`) — typed handle store
  (`Handle<T>` / `Region<T>` / `SyncRegion<T>`). `#![forbid(unsafe_code)]`.
  ([docs.rs/sefer-region](https://docs.rs/sefer-region) — link live after publish.)

- **`aligned-vmem`** (`crates/vmem/`) — OS virtual-memory aperture:
  SEGMENT-aligned `mmap`/`VirtualAlloc` + page decommit/recommit.
  `#![allow(unsafe_code)]` — sole purpose IS the OS unsafe, single
  responsibility, small codebase, independently auditable.
  ([docs.rs/aligned-vmem](https://docs.rs/aligned-vmem) — link live after publish.)

- **`numa-shim`** (`crates/numa/`) — dependency-free NUMA detection and
  binding. Linux `mbind(2)` via `syscall(2)` (no `libnuma`), Windows
  `VirtualAllocExNuma`. `#![allow(unsafe_code)]` — sole purpose IS the NUMA
  syscall unsafe, single responsibility, independently auditable.
  ([docs.rs/numa-shim](https://docs.rs/numa-shim) — link live after publish.)

- **`malloc-bench-rs`** (`crates/malloc-bench/`) — portable `GlobalAlloc`
  benchmark harness (larson + mstress). Callable against any allocator without
  installing it as `#[global_allocator]`. Not in sefer-alloc's runtime dep
  tree.
  ([docs.rs/malloc-bench-rs](https://docs.rs/malloc-bench-rs) — link live after publish.)

**sefer-alloc itself** re-exports `sefer-region`'s surface for backward
compatibility — existing `use sefer_alloc::{Region, Handle, SyncRegion}` code
compiles unchanged. `alloc_core::os` and `alloc_core::numa` are now thin
interop wrappers that delegate to `aligned-vmem` and `numa-shim` respectively.

**Audit story improved:** an auditor no longer has to navigate the full
allocator codebase to verify the OS-memory unsafe. `aligned-vmem` (~few hundred
lines, single purpose) and `numa-shim` (~few hundred lines, single purpose) can
each be audited in complete isolation with `cargo test` confirming green.

### Added — large-cache redesign Phase 3 (alloc-decommit, mode-selector + future stub)

- **`LargeCacheMode { Lazy, Background, Both }`** enum, re-exported from
  `sefer_alloc::` under `alloc-core + alloc-decommit`. The mode is selected
  via the new `SEFER_LARGE_CACHE_MODE` env var (case-insensitive: `lazy` /
  `background` / `both`; unrecognised values fall back to `Lazy`).

- **Default = `Lazy`** — Phase 2 behaviour is preserved bit-for-bit. Setting
  `SEFER_LARGE_CACHE_MODE=background` currently prints a one-time process
  warning ("background mode requested but not yet implemented — falling back
  to lazy") and continues with lazy decay. The full background-thread
  implementation has identified risks documented inline (Mutex refactor +
  HeapRegistry iteration API + safe spawn timing + TSan validation) and is
  intentionally deferred to a follow-up; the mode-selector plumbing lets a
  future commit turn it on without any user-facing API change.

- **`tests/large_cache_mode.rs`** — 3 new tests covering default-Lazy,
  per-shard mode storage, and env-var parsing.

### Changed — large-cache redesign Phase 2 (alloc-decommit)

- **Lazy exponential decay**: large-cache excess over the headroom target
  decays toward the OS at 10 %/tick by default. On every large `alloc` or
  `free`, a single `Instant::now()` comparison checks whether
  `decay_interval` has elapsed; if so, `excess = used - headroom` and
  `release = excess × rate` bytes are FIFO-evicted to the OS. No background
  thread — the decay is fully inline, paying nothing while the process is idle
  (mobile/embedded friendly). Phase 3 will add an optional background thread.

- **Three new env vars** (all read once at `AllocCore::new`, allocation-free):
  - `SEFER_LARGE_CACHE_DECAY_RATE` — integer percent (`"10"`, `"10%"`;
    default 10). Parsed without floats to avoid any floating-point dependency.
  - `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS` — integer ms (default 1000).
  - `SEFER_LARGE_CACHE_HEADROOM_BYTES` — bytes with K/M/G suffix (default
    256 MiB). The cache is allowed to hold up to this many bytes; only the
    excess above it is subject to decay.

- **Generalized `os::read_env_var_raw(name_nul, buf)`**: the allocation-free
  env-var reader is now parameterized on the variable name (NUL-terminated
  `&[u8]`). `read_env_budget_raw` is kept as a thin backward-compatible
  wrapper. This lets all three decay env parsers share the same reentrancy-safe
  pattern without duplicating the Windows/Unix platform dispatch.

- **Test seams** (`dbg_set_decay_config`, `dbg_force_decay_tick`,
  `dbg_decay_config`): deterministic test control without sleep or real
  wall-clock advances. `dbg_force_decay_tick` rewinds `last_decay_tick` by
  `decay_interval` and immediately invokes one decay step.

- **`tests/large_cache_decay.rs`**: 5 new tests covering excess release,
  headroom invariant, no-op when under target, interval guard, and env-var
  parsing.

### Changed — large-cache redesign Phase 1 (alloc-decommit)

- **Removed `MAX_CACHED_LARGE_BYTES`** (was 64 MiB per-span cap). Spans of
  any size can now enter the large-cache, removing the arbitrary ceiling that
  prevented caching of 100 MiB+ allocations.

- **Per-shard byte-budget admission** replaces the old per-span cap. A new
  `AllocCore::large_cache_budget_bytes: Option<usize>` field (under
  `alloc-decommit`) tracks the total bytes of all cached spans. When the
  budget would be exceeded, the oldest cached slot (FIFO: lowest index) is
  evicted to the OS before the new span is admitted. `None` = unbounded
  (default when the env var is not set).

- **`SEFER_LARGE_CACHE_BUDGET` environment variable** is read once at
  `AllocCore::new()` via a raw OS call (no heap allocation — safe even when
  `SeferMalloc` is the `#[global_allocator]`). Accepted formats: `"64M"`,
  `"2G"`, `"1024"` (raw bytes), etc. Parsed case-insensitively.

- **`large_cache_used_bytes` invariant counter**: maintained on every deposit
  and every eviction / cache hit. Verified by new tests via
  `dbg_large_cache_used()` / `dbg_large_cache_slot_sizes()` test seams.

### Removed

- **`byte` / `byte-sharded` features** — research-tier `ByteRegion` /
  `ByteAllocator` / `ShardedByteArena` removed. They were never expected to
  compete with mimalloc (see the BYTE_BENCH / BYTE_SHARDED_BENCH writeups in
  git history) and are fully superseded by the production stack (`alloc-global`
  + `alloc-xthread` + `alloc-decommit`). Old Phase 4 / Phase 7d log entries
  below are intentionally left intact as historical record.

### Deprecated

- **`experimental` concurrent regions** (`EpochRegion`, `LockFreeRegion`,
  `ShardedRegion`) — marked `#[deprecated]`. Superseded by the production
  `alloc-xthread` cross-thread free path. `PinnedRunner` is NOT deprecated.

### Summary

The initial public release.

**Pure Rust, no C / C++ libraries.** Unlike `mimalloc` (C++), `jemalloc`
(C), `snmalloc` (C++), `tcmalloc` (C++), or the typical `libnuma`-wrapping
NUMA crates, `sefer-alloc` is 100 % Rust — it calls into the OS directly
(`mmap` / `VirtualAlloc` / `mbind` etc.), but does not link a single C or
C++ library. The only C dependency in the repository is the optional
`mimalloc` dev-dependency used as a baseline in benchmarks (never on a
consumer's runtime path).

Two faces on one verified substrate:

- **`Region<T>` / `Handle<T>`** — a safe-by-construction handle store
  (default `std`, also `no_std` + `alloc`). `#![forbid(unsafe_code)]`
  at the top — the only `unsafe` is `slotmap`'s audited core wrapped
  by a typed membrane.

- **`SeferMalloc`** — a drop-in `#[global_allocator]` (opt-in
  `production` feature = `alloc-global + alloc-xthread +
  alloc-decommit`). Up to **~18× faster than `mimalloc` on cached
  large alloc/free** after the OPT-E large-cache (4 MiB cycle ≈ 45 ns
  vs ~718 ns ≈ **~16×**; 16 MiB ≈ 48 ns vs ~869 ns ≈ **~18×** — single
  Windows dev host, criterion `sample_size(10)`, see
  `docs/ALLOC_BENCH.md`); competitive with `mimalloc` on multi-thread
  cross-thread paths (`examples/malloc_macro.rs`). Confined-`unsafe`
  inventory under `production` (eight files): `alloc_core::{os, node}`
  + `global::{sefer_malloc, tls_heap, fallback}` +
  `registry::{heap_slot, heap_registry}`. `numa-aware` adds one more
  (`alloc_core::numa`). The crate is `#![deny(unsafe_code)]` (or
  `#![forbid]` in the default `std`-only build) and every `unsafe`
  block carries a `// SAFETY:` proof; compile-enforced.

Verification stack: 51 integration test files, 6 loom models
(`tests/loom_*.rs`), proptest differential vs reference model, miri
with strict-provenance (CI gate), ThreadSanitizer (×3 verified
clean on cross-thread + decommit), Valgrind memcheck clean,
aarch64 13/13 under qemu-user, libFuzzer (`region_ops`,
`global_alloc_ops`), soak / RSS / tokio-burn-in harnesses,
criterion benches with flamegraph profiling. Full details in
`docs/ARCHITECTURE.md` and `docs/ALLOC_BENCH.md`.

### Added

- **OPT-B (#67) — O(1) `SegmentTable::contains_base`**: a self-hosted
  open-addressing hash (2048 slots, 16 KiB in the primordial segment)
  replaces the O(count) linear scan. Tombstone encoding for removed
  entries keeps probe chains intact under recycle/decommit churn.
  Matters at DBMS scale (50–100+ live segments).
- **OPT-C (#66) — lazy `stamp_segment_owner`**: `HeapCore` caches the
  last-stamped segment base; cache-hit fast path is a single Relaxed
  load + ownership compare (no Release-store), skipping the costly
  MFENCE on 99 % of hot-segment allocations.
- **OPT-E (#65) — large-segment free-cache** (the headline win):
  1-2 fixed slots per `AllocCore` hold freed OS reservations; the
  next similarly-sized `alloc_large` reuses without mmap.
  **Measured: 4 MiB from 254 µs to 42 ns (~6,000× speedup, 18× faster
  than mimalloc 788 ns); 16 MiB from 701 µs to 48 ns.** Pages stay
  committed inside the cache (eliminates Windows
  `VirtualAlloc(MEM_COMMIT)` cost on hit). Bounded RSS at
  `LARGE_CACHE_SLOTS × MAX_CACHED_LARGE_BYTES = 2 × 64 MiB =
  128 MiB`. Gated on `alloc-decommit` for `SegmentTable` `unregister`
  consistency.
- **OPT-F (#64) — in-place small→small realloc**:
  `AllocCore::realloc` short-circuits when `new_size` resolves to the
  same or smaller size class as `old_size` — returns the same pointer,
  no copy, no alloc, no dealloc. Bench `realloc_in_place_unfavorable`
  improved 28.6 %.
- **OPT-G (#63) — `production` feature alias** + README guidance:
  `production = ["alloc-global", "alloc-xthread", "alloc-decommit"]`
  is the recommended set for long-running multi-thread workloads
  (DBMS, async runtimes); without `alloc-decommit` the
  `SegmentTable` slot-recycle path is disabled and the 1024-slot
  table is a hard ceiling.
- **NUMA-aware path** (Phases A–E of #58): opt-in `numa-aware`
  feature, default OFF. New confined-`unsafe` module
  `src/alloc_core/numa.rs` (Linux `mbind(2)` via `syscall(2)` —
  avoids `libnuma` dep — `MPOL_PREFERRED`; Windows
  `VirtualAllocExNuma`; macOS / miri no-op). Layout-stable
  `SegmentHeader::node_id` (present in every build).
  `reserve_small_segment` / `alloc_large` stamp the current thread's
  NUMA node; `find_segment_with_free` prefers local-node segments
  with foreign-node fallback. Tests: `numa_seam` (5),
  `numa_segment_id` (2), env-guarded `numa_alloc` (3, run with
  `SEFER_NUMA_TEST=1` under multi-NUMA topology). Honest caveat:
  QEMU verifies correctness, not latency-asymmetry; real measurement
  requires 2-socket hardware. See `docs/PHASE_NUMA_DESIGN.md`.
- **SegmentTable slot-recycle** (#60): under `alloc-decommit`, an
  empty decommitted segment NULLs its table slot for future
  re-registration, lifting the hard `MAX_SEGMENTS = 1024` cumulative
  ceiling. Found by the #52 tokio burn-in hitting OOM at >512
  concurrent tasks. New `recycle` (atomic NULL + `release_segment`)
  and partner `unregister` (NULL without release; used by OPT-E
  cache deposit).
- **strict-provenance miri fix** (#59): converted 11 sites of the
  `os::segment_base_of(ptr as usize) as *mut u8` idiom to the
  provenance-preserving `os::segment_base_of_ptr(ptr) =
  ptr.map_addr(|a| a & !(SEGMENT - 1))`. The CI miri job (which
  runs with `-Zmiri-strict-provenance`) now passes
  `decommit_miri_cycle` and `reclaim_offset_unit`.
- **Highload-hardening harnesses**:
  - `examples/soak_xthread.rs` (#51) — N-thread × hours stability
    test (32 / 64 / 128 workers); end-of-run invariant
    `total_alloc == total_free`.
  - `examples/rss_probe.rs` (#53) — measures peak / final RSS under
    sustained asymmetric cross-thread free; smoke: `alloc-decommit`
    keeps peak 13 % lower (91 → 79 MB).
  - `examples/tokio_burn_in.rs` (#52) — SeferMalloc installed as
    `#[global_allocator]` under tokio multi-thread runtime with a
    DBMS-pipeline-shaped workload.
  - `benches/large_realloc.rs` (#54) — three groups (large
    alloc+free, geometric realloc grow, realloc under neighbour
    pressure) comparing SeferMalloc, mimalloc, System through their
    `GlobalAlloc` traits.
- **Low-noise criterion benches** (#62): `benches/heap_xthread.rs`
  (direct ring push/drain, no channels) and
  `benches/heap_async_pattern.rs` (synthetic async-like pattern
  without tokio) — allocator visibility rises from 1.7 % to 13 % of
  self-time vs the noisier `global_alloc` / `large_realloc` benches.
- **Comprehensive verification runs** (one-off, evidence preserved
  in `docs/`):
  - ThreadSanitizer ×3 clean on `race_repro`, `race_norecycle`,
    `global_alloc_mt`, `heap_cross_thread`; ×3 clean on
    `decommit_stale_ring`, `decommit_soak`.
  - aarch64 (qemu-user 8.2.2) 13/13 tests pass, with honest caveat
    about TCG vs real ARM weak-memory.
  - Valgrind memcheck clean on three cross-thread test binaries;
    helgrind / DRD inapplicable to lock-free atomic code (known
    Valgrind limitation — TSan is the right tool).
  - Full Linux feature-matrix (6 combos × 248 tests) all green.
- **Documentation**:
  - `docs/ARCHITECTURE.md` — compact technical overview (synthesis
    of design memos).
  - `docs/PHASE_NUMA_DESIGN.md` (#55) — full NUMA design.
  - `docs/PROFILE_FLAMEGRAPHS.md` (#61) — flamegraph profiling
    report on 4 scenarios with 6 prioritised optimisation
    candidates (OPT-B/C/E/F/G all realised in this release; OPT-H
    documented but deferred as low impact).
  - `docs/ALLOC_BENCH.md` — extensive update with OPT-E large-cache
    numbers, NUMA section, honest verdicts.
- **OSS infrastructure** (preparing for crates.io publication):
  `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`,
  `.github/ISSUE_TEMPLATE/*`, `.github/PULL_REQUEST_TEMPLATE.md`.
  `Cargo.toml` metadata refreshed for crates.io (description
  mentions both faces, `keywords` rebalanced to `["allocator",
  "arena", "generational", "handle", "lock-free"]`, `categories`
  extended with `concurrency` and `no-std`, `repository` /
  `homepage` / `documentation` URLs added).
- **Build infrastructure**: `cargo-fuzz` metadata fix to enable
  `cargo fuzz build` (#56); `region_ops.rs` idiom corrected to match
  `arbitrary` 1.4.2 (#56); `malloc_macro` registered as
  `[[example]]` with `required-features` (was missing, causing CI
  `cargo test` without `--tests` to fail with E0601).

- **Phase 35 — M6 decommit: return empty segments to the OS** (behind a new
  opt-in `alloc-decommit = ["alloc-core"]` feature; **default OFF — the default
  build is byte-for-byte unchanged**). When a small segment's live-block count
  drops to zero and it is not the current carve target, its payload pages
  `[small_meta_end, SEGMENT)` are returned to the OS (`VirtualFree MEM_DECOMMIT`
  / `madvise MADV_DONTNEED`; no-op under miri) and the segment is reset to a
  clean blank (`bump = small_meta_end`, `BinTable` heads = NULL, payload
  page-map = Free, alloc-bitmap = 0, `decommitted` flag set); the payload is
  recommitted on the first reuse. This bounds steady-state RSS under churn (the
  one honest gap in `ALLOC_BENCH`). Bookkeeping: a new **owner-only** `u32`
  `live_count` field in `SegmentHeader` (present in every build's layout so the
  byte layout is stable; mutated only under the feature) — `+1` on
  `pop_free`/`carve_block` hand-out, `−1` on `dealloc_small`/`reclaim_offset`;
  refill blocks net to zero (carve `+1`, push-to-free-list `−1`). **No
  crossbeam-epoch / M11 barrier is needed** — Variant-2 (Phase 12.6) already
  removed the only reason the original plan reached for epoch: the cross-thread
  freer never dereferences the block (it pushes `offset|class` into the
  in-metadata `RemoteFreeRing`, and metadata pages are never decommitted). The
  full safety argument is recorded in code at the decommit point and in
  `docs/PHASE35_DECOMMIT_DESIGN.md` §1. A **post-decommit stale-free guard**
  (`off >= bump` after the reset) in both `dealloc_small` and `reclaim_offset`
  closes the window where a late free / double-free / stale ring entry targeting
  a reset segment would write a free-list `next` into a decommitted page. NO new
  dependency, NO new `unsafe` site (the OS seam already existed; the bookkeeping
  is plain safe arithmetic through the `node` seam). Tests (`alloc-decommit`):
  `decommit_soak` (decommit fires on `live→0` + recommit readback; counterfactual
  proven — the soak goes red if the hook is disconnected), `decommit_stale_ring`
  (stale ring entry into a decommitted segment is a no-op, no UAF),
  `decommit_miri_cycle` (bounded miri decommit/recommit bookkeeping). Verified:
  full suite green WITH and WITHOUT the feature (incl. `alloc_core_differential`,
  the heap suite, `race_repro`/`race_norecycle`/`global_alloc_mt`), clippy clean,
  miri on the bounded cycle. `heap_cross_segment`'s strict free-list-reuse
  invariant is relaxed under `alloc-decommit` to a bounded-footprint invariant
  (decommitted segments are legitimately re-carved, not free-list-reused).

- **Phase 12 — production multithreaded trust + Phase 12.6 cross-thread-free
  reclaim** (behind `alloc-xthread`). The installed `#[global_allocator]` is now
  a SOUND multithreaded drop-in: heap-as-shard isolation (each heap = a shard
  owned by one thread via a FREE/LIVE slot token), a self-hosted `HeapRegistry`,
  raw-pointer TLS with a never-null fallback heap, and loom-gated segment
  adoption (12.1–12.5). **Phase 12.6** closes the cross-thread-free
  *reclaim*: a non-intrusive per-segment MPSC ring carries each freed block's
  `offset | class` (the freer has the `Layout`; the owner's `page_map` class is
  unreliable for the mixed-class pages a shared bump cursor produces — the true
  root, found via ThreadSanitizer + a Linux free-list audit; NOT a data race).
  The owner reclaims lazily on its alloc-slow-path. This removes the Phase-12.5
  bounded-leak *discard* — cross-thread-freed blocks are now **reused**. Also
  fixed a real `SegmentHeader` data race (field-specific `bump`/`magic`/
  `owner_thread_free` accessors). Verified on Windows + Linux: `race_repro` ×5,
  `race_norecycle` (reliable Linux repro), isolated ring + reclaim unit tests,
  loom protocol/ring models with counterfactuals, full suite, clippy.
  See `docs/RACE_DRAIN_RECLAIM.md` (§13 root, §14 fix) and
  `docs/CROSS_THREAD_STATE_MACHINES.md` (the state-machine spec).
- **Phase 13.1 — O(1) size-class lookup** (`const SIZE2CLASS` table replacing the
  per-alloc linear scan).

- **Phase 11 -- the `malloc` face: `SeferMalloc` (`#[global_allocator]`) +
  no-panic hardening + honest mimalloc verdict** (behind a new opt-in
  `alloc-global = ["alloc"]` feature). `SeferMalloc` is an `unsafe impl
  GlobalAlloc` over the per-thread segment heap (one substrate, two faces: the
  typed `Handle` face and this raw `*mut u8` drop-in face), routing
  `alloc`/`dealloc`/`realloc`/`alloc_zeroed` through the no-panic TLS binding
  `with_heap_try` (returns null / no-ops instead of panicking — a panic in a
  global allocator aborts the process). **No-panic hardening:** the substrate's
  alloc-path panic sites were made graceful — the `alloc_small` `.expect` is
  gone, `SegmentTable::register` and `Segment::reserve` now return `Option`
  (null on failure, never `assert!`-panic). **Reentrancy-freedom (M5)** holds on
  the malloc path (no `Vec`/`Box`/`std::alloc`/`format!`). The `unsafe impl
  GlobalAlloc` is the documented malloc-face seam (every method `// SAFETY:`);
  `unsafe` stays confined. **Honest verdict (`docs/ALLOC_BENCH.md`):** on the
  alloc/dealloc hot path `SeferMalloc` is competitive with `mimalloc` (faster at
  1024 B and on realistic `Vec` push/grow churn; ~1.2-2x behind on small
  fixed-size churn) and consistently **~2.5-5x faster than the Windows system
  allocator** — safe by construction. Proven working as a real
  `#[global_allocator]` for a single-threaded workload
  (`examples/global_allocator.rs`: 100 k-`Vec` + 10 k-`HashMap`), and correct via
  direct-API tests (`tests/global_alloc.rs`: aligned, non-overlapping, reusable,
  realloc-prefix-preserving, 20 k churn). **NOT yet production-trusted:** as a
  *process-wide multithreaded* `#[global_allocator]` (e.g. under libtest's
  reentrancy-heavy harness) the current TLS binding returns null on
  reentrant/early-init/teardown access and aborts — a bootstrap-safe,
  reentrancy-tolerant TLS discipline is the remaining work, alongside the
  deferred heavy gate (`cargo-fuzz` CPU-hours, aarch64 multi-arch CI,
  ThreadSanitizer) and the Phase-10 deferrals (abandoned-heap adoption, M6
  decommit wiring). Honestly documented; for a process-wide allocator today, use
  `mimalloc`.
- **Phase 10 -- cross-thread free (M7), opt-in via `alloc-xthread`** (extends
  the `alloc` feature). Correct, lock-free cross-thread `dealloc` behind a
  new opt-in `alloc-xthread = ["alloc"]` sub-feature. When a thread frees a
  block it does NOT own, it pushes it onto the owning heap's atomic Treiber
  stack via a `compare_exchange` loop (the Phase-7b linearization protocol,
  re-based onto the Phase 8/9 segment substrate). The owner drains the stack
  in bulk on its next operation and returns each block to its per-class
  `FreeList`. O(1) owner lookup via `segment_base_of(ptr)` -> segment header
  -> `owner_thread_free` pointer (a stable `*const AtomicPtr<u8>` stored in
  each segment's header, pointing to the owning heap's `Box`-allocated Treiber
  head). The `ThreadFreeStack` is pure safe composition over
  `core::sync::atomic::AtomicPtr` + the `Node` seam (one new
  `Node::deref_atomic_ptr` in the existing `node` unsafe seam; no new unsafe
  module). **Thread-death soundness via abandonment-leak:** under
  `alloc-xthread`, `Heap::drop` intentionally LEAKS its segments (via
  `ManuallyDrop<AllocCore>`) and the Treiber head (via
  `ManuallyDrop<ThreadFreeStack>`) so that late cross-thread `dealloc` calls
  from other threads never touch unmapped memory or a freed `Box` -- segments
  stay mapped, the `AtomicPtr` stays allocated. This is a BOUNDED leak on
  thread death (one heap per thread), acceptable for the target long-lived
  thread-pool workload. Full abandoned-heap adoption (reclaiming leaked
  segments) is a Phase 11 deliverable. **Default `alloc` (no `alloc-xthread`)
  is unchanged Phase 9:** the single-thread-owner allocator with no
  `ThreadFreeStack`, no owner stamping, and normal segment release on
  `Heap::drop` (sound: single owner, no cross-thread refs). **Large / unstamped
  cross-thread free:** under `alloc-xthread`, a cross-thread free of a large
  block (`SegmentKind::Large`) or a block in an unstamped segment
  (`owner_thread_free == null`) is a documented no-op -- the block is
  conservatively leaked until the owning heap drops (or until Phase 11
  adoption). This avoids mis-accounting and is sound. **Decommit (M6) is NOT
  delivered** -- the `os::decommit_pages` / `os::recommit_pages` seam landed in
  Phase 10 (ready to wire) but is not integrated into the heap path. M6 is a
  Phase 11 deliverable. The soak test (`tests/heap_soak.rs`) asserts bounded
  segment growth via free-list reuse, not via decommit. Verification: **loom**
  model-check (`tests/loom_thread_free.rs`, 2 pushers + 1 drainer,
  `preemption_bound = 3`) with a proven counterfactual -- the naive non-CAS
  push demonstrably loses blocks under loom (the
  `counterfactual_naive_push_loses_blocks` test `#[should_panic]`s).
  Cross-thread differential proptest (`tests/heap_cross_thread.rs`, 64 cases,
  multiple threads, pattern write+readback -- non-vacuous). Soak test
  (`tests/heap_soak.rs`) -- bounded segment usage under sustained churn.
  Miri-clean on the cross-thread atomic seam (`tests/heap_miri_xthread.rs`,
  2-thread alloc/free, with `-Zmiri-ignore-leaks` for the intentional
  abandonment-leak).
- **Phase 9 -- per-thread heap + intrusive free lists (the lock-free fast
  path)** (behind a new opt-in `alloc` feature = `["alloc-core"]`). Each
  thread owns a `Heap` with per-size-class intrusive free lists stored inside
  the freed blocks themselves (via the Phase 8 `node` seam -- zero metadata
  allocation). The hot path (`alloc_small` / `dealloc_small`) is a single
  pointer read/write -- no lock, no atomic, no `Vec`/`Box`/`std::alloc` (M5
  reentrancy-freedom upheld). On free-list drain, a batch refill carves
  blocks from the Phase 8 `AllocCore` substrate. TLS heap binding via
  `std::thread_local!` with lazy, allocation-free init (`with_heap`); heap
  released on thread exit. Large/huge allocations route through the Phase 8
  dedicated-segment path. No new `unsafe` module -- the heap is pure safe
  composition over the Phase 8 `os` + `node` seams. Cross-thread free is
  Phase 10. Differential proptest (M1--M4 through the heap, 64 cases),
  targeted unit tests (alignment, reuse, refill, realloc, churn, multi-thread
  isolation), miri-clean. Single-thread throughput bench vs mimalloc and the
  system allocator (`benches/heap_alloc.rs`, `docs/HEAP_BENCH.md`): the heap
  matches the system allocator but is ~7--12x slower than mimalloc on the hot
  path; the architecture is structurally correct (same design as mimalloc) and
  the constant-factor gap is implementation overhead targeted for Phase 11.
- **Phase 8 — segment substrate + self-hosted metadata (the Membrane
  Inversion)** (behind a new opt-in `alloc-core` feature). The foundation of a
  real general-purpose allocator: the safe slot-table discipline stops
  *consuming* `Vec<T>` and starts *governing* OS-backed, SEGMENT-aligned memory
  (default 4 MiB), with the allocator's own metadata **carved from the segments
  it manages** (no `Vec`/`HashSet`/`std::alloc` on any alloc path). `unsafe`
  stays confined to exactly two documented seams: `os` (the OS aperture —
  `VirtualAlloc`/`VirtualFree` on windows, `mmap`/`munmap` on unix, via an
  over-reserve+trim for SEGMENT alignment; replaces `std::alloc` entirely) and
  `node` (the intrusive free-list node r/w, generalising the `hand` discipline).
  Everything between — `SegmentTable` (self-hosted generational registry),
  `PageMap`/`BinTable` (per-segment page descriptors + per-class free bins), the
  primordial `bootstrap`, the ~40-class size scheme, and `AllocCore`'s
  single-threaded `alloc`/`dealloc`/`realloc`/`alloc_zeroed` — is pure safe
  integer arithmetic (the Cartographer). Invariants **M1–M8** documented
  (`docs/INVARIANTS.md`, spec in `docs/ALLOC_PLAN.md` §4) and encoded as a
  differential proptest (M1–M4 vs a reference model), targeted unit tests, and a
  **runtime reentrancy audit (M5)** — a counting global allocator proves the
  alloc path never recurses into `std::alloc`. The core is **miri-clean**:
  because miri cannot execute the raw OS FFI, the `os` aperture has a
  `#[cfg(miri)]`-only fallback to `std::alloc` (test instrumentation; the
  production aperture is unchanged and the M5 proof runs without miri). Single
  confined unsafe per seam; `forbid`/`deny(unsafe_code)` everywhere else.
  **Supersedes** the Phase-4 `byte_region.rs` `std::alloc` fallback and its
  `Vec`/`HashSet` metadata. Per-thread heaps (Phase 9), cross-thread free +
  decommit (Phase 10), and the `GlobalAlloc` face (Phase 11) build on this.
- Initial scaffold of the `sefer-alloc` crate.
- Single-threaded `Region<T>` — a thin typed membrane over the
  [`slotmap`](https://crates.io/crates/slotmap) crate (`insert` / `get` /
  `get_mut` / `remove` / `contains` / `iter` / `clear`, all `O(1)`), built under
  `#![forbid(unsafe_code)]`; `slotmap`'s audited `unsafe` owns the dense
  generational engine, including version-saturation slot retirement.
- Typed, copyable `Handle<T>` — a newtype over `slotmap::DefaultKey` with
  hand-written `Copy`/`Eq`/`Hash`/`Debug` impls that hold for every `T`.
- `SyncRegion<T>` — the always-shippable concurrent baseline: a
  `RwLock<Region<T>>` with a guard API plus one-shot convenience methods, with
  poison recovery, still `#![forbid(unsafe_code)]`.
- `LockFreeRegion<T>` (behind the opt-in `experimental` feature) — **lock-free
  reads** via `arc-swap` RCU with page-granularity copy-on-write: readers load
  an immutable snapshot and resolve handles without any lock; rare writers
  serialise, copy only the touched page, and publish atomically. Values live
  behind `Arc<T>`; reclamation is plain `Arc` refcounting. **Zero `unsafe` of
  our own** — the crate stays `#![forbid(unsafe_code)]` with the feature on.
- `EpochRegion<T>` (behind `experimental`) — the fixed-capacity epoch tier with
  O(1) per-slot writes: lock-free reads via a seqlock-validated
  `(generation, value)` publication protocol and `crossbeam-epoch` reclamation.
  Introduces the crate's **single confined `unsafe` organ** (`concurrent::hand`,
  `AtomicSlot<T>`); confinement is compiler-enforced (`#![deny(unsafe_code)]`
  crate-wide under the feature, lifted only in that one module). The publication
  protocol is **loom-model-checked**; live values are dropped on region drop
  (I5). miri cannot run the tier only because `crossbeam-epoch`'s global
  collector is not miri-clean upstream — our `unsafe` is not implicated.
- `ShardedRegion<T>` and `ShardedHandle<T>` (behind `experimental`, Phase 7a) —
  **N-way parallel writes** via the single-writer principle: a `Box<[EpochRegion]>`
  of shards plus a thread-local router that lazily binds each writer thread to one
  shard (atomic round-robin), so two writers in different shards never meet on a
  lock. Reads stay the untouched lock-free `EpochRegion` seqlock. **Pure safe
  composition — zero new `unsafe`**; the module compiles under the crate's
  unsafe-confinement. `ShardedHandle` carries the shard id so reads/removes route
  back to the owning shard. Honest 7a edge: a claimed shard is not released
  (fits a bounded pool of long-lived threads; the shard lifecycle + lock-free
  cross-thread remove land in 7b). A multi-shard differential proptest (I1–I4
  across shards) and a routed concurrent stress test guard it; a write-scaling
  bench (`benches/sharded_write.rs`) compares it to the `SyncRegion` / `Arc<Mutex>`
  baselines.
- **Phase 7b — lock-free cross-thread removal + shard lifecycle** (behind
  `experimental`). A non-owner thread can now `remove` a handle WITHOUT taking
  the owning shard's writer mutex: `AtomicSlot::try_evict_at` performs a
  generation **`compare_exchange`** as the single linearization point — exactly
  one thread wins per generation, so exactly one schedules `defer_destroy` and
  decrements the (now `AtomicUsize`) live count (no double-free, no
  lost-live-value). The freed index is enqueued to a per-shard remote-free queue
  the owner drains on its next op (free list stays owner-only). `EpochRegion`
  gains `remote_evict`; `ShardedRegion::remove` routes owner-path vs lock-free
  remote-path by the calling thread's shard. Shards are now **releasable**: a
  thread-local `Drop` guard frees the shard's `occupied` token on thread exit,
  so a dead thread's shard can be adopted by a new thread while its live slots
  stay resolvable (reads are ownership-free). The relaxed "any thread may evict"
  contract is **loom-model-checked** (`tests/loom_sharded.rs`, 1 owner + 1
  remote-remover + 1 reader, `preemption_bound = 3`) — verified to FAIL on the
  naive load-then-swap protocol. `unsafe` stays confined to `concurrent/hand.rs`.
- **Phase 7c — thread-per-core pinning** (behind a new opt-in `pinning` feature
  = `["experimental", "dep:core_affinity"]`). `ShardedRegion::bind_current_thread_to_shard`
  deterministically routes a thread to a specific shard (the auto round-robin
  claim cannot), and `PinnedRunner` spawns one worker per core, pins worker *i*
  to core *i* (via `core_affinity`, a safe wrapper — **zero new `unsafe`**), and
  binds it to shard *i* — so `shard == core` and the hot path has no lock and no
  cross-shard contention (also why it composes with `glommio`/`monoio`/`tokio`
  current-thread-per-core without "lock across `.await`"). `core_affinity` is an
  **optional** dependency: the default and `experimental` builds do not pull it.
  Pinning is best-effort (honoured per OS); the shard binding (the routing
  truth) always holds, so tests assert routing, not affinity. A `pinned_write`
  bench compares pinned vs unpinned with an honest, workload-dependent verdict.
- **Phase 7d — `ShardedByteArena`** (behind a new opt-in `byte-sharded` feature
  = `["byte"]`, research-flagged). N per-thread `ByteRegion` shards
  (`Box<[Mutex<ByteRegion>]>`) for parallel raw allocation: a thread binds to its
  own shard via a TLS round-robin router, so threads in different shards never
  contend on one lock. Cross-thread `dealloc`/`realloc` route to the owning shard
  via a scan over `ByteRegion::contains_ptr` (safe pointer-comparison, no
  dereference) — a pointer is never freed against the wrong shard. `prewarm()`
  carves a chunk per shard and touches its pages up front to remove cold-start
  latency (callable from a background thread; the arena is `Send + Sync`). The
  only added `unsafe` is a one-line `unsafe impl Send for ByteRegion` (the region
  owns all its memory; access is `Mutex`-serialised) — everything else is safe
  composition; `unsafe` stays confined to `src/byte/*`. Correctness (cross-thread
  free, concurrent per-shard churn, bounded chunk growth, realloc byte
  preservation) is covered by `tests/byte_sharded.rs` and is **miri-clean**.
  Honest verdict (`docs/BYTE_SHARDED_BENCH.md`): it parallelises across shards
  but is NOT a `mimalloc` competitor and never returns memory to the OS until
  drop — research, not production.
- `ByteRegion` and `ByteAllocator` (behind the research-flagged `byte` feature)
  — the descent to raw bytes: a size-classed free-list byte arena whose
  placement logic is pure safe integer arithmetic (the Cartographer), with the
  single irreducible `*mut u8` aperture confined and documented, plus an
  experimental `unsafe impl GlobalAlloc` delegating through a `Mutex`. The
  second confined-`unsafe` module; confinement stays compiler-enforced. The
  whole byte tier is **miri-clean**. Honest scope: it does not aim to beat the
  system allocator / `mimalloc` (see `docs/BYTE_BENCH.md`); resocks5's global
  allocator stays `mimalloc` regardless.
- Safety invariants I1–I5 documented (`docs/INVARIANTS.md`) and encoded as
  unit tests plus a proptest differential harness against a reference model
  (`tests/differential.rs`).
- Full detailed implementation plan — per-phase goals, deliverables, steps, and
  gates, plus dependency DAG, risk register, decisions log, and open questions
  (`docs/PLAN.md`) — alongside architecture notes (`docs/DESIGN.md`).
- Dual MIT / Apache-2.0 licensing; MSRV pinned to 1.88.
