# `tagged-index-stack`: full from-scratch static audit — run 5 (fxx)

- Audited tree: `main` @ `15245fcdf6acb160ff5d193168445a626ea76643` (working tree clean at start; no tracked file modified by this audit).
- Toolchain: rustc 1.97.0 (2d8144b78 2026-07-07), cargo 1.97.0; pinned `1.79`/`1.78` toolchains used for the MSRV check.
- Mode: read every file in `crates/tagged-index-stack/` (`src/`, `tests/`, all six `tests/compile_fail/*/src/main.rs` fixtures, `README.md`, `CHANGELOG.md`, `Cargo.toml`), both 2026-09-01 ADRs, the Sol-codex run-4 review, `src/registry/heap_registry.rs`, the `Registry`/`loom_shim` portions of `src/registry/bootstrap.rs`, `src/registry/heap_slot.rs` (`next_free`), and `src/lib.rs` / `src/registry/mod.rs` (visibility of `Registry`). Everything below that is stated as a fact about compiler behaviour was actually compiled; commands and outputs are in the appendix. All scratch crates live OUTSIDE the repo (`D:\system_artefact\Temp\tis-audit-run5\`, `tis-copy-c{1,2,3}`, `fix-c{1,3}`).
- Scope note: this is the audit the run-4 review's final paragraph mandated after items 1-4 landed. It was done as a fresh read, not as a diff review of the campaign's remediation.

## Verdict: CONDITIONAL-GO

Publication (#661) can proceed once ONE condition closes (P2-1 below); nothing found blocks it outright. The two structural claims the campaign now makes are true and were verified adversarially, not just re-read: (Group A) no route from a shipped `ArrayIndexStack` to its `&StackHead` or hooks exists in any spelling — generic bound, inherent method, `dyn` coercion, field access, downstream newtype, downstream orphan impl all fail to compile, and a counterfactual crate copy that re-adds the public impl fails with **E0119** (the `SealedStorage` blanket + direct impls overlap), so the seal is pinned by coherence, not only by the compile-fail fixture; (Group B) every one of the five surviving hazard shapes plus the reference implementor requires `unsafe impl` — seven impl sites without the keyword produce seven **E0200**s, and a counterfactual crate copy with the `unsafe` removed from the trait makes the plain-impl fixture compile, so that oracle is load-bearing. All six compile-fail fixtures emit exactly the asserted error and no other. Loom 11/11, the full suite (default, `--release`, `--features test-internals`), clippy `-D warnings`, rustdoc `-D warnings` (default and `test-internals`), and `cargo +1.79 check` are all green. The one defect that needs to close before publication is not a new hazard shape but a residual of the run-4 P1-1 list that the ADR neither implemented nor recorded a decision on: the three storage hooks are `pub` **safe** methods on an `unsafe trait`, so any safe code holding `&T` for a downstream implementor `T` can call `store_next`/`load_next`/`head` directly and forge the chain — the crate's own trait doc admits this (`src/imp.rs:506-534`), the ADR's rationale for safe methods ("every known hazard is implementor-side, never caller-side") is contradicted by that paragraph, and `# Safety` clause 2 as written cannot be discharged by any implementor whose `&Self` is reachable. For the one real consumer (`sefer-alloc`'s `Registry`) the blast radius is bounded to availability failures by the slot-state CAS, so it is not a soundness bug in the shipping allocator; for the published crate it is a contract-design gap at exactly the safe/unsafe boundary this audit exists to check. It is cheap to close structurally with zero `unsafe` (a crate-private hook token), see P2-1.

## Direct answers to the three questions

### Q1 — Is the double-issue hazard expressible through a FULLY SAFE public API today?

**Against the crate's own public API in isolation: NO** for the storage-binding hazard class (P1-1's subject). **YES** for two things that are not that class, and both must be stated plainly:

