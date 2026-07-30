# R30-7 — an explicit, caller-driven trim/scavenge API: DESIGN PROPOSAL, not implemented this round

**Task:** R30-7 (task #456), Round 30, Deliverable 3. **DESIGN-ONLY — no
`src/`, `Cargo.toml`, `tests/`, or `benches/` file changes anything about
runtime behavior in this document's scope.** This proposal is written
alongside R30-7's other two deliverables (named `Profile` presets,
`docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md`'s throughput-profile A/B)
but is independent of them: it does not depend on the profiles landing, and
the profiles do not depend on this landing.

**Style precedent:** `docs/perf/R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md` and
`docs/perf/R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` — a real design with
concrete signatures, explicit invariants, an honest scope of what it does
NOT claim, and a recommendation that may be "not yet" rather than a rubber
stamp.

**Date:** 2026-07-30. **Base revision analyzed:** `main` @ `97c2f07` (R30-6
landed) + this task's own uncommitted profile additions (`src/alloc_core/profile.rs`,
`src/alloc_core/large_cache_config.rs`, `src/global/sefer_alloc.rs`).
Line numbers cited below are current as of that tree.

---

## 0. TL;DR

R29-13 proved, exactly and reproducibly across 36 measured arms, that **pure
idle reclaims 0 KiB** from the large cache — the decay mechanism is
event-driven (fires only inside `maybe_decay_large_cache`, called from the
large alloc/dealloc slow path) and a heap that goes idle after a burst never
calls that path again, so its retained memory is permanent until either more
allocation traffic or thread-exit. R27-3 proved the same shape for the
small-segment pool (`maybe_decay_small_pool`, fired only from
`reserve_small_segment`). Both mechanisms are **decay-on-use**, not
**decay-on-idle** — an application that knows "this burst just ended, I am
about to go idle" has no way to tell the allocator that today.

This document proposes a small, explicit, caller-driven API —
`SeferAlloc::trim_current_thread()` — that lets an application say exactly
that. It is deliberately narrow in scope: one safe method, callable at any
time, that does for the CALLING thread's own heap what
`HeapCore::trim_for_recycle` already does at thread-exit, MINUS the
TLS-teardown/slot-recycle side effects, so the thread can keep using the
SAME heap afterward. The mechanism already exists almost verbatim as a
`#[doc(hidden)]` test-only hook (`SeferAlloc::dbg_trim_current_thread`,
`src/global/sefer_alloc.rs:423-431`) — this proposal is "promote that hook
to a real, documented, always-available public API," not "invent a new
mechanism."

**This explicitly differs from R27-5's adaptive-pool-budget design**
(§2 below), which was NOT built partly because idle shrink-back was
unsolved within this project's no-background-thread constraint. An
explicit, caller-driven trim call sidesteps that exact constraint instead
of fighting it: the caller supplies the "when," the allocator supplies the
"how," and no timer/background thread is needed at all.

---

## 1. The gap this closes — restated precisely

Both retention mechanisms this project has actually measured share the same
shape:

| mechanism | fires on | idle behavior (measured) |
|---|---|---|
| small-pool decay (`maybe_decay_small_pool`, `src/alloc_core/alloc_core_small_pool.rs:516`) | `reserve_small_segment` cold path only | flat across a 2 s idle window — [`R27_3`](../perf/R27_3_POOL_RETENTION_GATE.md) §3, exact, not noisy |
| large-cache decay (`maybe_decay_large_cache`, `src/alloc_core/alloc_core_large_cache.rs:320-356`) | large alloc/dealloc slow path only | flat across a 2 s idle window at EVERY headroom arm, 36/36 cells — [`R29_13`](../perf/R29_13_LARGE_CACHE_RETENTION_GATE.md) §0, "not one byte was reclaimed" |

Both reports independently established the SAME fact through DIFFERENT
mechanisms: **"no background thread" (this project's repeated, documented
design choice — `src/alloc_core/alloc_core.rs:135`, `large_cache_config.rs:330`,
`large_cache_mode.rs:14`) means retention only shrinks in response to
allocation TRAFFIC, and idle produces no traffic.**

This is the correct trade for the common case (an active server keeps
allocating, so decay ticks fire naturally) but it leaves exactly one
scenario with no lever: **a workload that bursts, then goes idle for a long
time, and wants its RSS back before the next burst** (e.g. a batch job that
processes one large request per minute; a connection handler between
requests; a CLI tool between subcommands). Today the only ways to reclaim in
that scenario are:

