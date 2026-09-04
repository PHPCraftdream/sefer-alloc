# tagged-index-stack — pre-release static review, round 21

**Reviewer:** fx (claude-fable-5-1, xhigh) — independent pass, not a continuation of the Sol-codex series

**Round:** 21

**Review timestamp:** 2026-09-04 09:54 (local)

**Reviewed revision:** `1e75569b7fc56aa7a27c7373e3c63f89bdc37ba1` (`main`)

**Delta since the run-20 revision (`d510da3`):** `d510da3..1e75569` — seven commits, five touching this
crate (`6de4f91`, `c519410`, `a1dc3e3`, `ad50e0b`, `1e75569`), the other two a checkpoint and the
run-20 report itself.

## Verdict

**GO on the production algorithm; the crate is publishable as an allocator primitive today.** No P0/P1/P2
defect exists in `src/`. I re-derived the three ordering arguments (push's `Relaxed` initial load and
`Relaxed` CAS-failure ordering; pop's `Acquire`-only CAS success riding the RMW release sequence; the H-2
running-tag empty transition), the seal arithmetic, the self-loop detector's false-positive impossibility
under the contract, and the eight-region unsafe inventory — all hold. Clippy (`--all-targets -D warnings`,
default and `test-internals`) and `RUSTDOCFLAGS="-D warnings" cargo doc` are clean on the local rustc
1.97.0, and the E0133 message shapes the compile-fail driver asserts still match this toolchain (I built the
`hook_call_requires_unsafe` fixture directly to check).

