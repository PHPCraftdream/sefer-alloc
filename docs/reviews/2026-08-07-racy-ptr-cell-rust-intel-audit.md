# rust-cc-audit report — `racy-ptr-cell`

**Date:** 2026-08-07
**Produced by:** the `/rust-intel` skill's fan-out audit workflow (`audit-project.workflow.js`,
run `wf_12cd2580-8bc`) — 14 agents total. ~997k subagent tokens, 82 tool calls.
**Audited tree:** `main` @ current HEAD, `crates/racy-ptr-cell`.

---

**Scope:** D:\dev\rust\sefer-alloc\crates\racy-ptr-cell
**Pinned versions:** loom = "0.7" (the crate's only non-std dependency, `[target.'cfg(loom)'.dependencies]`-gated; also pinned at workspace root Cargo.toml:949 and crates/tagged-index-stack/Cargo.toml:28 — no `[workspace.dependencies]` table exists)
**Found:** 0 critical, 1 high, 5 medium, 7 info

---

## CRITICAL

none

## HIGH

### [§F2] crates/racy-ptr-cell/README.md:37 — published loom-proof claim is silently false for the Release-publish rule

> "Both rules are pinned by executable loom proofs that run against the real `RacyPtrCell` type ... including `#[should_panic]` counterfactuals that fail without the correct code"

The Release-publish rule is NOT pinned by any real-type test: every real-type happens-before assertion reads `init_marker` AFTER `join()` (tests/loom_racy_ptr_cell.rs:136, :185), which the file's own comment (:448-450) says "synchronises, hiding the bug"; the counterfactual (:456-498) runs on a hand-transcribed shadow AtomicPtr, never on `RacyPtrCell`. Changing lib.rs:329 Release→Relaxed leaves the whole suite green, so the published verification claim (repeated in Cargo.toml description line 7 and the test module doc's "property 4", loom_racy_ptr_cell.rs:22-23) is silently false for a data-race-relevant ordering — the §D1a invalid-oracle shape elevated to a documented-guarantee divergence because the README/Cargo description sell this proof as the crate's guarantee surface.

**Fix:** Move the `init_marker` assertion INSIDE the loser thread, before any join — have the `run` closure in `real_exactly_once_two_threads`/`three_threads` load `init_marker` immediately after `get_or_try_init` returns and assert 0xDEAD_BEEF there (exactly what `ensure_relaxed_publish_broken_and_check` already does for the shadow model). With Release this passes; with Relaxed loom reports the causality violation, making the README claim true. Alternatively soften the README/Cargo wording to say the counterfactuals are transcribed models — but fixing the oracle is strictly better and ~5 lines.

## MEDIUM

### [§B5] crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:430 — provenance-losing int→ptr round-trip, then deallocated through

> `reclaim_payload(core::ptr::NonNull::new(addr as *mut Payload).unwrap())`

The test collects pointers as `p.as_ptr().addr()` (lines 370/387) — `.addr()` explicitly discards provenance — then reconstructs a pointer via a bare `addr as *mut Payload` int→ptr cast and DEALLOCATES through it (`Box::from_raw` in `reclaim_payload`). This is exactly the §B5-banned `usize as *const T` round-trip: under strict provenance the reconstructed pointer has no valid provenance, so freeing through it is UB in the model miri/strict-provenance lints check — and the source crate itself is otherwise scrupulously strict-provenance-clean (`without_provenance_mut` sentinel, `addr()`-only comparisons), so the test undercuts the crate's own discipline.

**Fix:** Store the published pointers themselves (`Vec<*mut Payload>` or `NonNull`, using `addr()` only for the assert comparisons), or if an integer detour is genuinely needed, use `p.as_ptr().expose_provenance()` when collecting and `core::ptr::with_exposed_provenance_mut::<Payload>(addr)` when reclaiming, with a `// SAFETY:` naming the exposure.

### [§B19] crates/racy-ptr-cell/src/lib.rs:317 — INITIALIZING sentinel held across the caller-supplied init closure with no unwind guard

> `Ok(_) => { ... match init() { Some(ptr) => { ... self.ptr.store(raw, Ordering::Release); } None => { self.ptr.store(core::ptr::null_mut(), Ordering::Release); ... } } }`