1. **Wait for the thread to exit** (`trim_for_recycle` runs automatically).
   Not usable for a long-lived thread/thread-pool worker that intends to
   keep serving future bursts.
2. **Wait for more allocation traffic** to happen to cross the decay
   interval. Not usable if the application specifically wants to shrink
   BEFORE the next burst, not incidentally during it.
3. **The existing `bench-internals`-gated / `#[doc(hidden)]` hooks**
   (`dbg_trim_current_thread`, `dbg_drain_small_pool`,
   `dbg_force_decay_tick`) — these already DO the mechanical work, but are
   explicitly test/bench-only, undocumented, and not part of the stable
   public surface an application would build a memory-management policy on.

---

## 2. Why this differs from R27-5's adaptive design — and why it is buildable where that one was not

[`R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md`](../perf/R27_5_ADAPTIVE_POOL_BUDGET_DESIGN.md)
proposed a PROCESS-DRIVEN mechanism: the allocator itself would decide,
via a growth heuristic + token budget, when a heap should shrink back
after a burst. Its §3.5 named the shrink-back sub-problem as "the hardest
part," and rejected the only option that reclaims during PURE idle (a
background timer thread) as against this project's repeated anti-precedent
(§3.5(a)). Its remaining options were both unsatisfying for exactly that
reason:

- **Option (b) (piggyback on the next alloc event)** — the design's own
  chosen fallback — has "a fundamental limitation R27-3 §3 exposes: a heap
  that went idle after growing NEVER calls `reserve_small_segment` ...
  so it NEVER gets a decay tick." R27-5 §4.3 states this plainly: "a
  once-grown heap retains its growth until thread-exit."
- **Option (c) (an explicit scavenge call site)** was named by R27-5 itself
  as "an escape hatch, not... the primary mechanism" — R27-5's own §3.5
  text: *"This shifts the idle-reclamation burden to the application and is
  functionally equivalent to... a future `trim` API — it does not solve
  automatic idle reclamation, it just names a manual hook."*

**This proposal IS R27-5's option (c), taken seriously as the PRIMARY
mechanism rather than dismissed as a mere escape hatch** — and the
reasoning for why that is the right call, not a consolation prize, is
exactly R27-5's own observation turned around:

> R27-5's shrink-back problem is hard SPECIFICALLY because the allocator
> does not know when a burst has ended — it can only infer "pressure
> ceased" from the ABSENCE of the next allocation, which is
> indistinguishable from "still busy, just between two individual
> operations" without a timer. An explicit `trim()` call removes that
> inference problem entirely: the APPLICATION knows when its own burst
> ended (a request finished, a batch job's phase completed, a connection
> went idle) with perfect information the allocator structurally cannot
> have. The caller is not being asked to solve a HARDER problem than the
> allocator's own automatic mechanism — it is being asked to supply the ONE
> piece of information (the phase boundary) that made automatic
> shrink-back hard in the first place. Once that information is supplied
> externally, the "how to shrink back" mechanism is NOT hard — it already
> exists, verbatim, as `trim_for_recycle` (§3.1 below); it was already
> exercised automatically at thread-exit long before this proposal, and
> reused unchanged by `dbg_trim_current_thread`, R30-6/R29-13's own
> measurement harnesses' teardown-and-remeasure step.

Put differently: R27-5 tried to build a policy engine (growth heuristics,
process-wide token budgets, a decay/scavenge subsystem) whose entire
purpose was to GUESS, from allocation-pattern shape alone, when it was
safe to shrink a heap back — and concluded the guess was either unneeded
(uniform pressure ⇒ cap-8-for-all) or insufficient at the exact case that
matters (idle stickiness, R27-5 §4.3). This proposal removes the guessing
requirement altogether by asking the party with ground truth (the
application) to state it directly. No heuristic, no token budget, no new
per-heap growth-state fields, no process-wide atomic — the mechanism this
proposal needs is a public wrapper around code that has existed and been
exercised since task #95/N1 (`trim_for_recycle`, thread-exit) and since
R30-6/R29-13 (measurement-harness teardown-and-remeasure steps that already
call the `dbg_`-gated equivalent).

