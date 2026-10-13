# R20 PRF-04: free-heap-slot selection behind a user flag — design (ox, read-only)

Owner decision (2026-10-13): every PRF-04 variant goes behind a flag that the user sets when configuring the
allocator; the default is the safest scenario; ask @ox for the best possible design. This file records the
answer of the `ox` agent (claude-opus-5-5, xhigh), run read-only in an isolated worktree at `origin/main`
`d5c8b970`; nothing was built, run or edited. The text below is the agent's answer as delivered; it is a
design proposal, not an accepted plan. Owner decisions on it are pending (see the end).

---

## Verdict

1. **The flag is a type parameter on `SeferAlloc`.** It is not a `Profile` field, not a runtime enum, not a
   Cargo feature, and not a first-bind-wins process word.
   - Shape: `pub struct SeferAlloc<R = FreshFirst>`.
   - Three marker types implement a sealed trait `HeapReuse`:
     - `FreshFirst` — the default, current `main` behaviour;
     - `ReuseFirst` — the new lean variant (c), see §5;
     - `ReuseFirstMirror` — V4′ semantics as they are.
   - The user picks the policy in the `#[global_allocator]` static:
     `static GLOBAL: SeferAlloc<ReuseFirst> = SeferAlloc::with_profile(p).heap_reuse();`.
2. **The default is `FreshFirst`.** Its compiled claim, release, recycle and teardown paths are main's code,
   and the registry layout is unchanged. The 576-byte set is a separate static that the default path never
   names. Zero cost is proven by:
   - a deterministic Ir identity gate (default arm vs main): two-sided, zero allowance, 10-Ir noise ceiling,
     at both the registry layer and the real `GlobalAlloc` layer;
   - layout and binary receipts (§2).
3. **No strictly zero-tax reuse variant exists** (proof sketch, §5.3). Any variant that serves a stranded FREE
   slot before minting a new one must execute at least one extra instruction per operation, either on cold
   claims or on recycled claim+release pairs. Under the owner's "nothing measurable" rule, only a non-reusing
   policy can be the default. So "safest default" and "zero-cost default" select the same variant.
4. **Variant (c) `ReuseFirst` is a hint-spill design.**
   - Release *swaps* the single hint word instead of overwriting it, and spills a displaced valid hint into
     the bitmap.
   - A claim consults the bitmap only when the hint is empty and the bitmap's summary word is non-zero.
   - Reuse coverage in a quiet system equals V4′'s. Expected cost (INFERRED): about 3 Ir per cold claim and
     about 2 Ir per recycled pair, against V4′'s measured +8 and +15.
5. **Any mix of policies on the shared registry is safe.**
   - Hints and the bitmap are advisory. The slot-state CAS remains the only ownership authority, and the
     at-capacity scan remains the backstop. A mix costs reuse precision only, never correctness.
   - The process-wide maintenance worker picks its publication discipline once per pass from a one-byte
     "non-default policy seen" word. Only non-default binds write that word.
6. **V4′ (b) ships as asked, but `ReuseFirst` dominates it on every path at equal coverage.** Recommendation:
   deprecate `ReuseFirstMirror` one round after `ReuseFirst` passes its gate (owner decision).

## 0. Baseline facts (OBSERVED)

- **Main's selection order is hint → bump → scan at capacity.**
  - Hint swap: `src/registry/heap_registry/stack.rs:116`. Bump before any scan: `:124`.
  - Every FREE publication overwrites the one hint with a Relaxed store:
    - `HeapLease::drop`: `src/registry/heap_registry/claim.rs:593`;
    - `MaintenanceLease::drop`: `:489`;
    - `push_back_after_oom`: `:653`;
    - chunk-OOM minted index: `:243`.
  - The registry doc admits that "Older FREE entries may wait until a capacity … scan":
    `src/registry/bootstrap/registry.rs:79-81`. `MAX_HEAPS = 4096`: `:45`.
- **Cost of that order.** R19 barrier waves, K=32, W=128, Windows (`docs/perf/OPEN_ITEMS.md:3061`):
  - main: 3970 heaps, 5,420,187,648 B idle commit charge;
  - V4: 45,465,600 B.
- **V4′ run01, registry-layer Ir** (`docs/perf/R20_PRF04_CLAIM_PATH_GATE.md:54-64`):

  | Cell | Delta |
  |---|---|
  | start | +8 Ir |
  | cold claim | +8 Ir per claim |
  | recycled claim+release pair | +15 Ir per pair |
  | wave claim | −34,463 Ir (−91.40 %) per claim |

- **The reviewer's absolute rule.** No planned per-operation tax plus a 10-Ir noise tolerance per window would
  fail V4′'s cold and recycled windows
  (`docs/reviews/2026-10-13-r20-prf04-absolute-ir-instrument-reviewer-answer.md:107-113,157`).
