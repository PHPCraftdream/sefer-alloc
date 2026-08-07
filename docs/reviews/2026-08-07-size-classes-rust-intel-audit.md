# rust-cc-audit report — `size-classes`

**Date:** 2026-08-07
**Produced by:** the `/rust-intel` skill's fan-out audit workflow (`audit-project.workflow.js`,
run `wf_5ce47cc9-736`) — 14 agents total. ~832k subagent tokens, 67 tool calls.
**Audited tree:** `main` @ current HEAD, `crates/size-classes`.

---

**Scope:** D:\dev\rust\sefer-alloc\crates\size-classes
**Pinned versions:** MSRV 1.88, edition 2021; zero runtime dependencies; sole dev-dependency `proptest = "1"`
**Found:** 0 critical, 0 high, 4 medium, 8 info

---

## CRITICAL

(none)

## HIGH

(none)

## MEDIUM

### [§B26] crates/size-classes/src/lib.rs:176-180 — Geometric-advance overflow silently masked by the min-step fallback

`let mut next = (cur * num).div_ceil(den); ... if next <= cur { next = cur + min_block; }` — bare `cur * num` on a geometrically-accumulating value: in a release-profile runtime call (the fn is `pub` and the tests call it at runtime) an overflow silently wraps, and the `next <= cur` min-step fallback then MASKS the wrap into a valid-looking strictly-increasing table with min_block steps instead of the requested geometry — debug panics, const-eval errors, release silently returns wrong geometry; the downstream monotonicity check in `build_size2class` cannot catch it because the masked table is still strictly increasing. This is a library, so per §B26 it cannot assume the consumer sets `overflow-checks=true`.

**Fix:** Use `cur.checked_mul(num).expect("geometric progression overflows usize — reduce geo_count/growth")` (and checked round-up `next.checked_add(mask)`); in const context this still becomes a compile error, but with a diagnostic, and runtime release callers get a panic instead of a silently degraded table.

### [§C1a] crates/size-classes/src/lib.rs:52 — All-pub-field `Params` lacks `#[non_exhaustive]`/constructor decision before first publish

`#[derive(Debug, Clone, Copy)] pub struct Params<'a> { pub min_block: usize, ... pub huge_threshold: usize }` — all-pub-field config struct in a publish-ready crate, constructed by consumers via struct literal, with no `#[non_exhaustive]` and no constructor. Adding any future policy field (plausible: the crate doc itself anticipates growth, and `small_align_max` is currently hardwired to `min_block` inside `build()`) is a semver-major break for every downstream struct-literal; §C1a says this must be decided at first publication because retrofitting `#[non_exhaustive]` later is itself a breaking change.

**Fix:** Decide before first publish: either (a) add `#[non_exhaustive]` to `Params` AND ship a const-compatible construction path in the same commit (a `const fn Params::new(...)` or a const-chainable builder — plain `#[non_exhaustive]` alone makes the type unconstructable downstream, and const context has no Default/FRU escape hatch), or (b) explicitly declare the shape frozen with an inline comment stating that any future knob will arrive as a new sibling type/major bump. Option (a) is the module's default assumption for a policy/config struct.

### [§D1] crates/size-classes/tests/builder.rs:191 — Ambiguous `should_panic` substring can be satisfied by the setup path

`#[should_panic(expected = "strictly increasing")]` matches BOTH "Params::extras: must be strictly increasing" (lib.rs:144, `build_table`) and "table must be strictly increasing" (lib.rs:234, `build_size2class`). The test's setup calls `build_table` before the SUT (`SizeClasses::build` → `build_size2class`); the shared substring means a spurious panic from the setup path would coincidentally satisfy the expectation, so the chokepoint the test exists to pin could regress silently.

**Fix:** Pin the site-specific substring: `expected = "table must be strictly increasing"` (the `build_size2class` message), which cannot be produced by `build_table`'s extras check.

### [§F2] crates/size-classes/src/lib.rs:408 — `class_for` fast path silently violates its own documented fit predicate for non-pow2 aligns

