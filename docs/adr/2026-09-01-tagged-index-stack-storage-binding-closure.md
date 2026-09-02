# ADR: closing `tagged-index-stack`'s safe-API double-issue hazard (task #1807)

- Status: **DECIDED.** Owner approved both Group A and Group B below (2026-09-01), after an independent second opinion from `@fh` on Group B specifically, explicitly requested with the framing "we are striving for perfection and are willing to do more and break backward compatibility for it." Feeds directly into #1808 (`tis-sol4-W2-Core`).
- Source: `docs/reviews/2026-09-01-124216-tagged-index-stack-review-Sol-codex-run-4.md`, finding P1-1 (release-blocking) + P4-2.
- Author: this session, task #1807 (`tis-sol4-W2-Design`); Group B second opinion by `@fh` (agent `a52ce74b404b5a2bd`).

## The problem, restated precisely

`StackStorage<INDEX_BITS>`'s three methods (`head`, `load_next`, `store_next`) are `pub`, safe, and callable by anyone who has the trait in scope. `StackOps` is blanket-implemented for every `StackStorage` implementor and trusts that a given implementor's `head()` and its `load_next`/`store_next` pair describe ONE coherent, exclusively-owned head↔links binding. Nothing in the type system enforces that. Six committed, compiling tests in `tests/custom_storage_impl.rs` demonstrate double-issue through fully safe code:

1. `array_index_stack_head_still_double_issue` — extracts `&StackHead` and the link hooks directly off a standalone `ArrayIndexStack`, the type marketed as a safe, self-contained abstraction, and builds a competing binding against it.
2. `hand_crafted_acyclic_forgery_still_double_issues` — a custom implementor whose `load_next`/`store_next` are hand-forged to answer an acyclic-but-wrong chain.
3. `two_stacks_sharing_link_storage_still_double_issue` — two implementor values with separate heads sharing one link-cell population with overlapping reachable indices.
4. `head_moved_into_fresh_links_leaks_and_then_panics` — a live `StackHead` moved (by value) out of one implementor into a fresh one with different links.
5. `one_value_two_bindings_shared_backing_still_double_issue` — ONE implementor value with two `StackStorage` impls at different const-generic widths, both over one link backing.
6. `internally_disagreeing_storage_still_double_issue` — one implementor whose own `load_next`/`store_next` disagree about which backing they read/write.

Six rounds of this crate's own `@oh` review loop (rounds 10–15) found these one at a time and closed each with **documentation** — a growing, now-4-shape hazard inventory, single-sourced panic-cause prose, pinning tests. Run-4's verdict is that this was the wrong remedy for a defect that isn't a documentation gap: *"a contract the safe type system doesn't enforce cannot be considered closed just because it's thoroughly documented."*

## A fact the review didn't have, and that changes the shape of the fix: the seam is real and load-bearing

`sefer-alloc`'s own `Registry` (`src/registry/heap_registry.rs:588`) implements `StackStorage<16>` for itself, binding its head to **slot-resident** links (each slot's own `next_free: AtomicU32` field, not a separate array) — this is the crate's REAL production consumer, in a genuinely separate crate from `tagged-index-stack`'s own perspective (a Cargo workspace member, not an internal module). It is not a hypothetical or a test fixture.

Checked (`grep -n "\.head()\|\.load_next(\|\.store_next(" src/registry/heap_registry.rs src/registry/bootstrap.rs src/kani_proofs.rs`): `Registry` **never calls `head()`/`load_next()`/`store_next()` directly** — it only implements the trait (three methods) and calls `push_index`/`pop_index` through `use tagged_index_stack::StackOps as _;`. The three `.head()`/`.load_next()`/`.store_next()` call sites found in `src/registry/bootstrap.rs` are on a *separate*, `pub(crate)`-scoped mirror trait (`bootstrap::loom_shim::StackOps`, used only under `--cfg loom`) — not the real `tagged_index_stack` crate's public trait at all.

Consequence: **the external custom-storage extension point cannot be removed outright** without breaking `Registry` — options that eliminate the public `StackStorage` trait entirely are off the table. But **any signature change to `StackStorage`'s three methods is compatible with `Registry`**, because `Registry` only ever *implements* them, never *calls* them directly.

## The hazards split into two structurally different groups