- **Native R-live is still open.** R19 run02 measured +19.08 %, but that estimator cannot resolve 5 %
  (paired t p = 0.969) (`OPEN_ITEMS.md:3060`).
- **What V4′ changed:**
  - `free_set` added *inside* `Registry` (`6951a9d0:src/registry/bootstrap/registry.rs`);
  - `pick_with_saturation` signature changed (`6951a9d0:stack.rs:110,124`);
  - a forget after every won FREE→LIVE CAS (`6951a9d0:claim.rs:274`);
  - a set publish on every release (`:499,604,665`);
  - fresh-first-pinning tests rewritten, which stales evidence rows R4c-10/11/13/18
    (`docs/reviews/2026-10-13-r20-candidate-evidence-registry-readiness.md:23-31`).
- **Configuration today is per instance.**
  - `SeferAlloc { config: LargeCacheConfig }`, a ZST without `alloc-decommit`
    (`src/global/sefer_alloc/core.rs:133-141`); `with_config` and `with_profile` at `:277,319`.
  - The config is consulted only on the cold TLS bind path (`src/global/tls_heap.rs:356-367,431-436`).
  - The per-slot policy is first-materialisation-wins, with N2 conflicts counted (`claim.rs:144-177`).
- **Structure constraints.**
  - `src/global` already has 9 entries (grandfathered in `docs/structure_limits_baseline.csv`), so no new
    file can go there.
  - `bootstrap/` has 6 entries, `heap_registry/` 5, `sefer_alloc/` 6.
  - Evidence row R4b-11 pins `impl Drop for AbandonGuard` and `impl Drop for HeapLease`
    (`docs/evidence/source_scope.csv:3`).

## 1. Flag surface

### API

```text
// src/global/sefer_alloc/core.rs
pub struct SeferAlloc<R = FreshFirst> {
    #[cfg(feature = "alloc-decommit")] config: LargeCacheConfig,
    reuse: PhantomData<R>,                       // ZST: size unchanged
}
impl SeferAlloc {   // = SeferAlloc<FreshFirst>; constructors and assoc fns stay HERE
    pub const fn new() -> Self;  pub const fn with_config(..) -> Self;  pub const fn with_profile(..) -> Self;
    pub fn start_maintenance() -> Result<(), MaintenanceStartError>;   // unchanged, process-wide
    pub fn maintenance_running() -> bool;
}
impl<R: HeapReuse> SeferAlloc<R> {
    pub const fn heap_reuse<R2: HeapReuse>(self) -> SeferAlloc<R2>;   // type-changing builder
    pub const fn heap_reuse_kind(&self) -> HeapReuseKind;              // = R::KIND
    // stats(), trim_current_thread(), batch API: bodies unchanged, impl made generic
}
unsafe impl<R: HeapReuse> GlobalAlloc for SeferAlloc<R> { /* current_heap() -> R's resolver */ }

// src/global/sefer_alloc/heap_reuse/  (new dir; sefer_alloc/ 6 -> 7 entries)
//   mod.rs (re-exports only), heap_reuse.rs, heap_reuse_kind.rs,
//   fresh_first.rs, reuse_first.rs, reuse_first_mirror.rs  — one export each
pub trait HeapReuse: sealed::Sealed + Copy + Send + Sync + 'static {
    const KIND: HeapReuseKind;
    #[doc(hidden)] fn __current_heap(/* &LargeCacheConfig under alloc-decommit */) -> CurrentHeap;
}
#[non_exhaustive] pub enum HeapReuseKind { FreshFirst, ReuseFirst, ReuseFirstMirror }
pub struct FreshFirst;  pub struct ReuseFirst;  pub struct ReuseFirstMirror;   // ZST markers

// user
#[global_allocator]
static GLOBAL: SeferAlloc<ReuseFirst> = SeferAlloc::with_profile(Profile::new()).heap_reuse();
```

- Constructors and associated functions stay on the default impl (the `HashMap::new` with `S = RandomState`
  pattern). So `SeferAlloc::new()` still infers `FreshFirst`, `static A: SeferAlloc` still means
  `FreshFirst`, and existing code compiles unchanged.
- `lib.rs` adds `pub use global::heap_reuse;`.
- To check: a value-taking builder `.heap_reuse(ReuseFirst)` needs a const drop of a generic parameter. If
  that does not compile, keep the inference/turbofish form shown above.

### Why type-level