"A class fits iff its `block_size >= max(size, align)` AND `block_size % align == 0`" (lib.rs:387-391) vs fast path `if align <= self.small_align_max { return Some(seed); }` (lib.rs:414-416). For a non-power-of-two align in (1, min_block] (e.g. align=3/6/12 with min_block=16) the fast path returns a class whose block is NOT divisible by align — silently violating the fn's own documented fit predicate, which is exactly the digest's named security-relevant failure mode (misclassification); the pow2-align precondition is stated only in an internal slow-path comment (lib.rs:417-418, "the `Layout` contract"), never in `class_for`'s public contract, and no test generates a non-pow2 align (both oracles are only ever fed pow2s). Practical exposure is low because any `Layout`-derived align is pow2. Related: the §B26 info finding at lib.rs:432 covers the same unchecked precondition on the slow-path jump.

**Fix:** State the precondition in `class_for`'s rustdoc ("`align` must be a power of two — the `Layout` contract; non-pow2 aligns are outside the contract and may return a non-conforming class") and/or add `debug_assert!(align.is_power_of_two())`; optionally add a proptest negative-control documenting the intended behavior for out-of-contract aligns.

## INFO

### [§B1b] crates/size-classes/src/lib.rs:52 — Public lifetime on `Params<'a>` is justified (recorded, not residual)

`pub struct Params<'a> { ... pub extras: &'a [usize], ... }` — flagged only to record it was checked: in a no_std, zero-alloc, const-fn crate a borrowed slice is the only representable form for `extras`, and the const-context zero-copy design goal is explicitly documented in the crate-level docs (the §B1b escape condition). In practice consumers use `&'static` promoted slices, as both test files do.

**Fix:** No code change needed. Optionally note on `Params::extras` that `'a` is typically `'static` in const usage.

### [§B26] crates/size-classes/src/lib.rs:176 — Growth denominator never asserted non-zero

`(cur * num).div_ceil(den)` — `Params::growth.1` (den) is caller-supplied and never asserted non-zero, unlike the sibling preconditions (min_block power-of-two, geo_count > 0, extras shape) which are all machine-checked with named messages — `den == 0` panics with a bare divide-by-zero instead of a diagnostic naming the bad param; `num == 0` silently degrades to a linear min_block-step table via the min-step clause.

**Fix:** Add `assert!(params.growth.1 > 0, "growth denominator must be > 0")` (and optionally `growth.0 >= growth.1` or `> 0`) alongside the existing param asserts at the top of `build_table`.

### [§B26] crates/size-classes/src/lib.rs:88 — `size2class_len` is the one pub fn with zero param validation

`max_class / min_block + 1` — `min_block == 0` hits an unguarded integer division (panics in both profiles, bare message) even though every sibling entry point asserts `min_block.is_power_of_two()` with a named message — inconsistent chokepoint for the same precondition.

**Fix:** Add `assert!(min_block.is_power_of_two(), "min_block must be a power of two")` to `size2class_len`, matching `build_table`/`build_size2class`.

### [§B26] crates/size-classes/src/lib.rs:432 — Slow-path bitmask round-up's pow2-align precondition unchecked even in debug

`let next_mult = (block | (align - 1)) + 1;` — the jump's bitmask round-up is only correct for power-of-two align (the comment cites "the Layout contract") but the precondition is not machine-checked even in debug: a non-power-of-two align > min_block silently overshoots multiples (e.g. align=48, block=64 → next_mult=112, skipping 96) and can return a non-minimal class or None where a fitting class exists — a silent wrong answer, not a panic, and the proptest suite only generates pow2 aligns so it can never catch a violating caller. Companion to the §F2 medium finding at lib.rs:408.

**Fix:** Add `debug_assert!(align.is_power_of_two())` at the top of `class_for`'s slow path (free in release, catches contract violations where the tests run); the debug_assert-vs-assert choice is deliberate per §B26 since the failure mode is suboptimal-fallback, not memory unsafety.

### [§D1] crates/size-classes/tests/builder.rs:236 — `is_huge` test comment promises a two-threshold proof but builds one scheme

`// Two different thresholds → two different verdicts for the same size, proving it is parameterized.` — but the body builds only `P_SMALL` (huge_threshold: 1024); an `is_huge` hardcoded to 1024 would pass, so the stated claim is asserted in prose, not by the test. The >= boundary pin (1023 vs 1024) is real, so the test is not vacuous — only under-delivering on its comment.

