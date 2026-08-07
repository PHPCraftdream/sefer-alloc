# rust-cc-audit report — `sefer-region`

**Date:** 2026-08-07
**Produced by:** the `/rust-intel` skill's fan-out audit workflow (`audit-project.workflow.js`,
run `wf_6a104c8f-b46`) — 14 agents total: 2 prepare (trigger-table slicer + crate scoper),
10 per-module auditors (one per rust-intel theme module: async×2 slices, concurrency,
data/types, security, unsafe/FFI, drop/RAII, deps/macros, lifetimes/API, testing,
semantics/conformance), 1 synthesis. ~990k subagent tokens, 131 tool calls, ~11 min.
**Audited tree:** `main` @ `aa24f84` (clean).
**Task filing:** every actionable finding below is filed in the session TaskList —
MEDIUM §B17/§F2/§B1b → task #687 (updated), §F1 → #688 (updated), §B26+§B7 → #690,
§C1 → #691, §D1 catch_unwind → #692, both §D3 → #693; INFO §D1a drain-order → #694,
§F2 Debug-escape → #695, §D1 probe/redundant-tests → #696, §B26 with_capacity → folded
into #690.

---

**Scope:** D:\dev\rust\sefer-alloc\crates\region
**Pinned versions:** sefer-region v0.1.0 (edition 2021, `#![forbid(unsafe_code)]`); runtime dep: slotmap 1.1.1 (crates.io, checksum in committed root Cargo.lock); dev-only: bench-scale-tool 0.1.0, captrack 0.1.1 (feature `telemetry`)
**Found:** 0 critical, 0 high, 7 medium, 5 info

---

## CRITICAL

(none)

## HIGH

(none)

## MEDIUM

### [§B17] src/sync_region.rs:64 — one-shot convenience methods deadlock under a held guard from the same SyncRegion, undocumented