**Group A — `ArrayIndexStack`-specific leakage (test #1 only).** The crate's own doc already states the intent: *"Custom implementors with slot-resident links do not use this type: they implement `StackStorage` instead."* `ArrayIndexStack` was never meant to be an extensible backing. Its `impl StackStorage<B> for ArrayIndexStack<B, N>` exists ONLY so the blanket `StackOps` impl can supply `push`/`pop`'s bodies — but that same public trait impl is what lets any external caller extract `&StackHead` and the hooks off a value that's supposed to be a closed, safe, standalone type. This is closeable **structurally, with no new public API surface and no `unsafe`**: move `ArrayIndexStack`'s push/pop off the *public* `StackStorage`/`StackOps` plumbing entirely and onto a `pub(crate)`-sealed equivalent, so `ArrayIndexStack` no longer implements the public trait at all. `array_index_stack_head_still_double_issue`, as written, stops compiling (E0277: the trait isn't implemented) — not "panics differently", genuinely unexpressible.

**Group B — custom-implementor value-level obligations (tests #2, #3, #4, #5, #6).** Every one of these is a THIRD-PARTY implementor's own struct manipulating its own fields or writing its own (possibly wrong) hook bodies — moving a `StackHead` field, sharing a `&ArrayLinks` reference across two owned values, implementing the trait twice at different widths, hand-rolling a wrong `load_next`. None of them go through `ArrayIndexStack` or any crate-owned code path at all. **No type-system mechanism inside `tagged-index-stack` can prevent a downstream crate from doing any of this while the external extension point stays safe-callable** — the obligation is fundamentally about data the crate cannot observe (is this `StackHead` value reachable through a second live implementor right now? do these two `&ArrayLinks` alias?). The only mechanisms that change this are: making the trait (or the specific methods) `unsafe`, so the compiler at least forces an explicit `unsafe impl` acknowledgment at every implementation site; or removing the external seam altogether (ruled out above, breaks `Registry`).

## The real cost of the `unsafe trait` option — bigger than the review's phrasing suggests

`crates/tagged-index-stack/src/lib.rs:243` is `#![forbid(unsafe_code)]` — **`forbid`, not `deny`**. Unlike `deny`, a `forbid` lint cannot be locally relaxed by an inner `#[allow(unsafe_code)]` ANYWHERE in the crate, not even in a dedicated, narrowly-scoped seam module — attempting that is itself a compile error. Round 13 already reproduced this exact failure for the narrower `unsafe fn head()` alternative (documented in `src/imp.rs`'s `# Stability` section, with the actual `E0053`/`E0133`/"declaration of an `unsafe` method" errors captured). This crate has **zero** `unsafe` anywhere today, and `lib.rs:18`'s own headline advertises `#![forbid(unsafe_code)]` as a selling point next to "Allocation-free" and "`no_std`".

So "mark `StackStorage` `unsafe trait`" is not a contained, one-file change — it requires **downgrading the whole crate from `forbid(unsafe_code)` to `deny(unsafe_code)`** (matching the pattern the *root* `sefer-alloc` crate already uses, with its own README-documented "where unsafe lives" inventory) before a single `unsafe trait`/`unsafe impl` anywhere in this crate becomes possible at all. That is a crate-wide policy reversal and the loss of a claim currently on the crate's own front page, not "add one seam."

## Three real choices for Group B — presented for the owner's decision, not decided here

1. **Flip to `unsafe trait StackStorage` (or `unsafe fn` on the three methods)**, accepting the `forbid → deny` policy reversal crate-wide, with a new, single, documented `#![allow(unsafe_code)]` seam around the trait declaration and a `# Safety` contract stating exactly what an implementor must uphold (one live implementor value per head for its whole life; disjoint reachable-index populations across any shared link-cell population). `Registry`'s own `impl StackStorage<16> for Registry` becomes `unsafe impl`, forcing `sefer-alloc` to explicitly assert (at its own call site) that it upholds the contract — which it already does by construction (one `Registry`, one binding), so this costs `Registry` nothing beyond the `unsafe impl` keyword and a `# Safety` justification comment. This is the only option that makes the compiler force an explicit acknowledgment at every implementation site — closest to what run-4's point 4 asks for, at the real cost of the crate-wide policy reversal above.

2. **Remove the external seam entirely** (fold `Registry`'s Treiber-stack usage into a `tagged-index-stack`-internal-only mechanism, retiring the public custom-storage extension point). Structurally the cleanest closure for Group B — no implementor obligation survives because there is no external implementor. Cost: `Registry`'s own architecture would need to change (how much depends on whether an internal-only, non-`pub` generalization is even worth keeping vs. inlining the Treiber-stack logic back into `sefer-alloc` directly), and this crate's advertised feature ("bring your own storage") disappears. Not evaluated in depth here — flagged as available, not recommended without knowing whether the custom-storage extension point has value to the owner beyond `Registry`'s current use.
3. **Keep `StackStorage` safe; do not attempt structural closure of Group B.** Close Group A (below), fix every overclaiming statement (already scoped into task #1804/W1-DocSync), and reframe the remaining Group B obligations honestly as an accepted, documented trust boundary for anyone implementing the trait — not a promise of detection, a statement of what the crate does NOT and structurally cannot verify. This keeps `forbid(unsafe_code)` intact and costs nothing beyond honest wording, but does **not** fully satisfy run-4's stated bar for a safe trait ("must not promise integrity the compiler doesn't check") — it satisfies the "don't promise falsely" half, not the "have the compiler check it" half.

None of these three is free. (1) is the only one that actually closes Group B at the type level, at the cost of a crate-wide policy reversal. (2) closes it more completely still, at the cost of retiring a public feature and touching `Registry`. (3) preserves everything else about the crate's current posture, at the cost of leaving run-4's core objection about Group B only partially answered.

## Recommendation

- **Group A: close it now, unconditionally.** Move `ArrayIndexStack`'s push/pop off the public `StackStorage`/`StackOps` plumbing onto a `pub(crate)`-sealed equivalent (a private trait + a shared private generic algorithm function that both `ArrayIndexStack`'s inherent methods and the public blanket `StackOps` impl call into). No new public API, no `unsafe`, no cost to `Registry` (`Registry` never used `ArrayIndexStack`'s trait impl — it has its own). This is close to the review's own option 2, executed via a concrete mechanism rather than left abstract. `array_index_stack_head_still_double_issue` becomes a genuine compile-fail oracle instead of a compiling demonstration.
- **Group B: the owner's call — this is exactly what task #1807 exists to escalate, not to decide unilaterally.** My own lean, stated plainly: option 1 (`unsafe trait`) is the only one of the three that actually satisfies run-4's stated bar, and the crate-wide `forbid → deny` reversal it costs is a one-time, well-precedented change (the root crate already lives at `deny` with a documented seam inventory) — not a novel risk to this codebase's overall practice, just novel *for this one crate*. But it does spend something this crate's own marketing currently claims, and that is squarely a product decision, not an engineering one.
- **P4-2 (public blanket `StackOps` / semver-coherence cost): keep it public, do not act on this finding beyond acknowledging the tradeoff.** `Registry` genuinely needs "implement `StackStorage`, get `push_index`/`pop_index` for free" — that mechanism cannot be made non-public without breaking the crate's only real external consumer. Revisit only if a second, colliding blanket-impl trait need materializes (currently hypothetical, zero evidence of one).

## Final decision (owner-approved 2026-09-01)

**Group A: closed as recommended.** `ArrayIndexStack` loses its public `StackStorage` impl in favor of a `pub(crate)`-sealed equivalent.

**Group B: Option 1, whole-trait `unsafe`, `@fh`'s exact shape — methods stay safe.** Second opinion requested explicitly with the "willing to break compatibility, striving for perfection" framing; `@fh`'s full reasoning is reproduced below because it is the operative rationale for a decision this consequential, not just a citation.

`@fh`'s soundness argument, verbatim in substance: `StackStorage`'s contract — exclusive issuance of a free-list index — is precisely the property `sefer-alloc`'s own unsafe allocator code (`Registry::pop_free_slot`, `src/registry/heap_registry.rs:652`) relies on for memory safety once a slot index is handed back. A safe trait can carry an unchecked semantic contract only as long as nothing unsafe is *entitled* to rely on it; the moment unsafe code (today, `Registry`'s own consumer code; on publication, any third party's unsafe allocator built on this crate) depends on the contract, the trait is morally in `GlobalAlloc`/`Allocator` territory — both of which are `unsafe trait` for exactly this reason. Marking `StackStorage` `unsafe` does not make the compiler verify the value-level binding invariant (unobservable to the type system, irreducibly) — it moves the unchecked promise into Rust's unsafe-contract system, where responsibility for a violation is formally assigned to whichever `unsafe impl` asserted a contract it didn't uphold. Combined with Group A's sealing, this takes all six demonstrating tests out of "reachable from 100% safe code": test #1 stops compiling; tests #2–#6 require the implementor to have already written `unsafe impl` — i.e., to have asserted the very contract they go on to violate.

`@fh`'s form choice — whole-trait `unsafe`, NOT `unsafe fn` on the three methods (the literal `GlobalAlloc` shape) — and why: every Group B hazard is implementor-side (a third party's own struct doing something to its own fields or hook bodies), never caller-side misuse of an otherwise-safe method call. `unsafe fn` addresses callers, not implementors — it would force implementors to signature-match `unsafe fn head()` with no contract acknowledgment at the `impl` site (the actual thing that needs forcing), falsely imply that *calling* `head()` is the dangerous act (it isn't — building on the returned reference is), and reopen ~6 `unsafe {}` blocks inside `StackOps`'s blanket impl for no corresponding safety gain. Whole-trait `unsafe` with safe methods leaves exactly ONE `unsafe` token in the entire crate — the trait declaration — with zero unsafe blocks anywhere, `StackOps`'s blanket impl unchanged, and every `push_index`/`pop_index` caller (including `Registry`'s) untouched.

`@fh` also evaluated and rejected, on the record: brand-lifetime/`GhostCell`-style tokens (can't reach inside hook bodies — shapes 1/5/2's hand-forged `load_next` are invisible to any such token; and closure-scoped brands are structurally incompatible with `Registry::free_slots` living in a `static`), and a runtime binding-identity stamp (false-positives on legitimate moves of an owned stack, unsound under address reuse, adds a hot-path check that only catches 2 of 5 remaining shapes). Confirms the ADR's own earlier finding: no mechanism inside the type system alone reaches Group B; only `unsafe` (this decision) or removing the external seam (Option 2, rejected below) changes anything.

Option 2 (remove the seam) was rejected on the same reasoning already in this ADR, reinforced by `@fh`: it retires the crate's entire reason to exist (the CRATE-P7 extraction of the H-2/RAD-1 algorithm specifically so it stops being reinvented per-consumer), and every concrete shape of "keep an internal-only escape hatch for `Registry`" reintroduces the same safe seam with extra steps. Option 3 (stay safe, document only) was rejected because the owner explicitly offered to pay for structural closure and chose to spend it.

## Concrete implementation plan for #1808/#1809 (from `@fh`, adopted verbatim)

1. `crates/tagged-index-stack/src/lib.rs:243`: `#![forbid(unsafe_code)]` → `#![deny(unsafe_code)]`. Rewrite the crate's own front-page headline (`lib.rs:18`, and any other site making the same "forbid" claim — grep the whole crate, do not trust one citation) to state honestly: no `unsafe` BLOCKS or FUNCTIONS anywhere, `deny(unsafe_code)` with exactly one audited `unsafe` token (the `unsafe trait StackStorage` declaration), and why (allocator consumers rely on exclusive index issuance for memory safety). Add a short "Where unsafe lives" section mirroring the root `sefer-alloc` crate's own established practice (see that crate's README for the pattern) — this crate's `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]'` self-verifying inventory command (the one the root crate's CLAUDE.md rule already mandates workspace-wide) will now return exactly one hit here; that hit must be named in this new section.
2. `crates/tagged-index-stack/src/imp.rs:885`: item-level `#[allow(unsafe_code)]` (the tier-2 pattern this workspace already uses elsewhere — an individual `unsafe`-carrying declaration in an otherwise-safe file, with its own `# Safety` doc) + `pub unsafe trait StackStorage<const INDEX_BITS: u32>`. Promote the existing rules 1–5 (currently descriptive prose in the hazard-inventory section) into a normative `# Safety` section on the trait itself: one live implementor value per head for its whole life, never shared, never rebound across time; stable 1:1 index↔cell mapping with `load_next`/`store_next` agreeing on the same backing; no index reachable from two live head↔links bindings over shared cells; `load_next` returns only `TAIL` or a currently-valid index from a DEDICATED link field (never payload-aliased); the same logical head every call. Keep the current hazard-inventory section as the explanatory appendix beneath the normative contract, not a replacement for it. Rewrite the `# Stability` "unsafe trait was considered and DECLINED" prose (`imp.rs:853-884`, the paragraph this exact ADR question reverses) to record the reversal and the reason: the decline was against `forbid`, and `forbid` is precisely the policy the owner chose to spend for structural closure.
3. `StackOps`, its blanket impl (`imp.rs`, currently ~line 1083), `ArrayIndexStack`'s (now `pub(crate)`-sealed, per Group A) push/pop, and all caller-facing API stay exactly as they are today — no signature changes, no new `unsafe` anywhere outside the one trait declaration.
4. `src/registry/heap_registry.rs:588`: `impl tagged_index_stack::StackStorage<16> for Registry` → `unsafe impl tagged_index_stack::StackStorage<16> for Registry`, with a `// SAFETY:` comment asserting the contract concretely for THIS impl (process-singleton `Registry`; `free_slots` is the head's only exposure; slot-resident `next_free` cells touched only through this one impl; pushed indices are `< MAX_HEAPS < INDEX_MASK` by construction — verify this bound is still accurate against current code before citing it, do not just copy the number). The `pub(crate)` loom-shim mirror trait (`src/registry/bootstrap.rs:552`, a wholly separate, crate-internal-only trait used only under `--cfg loom`) can stay safe — it was never a public extension point and this decision doesn't reach it; note that explicitly in a one-line comment so a future reader doesn't wonder why it wasn't converted too.
5. Test fixtures (`tests/custom_storage_impl.rs`): the five Group B implementors each become `unsafe impl` with a comment naming exactly which `# Safety` clause they deliberately violate — they convert from "safe code silently double-issues" demonstrations into pinning tests of the runtime guard's catch/miss boundary UNDER AN ACKNOWLEDGED-BROKEN CONTRACT (the honest framing this ADR's whole point was to reach). Any existing correct custom-storage example/fixture in the test suite becomes the reference model for what a CORRECT `unsafe impl` + `# Safety` comment looks like. #1809's new in-crate compile-pass fixture should mirror `Registry`'s actual shape: `unsafe impl`, calling only `push_index`/`pop_index`, never the three hooks directly — this is the cheap, in-crate stand-in for "the real external consumer still compiles" that doesn't require building the whole workspace in `tagged-index-stack`'s own CI.
6. Doc sync in the SAME change (not deferred): `tagged-index-stack`'s README + CHANGELOG (breaking change, unpublished 0.1.0 — record it plainly), and cross-check the root `sefer-alloc` crate's own unsafe-seam inventory (README §"Where unsafe lives — the complete list" + the `src/lib.rs` header mirror, per this workspace's own CLAUDE.md rule) for whether it needs to acknowledge the new dependency now carries one audited `unsafe` token of its own — read that rule's exact wording before editing it, this ADR does not attempt to settle whether IT is in scope for that inventory (a `tagged-index-stack` items is arguably out of the root crate's own inventory, which appears scoped to seams IN `src/`/`crates/` unsafe usage the root crate itself owns — verify, don't assume).

## What #1808 must still verify regardless of the plan above

Each of the six demonstrating tests gets an explicit, documented post-change status in #1808's own summary — no longer compiles (test #1, Group A), or now requires `unsafe impl` with a named violated clause (tests #2–#6, Group B) — stated plainly, not folded back into prose. #1808 must also confirm the compile-PASS side: a correct `unsafe impl` (the reference model referenced in step 5 above) compiles and its stack still behaves correctly end-to-end (push/pop round-trip, no spurious panics) — closing a hazard by breaking every legitimate implementor too is exactly the failure mode #1809's task description already warns against.

## Addendum (2026-09-01, later the same day): run-4's P1-1 item 3 — the unconstructible hook-access token — is now implemented

**What it closes.** This ADR's decision (via `@fh`) kept the three `StackStorage` hooks safe `fn`, on the rationale at line ~63 above ("every Group B hazard is implementor-side ... never caller-side misuse of an otherwise-safe method call"). The full external re-audit (run 5) **falsified that rationale**: in a `#![forbid(unsafe_code)]` crate, against a CORRECT downstream `unsafe impl`, the plain safe call `p.store_next(1, 3)` spliced a free-list cycle and made `alloc()` hand out the same index twice (attempt A4). This is finding P2-1 of `docs/reviews/2026-09-01-tagged-index-stack-full-audit-run5-fxx.md`; the run-4 review's P1-1 "what to fix" item 3 (an unconstructible access token) — which this ADR neither implemented nor explicitly rejected — is the item now implemented.

**The decision.** Owner approved the structural closure (the `Hook` witness), with the same recorded framing as this ADR's original decision: "we strive for perfection and are willing to break backward compatibility for it". The crate is still unpublished 0.1.0, so this is the cheapest this breaking change will ever be.

**What was implemented.**

- `pub struct Hook(())` — public type, PRIVATE field, no derives — unconstructible outside the crate by any spelling (tuple-struct `Hook(())` → E0423; struct-literal `Hook { 0: () }` → E0451).
- The three hooks now each take `_: &Hook`; a crate-private `const HOOK: Hook = Hook(());` is passed by the internal `pub(crate)` `SealedStorage` bridge.
- `SealedStorage` itself, `ArrayIndexStack`, and the root crate's `--cfg loom` mirror trait (a separate `pub(crate)` trait, never a public extension point) are unchanged.
- All downstream implementor signatures updated mechanically (incl. `sefer-alloc`'s `Registry`).
- Landing commits on branch `tis-audit-p2-1`: `142ec09` (trait + signatures), `dc13902` (the compile-fail fixture pair `tests/compile_fail/hook_token_unconstructible/` + `tests/compile_fail_hook_token_unconstructible.rs`).

**Why `&Hook`, not an owned token.** An owned non-Copy token could be stashed by a cooperating implementor into a `Cell<Option<Hook>>` and re-exposed through that implementor's own safe method, silently reopening the hole; the reference form makes the stash a lifetime error, and demanding `'static` instead would be E0308 against the trait's elided-lifetime signature — the audit report §P2-1 ("Recommended closure") holds the finding and the one-line rationale; the two stashing failure modes were verified by compiling the adversarial shapes (once before implementation, and re-attempted fresh against the landed implementation from a `#![forbid(unsafe_code)]` scratch workspace: `Cell<Option<&'a Hook>>` stash → "lifetime may not live long enough"; `&'static Hook` demand → E0308 "method not compatible with trait").

**Proof obligations met.** The fixture pins both forgery spellings: bare call → E0061 "this method takes 3 arguments but 2 arguments were supplied" with "argument #1 of type `&Hook` is missing"; forged witness → E0423 "cannot initialize a tuple struct which contains private fields"; the struct-literal spelling separately confirmed as E0451. The pre-implementation design was compiled and verified independently twice (audit run 5; the second-opinion review of the design). One `# Safety` consequence: clause 2 ("a `load_next` must observe the most recent `store_next` THE STACK ITSELF performed") is now dischargeable as written — no external party can perform a `store_next` at all, which was exactly P2-1's complaint against the old contract.

**Rationale status.** The line-63 form ("never caller-side") is superseded: it is true of the post-witness design only BECAUSE the witness makes the hooks unreachable from outside the crate. This addendum supersedes it on record.

## Addendum (2026-09-02): second reversal on this axis — the witness is replaced by `unsafe fn` hooks (task #1827)

**The decision.** Owner approved (2026-09-02, second-opinion consultation with `@fh`) replacing the Hook-witness design with: `Hook` deleted; `StackStorage` stays `unsafe trait`; the three hooks (`head`, `load_next`, `store_next`) become `unsafe fn`, each with its own CALLER-side `# Safety` clause; the crate-private `SealedStorage` bridge becomes the sole hook call site — three `unsafe {}` blocks, each with a `// SAFETY:` proof, under the crate's second item-scoped `#[allow(unsafe_code)]`. Source finding: review run 6, P1-1 — the review's blocking NO-GO (`docs/reviews/2026-09-02-091847-tagged-index-stack-review-Sol-codex-run-6.md`, a repository file cited in this ADR's header section). The crate is still unpublished 0.1.0 — the cheapest this breaking change will ever be.

**Why the witness was retired (the decisive finding).** Fabricating a `Hook(())` value is NOT an unsafe operation in Rust's model: it is an inhabited zero-sized type with exactly one representationally-valid value, and `mem::zeroed`/`transmute` produce it with zero unsafe-contract violation. E0423/E0451/E0061 close only the ORDINARY construction spellings; the "do not fabricate the witness" rule was prose with no `unsafe` operation to attach itself to — unenforceable by the compiler. `unsafe fn` attaches a real, compiler-checked caller-side contract to every call (E0133, "call to unsafe function is unsafe", on any call outside an `unsafe` block).

**Why the original line-61-65 rejection was based on since-outdated premises.** All three:

- **(a) The `GlobalAlloc`-shape correction.** The line-63 form choice contrasted "whole-trait `unsafe`, NOT `unsafe fn` on the three methods (the literal `GlobalAlloc` shape)" — but `core::alloc::GlobalAlloc` IS an `unsafe trait` whose methods (`alloc`/`dealloc`/`realloc`/`alloc_zeroed`) are ALL `unsafe fn`. The design adopted now IS the GlobalAlloc shape the original argument claimed to reject; that argument was backwards on its own terms.
- **(b) "never caller-side" — falsified by run-5.** The audit's P2-1 (a plain safe `p.store_next(1, 3)` spliced a cycle and double-issued) was conceded in the previous addendum ("Rationale status … supersedes it on record"); the witness was the first fix, this is the second.
- **(c) "~6 `unsafe {}` blocks in the blanket impl" — stale.** The `SealedStorage` bridge already funnels every hook call through ONE impl block, so the cost today is exactly three one-line `unsafe {}` blocks under a single item-scoped allow (the crate's two audited unsafe sites total: the trait declaration + bridge impl).

**The one load-bearing wording decision.** `load_next`'s caller-side contract is "the given index was pushed through THIS EXACT BINDING at least once (its link cell was initialised by a prior `store_next` through this binding)" — deliberately NOT "currently reachable/live": the pop loop calls `load_next` on an index it observed as head, but a concurrent popper may already have popped it before this caller's CAS lands (the CAS then fails and retries), so the stronger wording would make the crate's OWN algorithm violate its own hook's contract under contention.

**Compile-fail fixture swap.** `tests/compile_fail/hook_token_unconstructible/` (E0061 omitted-witness + E0423 forged-witness) removed; `tests/compile_fail/hook_call_requires_unsafe/` added (three bare hook calls → three E0133s; asserted by `tests/compile_fail.rs`, which also cites the compile-PASS counterpart pinned by `vec_backed_storage_push_pop_round_trips` + `push_pop_through_dyn_storage` in `tests/custom_storage_impl.rs` — the hooks are a barrier to misuse, not to legitimate use).

**TERMINAL shape.** This is intended as the TERMINAL shape for this boundary: no further Rust-type-system mechanism reaches further onto the caller side than `unsafe fn` does — a caller-side `# Safety` contract on an `unsafe fn` is the outermost caller-facing mechanism the language offers; the only stronger closure left would be removing the external seam entirely, which the original decision already rejected (it retires the crate's reason to exist). A future round should NOT reopen this axis a third time without NEW information (e.g., a language-level mechanism that does not exist today); documentation-only "closures" of the witness-fabrication class are precisely what this addendum rules out.

**What shipped.** This change set (crate `src/imp.rs` + docs); all `unsafe impl` implementor signatures updated, including `sefer-alloc`'s `Registry`; crate unsafe-count claims updated 1→2 sites; CHANGELOG supersedes the witness bullet.

## Addendum (2026-09-02, second same-day): `push_index` joins the unsafe boundary (task #1834)

**The decision.** Owner approved (2026-09-02, second-opinion consultation with `@fh`, the same "strive for perfection, willing to break compatibility" framing as both prior decisions in this ADR): `StackOps::push_index`, `ArrayIndexStack::push`, and the internal `push_index_impl` become `unsafe fn`; and the crate-private `SealedStorage::store_next` (trait + both impls) becomes `unsafe fn` too — so the bridge is a verbatim forwarder and the actual safety proof lives at the algorithm's call site inside `push_index_impl` ("my only caller is push_index_impl" is a privacy argument, not a proof). `pop_index`/`ArrayIndexStack::pop` stay safe. Source finding: review run 7 (`docs/reviews/2026-09-02-132254-tagged-index-stack-review-Sol-codex-run-7.md`), blocking P1-1 + grouped P2-1.

**NOT a third reversal on the hook axis.** `head`/`load_next`/`store_next`'s hook status, signatures, and caller-side contracts are unchanged from the previous addendum; `push_index` is a NEW member joining the unsafe boundary, not a re-litigation of the hooks. The previous addendum's TERMINAL-shape clause concerned the hook boundary and stands.

**The two-axis contract.** The new caller-side contract carries two clauses: (i) **DOMAIN** — the index has a real backing cell in the implementor's declared link domain (narrower, routinely, than the numeric range the `index < INDEX_MASK` guard admits — that guard is necessary for the head-word encoding but never sufficient proof of domain membership); (ii) **LIVENESS** — the index is not currently reachable (never pushed, or its most recent push was followed by a pop that actually returned it). The review's alternatives were rejected on posture coherence, not diff size. Model 1 (keep push safe; require memory-safety over the FULL numeric range): leaves `store_next` an `unsafe fn` resting on nothing memory-related — incoherent, and it unravels the hook decision the TERMINAL clause protects. Model 2 (a `contains_index`/`capacity` query): redundant once the caller already proves in-domain as part of the unsafe contract — a second, weaker copy of the same proof, plus an awkward checked-vs-unchecked cost decision pushed onto implementors. Model 3's "safe wrapper where the type can check" cannot exist generically for the crate's own blanket impl (no way to discharge liveness at all); the only safe push a caller can have is a downstream newtype privately owning every push call site — a documented recipe, not shipped API.

**The naming/scoping principle.** Push is unsafe because it is the exact analogue of `core::alloc::GlobalAlloc::dealloc` — "this index was issued to you (or is fresh, in-domain) and has not been handed back since"; dealloc is unsafe for precisely this reason. Pop is safe because an unauthorized pop can only LEAK an index, never double-issue one — it has no caller contract to carry.

**The normative contract, verbatim from the landed `src/imp.rs`** (`StackOps::push_index`'s `# Safety` section — two clauses plus frame sentence plus precision sub-clause):

> This is the caller-side unsafe contract, in two clauses; violating
> either is a soundness violation attributable to the caller — the
> same posture as [`core::alloc::GlobalAlloc::dealloc`], whose
> exclusive-issuance contract unsafe allocator code relies on.
>
> 1. **Link domain.** `index` must be in `self`'s LINK DOMAIN — the
>    set of indices for which this implementor owns a dedicated
>    backing cell, as the implementor documents it
>    ([`ArrayIndexStack<B, N>`](ArrayIndexStack)'s/[`ArrayLinks`]'s
>    domain is `0..N`; `sefer-alloc::Registry`'s is `0..MAX_HEAPS`).
>    The method's own `index < INDEX_MASK` guard (see `# Panics`) is
>    necessary for the head-word ENCODING and stays release-active
>    (same rationale as [`pop_index`](Self::pop_index)'s existing
>    clause-4 guard), but it is NEVER sufficient proof of domain
>    membership — a storage's domain may be (and routinely is)
>    narrower than the numeric range `INDEX_MASK` admits; the guard
>    observes only the numeric width. Do not conflate the numeric
>    guard with the domain obligation.
> 2. **Liveness (no double push).** `index` must NOT currently be
>    reachable through the head of any binding whose hooks touch the
>    same link cells as `self`'s: either `index` was never pushed
>    through such a binding, or its most recent push was followed by
>    a [`pop_index`](Self::pop_index) that actually RETURNED it, and
>    it has not been pushed again since. Precision sub-clause: a
>    concurrent popper that OBSERVED `index` as head but LOST its
>    CAS did NOT pop it and did not take ownership of it — such a
>    stale observer imposes no obligation on this push, and stale
>    content sitting in `index`'s link cell from an earlier push
>    cycle is irrelevant (the lazy-link/RAD-1 discipline: this
>    push's own [`store_next`](StackStorage::store_next) overwrites
>    it before the head CAS publishes `index`). Do not read this
>    clause as forbidding a lost-CAS observer.

**New trait clause 7 (atomic cells).** The trait `# Safety` contract gained a clause 7: a `store_next(i, ..)` can race with a stale popper's `load_next(i)` that will go on to lose its CAS, so a non-atomic implementor is UB even with every other clause honoured — previously only implied by the ordering contract.

**Allow-site inventory: EIGHT item-scoped tier-2 allows, all in `src/imp.rs`.** Verified with the crate's self-verifying grep (`grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' crates/tagged-index-stack/`): exactly eight hits, all `src/imp.rs` (lines 1050, 1232, 1357, 1386, 1434, 1624, 1766, 1859). Form choice: EIGHT item-scoped allows, NOT a single tier-1 seam module — consistent with the crate's existing two-site convention (each allow adjacent to its audited declaration with its own one-line reason), and appropriate because the boundary spans several distinct declarations — trait decls, impls, a free fn, an inherent method — that no single module naturally contains. The workspace's two-tier inventory and CLAUDE.md's self-verifying grep carry over unchanged.

**One change closes both findings.** Review run-7's P1-1 AND P2-1 are closed by this ONE change: P1-1 — the bridge could not prove a backing cell exists for a narrower storage — now the caller proves domain membership compiler-checkably before the call; P2-1 — the liveness rule was prose-only on a safe fn — now clause 2 of a real unsafe contract.

**What shipped.** `src/imp.rs`: `push_index`/`ArrayIndexStack::push`/`push_index_impl`/`SealedStorage::store_next` signature + contract change; ~52 test/bench call-site wraps with per-site SAFETY comments; the root `Registry` consumer (`push_free_slot`) unsafe-wrapped citing only release-active guards + the ACKNOWLEDGED EXPOSURE narrowing (external safe code can now only reach `pop_index`); the loom shim stays safe with divergence note 7; new compile-fail fixture `push_index_requires_unsafe` and miri unchecked-storage oracle `tests/narrow_domain_unchecked_storage.rs`; crate docs/README unsafe-site inventory updated 2→8.

## Addendum (2026-09-02, third same-day): the tag stops wrapping — a strictly monotonic tag with a stateless seal at the ceiling (task #1839)

**The decision.** Owner approved (2026-09-02, second-opinion architecture consult run before this task started) closing P1-1 from run-8's review
(`docs/reviews/2026-09-02-180547-tagged-index-stack-review-Sol-codex-run-8.md`)
with Option 2 in a specific shape — a strictly monotonic tag with a
stateless seal at the ceiling — plus Option 4 (a tiny-tag regression oracle
implemented by seeding the tag near the ceiling at the REAL tag width, not a
reduced-width cfg). `TaggedIndex::TAG_MAX` names the ceiling
(`2^TAG_BITS - 1`); `push_index`/`ArrayIndexStack::push` now return
`Result<(), TagExhausted>` instead of `()`, refusing a push that observes
`TAG_MAX` rather than bumping it to `2^TAG_BITS` (which the old code
truncated back to 0 via `pack_truncating`'s shift-discard — the wrap
mechanism this closes). Once refused, the head is SEALED: every later push
returns the same error, permanently; `pop_index` is unaffected and drains
the remaining chain. No reset/rotation API exists or is planned — see
`StackHead`'s "Sealing is permanent" doc section.

**The counterexample this closes, restated from the review.** Let head =
`(A, t)` with chain `A -> B -> TAIL`. Thread P begins a safe `pop_index`:
reads `(A, t)`, then `next[A] = B`, and stalls before its CAS. Thread Q
legitimately pops A then B (stack empty, same tag `t`); Q holds B. Q then
does `2^TAG_BITS - 1` cycles of `push(A); pop(A)` (each a genuine
in-domain, not-currently-live push — the precision sub-clause of clause 2
above explicitly permits re-pushing after a stale-observer's lost CAS), then
one final `push(A)` without popping. Every single push in that sequence is
individually contract-compliant under BOTH of `push_index`'s existing
clauses — neither the link-domain clause nor the liveness clause is ever
violated — yet after exactly `2^TAG_BITS` total pushes the head reads `(A,
t)` again: numerically IDENTICAL to what P captured before it ever stalled.
P's stale CAS then succeeds, handing P index A while B — still legitimately
Q's — becomes reachable through the corrupted chain, exposed to a second
`pop_index` winner. This is a genuine violation of the exclusive-issuance
promise `StackStorage`'s `# Safety` section makes to allocator consumers,
reachable without either caller-side clause being broken — the finding this
task exists to close.

**Why Option 2 in this shape, not Option 1 or Option 3 (second-opinion
rationale, recorded for this task's own record).** Option 1
(hazard/epoch-style reclamation folded into the protocol) was rejected as
architecturally wrong for this crate: `no_std`, allocation-free, and
const-generic over index width, with no clean way to provide per-thread
announcement slots; it would additionally add a fence/RMW to every `pop` to
guard against an event that (pre-fix) needed a full tag cycle to become
reachable at all, and it makes the tag mechanism itself redundant once
quiescence is enforced structurally. Option 3 (downgrade the promise to a
documented probabilistic risk, unchanged behaviour) was rejected because "no
pop stays in-flight across a full tag cycle" is a timing property no caller
can bound or discharge with a real `// SAFETY:` proof — it would have moved
the review's NO-GO from a soundness gap to an unenforceable caller
obligation, not closed it. Option 2 loses ZERO sound behaviour: every
execution that was sound before this change stays byte-identical after it
(the tag bump, the CAS orderings, H-2, and RAD-1 are all untouched) — the
ONLY executions that change are ones that were CURRENTLY undefined
behaviour under the old contract. Precedent cited during the consult:
`Arc::clone` aborts at `isize::MAX` for the identical reason — a wrapped
refcount is a use-after-free, so std chose terminal-and-loud over
reclamation there too; this crate's seal is the `Result`-returning
(non-aborting) analogue, appropriate because a stack push already has a
natural place to surface a `Result` that `Arc::clone` does not.

**Check placement and its RAD-1 interaction.** The `tag == TAG_MAX` check
runs immediately after `unpack`, BEFORE `store_next` — so a first-attempt
refusal has no side effect at all. A refusal on a CAS *retry* may leave
stale content sitting in `index`'s link cell from an earlier iteration of
that same retry loop; the existing RAD-1 discipline already covers this (a
link cell is meaningless until a successful push republishes it), so this
is not a new hazard the seal introduces — see `push_index`'s `# Errors`
section, which states this explicitly rather than overclaiming "refusal has
zero side effects in all cases."

**`TAG_MAX` off-by-one, pinned two ways.** `TaggedIndex::pack(_, TAG_MAX)` is
`Some`; `pack(_, TAG_MAX + 1)` is `None` — asserted directly in
`tests/tag_seal.rs`'s `tag_max_is_the_exact_pack_ceiling`, alongside a
`const _: () = assert!(TAG_MAX == (1u64 << TAG_BITS) - 1)` compile-time pin.

**Option 4: the tiny-tag oracle seeds near the ceiling, never reduces
`TAG_BITS`.** New `#[doc(hidden)]`, `test-internals`/loom-gated
constructors — `StackHead::with_tag_for_test(tag)` (built via
`AtomicU64::new(pack_truncating(..))`, i.e. INITIALISATION, not a live-head
mutation, so the release-sequence invariant on `head` is untouched),
`ArrayIndexStack::with_tag_for_test(tag)`, and the write-side twin of the
existing `load_next_for_test` — `ArrayIndexStack::store_next_for_test`.
`tests/tag_seal.rs` pins the single-threaded `Ok, Ok, Err` sequence at the
ceiling, `pushes_remaining()`'s readback, a byte-identical `raw_head()`
across a first-attempt refusal, that pops keep draining after a seal, and
that the seal survives a full drain (permanent, no reset).
`tests/loom_aba.rs`'s new "(h) Tiny-tag seal" section replays the review's
exact stale-observer-plus-churn schedule at the REAL 48-bit-plus tag width,
seeded a handful of pushes short of `TAG_MAX`: the FIXED test
(`tiny_tag_seal_rejects_stale_cas_at_the_real_width`) drives Q's churn
through the real, sealing `push` and confirms both that the seal engages
(`Err(TagExhausted)`) and that P's stale CAS is rejected; the counterfactual
(`counterfactual_bypassed_seal_lets_stale_cas_double_issue`, `#[should_panic]`)
hand-inlines what the OLD wrapping `push` would have installed for Q's one
final step — via `store_next_for_test` + a raw `cas_head_for_test`,
bypassing the `TAG_MAX` check entirely — and proves P's stale CAS then
SUCCEEDS and the free-list conservation check FAILS, the load-bearing proof
that the seal (not just the tag bump) is what closes P1-1. That
counterfactual's raw CAS installs the EXACT tag P observed, not a literal
`(index, 0)`: a single real `wrapping_add(1)` past `TAG_MAX` truncates to
tag 0 (see `pack_truncating`'s doc), but reaching P's stale tag again
through real pushes needs an entire `2^TAG_BITS`-push lap — the actual
arithmetic content of "wrap" is "returns to the exact starting tag after one
full cycle," which the raw CAS installs directly, collapsing the
infeasible-to-run lap into its end state.

**Consumer update: `sefer-alloc::Registry`.** `push_free_slot`
(`src/registry/heap_registry.rs`) now matches on `push_index`'s `Result`;
on `Err(TagExhausted)` the slot is already `FREE` (the state CAS already
ran) but the index never rejoins `free_slots` — an intentional, documented
leak of exactly ONE slot, not a panic and not propagated as an error,
because panicking in an allocator's free path is worse than losing one slot
this deep into a `2^48`-push tag lifetime (this module's own "ABA defence"
budget analysis puts the earliest reachable point at ~3.3 days of
continuously saturated pushes onto ONE `free_slots` head). The loom shim
(`src/registry/bootstrap.rs`'s `loom_shim::StackOps::push_index`) mirrors
the real seal check exactly (same placement, same `Result` signature) — NOT
listed as a new divergence note, because it isn't one; divergence note 5
(the checked-vs-private-truncating `pack`) is updated to note the tag-wrap
mask it used to need is now dead code, since the real protocol itself never
produces an out-of-range tag anymore.

**What shipped.** `crates/tagged-index-stack/src/imp.rs`: `TaggedIndex::TAG_MAX`,
`TagExhausted`, `StackHead::pushes_remaining`/`with_tag_for_test`,
`ArrayIndexStack::with_tag_for_test`/`store_next_for_test`, the seal check in
`push_index_impl`, and the `Result`-returning signature change propagated
through `StackOps::push_index`/`ArrayIndexStack::push`; `src/lib.rs` and
`README.md`'s wrap-risk framing rewritten as a strictly-monotonic-tag /
pushes-until-sealed-lifetime framing; new `tests/tag_seal.rs`; new "(h)"
section in `tests/loom_aba.rs`; every in-workspace call site (root
`sefer-alloc`'s `Registry::push_free_slot` + its loom shim, and every
tagged-index-stack test/bench/example) updated to handle the `Result` in the
same change; `CHANGELOG.md` (crate-local) gains a new `BREAKING (unpublished
0.1.0)` bullet under `### Changed`.