**Fix:** Build a second const scheme with a different huge_threshold (e.g. 4096) and assert the same size gets opposite verdicts across the two schemes, matching the comment.

### [§D1a] crates/size-classes/tests/builder.rs:16 — Table oracle is a line-for-line transcription of the implementation (also flagged by §F1)

`reference_table`'s core is `let mut next = (cur * num).div_ceil(den); next = (next + mask) & !mask;` — byte-identical to `build_table`'s lib.rs:176-177 (circular-oracle shape 1 / §F1 BANNED: verifying against your own implementation only): it proves const-eval ≡ runtime-eval of one formula, so a shared misconception in the rounding/spacing formula is structurally unobservable. Mitigated by independent structural asserts (strict increase, min_block multiplicity, extras containment, first == min_block), by the genuinely independent `scan_class_for`/`reference_class_for` classifier oracles, and by the semantics agent's verification that div_ceil-then-round-up equals round-up of the exact rational (code matches the prose spec).

**Fix:** Add a handful of hand-derived golden entries (e.g. the first 8 classes of the (5,4)/16 scheme, or the 16→32→48→…→258_752 run the README already pins) or a spec-level property (each class ≥ round_up(prev*5/4, 16), step ≥ min_block) that does not reuse the implementation's expression tree.

### [§F2] crates/size-classes/src/lib.rs:283 — Struct-level "no panics on the lookup path" claim contradicted by `block_size`'s own `# Panics`

"All query methods are `const` pure arithmetic — no allocation, no panics on the lookup path." vs `block_size`'s "# Panics: Panics if `idx >= N`" (lib.rs:369-372). Two doc statements on the same type disagree (§F2 REQUIRED: when docs and code already disagree, report it); additionally `class_for` with size=0/align=0 — outside the Layout contract — underflows `need - 1` into an out-of-bounds panic at runtime.

**Fix:** Qualify the struct-level claim: "no panics on the lookup path for in-contract inputs (`size >= 1`, pow2 `align`, `idx` from `class_for`)".

### [§F2] crates/size-classes/README.md:9 — README understates the machine-checked extras preconditions

"an arbitrary sorted list of explicit extra classes" — extras must be strictly increasing, each a multiple of min_block, and disjoint from the geometric run (const-eval panics in `build_table` lib.rs:134-149 / `build_size2class` lib.rs:229-239); the divergence fails LOUDLY at compile time (hence info, not a silent-guarantee red), but a reader of the README alone would expect e.g. [100, 200] to be accepted.

**Fix:** README wording: "a strictly increasing list of min_block-multiple extra classes (violations are compile errors)" — matching the `Params::extras` rustdoc, which already says this correctly.

---

## Post-flight summary

Aggregate 🔴 inventory across all agents (zero 🔴 findings crate-wide):