`pub fn read(&self) -> RwLockReadGuard<'_, Region<T>>` (and `write()` at :72) coexist with one-shot self-locking conveniences (`pub fn len(&self) -> usize { self.read().len() }`, `pub fn insert(&self, value: T) -> Handle<T> { self.write().insert(value) }`) on the SAME `std::sync::RwLock`, and the docs (lines 11–19, "correct under any interleaving") steer callers between the two surfaces without warning that mixing them on one thread is a reentrant acquisition std documents as "might panic or deadlock": a write-path one-shot under a held read guard self-deadlocks on every platform; a read-path one-shot deadlocks whenever a writer is queued (std's RwLock priority policy is unspecified — passes on a reader-preferring dev machine, hangs in prod). §B17's required read-reentrance-freedom documentation is absent. **Also flagged by §F2** (semantics-and-conformance, sync_region.rs:16 — the same reentrancy omission; matches already-pending task #687) **and by §B1b** (lifetimes-and-api, this same line — secondary concern: exposing std's CONCRETE `RwLockReadGuard`/`WriteGuard` types freezes `std::sync::RwLock` itself into the published semver contract; migrating to parking_lot later is semver-major).

**Fix (doc-only):** add a "Deadlock hazard" section to SyncRegion's type-level doc (alongside the existing "Poisoning policy" section) and to the one-shot methods: never call a one-shot convenience method, or re-entrant `read()`/`write()`, while a guard from the same SyncRegion is held on the current thread — drop the guard first or do all work through the single guard; cite std's recursive-acquisition warning. For the §B1b aspect: either add one doc line stating the std-RwLock guard types are a deliberate stable commitment, or (before first publish) wrap them in opaque newtype guards with `Deref<Target = Region<T>>`.

### [§B26] src/region.rs:130 — `reserve()` panics in debug but silently wraps to a no-op in release (also flagged by §B7, unsafe-and-ffi)

`pub fn reserve(&mut self, additional: usize) { self.inner.reserve(additional); }` — its own doc admits: "in a release build (overflow-checks off), an `additional` argument near `usize::MAX` may silently wrap in the underlying `slotmap` arithmetic (`len + additional`) and result in a no-op rather than a panic". Exactly §B26's debug-vs-release divergence at a public library API: the `# Panics` contract is profile-dependent, and a caller relying on reserve-then-insert-without-realloc gets no signal (the §B7 release-wrap-defeats-the-clamp shape). Currently handled by documentation only.

**Fix:** guard before delegating — `self.inner.len().checked_add(additional).expect("Region::reserve: capacity overflow");` (or a saturating clamp with an explicit panic/return) so debug and release agree and the `# Panics` section becomes unconditionally true; then simplify the doc caveat away. Apply the same policy to `with_capacity` (see INFO below) in the same pass.

### [§C1] src/handle.rs:16 — `Handle<T>` lacks `#[repr(transparent)]` while documenting and compile-time-asserting exact layout

`pub struct Handle<T> { pub(crate) key: slotmap::DefaultKey, _ty: PhantomData<fn() -> T> }` has no `#[repr(transparent)]`, yet tests/handle_static_asserts.rs:104,110 compile-time-assert `size_of::<Handle<u8>>() == 8` and `size_of::<Option<Handle<u8>>>() == 8`, and the docs claim "exactly 8 bytes... no padding" plus niche optimization. §C1 verbatim: without `#[repr(transparent)]` the layout is `#[repr(Rust)]` with NO guarantee — the asserts turn any future rustc layout change into a toolchain-dependent compile break of a published crate rather than a guaranteed invariant.

**Fix:** add `#[repr(transparent)]` (legal: `PhantomData<fn() -> T>` is a 1-aligned ZST, DefaultKey the single non-ZST field), so the 8-byte and niche assertions rest on DefaultKey's own layout by guarantee. Land BEFORE first crates.io publish (task #656) — invisible to consumers now, pins the documented layout contract.

### [§D1] tests/coverage_gaps.rs:451 — catch_unwind asserts only `is_err()`; any panic greens the overflow-contract test

`assert!(result.is_err(), "reserve(usize::MAX / 2) should panic")` — the payload is never inspected. Same hazard class as §D1's BANNED `#[should_panic]` without `expected`: ANY panic (a future slotmap internal assert, or debug's "attempt to add with overflow" from the very arithmetic the doc says wraps in release) passes as proof of the specific capacity-overflow contract this test exists to pin (the contract corrected in task #669).

**Fix:** downcast the `catch_unwind` Err payload to `&str`/`String` and assert it contains a specific substring ("capacity overflow" — and in debug, alternatively "attempt to add with overflow"), so an unrelated panic can't green the test.

### [§D3] tests/smoke.rs:157 — the headline "correct under any interleaving" claim is tested only sequentially

src/sync_region.rs:11 claims correctness "under any interleaving because every mutation serialises through the lock", but the only threaded tests spawn one thread and `join()` it before any assertion (smoke.rs:189-197, clear_partial_under_panic.rs:166-175). §D3 REQUIRED: for a thread-safety claim, the claim defines the test — a single-threaded green suite is silent on it. No test ever has two threads concurrently inside the RwLock-wrapped API; poison recovery under real contention is untested. Pending task #673 already acknowledges the missing contended measurement.

**Fix:** add one multi-thread stress test — N threads concurrently insert/remove/get_cloned/contains on a shared `Arc<SyncRegion<T>>` (yield-heavy schedule), then assert final `len()`/drop-count accounting; keep it fast per repo speed rules (small N, few iterations).

### [§D3] tests/coverage_gaps.rs:440 — "profile-independent" panic claim verified only in debug; no release-profile test run exists

The test comment claims Region::reserve "panics for genuine capacity-overflow arguments in both debug and release builds (profile-independent)", but CI runs only `cargo test -p sefer-region` in debug (ci.yml:708); no `cargo test --release -p sefer-region` exists anywhere. §D3 BANNED: relying on a test-suite pass as evidence about overflow behavior of the release binary — and this crate is exactly in the affected class: src/region.rs:126-129 itself documents that the arithmetic wraps silently in release but panics in debug.

**Fix:** add a `cargo test --release -p sefer-region` step to the test-workspace CI job (cheap: the suite is tiny), or weaken the test comment to state only the debug-verified behavior.

### [§F1] src/sync_region.rs:23 — poisoning-policy doc misstates std's RwLock semantics

"A panic while a guard is held poisons the `RwLock`." — diverges from the external reference (std docs): an RwLock is poisoned ONLY by a panic while locked exclusively (write mode); a panicking reader never poisons it. The recovery code itself (`PoisonError::into_inner` in `read()`/`write()`) is correct either way.

**Fix:** reword to "A panic while the WRITE guard is held poisons the RwLock (a panicking reader does not poison — std documents write-mode-only poisoning)". Matches already-pending task #688.

## INFO

### [§B26] src/region.rs:84 — `with_capacity` shares the reserve() release-wrap class, doc-mitigated only

Doc: "At the extreme (e.g. `capacity` near `usize::MAX`), the underlying `slotmap` arithmetic may wrap in a release build and result in a far smaller capacity than requested" — the wrap yields a silently smaller capacity in release while debug would panic. Currently mitigated by an honest caveat plus a "use capacity() to verify" escape hatch.

**Fix:** either add the same checked-add-style guard, or if the doc-only stance is deliberate, keep it — but align `with_capacity` and `reserve` on one policy in the same pass as the reserve() fix above.

### [§D1] tests/captrack_probe.rs:80 — the only executable check of reserve's churn-reuse claim lives in an `#[ignore]`d probe

`assert!(cap_after_refill <= cap_after_remove, ...)` sits inside `#[ignore = "manual capacity-telemetry probe..."]` (line 39). The `#[ignore]` is legitimately documented, but it buries the only executable validation of src/region.rs:117-120's claim (churn re-inserts "reuse existing capacity and does not grow unboundedly") in a test CI never runs.

**Fix:** lift the churn-refill capacity assertion (and the `with_capacity >= 1000` one at :100) into a normal non-ignored test in coverage_gaps.rs, leaving the probe purely observational.

### [§D1] tests/handle_static_asserts.rs:64 — runtime Send/Sync `thread::spawn` tests duplicate the const assertions that are the real guard

`std::thread::spawn(move || { let _ = h; })` with the comment "This must compile" — the two runtime tests assert nothing at runtime beyond absence of panic; their real check is compilation, which the const `assert_send_sync` at :47-:60 already pins more precisely. Structurally unable to fail for any reason the consts wouldn't already catch.

**Fix:** delete the two runtime thread::spawn tests (keep the const assertions), or fold them into a comment on the consts; `handle_layout_matches_expectations` (:113) self-documents as a visibility duplicate and may stay.

### [§D1a] tests/clear_partial_under_panic.rs:80 — partial-clear test pins slotmap's unspecified drain order (false-red hazard)

`assert_eq!(drop_count..., 3, "exactly 3 values should have been dropped (ids 0,1,2)")` plus per-handle survivor asserts at :88-105 — the oracle is slotmap's UNSPECIFIED clear/drain visitation order (the crate's own iter doc says "order is unspecified", and `slotmap = "1"` floats across minors). A slotmap 1.x drain-order change turns this into a false red on the crate's still-correct order-free contract.

**Fix:** assert order-agnostic postconditions instead — `drop_count + len() == 5`, survivor SET is the complement of the dropped set (via `iter()`), region reusable — or add a comment explicitly accepting the slotmap-order dependence as a deliberate pin.

### [§F2] src/handle.rs:55 — Debug impl renders the full DefaultKey vs README's absolute "never escape" claim

`f.debug_struct("Handle").field("key", &self.key)` renders index+version into the output string, and the crate's own test (tests/smoke.rs:30-51 `slot_index`) parses slot identity back out of it — vs README.md:13 "raw `DefaultKey`s never escape the crate boundary". The type-level guarantee holds (no DefaultKey VALUE is obtainable or usable against a Region), but the prose is stronger than what the code delivers.

**Fix:** qualify the README/lib.rs sentence: "raw DefaultKeys never escape as values through the API (Debug output renders the underlying key for diagnostics only — it cannot be turned back into a usable handle through this crate)."

---

## Post-flight summary

Aggregated 🔴 inventory across all 10 agents — no 🔴 item has a violating occurrence anywhere in the crate:

- **§B21 tokio::spawn with dropped JoinHandle** — whole crate: N/A, zero `tokio::spawn` (no async runtime dependency; slotmap is the sole runtime dep).
- **§B21 (std variant) thread::spawn handle dropped/detached** — 4 sites, all justified/clean: tests/handle_static_asserts.rs:70 and :92 (`.join().expect(...)` at :74-75/:96-97); tests/clear_partial_under_panic.rs:166 (joined at :175, `is_ok()` asserted, panic contained via catch_unwind); tests/smoke.rs:189 (joined at :197, `is_err()` asserted — intentional poisoning-panic thread).
- **§B22 impl Drop doing async work** — tests/clear_partial_under_panic.rs:32 and tests/coverage_gaps.rs:22: N/A, both test-only sync DropCounters (AtomicUsize increment; the first adds an intentional `thread::panicking()`-guarded test bomb); src/ has zero `impl Drop`.
- **§B15b Pin::new_unchecked** — whole crate: N/A, zero occurrences of Pin/new_unchecked/poll/manual Future (confirmed by two agents).
- **§B13 Relaxed-publish data race** — whole crate: pattern absent, zero `Relaxed` hits; all atomics inventoried are SeqCst standalone test drop-counters (clear_partial_under_panic.rs:34 + loads at :81,:120,:125,:179,:226,:234; coverage_gaps.rs:24 + loads at :39,:48,:75,:82,:107,:119,:128,:137,:163,:172,:194,:201) publishing no other memory.
- **§B14 unbounded channel / FuturesUnordered / unbounded admission** — N/A: no channels/futures/JoinSet anywhere; the keyed stores (region.rs:63, sync_region.rs:39) use internally generated slotmap keys (not request-derived), with removal/clear/capacity APIs and documented high-water-mark permanence (region.rs:102-112, 170-182).
- **§B12 any cryptographic operation** — whole crate: N/A, zero crypto surface.
- **§B24 `==` on secret material / distinguishable decrypt-failure errors** — handle.rs:44: N/A, compares a non-secret slotmap::DefaultKey (explicitly excluded by module calibration); no decrypt/verify path or trust boundary exists.
- **§B5 unsafe / transmute / zeroed / uninitialized** — N/A: `#![forbid(unsafe_code)]` at src/lib.rs:55, zero unsafe tokens crate-wide (src, tests, benches), grep-verified.
- **§B18 manual `unsafe impl Send/Sync`** — handle.rs:16, sync_region.rs:39: N/A, none exist — Send/Sync via auto-traits (`PhantomData<fn() -> T>`, std RwLock), pinned by compile-time asserts (tests/handle_static_asserts.rs:47-60).
- **§B18a variance / PhantomData on raw-pointer wrapper** — handle.rs:20: justified/not in scope — no raw pointer; covariant `PhantomData<fn() -> T>` deliberate, documented (handle.rs:11-13), regression-tested.
- **§B25 extern "C" / Box::from_raw / from_raw_parts** — crate-wide: N/A, no FFI surface at all.
- **§B25a FFI across threads without cited thread-safety contract** — sync_region.rs:3: N/A, no C library wrapped; only cross-thread surface is std's own RwLock.
- **§A1 unverified/unnamed dependency (slopsquatting)** — all three justified: Cargo.toml:20 slotmap = "1" (well-known, user-sanctioned, resolves to 1.1.1 with checksum in committed root Cargo.lock); Cargo.toml:25 bench-scale-tool = "0.1" (dev-only, user-named via TaskList #662/#663, registry+checksum pinned, v0.1.0); Cargo.toml:31 captrack = "0.1" (dev-only, user-named via #663, author's own repo, registry+checksum pinned, v0.1.1).
- **§A1 unpinned [patch]/git source** — crate + workspace root manifests: N/A, none.
- **§A1 network in build.rs** — N/A, crate has no build.rs.
- **§C1 blanket impl in public API** — src crate-wide: N/A, zero — all 11 generic impls are inherent or std-trait impls on the crate's own types; no pub trait exists, so no sealing decision is owed; Handle's hand-written unconditional Clone/Copy/PartialEq/Eq/Hash/Debug (handle.rs:36-57) are the module's CORRECT pattern.
- **§F1/§F2 divergence affecting wire format, security guarantee, or persisted data** — whole crate: no occurrences — no wire format/persistence/security guarantee; both doc divergences found (sync_region.rs:23, sync_region.rs:16) are concurrency-documentation defects → 🟡 tier per the module's conditional rule (reported above).
- **§F3 leaked/unclosed boundary resource holdable by an untrusted peer** — whole crate: N/A — only acquired resources are std RwLock guards, RAII-scoped and released on every path including panic (poison then recovered).

Modules with no 🔴 items defined (empty red inventories by construction): data-and-types, drop-and-raii (§B4/§B4a), testing (§D1–§D5/§E6).