Rollback runs ONLY on the `None` return path, so if `init` unwinds (panic in caller code, or the `debug_assert!` at :320 firing in a debug build) the sentinel is stuck forever: every concurrent loser busy-spins at 100% CPU indefinitely and every future `get_or_try_init`/`get` caller sees permanent INITIALIZING — a silent whole-process livelock, strictly worse than OnceLock's poison (which at least panics loudly). This is §B19's take-without-restore shape, and the crate's otherwise-exemplary anti-livelock documentation (OOM rollback, `== INITIALIZING` spin rule, reentrancy contract at :283-286) never mentions the panic path; the loom suite cannot find it either (loom does not model unwinding out of the init closure).

**Fix:** Add an RAII rollback guard: before calling `init`, construct a guard whose `Drop` stores null with `Release`; defuse it on both the successful publish and the explicit OOM-rollback paths. If unwinding is instead declared out-of-contract (panic=abort / no_std targets), say so explicitly in `get_or_try_init`'s contract docs and the crate-level docs ("init must not unwind — an unwinding init wedges the cell in INITIALIZING forever"). The guard is the stronger option for a publish-bound general-purpose crate.

### [§B26] crates/racy-ptr-cell/src/lib.rs:320 — sentinel-collision invariant guarded only in debug builds (also flagged by §F2 documented-guarantee divergence and §B13 check-exists-only-in-debug)

> `debug_assert!(Self::is_ready(raw), "RacyPtrCell: init returned the null/sentinel address"); ... self.ptr.store(raw, Ordering::Release);`

In release the check is compiled out: a SAFE init closure can return `NonNull` at address 1 (constructible via `NonNull::new(ptr::without_provenance_mut(1)).unwrap()`), the winner publishes the sentinel address as "READY", and every reader classifies it as INITIALIZING — all current losers and future callers spin forever with no diagnostic, and the unconditional documented guarantee "The returned pointer is never null and never the sentinel" (lib.rs:280-281) is violated from 100% safe code. §B26's exact debug-panic vs release-silent divergence: `NonNull` rules out null but not the sentinel address, and the check is two integer compares on a once-per-cell cold path — cheap check, critical failure, so it fails the "reserve `debug_assert!` for expensive checks whose failure is non-critical" calibration on both axes.

**Fix:** Promote to a release-active `assert!` on the publish path (cost negligible on the one-shot init path), or treat a sentinel-address init result as an error (roll back to null, return `None`), or downgrade the doc to an explicit init-closure precondition. The assert is the cheaper and more honest option.

### [§D1a] crates/racy-ptr-cell/tests/cell_unit.rs:1 — `dbg_rollback_reenterable`'s happy-path contract is non-evidence within the published crate

> sole in-crate call: `let _ = cp.dbg_rollback_reenterable();` (tests/loom_racy_ptr_cell.rs:355)

The crate's own suite never asserts the probe's happy path (`Some(true)` + cell restored to UNINIT): the only in-crate call discards the result; the only assertion of `Some(true)` lives in the PARENT repo (tests/regression_bootstrap_oom_sentinel_rollback.rs:98-102), which does not ship with the standalone published crate (release.yml tags `racy-ptr-cell-v*`). Counterfactual: mutate `dbg_rollback_reenterable` to `return None` unconditionally — every test in crates/racy-ptr-cell stays green.

**Fix:** Add a unit test in cell_unit.rs: on a fresh cell assert `dbg_rollback_reenterable() == Some(true)`, then `!cell.dbg_is_ready()` and `cell.get().is_none()` (restore postcondition), then a subsequent `get_or_try_init` succeeds; and one on a READY cell asserting `None` (the not-applicable arm).

### [§D1a] crates/racy-ptr-cell/src/lib.rs:192 — soundness-load-bearing `align_of >= 2` guard's documented panic has zero test coverage

> `assert!(core::mem::align_of::<T>() >= 2, "RacyPtrCell<T> requires align_of::<T>() >= 2 ...")`

The align>=2 sentinel-collision guard is soundness/liveness-load-bearing (an align-1 `T` could publish a real pointer at address 1, which readers would misread as INITIALIZING and spin on forever), and its documented panic contract (`# Panics`, lib.rs:175-185) is untested — deleting the assert or flipping the condition passes the entire suite. §D1a REQUIRED: negative controls where cheap; this one is a three-line test.

**Fix:** Add a `#[should_panic(expected = "align_of::<T>() >= 2")]` test constructing the runtime path with an align-1 payload, e.g. `let _ = RacyPtrCell::<u8>::default();` (the doc itself names `default()` as the runtime-panic route). The const-eval arm is untestable without a compile_fail doctest, which CLAUDE.md bans — the runtime arm covers the same predicate.

