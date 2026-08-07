# rust-cc-audit report — `tagged-index-stack`

**Date:** 2026-08-07
**Produced by:** the `/rust-intel` skill's fan-out audit workflow (`audit-project.workflow.js`,
run `wf_f39c9b3c-d84`) — 14 agents total. ~969k subagent tokens, 79 tool calls.
**Audited tree:** `main` @ current HEAD, `crates/tagged-index-stack`.

---

**Scope:** D:\dev\rust\sefer-alloc\crates\tagged-index-stack
**Pinned versions:** loom = "0.7" (cfg(loom)-gated, the crate's only non-std dependency; zero [patch] tables, zero git deps, no build.rs)
**Found:** 0 critical, 2 high, 2 medium, 7 info

---

## CRITICAL

none

## HIGH

### [§B13] crates/tagged-index-stack/src/lib.rs:453 — pop's CAS failure ordering is Relaxed; the retry reads links through a stale head

*(also flagged by §F2, semantics-and-conformance — documented-guarantee divergence)*

Citation: `.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Relaxed)` — on `Err(actual) => head = actual`, the loop re-runs `let next = links.load_next(index);` (line 444) for the index taken from the Relaxed-loaded word. The comment at :442-443 justifies the link read with "the push stored it under Release; our Acquire load of head + this Acquire read see it" — which holds only for the first iteration.

Why: pop's CAS *failure* is a pure Relaxed load with no happens-before to the concurrent push's Release CAS (release sequences don't rescue a relaxed load), so the retry may read a STALE `links` value for the new head while the CAS still succeeds against the fresh head word — installing a wrong `next` and losing/duplicating free-list indices. In the parent allocator a duplicated index is a double-allocated slot, directly violating the README's "loss/duplication-free" ABA guarantee. Invisible on x86 (TSO) and masked on ARMv8 hardware (ldaxr/casa acquire the failure load anyway), and unreachable by the shipped loom scenarios (racing link writes happen-before spawn; the split-pop probe is single-shot with no retry) — so the passing loom suite does not refute it. push's failure Relaxed (line 412) is fine: its retry dereferences nothing.

Fix: change pop's failure ordering to `Ordering::Acquire` (one word), update the :442-443 comment to state that the Acquire *observation* of head (initial load or failure load) is what validates the link read, and add a loom scenario driving a pop CAS-failure retry against a concurrent fresh push to pin the ordering.

### [§D1a] crates/tagged-index-stack/tests/loom_aba.rs:213 — untagged-ABA counterfactual panics for the wrong reason (harness accounting bug, not real corruption)

Citation: `if a_result.is_ok() { popped.push(0); }` — `a_result` is `Ok(prev_head)`, i.e. the actually-popped index, and it is discarded in favor of a hardcoded `0`.

Why: in the pop-then-repush-same-index schedule, `next[0]` never diverges from A's stale snapshot, so the untagged 2-slot stack is never actually corrupted — the "free-list corrupted" panic fires exclusively on interleavings where A really popped index 1 but the harness recorded 0 (`popped == [0,0]`). The `#[should_panic]` the crate cites as its headline non-vacuousness proof for the ABA tag therefore proves a harness bug, not that the tag is load-bearing (§D1a shape 3: invalid negative control).

Fix: record the real popped index (`let Ok(prev) = a_result → popped.push(prev)`), then repair the scenario so genuine untagged ABA corruption is reachable — e.g. thread B pops 0 AND 1, re-pushes only 0, leaving A's stale `next=1` dangling so A's stale CAS resurrects an index B still owns. Verify the fixed counterfactual still panics under loom and the tagged test still forces the retry.

## MEDIUM

### [§B26] crates/tagged-index-stack/src/lib.rs:383 — push's only index-validity guard is debug_assert-only; release-mode violation silently corrupts the free list

Citation: `debug_assert!((index as u64) < TaggedIndex::<INDEX_BITS>::INDEX_MASK, "index must be < INDEX_MASK...")`.

Why: this is the ONLY guard against pushing the empty sentinel / an out-of-width index, and the crate's own doc (lib.rs:373) says a release-mode violation "corrupts the head word" — downstream, duplicate slot indices (aliased slots) in the consuming allocator. ArrayLinks' bounds check only catches `index >= N`, not the sentinel case. §B26: an invariant that must hold in production belongs in `assert!`, not `debug_assert!`.

Fix: promote to full `assert!` (one predictable compare-and-branch, negligible next to the CAS-loop atomics), or return a Result; if the debug_assert is kept as a deliberate hot-path choice, say so explicitly in `# Panics` with the release-mode corruption consequence named.

### [§D1] crates/tagged-index-stack/tests/stack_unit.rs:127 — F1 regression test cannot fail: catch_unwind without message pinning is satisfied by an unrelated OOB panic

*(also flagged by §B26, data-and-types — profile-dependent test: the asserted panic is debug_assert-only, so the test's meaning diverges under `cargo test --release`)*

Citation: `let result = std::panic::catch_unwind(... stack.push(&links, TAIL)); assert!(result.is_err(), "pushing index == TAIL must panic (debug_assert)...")`.

Why: with no panic-message pinning, deleting the debug_assert guard (or compiling it out in release) sends `push(&links, TAIL)` into `ArrayLinks<4>::store_next(TAIL, …)` → `self.next[u32::MAX as usize]` → a guaranteed slice out-of-bounds panic, which is *also* caught — the test stays green with or without the guard it exists to pin. The counterfactual "what mutation would this catch?" answer is none.

Fix: downcast the caught panic payload and assert it contains the guard's message substring ("index must be < INDEX_MASK") so an OOB panic no longer satisfies the test; gate the TAIL-push half with `#[cfg(debug_assertions)]` (keeping the INDEX_MASK==TAIL and valid-push assertions unconditional) and add a release-profile companion documenting the guard is debug-only.

## INFO

### [§A3] crates/tagged-index-stack/src/lib.rs:467 — `raw_head` is unconditionally pub while its own doc disclaims it as tests/diagnostics-only

Citation: `/// The raw packed head word (Acquire) — for diagnostics/tests only. ... pub fn raw_head(&self) -> u64`. A pub fn in a publish-intended crate is a semver commitment that its own rustdoc disavows (unlike `cas_head_for_test`, which is cfg(loom)-gated and genuinely unreachable downstream). Fix: own it as a real public diagnostic API (drop the "tests only" framing), or narrow it — `#[doc(hidden)]` per the project's test-only-forwarder pattern, or a bench/test feature gate. Decide before first publication; removing it later is a breaking change.

### [§B26] crates/tagged-index-stack/src/lib.rs:199 — `_CHECK_BITS` width guard is forced only via `pack()`

Citation: `const _CHECK_BITS: () = assert!(INDEX_BITS >= 1 && INDEX_BITS <= 32, ...);` forced only by `let () = Self::_CHECK_BITS;` inside `pack`. `unpack()`/`INDEX_MASK`/`is_empty` at INDEX_BITS in 33..=63 compile and run unguarded (INDEX_MASK then exceeds u32::MAX — the pre-F1 hazard class). Honestly documented in-source; all real stack use routes through pack, so this is a residual gap, not a defect. Fix: reference `_CHECK_BITS` from `INDEX_MASK`'s own initializer so every mask-touching item forces the guard — zero runtime cost, and it structurally closes the acknowledged no-trybuild coverage gap (tests/stack_unit.rs:137-143).

### [§C1] crates/tagged-index-stack/src/lib.rs:277 — `pub trait Links` lacks an explicit sealed-vs-open statement

Citation: `pub trait Links { fn load_next(&self, index: u32) -> u32; fn store_next(&self, index: u32, next: u32); }`. Openness is deliberate and correct (slot-resident links in caller storage are the design point), but undeclared — and any future defaultless method is semver-major for every downstream implementor. Fix: one doc line: "This trait is intentionally OPEN to external implementation; new methods will only ever be added with default bodies (or via a major version bump)." No code change.

### [§C10] crates/tagged-index-stack/Cargo.toml:28 — loom = "0.7" pinned independently in three manifests instead of [workspace.dependencies]

Citation: `loom = "0.7"` also spelled at workspace root Cargo.toml:949 and crates/racy-ptr-cell/Cargo.toml:28. A future bump in one member alone creates the drifting-version / two-linked-copies hazard. Fix: declare once in `[workspace.dependencies]`, inherit with `loom = { workspace = true }` in each member's `[target.'cfg(loom)'.dependencies]`.

### [§F2] crates/tagged-index-stack/src/lib.rs:214 — pack doc claims tag-bit collision the code's mask prevents

Citation: doc says "`index` MUST be `< 2^INDEX_BITS`; a wider value silently collides with the tag bits" — but the code is `(tag << INDEX_BITS) | (index & Self::INDEX_MASK)` (:222): an over-wide index is silently *truncated* (wrong index round-trips out, tag intact), never colliding with the tag. A reader auditing tag integrity from the doc alone draws the wrong conclusion. Fix: make doc and code agree — either document the truncation, or drop the mask and keep the documented contract.

### [§F2] crates/tagged-index-stack/README.md:50 — tag-budget comparison silently switches push rates (~1000× inflated contrast)

Citation: "at an unrealistic 100k pushes/sec that is **~89 years** ... A 32-bit tag, by contrast, gives only ~43 s" (same claim in src/lib.rs:91-96). 2^32 at the SAME 100k/s is ~42,950 s ≈ 12 hours, not ~43 s — the 43 s figure implies ~100M pushes/sec. The structural conclusion (48-bit ample, 32-bit hazardous) survives, but the stated contrast rests on an unstated rate switch. Fix: recompute both figures at one stated rate (~89 years vs ~12 hours at 100k/s) in README §"Tag-width budget" and the matching lib.rs crate-doc paragraph.

### [§F4] crates/tagged-index-stack/tests/stack_unit.rs:20 — pack/unpack round-trip tested over hand-picked literals only; no proptest

Citation: `pack_unpack_round_trip_16` iterates `&[0u64, 1, 2748, 0xFFFE]` × `&[0u64, 1, 12345, (1u64 << 48) - 1]` — widths 16/20/32 only; widths 1..=31 otherwise untested (including degenerate INDEX_BITS=1). Boundary values including the 2^48 wrap ARE covered, so this is the mild form of the literals-only ban. Fix: add one proptest round-trip property (index in 0..INDEX_MASK, tag in 0..2^TAG_BITS, a few widths incl. 1 and 31, ~64 cases per the repo's fast-proptest convention). Low priority.

---

## Post-flight summary

**§B13 Relaxed-publish (reader-side Relaxed then payload read):**
- src/lib.rs:453 — **VIOLATED** (pop's CAS failure Relaxed; retry reads `links.load_next` through the Relaxed-loaded head — HIGH finding above)
- src/lib.rs:412 — justified (push's failure retry uses the value only as plain integers; no payload read; CAS revalidates)
- tests/loom_aba.rs:96 — justified (single-shot split-pop probe; Err result discarded)
- tests/loom_aba.rs:158 — justified (UntaggedStack counterfactual pop; deliberately-buggy test-only model)
- tests/loom_aba.rs:172 — justified (UntaggedStack counterfactual push; same test-only model)
- tests/loom_aba.rs:198 — justified (thread A single-shot CAS, untagged counterfactual; no retry, no payload read)
- tests/loom_aba.rs:311 — justified (thread A single-shot stale CAS in H-2 harness; nothing dereferenced through it)
- tests/loom_aba.rs:346 — justified (`bug_pop_drain_to_empty` intentional pre-H-2-fix bug model, loom-only)

**§F1/§F2 documented-guarantee divergence (wire/security/persisted):**
- src/lib.rs:453 — **violated** (C11-model-level: Relaxed failure ordering breaks the documented Acquire-visibility argument; free-list duplication = double-allocated slot downstream — merged into the §B13 HIGH finding)
- src/lib.rs:214, README.md:50 — justified as 🟡 not 🔴 (doc-accuracy defects, no wire/security/persistence impact — INFO findings above)
- src/lib.rs:115-140 (§F1 conformance to named references) — verified clean (Treiber-with-tag matches; all four cited no-64-bit-atomics targets confirmed via `rustc --print cfg`; compile_error! guard present)

**§B21 tokio::spawn with dropped JoinHandle:** none (zero tokio anywhere; adjacent loom `thread::spawn` sites at tests/loom_aba.rs:85,103,190,202,275 all justified — every handle joined at :109-110, :208-209, :314)

**§B22 impl Drop doing async work:** none (zero `impl Drop` in src/ or tests/, two agents grep-verified)

**§B15b Pin::new_unchecked:** none (zero `Pin` tokens; additionally foreclosed by `#![forbid(unsafe_code)]` at src/lib.rs:125)

**§B14 unbounded channel / FuturesUnordered:** none (no channels, no async, no keyed insert-only collections)

**§B12 cryptographic operations:** none (zero crypto/TLS/JWT/KDF/RNG code; pattern sweep clean)

**§B24 `==` on secret material:** none (all `==`/`!=` operands are public protocol constants — TAIL, INDEX_MASK, packed head words, test flags)

**§B5 unsafe / transmute / mem::uninitialized / mem::zeroed:** none (crate-wide `#![forbid(unsafe_code)]`, src/lib.rs:125; grep clean)

**§B18 manual `unsafe impl Send`/`Sync`:** none (Send/Sync derived automatically from std/loom atomic types)

**§B18a wrong/absent PhantomData on raw-pointer wrapper:** none (no type holds `*const`/`*mut`/`NonNull`; TaggedIndex is a ZST over plain integers, documented strict-provenance-clean)

**§B25 extern "C" / Box::from_raw / from_raw_parts:** none (no extern blocks, no_mangle, or raw-parts constructors; zero non-std deps)

**§B25a FFI across threads without cited thread-safety contract:** none (no C library called; all cross-thread activity on Rust std/loom atomics, loom-model-checked)

**§A1 unverified/unnamed dependency (slopsquatting):** crates/tagged-index-stack/Cargo.toml:28 — justified (`loom = "0.7"` is the real tokio-rs/loom registry crate, cfg(loom)-gated, deliberate and documented; API usage verified against the 0.7 surface)

**§A1 unpinned [patch]/git source:** none (zero `[patch]` tables, zero `git =` deps workspace-wide)

**§A1 network in build.rs:** none (crate has no build.rs)

**§C1 blanket impl in public API of a published library:** none (all six impl blocks — :178, :296, :318, :324, :346, :492 — target concrete local types parameterized only by const generics)

**§F3 leaked/unclosed boundary resource:** none (no_std, allocation-free, no I/O, no connections, no OS resources)
