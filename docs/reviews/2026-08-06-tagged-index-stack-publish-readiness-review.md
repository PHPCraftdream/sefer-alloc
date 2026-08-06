# `tagged-index-stack` — publish-readiness review (read-only)

**Date:** 2026-08-06
**Scope:** `crates/tagged-index-stack` (v0.1.0, leaf crate, no path deps)
**Measurement identity:** `main` @ `2a1ca35ae7ba78c286beaf3acff4b8210fa9766f`, working tree
dirty only with untracked `docs/reviews/2026-08-06-*.md` files (no tracked file modified —
every command below was run against the committed tree).
**Toolchain:** `rustc 1.97.0 (2d8144b78 2026-07-07)` / `cargo 1.97.0 (c980f4866 2026-06-30)`,
`x86_64-pc-windows-msvc`. Crate MSRV declared `1.88` (`crates/tagged-index-stack/Cargo.toml:5`)
— NOT verified against a 1.88 toolchain here (none installed locally).
**Ground truth:** not published on crates.io; not present in `.github/workflows/release.yml`.

---

## Verdict: **GO-WITH-FIXES**

The crate is technically ready — it builds, tests, lints, documents, packages and
`cargo publish --dry-run`s cleanly with zero findings from the standard toolchain. The
"with-fixes" is for **four small, real defects the toolchain does not catch**, all of which
are cheap and none of which is a design problem:

| # | Finding | Severity | Cost to fix |
|---|---|---|---|
| F1 | `push`'s documented contract (`index < INDEX_MASK`) is **insufficient for `INDEX_BITS > 32`** — pushing `index == u32::MAX` passes the `debug_assert` and causes **silent, reproducible slot loss** | Medium (correctness contract on a published lock-free primitive) | 1 line + 1 doc sentence |
| F2 | Crate is `no_std` and categorised `no-std::no-alloc`, but requires **`AtomicU64`** — it will not compile on `thumbv6m`/`thumbv7em`/`riscv32imc`/`armv5te`. Undocumented. | Medium (false portability promise to embedded users) | doc paragraph |
| F3 | `cargo doc` emits **1 broken intra-doc link** (`src/lib.rs:53`) — will render broken on docs.rs | Low | 1 line |
| F4 | **12 of the crate's 16 tests never run in CI** (`stack_unit.rs` + `regression_counter_wrap.rs` are `#![cfg(not(loom))]`; the only CI invocation is the loom job's `--test loom_aba` under `--cfg loom`) | Medium (a published crate whose conformance suite is CI-dead) | 1 CI step |

**My top recommendation, unambiguously: publish it standalone** (after F1–F4), and add
`tagged-index-stack-v*` to `.github/workflows/release.yml`. Of the three crates flagged by
task K3, this is the one with the strongest independent case — see §1.

---

## 1. Should this be a standalone published crate? — **Yes, strongly.**

This is the best standalone-publish candidate in the K3 set, and the argument is not
sentimental — it rests on three things the code itself demonstrates.

**(a) The primitive is genuinely general, and the crate says so honestly.**
`crates/tagged-index-stack/src/lib.rs:6-11` frames it as "the canonical 'recycle a small
integer id' primitive that slab allocators, object pools, entity-component stores, and
connection tables all reinvent — and routinely reinvent *wrong*". That is an accurate
description of a real, recurring need. Nothing in the crate is allocator-specific: there is
no segment, no size class, no page, no OS call. The entire public surface is
`TAIL` (`:129`), `TaggedIndex<INDEX_BITS>` (`:142`), `Links` (`:224`), `ArrayLinks<N>`
(`:239`), `TaggedIndexStack<INDEX_BITS>` (`:289`) — 438 lines total, zero dependencies in a
normal build (`crates/tagged-index-stack/Cargo.toml:20-25`).

**(b) The `Links` trait is the design decision that makes it publishable rather than
extracted-and-awkward.** `:224-232` externalises the per-index "next" link into caller
storage. That is exactly what lets sefer keep links slot-resident (see the adapter at
`src/registry/heap_registry.rs:543-624`) while a third party uses the bundled
`ArrayLinks<N>` (`:239-279`). Many extractions fail because the general API is a compromise;
here the general API is *better* than a hardcoded array would have been, and both consumers
are first-class.

