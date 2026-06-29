# Checkpoint — 2026-06-29 perf-fix-hardening

## Session summary

Tail of the 2026-06-29 release-prep arc. The earlier checkpoint
(`2026-06-29-release-prep.md`) closed at commit `92aeff1` with the
lazy-alloc Registry done; binary size 23.6 MB → 1.2 MB. This continuation
covers a **regression discovery + fix + hardening sweep**.

**Phase 5 — perf regression caught by user (commit `a782871`).** User
re-ran their `he_repl` benchmark (5 iterations × 2 binaries, `time` wall
+ internal `Instant::now()` instrumentation) and reported a **−30%
regression** introduced sometime between «sefer старая» and the post-
release-prep build:

- enum loop median:   8.8 ms → 11.5 ms (+31%)
- enum total median: 11.7 ms → 15.3 ms (+31%)
- process real:       239 ms →  260 ms (within ~10% noise)

Diagnosis: `c4be627` (env-var → const-builder refactor) made
`SeferMalloc::current_heap()` call `current_for_alloc_with_config(self.config)` —
passing a ~40-byte `LargeCacheConfig` value through `#[inline(always)]`
chain. Even with LTO and the config being USED only on cold-path
`bind_slow_tagged_with_config(config)`, LLVM materialised the 40-byte
struct on the stack at every alloc/dealloc site. The lazy Registry
change (`ad0da12`) was ruled out — it only touches cold path
(`HeapRegistry::claim`/`recycle`), not hot alloc.

Fix in `a782871`: take `config: &LargeCacheConfig` in
`current_for_alloc_with_config`, deref to value only on the cold
`bind_slow_tagged_with_config(*config)` branch. Hot path now loads an
8-byte pointer to the static config — cache-hot, near-free.

Re-measured: enum loop median 8.5 ms (within noise of 8.8 baseline),
enum total median 11.5 ms (within noise of 11.7). All 178+ tests pass.
Binary size unchanged (1.20 MB).

**Phase 6 — flamegraph for further wins.** Profiled he_repl under WSL
perf (cargo-flamegraph + inferno). Built a custom `[profile.profiling]`
in he_repl/Cargo.toml (`inherits = "release"` + `strip = false` +
`debug = "line-tables-only"`) to keep symbols. Captured 8335 samples
across 30 iterations. Findings:

  - **~22% of total runtime is std SipHash** on `HashSet<SmallVec<[u8;14]>>`
    using the DEFAULT RandomState hasher. They already use FxHash on
    some HashMaps; switching the HashSet's hasher to FxHash is a
    one-line user-side win.
  - Sefer-alloc footprint: **~10% of total** (4.19% __rust_alloc + 3.13%
    __rust_dealloc + 1.91% HeapCore::realloc + 1.18% AllocCore::carve_block_with_refill).
    Close to optimal for this workload after the recent passes.
  - Other top hotspots are he_repl's own algorithmic code
    (HeTools::unmask 10.41%, de_sofit_letters 8.45%, get_edge_w_letters
    7.62%, POLY3 6.44%).

The user's room for improvement is in their own code; sefer-alloc room
is marginal. SVG saved to `D:\dev\rust\he_repl\he_repl_flamegraph.svg`.

**Phase 7 — hardening sweep (miri / loom / TSan).** Before any further
push, ran the full hardening matrix to verify the recent refactors
(c4be627, ad0da12, a782871) introduce no UB, races, or M9-violations:

  - **Miri** `decommit_miri_cycle` ✅ 1/1 (201s) — alloc-decommit invariants
  - **Miri** `reclaim_offset_unit` ✅ 1/1 (381s) — cross-thread reclaim arithmetic
  - **Loom** `loom_registry` ✅ 4/4 — adopt protocol incl. counterfactual
  - **TSan (WSL)** `race_repro` ✅ 3/3 — drain-reclaim UAF repros
  - **TSan (WSL)** `race_norecycle` ✅ 1/1 — cross-thread reclaim no-recycle
  - **TSan (WSL)** `global_alloc_mt` ✅ 3/3 — SeferMalloc multi-thread churn
  - **TSan (WSL)** `heap_cross_thread` ✅ 3/3 — cross-thread free protocol

16/16 hardening tests pass. No data races, no UB, no UAF.

**Working-tree state at session close:** `run_tsan.sh` is untracked
(transient helper, not committed). One commit ahead of origin/main
(`a782871` not pushed yet).

## Active goal

none

## TaskList

(empty)

## Decisions

- **`LargeCacheConfig` plumbing — by reference on hot path, by value on cold.**
  Chose `&LargeCacheConfig` on the
  `SeferMalloc → current_for_alloc_with_config` boundary; the deref
  `*config` happens only on the cold `bind_slow_tagged_with_config`
  branch. Rejected: keep by-value (LLVM clearly doesn't DCE the
  unused-on-hot-path materialisation).
- **flamegraph profile preserved** as
  `[profile.profiling]` in he_repl/Cargo.toml (inherits release,
  unstrip + line-tables-only debug). User can re-profile any time.
- **Loom test for the new bootstrap CAS — deferred.** The pattern is
  textbook lazy-init (null → SENTINEL → real_ptr) and no existing
  harness covers it. Not a blocker for 0.1.0; tracked as a follow-up.
- **SeferMalloc → SeferAlloc rename — still deferred.** Raised earlier
  in the arc, no decision; not blocking the perf-fix work.

## Open questions

- **Push `a782871`?** One commit ahead of origin. User has not asked
  for push yet; previous push (10 commits) was on explicit request.
- **He_repl optimisation handoff.** User's biggest perf lever (~22%) is
  in their own code (FxHash on `HashSet<SmallVec<[u8;14]>>`). Mentioned
  briefly; not actioned in this session.
- **Loom on the rest of the suite** (`loom_xthread_protocol`,
  `loom_remote_ring`, `loom_thread_free`, `loom_sharded`, `loom_epoch`)
  — not run this session. The recent refactors don't touch those
  protocols; regression unlikely; can be deferred to CI.

## Repo state

```
?? run_tsan.sh
```

```
a782871 perf(global): pass LargeCacheConfig by reference to avoid hot-path copy
92aeff1 docs(checkpoint): 2026-06-29 release-prep arc
ad0da12 perf(registry): lazy-allocate Registry via aligned-vmem — −22 MB binary
6ebb0f4 test(alloc-decommit): close coverage gaps for LargeCacheConfig API
c4be627 feat(alloc-decommit): replace env-var config with LargeCacheConfig const builder
```

(1 commit ahead of `origin/main`: `a782871`. Push not done — awaiting
explicit user request.)