**What is NOT ready is the text around the code.** `src/imp.rs` is 1,715 comment lines to 541 code lines;
`src/lib.rs` is 432 to 23. The `StackStorage` trait doc alone is 361 lines; `push_index`'s doc is 192.
Much of it is not a contract but a *narration of the crate's own review process* — "P1-1 fix", "run-19
review P3-4", "since 2026-09-02", "the former width-32 cap", "replaces the seven unchecked caller-side
casts the old `(u64, u64)` signature forced" — written for readers who watched the crate being reviewed,
and published to crates.io readers who did not and cannot follow the `docs/reviews/` citations. Two of
those explanations are also factually wrong (`Backoff`'s stated overflow mode; "the branch is kept purely
for readability" on a branch that is load-bearing). The 357-line CHANGELOG describes BREAKING changes and
FIXES to a version that never shipped.

Two infrastructure findings are real but non-blocking: the arm64 gate's new `--mode summary` step (run-20
P3-2's closure) verifies the *committed Windows* wallclock CSV, never the run's own aarch64 output; and the
published `.crate` ships a 1,275-line Node.js benchmark driver plus a test that spawns `node`, `git`, and
four `cargo build`s.

This is a static review; I ran only read-only builds (clippy, rustdoc, one compile-fail fixture build) — no
test suite, no loom, no benchmarks.

## Priorities

| Level | Count | Meaning |
|---|---:|---|
| P0 | 0 | soundness holes — none found |
| P1 | 0 | serious runtime defects — none found |
| P2 | 0 | release blockers — none found |
| P3 | 9 | should land before the first publish: wrong doc claims, a hot-loop code smell, a hollow CI oracle, packaging weight |
| P4 | 10 | prose reduction, stale identifiers, test dedup, minor nits |

Findings are grouped by theme (the task's six axes), each tagged with its level.

## Scope and mode

Read in full: `src/lib.rs`, `src/imp.rs`, `Cargo.toml`, `README.md`, `CHANGELOG.md`, every file under
`tests/` (including all seven compile-fail fixtures and `tests/common/`), `benches/tagged_index_stack_bench.rs`,
`examples/backoff_per_call_latency.rs`, `scripts/tis_p3_ab_runner.mjs` and its three templates. Cross-checked
against: the root crate's `src/registry/heap_registry.rs` `StackStorage` impl and `bootstrap.rs` loom shim
(the one in-workspace consumer), the `tagged-index-stack` jobs in `.github/workflows/ci.yml`,
`docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` §3.2 and `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4
(to verify the numbers quoted in rustdoc), and `git log`/`git show` for the post-run-20 commits.

Verified mechanically rather than trusted: the "exactly EIGHT" `#[allow(unsafe_code)]` regions (8 —
`imp.rs:1033,1311,1446,1474,1522,1783,1936,2086`), the "TEN `unsafe fn`" (10), "SIX `unsafe {}`" (6,
`imp.rs:1483,1494,1504,1629,1793,1947`), "ONE `unsafe trait`" (1); every `docs/perf`, `docs/adr` and
`docs/reviews` path the crate cites exists in the repository; the seal-time arithmetic in
`lib.rs:167-184` (2^48/2×10^8 = 16.3 d, 2^48/10^9 = 3.26 d, 2^40/2×10^8 = 91.6 min, 2^32/2×10^8 = 21.5 s);
`cargo package --list` for the shipped file set.

---

## 1. Bugs

### BUG-1 (P3, doc-level) — `Backoff::spin` names the wrong overflow failure mode

`src/imp.rs:106-110`:

```rust
/// Capped, not unconditional: unbounded `K` would eventually be an
/// `attempt to add with overflow` panic under overflow-checks after
/// ~2^32 consecutive lost CASes in one call — remote, but free to
/// close.
```

That is not what would happen. The loop is `for _ in 0..(1u32 << self.0)` (`imp.rs:114`): an unbounded `K`
reaches `1u32 << 32` after **32** consecutive lost CASes — a debug `attempt to shift left with overflow`
panic, or in release a masked shift (`1 << (K & 31)`) that silently resets the backoff to one spin, then two,
then four... The file already knows this: the `const _: () = assert!(BACKOFF_SPIN_CAP < 32)` guard fifty
lines earlier (`imp.rs:55-58`) is annotated "`1u32 << K` masks/panics if `BACKOFF_SPIN_CAP` ever reaches
32". So `imp.rs` contradicts itself on the same constant within one screen, and the "remote" framing is
wrong by a factor of 2^27: 32 lost CASes in one call is an ordinary event under the contention this crate is
built for (the `threaded_conservation` oracle *requires* ≥7 in a row to pass). The saturation is not a
"free to close" nicety; it is the thing keeping the backoff monotonic.

**Fix:** replace the paragraph with the true statement: "Saturation keeps `K <= BACKOFF_SPIN_CAP`, so
`1u32 << K` can never overflow (K = 32 would, after only 32 lost CASes)." The `const _` guard's comment can
then point here instead of repeating the mechanism.

### BUG-2 (P4, doc-level) — "the branch is kept explicit purely for readability" is false; the branch is load-bearing

`src/imp.rs:1563-1573`:

```rust
// The link this index chains to: the current head's index, or TAIL
// if the stack is empty. The empty sentinel packs INDEX_MASK, which
// is <= 0xFFFF at every legal width per `_CHECK_BITS`, so it can no
// longer equal TAIL; and `TAIL & INDEX_MASK == INDEX_MASK` is a
// mathematical identity (all-ones AND all-ones-low-bits), not a
// coincidence. The branch is kept explicit purely for readability.
let next_link = if TaggedIndex::<B>::is_empty(head) { TAIL } else { cur_idx };
```

and the `TAIL` doc at `imp.rs:30-32` ("The two mappings are kept spelled out separately in `push_index` /
`pop_index` purely for readability").

Trace what happens without the branch: on an empty head `cur_idx == INDEX_MASK` (e.g. `0xFFFF`), so
`store_next(index, 0xFFFF)` would be written instead of `TAIL`. The next pop reads `next == 0xFFFF`, and the
clause-4 guard at `imp.rs:1725` — `next != TAIL && (u64::from(next) >= mask || ...)` — panics
("neither TAIL nor a valid index"). The branch is what makes the empty-head push encode `TAIL`; it is
semantically required, not cosmetic. The "identity" sentence is a leftover from the pre-cap design in which
`INDEX_MASK == TAIL` at width 32 made the two values coincide — exactly the "historical coincidence" the same
comment says is now impossible. Both comments should just say: "the empty head's index half is the sentinel
`INDEX_MASK`, not `TAIL`; map it explicitly."

### BUG-3 — production algorithm: nothing found (recorded for calibration)

Checked and sound, with the reasoning the code relies on:

- **push's `Relaxed` initial load and `Relaxed` failure ordering** (`imp.rs:1545`, `:1665`): push uses the
  observed word only as a value (`cur_idx` for the link, `tag` for the bump) and never dereferences a link
  through it; the popper that later follows `next[index]` synchronizes with push's `Release` CAS directly.
  Coherence also guarantees a contract-abiding push can never observe its own `index` as head (its authority
  came from a pop whose CAS moved the head off `index`, and that CAS happens-before this load), so
  `next[index] == index` is unreachable without a contract violation — the self-loop detector cannot
  false-positive.
- **pop's `(Acquire, Acquire)` CAS with no `Release` half** (`imp.rs:1748`): sound because every write to
  `head` is an RMW (constructors initialise, `raw_head` loads), so the release sequence headed by any push's
  `Release` CAS extends through every later pop CAS; a later popper's `Acquire` load of a value written by a
  pop still synchronizes with the push that wrote the link it is about to read. The `INVARIANT` on the
  `head` field (`imp.rs:489-512`) states this correctly and forbids the one thing that would break it (a
  plain `store`).
- **H-2** (`imp.rs:1728-1733`): the drain packs the observed tag; `is_empty` inspects only the index half.
- **Seal** (`imp.rs:1560-1562`): checked before `store_next`, so a first-attempt refusal has no side effect;
  `TAG_MAX - tag` in `pushes_remaining` cannot underflow because the tag is `TAG_BITS` wide by construction.
- **`pack_truncating`'s tag `debug_assert`** (run-20 P3-4, commit `c519410`): landed as described; all
  three call sites (`empty()`, push's `tag + 1` after the seal check, pop's observed tag) satisfy it.
- **Clause-2 "publication-relative lower bound"** (`imp.rs:701-723`): a `load_next` that happens-after an
  `Acquire` observation of push P's `Release` CAS reads P's own `store_next` or a later write in the cell's
  modification order (coherence), never an earlier one — and a later write (an intervening pop+repush) is
  harmless because the observer's tag expectation is stale. Correct as stated.

---

## 2. Performance

### PERF-1 (P3) — the retry arms are duplicated four times over `#[cfg]`; a return value removes all of it

`src/imp.rs:1667-1685` (push) and `:1750-1773` (pop) each carry this shape:

```rust
#[cfg(any(feature = "test-internals", loom))]
PUSH_RETRY_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
head = actual;
#[cfg(any(feature = "test-internals", loom))]
{
    backoff.spin();
    if backoff.spun_at_cap() {
        PUSH_BACKOFF_CAP_REACH_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }
}
#[cfg(not(any(feature = "test-internals", loom)))]
backoff.spin();
```

To make that work, `Backoff` grew a cfg-gated second tuple field (`imp.rs:65-71`), a `new()` with two
cfg-gated `return`s (`:74-80`), a `spun_at_cap()` accessor whose doc has to warn "Must be called AFTER
`spin` — before it, the flag still holds the PREVIOUS retry's verdict" (`:126-136`), and a `spin()` that
writes the flag under yet another cfg (`:120-123`). That is temporal coupling introduced purely to avoid
returning a `bool`. The whole apparatus collapses to:

```rust
struct Backoff(u32);

impl Backoff {
    /// Spins `1 << K`, then bumps K (saturating at the cap). Returns whether
    /// THIS spin already ran at full depth.
    #[inline]
    fn spin(&mut self) -> bool {
        let at_cap = self.0 >= BACKOFF_SPIN_CAP;
        for _ in 0..(1u32 << self.0) { core::hint::spin_loop(); }
        if !at_cap { self.0 += 1; }
        at_cap
    }
}

#[inline] fn note_push_retry()     { #[cfg(any(feature = "test-internals", loom))] PUSH_RETRY_COUNT.fetch_add(1, Relaxed); }
#[inline] fn note_push_cap_reach() { #[cfg(any(feature = "test-internals", loom))] PUSH_BACKOFF_CAP_REACH_COUNT.fetch_add(1, Relaxed); }
// (pop twins likewise)

// push retry arm:
Err(actual) => {
    note_push_retry();
    head = actual;
    if backoff.spin() { note_push_cap_reach(); }
}
// pop retry arm:
Err(actual) => {
    note_pop_retry();
    head = actual;
    if !TaggedIndex::<B>::is_empty(actual) {
        if backoff.spin() { note_pop_cap_reach(); }
    }
}
```

In a production build the `note_*` bodies are empty, the `if` folds away, and the hot loop contains zero
`#[cfg]` — identical codegen to today, four fewer cfg blocks per function, no second field, no
`spun_at_cap`, no dual-return constructor, and the oracle can no longer be read one retry early. The
`test-internals` observable behaviour is unchanged (the counters increment on exactly the same events), so
`tests/threaded_conservation.rs` and the loom suite need no edits.

### PERF-2 (P4) — pop's CAS success ordering `Acquire → Relaxed` is codegen-NULL; close it, don't re-recommend it

Runs 19 and 20 both listed "pop CAS success `Acquire` → `Relaxed`" as an unmeasured candidate, and
`CHANGELOG.md:186-192` records it as "unmeasured, same standard as the sibling candidates". It can be closed
by inspection, no measurement needed: the CAS instruction selection is driven by the *stronger* of the two
orderings. With `(success = Relaxed, failure = Acquire)`:

- x86-64: `lock cmpxchg` either way (a full barrier regardless of ordering).
- AArch64 with LSE: `casa` either way (the acquire variant is required for the failure path).
- AArch64 LL/SC (and RISC-V `lr/sc`): `ldaxr`/`lr.aq` either way — the acquire is on the load-exclusive.

Since the crate needs failure = `Acquire` (pop follows a link on retry — the loom counterfactual
`counterfactual_relaxed_cas_failure_corrupts_free_list` pins it), the success ordering cannot be lowered
below what the failure ordering already forces. The candidate is a provable NULL on every target this crate
plausibly runs on. Record that in the CHANGELOG bullet (one sentence) so the next reviewer does not
re-open it.

### PERF-3 (P4) — skipping the redundant `store_next` on a tag-only retry is measurable through the existing A/B harness

Every retry iteration of `push_index_impl` re-executes `s.store_next(index, next_link)` (`imp.rs:1629-1631`)
even when only the tag changed and `cur_idx` did not — the common shape when the loser's contender was
another *push* of a different index that then got popped again, or under pop-heavy churn on a stable head.
On x86 the extra store hits an M-state line and costs nothing measurable; on AArch64 it is a second `stlr`
(store-release, which stalls until prior accesses drain). This is exactly the class of question
`scripts/tis_p3_ab_runner.mjs` was built for: add a fourth anchor (`if next_link != last_stored { store; }`)
alongside `links_relaxed`/`cas_weak` and let the arm64 job answer it. I would not change the shipped code
on intuition — the branch may cost as much as the store — but the harness makes the measurement cheap.
Run-19 item 3 raised the same idea; it is still unmeasured.

### PERF-4 (P4) — `#[inline]` hygiene: one sibling missing, and the justification is longer than the functions

`Backoff::at_cap` and `Backoff::spin` carry `#[inline]` with a three-line rationale each
(`imp.rs:82-85`, `:103-104`: "a non-`#[inline]` non-generic private fn would not be
cross-crate-inlinable, a codegen regression in a hot path"); `Backoff::new` (`:74-80`), called from the
same monomorphised hot paths, has none. The inconsistency is harmless in practice — since Rust 1.75 rustc
marks small non-generic functions cross-crate-inlinable automatically under optimisation, and all three are
tiny — which also makes the quoted rationale stale for a crate whose MSRV is 1.79. Either add `#[inline]`
to `new` and cut the paragraphs to a single line ("hot path, monomorphised downstream"), or drop all three
attributes and let the heuristic work. PERF-1's refactor is the natural moment.

### PERF-5 (P4, doc note) — the aarch64 default target pays an outlined-atomics call per CAS

`docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` §3.2 established that on `aarch64-unknown-linux-gnu`'s
baseline feature set every `compare_exchange` is a `bl __aarch64_cas8_acq/rel` call (runtime-dispatched
to LSE or LL/SC). For a primitive whose whole hot path is one CAS, that is a real per-op cost consumers can
remove by building with `-C target-feature=+lse` or `-C target-cpu=...`. A single sentence under the
"Portability limit" section would save an aarch64 consumer a profiling session; nothing in the shipped
docs mentions it. Not a crate defect.

### PERF-6 — measured-and-rejected items I did not re-recommend

- `compare_exchange_weak`: codegen-identical to strong on both aarch64 lowerings and x86-64
  (`TIS_LINK_ORDERING_WEAK_CAS_GATE.md` §3.1-3.2, tripwire in the runner) — correctly left alone.
- `BACKOFF_SPIN_CAP` other than 6: swept 0/4/6/8/10 (`TIS_BACKOFF_CAP_SWEEP_GATE.md`) — the fairness trade
  is documented and I have no new evidence.
- Cache-line padding of `StackHead`/`ArrayLinks`: correctly delegated to the embedder (`imp.rs:455-469`,
  `:2107-2119`).
- Link `Acquire`/`Release` → `Relaxed`: ISA delta is real (`ldar`/`stlr` removed), wall-clock unmeasured;
  the CHANGELOG's "defence-in-depth until measured" posture is reasonable — see NONOPT-1 for why the
  measurement that would settle it has still not run.

---

## 3. Code smell

### SMELL-1 (P3) — `Backoff`'s cfg-gated tuple field and temporally-coupled oracle

Covered by PERF-1; listed here because it is primarily a structure problem: a private helper whose *shape*
differs by feature flag (`Backoff(u32)` vs `Backoff(u32, bool)`), whose constructor has two cfg-gated
`return` statements, and whose accessor doc must warn about call order. The return-value version has one
shape in every configuration.

### SMELL-2 (P3) — hard-coded counts that the crate's own policy says never to hard-code

The unsafe-surface counts appear as literals in ten places: `lib.rs:28,258,305,325,330,339,404`,
`README.md:22`, `CHANGELOG.md:14,240` ("exactly EIGHT ... regions"), plus "TEN `unsafe fn`", "SIX
`unsafe {}`", "ONE `unsafe trait`" at `lib.rs:292-303` and "three to six" at `lib.rs:270`. Meanwhile
`Cargo.toml:17-20` ("count drifts ... re-derive via `ls tests/compile_fail/ | wc -l`, do not trust a
number quoted here") and `tests/common/compile_fail.rs:2-4` apply a no-hard-coded-count rule to *other*
counts in the same crate — and `tests/compile_fail.rs:1-2,62` then hard-codes "eight" / "seven of the
eight" anyway. The git history shows what this costs: `ccb48aa`, `ad742cd`, `7306b86`, `f6abce6` are four
commits whose sole purpose was re-synchronising these literals. State the count once (in the "Where unsafe
lives" section, next to the grep that re-derives it) and have every other site say "see 'Where unsafe
lives'".

### SMELL-3 (P3) — the published `.crate` ships a Node.js benchmark driver and a test that drives it

`cargo package --list` includes `scripts/tis_p3_ab_runner.mjs` (1,275 lines), the three
`scripts/tis_p3_ab/*` templates, and `tests/tis_p3_ab_runner_scratch_guard.rs` (755 lines). A `cargo
test` in the extracted package — which the `tagged-index-stack package gates` CI job deliberately runs —
therefore spawns `node`, `git init`/`add`/`commit` in eleven skeleton repos, and four `cargo build`s of a
scratch crate (`build-check` mode compiles the copied `imp.rs` + the 384-line harness + the codegen
wrapper). None of this is about the crate's behaviour; it tests a measurement script's filesystem hygiene.
For a `no_std`, zero-dependency primitive this is the wrong tarball. Add `"scripts/"` and
`"tests/tis_p3_ab_runner_scratch_guard.rs"` to `exclude` together (the test hard-panics in `copy_file` if
the scripts are absent, so they must go together), and keep the scratch-guard suite as a repository-only
test the CI rows already run from the checkout.

### SMELL-4 (P4) — `Cargo.toml` is 65% comment, and the comments narrate review rounds

Of 121 lines, four are configuration the reader needs (`exclude`, the lint table, the optional loom dep,
the feature) and ~80 are prose: an 11-line explanation of a one-line `exclude` (`:17-29`), a 13-line
justification of `incompatible-msrv = "allow"` (`:41-53`), a 23-line essay on why `loom` is optional
(`:56-78`), and a 23-line feature comment (`:89-112`) that cites "Sol-codex run-6 review P2-1" and
"Sol-codex run-9 review P1-1" — reviews a crates.io user cannot read. The manifest ships as
`Cargo.toml.orig`. Two lines per item is the right budget; the reasoning belongs in the ADR that already
exists for it.

### SMELL-5 (P4) — `Debug` on `StackHead` prints an opaque packed integer

`#[derive(Debug)]` on `StackHead` (`imp.rs:487`) yields `StackHead { head: 281474976710655 }` — the reader
has to unpack it by hand to learn "empty, tag 5". A four-line manual impl printing
`StackHead { index: <n|empty>, tag: <t>, sealed: <bool> }` is what a consumer debugging a free-list
actually wants. Same for `ArrayIndexStack` (which derives through it). Ergonomics only.

### SMELL-6 (P4) — `harness_bin.rs:362-368` dead fallback

```rust
u64::try_from(timed_start.elapsed().as_millis().min(u128::from(u64::MAX))).unwrap_or(u64::MAX)
```

After `.min(u64::MAX)` the `try_from` cannot fail; `unwrap_or` is unreachable. `as u64` after the `min`
says the same thing honestly.

---

## 4. "Neuroslop"

The pattern across `src/`, README and CHANGELOG is the same: every fact is stated, then restated with
emphasis, then justified against a hypothetical objection, then cross-referenced to the other places it is
restated, with the review round that prompted it named inline. Representative instances, each with the
concrete duplication:

### SLOP-1 (P3) — the `StackStorage` trait doc (361 lines, `imp.rs:672-1032`)

Three sections — "# Safety" (7 clauses), "The binding: structural vs. value-level obligations"
(`:843-882`, one 40-line paragraph), and "Clause 1's coverage of the shared-head hazard is EXHAUSTIVE"
(`:884-898`) — say clause 1 and clause 3 three times each. The doc itself concedes it at `:755-761`:

> The sections below ... remain the explanatory appendix to this contract, not a replacement for it; the
> full design/audit detail beneath their compact summaries is archived in the repository ADR ...

An appendix that is archived elsewhere and summarised here should not also be here in full. Sample of the
register (`:856-862`):

> What NOTHING enforces — not the type system, not the [`StackOps`] blanket impl, not clause 4's
> release-active guard, and not even the `unsafe impl` acknowledgment itself (which forces every
> implementor to ASSERT the contract but detects no violation) — is the INSTANCE-level half of clause 1 ...

The seven numbered clauses plus the ordering contract plus the hazard table are the contract. Keep those
(~120 lines); move the rest to the ADR it already cites.

### SLOP-2 (P3) — one fact, seven statements: "the seal check runs before the bump, so `tag <= TAG_MAX`"

`imp.rs:256-260` (`pack` doc), `:292-298`, `:300-306`, `:308-316` (three consecutive paragraphs of the
`pack_truncating` doc), `:337-348` (the `debug_assert!` message — eleven lines of prose *inside a panic
string*), `:1552-1562` (push's seal-check comment), `:1632-1634` (push's `tag + 1` comment). The
`pack_truncating` doc additionally spends a paragraph (`:308-316`) explaining what would happen if a future
maintainer removed the seal check, and another (`:318-323`) explaining that the helper "is NOT a
wrap-on-truncate mechanism ... and must never be made to". The first `debug_assert!` message (`:329-335`)
argues for six lines why its own `<=` is not "a general invitation to pass INDEX_MASK". One sentence at
the two call sites and one at the helper is enough; the assert messages should be one line each.

### SLOP-3 (P3) — the published rustdoc narrates the crate's unreleased history

For a 0.1.0 with no prior release, every "now", "no longer", "former", "since <date>" in the API docs
describes a state the reader never saw:

- `lib.rs:266-271`: "Since 2026-09-02 this crate also sets `#![deny(unsafe_op_in_unsafe_fn)]` ... the
  actual unsafe-block count inside those regions rose from three to six".
- `imp.rs:355-359`: "the single, centralized, provably-dead truncation that replaces the seven unchecked
  caller-side casts the old `(u64, u64)` signature forced".
- `imp.rs:157-169` (`_CHECK_BITS` doc) and `:1565-1567`: "the historical `INDEX_MASK == TAIL` coincidence
  at the former width-32 cap".
- `imp.rs:1866-1869`: "the compile-fail successor of the former `array_index_stack_head_still_double_issue`
  runtime demonstration".
- `imp.rs:2013-2018`: "which they can no longer do through `StackStorage::load_next` now that this type
  deliberately does not implement the public `StackStorage` trait".
- `imp.rs:1551`: "Seal check (P1-1 fix)"; `imp.rs:321-322`: "the run-8 P1-1 fix".
- `imp.rs:1461`: "the qualifier is now SEMANTICALLY LOAD-BEARING".
- `imp.rs:372-384` (`TaggedIndex::empty` doc): twelve lines about `sefer-alloc`'s `#[cfg(loom)]`
  `bootstrap::loom_shim` — an internal detail of a *different* crate, shipped in this crate's public docs.

Plus the disclaimer "a repository file, not part of the published package", repeated ten times across
`src/`, README, CHANGELOG and the example. The published docs should describe the crate as it is; the
"was" belongs in `docs/adr/`, which already exists for exactly this and is cited on almost every page.

### SLOP-4 (P3) — CHANGELOG.md: 357 lines of changes to a version that never shipped

`CHANGELOG.md:7-9` says "First release. Everything below is new in this version; nothing has shipped
before it" — and then has a `### Changed` section with three "**BREAKING (unpublished 0.1.0)**" entries
(`:212-273`), a `### Fixed` section (`:275-335`) for bugs that never reached a user, a "Decision history"
subsection, and citations to `docs/reviews/2026-08-31-100751-...round7-oh.md`,
`docs/reviews/2026-09-02-180547-...run-8.md`, "Sol-codex review run 14, P1", "run 16, P2-1" — none of
which ship. The `### Added` bullets are themselves 10-30 lines each (`:44-45` is one 900-character bullet).
A first-release changelog is an inventory: one line per public item, one line per notable property (seal,
H-2, lazy links, loom-checked), MSRV, license. Twenty-five lines. The rest is already in the ADRs.

### SLOP-5 (P4) — README: a 34-line opening paragraph and a 45-line "Notes" section about items the reader cannot call

`README.md:3-34` is one paragraph; its sentence from line 20 to 34 introduces `#![deny(unsafe_code)]`,
"EIGHT audited, item-scoped `#[allow(unsafe_code)]` lint-exception regions", the *crate-private*
`SealedStorage` trait, and the integration-test fixture policy — before the reader has seen `push`/`pop`.
`README.md:245-290` ("Notes") documents `raw_head`, `load_next_for_test`, `store_next_for_test`,
`cas_head_for_test`, `retry_counts_for_test`, `backoff_cap_reached_for_test`, `with_tag_for_test` — every
one feature-gated off in a default build and `#[doc(hidden)]`. A README reader is deciding whether to
depend on the crate; those 45 lines are for the crate's own test authors. The first-contact example
(`:294-304`) asks the reader to understand "clause 3" in its SAFETY comment before any clause has been
introduced.

### SLOP-6 (P4) — comments that cite review rounds instead of explaining the code

My grep for `review|Sol-codex|run-N|P[0-9]-[0-9]|task #|Group[A-Z]|2026-0[89]-` finds 39 hits in
`scripts/tis_p3_ab_runner.mjs`, 21 in `tests/loom_aba.rs`, 17 in `tests/tis_p3_ab_runner_scratch_guard.rs`,
10 in `tests/compile_fail.rs`, 10 in `CHANGELOG.md`, 9 in `src/imp.rs`. Typical (`loom_aba.rs:971-976`):

> Sol-codex review run 14, finding P1 (`docs/reviews/2026-09-03-123945-tagged-index-stack-review-Sol-codex-run-14.md`).
> Why the retry gate (review run 16, finding P2-1 — `docs/reviews/2026-09-03-150547-...run-16.md`): ...

A comment should say *what invariant* the code protects; *who asked for it* is `git blame`'s job. Run-20
P4-2 made this point about `imp.rs`; commit `1e75569` trimmed five comments in the runner. The remaining
~100 sites are the same fix repeated.

---

## 5. Inaccuracies

### INACC-1 (P3) — see BUG-1 (`Backoff` overflow mode) and BUG-2 ("purely for readability")

Both are statements in shipped rustdoc/comments that a reader would act on and be misled by.

### INACC-2 (P4) — `spins` is a stale identifier in 13 places

The per-call counter was renamed from a local `spins` to the `Backoff` struct in `de75127`. It survives as
`spins` in `imp.rs:2262,2286,2290,2313` ("`spins` already saturated at `BACKOFF_SPIN_CAP`", "a regression
that caps `spins` at 0, resets it per iteration"), `tests/threaded_conservation.rs:7,13,23,24,108,203,207,213,217`
(including four assertion messages), and `CHANGELOG.md:146` ("per-call `spins` counter never persisted").
There is no `spins` anywhere in the code.

### INACC-3 (P4) — `CHANGELOG.md:291` "the loom suite stayed 11/11 green"

There are 15 `#[test]`s in `tests/loom_aba.rs` today. The sentence was true when written; in a changelog
for an unreleased version it reads as a current fact.

### INACC-4 (P4) — `CHANGELOG.md:69-71` reports noise as a win

> an out-of-tree A/B of this guard on the single-threaded `churn` bench ... measured the guarded arm
> *faster* at the median (50.58 vs 51.60 ns/op debug-only ...)

A 2% delta on one interleaved A/B of a ~50 ns operation is within run-to-run noise; the crate's own source
comment says it correctly ("measured ≈ free next to the head CAS", `imp.rs:1394-1395`). "Faster" invites a
reader to believe the guard has negative cost. Say "no measurable cost".

### INACC-5 (P4) — `tests/compile_fail.rs:1-2,62` hard-codes the fixture count its own helper forbids

`tests/common/compile_fail.rs:2-4`: "count drifts as new hazards get pinned, re-derive via `grep -c
'^#\[test\]' tests/compile_fail.rs` rather than trusting a number quoted here." `tests/compile_fail.rs:1`:
"eight negative-regression tests"; `:62`: "For seven of the eight fixtures". Both are currently correct
(8 tests, 7 strip `RUSTFLAGS`); the policy is not.

### INACC-6 (P4) — `imp.rs:82-85` `#[inline]` rationale

See PERF-4: "a non-`#[inline]` non-generic private fn would not be cross-crate-inlinable" has not been
true for small functions since Rust 1.75; at MSRV 1.79 the attribute is belt-and-braces, not a regression
guard.

---

## 6. Non-optimalities

### NONOPT-1 (P3) — the arm64 gate's `--mode summary` step never examines the run's own aarch64 wallclock data

Run-20 P3-2 asked for `--mode summary` in `tis-weak-memory-wallclock-gate`; `a1dc3e3` added it
(`ci.yml:3273-3275`). But `modeSummary` reads a hard-coded target set:

```js
const CODEGEN_CSV_TARGETS = ['x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu'];
const WALLCLOCK_CSV_TARGET = 'x86_64-pc-windows-msvc';
```

(`tis_p3_ab_runner.mjs:1124-1125`), so the "independent ratio oracle" re-derives medians from the
*committed Windows* CSV that has been in the repository since the smoke run — while the fresh
`TIS_LINK_ORDERING_WEAK_CAS_GATE_wallclock_aarch64-unknown-linux-gnu.csv` produced two steps earlier is
uploaded as an artifact and never read. The ci.yml comment (`:3262-3271`) states this openly ("re-checks
the committed windows-msvc evidence corpus, NOT this run's fresh aarch64 wallclock output"), which is
honest but means the P3-2 closure is nominal: the job still cannot fail on a bad aarch64 ratio. Give
`modeSummary` a `--target` (or iterate every `*_wallclock_*.csv` present in `docs/perf/`) so the step
checks what the job just measured. Relatedly, `ci.yml:3235-3236` still says the job was "authored ...
WITHOUT ever being executed" and `CHANGELOG.md:183` still says the arm64 wall-clock A/B "is still pending";
publishing with the one measurement that would settle the link-ordering question permanently "pending"
is a choice worth making explicitly — either dispatch the job once before the release or drop the
"pending" framing from the shipped CHANGELOG.

### NONOPT-2 (P4) — six scratch-guard tests exercise one `case` arm

`tests/tis_p3_ab_runner_scratch_guard.rs:300-466`: `out_dir_dot_is_rejected`, `out_dir_dotdot_is_rejected`,
`out_dir_absolute_repo_root_is_rejected`, `out_dir_absolute_temp_victim_is_rejected_and_canary_survives`,
`out_dir_repo_sibling_is_rejected_and_not_created`, `out_dir_symlink_escape_is_rejected_and_canary_survives`.
The runner's handling of `--out-dir` is value-independent (`tis_p3_ab_runner.mjs:189-194`:
`case '--out-dir': fail(...)` before the value is even read), so all six drive the identical statement;
the file's own helper comment says so ("The rejection is mode-independent (argument parsing)",
`:246-247`). Each builds a full skeleton repository (five file copies, `git init`/`add`/`commit`) to do it.
One test with the six values in a loop over one skeleton pins the same invariant; the symlink case adds
nothing over the `victim` case once the flag is rejected before any filesystem access. The suite's real
teeth are `target_dot_and_dotdot_*`, `scratch_root_junction_redirect_*`, and the three lifecycle oracles.

### NONOPT-3 (P4) — the pack-boundary tests overlap five ways

`pack(0x1_FFFF, _) == None` is asserted in `stack_unit.rs:88-93` and `:142-146`; `pack(_, 1 << 48) == None`
in `stack_unit.rs:154`, `:204-211`, `:344-350` and `tag_seal.rs:37-40`; and both boundaries are covered
generatively by `proptest_pack_unpack.rs:81-103`. Each of the three `stack_unit.rs` tests carries a
15-line doc explaining why the *old truncating pack* would have accepted the value — a pack that no user
ever had. One boundary test (exact words at `INDEX_MASK`, `1<<INDEX_BITS`, `TAG_MAX`, `TAG_MAX+1`) plus
the proptests is complete coverage.

### NONOPT-4 (P4) — the backoff-depth oracle asserts a probabilistic event with no slack

`tests/threaded_conservation.rs:200-219` requires that at least one `pop` and one `push` call each lose
≥7 consecutive CASes within 8 threads × 200,000 iterations. On the CI runner classes this has held, and
the start barrier fixed the one observed vacuous run; but on a 2-vCPU runner under external load the
scheduler can serialise the threads enough that no single call ever reaches K = 6. If it flakes, the
right fix is not to relax the assertion but to loop the threaded phase (bounded, e.g. up to 3 rounds)
until the cap counters move — keeping the oracle exact while removing the single-shot dependence on
scheduler luck.

### NONOPT-5 (P4) — test/bench layout nits

- `tests/stack_unit.rs` mixes `TaggedIndex` packing tests, `ArrayLinks` panics, a custom-storage guard
  test (`AlwaysInvalidStorage`, `:404-445`, which belongs with the other implementor fixtures in
  `custom_storage_impl.rs`), `Default` impls, and `with_tag_for_test` boundary tests in one 700-line file.
- `benches/tagged_index_stack_bench.rs:408-425` repeats the published-window protocol description already
  given at `:103-131` verbatim, and `examples/backoff_per_call_latency.rs:241-259` repeats it a third time.
- `README.md:241-243` and `tests/loom_aba.rs:120-122` both carry the loom invocation; CI carries a third
  copy. Fine, but if it changes, three places.

---

## Post-run-20 commits, verified

- `6de4f91` (P4-1): the three per-mode "scratch tree removed on exit" messages are gone; the `finally`
  block is now the sole reporter (`tis_p3_ab_runner.mjs:1266-1274`). Correct.
- `c519410` (P3-4): `debug_assert!(tag <= Self::TAG_MAX, ...)` added to `pack_truncating`
  (`imp.rs:337-348`). Correct; release-inert. Its message is eleven lines (SLOP-2).
- `a1dc3e3` (P3-1/P3-2): Windows job runs `cargo test -p tagged-index-stack` with `setup-node` and
  positive greps for the two junction-branch test names plus fail-on-skip needles (`ci.yml:1755-1775`) —
  good, non-vacuous shape. `--mode summary` added to the arm64 job — nominal only, see NONOPT-1.
- `ad50e0b` (P3-3): three lifecycle oracles (`tis_p3_ab_runner_scratch_guard.rs:617-755`) with a
  post-`mkdtemp` failure injection (`break_skeleton_imp_rs`) and a mechanism oracle
  (`assert_fatal_from_post_mkdtemp_cargo_build`) so a pre-scratch death cannot pass vacuously. Correct
  and well-constructed; the `--keep-scratch` test removes exactly the root it positively identified.
- `1e75569` (P4-2): five runner comments trimmed. Correct; the remaining ~100 sites are SLOP-6.

## Closing summary

**Nothing blocks publication on correctness.** The lock-free core, its orderings, the seal, and the unsafe
boundary are right, and the test infrastructure — loom counterfactuals, real-thread conservation with
activation oracles, compile-fail fixtures with message-level assertions, packaged-crate CI — is unusually
thorough for a first release.

**Before publishing** (P3): fix the two wrong doc statements (BUG-1, BUG-2); collapse the `Backoff` cfg
apparatus into a returned `bool` (PERF-1/SMELL-1); state each hard-coded count once (SMELL-2); exclude the
Node.js driver and its scratch-guard test from the tarball (SMELL-3); make the arm64 summary step check the
run's own CSV, and decide whether the "pending" arm64 measurement ships as pending (NONOPT-1); and cut the
published prose — trait doc, `pack_truncating`, "Where unsafe lives", CHANGELOG, README opening — to the
contract the reader needs (SLOP-1..4). That last item is not cosmetic: the crate's own recent history
(run-19 P2-1, a deterministic test defect hidden in a 500-line security test; run-19 P3-1, tautological
asserts labelled oracles) shows the prose volume is already costing review accuracy.

**Can wait** (P4): the `spins` rename sweep, the changelog/A-B noise wording, `Debug` impls, test
deduplication, the `#[inline]` paragraph, the codegen-NULL note closing the pop-ordering candidate.