**(c) The differentiator is real and rare: executable loom proofs against the real type,
with non-vacuousness counterfactuals.** Under `--cfg loom` the crate aliases its own atomics
to `loom::sync::atomic` (`src/lib.rs:115-118`), so `tests/loom_aba.rs` model-checks the code
that ships, not a transcription. The suite includes two `#[should_panic]` counterfactuals
(`tests/loom_aba.rs:181-183`, `:362-364`) proving the harness would catch the bugs it claims
to exclude. Verified passing — see §3. `README.md:8-12` positions this correctly against
prior art ("Crates like `sharded-slab` embed one privately; this ships it as a standalone
primitive **with executable loom proofs run against the real type**").

**(d) The two documented subtleties are genuine hard-won knowledge, not padding.** The H-2
empty-transition tag preservation (`src/lib.rs:43-62`, implemented at `:387-390`) is a real
ABA bug that a naive `pack(empty_index, 0)` reintroduces — and the loom counterfactual
`counterfactual_empty_transition_tag_reset_lets_aba_recur` (`tests/loom_aba.rs:364`) proves
the fix is load-bearing rather than decorative. The RAD-1 lazy-link discipline
(`src/lib.rs:64-83`) is a first-touch/commit-footprint property most implementations get
wrong by eagerly chaining `0..N` at construction. Publishing these two write-ups alongside
the code is, on its own, more value than most micro-crates carry.

**Honest counter-arguments, and why they do not change the verdict:**

- *"It's 438 lines — a micro-crate."* Size is the wrong axis for a lock-free primitive; the
  value is the correctness argument, not the LOC. `crossbeam-epoch`-adjacent utilities and
  `sharded-slab`'s internals are the same size.
- *"Only 2 commits of history"* (`git log --oneline -- crates/tagged-index-stack` → `0ecfaa4`,
  `dbfeca3`). Misleading: the code is an extraction of an already-shipping, already-loom-proven
  registry protocol, and the extraction commit explicitly swapped `heap_registry` onto it
  (`0ecfaa4`, "extract ABA-tagged Treiber free-index stack; swap heap_registry onto it").
  It has one real production consumer from day one.
- *"`publish = false` is simpler."* It is, but it directly contradicts `README.md:515`'s
  standing claim that every workspace member is "a real crates.io crate someone can `cargo add`
  on its own" and the crates.io badge already displayed at `README.md:552` — the exact
  inconsistency filed as `docs/CORRECTNESS_OPEN_ITEMS.md` item 24. Marking it `publish = false`
  resolves K3 by retreat; publishing resolves it by delivering what the README already promises.

---

## 2. Metadata completeness — **complete, one cosmetic nit**

`crates/tagged-index-stack/Cargo.toml:1-13` — every crates.io-relevant field is present:

| Field | Value | Status |
|---|---|---|
| `description` (`:7`) | 510 chars | ✅ present — but see nit |
| `license` (`:6`) | `MIT OR Apache-2.0` | ✅ (both files present: `LICENSE-MIT`, `LICENSE-APACHE`) |
| `repository` (`:9`) | `https://github.com/PHPCraftdream/sefer-alloc` | ✅ |
| `homepage` (`:10`) | `.../tree/main/crates/tagged-index-stack` | ✅ correctly points at the subdirectory |
| `documentation` (`:11`) | `https://docs.rs/tagged-index-stack` | ✅ |
| `keywords` (`:12`) | `lock-free`, `aba`, `free-list`, `slab`, `no-std` | ✅ exactly 5 (the crates.io max), all well-chosen |
| `categories` (`:13`) | `concurrency`, `data-structures`, `no-std::no-alloc` | ✅ all valid slugs |
| `readme` (`:8`) | `README.md` | ✅ present, 79 lines |
| `rust-version` (`:5`) | `1.88` | ✅ declared |

**Nit (cosmetic, not blocking):** the `description` is 510 characters — a full paragraph, not
a one-liner. crates.io truncates search-result descriptions, so the first ~100 characters do
all the work; the `#[should_panic]` and `~89-year` details at the end are invisible where it
matters and duplicate `README.md:1-12`. A ~120-char lead ("Lock-free, `no_std`, zero-unsafe
ABA-tagged free-index stack (slot recycler) with real-type loom proofs") would read better in
listings. Purely taste.

**No `[package.metadata.docs.rs]` section** — correct here, because the crate has **no
`[features]` table at all** (verified: `grep -n "features" crates/tagged-index-stack/Cargo.toml`
returns nothing). `--all-features` is therefore identical to a default build. Note this means
the task brief's "check its Cargo.toml for the exact loom feature name" has no answer: loom is
reached via `--cfg loom` + `[target.'cfg(loom)'.dependencies]` (`:27-28`), **not** a feature.
That is the right choice — a `loom` *feature* would be additive-unification-hostile.

---

## 3. Build / test / lint health — **all green**

| Command | Result |
|---|---|
| `cargo test -p tagged-index-stack --all-features` | ✅ **12 passed, 0 failed** (`regression_counter_wrap`: 4, `stack_unit`: 8, `loom_aba`: 0 — correctly gated out, doc-tests: 0) |
| `cargo clippy -p tagged-index-stack --all-features --all-targets -- -D warnings` | ✅ clean, zero warnings |
| `cargo doc -p tagged-index-stack --all-features --no-deps` | ⚠️ **1 warning** — see F3 below |
| `RUSTFLAGS="--cfg loom" cargo test -p tagged-index-stack --release --test loom_aba` | ✅ **4 passed** in 0.07 s |
| `cargo build -p tagged-index-stack --no-default-features` | ✅ clean (no-op — no features exist) |
| `RUSTFLAGS="-D missing_docs" cargo build -p tagged-index-stack` | ✅ clean — see §6 |

**Loom detail** (the crate's headline claim, so worth spelling out — all 4 passed):

```
test counterfactual_empty_transition_tag_reset_lets_aba_recur - should panic ... ok
test pop_empty_transition_preserves_tag ... ok
test counterfactual_untagged_head_lets_aba_corrupt_free_list - should panic ... ok
test aba_repush_forces_stale_cas_retry_and_stays_consistent ... ok
```

Both non-vacuousness counterfactuals fire as designed. The 0.07 s runtime is not a red flag:
the models are deliberately tiny (2 threads, `N = 2` slots, `preemption_bound = Some(3)` —
`tests/loom_aba.rs:52`, `:75`, `:186`, `:260`), and the H-2 model uses a two-flag rendezvous
(`:275-309`) to pin the exact interleaving rather than search for it, which is the correct
design for a targeted ABA proof.

### F3 — broken intra-doc link (`src/lib.rs:53`)

```
warning: unresolved link to `pop`
  --> crates\tagged-index-stack\src\lib.rs:53:59
   |
53 | //! ABA collision that corrupts the free-list. The fix ([`pop`] here) packs the
```

This renders as broken text on docs.rs. Every *other* crate-doc reference to the same method
is correctly qualified as `` [`pop`](TaggedIndexStack::pop) `` (e.g. `src/lib.rs:44`, `:58`,
`:66`) — `:53` is simply the one that was missed. One-character-class fix.

Note this warning does **not** fail any gate: the crate has no `#![deny(rustdoc::broken_intra_doc_links)]`,
and `cargo doc` is not part of `npm run check` or of any `.github/workflows/ci.yml` job for
this crate.

### F2 — `no_std`, but requires `AtomicU64` (undocumented portability limit)

`src/lib.rs:107` is `#![cfg_attr(not(test), no_std)]` and the crate is categorised
`no-std::no-alloc` (`Cargo.toml:13`), but the head word is an `AtomicU64` (`src/lib.rs:290`),
reached through `core::sync::atomic::AtomicU64` (`:116`). `AtomicU64` is gated on
`target_has_atomic = "64"`, which a large share of the embedded targets that search
`no-std::no-alloc` do not have. Verified via `rustc --print cfg --target <t>`:

| Target | `target_has_atomic="64"` |
|---|---|
| `thumbv7em-none-eabi` | **absent** |
| `thumbv6m-none-eabi` | **absent** |
| `riscv32imc-unknown-none-elf` | **absent** |
| `armv5te-unknown-linux-gnueabi` | **absent** |

Neither `README.md` nor `src/lib.rs` mentions this anywhere. An embedded user who `cargo add`s
this on the strength of the `no-std::no-alloc` category gets a compile error, not a graceful
message. This is not a design flaw — a 64-bit packed word is the whole point of the 48-bit tag
budget (`src/lib.rs:85-97`) — it is a **documentation** gap: the README should state
"requires 64-bit atomics (`target_has_atomic = "64"`); 32-bit-atomic-only targets are not
supported". (A `TaggedIndex` over `AtomicU32` with a ~16-bit tag would be a *different*,
probabilistically-unsafe primitive — the crate's own `:94-96` explains why. Do not build it;
just document the requirement.)

---

## 4. Packaging — **clean**

`cargo package -p tagged-index-stack --list` → **11 files, no leakage**:

```
.cargo_vcs_info.json   Cargo.lock   Cargo.toml   Cargo.toml.orig
LICENSE-APACHE   LICENSE-MIT   README.md
src/lib.rs
tests/loom_aba.rs   tests/regression_counter_wrap.rs   tests/stack_unit.rs
```

`cargo package -p tagged-index-stack --allow-dirty` → **`Packaged 11 files, 70.6KiB (21.8KiB
compressed)`**, then `Verifying` + `Compiling` **succeeded** (the verify build compiles the
extracted tarball, so this proves the published artifact is self-contained).

`cargo publish -p tagged-index-stack --dry-run --allow-dirty` → reached
`Uploading tagged-index-stack v0.1.0` before `warning: aborting upload due to dry run`. **No
real publish was performed.**

**Manifest normalisation verified** (I read the generated
`Cargo.toml` inside `.cargo-target/package/tagged-index-stack-0.1.0/`): `[lints] workspace = true`
(`crates/tagged-index-stack/Cargo.toml:15-18`) is correctly **inlined** by cargo into
`[lints.rust.unexpected_cfgs] check-cfg = ["cfg(loom)", "cfg(kani)"]`, so the published crate
carries no dangling workspace reference. `[dependencies]` is **empty**; `loom 0.7` appears only
as `[target."cfg(loom)".dependencies.loom]`. That is exactly the intended shape: a downstream
`cargo add tagged-index-stack` pulls **zero** transitive dependencies.

**One consequence worth stating explicitly:** because `loom` is a real (not dev-) dependency —
deliberately, and correctly justified in the manifest comment at `:21-25` — crates.io will list
`loom` in the crate's dependency graph under a `cfg(loom)` target. It will never be built by a
normal consumer, but it *will* be visible on the crates.io page. That is a cosmetic surprise,
not a problem, and the alternative (a shadow-model loom test) is exactly what this crate was
built to eliminate.

---

## 5. Completeness scan — one real edge-case gap (F1)

**Marker scan:** `grep -nE 'TODO|FIXME|XXX|unimplemented!|todo!|HACK|dead_code|allow\('` across
`crates/tagged-index-stack/` → **zero matches**. No placeholders, no suppressed lints, no
`#[allow]` anywhere in the crate. Clean.

### Index-bit exhaustion — **handled**

`src/lib.rs:147-151` is a compile-time `const _CHECK_BITS: ()` asserting `1 <= INDEX_BITS < 64`,
and `pack` forces its evaluation (`:168`, `let () = Self::_CHECK_BITS;`) so the guard cannot be
optimised away by never being touched. The empty sentinel is the all-ones index value
(`:155`, `:201-203`), so the usable range is `0 .. (1 << INDEX_BITS) - 1` — documented at
`:18-20` and pinned by `tests/regression_counter_wrap.rs:61-92` and
`tests/stack_unit.rs:84-95` (`INDEX_BITS = 20`).

### Tag wraparound — **handled, documented, tested, and honestly bounded**

The tag wraps at `2^TAG_BITS` via `wrapping_add(1)` (`src/lib.rs:348`) and `pack`'s
`tag << INDEX_BITS` discarding the overflow bit. This is **not** a soundness hole — it is a
bounded probabilistic window, and the crate does the arithmetic in public
(`src/lib.rs:85-97`): at `INDEX_BITS = 16` the 48-bit tag needs ~89 years of a frozen victim
at 100k pushes/sec on a single slot. Critically, `:96-97` also states the *inverse* — "Widening
the index half shrinks this budget; a caller choosing `INDEX_BITS` trades index range against
tag headroom" — and `:94-96` quantifies the bad end (a 32-bit tag gives ~43 s, "a probabilistic
hazard, not a structural non-hazard"). Tested at the exact boundary:
`tests/regression_counter_wrap.rs:25-58` (`2^48 - 1` → wrap to 0, index survives, does not read
as empty) and `tests/stack_unit.rs:56-79`.

This is the right treatment for a published crate. My only suggestion: the "`INDEX_BITS = 32`
gives you a 32-bit tag ≈ 43 s" warning currently lives only in the *crate-level* doc; a
third party picking a width reads `TaggedIndex`'s own doc (`:131-140`) or `TAG_BITS`
(`:157-159`, which says merely "The tag wraps at `2^TAG_BITS`"). Repeating one sentence of the
budget warning on `TAG_BITS` would put it where the decision is actually made. Nice-to-have.

### F1 — `push`'s contract is insufficient for `INDEX_BITS > 32` (**reproduced**)

`push`'s doc says "`index` MUST be a valid index (`< TaggedIndex::INDEX_MASK`)"
(`src/lib.rs:317-319`) and the guard is `debug_assert!((index as u64) < INDEX_MASK)`
(`:325-328`). For `INDEX_BITS <= 32` that is airtight, and `:332-335` even explains why
(`INDEX_MASK` numerically equals `TAIL = u32::MAX` only at `INDEX_BITS == 32`, and the
`< INDEX_MASK` bound excludes it).

But **for `INDEX_BITS` in `33..=63`**, `INDEX_MASK` exceeds `u32::MAX`, so `index == u32::MAX`
— which *is* `TAIL` (`:129`) — passes the assert. A link written as `TAIL` is then
indistinguishable from end-of-chain, and `pop`'s `next == TAIL` branch (`:387`) truncates the
chain. The real contract is `index < min(INDEX_MASK, TAIL)`, which is neither asserted nor
documented.

Reproduced against the committed crate via a scratch consumer outside the repo (no repo file
was modified):

```
INDEX_BITS=40: INDEX_MASK=0xffffffffff TAIL=0xffffffff;
push's debug_assert `(index as u64) < INDEX_MASK` for index=u32::MAX: true

  stack.push(&links, u32::MAX);   // onto an EMPTY stack -> links[u32::MAX] = TAIL
  stack.push(&links, 5);          // -> links[5] = u32::MAX, which READS as TAIL

  drain -> pops = [5]             // expected [5, 4294967295]
                                  // index u32::MAX is SILENTLY LOST
```

**Severity assessment — deliberately not inflated.** This is unreachable in practice: it needs
`INDEX_BITS > 32` *and* a live index of `u32::MAX`, i.e. a four-billion-slot link array, and
sefer itself uses `INDEX_BITS = 16` (`src/registry/bootstrap.rs`, `src/kani_proofs.rs:185`).
It is **not** memory-unsafety — the crate is `#![forbid(unsafe_code)]` (`src/lib.rs:108`), so
the worst case is logical slot loss, never UB. It is also a `debug_assert`, so nothing changes
in release either way.

**Why it should nonetheless be fixed before publishing.** The crate's own pitch is that this is
the primitive "people routinely reinvent *wrong*" and that here the subtleties are
"structurally enforced" (`src/lib.rs:6-11`). A third party is being explicitly invited to pick
their own `INDEX_BITS`, and the one width-dependent constraint the docs *don't* state is this
one — while the docs go out of their way to discuss the adjacent `INDEX_BITS == 32` coincidence
at `:332-335`, which is exactly where a reader would expect to be told. Fix is one line:

- tighten to `debug_assert!((index as u64) < INDEX_MASK && index != TAIL, ...)`, **or**
- add `const _: () = assert!(INDEX_BITS <= 32)` if widths above 32 are not intended to be
  supported (they are arguably pointless — `push` takes a `u32`, so indices above `u32::MAX`
  are unreachable anyway, making `INDEX_BITS > 32` pure wasted index range at the cost of tag
  headroom),

plus one sentence in `push`'s doc and one regression test in `tests/stack_unit.rs`.

### F4 — 12 of 16 tests are CI-dead

`tests/stack_unit.rs:11` and `tests/regression_counter_wrap.rs:12` are both `#![cfg(not(loom))]`.
The **only** CI invocation of this crate is in the `loom-alloc-global` job
(`.github/workflows/ci.yml:952-953`):

```yaml
cargo test --release -p tagged-index-stack
  --test loom_aba
```

…run with `RUSTFLAGS: "--cfg loom"` (`.github/workflows/ci.yml:930`). Under `--cfg loom` the
two conformance files compile to zero tests, and `--test loom_aba` would exclude them regardless.
Every other `cargo test` step in `ci.yml` targets the **root** package with feature sets
(`:284`–`:426`); none passes `--workspace`. `scripts/check-all.mjs` contains **zero** references
to `tagged-index-stack` (`grep -c "tagged" scripts/check-all.mjs` → `0`).

Net: the 12 tests that pin the packing round-trip, the empty sentinel, the `2^48` wrap boundary,
LIFO order, the H-2 single-threaded observation and the RAD-1 laziness **have never run in CI**.
They pass locally today, but nothing would catch a regression. For a crate about to acquire
third-party users this is the most consequential of the four findings, and the cheapest to fix:
one `cargo test -p tagged-index-stack` step (no `RUSTFLAGS`) in the existing test job.

---

## 6. Public API doc coverage — **excellent**

`RUSTFLAGS="-D missing_docs" cargo build -p tagged-index-stack` compiles **clean**: every
public item carries a doc comment. Item by item:

| Item | Doc |
|---|---|
| crate root (`:1-105`) | 105 lines: packed word, links model, H-2, RAD-1, tag budget, loom |
| `TAIL` (`:120-129`) | ✅ and — importantly — explicitly distinguishes itself from the empty sentinel |
| `TaggedIndex` (`:131-142`) | ✅ states the valid range `0 .. (1 << INDEX_BITS) - 1` |
| `INDEX_MASK` / `TAG_BITS` (`:153-159`) | ✅ |
| `pack` / `unpack` (`:161-176`) | ✅ `pack` states the `index < 2^INDEX_BITS` precondition and the consequence of violating it |
| `empty` / `empty_index` / `is_empty` (`:178-210`) | ✅ `empty`'s doc is notably good — it warns in bold that tag-0 is **bootstrap-only** and that a runtime drain must preserve the tag |
| `Links` (`:213-232`) | ✅ carries an explicit **"# Ordering contract"** section requiring `Acquire`/`Release` |
| `ArrayLinks` / `new` (`:234-263`) | ✅ notes the loom-build non-`const` split |
| `TaggedIndexStack` / `new` (`:281-311`) | ✅ states "a fresh stack is EMPTY (lazy links, RAD-1)" |
| `push` (`:313-328`) | ✅ has a `# Panics` section |
| `pop` (`:362-375`) | ✅ has a dedicated **H-2 empty transition** paragraph |
| `raw_head` (`:403-407`) | ✅ explains *why* it is `Acquire` (so a split-pop loom test forms the same happens-before edge) |
| `cas_head_for_test` (`:413-421`) | ✅ `#[cfg(loom)]`-only, `# Errors` section, explicitly "NOT part of the public API" |

Two structural points that matter more than raw coverage for a lock-free crate a third party
builds on:

1. **The `Links` ordering contract is a correctness obligation on a *safe* trait**
   (`:217-223`). A downstream `impl Links` using `Relaxed` breaks the algorithm. Because the
   crate is `#![forbid(unsafe_code)]` this can never be UB — only logical corruption — so
   `unsafe trait` would be wrong here, and prose is the correct instrument. It is stated
   clearly and in the right place. ✅ No change needed; worth noting this is the crate's
   single largest "trust the docs" surface.
2. **Limits are documented where they belong**, with the two exceptions already filed: F1
   (the `INDEX_BITS > 32` / `TAIL` interaction is undocumented anywhere) and F2 (the
   `AtomicU64` target requirement is undocumented anywhere). Max capacity ✅, tag-wrap
   behaviour ✅, empty-vs-`TAIL` sentinel distinction ✅, lazy-link/empty-on-construction
   surprise ✅.

**Doctests:** none — correct per this repo's own "No doctests" rule (`CLAUDE.md`, "Tests"),
and both the README example (`README.md:67-75`) and the loom invocation (`README.md:61-63`)
use ` ```text ` fences. Verified `cargo test` reports `Doc-tests tagged_index_stack ... 0 tests`.
**I compiled the README example verbatim in a scratch consumer and it is correct** (prints
`Some(7)`). Note it is therefore *unverified by CI* — an acceptable trade under the project's
no-doctests rule, but if the README example ever drifts, nothing catches it.

---

## 7. Performance angle — **no open perf work targets this crate, and it is not on the hot path**

Grepped both indexes as instructed:

- `docs/perf/OPEN_ITEMS.md` — 3 hits (`:1526`, `:1532`, `:1593`), **all inside item 26**
  (the `batch-api` no-consumer reconfirmation). The crate appears there only as a crate that
  was *checked and ruled out* as a batch-API consumer: `:1532-1537` records it as "`no_std` +
  `#![forbid(unsafe_code)]` + explicitly allocation-free (a bare index recycler with no backing
  storage of its own) — its own doc comment MENTIONS 'object pools, entity-component stores' as
  prior art this primitive is *for*, but the crate itself is not such a consumer". **No perf
  item targets this crate.**
- `docs/CORRECTNESS_OPEN_ITEMS.md` — 1 hit (`:1412`), inside **item 24**, which is the K3
  finding itself (README claims all 11 members are on crates.io; `racy-ptr-cell`, `size-classes`
  and `tagged-index-stack` are not). Its "Next trigger" (`:1437-1440`) is precisely this
  publish-DAG decision. **No correctness defect is filed against the crate's code.**
- `docs/perf/OPEN_ITEMS_ARCHIVE.md:592` mentions `free_slots` only in the R25-5/R26-1
  slot-lifecycle narrative (registry slot recycling), not the stack's performance.
- The remaining `docs/perf/` hits are `_raw_*.log` symbol dumps and `R34_MANIFEST.md`, not
  work items.

**Correction to the task brief's premise.** The brief describes this as backing sefer's
`free_slots` "on a hot path". It does not. `free_slots` is reached only through
`pop_free_slot`/`push_free_slot` (`src/registry/heap_registry.rs:600-624`), whose callers are
`pick_slot` (`:319-322`) and `recycle` (`:372-374`). `pick_slot`'s only callers are
`HeapRegistry::claim` (`:120`) and `claim_with_config` (`:211`) — i.e. **thread-heap acquisition,
once per thread**, driven from `src/global/tls_heap.rs:599`/`:611`. `recycle` runs on thread
exit. This is a **thread-lifecycle** path, not a per-allocation path; no `alloc`/`dealloc`
fast path touches the stack. That is consistent with there being no perf item against it, and
it means **there is no perf-driven reason to delay publishing**.

Two related facts worth recording, since they bear on the crate's correctness pedigree rather
than its speed:

- The packing is Kani-proven at `INDEX_BITS = 16` — `src/kani_proofs.rs:179-207` (CRATE-P7)
  imports `tagged_index_stack::TaggedIndex` directly and proves `pack`→`unpack` round-trip and
  no bit-mixing on any word. That is real formal coverage of the published crate's core.
- `src/registry/bootstrap.rs:358-437` keeps a `loom_shim::TaggedIndexStack` **replica**
  (`:360-366`, "Const-capable stand-in") used only under `--cfg loom`, because the real type's
  `new()` is non-`const` in loom builds (`src/lib.rs:304-311`). It is documented as a faithful
  replica (`:377`, `:404`) and `bootstrap.rs:446-451` explicitly notes the real coverage lives
  in the crate's own suite. This is honest and correctly scoped, but it *is* a second copy of
  the protocol that could silently drift from the crate — worth a maintainer's awareness, not
  a publish blocker (and out of scope for this review).

---

## Recommended pre-publish checklist

1. **F1** — tighten `push`'s `debug_assert` to also exclude `index == TAIL` (or add
   `const _: () = assert!(INDEX_BITS <= 32)`), document the real bound, add a regression test.