1. The documented **no-double-push caller contract** (`src/imp.rs:988-1023`): `push(1); push(2); push(1)` on a plain `ArrayIndexStack` silently double-issues forever. Compiled and run under `#![forbid(unsafe_code)]` (attempt A3): output `[Some(1), Some(2), Some(1), Some(2), Some(1), Some(2)]`. This is the `HashMap`-with-inconsistent-`Hash` category (a logic error contained to the data structure, no UB in this crate), is explicitly accepted by the crate as caller discipline, and is not new. It matters only to a downstream unsafe consumer, which must not let untrusted safe code push into its stack — the same obligation every safe free-list API carries.
2. The **caller-side hook route against a downstream implementor** (attempt A4, the basis of P2-1): with a CORRECT downstream `unsafe impl StackStorage<16> for Pool` (private head, dedicated cells, drives the stack only via `push_index`/`pop_index`, hands out `&'static Pool` — a faithful stand-in for `Registry`), an attacker crate that is `#![forbid(unsafe_code)]` and merely has `use tagged_index_stack::{StackOps, StackStorage};` in scope compiled and ran: `p.store_next(1, 3)` spliced the chain `3 -> 2 -> 1 -> 3 …` and six `alloc()` calls returned `[Some(3), Some(2), Some(1), Some(3), Some(2), Some(1)]`; `p.push_index(7)` then made `alloc()` hand out a slot the pool never freed. No hazard shape from the inventory is involved: the implementor is contract-abiding and the forgery is a plain safe method call.

Every attempt to reach the binding hazard through the crate alone failed to compile, with the compiler output recorded:

| # | Attempt (all `#![forbid(unsafe_code)]`) | Result |
|---|---|---|
| A1 | plain `impl StackStorage<16> for S` with correct hook bodies | `error[E0200]: the trait `StackStorage<16>` requires an `unsafe impl` declaration` |
| A2 | `fn via_generic<S: StackStorage<16>>(&owned)`; `fn via_ops<S: StackOps<16>>`; `owned.head()`; `&dyn StackStorage<16>`; `&dyn StackOps<16>`; `owned.push_index(1)`; `&owned.links`; struct-pattern destructure | 3× `E0277` (`the trait bound `ArrayIndexStack<16, 64>: StackStorage<16>` / `: StackOps<16>` is not satisfied`), 2× `E0599` (`no method named `head``; `push_index` exists but trait bounds not satisfied), 1× `E0616` (`field `links` … is private`) |
| A5 | `StackHead::<17>::new()`, `StackHead::<0>::default()`, `ArrayIndexStack::<32, 4>::new()` | 3× `error[E0080]: evaluation panicked: INDEX_BITS must be in 1..=16 …` — the guard is reachable from every constructor, not only from `TaggedIndex` |
| A7 | the six `tests/custom_storage_impl.rs` implementors (seven impl sites, `DualWidth` at two widths) with `unsafe` removed | 7× `E0200` (six for `StackStorage<16>`, one for `StackStorage<12>`) |
| A8 | downstream newtype `Wrap(ArrayIndexStack<16,64>)` forwarding `self.0.head()` | `E0599: no method named `head`` — the inner head is unreachable even to a wrapper that itself writes `unsafe impl` |
| A8b | downstream `unsafe impl StackStorage<16> for ArrayIndexStack<16, 64>` | `E0117` (orphan rule) — no crate other than `tagged-index-stack` can ever add the impl |
| C2 | counterfactual: crate COPY with `unsafe impl<B, N> StackStorage<B> for ArrayIndexStack<B, N>` re-added in `imp.rs` | `error[E0119]: conflicting implementations of trait `imp::SealedStorage<_>` for type `ArrayIndexStack<_, _>`` (plus the crate's own `deny(unsafe_code)` firing) — Group A is pinned by coherence inside the crate itself |

Rule 1's "completeness of coverage" census (`src/imp.rs:627-659`) re-verified mechanically on the current file: 10 `impl` blocks (lines 88, 317, 418, 1161, 1375, 1469, 1574, 1580, 1615, 1673), 3 derives, all `Debug` (288, 1463, 1610), 4 signatures returning `&StackHead` (950 trait decl; 1152, 1162, 1581 all `pub(crate) SealedStorage`). No `Clone`/`Copy`/`Deref`/`AsRef`/`Borrow`/`Index`/`From`. The two routes to a `&StackHead` the doc names (own a value; call `head()` on an implementor) are the only two.

A corollary worth stating for downstream users: a downstream crate that is itself `#![forbid(unsafe_code)]` cannot implement `StackStorage` at all (C2's copy showed `error: implementation of an `unsafe` trait` under `deny`; under `forbid` it is unconditional). Group B therefore does more than force an acknowledgment — it excludes the entire forbid-unsafe ecosystem from the extension point, which is the intended reading of "allocator category".

### Q2 — Is every compile-fail oracle proof of the property it claims, or only of an old spelling's absence?

Each fixture was built directly (child `cargo build --offline`, `RUSTFLAGS`/`CARGO_ENCODED_RUSTFLAGS` stripped exactly as the drivers do) and its complete error inventory recorded, then three of the properties were falsified counterfactually on crate copies outside the repo:

| Driver / fixture | Asserts | Complete error inventory observed | Proves what it claims? |
|---|---|---|---|
| `compile_fail_two_backings.rs` / `two_backings` | `E0599` + `no method named `push` ` + `no method named `pop` ` | 1× `E0599 push`, 2× `E0599 pop`, nothing else | Yes — and the prose now correctly scopes it as an API-removal regression, NOT a safety proof (`tests/compile_fail_two_backings.rs:1-25`). The P1-2 overclaim is gone. |
| `compile_fail_array_index_stack_head.rs` / `array_index_stack_head` | `E0277` with the exact bound wording + `E0599 no method named `head`` | 2× `E0277` (generic route and `dyn` route), 1× `E0599`, nothing else | Yes. An `E0277` naming `ArrayIndexStack<16, 64>: StackStorage<16>` is a direct statement that no impl exists, which is the structural property; every other route (inherent, `dyn`, newtype, orphan) is a consequence. The fixture pins one instantiation only (`<16, 64>`); the coherence pin (C2, `E0119`) is the instantiation-independent guarantee and is not cited anywhere in the crate — see P4-4. |
| `compile_fail_unsafe_impl_required.rs` / `unsafe_impl_required` | `E0200` + exact wording + `PlainStorage` named | 1× `E0200`, nothing else | Yes. **Counterfactual C1:** the same fixture built against a crate copy with `pub unsafe trait` → `pub trait` COMPILES (`Finished`); against the real crate it fails with `E0200`. **Compile-pass twin A6:** the exact fixture with `unsafe` added compiles and round-trips (`Some(1)`, then `None`), so the fixture's only defect really is the missing keyword. |
| `compile_fail_index_bits_bounds.rs` / `index_bits_zero`, `index_bits_seventeen` | `E0080` + `INDEX_BITS must be in 1..=16` | 1× `E0080` each with the full `_CHECK_BITS` message, nothing else | Yes. **Counterfactual C3:** with `_CHECK_BITS`'s condition replaced by `true` in a crate copy, `index_bits_seventeen` COMPILES; against the real crate → `E0080`. |
| `compile_fail_loom_cfg_without_feature.rs` / `loom_cfg_without_feature` | the named `compile_error!` text present AND no `E0433` / no `cannot find module or crate `loom`` | exactly `error: building with --cfg loom requires --features loom (loom is now an optional dependency)` + the `could not compile` line; no `E0433` | Yes — this is the one driver that asserts the ABSENCE of a secondary error, and the inventory confirms the module gating works. |

None of the six can pass for a weaker reason: every driver asserts a specific error code AND a message fragment that only the named guard produces, and the inventories show no extraneous errors that could mask a regression. The drivers do not assert "no other errors" (only the loom driver does), which is acceptable because the fixtures are single-purpose and small; noted, not a finding.

### Q3 — Has any structural guarantee been replaced by documentation?

Checked each guarantee the current tree claims, against what actually enforces it:

| Guarantee | Enforced by | Verified how |
|---|---|---|
| No route from `ArrayIndexStack` to `&StackHead`/hooks (Group A) | Type system: no impl; `SealedStorage` is `pub(crate)`; fields private; orphan rule | A2, A8, A8b; coherence counterfactual C2 (`E0119`) |
| Every custom implementor asserts the contract (Group B) | `unsafe trait` → `E0200` | A1, A7 (7/7), counterfactual C1 |
| `INDEX_BITS ∈ 1..=16` | const assert reached from every associated item and every constructor | A5 (3 constructors), C3 counterfactual |
| Rule-4 value guard (out-of-range / self-loop) | release-active runtime panic (`src/imp.rs:1321-1324`) | `pop_rule_4_guard_fires_on_invalid_next_from_backing`, three `#[should_panic]` shape tests, `double_push_of_current_head_panics_on_first_pop` — all non-vacuous (removing the guard flips each `should_panic` to a failure) |
| Ordering proof (release sequence on `head`) | code + loom | loom 11/11 including the three `#[should_panic]` counterfactuals; the `head` field's "every write is an RMW" invariant holds — `grep` finds no `store(` on the head anywhere in `src/` |
| Hazard shapes 1-4 for custom implementors | **documentation + `unsafe impl` acknowledgment only** (the ADR is explicit that these are irreducible) | A7 confirms the acknowledgment is compiler-forced; the six pinning tests still exercise each shape under `unsafe impl` |
| "The three hooks are implementor hooks, not caller-facing API" (`src/imp.rs:506-534`) | **documentation only** — the methods are `pub` safe `fn` on a `pub` trait | A4: forged in a `forbid(unsafe_code)` crate → **P2-1** |
| No double-push | documentation only (accepted caller discipline; cannot be checked in O(1)) | A3 — not a new finding |

So the answer is: **yes, one** — the non-callability of the hooks by ordinary callers. It is the shape the campaign's own history warns about: an honest, accurate paragraph ("nothing warns … Doing so corrupts the free-list exactly the way a double-push does") standing in for an enforcement mechanism that the run-4 review explicitly asked for (P1-1 "what to fix", item 3: make the storage hooks inaccessible to an ordinary caller via an unconstructible access token) and that the ADR neither adopted nor recorded as rejected — `grep -i token` over the crate and both ADRs finds only the unrelated `unsafe` "token" wording. The ADR's stated reason for keeping the methods safe (`docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md:63`: "every Group B hazard is implementor-side … never caller-side misuse of an otherwise-safe method call") is false for exactly this route, and the crate's own rustdoc says so 400 lines above the trait declaration.

## Findings

### P1 — blocks publication

None.

### P2 — should fix before publication

**P2-1. The storage hooks are safe `pub` methods on an `unsafe trait`, so `# Safety` clause 2 is not dischargeable by any implementor whose `&Self` is reachable, and the caller-side forgery route the run-4 review asked to close is still open.**
`src/imp.rs:943-965` (trait and hook declarations), `src/imp.rs:453-458` (clause 2: "a `load_next` must observe the most recent `store_next` the stack itself performed"), `src/imp.rs:506-534` (the paragraph admitting callers can invoke the hooks and corrupt the chain), `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md:63` (the contradicted rationale).
Failure scenario (reproduced, attempt A4): a third party publishes `pub struct MyPool` with a CORRECT `unsafe impl StackStorage<16> for MyPool` and unsafe slot storage keyed on the issued indices — precisely the allocator use case the crate is marketed for. Any user of `my-pool` writes, in 100% safe code, `use tagged_index_stack::StackStorage; pool.store_next(1, 3);` and the pool now issues index 1 to two owners. The `unsafe impl` author asserted clause 2 but has no means to uphold it; the `# Safety` section — the only text an `unsafe impl` author is obliged to read — never tells them the hooks are world-callable or that `&Self` must not escape. Under Rust's soundness rules the unsound party is `my-pool`, but the trap was set by this crate's contract design. For `sefer-alloc` the same route exists under `--features internals` (see P3-1) and is bounded to availability failures; for an arbitrary downstream it is not bounded.
Why this is P2 and not P1: it is not expressible against the crate alone (no implementor ships in the crate), the crate does document it, and the one real consumer is defended by its slot-state CAS. Why it is not P3: it sits on the exact safe/unsafe boundary the campaign spent six rounds and an ADR on, the fix is small, and it is the last documented-only surface in the crate's own design.
Recommended closure (structural, zero `unsafe`, keeps `dyn StackStorage` object-safe, keeps `Registry`'s impl a three-line mechanical change): add a crate-private-constructible witness parameter to the three hooks —
```text
pub struct Hook(());                 // private field: only this crate can construct it
static HOOK: Hook = Hook(());         // crate-private
fn head(&self, _: &Hook) -> &StackHead<INDEX_BITS>;
fn load_next(&self, _: &Hook, index: u32) -> u32;
fn store_next(&self, _: &Hook, index: u32, next: u32);
```
The `SealedStorage` bridge passes `&HOOK`; no external caller can name a `Hook` value, and a `&Hook` cannot be stashed past the call. Add a compile-fail fixture (`hook_token_unconstructible`: `pool.store_next(&Hook(()), 1, 3)` → `E0451`, and a bare `pool.store_next(1, 3)` → `E0061`) plus the compile-pass twin, delete the now-false "implementor hooks, not caller-facing API" paragraph in favour of a one-line pointer to the fixture, and record the decision in the Wave-2 ADR. If the owner instead declines the structural route, the minimum acceptable alternative is a NORMATIVE clause 6 in `# Safety` ("the hooks are safe to call by anyone holding `&Self` with this trait in scope; the implementor must ensure no such caller exists outside its trust boundary — in practice, do not expose `&Self`/`&dyn StackStorage` of the implementor to code you do not control") plus an ADR entry stating why run-4's item 3 was rejected. The structural route is recommended: the owner's recorded framing for Group B was "willing to break compatibility for it", and this is the same decision one level down.

### P3 — should fix soon after (all trivial; P3-1 and P3-2 are best landed with P2-1)

**P3-1. `sefer-alloc`: the real `Registry` — with its `unsafe impl StackStorage<16>` — is reachable from external safe code under `--features internals`, and the impl's `// SAFETY:` clause 1/3 wording does not acknowledge it.**
`src/lib.rs:375-377` (`pub mod registry` under `alloc-global + internals`, `#[doc(hidden)]`), `src/registry/mod.rs:45-46` (`pub mod bootstrap`), `src/registry/bootstrap.rs:686` (`pub struct Registry`), `:989` (`pub fn ensure() -> &'static Registry`), `src/registry/heap_registry.rs:594-611` (clause 1: "`free_slots` is `pub(crate)` whose ONLY reference ever handed out is this impl's `head()`"; clause 3: "no second binding over the cells exists").
Verified (probe2, `cargo check` only, external crate with `sefer-alloc = { features = ["alloc-global", "internals"] }`, `#![forbid(unsafe_code)]`): `ensure().head()`, `.load_next(3)`, `.store_next(3, 3)`, `.push_index(5)`, `.pop_index()` all type-check. Consequences were traced through `claim`/`claim_with_config`/`recycle` (`heap_registry.rs:120-181, 211-315, 343-376`): every issued index must additionally win a `FREE -> LIVE` CAS and every push is preceded by a `LIVE -> FREE` CAS, so a forged chain can leak slots (`pop_index` steals a FREE slot off the list forever), livelock `claim` (a cycle of all-LIVE entries loops forever), or trip the rule-4 panic inside the allocator — availability failures, **not** double ownership of a `HeapCore`. Not a soundness bug; a hardening gap and an inaccurate safety comment. Fix: either make `ensure`/`Registry` `pub(crate)` with a narrower `#[doc(hidden)]` test surface, or (if P2-1's token lands) nothing structural is needed beyond correcting the SAFETY comment to say the `internals`-gated exposure exists and that the slot-state CAS is the defence.

**P3-2. `CHANGELOG.md` still advertises `#![forbid(unsafe_code)]` as a current property, in the first `### Added` bullet, inside the file that ships in the `.crate`.**
`crates/tagged-index-stack/CHANGELOG.md:14-16` vs `:311-319` (`### Changed`: moved to `deny`). `cargo package --list` confirms `CHANGELOG.md` is packaged. The doc-consolidation ADR's own principle 4 ("Entries describing CURRENT behavior must be corrected when false") was applied to three other entries and missed this one. One-line fix.

**P3-3. Root crate's unsafe-inventory mirror is stale for this crate.**
`src/lib.rs:167` (root `sefer-alloc`): "tagged-index-stack … — `#![forbid(unsafe_code)]`" (and "AtomicUsize head" — it is `AtomicU64`). `README.md:603` and `:672` were updated to the `deny` + one-token wording; the `src/lib.rs` header, which CLAUDE.md names as the README's mirror, was not (ADR step 6 asked for exactly this cross-check).

### P4 — minor / cosmetic

**P4-1. README wording is false as written.** `crates/tagged-index-stack/README.md:65-66`: "`head()` is NOT a plain safe method on every implementor" — it IS a plain safe method on every implementor (that is P2-1); what the sentence means is that `ArrayIndexStack` is not an implementor. Reword.

**P4-2. Loom-shim ordering divergence not in its divergence list.** `src/registry/bootstrap.rs:466` claims the shim uses the "same Acquire/Release/Relaxed orderings"; its `push_index` loads the head with `Acquire` (`:572`) where the shipped `push_index_impl` uses `Relaxed` (`src/imp.rs:1195`). Stronger, so harmless, but the list at `:472-506` enumerates five deliberate divergences and this sixth is undocumented.

**P4-3. `push_back_after_oom` discards its CAS result.** `src/registry/heap_registry.rs:485-488`: `let _ = slot.cas_state(LIVE, FREE, ..); push_free_slot(..)`. Every current caller is the sole writer of a LIVE slot (verified: `claim`, `claim_with_config`, `ConflictRollback::drop`, `dbg_claim_then_simulate_oom`), so today this is correct; a future caller reaching it with a slot that is already FREE would double-push. A `debug_assert!(cas.is_ok())` or pushing only on `Ok` costs nothing.

**P4-4. The Group A oracle pins one instantiation; the coherence pin is the general guarantee and is uncited.** `tests/compile_fail/array_index_stack_head/src/main.rs` asserts `<16, 64>` only. The instantiation-independent fact — any in-crate `impl StackStorage<B> for ArrayIndexStack<B, N>` is `E0119` against the `SealedStorage` blanket (`src/imp.rs:1161` vs `:1580`), and any out-of-crate one is `E0117` — is stronger and should be stated in `ArrayIndexStack`'s type doc (`src/imp.rs:1446-1456`) and the driver's module doc, so a future reader does not believe the fixture is the only thing holding the seal.

**P4-5. ADR ledger line numbers are stale.** `docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md:79` cites the rule-1 census at lines 980/1185/1195/1614; after the consolidation commit they are 950/1152/1162/1581 (counts unchanged: 10 impls, 3 derives, 4 signatures). The ADR calls it a dated snapshot, so this is cosmetic.

## What was verified green (evidence, not assertion)

- `cargo test -p tagged-index-stack --features test-internals`: 45 tests across 12 integration-test binaries, 0 failures (compile-fail drivers 6/6, `custom_storage_impl` 8/8, `stack_unit` 20/20, `threaded_conservation` 1/1 with both activation-oracle levels asserted, proptest 7/7, `regression_counter_wrap` 2/2, `readme_example` 1/1).
- `cargo test -p tagged-index-stack --release` (the CI row): all green.
- `RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --features loom --test loom_aba` (dedicated target dir): 11 passed, 0 failed, including the three `#[should_panic]` counterfactuals.
- `cargo clippy -p tagged-index-stack --all-targets --features test-internals -- -D warnings`: clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` (default and `--features test-internals`): clean. (The crate declares no `package.metadata.docs.rs.features`, so docs.rs builds default features — covered.)
- `cargo +1.79 check -p tagged-index-stack` (default and `--features test-internals`): clean; `cargo +1.78 check` is refused by the `rust-version` gate.
- `cargo package -p tagged-index-stack --list`: 28 files; the `tests/compile_fail/` fixture crates are excluded as documented, the five `compile_fail_*.rs` drivers ship with their packaged-skip guard. (`cargo publish --dry-run` needs network; CI runs it at `ci.yml:761`.)
- `Registry`'s `unsafe impl` (`src/registry/heap_registry.rs:622-648`) against each `# Safety` clause: (1) process singleton `static REGISTRY`, private `const fn new`, `free_slots` `pub(crate)` — holds, modulo the `internals` exposure in P3-1; (2) `load_next`/`store_next` both resolve through `Registry::slot` onto `HeapSlot::next_free`; `OncePtrCell` publishes each chunk exactly once, so the index→cell mapping is stable — holds; (3) `grep -rn next_free src/` finds only the two cfg-gated impl bodies and the field declaration — holds inside the crate; (4) `next_free` is a dedicated `AtomicU32` field (`heap_slot.rs:414-418`), every pushed index is `< MAX_HEAPS = 4096 < 0xFFFF` (`bump_count` returns `None` at the cap; `recycle` range-checks `heap.id()`; `push_free_slot` debug-asserts) — holds; (5) `head()` returns the fixed field — holds. No double-push path: `recycle` and `push_back_after_oom` both CAS `LIVE -> FREE` first (the latter's ignored result is P4-3). The ordering pairing (`Acquire` load / `Release` store) matches the trait's ordering contract.
- Release-sequence invariant on `StackHead::head`: every write in `src/` is a `compare_exchange`; no `store(` exists on the head; the proof in `src/imp.rs:290-313` is sound as stated (each RMW on the head extends every earlier push's release sequence, so any `Acquire` read of any head value synchronizes with every earlier link-publishing push).

## Condition for #661 to proceed

Close **P2-1** — preferably structurally (the `&Hook` witness parameter, with the compile-fail/compile-pass fixture pair and the ADR entry), or, if the owner declines that, by promoting the caller-side obligation into a normative `# Safety` clause AND recording in the Wave-2 ADR why run-4's item 3 was rejected. P3-2 (the CHANGELOG `forbid` claim) ships inside the `.crate` and should land in the same commit; P3-1 and P3-3 are `sefer-alloc`-side and do not gate the crate's publication.

## Appendix — reproduction

Scratch workspace (outside the repo): `D:\system_artefact\Temp\tis-audit-run5\` with members `pool` (correct downstream `unsafe impl`, `pub fn pool() -> &'static Pool`), `probe` (bins `a1_safe_impl`, `a2_steal_head`, `a3_deep_double_push`, `a4_hooks_on_downstream`, `a5_width_17`, `a6_fixture_with_unsafe`, `a7_shapes_without_unsafe`, `a8_newtype`, `a8b_orphan`; every attack bin is `#![forbid(unsafe_code)]`), `probe2` (check-only, depends on `sefer-alloc` with `alloc-global,internals`). Crate copies for counterfactuals: `tis-copy-c1` (`pub unsafe trait` → `pub trait`), `tis-copy-c2` (public impl for `ArrayIndexStack` re-added), `tis-copy-c3` (`_CHECK_BITS` condition → `true`); fixture copies `fix-c1` (`unsafe_impl_required`) and `fix-c3` (`index_bits_seventeen`) re-pointed at the copies, then re-pointed at the real crate as controls.

Commands (all run from clean targets):
```text
cargo build --offline -p probe --bin a1_safe_impl          # E0200 x1
cargo build --offline -p probe --bin a2_steal_head         # E0277 x3, E0599 x2, E0616 x1
cargo run   --offline -p probe --bin a3_deep_double_push   # [Some(1),Some(2),Some(1),Some(2),Some(1),Some(2)]
cargo run   --offline -p probe --bin a4_hooks_on_downstream# [Some(3),Some(2),Some(1),Some(3),Some(2),Some(1)]; push_index(7) -> alloc()==Some(7)
cargo build --offline -p probe --bin a5_width_17           # E0080 x3
cargo run   --offline -p probe --bin a6_fixture_with_unsafe# compiles; Some(1) then None
cargo build --offline -p probe --bin a7_shapes_without_unsafe # E0200 x7
cargo build --offline -p probe --bin a8_newtype            # E0599 x1
cargo build --offline -p probe --bin a8b_orphan            # E0117 x1
cargo check --offline -p probe2                            # sefer-alloc Registry hooks type-check from safe code
(cd fix-c1 && cargo build --offline)                       # Finished  (trait made safe)  | control vs real crate: E0200
(cd tis-copy-c2 && cargo check --offline)                  # E0119 conflicting SealedStorage impls (+ deny(unsafe_code))
(cd fix-c3 && cargo build --offline)                       # Finished  (_CHECK_BITS disarmed) | control vs real crate: E0080
```
Fixture inventories were produced with the drivers' exact child environment: `env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS CARGO_TERM_COLOR=never cargo build --offline --manifest-path crates/tagged-index-stack/tests/compile_fail/<name>/Cargo.toml` (and `RUSTFLAGS="--cfg loom"` for the loom fixture), with `CARGO_TARGET_DIR` pointed outside the repo.
