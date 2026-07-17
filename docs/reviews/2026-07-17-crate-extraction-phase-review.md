# ZERO-TRUST review — crate-extraction phase (`b8d11f4..0ff8497`, 11 commits, 93 files)

**Reviewer:** fh (delegated), 2026-07-17. Read-only review; no source edited, no commits.
**Scope:** CRATE-P1…P10 (#171–#180): 7 new crates (racy-ptr-cell, ring-mpsc, size-classes,
globalalloc-model, tagged-index-stack, proc-memstat, proc-probe), aligned-vmem 0.2,
malloc-bench publish-prep, the two in-tree SWAPS (registry `free_slots` → tagged-index-stack;
chunk cells → racy-ptr-cell; sidecar reservations → `leak_zeroed_pages`), the loom-suite
migration, and the docs/CI/scripts wiring.

---

## Verdict: **SHIP-WITH-FIXES**

The production (non-loom) code is correct: both concurrency swaps preserve the exact
atomic orderings and invariants of the code they replaced, the new crates' protocols are
sound, and the counterfactual test suites are genuinely non-vacuous. But the phase broke
its own **loom verification gate**: sefer-alloc no longer compiles under
`RUSTFLAGS="--cfg loom"` with `alloc-global` (reproduced locally, E0015). CI's
`loom-misc` job and four `scripts/loom.mjs` entries are red. That must be fixed before
any push (per the repo's own "npm run check / red-CI" rule). Two medium license/docs
gaps also need closing.

Counts: **0 blocker / 1 high / 2 medium / 5 low / 3 nit.**

---

## Findings table (severity-ranked)

| # | Sev | Status | File:Line | Summary |
|---|-----|--------|-----------|---------|
| F1 | **high** | CONFIRMED (reproduced) | `src/registry/bootstrap.rs:413` | `const fn Registry::new()` calls `TaggedIndexStack::new()`, which is **non-const under `--cfg loom`** → sefer lib fails to compile in every loom+`alloc-global` build. CI `loom-misc` step (`ci.yml:563`) and 4 `scripts/loom.mjs` entries are red. |
| F2 | medium | CONFIRMED | `crates/size-classes/`, `crates/proc-probe/` | Both crates declare `license = "MIT OR Apache-2.0"` but ship **no LICENSE-MIT / LICENSE-APACHE files** (the other five new crates have both). |
| F3 | medium | CONFIRMED | `README.md:927` | Loom row still lists the deleted `loom_bootstrap_cas` / `loom_fallback_init` / `loom_free_slots_aba` (removed this phase) plus the long-gone `loom_registry.rs`, and claims "11 models". ARCHITECTURE.md was updated; README was not. |
| F4 | low | CONFIRMED | `crates/racy-ptr-cell/src/lib.rs:78–84` | Seam comment claims "the crate body itself contains no `unsafe {}` blocks at all" — false: three `unsafe { NonNull::new_unchecked(p) }` (lines 231, 274, 339) plus `unsafe impl Send/Sync` (161–163). The sites are sound; the seam *description* is inaccurate. |
| F5 | low | CONFIRMED | `src/registry/heap_overflow.rs:271, 544`; `tests/loom_deferred_large.rs:3` | Stale comment references to deleted files: `tests/loom_overflow_sidecar_cas.rs` cited as live coverage; `src/registry/tagged_ptr.rs` cited as existing. |
| F6 | low | CONFIRMED (behavior delta) | `src/registry/heap_registry.rs` (`RegistryLinks::load_next`) | The old `pop_free_slot` had a release-mode defensive `idx >= MAX_HEAPS → None`; the swap dropped it — a corrupted head now panics (chunk-array OOB in `Registry::slot`) instead of returning `None`. Unreachable by construction; fail-loud is defensible, but the defense-in-depth delta is undocumented. |
| F7 | low | SUSPECTED | `crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:422–465` | Counterfactual B (spin-on-READY livelock) panics after a bounded 8-iteration spin; loom can plausibly schedule that panic while the winner is merely mid-init (not only after the OOM rollback), so the counterfactual is less *specific* than its name claims. Still non-vacuous as a `#[should_panic]`. |
| F8 | low | CONFIRMED (accepted-risk note) | `src/registry/bootstrap.rs:222–345` | `loom_shim` is a ~95-line behavioral duplicate of `RacyPtrCell` that can drift silently. Documented and never on a modeled interleaving — but F1's fix will need a *second* such shim (TaggedIndexStack); consider one shared pattern/location. |
| F9 | nit | CONFIRMED | `crates/proc-memstat/src/lib.rs:97` | Linux backend hardcodes `PAGE_SIZE = 4096` — under-reports 4×/16× on 16k/64k-page kernels. Documented as a rough probe, but `/proc/self/status` `VmRSS:`/`VmSize:` (kB units) would be page-size-independent for free. |
| F10 | nit | CONFIRMED | `crates/globalalloc-model/src/lib.rs:242–257` | `drive`'s doc says it panics on "a null return where memory was not exhausted", but the code asserts non-null unconditionally. Doc/code mismatch only. |
| F11 | nit | CONFIRMED | `.github/workflows/ci.yml` (loom jobs) | `loom_heap_overflow`, `loom_heap_overflow_drain_guard`, `loom_overflow_first_retry`, `loom_remote_ring_drain_guard`, `loom_dirty_*` run only via `scripts/loom.mjs`, not CI (pre-existing gap, not introduced here — but the new CI comments imply they are covered, and the first three are currently *uncompilable* per F1). |

---

## Per-finding detail

### F1 (high) — loom + `alloc-global` no longer compiles: `TaggedIndexStack::new()` is non-const under `--cfg loom`

Reproduced verbatim on the committed tree:

```
$ RUSTFLAGS="--cfg loom" cargo check -p sefer-alloc --lib --features "alloc-global,alloc-xthread"
error[E0015]: cannot call non-const associated function `TaggedIndexStack::<16>::new`
   --> src\registry\bootstrap.rs:413:25
    |
413 |             free_slots: TaggedIndexStack::new(),
```

Mechanism: `RUSTFLAGS="--cfg loom"` is global to the build graph, so the
`tagged-index-stack` dependency compiles in its loom-aliased mode, where `new()` is
deliberately non-`const` (loom atomics have no const constructor —
`crates/tagged-index-stack/src/lib.rs:304`). `Registry::new()` is a `const fn`
(`static REGISTRY: Registry = Registry::new()`), so the call is a hard E0015.

The P3 agent hit and solved this **exact** problem for `RacyPtrCell` — that is the whole
reason `bootstrap.rs`'s `loom_shim` exists (its comment even says "`RUSTFLAGS=--cfg loom`
is global, so the real crate would then be built in its loom-aliased mode"). The P7 agent
did not apply the same treatment to `TaggedIndexStack`.

Blast radius (all currently red):
- CI `loom-misc` job, step `loom_magazine_ring_compose`
  (`cargo test --release --features "alloc-global alloc-xthread" --test loom_magazine_ring_compose`
  under `RUSTFLAGS: "--cfg loom"`, `.github/workflows/ci.yml:563`).
- `scripts/loom.mjs` entries `loom_magazine_ring_compose` (pre-existing) and the three
  entries this phase **added** with `alloc-global,alloc-xthread`
  (`loom_overflow_first_retry`, `loom_heap_overflow`, `loom_heap_overflow_drain_guard`)
  — so `node scripts/loom.mjs` fails.

Production builds, `npm run check` (fmt/clippy/test/iai), miri, and all non-loom CI jobs
are unaffected — which is exactly why the per-task gates missed it.

**Suggested fix (small):** mirror the `RacyPtrCell` solution — a `#[cfg(loom)]`
const-capable, core-atomic `TaggedIndexStack` shim in `bootstrap.rs` (or a shared
`loom_shims` module holding both), with `#[cfg(not(loom))] use tagged_index_stack::…`.
Alternative: give the crate a `const fn new()` in both modes by making the loom aliasing
apply only to `push`/`pop` internals — but the shim is the established in-repo pattern.
Then run `node scripts/loom.mjs` to confirm the whole matrix is green again.

### F2 (medium) — missing LICENSE files in `size-classes` and `proc-probe`

`ls crates/size-classes/` and `ls crates/proc-probe/` show `Cargo.toml README.md src tests`
— no `LICENSE-MIT` / `LICENSE-APACHE`, while `racy-ptr-cell`, `ring-mpsc`,
`globalalloc-model`, `tagged-index-stack`, `proc-memstat` all ship both. `cargo publish`
would succeed (the SPDX field is set) but the packaged crate would carry no license text,
and the repo's own publish-prep convention (per the five siblings) is violated.
**Fix:** copy the two license files into both crates.

### F3 (medium) — README.md loom table stale

`README.md:927` still names `tests/loom_bootstrap_cas.rs`, `loom_fallback_init.rs`,
`loom_free_slots_aba.rs` (all deleted by this phase) and `loom_registry.rs` (deleted in a
prior phase), with the count "11 models". `docs/ARCHITECTURE.md` was correctly rewritten
(verified: its "169 files" claim matches the actual 169 files in `tests/`); README was
missed. **Fix:** update the README testing table to mirror the ARCHITECTURE.md wording
(in-tree models + the three crate real-type suites).

### F4 (low) — racy-ptr-cell seam comment misstates its own unsafe inventory

The tier-1 rationale block (lib.rs:71–84) asserts the crate body "contains no `unsafe {}`
blocks at all". It contains three (`NonNull::new_unchecked` after the `is_ready` proof at
lines 231, 274, 339) and the two `unsafe impl`s (161, 163). Each *site* is individually
justified (the `is_ready` check proves non-null; the Send/Sync impls carry a correct
SAFETY argument mirroring `AtomicPtr`'s), so this is purely a wrong meta-claim — but a
formal seam audit that trusts the header would mis-inventory the crate.
**Fix:** reword the header to enumerate the actual unsafe surface.

### F5 (low) — stale references to deleted files

- `src/registry/heap_overflow.rs:271` and `:544` still point readers at
  `tests/loom_overflow_sidecar_cas.rs` as the live model for the sidecar CAS race; that
  file was deleted (replaced by the racy-ptr-cell crate suite — `tests/heap_overflow_sidecar.rs`
  was correctly updated to say so; these two comments were not).
- `tests/loom_deferred_large.rs:3` cites `src/registry/tagged_ptr.rs`, deleted this phase.

### F6 (low) — dropped release-mode bounds guard in the free_slots swap

Old `pop_free_slot` (pre-swap) checked `idx >= MAX_HEAPS → return None` before
`reg.slot(idx)`. The new `RegistryLinks::load_next`/`store_next`
(`src/registry/heap_registry.rs`) index `reg.slot(index as usize)` with only a comment
("by construction"). A corrupted head word (index up to 0xFFFE vs `MAX_HEAPS = 4096`,
`NUM_CHUNKS = 64`) now takes a safe OOB panic in `Registry::slot`'s chunk-array index
instead of a graceful `None`. Not a memory-safety issue (panic, not UB), and fail-loud on
corruption is arguably superior — but it is an undocumented behavior change relative to
the code it "exactly preserves". **Fix (optional):** either restore a
`debug_assert!`/early-`None` in the adapter or document the deliberate
graceful-degradation → fail-loud change at the adapter.

### F7 (low, suspected) — counterfactual B's panic is not livelock-specific

`ensure_spin_on_ready_broken` panics unconditionally after 8 spin iterations without
READY. Loom's exploration can reach that panic on schedules where the winner holds
`INITIALIZING` but has simply not been scheduled to publish (a bounded-spin timeout, not
the rollback livelock the test names). The `#[should_panic]` contract is still satisfied
and the suite is still non-vacuous — but the counterfactual does not *isolate* the
`!= READY` vs `== INITIALIZING` distinction as sharply as the H-2 rendezvous test does
for the tag reset. **Fix (optional):** gate the panic on having observed the rollback
(`state == UNINIT` seen at least once inside the spin), which pins the failure to the
exact livelock.

### F8–F11 — see table; all are hygiene/documentation grade.

---

## What I verified is correct (coverage for the orchestrator)

**In-tree swaps (traced line-by-line against the pre-images):**
- `heap_registry.rs` `free_slots` swap: the crate's `pop`/`push` reproduce the removed
  inline Treiber loops exactly — head load `Acquire`; pop CAS `Acquire`/`Relaxed`
  (tag preserved); push link-store `Release` before CAS `Release`/`Relaxed` with tag
  bump; link read `Acquire` before the pop CAS. The **H-2 empty-transition tag
  preservation** and the **RAD-1 lazy-link discipline** (link written only inside push)
  are inside the crate and match the removed code verbatim. `NEXT_FREE_TAIL == TAIL`
  is pinned by a `const` assert; the empty→TAIL mapping stays explicit (not
  coincidence-based). `INDEX_BITS = 16` holds `MAX_HEAPS = 4096` with sentinel 0xFFFF
  above the cap; 48-bit tag budget analysis carried over. Kani `pack_proofs` re-bound to
  `TaggedIndex<16>` with the same two properties.
- `bootstrap.rs` chunk-cell swap: `RacyPtrCell` implements the same
  `null → sentinel(1) → real` machine with identical orderings (CAS `Acquire`/`Relaxed`,
  publish `Release`, rollback `Release`, loser spin-`Acquire` **only while
  INITIALIZING** — the anti-livelock rule); OOM-abort policy unchanged in effect; the
  `dbg_rollback_chunk_sentinel_reenterable` hook now drives the shipped cell (not a
  copy); the sidecar path's deliberate non-migration has a sound 3-point rationale
  (different membrane split, return-false-not-re-race OOM contract, ZST-under-miri
  tripping the `align >= 2` guard).
- `leak_zeroed_pages` migration (bootstrap.rs, os.rs): same PAGE alignment as the removed
  `CHUNK_ALIGN`/`DIRECTORY_SIDECAR_ALIGN`, same miri explicit-zeroing, same
  `mem::forget` leak; the all-zero-is-valid-`RegistryChunk` audit is preserved.

**New crates' protocols:**
- **ring-mpsc:** Vyukov reserve (`tail` CAS `AcqRel`) / gate-publish (`Release`) /
  single-consumer drain (gate `Acquire`, stop-at-unpublished, clear `Relaxed`,
  head `Release`); `drain` returns the actual stop `h` (the R2-4 contract);
  full-check with monotone-stale-safe `Acquire` head; pair-publish `packed` Relaxed →
  `base` Release with matching read order; power-of-two CAP pinned at compile time;
  raw tier's layout arithmetic (`SLOTS_OFF`/`SLOT_STRIDE` alignment rounding) checked;
  `# Safety` on `over_raw`/`view_raw`/`slot_at` complete; DirtyRouter's
  at-least-once/bounded-deferral contract honestly documented. ADDITIVE as claimed —
  the seven in-tree ring/dirty loom models were kept (verified in `scripts/loom.mjs`
  and job comments), and the in-tree rings are untouched.
- **racy-ptr-cell / tagged-index-stack:** state machines correct (see swap notes);
  sentinel is provenance-free (`without_provenance_mut`), never dereferenced;
  `align_of >= 2` const guard prevents sentinel collision.
- **size-classes:** two-pointer const merge, monotone-pointer `size2class` derivation,
  and the divisibility-**jump** classifier verified equivalent to the step-by-1 walk
  (next candidate = smallest multiple of `align` above the current block; nothing in
  between can divide; strict-increase ⇒ termination). The compat shim reproduces
  sefer's exact 49/55-class scheme; no test hardcodes a class count (the crate's tests
  compare against an independent reference-scan oracle; proptest covers three schemes).