2. **F2** — one README + crate-doc paragraph: requires `target_has_atomic = "64"`.
3. **F3** — fix `src/lib.rs:53`'s `[`pop`]` → `` [`pop`](TaggedIndexStack::pop) ``. Consider
   `#![deny(rustdoc::broken_intra_doc_links)]` so docs.rs breakage becomes a build failure.
4. **F4** — add `cargo test -p tagged-index-stack` (no `RUSTFLAGS`) to `ci.yml`'s test job so
   the 12 conformance tests run.
5. Add `tagged-index-stack-v*` to `.github/workflows/release.yml:51-55` and
   `tagged-index-stack` to the `workflow_dispatch` `options:` list at `:62-67`.
6. (Optional) shorten `description` to a search-result-friendly lead.

None of 1–4 changes runtime behaviour; under this repo's R30-12 taxonomy they are `fix(perf)`
(F1, arguably plain `fix`), `docs(config)` (F2, F3) and `build`/`test` (F4, 5).

---

## Open questions for the maintainer

1. **Is `INDEX_BITS > 32` intended to be supported at all?** `push` takes a `u32`, so any width
   above 32 buys unreachable index range while shrinking the tag budget. If the answer is "no",
   F1's fix is the stricter and simpler `const _: () = assert!(INDEX_BITS <= 32)` rather than
   the `index != TAIL` guard — and the `_CHECK_BITS` bound at `src/lib.rs:147-151` should be
   narrowed from `1..64` to `1..=32` to say so structurally.