| Alternative | Rejected because |
|---|---|
| Field in `Profile` / `LargeCacheConfig` (runtime) | The default would need a runtime branch at bind and at release, plus a policy tag in the lease: about 4 Ir per thread lifecycle, which fails the zero-tax rule. `LargeCacheConfig` is a per-slot first-wins setting compared by N2 conflict detection; the reuse policy belongs to the claim/release protocol, not to a slot. |
| Process-wide policy word, first bind wins | Every default claim and release would read it. A library's instance could silently override the binary's choice. |
| Cargo feature | Features unify across the dependency graph, so any dependency could flip the policy and the choice would no longer be the user's. Mutually exclusive features are an anti-pattern, and the powerset CI grows. |
| Type-level (chosen) | Resolved at compile time, so the default path has nothing to dispatch. Only the author of the `#[global_allocator]` static chooses it. It is visible in review, and a mismatch between instances shows up in the types. |

### Why `FreshFirst` is the safest default

| Criterion | FreshFirst | ReuseFirst / ReuseFirstMirror |
|---|---|---|
| New shared mutable state on thread paths | none | 520-byte bitmap (576 aligned), summary word, seen word |
| Proof surface | existing: `tests/loom_r11_registry_claim.rs`, `loom_r8_maintenance_lease.rs`, fresh-first pins (`tests/r11_p3_registry_claim_selector.rs:72`, `tests/registry_basic.rs:211`) | new Loom models (§4) |
| Evidence registry | rows stay valid (only R4b-11 is re-pinned: file text changes) | new rows |
| Open native question | none | R-live +19.08 % for V4 is unexplained |
| NUMA (`numa-aware`) | a new thread first-touches its own primordial heap | reuse hands out pages already placed on another node |
| Worst case | bounded, observable retention: up to `MAX_HEAPS` heaps under barrier-wave churn (`stats().heaps_claimed_high_water`) | heap count ≈ peak concurrent bindings |

"Safest" here means: no new shared state, every existing proof still applies, and no open question.
FreshFirst's only known weakness is resource retention, not correctness. The documentation must state that
retention plainly and point thread-pool-recreating workloads (join-all then spawn-all, `thread::scope` in a
loop) to `ReuseFirst`.

### Documentation contract (rustdoc on `HeapReuse` and each marker; prose only, no doctests)

1. What is selected: how a thread's first allocation picks a registry heap, and how the thread's exit
   republishes it.
2. The default rationale above, with the retention numbers.
3. `ReuseFirst` bounds the *count* of materialised heaps, not RSS.
4. Mixing semantics (§3).
5. Maintenance interaction.
6. The NUMA caveat, and the native R-live caveat until a quiet-host measurement exists.
7. Measured costs, inserted from the gate.
8. `ReuseFirstMirror` behaves the same as `ReuseFirst` with more bookkeeping; it is kept as the evidence
   reference.

## 2. Zero cost for the default

### How (by construction)

- **Discipline as a type parameter.** A crate-internal sealed `SlotDiscipline` trait, with only
  `#[inline(always)]` hooks, parameterises:
  - `claim_impl::<D>` (`claim.rs:218`) and `push_back_after_oom::<D>` (`:641`);
  - `HeapLease<D = FreshDiscipline>` (`:513,575`) and `MaintenanceLease<D = FreshDiscipline>` (`:420,471`);
  - `AbandonGuard<D = FreshDiscipline>` (`tls_heap.rs:162,195`) and `finish_bind::<D>` (`:486`).

  `FreshDiscipline`'s hook bodies are exactly the statements they replace:
  - `pick` = today's `pick_slot` (`claim.rs:391-414`), with `pick_with_saturation` (`stack.rs:108`)
    **untouched**;
  - `publish_free` = `reuse_hint.store(idx, Relaxed); saturation.publish_claimable()`;
  - `after_reclaim` = no-op.

  Typestate guarantees that a lease is always released with the discipline it was claimed under.
- **No new storage in the default layout.**
  - `Registry` gains **no field**.
  - The bitmap is V4′'s `FreeSlotHint` (`6951a9d0:src/registry/bootstrap/free_slot_hint.rs:20-89`,
    unchanged), held in a separate `pub(crate) static FREE_SLOTS` in the same file. `bootstrap/` goes from
    6 to 7 entries.
  - The non-default TLS guards `GUARD_MIRROR` and `GUARD_SPILL` are separate `thread_local!`s. The default
    `GUARD` (`tls_heap.rs:157`) is unchanged.
  - The non-default resolvers and binds are referenced only by the non-default markers, so they are
    unreferenced in default release binaries and linker garbage collection drops them (INFERRED; receipts
    below).
- **The only non-zero default delta.** The opt-in maintenance worker loop (`maintenance_service.rs:137-144`)
  gains one Relaxed load and one branch per 10 ms pass, to pick the discipline (§3). That is not a claim,
  release or recycle path, and it is disclosed. A default binary that calls `start_maintenance` links
  `FREE_SLOTS` as untouched zero-initialised memory: no RSS, no file size.

### Proof (budgets fixed before data, §6)