- **§B21 tokio::spawn with dropped JoinHandle** — crates/size-classes (all 3 files): N/A, zero occurrences; no async runtime, no tokio, no spawn calls (inventoried independently by both async agents).
- **§B22 impl Drop doing async work** — crates/size-classes (all 3 files): N/A, zero `impl Drop` blocks; all types are Copy plain data (inventoried by both async agents).
- **§B15b Pin::new_unchecked** — crates/size-classes (all 3 files): N/A, zero occurrences; `#![forbid(unsafe_code)]` crate, no Pin usage (grep hits are doc prose only) (inventoried by both async agents).
- **§B13 Relaxed-publish data race** — crates/size-classes/ (all 3 source files): N/A — zero atomics anywhere (no `core::sync::atomic`, no `Ordering` tokens).
- **§B14 unbounded channel / FuturesUnordered / unbounded admission / insert-only keyed collection** — crates/size-classes/ (all 3 source files): N/A — zero channels, spawns, Semaphores, or collections; fixed-size const arrays only.
- **§B12 any cryptographic operation** — src/lib.rs:1-441 + both test files: N/A — zero crypto; no keys, nonces, hashes, RNG, TLS, or JWT.
- **§B24 `==` on secret material** — src/lib.rs (:118, :139, :244), tests/proptest_builder.rs:131-160: N/A — every equality operates on public usize block sizes / class indices.
- **§B5 unsafe / transmute / mem::uninitialized / mem::zeroed** — src/lib.rs:43: N/A — `#![forbid(unsafe_code)]` makes any unsafe token a compile error; grep confirms zero hits.
- **§B18 manual unsafe impl Send/Sync** — src/lib.rs:43: N/A — no impl Send/Sync anywhere; auto-derived is sound for `[usize; N]`/`[u8; L]`/usize fields.
- **§B18a raw-pointer wrapper variance / PhantomData** — src/lib.rs:285: N/A — no `*const`/`*mut`/`NonNull` field, no PhantomData; `Params<'a>`'s variance is compiler-derived.
- **§B25 extern "C" / Box::from_raw / from_raw_parts / no_mangle** — src/lib.rs:45: N/A — no extern blocks, no `#[no_mangle]`, no `from_raw*`; zero runtime deps, no FFI.
- **§B25a FFI calls across threads without a cited thread-safety contract** — src/lib.rs:45: N/A — no C library wrapped or called; no threading.
- **§A1 unverified/unnamed dependency (slopsquatting)** — crates/size-classes/Cargo.toml:17-18: **justified** — sole dependency is dev-only `proptest = "1"`, a well-known, correctly-spelled registry crate sanctioned by repo conventions; `[dependencies]` is empty.
- **§A1 unpinned [patch]/git source (BANNED sub-case)** — crate + workspace Cargo.toml: N/A — grep for `[patch` and `git =` returned no hits.
- **§A1 network in build.rs (BANNED sub-case)** — N/A — the crate has no build.rs at all.
- **§C1 blanket impl in public API of a published library** — src/lib.rs:295: N/A — the only impl block is the inherent impl on `SizeClasses<N, L>`; no traits defined or implemented, no `impl<T: Bound>` anywhere.
- **§F1/§F2 divergence — no_std / no-alloc / zero-dep / forbid(unsafe_code) claims** — src/lib.rs:43-45 + Cargo.toml: not violated — all four claims hold, machine-enforced.
- **§F1/§F2 divergence — geometric formula `round_up(prev * num / den, min_block)` with min step** — src/lib.rs:176-181: not violated — div_ceil-then-mask-round-up is arithmetically equal to round-up of the exact rational; matches README and rustdoc prose.
- **§F1/§F2 divergence — compile-time u8 pin on class count** — src/lib.rs:218-221: not violated — `assert!(N < 256)` in const eval; class_idx ≤ 254 always fits u8.
- **§F1/§F2 divergence — "provably-equivalent" jump slow path (the crate's central conformance claim)** — src/lib.rs:408-439: not violated — verified analytically AND empirically (exhaustive size×align sweep against an independent scan oracle; 3-scheme proptest jump≡walk≡scan). Holds for in-contract pow2 aligns; the non-pow2 edge is the 🟡 §F2 medium finding above, not a 🔴.
- **§F1/§F2 divergence — "align >= 512 falls to whole-segment" bug class claimed fixed** — tests/builder.rs:108-129, 145-155: not violated — sweep covers every pow2 align up to SEFER_MAX; jump test pins the 128-align hop; extras 512/1024/2048/4096 present.
- **§F1/§F2 divergence — huge threshold is policy, no OS segment-size constant** — src/lib.rs:381-383 + tests/builder.rs:236-254: not violated — `is_huge` compares only `Params::huge_threshold`; no segment-size constant in src.
- **§F1 "mimalloc-style" naming (conformance-claim trigger)** — src/lib.rs:60 + README.md:3 + Cargo.toml description: justified — style claim only; no "compatible with mimalloc" claim, so no external test-vector obligation attaches.
- **§F3 leaked/unclosed boundary resource an untrusted peer can hold open** — whole crate: N/A — pure const-computation crate; no I/O, sockets, files, tasks, or Drop-bearing resources.

Modules declaring no 🔴 items (empty red inventory by construction): drop-and-raii (§B4/§B4a), testing (§D1–§E6), data-and-types (§B6–§E3 are 🟡/🟢 only). No 🔴 item has any confirmed occurrence anywhere in the crate.