**Where this proposal does NOT compete with R27-5's finding:** it does not
attempt uneven-pressure token routing (R27-5 §3, deferred, CONDITIONAL-GO
pending a measured uneven-pressure victim — still true, unaffected by this
proposal). If a future round DOES find that victim and revisits R27-5's
adaptive design, an explicit `trim()` and an adaptive growth/shrink policy
are complementary, not exclusive — `trim()` remains useful as a manual
override even if an automatic policy also ships.

---

## 3. Proposed API shape

### 3.1 The method — `SeferAlloc::trim_current_thread()`

```text
impl SeferAlloc {
    /// Trim the CALLING thread's own heap back to a comparable empty-ish
    /// baseline: flush every tcache class's magazine, drain the
    /// small-segment hysteresis pool, and evict the entire large cache.
    /// Call this when the calling thread KNOWS a burst/phase has ended
    /// (e.g. after finishing a batch of request handling, before an
    /// expected idle period) and wants its retained memory released to the
    /// OS now, rather than waiting for the next allocation-driven decay
    /// tick or thread-exit.
    ///
    /// This does NOT tear down the thread's TLS binding or recycle its
    /// registry slot — the thread keeps using the SAME heap afterward. The
    /// next allocation after this call takes the normal cold
    /// reserve-a-fresh-segment / re-populate-the-large-cache path instead
    /// of reusing whatever this call just released.
    ///
    /// A no-op on the fallback heap (no per-thread heap bound yet, or TLS
    /// already torn down) — there is nothing thread-local to trim.
    ///
    /// Cost: O(live tcache classes + pooled segments + cached large spans)
    /// for THIS thread only — no cross-thread coordination, no lock
    /// contention with any other heap. Safe to call from a hot request
    /// handler's cold "end of batch" branch; NOT intended to be called on
    /// every allocation (it defeats the warm-cache/warm-pool amortization
    /// this project's whole small-pool/large-cache design exists to
    /// provide — see §4.3 for the mis-use hazard this creates).
    #[cfg(feature = "alloc-decommit")]
    pub fn trim_current_thread(&self) {
        // identical body to the existing dbg_trim_current_thread
    }
}
```