- **Ir identity, default candidate (F) vs main (A).** Every window of every family, at both layers
  (L1 registry, L2 `GlobalAlloc`), plus the start cell:
  - rule: `|w_F − w_A| ≤ T_s`, two-sided, with zero planned per-operation allowance;
  - `T_s = max(10, |A1−A2|, |C0−A1|)`;
  - absolute noise qualification: `|A1−A2|` and `|C0−A1|` must each be ≤ 10 per window, otherwise the
    result is INCONCLUSIVE;
  - the F cells call exactly A's entry points: `dbg_claim_lease()` plus drop at L1, and plain `SeferAlloc`
    as `#[global_allocator]` at L2.
- **Layout pins** (`tests/heap_reuse_flag_surface.rs`):
  - `size_of::<SeferAlloc>()` equals main's: `size_of::<LargeCacheConfig>()` under `alloc-decommit`,
    otherwise 0;
  - `size_of::<Registry>()` equals main's literal;
  - `size_of::<HeapLease>()` is unchanged.
- **Binary receipts** on the L2 default binary:
  - `readelf -lW` PT_TLS MemSiz: A == F;
  - `nm` shows no `FREE_SLOTS`, `GUARD_SPILL` or `GUARD_MIRROR`;
  - a disassembly diff of the default-path symbols (inlined alloc → `bind_slow_tagged_with_config` →
    `claim_impl`, `AbandonGuard::drop`, `HeapLease::drop`, `MaintenanceLease::drop`), reported
    descriptively with normalised addresses and hashes.
- **Behavioural receipt.** In the FreshFirst integration test, after the waves, `FREE_SLOTS` popcount = 0
  and summary = 0, read through an `internals`-gated read-only observer.

## 3. Shared-state semantics

1. **The binding instance's type decides.** The `HeapReuse` of the instance through which a thread makes its
   first allocating call governs that thread's claim and its exit release (through that discipline's
   guard). After binding, all instances use the thread's heap through the shared `LOCAL`, as today
   (`tls_heap.rs:141`). Dealloc-only, trim and fallback paths never bind, so they never involve a policy.
2. **Ownership is unaffected by any mix.** Only the slot-state CAS grants ownership
   (`claim.rs:252-273,112-122`). `reuse_hint` and `FREE_SLOTS` produce *candidates* only. Every FREE slot
   is still found by `scan_claimable_slot` at capacity (`stack.rs:12`) and by `scan_claim_recovery_free`
   (`:62`). So a mix never yields a duplicate owner or a permanently lost slot. It can only lose reuse
   precision:
   - a FreshFirst release overwrites a hint that a ReuseFirst process wanted kept (a stranded slot,
     recovered at capacity or by maintenance);
   - a ReuseFirst claim through the hint leaves a stale Mirror bit, which costs one failed CAS later.
3. **The supported configuration is unchanged.** One `#[global_allocator]` per process (already documented,
   `core.rs:246-259`), so per-instance equals per-process.
   - Detection: `AllocStats` (already `#[non_exhaustive]`, `src/global/alloc_stats.rs:55`) gains
     `heap_reuse_nondefault_mask: u8`, backed by `HEAP_REUSE_SEEN: AtomicU8`.
   - Only non-default binds write that word (load, test, then `fetch_or` once). A FreshFirst bind writes
     nothing.
4. **Slots carry no policy.** A heap materialised under one policy can be reused under another.
   `LargeCacheConfig` first-wins and N2 counting (`claim.rs:149-175`) are unchanged. Because ReuseFirst
   reuses more, multi-config processes will see N2 conflicts more often; document this.
5. **Maintenance** (a process singleton, `maintenance_service.rs:129-144`) picks its MAINT→FREE discipline
   once per pass from `HEAP_REUSE_SEEN`:

   | `HEAP_REUSE_SEEN` | Discipline |
   |---|---|
   | empty | Fresh (main, byte-identical `maintenance_pass`, `heap_registry/maintenance.rs:17`) |
   | {Mirror} | Mirror (V4′ as is) |
   | contains Spill | **Superset**: swap the hint, spill the previous value, also publish idx |

   - The superset conserves availability under any mixture, and its per-visit republication repairs slots
     stranded by mixing.
   - Residual: a pass that began before the first non-default bind finishes with Fresh discipline. That
     strands at most `SLOT_BUDGET` = 32 hint overwrites (`maintenance_service.rs:28`), which are recovered
     later. This is documented.
6. **`SeferAlloc::start_maintenance()` stays one associated function.** The discipline is automatic, so
   there is no inference break and nothing for the user to choose.

## 4. Correctness per variant

### Invariants (all variants)

- **I-own.** Authority is granted only by winning the EMPTY→INITIALIZING, FREE→LIVE or FREE→MAINTENANCE CAS.
  `HeapLease` and `MaintenanceLease` are created only after the win.
- **I-pub.** An index enters the hint or the bitmap only *after* its Release CAS to FREE. V4′'s Loom
  negative `negative_free_set_publish_before_state_cas` shows the failure when this is violated.