2. **Publish at `0.1.0` or `1.0.0`?** The API is 5 items, has one production consumer, is
   loom- and Kani-proven, and I see no expansion pressure. A `0.1.0` invites a churn expectation
   that does not exist; a `1.0.0` commits to the `Links` trait signature forever. My read: ship
   `0.1.0` now (it is what the root `Cargo.toml:892` already pins, `version = "0.1"`, so no
   version bump is entangled with this decision), and promote to `1.0` after the first external
   user. **No version change is required to publish.**
3. **Does the maintainer want `loom` visible as a crates.io dependency?** It is correct as-is
   (`Cargo.toml:27-28`) and is the mechanism behind the crate's headline differentiator, but it
   will render on the crates.io dependency list. Confirming this is intended, rather than
   discovering it post-publish, seems worth 10 seconds.
4. **Should `src/registry/bootstrap.rs:360-437`'s `loom_shim` replica be pinned against the real
   crate?** Once `tagged-index-stack` is a versioned external dependency, the replica can drift
   from a crate the workspace no longer edits in lockstep. A single test asserting replica/real
   equivalence under a non-loom build would close it. Out of scope here — flagging only.
5. **Confirm the ordering of the publish DAG.** This crate is a leaf (zero path dependencies),
   so it can be published at any time and blocks nothing; but `sefer-alloc`'s own
   `tagged-index-stack = { path = ..., version = "0.1", optional = true }` (`Cargo.toml:892`)
   means **`sefer-alloc` 0.3.0 cannot be published until this crate is** — the `alloc-global`
   feature (`Cargo.toml:179`) pulls it in, and crates.io rejects unresolvable path deps.
   That makes this crate a **hard prerequisite** for the 0.3.0 tag, which is the strongest
   practical argument for resolving K3 by publishing rather than by `publish = false`
   (`publish = false` would force `alloc-global` — and therefore `production` — to be removed
   from the published `sefer-alloc`).