## INFO

### [§A3] crates/racy-ptr-cell/src/lib.rs:431 — `#[doc(hidden)] pub` state-mutating probe with contradictory API posture on a publish-bound crate

> `#[doc(hidden)] ... pub fn dbg_rollback_reenterable(&self) -> Option<bool>` — doc says "Exists so a consumer's test can drive the rollback on a REAL, LIVE cell"

On a published crate, `#[doc(hidden)] pub` items remain callable semver surface; this probe is simultaneously advertised for external consumers' tests and disclaimed as "not part of the value contract" (the CLAUDE.md doc-hidden sanction covers in-repo test forwarders, not consumer-facing hooks).

**Fix:** Pick one posture before first publish: (a) promote to a documented pub API with an explicit stability note, or (b) gate behind a non-default feature (e.g. `internals`/`test-probe`) so it is opt-in and visibly semver-exempt; state the chosen policy in the README. Same decision applies to `dbg_is_ready` (lib.rs:388).

### [§B5] crates/racy-ptr-cell/src/lib.rs:252 — invariant named but `// SAFETY:` tag missing

> `// \`is_ready\` proved \`p\` non-null.` `Some(unsafe { NonNull::new_unchecked(p) })`

The invariant IS named, but without the `// SAFETY:` tag the crate's own header claim ("Every unsafe fn / unsafe impl carries a # Safety / // SAFETY: justification", lib.rs:85-86) and grep-based SAFETY audits miss this site; the sibling sites at lib.rs:298 and :368 use the proper tag.

**Fix:** `// SAFETY: is_ready(p) just proved p is non-null (neither null nor the sentinel).`

### [§B5] crates/racy-ptr-cell/tests/cell_unit.rs:58 — untagged unsafe `Box::from_raw` reclaim sites (also :77)

> `// Reclaim the leak.` `unsafe { drop(Box::from_raw(p1.as_ptr())) };`