- **globalalloc-model:** M1–M4 oracles match the three retired copies; the fuzz
  front-end preserves the historical bounds exactly (2 MiB size modulus, `2^0..2^21`
  align — the #130 large-align corridor is still fuzzed); `double_free` wiring matches
  history per consumer (`alloc_core_differential`: true; `heap_differential`: false;
  fuzz target: true).
- **proc-memstat:** Windows `PROCESS_MEMORY_COUNTERS` layout and `K32` binding correct;
  macOS `mach_task_basic_info` layout and COUNT (12 natural_t) correct; error paths never
  read untouched out-params; unknown targets report honest zeros.
- **malloc-bench:** `run_with`/`sweep_with` hook runs pre-barrier per worker; `run`/`sweep`
  forward with a no-op; the smoke test is counterfactual-grade (asserts the hook fired
  exactly 4× with indices {0,1,2,3}); `examples/malloc_macro.rs` duplicate genuinely
  retired (489-line reduction, drives the crate through `run_with`).

**Loom counterfactuals (non-vacuousness):** traced the H-2 tag-reset counterfactual's
rendezvous — seeded tag 1, buggy drain → `(MASK,0)`, refill → `(0,1)` recurs A's exact
stale snapshot, CAS succeeds, panic; the fixed arm uses the **real** `pop` and the head
becomes `(0,2)` ≠ snapshot. The untagged-stack, wrong-pair-publish-order,
drain-no-break, drain-no-clear, and R2-4 return-tail counterfactuals are each a faithful
transcription with exactly one detail flipped and a reachable failing interleaving.
CI keeps `RUSTFLAGS: "--cfg loom"` in the jobs that run the crate suites (so they are
not vacuously compiled-out).

**Conventions / hygiene:**
- Confined-unsafe inventory grep run: every new `#![allow(unsafe_code)]` is a
  single-file seam crate with a single documented reason (racy-ptr-cell, ring-mpsc,
  globalalloc-model, proc-memstat, vmem); `tagged-index-stack`, `size-classes`,
  `proc-probe` are `#![forbid(unsafe_code)]`; no `unsafe` leaked into otherwise-safe
  in-tree files (the range's in-tree unsafe deltas are all inside the pre-existing
  tier-1 seams `bootstrap.rs`/`os.rs`/`heap_registry.rs`).
- **No runnable doctests added** — every new doc fence in `src`/`crates/*/src` is
  ```` ```text ````/```` ```sh ```` (checked opener-by-opener); README fences are not
  compiled by `cargo test --doc`.
- **No unauthorized version bumps:** only the sanctioned aligned-vmem 0.1.0→0.2.0 (with
  the `alloc-lazy-commit` → `lazy-commit` rename kept as an alias); `numa`'s change is a
  dependency-spec update to `0.2`, not a version bump; no `cargo publish` artifacts.
- Fuzz corpus intact (`fuzz/corpus/{global_alloc_ops,heap_core_ops,region_ops}` present,
  no commits in the range touch them). No TODO/FIXME/placeholder added in the diff.
- Tests live in `tests/` for every crate; `mod.rs` files touched (registry/mod.rs) remain
  reexport-only. All 7 new crates carry full publish metadata (description, repository,
  keywords, categories, MSRV); LICENSE gap is F2.
- `docs/ARCHITECTURE.md`'s "169 files" test count matches the actual `tests/*.rs` count.
- P10 is genuinely docs-only (`docs/crate_extraction/P10_DEFERRED_VERDICT.md`).

**Out of scope / not attributed:** the working tree contains uncommitted edits
(`src/alloc_core/alloc_core*.rs`, `audit_rg.txt`) from other in-flight work — excluded
from this review; the pre-existing `scripts/tsan.mjs` stale test name is already tracked
as task #188.