- **I-range.** A bitmap candidate is accepted only if `< count`, checked with a fresh Acquire load (V4′
  `stack.rs:124-130`).
- **I-sat.** Saturation is certified only by a scan that started at capacity (`stack.rs:130-138`); every
  publication calls `publish_claimable`.
- **I-bound.** `take()` makes at most `2·WORDS` attempts. Each stale or duplicate candidate costs at most one
  failed CAS, and these are bounded by the number of publications.
- **I-avail (quiet system).** Every initialised FREE slot is the hint value or has its bit set. Main
  satisfies this only for the latest slot, which is the defect.

### Per-variant hooks

| Hook | FreshDiscipline (= main) | MirrorDiscipline (= V4′) | SpillDiscipline (new) |
|---|---|---|---|
| `pick` | hint → saturation → bump → cap scan | hint → `take` → saturation → bump → scan | hint → [summary ≠ 0 ⇒ `take`] → saturation → bump → scan (`forget` on a scan hit) |
| after won FREE→LIVE | — | `forget(idx)` | — |
| `recover` (chunk OOM) | `scan_claim_recovery_free` | same | same, then `forget` |
| `publish_free` (lease, ctor OOM) | hint `store`; `publish_claimable` | hint `store`; `set.publish(idx)`; `publish_claimable` | `prev = hint.swap(idx, Relaxed)`; if `prev < MAX`, `set.publish(prev)`; `publish_claimable` |
| `publish_unmaterialised` (`claim.rs:242-246`) | `store` | `store` (V4′ does not set-publish) | same as `publish_free` |

### Hazards

- **ABA.** Bits are idempotent, with no links. Hint values are plain indices, so an ABA on them yields only a
  stale candidate.
- **Lost slot under Spill.** The hint word is an exchanger. Every value leaves it either by a claim swap (it
  becomes a candidate) or by a release swap (it is spilled), so RMW coherence alone conserves it and Relaxed
  ordering suffices. A candidate dropped after a lost CAS belongs to a slot that is LIVE (its owner
  republishes on exit) or MAINT (its MAINT→FREE release republishes).
- **Duplicates under Spill.** A scan-claimed slot that is still the hint value, or a superset maintenance
  release, can produce a duplicate candidate. Cost: one failed CAS.
- **Duplicate claim.** Impossible (I-own).
- **Typestate.** Because of `HeapLease<D>`, the rollback paths in `finish_bind` (`tls_heap.rs:496,508,534`)
  and the process-exit path (`:213-217`) automatically publish with the lease's own discipline.
- **R11 acceptance.** An all-live first-touch wave makes 0 slot probes: Spill reads only the summary word
  (`tests/r11_p3_registry_claim_selector.rs:51` stays as is).

### Loom and tests (all in `tests/`, wired into `scripts/loom.mjs`, `ci.yml` and the sentinels)

- **Fresh:** the existing `loom_r11_registry_claim.rs` and `loom_r8_maintenance_lease.rs`, unchanged.
- **Mirror:** `loom_r19_registry_free_set.rs`, ported from `6951a9d0`.
- **Spill:** new `loom_r20_heap_reuse_spill.rs`.
  - Model: 3 slots, two releasers, one claimer (hint → maybe-take → CAS) and maintenance.
  - Asserts unique owners, and I-avail when quiet.
  - `should_panic` mutants: store instead of swap; swap before the CAS to FREE; Fresh-discipline maintenance
    in a Spill process; `take` ignoring the old bit.
- **Mixed:** new `loom_r20_heap_reuse_mixed.rs`. Fresh releaser + Spill claimer + Mirror claimer + superset
  maintenance. Asserts unique owners only.
- **Selector model:** `heap_reuse_selector_model.rs`, a deterministic selector using the real
  `FreeSlotHint`. Also the bitmap unit test from `6951a9d0`, plus a Miri run of it.
- **Wave tests:** `heap_reuse_wave_{fresh,mirror,spill}.rs`, each its own binary with
  `#[global_allocator] SeferAlloc<P>`: warmup, then W=6 waves of K=4 threads.
  - Fresh asserts heap-count growth == W·(K−1), pinning that the default is unchanged.
  - Mirror and Spill assert growth ≤ K, the expected seen-mask, and Δconfig_conflicts = 0.
- **Registry paths:** `heap_reuse_registry_spill.rs` covers the chunk-OOM, constructor-OOM and
  cap-scan-forget paths through the existing injection hooks.
- **Counterfactual:** the spill tests must go red with a store-based release.
- **Untouched:** the existing fresh-first tests and evidence rows R4c-10/11/13/18.

## 5. Lean variant (c): `ReuseFirst` = hint-spill

### 5.1 Mechanism

