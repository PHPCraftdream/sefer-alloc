# global perf/quality review — 2026-09-20

Scope: `src/global/` only — `sefer_alloc/{mod,core,diag,batch,global_alloc}.rs`,
`tls_heap.rs`, `fallback.rs`, `alloc_stats.rs`, `mod.rs`. Read-only static
review; no code was modified. Conventions/invariants read from `CLAUDE.md`
(one-export-per-file + sanctioned doc-hidden test-forwarder exception, the
tier-1/tier-2 `unsafe` discipline, the benchmark-hook rules, M5/M10 as
documented in the module docs themselves).

## Summary

No P0 soundness defect found in `src/global/`: the TORN-stamp-before-recycle
protocol, the Э2 one-branch sentinel collapse, the fallback init state machine
(with F-8's `InitStateGuard`), and the M5 no-`std::alloc` property all check
out against concrete failure scenarios, and the recent file split lost nothing
(same item set, same `#[inline(always)]`/`#[cold]` marker counts, `current_heap`
still `#[inline(always)]`). The findings below are hot-path asymmetries
(P1), a verified-clean representation audit with one micro note (P2), and a
cluster of post-split doc drift + dead code + gate inconsistencies left by
exactly the kind of move-only refactor the split was (P3).

## P0 — correctness/soundness

**None found.** Concretely refuted (OBSERVED by reading the paths, each with
the scenario that would expose it if broken):

- **Torn TLS read / stale-pointer UAF** — `AbandonGuard::drop` stamps
  `TORN` into `LOCAL` *before* `recycle` (`src/global/tls_heap.rs:203`,
  recycle at `:302`), and all five resolvers check the sentinel before any
  non-null deref (`src/global/tls_heap.rs:410`, `:499`, `:529`, `:557-571`).
  The `null`/`TORN` unsigned-range compare
  (`p.addr().wrapping_sub(1) < usize::MAX - 1`, `:410`) is arithmetically
  airtight: real pointers (1..=MAX-1) pass, `0` and `MAX` fall to cold arms.
  A late allocating TLS-dtor after `GUARD` drops resolves `TORN → Fallback`,
  never the stale pointer.
- **Reentrancy (M5)** — no `std::alloc` is reachable from any resolver or
  fallback path: `bind_slow` claims via the OS-aperture registry bootstrap,
  the fallback's TFS head is a `'static` atomic planted at init
  (`src/global/fallback.rs:101`, bound at `:249` before the `READY` publish),
  and `with_heap` is a monomorphized generic (no closure boxing). The raw
  `Cell<*mut HeapCore>` has no borrow state to fail reentrantly — the
  Phase-11 `RefCell` abort mechanism is not quietly reintroduced anywhere.
- **Fallback init race** — the `UNINIT → INITIALIZING → READY` machine plus
  F-8's armed-on-unwind rollback (`src/global/fallback.rs:199`, `:419-431`)
  closes the loser-spin livelock; loser threads re-race the CAS on rollback
  (`:283-285`). The `addr_of_mut!` discipline avoids `static_mut_refs` UB
  (`:175`, `:227`).
- **Claimed-but-unguarded slot leak (L-6)** — `finish_bind` arms `GUARD`
  before publishing `LOCAL` and recycles + falls back on arm failure
  (`src/global/tls_heap.rs:681-687`). Correct, including the teardown-window
  case.

One cross-seam dependency is noted rather than re-proven (out of scope;
registry reviewers own it): the `ForeignNoBind` dealloc shortcut's soundness
rests on `HeapCore::dealloc_foreign_routing`'s null-base/magic guards
(`src/global/sefer_alloc/global_alloc.rs:110-111`). The in-file claim that a
"dangling/garbage `ptr` cannot fault" (`:103-109`) is overbroad for the
*unmapped*-segment case (see P3-5) but describes no additional exposure
relative to the own-heap path.

## P1 — complexity / hot-path cost

1. **`src/global/sefer_alloc/global_alloc.rs:151`, `:162` — `alloc_zeroed`
   and `realloc` are `#[inline]` while `alloc`/`dealloc` are
   `#[inline(always)]` (`:30`, `:49`).** The four methods are the same shape
   of tiny tagged dispatch over `current_heap()`, and `alloc_zeroed` is the
   calloc-shaped entry (`vec![0; n]`, `Box::new([0; N])`, `__rust_alloc_zeroed`)
   that real workloads hit nearly as often as `alloc`. The asymmetry predates
   the split (marker counts identical old vs new: 3 `inline(always)` both
   sides), so it is not split damage — but on this module's own hot-path
   discipline it is an unprincipled difference. Suggested direction: make all
   four `#[inline(always)]` (or all four `#[inline]`) and record which was
   chosen; do not mix.

2. **`src/global/sefer_alloc/diag.rs:57-58` + `:73-76` — under
   `alloc-stats` + `production`, `stats()` walks the registry slot array
   TWICE.** `large_cache_hits_total()` and `tcache_hits_total()` each
   independently iterate `0..count` (implementations at
   `src/registry/heap_registry/counters.rs:149` and `:232`, cited for
   context, out of scope) — each re-resolving `ensure()`, re-loading
   `count`, and taking one Acquire load of `HeapSlot::initialised` per slot.
   A single fused walk summing both counters per slot would halve the
   per-scrape cost exactly in the configuration the crate's own measurement
   docs recommend (`--features "production alloc-stats"`). Complexity class
   is unchanged (O(minted slots) either way); the constant halves. Also a
   small doc imprecision: `stats()`'s doc (`diag.rs:23-26`) says "walk over
   the initialized registry slots" — the walk covers all *minted* slots
   (`0..count`), skipping un-initialised ones after one load each.

3. **`src/global/tls_heap.rs:411-413` + `:669-676` — no negative caching on
   the bind-less cold arm, so some cold states repeat full claim work per
   allocation.** Two concrete (both rare, both currently *correct*) loops:
   (a) a thread in a registry-exhausted process (`> MAX_HEAPS` live threads)
   stays at `LOCAL == null` forever (exhausted `bind_slow_tagged` returns
   `Fallback` without publishing), so *every* `alloc` re-runs `pick_slot` +
   the `FREE → LIVE` CAS attempt before falling to the fallback — per-alloc
   O(claim attempt), not O(1); under `alloc-decommit` each attempt also
   copies the full config (`tls_heap.rs:530`, `*config`). (b) a never-bound
   thread whose TLS teardown has begun (another TLS-dtor allocates):
   `GUARD.try_with` now fails, so each `alloc` runs claim → rollback
   `recycle` → fallback (`:681-687`). Both are documented as pathological/
   cold, but a one-bit "bind is known-hopeless this thread" TLS flag would
   make them O(1) after the first attempt. Low priority; measure first.

4. **(non-finding, verified)** — no redundant `current_heap()`/
   `current_for_alloc[_with_config]` calls: each `GlobalAlloc` method
   resolves exactly once, `dealloc` correctly uses the passive
   `current_for_dealloc` under `alloc-xthread`
   (`src/global/sefer_alloc/global_alloc.rs:61-63`), and `batch.rs`
   resolves once per batch (`batch.rs:65`, `:105`). The `CurrentHeap` tag
   avoids a second `fallback::heap_ptr()` on the fast path as designed
   (`tls_heap.rs:372-374`).

## P2 — types / representation

- **`SeferAlloc` ZST-when-absent property: VERIFIED intact.**
  `src/global/sefer_alloc/core.rs:93-101` — the sole field is
  `#[cfg(feature = "alloc-decommit")] config: LargeCacheConfig`, and
  `new()` returns `Self {}` without it (`:145-148`). The struct is a true
  ZST in every `not(alloc-decommit)` configuration.
- **`AllocStats` (`src/global/alloc_stats.rs:54-195`)** — fixed 10×`u64`
  shape across feature combinations (documented, deliberate), `u32→u64`
  widening of `heaps_claimed_high_water` documented at the field
  (`:131-134`), `#[non_exhaustive]` present. No narrowing or
  representation issue found.
- **Micro (no action urged): `LargeCacheConfig` niche opportunities** —
  `Option<usize>`/`Option<u32>` fields could use `NonZero*` to shave bytes,
  and `Option<LargeCacheMode>` has no niche. Immaterial: the config is read
  by reference on the hot path (`core.rs:279-289`) and copied only on the
  cold bind. Recorded only because the docs themselves argue from the
  config's size (see P3-6).

## P3 — code quality / duplication / dead code

1. **Post-split doc drift in `src/global/sefer_alloc/mod.rs`.**
   `:77` — the five-tripwire inventory says "all reachable from **this
   file's** `GlobalAlloc` impl under `production`"; post-split the impl
   lives in `global_alloc.rs`, `mod.rs` has none. `:20`, `:25`, `:34`,
   `:52-55`, `:121` — the M5/M10 narrative is written around
   `tls_heap::current()`, which is dead code (see item 2); the mechanism the
   face actually uses is `current_for_alloc[_with_config]` /
   `current_for_dealloc`. `:63` — "`alloc_zeroed`: `alloc` + zero-fill"
   mischaracterizes the mechanism: the face delegates to
   `HeapCore::alloc_zeroed` (`global_alloc.rs:158`), whose zero-sourcing
   (virgin-page reuse, the R29-16/R30-3-measured path) is not a literal
   fill. Suggested direction: one doc pass retargeting `current()` → the
   tagged resolvers and "this file" → `global_alloc.rs`, consistent with the
   `tests/no_panic_doc_accuracy.rs` pins the doc itself leans on.

2. **Dead resolver cluster `current()` + `bind_slow()`**
   (`src/global/tls_heap.rs:340-367`, `:583-592`). `current()` is
   `pub(crate)` with `#[allow(dead_code)]` (`:342`) and zero callers
   (verified by grep: only doc-comment references crate-wide);
   `bind_slow()` (`:584`) is reachable only from it. The L-9g rationale
   (`:332-337`) justifies keeping it "pub(crate) (not pub)" to protect
   *external* consumers from the untagged-pointer hazard — but a
   `pub(crate)` item has no external consumers, so the rationale
   contradicts itself. Either delete both (the Э2 logic is duplicated
   verbatim in `current_for_alloc`), or move them under the
   `internals`-gated `pub mod global` surface if a test genuinely needs
   the untagged form.

3. **`trim_current_thread`'s feature gate is wider than its own documented
   work** (`src/global/sefer_alloc/diag.rs:155-156`,
   `#[cfg(feature = "alloc-decommit")]`). The method documents three jobs
   (tcache flush + small-pool drain + large-cache evict, `:105-108`), but
   `HeapCore::trim_for_recycle`'s tcache-flush half exists under
   `alloc-global + fastbin` with `alloc-decommit` off — proven in-tree by
   the unguarded call in `AbandonGuard::drop` (`tls_heap.rs:263-265`) and
   the per-step cfgs in `src/registry/heap_core/state/ownership.rs:255-268`.
   R31-10/task #474 already established exactly this fact — but fixed only
   the `dbg_trim_current_thread` bench hook (deliberately unconditional,
   `diag.rs:189-205`), leaving the *public* trim API absent (not merely
   degraded) in fastbin-only builds. Suggested direction: gate on
   `any(alloc-decommit, fastbin)` (or unconditionally, matching the hook),
   with a perf gate per the project's measurement rules if promoted.

4. **`dbg_*` test-hook gate inconsistency in `src/global/tls_heap.rs`.**
   `dbg_teardown_then_resolve_is_fallback` (`:723-731`) and
   `dbg_teardown_then_resolve_is_foreign_no_bind` (`:745-754`, gated only on
   `alloc-xthread`) are ungated/production-feature-gated, while their
   siblings in the same file, doing the same class of test-only LOCAL
   poking, were bench-internals-gated by R29-7/task #438 *citing
   CLAUDE.md's benchmark-hook rule 2* (`:779-786`, `:815-819`); same for
   `fallback.rs` (`dbg_fallback_lock_acquisitions` ungated `:474-478`,
   `dbg_panic_in_with_heap_releases_lock` std-gated only `:447-467`).
   Post-R34-3 the whole `global` module is `pub(crate)` without
   `internals`, so this no longer widens any production public surface —
   but the file now contains two different answers to the same rule for
   the same class of hook. Suggested direction: harmonize (cheap: gate the
   four stragglers on `bench-internals` or record why they are exempt),
   so the next R29-7-style sweep has one pattern to follow.

5. **SAFETY-wording inconsistency between the two `dealloc` arms**
   (`src/global/sefer_alloc/global_alloc.rs`). The `ForeignNoBind` arm's
   SAFETY note claims the guards mean "a dangling/garbage `ptr` cannot
   fault here either" (`:103-109`); the `not(alloc-xthread)` arm directly
   below explicitly *disclaims* any blanket dangling-pointer safety and
   pins fault-freedom to the caller's trait obligation (`:134-145`). Both
   arms have identical underlying exposure (a header read into a
   released/unmapped segment faults in either path), so one of the two
   comments is wrong about scope. Align both to the `:134-145` wording.

6. **A checkable size claim in docs, the exact thing the module's own
   drift-proofing discipline warns about.** `core.rs:274-276` ("not a
   ~40-byte value copy") and `tls_heap.rs:514-517` ("the ~40-byte
   `LargeCacheConfig` value… The 40-byte copy") state a byte count.
   Actual fields (`src/alloc_core/config/large_cache_config.rs:177`):
   2×`Option<usize>` + 2×`Option<u32>` + `Option<LargeCacheMode>` +
   `SmallSegmentPoolConfig` — ≥ ~49 bytes before counting `pool`, so the
   number understates and will rot silently on the next config field.
   `mod.rs:79-83` already establishes the right pattern for this module
   ("line numbers deliberately omitted… pinned by message string instead"):
   drop the magic number ("a small value copy, only on the cold bind
   branch").

7. **`mod core` shadows the `core` crate name inside `sefer_alloc/mod.rs`**
   (`src/global/sefer_alloc/mod.rs:125` + `pub use core::SeferAlloc;` at
   `:129`). Resolution is correct (local module wins; verified compiling at
   HEAD) and unavoidable without renaming given the split, but any future
   `use core::…` added to *this one file* will silently resolve to the
   child module. A one-line comment on the `mod core;` declaration naming
   the hazard would be cheap insurance. Note only — no rename requested.

## Follow-up measurement ideas

- **Fused `stats()` slot walk** (P1-2): single-pass aggregator returning
  both hit counters; judge with iai Ir on a metrics-scrape microbench under
  `--features "production alloc-stats"` — a deterministic instruction-count
  gate, not wall-clock. Only `alloc-stats` builds change; `production`
  alone is untouched by construction.
- **`#[inline(always)]` on `alloc_zeroed`/`realloc`** (P1-1): paired-A/B on
  a calloc-heavy churn workload (`vec![0; n]` alloc/sum/drop) under
  `production`; report the mechanism-activation side (that the zeroed path
  actually dominated) alongside the wall-clock, per the CLAUDE.md
  path-activation-oracle rule.
- **Fallback-traffic census**: `dbg_fallback_lock_acquisitions` /
  `LOCK_ACQUISITIONS` (`fallback.rs:145`, `:474-478`) already exist — a
  thread-churn workload reading the counter delta would size how often the
  TORN/teardown fallback path (and its documented ring-push trade-off,
  `global_alloc.rs:80-101`) really fires in production shapes, telling you
  whether P1-3's negative-caching flag or the R6 trade-off is worth
  revisiting with numbers.
- **`trim_current_thread` gate widening** (P3-3): if widened to
  fastbin-without-decommit builds, run the R31-10-style RSS/latency gate in
  exactly that feature set before promoting — the entry-point-layer rule
  (measure the layer the feature ships at) applies literally here.
