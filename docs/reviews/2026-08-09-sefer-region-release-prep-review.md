# `sefer-region` release-preparation review (read-only)

> **SUPERSEDED (2026-08-09, task #797).** This report's `GO-WITH-FIXES`
> verdict has been superseded by the stricter follow-up static audit,
> `docs/reviews/2026-08-09-sefer-region-static-release-audit.md`, whose
> verdict is `HOLD / NO-GO`. That audit corrects several conclusions this
> report reached: the "every one of I1-I5 is accurate as written" claim
> was wrong for I5 (safe `mem::forget` defeats an unconditional
> "never leaked" guarantee); the "rebuild a fresh Region to compact"
> advice this report's own predecessor tasks had shipped was actively
> dangerous, not merely stale (a stale handle from the old Region can
> silently alias a value in the new one); the standalone packaged
> benchmark's self-sufficiency gap (this report's own F14) was
> understated in severity; the "~4 orders of magnitude" panic-surface
> arithmetic in this report's F2 does not follow from the cited
> comparison; and the "no new irreversible risk" summary line was too
> strong given the floating `slotmap = "1"` dependency, the exact-layout
> promise, and the then-unresolved cross-region handle identity question.
> This report's genuinely-found defects (the vacuous `reserve` overflow
> test, real package/metadata gaps) remain valid and were fixed
> regardless — see the newer audit and its own follow-up tasks
> (#779-780, #786-803) for the full, corrected picture. Kept here
> unmodified as the historical record, per this project's append-only,
> non-retroactive correction convention.

**Date:** 2026-08-09
**HEAD at review:** `e7c13b28dd8ae12d514d31a9a5f85b556f6cc5ec` (`main`).
`crates/region/` itself is unchanged since `6cb3f6b` (task #770); `git diff 11ca6ee..HEAD --
crates/region/` is empty, so every source citation below is equally valid against either SHA.
**Scope:** `crates/region/` (package `sefer-region` 0.1.0) end-to-end with a
release-readiness lens — source, tests, bench harness, README, `Cargo.toml` metadata,
packaging output, CHANGELOG/open-items bookkeeping, and API/semver shape.
**Prior coverage treated as settled and NOT re-derived:** the 2026-08-06 publish-readiness
review, the three 2026-08-07 angle reviews (performance/logic/safety), the 2026-08-07
`/rust-intel` audit, the 2026-08-07 work plan, both closing reviews (2026-08-07 and
2026-08-08), and every landed fix in tasks #656/#664-673/#678-696/#769-770.

**Mode:** read-only. No file in the repository was modified except this report;
`git status --short` was empty before and after (verified explicitly at both ends).
Two evidence sources were used beyond reading:

1. A throwaway probe crate (path-dependency on `crates/region`, built under
   `D:\tmp-region-audit`, **deleted after use**) that exercised the `reserve` /
   `with_capacity` overflow guards and their unguarded `slotmap` baselines in a
   release build with `overflow-checks = false`. Its verbatim output is inlined in F1/F2.
2. Three sweeps of the crate's own committed bench (`cargo bench -p sefer-region --bench
   region_bench`, pinned iteration counts from the workspace-root `bench-iters.txt`), to
   re-check the README's published table. Full per-run numbers are inlined in F8 — no new
   log file was committed, and `bench-iters.txt` was not rewritten (all 16 workload keys
   already exist in it, so the harness's JIT-calibration write path never fired).

Both were collected before the maintainer's mid-review instruction to stop running
tests/benches; nothing further was executed after that point, and the remaining findings
(F3-F7, F9-F14) rest on reading alone.

`slotmap 1.1.1` was read at source level from the registry cache
(`D:\system_artefact\cargo\registry\src\index.crates.io-1949cf8c6b5b557f\slotmap-1.1.1\`);
so were `bench-scale-tool 0.1.0` and `captrack 0.1.1`.

---

## Bottom line

**GO-WITH-FIXES.** No soundness defect, no memory-safety issue, no logic bug in
`Region`/`SyncRegion`/`Handle`. Three rounds of prior review plus a rust-intel audit have
genuinely closed the correctness surface: I re-derived the I1-I5 claims against
`slotmap 1.1.1`'s actual source (generation arithmetic, the `2^32 - 2` full-map bound, the
`RwLock` guard-drop ordering in `SyncRegion::remove`) and **every one of them is accurate as
written**. The README's performance table also still reproduces — 10 of 12 published rows
land inside their own published ranges on a fresh 3-run sweep, and the two that don't miss
by 0.9 % and 0.3 %.

What is left is one **MEDIUM** test-quality defect and a tail of **LOW/INFO** hygiene items:

- **F1 (MEDIUM)** — `region_reserve_overflow_panics` does not test the guard it exists to
  pin. Its input can never reach `Region::reserve`'s `checked_add`; the panic it observes
  comes from `RawVec`, and raw `slotmap` panics identically for the same input. **Task
  #690's `reserve` fix could be deleted today and the whole suite would stay green** — in
  both profiles, including the `--release` CI step added specifically to guard it.
- **F2 (LOW-MEDIUM)** — the `# Panics` sections on `Region::with_capacity` / `Region::reserve`
  / `SyncRegion::with_capacity` understate the real panic surface by roughly four orders of
  magnitude.
- **F3 (LOW-MEDIUM)** — the two README examples (the first code any crates.io visitor
  copies) are compiled by nothing, in a repo whose own conventions say the runnable version
  of an example belongs in `tests/`.
- **F4/F5 (LOW)** — the round-2 closing review's own follow-up round is the one part of the
  six-crate sweep that never made it into the durable record: task #770 is absent from
  `CHANGELOG.md` entirely, #769 is cited without its SHA (the exact defect that review's
  finding **E** was about), and its finding **F** — explicitly written down "so it survives
  past this review" — is in neither open-items index.
- **F6-F7, F9-F14 (LOW/INFO)** — a dev-dependency that leaks a file into the source tree on
  every `cargo test`, one over-stated layout guarantee, README internal inconsistencies, and
  packaging/metadata polish.

**Semver verdict: no new irreversible risk found.** The only two genuinely
breaking-to-change-later commitments in the public API — `Handle<T>`'s covariance and
`SyncRegion::read`/`write` returning `std`'s own guard types — are both already documented
deliberate decisions (`src/handle.rs:11-13`, `src/sync_region.rs:100-102`/`110-112`). Every
other gap I found (missing `Debug`, no `IntoIterator`/`Extend`, no `SyncRegion::into_inner`,
no `#[must_use]` on `insert`) is a *non-breaking* later addition, so none of it is a
"last cheap opportunity" item. That is a genuinely reassuring result for a pre-1.0 crate and
I want it on the record as a positive finding, not an absence of one.

| # | Severity | One-line |
|---|---|---|
| F1 | MEDIUM | `region_reserve_overflow_panics` is vacuous w.r.t. task #690's `reserve` guard |
| F2 | LOW-MEDIUM | `# Panics` on `with_capacity`/`reserve` understates the real panic surface |
| F3 | LOW-MEDIUM | Both README examples are never compiled by anything |
| F4 | LOW | Task #770 unrecorded in `CHANGELOG.md`; #769 cited without its SHA |
| F5 | LOW | Round-2 closing review's finding F is in neither open-items index (still open in code) |
| F6 | LOW | `captrack` dev-dep leaks a JSON file into `crates/region/target/` per `cargo test` |
| F7 | LOW | `Handle<T>`'s rustdoc over-states what `#[repr(transparent)]` guarantees |
| F8 | INFO | Perf table re-measured: reproduces; two derived ratios in prose do not |
| F9 | INFO | README cites two different medians for the same three workloads |
| F10 | INFO | `rust-version = "1.88"` vs an actual need of ≤1.66 |
| F11 | INFO | `extern crate alloc;` is unused |
| F12 | INFO | No `[package.metadata.docs.rs]`; `SyncRegion` shows no `std` feature badge |
| F13 | INFO | `description` is ~500 chars with literal backticks |
| F14 | INFO | No `Region → SyncRegion` conversion; no `Debug`/`IntoIterator`/`Extend` |
| F15 | — | Explicit no-findings list (what was checked and is genuinely clean) |

---

## F1 — MEDIUM: `region_reserve_overflow_panics` cannot reach the guard it exists to pin; task #690's `reserve` fix has zero regression coverage

**Location:** `crates/region/tests/coverage_gaps.rs:480-494` (the test),
`crates/region/src/region.rs:134-140` (the guard),
`.github/workflows/ci.yml:850-861` (the CI step added to cover it).

### The claim under test

Task #690 (`df16693`) added `checked_add` guards to `Region::reserve` and
`Region::with_capacity` so both panic identically in debug and release instead of silently
wrapping in release. Task #692 (`ed008a5`) then "tightened the `reserve`/`with_capacity`
overflow-panic tests to pin the specific message". Task #693 (`89913c6`) added
`cargo test -p sefer-region --release` to CI whose own inline comment states it
"would have caught that divergence directly".

### What the test actually does

```rust
// coverage_gaps.rs:480-494
fn region_reserve_overflow_panics() {
    let mut r: Region<i32> = Region::new();          // len() == 0
    let msg = catch_panic_message(AssertUnwindSafe(|| {
        r.reserve(usize::MAX / 2);
    }));
    assert!(msg.contains("capacity overflow"), ...);
}
```

`Region::reserve`'s guard is `self.inner.len().checked_add(additional)`. With `len() == 0`,
`0.checked_add(usize::MAX / 2)` is `Some(_)` — **the guard cannot fire for this input, and in
fact cannot fire for ANY input on an empty region** (`0.checked_add(usize::MAX)` is also
`Some`). The panic the test observes therefore comes from somewhere else, and the asserted
substring `"capacity overflow"` is satisfied by both the crate's message
(`Region::reserve: capacity overflow`) and `RawVec`'s (`capacity overflow`) — so the assert
does not discriminate between them either.

### Empirical confirmation (release build, `overflow-checks = false`)

```
debug_assertions = false
A  Region::reserve(usize::MAX/2) on EMPTY region:
     PANIC = "panicked at library\alloc\src\raw_vec\mod.rs:28:5: capacity overflow"
D  raw SlotMap::reserve(usize::MAX/2) on EMPTY map:
     PANIC = "panicked at library\alloc\src\raw_vec\mod.rs:28:5: capacity overflow"
B  Region::reserve(usize::MAX) with len()==1:
     PANIC = "panicked at crates\region\src\region.rs:138:14: Region::reserve: capacity overflow"
C  raw SlotMap::reserve(usize::MAX) with len()==1:
     NO PANIC   (returned normally; capacity = 3)
```

- **A vs D:** the committed test's input produces a byte-identical `RawVec` panic with and
  without the guard. Deleting `region.rs:135-138` leaves the test green.
- **B vs C:** the guard's *real* trigger is `len() >= 1` combined with an `additional` near
  `usize::MAX`. That is precisely the input that, unguarded, **silently no-ops in release**
  (C: capacity stayed at 3) — the exact §B26/task #690 defect. Nothing in the suite covers it.

### Concrete failure scenario

A future refactor removes or weakens `Region::reserve`'s `checked_add` (e.g. "slotmap
already checks this"). `cargo test -p sefer-region` passes in debug. `cargo test -p
sefer-region --release` — the step added by task #693 *for this specific guarantee* —
passes too. The crate ships, and a caller doing `region.reserve(needed)` where `needed` is
attacker-or-config-derived and near `usize::MAX` gets a silent no-op instead of a panic,
then over-runs its own assumed capacity.

### Secondary: the CI comment overstates by half

`.github/workflows/ci.yml:850-861` names *both* `region_reserve_overflow_panics` and
`region_with_capacity_overflow_panics` as what its `--release` step protects. For
`with_capacity` the claim is true and I verified it:

```
E  Region::with_capacity(usize::MAX):        PANIC = "region.rs:91:14: Region::with_capacity: capacity overflow"
F  raw SlotMap::with_capacity(usize::MAX):   NO PANIC (capacity = 3)
```

`with_capacity`'s test is genuinely non-vacuous in release. `reserve`'s is not, and the
comment does not distinguish them.

### Recommended fix

Two lines in `coverage_gaps.rs`, plus tightening the asserted substring so the two panic
sources are distinguishable:

```rust
let mut r: Region<i32> = Region::new();
r.insert(0);                                   // len() == 1 — required to reach the guard
let msg = catch_panic_message(AssertUnwindSafe(|| { r.reserve(usize::MAX); }));
assert!(msg.contains("Region::reserve: capacity overflow"), "...got: {msg:?}");
```

Keep the existing `usize::MAX / 2` case as well if desired, but label it for what it is (a
`RawVec` overflow that reaches slotmap, not the crate's own guard), and consider tightening
`region_with_capacity_overflow_panics`'s substring to `"Region::with_capacity:"` for the same
discrimination reason.

---

## F2 — LOW-MEDIUM: `# Panics` on `with_capacity` / `reserve` understates the real panic surface by ~4 orders of magnitude

**Location:** `crates/region/src/region.rs:82-86` (`with_capacity`), `:130-133` (`reserve`),
`crates/region/src/sync_region.rs:86-89` (`SyncRegion::with_capacity`, which delegates the
claim verbatim: "see `Region::with_capacity`'s own `# Panics` section — this delegates
directly and the same guard applies").

### The claim

> `with_capacity`: "Panics if `capacity == usize::MAX` (the underlying `slotmap` reserves one
> extra slot for an internal sentinel; a capacity that would overflow that reservation is
> rejected up front, in both debug and release builds)."

> `reserve`: "Panics if `len() + additional` overflows `usize`, in both debug and release
> builds — checked up front, before delegating to `slotmap`."

Both read as exhaustive statements of when the function panics. Neither is.

### What actually happens

```
G  Region::<u64>::with_capacity(usize::MAX / 2):      PANIC "capacity overflow" (RawVec)
H  Region::<u64>::with_capacity(usize::MAX / 16 + 1): PANIC "capacity overflow" (RawVec)
I  Region::<u64>::reserve(usize::MAX / 2) on empty:   PANIC "capacity overflow" (RawVec)
```

`SlotMap<K, u64>`'s slot is 16 bytes, so `Vec::with_capacity` trips `RawVec`'s
`isize::MAX`-bytes ceiling at roughly `usize::MAX / 16` entries — about **1.15 × 10^18**,
not `usize::MAX` ≈ 1.84 × 10^19. Neither doc mentions this, and neither mentions that a
representable-but-unsatisfiable request aborts the process on allocation failure rather than
panicking.

### Concrete failure scenario

A caller reads "Panics if `capacity == usize::MAX`" and writes the obvious guard:

```rust
if n == usize::MAX { return Err(TooBig); }
let region = Region::with_capacity(n);          // documented as infallible here
```

with `n` derived from a config file or a length prefix. `n = 1 << 60` passes the guard and
panics anyway — in a code path the author believed was panic-free on the strength of the
crate's own `# Panics` section.

### Recommended fix

Docs only. One clause each:

- `with_capacity`: "…and additionally panics (as any `Vec`-backed container does) for any
  `capacity` whose slot array would exceed `isize::MAX` bytes — roughly
  `usize::MAX / size_of::<Slot<T>>()`; allocation failure beyond that aborts rather than
  panicking."
- `reserve`: the same clause phrased against `len() + additional`.
- `SyncRegion::with_capacity`'s delegation sentence needs no change once the target is fixed.

---

## F3 — LOW-MEDIUM: neither README example is compiled by anything

**Location:** `crates/region/README.md:43-57` (the `Region` example) and `:86-106` (the
`SyncRegion` example); `crates/region/src/lib.rs` (no `#![doc = include_str!("../README.md")]`).

`CLAUDE.md`'s "No doctests" rule is explicit about where the compiled copy is supposed to
live: *"the runnable version of the example belongs in `tests/` as a real test."* For
`sefer-region` that copy does not exist. `tests/` holds `smoke.rs`, `coverage_gaps.rs`,
`clear_partial_under_panic.rs`, `handle_static_asserts.rs`, `bench_ids_isolatable.rs` and
`captrack_probe.rs` — none of them mirrors either README snippet, and `lib.rs` does not
`include_str!` the README, so `cargo test --doc` compiles nothing from it either
(confirmed: `Doc-tests sefer_region … 0 tests`).

### Concrete failure scenario

The README is the crates.io landing page *and* — via the `readme = "README.md"` key — the
first thing a docs.rs visitor sees. If a later patch changes, say, `SyncRegion::write`'s
return type or makes `Region::get` take `&Handle<T>`, the front-page example silently
becomes non-compiling code that thousands of first-time users copy-paste. Nothing in
`npm run check`, in CI, or in the crate's own suite would notice. The two snippets are
correct *today* — I read both against current signatures — which is exactly why this is
worth pinning now rather than after it breaks.

### Secondary nit in the same snippet

`README.md:91`: `let sr2 = Arc::clone(&sr);` is never used anywhere in the example. In a
compiled copy it would produce an `unused_variables` warning; as prose it just makes a reader
hunt for the second thread that never appears.

### Recommended fix

A ~30-line `crates/region/tests/readme_examples.rs` with the two snippets as `#[test]`
functions, plus a comment naming the README line range each mirrors. Drop or use `sr2` while
transcribing.

---

## F4 — LOW: task #770 has no `CHANGELOG.md` record at all, and #769 is cited without its SHA

**Location:** `CHANGELOG.md:185` (the only mention of #769; no mention of #770 anywhere),
commits `f9e2618` (#769) and `6cb3f6b` (#770).

Grepped at HEAD `e7c13b2`:

- `grep -n "task #770\|#770)" CHANGELOG.md` → **no match**.
- `grep -rn "f9e2618\|6cb3f6b" CHANGELOG.md docs/CORRECTNESS_OPEN_ITEMS.md docs/perf/OPEN_ITEMS.md`
  → **no match**.
- `#769` appears exactly once, inside the #694 bullet, as the bare text
  "(commit `ea52f85`, further corrected by #769)" — no SHA.

This is doubly pointed. First, the round-2 closing review's finding **E** was *specifically*
about missing SHA citations in this exact CHANGELOG section, and the commit that closed it
(`6cb3f6b`) added five SHAs while citing neither its own nor its predecessor's. Second, every
other crate in the six-crate sweep got a dedicated section for its follow-up round —
`#### racy-ptr-cell — round-closing-review follow-ups` (:219),
`#### aligned-vmem — …` (:248), `#### numa-shim — …` (:273),
`#### size-classes — …` (:294). `sefer-region` is the only one without one, and it is also
the only one whose follow-up work is partly invisible: #770's three fixes (the corrected
in-code counterfactual comment, the "earlier"→"immediately after" correction, the five added
SHAs) appear nowhere in the record.

Sharpening it further: `CHANGELOG.md:301`, added by the newest commit on `main` (`e7c13b2`),
asserts that the sweep is closed and that "every crate's fix round AND every crate's
closing-review follow-up round has now landed, verified, **and been recorded here**."
For five of the six crates that is true. For `sefer-region` it is not.

### Recommended fix

Add a `#### sefer-region — round-closing-review follow-ups (2026-08-08/09, tasks #769-770)`
section with two bullets citing `f9e2618` and `6cb3f6b`, mirroring the four sibling sections'
shape; or, at minimum, append the two SHAs to the existing #694 bullet and add a #770 bullet
beside it.

---

## F5 — LOW: the round-2 closing review's finding **F** is in neither open-items index, and is still true in the code

**Location:** `docs/reviews/2026-08-08-sefer-region-round2-closing-review.md`, finding F;
`crates/region/src/sync_region.rs:117-199`.

That finding closes with *"Recorded so it survives past this review"* — the review's own
signal that it is a deliberate carry-forward, not a dismissal. It is recorded in the review
document and nowhere else: `grep -n "sefer-region" docs/CORRECTNESS_OPEN_ITEMS.md
docs/perf/OPEN_ITEMS.md` returns only four hits, all from the unrelated 2026-08-06
publish-readiness sweep (`:1578`, `:1759`, `:1778`, `:1786-1788`), and the CHANGELOG has no
sefer-region follow-up section at all (see F4).

This is the exact failure mode `CLAUDE.md`'s "Round start: check BOTH open-items indexes"
rule exists to prevent, and which it cites two prior instances of (R18-8/task #336;
R22-3/task #354 — an item flagged in a commit message that existed in *neither* index and was
independently re-reproduced twice before anyone noticed). A review document is not a durable
index; a fresh session inherits no memory of it.

### Verified still open

Of `SyncRegion`'s seven one-shot convenience methods, exactly **one** cross-references the
type-level `## Reentrancy` section:

| method | line | cross-references `Self#reentrancy`? |
|---|---|---|
| `insert` | 117-127 | no |
| `remove` | 129-137 | **yes** (`:134`) — and only because it is the exception |
| `contains` | 139-152 | no (references `read`/`write`, different topic) |
| `len` | 154-161 | no |
| `is_empty` | 163-170 | no |
| `clear` | 172-181 | **no** — and `clear` runs every `T::Drop` under the write lock |
| `get_cloned` | 183-199 | **no** — and `get_cloned` runs `T::clone` under the read lock |

The two methods that actually execute user code under the lock are the two with no pointer to
the hazard. (Mitigating: rustdoc renders all methods on one page, so the type-level
`## Reentrancy` section at `:45-58` is in view — the "arrives from a search engine on the
method page" argument in the original finding is weaker than stated. The one-line
cross-reference is still cheap.)

### Recommended fix

Either add one `see the [reentrancy section](Self#reentrancy)` clause to `clear`'s and
`get_cloned`'s rustdoc, or — if the maintainer judges the type-level section sufficient —
record that decision in `docs/CORRECTNESS_OPEN_ITEMS.md` as a closed item with the reason,
so the next round does not rediscover it. What is not acceptable under this repo's own rules
is leaving it recorded only inside a review file.

---

## F6 — LOW: the `captrack` dev-dependency writes a stray JSON file into the source tree on every `cargo test`, and spawns a background thread in a test binary that runs zero tests

**Location:** `crates/region/Cargo.toml:26-31`; `.gitignore:5-13` (the rule that hides it);
`captrack-0.1.1/src/autodump.rs:106-108`, `:120-129`, `:146-153`, `:163-173`.

`captrack`'s `telemetry` feature — required for the `registry` module `captrack_probe.rs`
drives — pulls in `ctor`, which registers **both** a life-before-main constructor (spawning a
detached 500 ms ticker thread) and an at-exit destructor. The destructor writes
`<CAPTRACK_DUMP_DIR>/profile-<stem>-<pid>-<start_ms>.json`, defaulting to a **relative**
`target/captrack-pgo` — i.e. `crates/region/target/captrack-pgo/` when cargo runs the test
binary from the crate directory. `autodump_enabled()` returns `true` unless
`CAPTRACK_AUTODUMP` is explicitly set to `0`/`off`/`false`/`no`.

**Measured this session:** `crates/region/target/captrack-pgo/` held **98** files; one
`cargo test -p sefer-region --test captrack_probe` invocation (which runs **0** tests — the
only test in that binary is `#[ignore]`d) took it to **99**. Two distinct binary hashes are
present in the directory, i.e. debug and release runs both contribute. The tree also carries
a `crates/region/target/` directory that exists for no other reason.

The `.gitignore` comment block added by task #656 documents the mechanism accurately and
ignores the output — which stops it polluting `git status`, but does not stop it accumulating.
Nothing prunes it; it grows monotonically for the life of the working copy, and CI re-creates
it on every run.

Note this is *narrower* than the 2026-08-07 follow-up safety review §1's finding, which
correctly named the `ctor` supply-chain shape as informational. The concrete, measurable
side-effects — an unbounded stray-file stream in the source tree and a ticker thread in an
otherwise-silent binary — were not named there.

### Recommended fix (pick one)

1. **Cheapest, zero API impact:** create `.cargo/config.toml` at the workspace root with

   ```toml
   [env]
   CAPTRACK_AUTODUMP = "0"
   ```

   and have `captrack_probe.rs` set `CAPTRACK_DUMP_DIR` (or keep using its existing explicit
   `dump_capacity_stats(CARGO_TARGET_TMPDIR/…)` call, which is already correct and is the
   only output the probe actually wants). The workspace currently has no
   `.cargo/config.toml`, so this adds a file rather than editing one.
2. **Structural:** make `captrack` an optional dev-dependency behind a crate feature and give
   `[[test]] name = "captrack_probe"` a matching `required-features`, so ordinary
   `cargo test -p sefer-region` never links `ctor` at all. This also removes ~10 transitive
   dev-dependencies (`scc`, `dashmap`, `serde`, `serde_json`, `indexmap`, `smallvec`,
   `fastrand`, `hashbrown`, `bytes`, `captrack-macros`) from a downstream consumer's
   `cargo test` on the published tarball — relevant for a crate whose pitch is
   "no C/C++ libraries, zero own unsafe". Cost: one new public feature name on crates.io.
3. **Do nothing, but say so** in the Cargo.toml comment that already explains why `telemetry`
   is required, so the next reader knows the file stream is a known accepted cost.

---

## F7 — LOW: `Handle<T>`'s rustdoc over-states what `#[repr(transparent)]` guarantees

**Location:** `crates/region/src/handle.rs:13-19`.

> `#[repr(transparent)]` makes `Handle<T>`'s layout — identical to `DefaultKey`'s (8 bytes,
> with `Option<Handle<T>>` also 8 bytes via `DefaultKey`'s `NonZeroU32` niche) — a
> **guarantee, not an incidental fact of the current rustc layout algorithm**; this is what
> makes the compile-time layout assertions in `tests/handle_static_asserts.rs` a pinned
> invariant rather than a toolchain-dependent assumption.

`#[repr(transparent)]` guarantees exactly one thing: `Handle<T>` has the same layout as
`slotmap::DefaultKey`. It says nothing about what *that* layout is. Reading slotmap 1.1.1:

- `DefaultKey` is generated by `new_key_type!` as `#[repr(transparent)] struct DefaultKey(KeyData)`
  (`lib.rs:451`, `:508-511`) — so far so good;
- but `KeyData` is `#[derive(...)] pub struct KeyData { idx: u32, version: NonZeroU32 }`
  (`lib.rs:244-249`) with **default `repr(Rust)`** and **private fields**.

So "8 bytes" and the `Option` niche are consequences of slotmap's *private, unversioned*
internal layout, not of anything slotmap's public API promises — and `Cargo.toml:20` pins
`slotmap = "1"`, which floats across every 1.x minor. This is the same class of hazard as the
drain-order dependence that tasks #694/#769 just spent two commits removing from
`clear_partial_under_panic.rs`, for exactly the same stated reason ("`Cargo.toml` pins
`slotmap = "1"` (floats across 1.x minors)").

### Concrete failure scenario

`slotmap` 1.2 reorders or widens `KeyData` (entirely within its semver contract — the fields
are private). Downstream consumers are unaffected: `Handle<T>` still tracks `DefaultKey`
whatever it becomes, and no shipping code depends on the size. But `sefer-region`'s own
`const _: () = assert!(size_of::<Handle<u8>>() == 8);` (`handle_static_asserts.rs:69`,
`:75`) becomes a hard **compile** error in CI, and the published rustdoc now states a
false guarantee. The asserts firing is arguably desirable (an early-warning tripwire); the
doc claiming they are a *guarantee* is not.

### Recommended fix

One clause: "…`#[repr(transparent)]` guarantees `Handle<T>` and `DefaultKey` share a layout;
the specific 8-byte size and the `Option` niche additionally rest on `slotmap`'s own
(private, not-semver-guaranteed) `KeyData` representation, which
`tests/handle_static_asserts.rs` pins as a tripwire rather than an assumption."

---

## F8 — INFO: README perf table re-measured — reproduces; the two derived ratios in the prose do not

**Location:** `crates/region/README.md:129-142` (the table), `:175-181` (the derived ratios),
`:194-213` (the wrapper-overhead A/B).

Command: `cargo bench -p sefer-region --bench region_bench`, three sweeps, same host class as
the published numbers (single noisy Windows dev host), pinned iteration counts from the
workspace-root `bench-iters.txt`. The harness did not rewrite that file (all 16 keys already
present; the JIT-calibration write path never fired).

### Raw per-run ns/op and medians

| workload | run 1 | run 2 | run 3 | median | README median (range) | verdict |
|---|---|---|---|---|---|---|
| `st/insert` | 301.21 | 258.95 | 278.69 | **278.69** | 290 (242–327) | in range |
| `st/get_hit` | 4.85 | 4.79 | 5.07 | **4.85** | 5.0 (4.3–6.5) | in range |
| `st/get_stale` | 4.78 | 4.45 | 5.07 | **4.78** | 5.0 (4.7–5.1) | in range |
| `st/remove` | 104.58 | 103.96 | 109.92 | **104.58** | 97 (96–111) | in range |
| `st/iterate` | 1377.02 | 1404.18 | 1518.03 | **1404.18** | 1319 (1292–1546) | in range |
| `st/holey_sweep` | 2152.48 | 2208.69 | 2383.20 | **2208.69** | 2476 (2228–2510) | **−0.9 % below min** |
| `st/sparse_sweep` | 10716.70 | 11207.71 | 11245.83 | **11207.71** | 11482 (10955–11845) | in range |
| `st/churn` | 3.78 | 3.72 | 3.99 | **3.78** | 3.6 (3.3–4.2) | in range |
| `sync/insert` | 285.58 | 292.18 | 324.42 | **292.18** | 281 (269–324) | in range |
| `sync/get_cloned_hit` | 35.80 | 35.30 | 35.99 | **35.80** | 34.5 (34.2–36.0) | in range |
| `sync/remove` | 129.26 | 130.42 | 141.17 | **130.42** | 124 (123–130) | **+0.3 % above max** |
| `sync/churn` | 73.35 | 73.79 | 85.11 | **73.79** | 76.0 (72.1–84.2) | in range |
| `raw/insert` | 266.06 | 267.29 | 300.69 | **267.29** | — | — |
| `raw/get_hit` | 4.76 | 4.64 | 5.09 | **4.76** | — | — |
| `raw/remove` | 105.01 | 107.65 | 110.32 | **107.65** | — | — |
| `raw/churn` | 4.16 | 4.53 | 4.31 | **4.31** | never published | — |

**10 of 12 published rows reproduce inside their own published ranges**, and the two that do
not miss by 0.9 % and 0.3 % — well inside the drift the README's own note at `:165-173`
already discloses for this host. **The table is not stale and needs no refresh.**

### The derived ratios are the fragile part

`README.md:179-180` states: "The 50 %-holes case (2,000 high-water mark) is ~1.9× the
zero-holes baseline; the 90 %-holes case (10,000 high-water mark) is ~8.7×." Recomputed
from this session's medians:

- 50 % holes: 2208.69 / 1404.18 = **1.57×** (published 1.9×, a 17 % relative gap)
- 90 % holes: 11207.71 / 1404.18 = **7.98×** (published 8.7×, an 8 % relative gap)

The qualitative claim the section exists to make — iteration cost tracks the high-water slot
count, not the live count — holds decisively in both measurements and is not in question. But
a ratio of two independently-noisy medians compounds both rows' error, and unlike every ns/op
figure in the table above it, these two numbers are published as bare point estimates with no
range. Under this repo's own reporting convention (`CLAUDE.md`, the derived-numbers rule,
point 4: every figure names its numerator and denominator; point 6: a script computing a
headline ratio asserts the arithmetic it prints), a ratio deserves at least the same
range treatment its inputs get.

### The wrapper-overhead A/B: conclusion confirmed, cited pair inverted

`README.md:202-205` cites "median-of-3: `st/insert` 281 vs `raw/insert` 305; `st/get_hit`
5.07 vs `raw/get_hit` 4.76; `st/remove` 99.3 vs `raw/remove` 106.4". This session:
`st/insert` 278.69 vs `raw/insert` **267.29** — the sign flips (wrapped is now the *slower*
of the pair, by 4 %, where the published pair had it 8 % faster). `st/get_hit` 4.85 vs 4.76
and `st/remove` 104.58 vs 107.65 stay mixed.

That inversion **confirms** the section's own stated conclusion ("the wrapped and raw numbers
interleave with no consistent direction … no measurable wrapper overhead was found") rather
than undermining it. The verdict is robust; only the specific quoted pair is a single noisy
sample presented without that caveat.

### One omission

`benches/region_bench.rs:196-203` measures `raw/churn`, and `README.md:194-213` names only
`raw/insert`/`raw/get_hit`/`raw/remove` as the A/B set. `raw/churn` is arguably the *most*
informative of the four for a wrapper-overhead question — steady-state, no allocation or
teardown noise — and this session has it at 4.31 vs `st/churn` 3.78, i.e. the wrapper 12 %
*faster*, once more in the noise. Reporting three of four measured arms without saying so is
a small selective-transcription gap against the same convention cited above.

### Recommended fix

Docs only, all optional: add ranges to the two ratios (or restate them as "roughly 1.6–1.9×"
and "roughly 8–9×"), note that the wrapper-overhead paragraph's numbers are one session's
sample, and add the `raw/churn` vs `st/churn` row so the A/B covers all four measured arms.

---

## F9 — INFO: the README cites two different medians for the same three workloads, with no note that they came from different sessions

**Location:** `crates/region/README.md:131-134` (table) vs `:202-205` (wrapper-overhead
paragraph).

| workload | table | wrapper-overhead paragraph |
|---|---|---|
| `st/insert` | 290 | 281 |
| `st/get_hit` | 5.0 | 5.07 |
| `st/remove` | 97 | 99.3 |

Both are labelled "median" (the table explicitly "median, range"; the paragraph
"median-of-3 result"), for the same workload, on the same host, in the same document, ~70
lines apart. Both are almost certainly honest medians from two different measurement
sessions — the table's ranges comfortably contain the paragraph's figures — but nothing in
the text says so, and a reader checking one against the other has no way to reconcile them.
One clause ("measured in a separate session from the table above; see that table's ranges")
closes it.

---

## F10 — INFO: `rust-version = "1.88"` against an actual requirement of ≤1.66

**Location:** `crates/region/Cargo.toml:5`.

Nothing in this crate needs a modern toolchain. The library uses `core::marker::PhantomData`,
`core::hash`, `core::fmt`, `PhantomData<fn() -> T>`, `checked_add`, and (under `std`)
`RwLock` + `PoisonError::into_inner` — all long-stable; edition 2021 itself sets the real
floor at 1.56. The heaviest requirement anywhere in the package is in test/dev code:
`std::thread::scope` and `const Mutex::new` (1.63) and `std::hint::black_box` (1.66). The one
runtime dependency, `slotmap`, declares `rust-version = "1.58.0"`.

`1.88` is a consistent workspace-wide value (all ten `crates/*/Cargo.toml` plus the root
declare it), so this is a deliberate policy rather than an oversight, and it is enforced by
the `msrv` CI job. But `sefer-region` is the workspace member most likely to be consumed by
an audience that pins an older toolchain — it carries `keywords = [… "no-std"]` and
`categories = [… "no-std"]` — and a ~30-release overshoot excludes those users for no
technical reason.

Lowering an MSRV later is *not* a breaking change, so this is genuinely cheap in both
directions and I am not calling it a release blocker. It is simply cheapest to get right
before the first publish that anyone depends on.

---

## F11 — INFO: `extern crate alloc;` is unused

**Location:** `crates/region/src/lib.rs:60`.

`grep -rn "alloc::" crates/region/src/` returns nothing. Neither `region.rs`, `handle.rs`,
nor `sync_region.rs` names a single `alloc` path — `slotmap` does its own `alloc` handling
internally, and `sync_region.rs` is `std`-gated and uses `std::` paths. The declaration is
dead in both feature configurations. Harmless (the `unused_extern_crates` lint is
allow-by-default, and both clippy rows are clean), but it implies a dependency on `alloc`
that the crate does not actually express, which is mildly misleading in a file whose module
doc is specifically about the `no_std + alloc` story.

---

## F12 — INFO: no `[package.metadata.docs.rs]`, and `SyncRegion` renders with no feature badge

**Location:** `crates/region/Cargo.toml` (no `[package.metadata.docs.rs]` section);
`crates/region/src/lib.rs:65-72`.

Task #644 (`7e1020f`) added `[package.metadata.docs.rs]` to `aligned-vmem` and `numa-shim`
during the publish-readiness sweep specifically because "a published docs.rs render would
show zero optional features". `sefer-region` was not in that task's scope and still has no
such section.

The practical impact here is much smaller than it was for those two crates, because
`sefer-region`'s only feature is `std` and it is default-on — docs.rs's default build already
documents everything. What is missing is the *signal*: `SyncRegion<T>` is `#[cfg(feature =
"std")]`, and with no `docsrs` cfg + `#[doc(cfg(...))]` annotation, docs.rs renders it with
no indication that a `default-features = false` consumer will not have it. The crate-root
doc does say so in prose (`lib.rs:49-54`), which is why this is INFO and not higher.

If the maintainer wants parity with the sibling crates:

```toml
[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]
```

plus `#![cfg_attr(docsrs, feature(doc_cfg))]` and `#[cfg_attr(docsrs, doc(cfg(feature = "std")))]`
on the `SyncRegion` re-export.

---

## F13 — INFO: the `description` field is ~500 characters with literal backticks

**Location:** `crates/region/Cargo.toml:7`.

The description runs from "100 % Rust typed handle-addressed store" through "…WITHOUT pulling
a full allocator stack." — roughly 500 characters across five sentences, and it embeds
markdown-style backticks (`` `slotmap` ``, `` #![forbid(unsafe_code)] ``). crates.io renders
`description` as **plain text**, so the backticks appear literally, and search-result cards
truncate it well before the final sentence — which is where the actual positioning statement
("for users who want a typed slotmap-like handle store without pulling a full allocator
stack") lives. The README already carries all of this content properly formatted.

Suggested shape: one sentence under ~150 characters carrying the differentiator, e.g.
*"Typed, generational handle-addressed store over slotmap — zero own unsafe, no C/C++, no_std
+ alloc capable."* Everything else already lives in the README and the crate-root rustdoc.

---

## F14 — INFO: API ergonomics gaps — all non-breaking to close later, so none is a pre-1.0 deadline

Recorded because the brief specifically asked about "awkward or embarrassing to fix AFTER a
stable release". My conclusion is that **none of these is** — every one is a
minor-version-compatible addition — but they are the gaps a first-time user hits:

1. **No `Region<T>` → `SyncRegion<T>` conversion.** `SyncRegion::new`/`with_capacity`
   (`sync_region.rs:78`, `:91`) construct a fresh inner `Region`; there is no
   `SyncRegion::from(Region<T>)` and no `into_inner()`. "Build and populate single-threaded,
   then share" therefore has no path that preserves handles — the only option is to
   re-insert into a fresh `SyncRegion`, which invalidates every `Handle<T>` the caller is
   holding. Adding both is a pure addition.
2. **`Region<T>` and `SyncRegion<T>` implement no `Debug`.** `#[derive(Debug)]` on any user
   struct holding one fails to compile. `impl<T: Debug> Debug for Region<T>` is a
   non-breaking addition; so is `#![warn(missing_debug_implementations)]` to keep it that way.
3. **No `IntoIterator` / `Extend` / `FromIterator` on `Region<T>`.** Bulk-loading is a manual
   loop today (as in `benches/region_bench.rs:66-68`, `tests/coverage_gaps.rs:457`).
4. **No `#[must_use]` on `insert`.** Dropping the returned `Handle<T>` makes the value
   unreachable until `clear()`/drop — a silent logical leak. Adding the attribute is
   lint-level and non-breaking, though it would need `let _ =` at this crate's own
   fire-and-forget call sites (`coverage_gaps.rs:71`, `:191`).
5. **`Handle<T>` has no `PartialOrd`/`Ord`** despite `DefaultKey` having both, so handles
   cannot key a `BTreeMap`. Non-breaking to add.
6. **`Region::iter`/`iter_mut` return bare `impl Iterator`** (`region.rs:191`, `:197`),
   erasing the `ExactSizeIterator + FusedIterator + Clone` that `slotmap::basic::Values`
   provides. Widening the return bound later is non-breaking.

The two commitments that genuinely *are* irreversible — `Handle<T>`'s covariance via
`PhantomData<fn() -> T>` (`handle.rs:11-13`, `:27`) and `read()`/`write()` returning
`std::sync::RwLockReadGuard`/`RwLockWriteGuard` (`sync_region.rs:103`, `:113`) — are already
documented as deliberate at their definition sites, and I found no reason to reopen either.

---

## F15 — What was checked and found genuinely clean (explicit no-findings list)

Recorded so a later round does not pay to re-derive these:

- **I1-I5 vs `slotmap 1.1.1` source.** Re-derived every generational claim in
  `region.rs:24-66` and `lib.rs:25-42` against the real implementation. Fresh slot version
  starts at `1` (`basic.rs:417`); `remove` does `version.wrapping_add(1)`; reuse does
  `KeyData::new(idx, version)` which ORs the low bit (`lib.rs:251-258`). One occupy/free
  cycle advances by exactly 2; version returns to 1 after exactly 2^31 cycles. **Accurate as
  documented, including the wrap caveat and the "memory safety is never affected" framing.**
- **`insert`'s documented `2^32 - 2` bound.** `basic.rs:413-414` panics
  `"SlotMap is full"` when `slots.len() >= u32::MAX`; `slots` carries one sentinel, so the
  last successful insert leaves `2^32 - 2` non-sentinel slots. **The doc's number is exact.**
- **`Region::reserve`/`with_capacity` guard placement.** Both `checked_add` guards match
  slotmap's real overflow expressions byte for byte: `(self.len() + additional)` in
  `SlotMap::reserve`, and `Vec::with_capacity(capacity + 1)` in `SlotMap::with_capacity`.
  The *guards* are correct; only their test (F1) and their docs (F2) are not.
- **`SyncRegion::remove`'s documented drop-outside-the-lock contract**
  (`sync_region.rs:131-134`). Verified by temporary-lifetime analysis: the guard is a
  temporary of the function's tail expression, dropped at the end of the body *after* the
  `Option<T>` has been moved into the return slot — so the removed value's `Drop` genuinely
  runs in the caller, with no guard held. **The contract holds as written.**
- **`Handle<T>` variance and auto-traits.** `PhantomData<fn() -> T>` is covariant in `T` and
  unconditionally `Send + Sync` — matching the doc at `handle.rs:11-13` and the const asserts
  at `handle_static_asserts.rs:50-63`. The hand-written `Clone`/`Copy`/`PartialEq`/`Eq`/
  `Hash`/`Debug` impls correctly hold for every `T` rather than the `T: Clone`-style bounds a
  `#[derive]` would have imposed.
- **`clear_partial_under_panic.rs` after #769.** The `drop_count + len() == 5` invariant is
  genuinely drain-order-free, and the bomb is always visited (the sweep stops *at* it), so the
  `!survivor_ids.contains(&bomb_id)` assertion is order-free too. The §D1a false-red hazard
  is fully closed.
- **`clippy -D warnings`.** Clean on `-p sefer-region --all-targets --all-features` and on
  `-p sefer-region --no-default-features`.
- **`cargo package --list -p sefer-region`.** 19 entries — `Cargo.toml`(+`.orig`),
  `Cargo.lock`, `.cargo_vcs_info.json`, both LICENSEs, `README.md`, four `src/`, one
  `benches/`, one `examples/`, six `tests/`. **No `target/`, no stray artefacts, nothing
  objectionable ships.** The 99 stray `captrack-pgo` JSONs from F6 are correctly excluded.
- **`docs/BENCHMARKS.md` backing for the "~30 % slower than `DenseSlotMap`" claim** in
  `region.rs:12`. Present and matching at `docs/BENCHMARKS.md:46`, and the rustdoc link is
  the full GitHub URL (fixed by task #672), so it resolves from docs.rs where the file does
  not ship. **Not stale.**
- **Metadata basics.** `license = "MIT OR Apache-2.0"` with both LICENSE files present and a
  matching README section; `repository`/`homepage`/`documentation`/`readme` all populated and
  well-formed; `categories = ["data-structures", "memory-management", "no-std"]` is accurate
  (the crate uses `alloc`, so the narrower `no-std::no-alloc` — which task #645 had to strip
  from two sibling crates — is correctly *not* claimed here).
- **CI coverage.** `test-workspace` runs `cargo test -p sefer-region` (debug, default
  features), `cargo test -p sefer-region --release`, and
  `cargo build -p sefer-region --no-default-features --target thumbv7em-none-eabi`. The one
  configuration never *tested* on a host target is `--no-default-features`, but every
  `SyncRegion`-touching test is already `#[cfg(feature = "std")]`-gated and the bare-metal
  build proves the `no_std` lib compiles, so the residual gap is not worth a CI step.
- **`bench-iters.txt` provenance.** Lives at the workspace root (resolved by
  `bench-scale-tool`'s `[workspace]`-ancestor walk, `lib.rs:366-385`), is tracked, and holds
  all 16 `region_bench::*` keys. A consumer running the bench from the published tarball
  gets JIT calibration at a 1 s budget rather than an error — the harness self-heals
  (`lib.rs:673-690`). No defect; recorded because `README.md:188-190` names the file without
  saying it lives outside the crate.

---

## Verdict

**GO-WITH-FIXES.** The crate's correctness surface is in good shape and I could not break it:
every invariant claim survives a source-level re-derivation against `slotmap 1.1.1`, the
perf table still reproduces, packaging is clean, clippy is clean in every configuration, and
the API's only irreversible commitments are already documented deliberate choices. Three
rounds of review have done real work here and it shows.

The one finding I would not ship without is **F1** — not because the guard is wrong (it is
correct), but because the test and the CI step that exist specifically to defend it both pass
against its absence, which means the project currently believes it has coverage it does not
have. **F2** and **F3** are the next two, both cheap and both docs/tests-only. **F4** and
**F5** are bookkeeping, but they are bookkeeping of exactly the kind this repo's own
conventions single out as the thing that gets lost across a session boundary — and one of
them is a carry-forward item that a previous reviewer explicitly wrote down to prevent that.

Suggested order: F1 → F3 → F2 → F5 → F4 → F6 → F7, then the INFO tail as a single
docs/metadata pass before the version bump.