```text
release (LIVE/INIT/MAINT -> FREE CAS done):
    prev = reg.reuse_hint.swap(idx, Relaxed)      // xchg replaces mov: same Ir
    if prev < MAX_HEAPS { FREE_SLOTS.publish(prev) }   // only when a valid hint is displaced
    reg.saturation.publish_claimable()
claim pick:
    cand = hint.swap(MAX, AcqRel); before = count   // identical to main
    if cand < before { return cand }                // recycled path: identical, NO forget
    if FREE_SLOTS.summary.load(Acquire) != 0 {      // cold path: +1 load +1 test/branch
        if let Some(i) = FREE_SLOTS.take_nonempty() { if i < count.load(Acquire) { return i } }
    }
    saturation -> bump -> cap scan(+forget on hit)  // as main
```

- The emptiness check is an inline load-and-branch in front of an out-of-line `take`. V4′'s +8 on cold claims
  came from the `take()` loop and closure overhead (V4′ `stack.rs:124`, `free_slot_hint.rs:49-53`).
- **Why it dominates V4′.** Quiet-system coverage is the same: the hint gives the most recent slot (LIFO) and
  the bitmap the lowest index. But Spill has no forget RMW on reclaim, no double publish on release, and the
  hint and bitmap are disjoint at publication.
- **Expected per-path cost (INFERRED):**

  | Path | Cost |
  |---|---|
  | cold claim | +3 Ir |
  | recycled pair | +2 Ir |
  | wave release with a spill | about +11 Ir |
  | wave claim | about 10–15 Ir, replacing about 37.7k Ir of bump + `HeapCore::new` (run01: −34,463 per claim) |

- **Memory.** 576 B in its own 64-byte-aligned static, off the `Registry` hot line (count/hint/saturation).
  The summary word is read-mostly, so cold claims cause no false sharing.

### 5.2 Disclosed limits

- `xchg` is a locked operation natively: about 20 cycles once per thread exit (0 Ir).
- On ARM64 without LSE, swap becomes an LL/SC loop.
- With K ≥ 32 concurrent wave claimers, they contend on the lowest bit. That is bounded by I-bound, after
  which the claim falls back to bump, and is visible only natively.

### 5.3 Why no reuse variant can be strictly zero-tax (INFERRED proof sketch)

1. On a cold claim (hint empty), main reads only `reuse_hint` and `count`, then bumps.
2. A variant that must reuse a stranded slot `a` in the wave state, but bump in the cold state, has to
   distinguish the two states. It can do so in one of two ways:
   - (i) read other state on that claim, which costs at least one extra instruction on cold claims; or
   - (ii) keep `a` in the hint word, which requires refilling the hint after it is consumed, or a release
     that tests or preserves the displaced value. That conditional (or the unconditional store) also runs in
     the recycled family, costing at least one extra instruction per pair.
3. Therefore the reviewer's rule (no planned per-operation tax, 10-Ir tolerance per window) admits only
   non-reusing policies. That is why the default must be `FreshFirst`, and why `ReuseFirst` is "lean", not
   "zero".
4. Rejected sub-design: refilling the hint on a hit makes cold claims free, but costs about 5 Ir per
   recycled pair and loses reuse under concurrent claimers.

## 6. Verification plan: one gate, all variants (CI-only)

### Artefacts

- prereg `docs/perf/R20_PRF04_HEAP_REUSE_FLAG_PREREG.md` (+ `.sha256`);
- judge `scripts/r20_prf04_heap_reuse_gate.mjs`, with `--self-test` and counterfactual mutants for: an
  absolute-only failure inside 5 %, boundary equality, negative deltas, and noise above the ceiling;
- runner `scripts/r20_prf04_heap_reuse_run.mjs` (the name fits the `r20-gates.yml` regex);
- cells `benches/r20_prf04_heap_reuse_iai.rs` + `benches/support/r20_prf04_heap_reuse.rs`;
- oracle `examples/r20_prf04_heap_reuse_oracle.rs`.

All runs go through `r20-gates.yml` `workflow_dispatch`, only on the owner's command (ubuntu-24.04,
valgrind 3.22).

### Arms

| Arm | What it is |
|---|---|
| A | main |
| X | one candidate commit; its cells instantiate F = `FreshFirst`, M = `ReuseFirstMirror`, S = `ReuseFirst` |
| C0 | main under another path label with `-C debuginfo=line-tables-only` (R20 noise control) |
| F′ | the same control for the X binaries |

Runs: A1/A2, F1/F2, M1/M2, S1/S2.

### Entry points (CLAUDE.md entry-point rule)

- **L2 (decisive)** is the shipping layer: `<SeferAlloc<P> as GlobalAlloc>::alloc` in one
  `#[global_allocator]` binary per arm.
  - A child thread's first allocation runs inside a named `#[inline(never)]` function. Collection uses a
    custom callgrind toggle on that symbol, with `--separate-threads=yes`, reusing the per-thread profile
    parsing in `scripts/r19_prf01_sidecar_summary_run.mjs:324-328`.
  - The release side uses a toggle on the guard-drop symbol (`AbandonGuard<…>::drop`).
  - Children are sequenced (spawn → first-alloc → park; join one at a time), so the order is deterministic.