Two unsafe reclaim sites carry no `// SAFETY:` tag (line 57 says only "Reclaim the leak."; line 77 has none) — §B5 bans any untagged unsafe block; the deref sites at :52/:76 in the same file ARE tagged. Ownership itself is sound (pairs with `leak()`'s `Box::leak`, same binary/allocator, dropped once).

**Fix:** Tag both: `// SAFETY: p came from leak()'s Box::leak, still live, reclaimed exactly once here.`

### [§B5] crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:141 — six untagged unsafe blocks in the loom harness (also :185, :187, :234, :309, :530)

> `unsafe { reclaim_payload(r1) };`

The `reclaim_payload` call sites at 141/187/234/309, the raw marker deref at 185 (its twin at 134-136 IS tagged), and the `Box::from_raw` at 530 have no `// SAFETY:` comment — §B5 does not exempt test code; the same file shows the compliant pattern at 426-431, 471-472, 485-488.

**Fix:** Add one-line `// SAFETY:` comments (e.g. "// SAFETY: r1 is the leaked make_payload box, reclaimed exactly once after all threads joined" — the invariant `reclaim_payload`'s own doc already states).

### [§C10] crates/racy-ptr-cell/Cargo.toml:28 — shared `loom` version req duplicated across three manifests with no workspace inheritance

> `loom = "0.7"` (also pinned independently at workspace Cargo.toml:949 and crates/tagged-index-stack/Cargo.toml:28; no `[workspace.dependencies]` table exists)

§C10 REQUIRED: declare shared dependencies once in `[workspace.dependencies]` and inherit with `workspace = true`. Today all three pins are identical "0.7", but this is the exact drift-prone shape behind §C10's "same dependency at drifting versions → multiple linked copies" hazard.

**Fix:** Add `loom = "0.7"` to a root `[workspace.dependencies]` table and switch the three member pins to `loom = { workspace = true }` under their `[target.'cfg(loom)'.dependencies]` sections (no version bump — same "0.7" req, per the no-version-bump CLAUDE.md rule).

### [§F2] crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:550 — stale parent-crate identifier + counterfactual-B shape fidelity gap

> `// ... the outer \`loop\` faithfully mirrors the real \`heap_ptr\` retry structure.`

Stale parent-crate identifier (`heap_ptr`) in a crate positioned for standalone crates.io publication; additionally counterfactual B models an AtomicU8 3-state enum, not the AtomicPtr-with-sentinel shape the module doc (:39) claims is "the exact shape RacyPtrCell implements, with the ONE ordering/condition flipped" — two flips of shape, weakening the fidelity claim.

**Fix:** Reword the comment to reference `RacyPtrCell::get_or_try_init`'s retry loop, and either rebuild counterfactual B over an AtomicPtr with the null/sentinel encoding or soften the "exact shape" wording in the module doc.

### [§F2] docs/reviews/2026-08-06-racy-ptr-cell-publish-readiness-review.md:382 — review doc describes an already-fixed bug as live (docs-vs-code disagreement; reported, not adjudicated)

> "[MEDIUM] dbg_rollback_reenterable can clobber a concurrent winner's sentinel ... **This store is unconditional.**"

The untracked review describes the step-4 clobber bug as live, but the shipped code already contains the review's suggested fix 1+2 (lib.rs:471-474 gates the restore on `postcondition_holds`; the doc parenthetical was rewritten to "POINT-IN-TIME check, not mutual exclusion", lib.rs:420-428) plus a new real-type loom regression test (`real_probe_rollback_does_not_clobber_concurrent_winner`, loom_racy_ptr_cell.rs:343) — the review is a pre-fix snapshot, not a live defect.

**Fix:** No code change needed. Optionally append a one-line resolution note to the review doc (per the repo's append-only-corrections convention) so a future reader does not re-open §5.1 as an open bug. Review fix 3 (feature-gating the dbg_* hooks) was explicitly optional and not taken; the hooks take no raw pointer, so the R25-1 rule does not apply.

---

## Post-flight summary

**🔴 §F2 documented-guarantee divergence:**
- crates/racy-ptr-cell/README.md:37-44 — VIOLATED: claim false for rule 1; no real-type test fails if lib.rs:329 becomes Relaxed (post-join marker reads at tests/loom_racy_ptr_cell.rs:136/:185 are join-synchronized; counterfactual is a shadow model)
- crates/racy-ptr-cell/Cargo.toml:7 — VIOLATED: package description repeats the same proof claim, inherits the same vacuous-oracle gap
- crates/racy-ptr-cell/tests/loom_racy_ptr_cell.rs:22-23 — VIOLATED: "property 4: Happens-before (Release/Acquire pair)" asserted only after `join()`, which the file's own comment (:448-450) identifies as hiding a Relaxed publish
- crates/racy-ptr-cell/src/lib.rs:329 — CONFORMS: `store(raw, Ordering::Release)` present (divergence is in the proof claim, not runtime behavior)
- crates/racy-ptr-cell/src/lib.rs:353-374 — CONFORMS: loser spin exits on null and real pointer; genuinely pinned by `real_survives_oom_rollback_two_threads`
- crates/racy-ptr-cell/src/lib.rs:192-196, :213-215 — CONFORMS: align>=2 asserted in both `new()` variants; const-evaluated in the documented static usage
- crates/racy-ptr-cell/src/lib.rs:87-120, :137-145 — CONFORMS: no_std / allocation-free / no std sync / no parking / never reads through T all hold

**🔴 §B13 Relaxed-publish data race:**
- src/lib.rs:329 — COMPLIANT: Release publish store, commented as load-bearing; pairs with Acquire loads at :249, :293, :354
- src/lib.rs:340 — COMPLIANT: Release store of null on OOM rollback
- src/lib.rs:311 — JUSTIFIED: failed-CAS Relaxed; returned value discarded, loser re-loads with Acquire at :354
- src/lib.rs:439, :455 — JUSTIFIED: probe CAS-failure orderings used for control flow only; probe stores (:445, :474) are Release
- tests/loom_racy_ptr_cell.rs:474 — JUSTIFIED: deliberate broken-protocol Relaxed publish inside `#[should_panic(expected = "Causality violation")]` counterfactual (non-vacuousness proof)
- tests/cell_unit.rs:36,43,49; tests/loom_racy_ptr_cell.rs:113,131,157,170,182,204,214,229,265,301,366,383,397,578 (+ AcqRel gates :260,:572; post-join loads :136,:185) — N/A: standalone counters / gates publishing no other memory; post-join loads synchronized by `join`

**🔴 §B5 unsafe hygiene (seam lift / unsafe blocks / transmute):**
- src/lib.rs:87 seam lift — JUSTIFIED: sanctioned single-file seam crate (CLAUDE.md exception 3), documented header, matches the repo's self-verifying grep
- src/lib.rs:252 — VIOLATED (format only): invariant named but `// SAFETY:` tag missing (finding); soundness fine
- src/lib.rs:298, :368 — JUSTIFIED: tagged, invariants proven by `is_ready`
- tests/cell_unit.rs:52, :76 — JUSTIFIED: tagged derefs
- tests/cell_unit.rs:58, :77 — VIOLATED (untagged; finding); ownership pairing with `Box::leak` justified
- tests/loom_racy_ptr_cell.rs:92-93 — JUSTIFIED: `unsafe fn reclaim_payload` with SAFETY doc (89-91)
- tests/loom_racy_ptr_cell.rs:136, :472, :488 — JUSTIFIED: tagged
- tests/loom_racy_ptr_cell.rs:141, :185, :187, :234, :309, :530 — VIOLATED: no `// SAFETY:` tag (finding)
- tests/loom_racy_ptr_cell.rs:429-431 — VIOLATED on provenance: tag present but `addr as *mut Payload` round-trip from `.addr()` is the banned provenance-losing cast, then deallocated through (medium finding)
- transmute / mem::uninitialized / mem::zeroed — none (zero occurrences crate-wide)

**🔴 §B18 / §B18a manual Send/Sync and PhantomData:**
- src/lib.rs:164 (`unsafe impl Send`), :166 (`unsafe impl Sync`) — JUSTIFIED: SAFETY block (159-163) cites the AtomicPtr as the synchronization primitive; mirrors AtomicPtr<T>'s own std impls
- src/lib.rs:144 (`PhantomData<*mut T>`) — JUSTIFIED: invariant in T, matching the AtomicPtr field; rationale documented at :142-144/:155-157

**🔴 §B25 / §B25a raw-ownership transfer and FFI:**
- tests/loom_racy_ptr_cell.rs:470 (`Box::into_raw`) — JUSTIFIED: pairs with `from_raw` at :530, same binary/allocator
- tests/cell_unit.rs:58/:77 and tests/loom_racy_ptr_cell.rs:530 `Box::from_raw` — ownership pairing JUSTIFIED (tag gaps reported under §B5 above)
- `extern "C"` boundary — none (no FFI, no `#[no_mangle]` anywhere)
- §B25a FFI-across-threads without cited thread-safety contract — none (no C library wrapped)

**🔴 §A1 supply chain:**
- crates/racy-ptr-cell/Cargo.toml:28 — JUSTIFIED: `loom = "0.7"` is the real tokio-rs registry crate (exact name, no typo-variant), cfg-gated with documented reason
- unpinned `[patch]`/git sources — none (no `[patch.*]`, no `git =` deps anywhere in workspace root or crate manifest)
- network in build.rs — none (crate has no build.rs)

**🔴 §C1 blanket impl in pub API of a published library:**
- src/lib.rs:164, :166, :480 — N/A: all three `impl<T>` occurrences target the crate's own `RacyPtrCell<T>`, not a bare type parameter; zero true blanket impls crate-wide

**🔴 §B24 `==` on secret material:** src/lib.rs:234,356,361; tests/loom_racy_ptr_cell.rs:479,483,559,585 — N/A: all comparisons are pointer addresses vs public sentinel constants, not secrets

**🔴 §B12 any cryptographic operation:** none (no crypto deps, RNG, or key/nonce/salt material anywhere)

**🔴 §B14 unbounded channel / FuturesUnordered / JoinSet:** none (no channels, no async, no collections — single AtomicPtr, zero non-std deps)

**🔴 §B15b Pin::new_unchecked:** none (zero Pin usage of any kind; only `NonNull::new_unchecked` at src/lib.rs:252/:298/:368 — different API; flagged by both async agents)

**🔴 §B21 tokio::spawn with dropped JoinHandle:** none (no tokio; only `loom::thread::spawn` in tests, every handle joined at loom_racy_ptr_cell.rs:124-125, 175-176, 225-226, 284-285, 391-393, 527-528, 622-623; flagged by both async agents)

**🔴 §B22 impl Drop doing async work:** none (no `impl Drop` exists; the cell deliberately never drops/frees its pointee, documented at src/lib.rs:15, 135-136; flagged by both async agents)

**🔴 §F3 leaked/unclosed boundary resource an untrusted peer can hold open:** none (no sockets/files/streams/peers; the loser busy-spin is in-process and bounded by the winner's single init closure)