Gated identically to the mechanisms it drives (`alloc-decommit` — the small
pool and large cache are both `alloc-decommit`-only; without that feature
there is nothing to trim, matching how `with_config`/`with_profile` are
already gated). NOT gated behind `bench-internals` — unlike the `dbg_*`
diagnostic accessors, this has a real, intended production caller (an
application's own phase-boundary code), so it does not fit the
"measurement-only, no production caller" category the `bench-internals`
gate exists for (CLAUDE.md's benchmark-hook rule; R25-10 sub-rule 2).

### 3.2 Naming: why `trim_current_thread`, not `trim()` or `scavenge()`

The task brief's own examples (`heap.trim()`, `SeferAlloc::trim_current_thread()`)
name both options. `trim_current_thread` is recommended over a bare `trim()`
for the same reason `dbg_trim_current_thread` is already named that way: a
bare `.trim()` on `&SeferAlloc` could misleadingly suggest it trims the
WHOLE process (every heap in the registry), when the actual mechanism
(mirroring `trim_for_recycle`) is inherently single-heap, own-thread-only —
the same single-writer constraint every other `HeapCore` mutation relies on
(no cross-thread heap access without going through the
remote-free-ring/registry machinery this project already has for a
different purpose). A whole-process trim would need to iterate every
registry slot and either (a) trim slots this thread does not own, which is
unsound without additional cross-thread synchronization this proposal does
not attempt to design, or (b) only be meaningfully callable from each
owning thread anyway — making a process-wide `trim()` either unsound or a
thin wrapper that just calls `trim_current_thread()` once per thread with
each thread's own cooperation, which is not simpler than calling
`trim_current_thread()` directly from each thread's own phase-boundary
code. `trim_current_thread` states the actual scope in the name, avoiding
the false promise.

`scavenge` was considered (the task brief's second example name,
`heap.trim()`, and `LargeCacheMode`'s own reserved `#[non_exhaustive]`
"background-scavenger" variant naming) but rejected: "scavenge" in this
codebase's existing vocabulary (`large_cache_mode.rs:14`'s reserved variant)
already connotes an AUTOMATIC background mechanism this project has
declined to build; reusing that word for a manual, caller-driven action
risks exactly the confusion §2 spent effort distinguishing this proposal
from.

### 3.3 What it touches

Per §3.1's body being byte-identical to the existing hook, no new
mechanism is required — only new EXPOSURE of an existing one:

- `HeapCore::trim_for_recycle` (`src/registry/heap_core_ownership.rs:252-265`)
  — already does exactly the right work (flush tcache, drain small pool,
  evict large cache); no changes needed to this function itself.
- `SeferAlloc::dbg_trim_current_thread` (`src/global/sefer_alloc.rs:423-431`)
  — already resolves the calling thread's own heap via `current_heap()`
  and calls `trim_for_recycle` on it, explicitly WITHOUT tearing down TLS
  or recycling the slot. This is already the exact behavior §3.1 wants.
- **The only change this proposal actually requires:** promote the
  existing hook from `#[doc(hidden)]`, unconditionally-`pub` (no feature
  gate beyond the implicit `alloc-global` the whole `sefer_alloc.rs` file
  is under) to a properly-`#[cfg(feature = "alloc-decommit")]`-gated,
  documented, discoverable public method with a name reflecting its real
  API status (`trim_current_thread`, not `dbg_trim_current_thread`) — and
  decide what to do with the OLD `dbg_` name (see §5, migration note).

No change is needed to `maybe_decay_small_pool` / `maybe_decay_large_cache`
themselves — this proposal does not touch the automatic event-driven decay
path at all; it adds a SEPARATE, explicit entry point that happens to reuse
the same underlying drain/evict primitives those paths already call at
thread-exit.

### 3.4 `dbg_force_decay_tick` as a considered, REJECTED alternative starting point

The task brief names `dbg_force_decay_tick`'s "existing production-adjacent
logic" as a possible starting point, since it already does forced
convergence for measurement purposes. This was evaluated and NOT chosen as
the primitive this proposal wraps, for a concrete reason:

`dbg_force_decay_tick` (`src/alloc_core/alloc_core_large_cache.rs:435-447`)
performs exactly ONE decay step (10% of the excess-over-headroom, at the
default decay rate) per call — R29-13 §1.6 documents that reaching a fixed
point with it requires LOOPING the call until the cache stops shrinking
("this gate loops that call until `dbg_large_cache_used()` stops changing
between iterations"). That is the right shape for a MEASUREMENT harness
that wants to characterize the geometric-decay curve step by step, but the
WRONG shape for an application-facing trim API: an application calling
"trim my heap" wants a single call that reaches the floor (or empties
entirely, for the small pool, which has no headroom concept), not N calls
with the caller responsible for detecting the fixed point. `evict_all`
(`alloc_core_large_cache.rs:409-420`, already looping internally via
`while self.evict_one_oldest() {}`) is the correct-shaped primitive — and
it is exactly what `trim_for_recycle` already calls, which is why §3.3
recommends reusing `trim_for_recycle`'s existing composition rather than
building a new one around `dbg_force_decay_tick`.

One narrow use IS worth naming for a future task, not this one: a caller
who wants trim behavior with a HEADROOM FLOOR (i.e. "release excess but
keep some), analogous to what R29-13 measured for headroom>0, rather than
`trim_current_thread`'s all-the-way-to-empty semantics — could reasonably
want a `trim_current_thread_to_headroom()` variant built on
`maybe_decay_large_cache` looped to convergence (the same loop
`dbg_force_decay_tick`'s call sites already perform). This is explicitly
OUT OF SCOPE for the recommendation below (§6) — named here only so a
future task inherits the idea rather than re-deriving it.

---

## 4. Safety and soundness considerations

### 4.1 Single-writer invariant — already upheld, not newly at risk

`trim_current_thread` operates ONLY on the calling thread's OWN heap,
resolved via the exact same `current_heap()` path `alloc`/`dealloc` already
use (`src/global/sefer_alloc.rs:282-291`). The single-writer invariant
every other `HeapCore` mutation already relies on (the CAS-won slot owner
is the sole writer) is unchanged: this proposal adds no new way to reach a
`HeapCore` a thread does not own, and no new `unsafe` beyond what
`dbg_trim_current_thread` already carries (the existing `unsafe {
(*heap).trim_for_recycle() }` block, whose safety comment already states
the exact invariant this proposal relies on unchanged:
"`heap` is non-null and points to a live `HeapCore` in a registry slot
owned by THIS thread").

### 4.2 No new `unsafe` surface

The proposed public method's body is IDENTICAL to the existing
`dbg_trim_current_thread`'s body — same `current_heap()` resolution, same
single `unsafe { (*heap).trim_for_recycle() }` call, same `# Safety`
reasoning. This is a visibility/documentation change, not a new capability;
it does not add a new raw-pointer parameter, does not touch allocator
metadata through a caller-supplied pointer, and is not the "benchmark-only
`dbg_*` hook" shape CLAUDE.md's R25-1 rule targets (that rule is about
hooks whose safety depends on a CALLER-SUPPLIED pointer; this method takes
`&self` and no pointer argument at all — it is a zero-argument state
mutation of the calling thread's OWN heap, the same category as the
already-safe, already-`pub` `dbg_trim_current_thread` and
`SeferAlloc::stats()`).

### 4.3 The real hazard: PERFORMANCE mis-use, not memory-safety

The one genuine risk this proposal introduces is not a soundness hazard
but a footgun: a caller who calls `trim_current_thread()` too often (e.g.
after every single allocation, or on a tight loop boundary rather than a
genuine phase boundary) defeats the ENTIRE point of the small-pool/
large-cache warm-retention mechanisms this project measured and tuned
(R27-3/R27-4/R29-13/R30-6) — every trim forces the NEXT allocation back
onto the cold OS-reservation path, reproducing exactly the "9 decommits per
run" cliff R27-4 measured as the thing the small pool exists to eliminate.
This is a documentation/API-design responsibility, not a code-level one:

- The doc comment (§3.1) explicitly states "NOT intended to be called on
  every allocation" and names the amortization it defeats.
- Naming it `trim_current_thread` (not a terser, more inviting `trim()`)
  and requiring the caller to reason about "the calling thread's own heap"
  keeps the call site's cost model visible rather than implying a cheap,
  process-wide, fire-and-forget operation.
- A future round could add a `#[must_use]`-style cost-acknowledgment
  pattern or a rate-limiting wrapper if misuse turns out to be common in
  practice — not designed here, since no usage evidence exists yet to
  justify the complexity (mirroring R27-5's own "measure before adding
  complexity" discipline, applied to API ergonomics rather than a runtime
  heuristic this time).

### 4.4 Interaction with the automatic decay path — none, by construction

`trim_current_thread` and the existing event-driven decay ticks
(`maybe_decay_small_pool`/`maybe_decay_large_cache`) do not interact: a
trim call drains/evicts unconditionally (ignoring headroom entirely, same
as `trim_for_recycle`'s existing thread-exit behavior), while the automatic
ticks respect `headroom_bytes`/`pool_segments` as their target floor. A
caller who trims and then keeps allocating will simply re-populate the pool
and cache from scratch, subject to the SAME headroom/cap policy as before —
`trim_current_thread` does not change a heap's CONFIG (its `pool_cap` /
`headroom_bytes`), only its current CONTENTS. This mirrors exactly how
`trim_for_recycle` behaves today at thread-exit (the config is a
per-`AllocCore`, set-once-at-materialization field, untouched by any drain
operation — R27-5 §2.3 already established this for `pool_cap` specifically,
citing `src/alloc_core/alloc_core.rs:836-839`).

---

## 5. Migration note — the existing `dbg_trim_current_thread` hook

If this proposal is implemented, the existing `#[doc(hidden)]`
`dbg_trim_current_thread` (used today by `benches/global_alloc.rs`'s
cross-group state reset, per its own doc comment) has three options, to be
decided by the implementing task, not this design:

1. **Keep both** — `dbg_trim_current_thread` stays as the bench-internal
   name (no `#[doc(hidden)]` removal needed, no call-site churn in
   `benches/global_alloc.rs`), and `trim_current_thread` is a NEW public
   method with an identical body. Simplest, zero risk to existing callers,
   at the cost of two names for the same operation living side by side.
2. **`dbg_trim_current_thread` becomes a thin `#[doc(hidden)]` alias**
   calling the new public `trim_current_thread` — avoids body duplication,
   keeps the bench call site unchanged, single source of truth.
3. **Migrate `benches/global_alloc.rs` to call the new public name
   directly and delete `dbg_trim_current_thread`** — cleanest end state,
   but touches an existing, working benchmark file for a rename with no
   behavior change, and loses the `#[doc(hidden)]` "test-only export"
   framing for a name that WAS always test/bench-only until this proposal.

**Recommendation for the implementing task: option 2** (thin alias) —
matches this project's general preference (seen elsewhere in this codebase,
e.g. `HeapCore::dbg_pool_cap`/`AllocCore::dbg_pool_cap` delegation pattern)
for "one real implementation, thin named wrappers at each API layer that
needs to expose it," and avoids unnecessary churn to an existing,
already-tested benchmark file.

---

## 6. Acceptance criteria for a future implementation task

- [ ] **AC1 — behavioral equivalence.** `trim_current_thread()`'s effect on
  a heap's `dbg_pooled_count()`/`dbg_large_cache_used()` (or their
  post-this-round successor accessors) is IDENTICAL to what
  `trim_for_recycle` already produces at thread-exit — verified by a test
  that trims mid-thread-lifetime and asserts the same post-trim state
  `tests/small_segment_pool.rs`'s existing drain tests already assert for
  `dbg_drain_small_pool`.
2. **AC2 — the thread keeps working afterward.** A test allocates, trims,
  and allocates again on the SAME thread, asserting the second allocation
  succeeds and produces correct, readable memory (proving TLS/registry
  slot survive the trim, unlike `trim_for_recycle`'s thread-exit-only
  normal caller).
3. **AC3 — no cross-thread effect.** A test trims thread A's heap and
  asserts thread B's heap (`dbg_pooled_count`, `dbg_large_cache_used`) is
  UNCHANGED — proving the single-writer, own-heap-only scope holds.
4. **AC4 — fallback-heap no-op.** Calling `trim_current_thread()` before
  any allocation has bound a per-thread heap (or after TLS teardown) does
  not panic and does not corrupt the fallback heap's own state — mirrors
  `dbg_trim_current_thread`'s existing documented fallback behavior.
5. **AC5 — a measured burst-idle-burst RSS win.** A gate report
  (`docs/perf/`, following R27-3/R29-13's subprocess-per-arm protocol)
  demonstrating that a burst → `trim_current_thread()` → idle → burst
  sequence reclaims RSS DURING the idle window that an otherwise-identical
  burst → idle → burst sequence (no trim call) does NOT — the direct,
  measured closing of the R29-13-proven gap this design exists to fill.
  This is the load-bearing acceptance criterion: everything else in this
  list is correctness; this one is the actual value proposition.
6. **AC6 — feature gating and clippy/fmt clean** under the same matrix
  this project's other `alloc-decommit`-gated public API already passes
  (`cargo clippy --features production -- -D warnings`, `cargo fmt --check`).

---

## 7. What this document does NOT claim

- **No measurement was performed in this task.** This is a design proposal
  only, per the task's explicit instruction ("Do NOT implement it this
  round — this deliverable is the design doc only"). AC5 above is what a
  future implementation task would need to measure to prove the design's
  value, not something this document asserts already holds.
- **No claim that the mechanism is novel.** `trim_for_recycle` and
  `dbg_trim_current_thread` already exist and are already exercised
  (thread-exit; `benches/global_alloc.rs`'s cross-group reset). This
  proposal's contribution is API SURFACE (promote an existing internal
  mechanism to a documented, stable, discoverable public entry point), not
  a new algorithm.
- **No claim this replaces R27-5's adaptive design, or that R27-5's
  finding is wrong.** R27-5's conclusion (the uniform-pressure workloads
  measured so far do not justify an adaptive growth/token-budget
  subsystem) is unaffected by this proposal. This document's §2 argues the
  two are complementary, not competing: this proposal supplies a manual
  lever usable TODAY, independent of whether an automatic policy is ever
  built.
- **No `src/`/`Cargo.toml`/`tests/`/`benches/` change is made by this
  document.** Everything in §3–§6 is illustrative pseudocode/signatures
  ("SKETCH", matching R27-5's own convention for the same purpose), not
  applied code.
- **No claim about the small-pool's OWN decay having a HEADROOM-style
  floor.** Unlike the large cache (`headroom_bytes`), the small pool's
  drain (`drain_small_pool`) is all-or-nothing (release every pooled
  segment) — `trim_current_thread` inherits that same all-or-nothing
  small-pool behavior via `trim_for_recycle`; §3.4's headroom-floor variant
  idea applies only to the large-cache half of a future extended API, not
  the small pool.

---

## 8. Files changed by this task (Deliverable 3 only)

| file | change |
|---|---|
| `docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md` | this design document (new) |

No `src/`, `Cargo.toml`, `tests/`, or `benches/` file is touched by this
deliverable. (R30-7's OTHER two deliverables — the `Profile` enum and the
deliverable-4 application-shaped gate — are separate `src/`/`tests/`/
`examples/` changes tracked in the same task's commit, not part of this
design document's own scope.)