- **L1 (decomposition)** uses the R20 registry-layer cells (`benches/support/r20_prf04.rs`).
  - F cells call `dbg_claim_lease()` plus drop, exactly as A does.
  - M and S cells call a new `HeapRegistry::dbg_claim_lease_as::<D>()` plus drop of `HeapLease<D>`,
    delegating to the same `claim_impl::<D>` as the TLS bind.
  - F, M and S live in one binary, so M/S-vs-F comparisons carry no build-layout noise.

### Families and windows (R20 rule §2.3 / §3a)

- start; cold, recycled and wave at N/2N/4N, with the same registered windows as R20.
- **New wave-release family:** hold 64 leases, release N of them; judged on slopes, with a 4N linearity
  check. This closes run01 §5's "release-only cost in a wave" gap.
- At L2: cold, recycled and wave claim; recycled and wave release.

### Budgets (frozen before any `SpillDiscipline` code exists; the owner signs the allowances)

| Comparison | Rule |
|---|---|
| F vs A, all windows and layers + start | two-sided `\|Δ\| ≤ T_s`, zero allowance, 10-Ir noise ceiling, otherwise INCONCLUSIVE |
| S vs F | one-sided `Δ ≤ L_w·P_S + T_s` |
| S and M vs A | `20·(w − w_A) ≤ w_A` (R20 5 %, unchanged) |
| M vs F | 5 % relative only; M's absolute deltas were already seen in run01, so they are reported, not judged (disclosed per the reviewer's §4) |

P_S allowances:

| Path | P_S |
|---|---|
| cold claim | 4 per claim |
| recycled | 4 per pair |
| spill release | 14 per release |
| start | `T_s + 4` |
| L2 cold | 7 per bind (L1 value + 3 for the seen-word mark at bind) |
| L2 recycled | 7 per pair (L1 value + 3 for the seen-word mark at bind) |

Principle for P_S: the mechanism's minimum instruction count from §5.1, plus one instruction of codegen
margin per path. Exceeding it means "not lean", which is a NO-GO for the `ReuseFirst` label. No budget is
ever widened after data.

### Activation oracles (bench-internals build, per arm × family × size)

- `dbg_registry_claim_sources()` extended to `[hint, bump, cap_scan, recovery, free_set]`, the
  `heaps_claimed_high_water` delta, and a read-only `FREE_SLOTS` popcount.
- At L2 also: the child is unbound on entering the named function and bound on leaving it; the process
  seen-mask equals the arm's bit (F/A: 0); Δconfig_conflicts = 0.
- Expected values:

  | Family | Arm | Expected |
  |---|---|---|
  | cold | all | bump = N, free_set = 0, popcount 0 |
  | recycled | all | hint = N; popcount afterwards: F/S 0, M 1 |
  | wave | A/F | hint = 1, bump = N−1 |
  | wave | M/S | hint = 1, free_set = N−1, bump = 0, new heaps 0 |
  | wave-release | F | popcount 0 |
  | wave-release | S | popcount N−1 |
  | wave-release | M | popcount N |

- R26-4 evidence per arm: requested type (runner), resolved `heap_reuse_kind()` plus seen-mask, conflict
  delta, and process boundary (each iai cell is a fresh process, so arms cannot leak by construction).
- Cost and benefit sit in the same regime: the wave family carries both the claim saving and the release
  cost. No Pareto claim may combine regimes.

### Verdicts

- **DEFAULT-IDENTICAL** is required before anything merges.
- S and M each get GO / NO-GO / INCONCLUSIVE / INVALID / INCOMPLETE, with R20 priority order.
- Native axes (RSS census from `examples/r19_prf04_thread_wave_gate.rs` extended per policy; first-call
  latency) are descriptive only on CI. R-live stays a documented caveat for a quiet host.

## 7. Implementation plan (separate PRs)

PRs 2–5 stack on an integration branch and merge to main only after the gate's DEFAULT-IDENTICAL.

1. **`bench(prf04)`: preregistration, judge and budgets.**
   - Freeze families, windows, budgets (including P_S) and verdict rules.
   - A-only harness-validity runs for L2: the child-thread toggles must collect non-zero Ir, with symbol
     receipts for both the first-alloc and the guard-drop symbols, and the raw logs are kept.
   - Pre-registered fallback: if guard-drop collection is infeasible, release is judged at L1 only, and this
     is disclosed.
2. **`feat(api)`: flag surface with `FreshFirst` only.**
   - `src/global/sefer_alloc/heap_reuse/*`; `core.rs` (type parameter, builder, kind); generic impls in
     `global_alloc.rs`, `diag.rs`, `batch.rs`; the `lib.rs` re-export.
   - Tests: `tests/heap_reuse_flag_surface.rs`, `tests/heap_reuse_global_fresh.rs`.
3. **`refactor(registry)`: discipline parameterisation, behaviour unchanged.**
   - New `src/registry/heap_registry/discipline/` (`mod.rs`, `slot_discipline.rs`, `fresh.rs`), taking
     `heap_registry/` from 5 to 6 entries.
   - `claim.rs`: `claim_impl::<D>`, `HeapLease<D>`, `MaintenanceLease<D>`, `push_back_after_oom::<D>`.
   - `tls_heap.rs`: `AbandonGuard<D>`, `finish_bind::<D>` (about 785 lines, under 1000).
   - No new `unsafe`: the unsafe-inventory grep output is unchanged.
   - Re-pin evidence row R4b-11 (teardown pin and the "torn after lease drop" mutant).
4. **`perf(opt-in)`: `ReuseFirstMirror`.**
   - `bootstrap/free_slot_hint.rs`: `FreeSlotHint` verbatim from `6951a9d0`, plus `static FREE_SLOTS`.
   - `discipline/mirror.rs`, `GUARD_MIRROR`, `HEAP_REUSE_SEEN`.
   - Maintenance per-pass selection: `maintenance_service.rs`, and `maintenance_pass_with::<D>` in
     `heap_registry/maintenance.rs`.
   - Ported Loom model and tests.
   - Register in `scripts/verify-dbg-hook-safety.mjs`: `dbg_claim_lease_as` (delegation),
     `dbg_forget_free_slot_hint` (inert mutator) and the popcount observer.
5. **`perf(opt-in)`: `ReuseFirst`.**
   - `discipline/spill.rs`, `discipline/maintenance_superset.rs`.
   - `pick_with_free_set` in `stack.rs`, without touching `pick_with_saturation`.
   - `GUARD_SPILL`.
   - Spill and mixed Loom models, the selector model test, wave and registry tests.
   - `discipline/` reaches 7 entries.
6. **`bench(prf04)`: measurement and report.**
   - Freeze cell and runner hashes before the first measuring build; one CI dispatch.
   - Report, summary CSV and evidence dir (by size tier); `OPEN_ITEMS.md` item 87 card; round manifest.
   - In the same task, if S or M is NO-GO, remove its hooks and variant (dbg-hook rule 3).
7. **`docs(config)`.**
   - README, `docs/INTEGRATION.md`, `docs/ARCHITECTURE.md` (selection order per policy), CHANGELOG, and the
     rustdoc numbers from the gate.
   - `npm run check` before each push, then confirm CI on the landing SHA.

### Risks

1. **The default is not machine-identical** because thin-LTO inlining or import decisions shift after the
   generic changes. Investigate with the disassembly diff; never widen the budget. Last resort: keep the
   legacy non-generic default functions verbatim, and route only non-default policies through generics.
2. **L2 cannot capture TLS-destructor collection.** Covered by the pre-registered fallback.
3. **Mixed policies and the first-pass maintenance window** cost reuse precision only; visible through the
   stats mask and documented.
4. **Native R-live and summary contention for M/S** cannot be resolved on CI; they stay as opt-in doc
   caveats.
5. **NUMA:** under `numa-aware`, reuse returns heaps whose pages sit on another node. Recommend FreshFirst
   for NUMA-pinned churn.
6. **API:** the defaulted type parameter is minor-compatible because constructors and associated functions
   stay on the default impl. The generic `const fn` builder needs a compile check.
7. **Surface:** M is dominated by S. Deprecate M one round after S passes, and remove it the round after
   (owner decision).
8. **Evidence:** only R4b-11 goes stale (`claim.rs`/`tls_heap.rs` text), against five rows for V4′. New
   Loom and test rows need `source_scope.csv` entries (VER-2).

## Observed vs inferred

- **OBSERVED:** all file:line facts in §0 and the gate numbers.
- **INFERRED**, each with a receipt or a pre-registered check above:
  - the Ir estimates and per-path allowances (§5.1, §6);
  - linker removal of the unreferenced static and TLS;
  - machine-code identity of the `FreshDiscipline` instantiation;
  - feasibility of the L2 child-thread toggles (precedent in PRF-01; needs an A-only validity run);
  - the lower-bound proof sketch (§5.3);
  - the const-generic builder's compile details.

---

## Owner decisions pending

1. Accept the type-level flag (`SeferAlloc<R = FreshFirst>`) as the user surface.
2. Sign the P_S allowances (§6) before any `ReuseFirst` code exists.
3. Keep `ReuseFirstMirror` (V4′ as is) as a selectable variant, and agree to deprecate it after
   `ReuseFirst` passes.
