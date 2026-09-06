# `globalalloc-model` — independent read-only pre-publish review (run 1)

**Verdict: GO-WITH-FIXES.** The crate is coherent, genuinely useful, and its
`unsafe` is correctly scoped and commented for every path its own two
front-ends can generate — but `drive()` is a *safe* `pub fn` that will call
`GlobalAlloc::alloc`/`realloc` with a zero size if a caller hands it a
hand-built `Op` (documented UB, reachable from 100% safe downstream code,
P0), and the crate ships **zero negative tests** — not one test proves that
any M1–M4 oracle actually *fires*, so every existing test would still pass
with every `assert!` in `drive` deleted (P1). Both fixes are small; neither
requires a design change.

- **Scope:** `crates/globalalloc-model/` only (`src/lib.rs`, `src/strategy.rs`,
  `src/arbitrary_stream.rs`, `Cargo.toml`, `README.md`, `CHANGELOG.md`, all
  four files under `tests/`), plus the `ci.yml` / `release.yml` rows that wire
  it. Consumers (`tests/alloc_core_differential.rs`, `tests/heap_differential.rs`,
  `fuzz/fuzz_targets/global_alloc_ops.rs`) were read for context only.
- **Method:** static read only. No `cargo` command was run; nothing here is a
  measured result, every claim below is traceable to a cited line.
- **Axes:** bug · smell · improvement. **Severity:** P0 (blocks publication /
  real soundness bug) → P4 (trivial nit).
- **Counts:** **P0: 1 · P1: 1 · P2: 8 · P3: 19 · P4: 18** (47 total).

### What is genuinely good (not a finding — context for the severities below)

Two design decisions are worth naming so they are not lost in a list of
defects. First, **every block is dirtied before it is freed** — the `Alloc`
arm fills with a non-zero `fill` byte, the `AllocZeroed` arm re-fills *after*
its zero check (`src/lib.rs:316-324`), and the `Realloc` arm re-fills the
whole new extent (`:377-384`). That makes the `alloc_zeroed` oracle a real
oracle rather than a tautology: a recycled block handed back by
`alloc_zeroed` is guaranteed to have held non-zero bytes, so a missing
re-zero is caught. Second, freeing every survivor and only then returning
(`:409-415`) means a green run is itself evidence about the teardown walk,
as the CHANGELOG claims. Both are correct as written.

---

## P0 — blocks publication

### P0-1 · bug · `src/lib.rs:271-275`, `:352-360`, `:130-133`

**A safe `pub fn` can invoke documented undefined behaviour.** `drive` builds
a `Layout` from an `Op`'s `size`/`align` and calls into the allocator without
validating the preconditions `GlobalAlloc` attaches to those calls:

- `Op::Alloc { size: 0, .. }` / `Op::AllocZeroed { size: 0, .. }` —
  `Layout::from_size_align(0, align)` is **`Ok`** (zero-sized layouts are
  legal `Layout`s), so `.expect("valid layout")` at `:272` / `:304` passes and
  `:275` / `:307` calls `alloc.alloc(layout)`. `GlobalAlloc::alloc`'s safety
  contract requires `layout` to have **non-zero size**.
- `Op::Realloc { new_size: 0, .. }` — `:360` calls `alloc.realloc(ptr,
  old_layout, 0)`. `GlobalAlloc::realloc`'s safety contract requires
  `new_size` to be **greater than zero**.
- `Op::Realloc { new_size: usize::MAX, .. }` — `:360` calls `realloc` with a
  `new_size` that, rounded up to `old_layout.align()`, overflows `isize`;
  that is also forbidden by `GlobalAlloc::realloc`. (The `Alloc` arms are
  incidentally protected here, because `Layout::from_size_align` *does* reject
  the overflow case and the `.expect` panics — only `realloc` slips through,
  because `drive` never constructs a `Layout` for `new_size`.)

`Op` is a `pub enum` with public fields, `drive` is a safe `pub fn`, and the
blanket impl means `System` is an in-scope allocator. So
`drive(&System, Config::default(), &[Op::Alloc { size: 0, align: 1 }])` is UB
reachable with no `unsafe` token anywhere on the caller's side. The
`assert!(!ptr.is_null(), "M1: alloc returned null")` at `:276` also turns this
into a spurious oracle failure on any platform where `malloc(0)` returns null.

The root inaccuracy is the blanket impl's SAFETY comment at `:130-133`:
"`GlobalAlloc` has exactly this contract by definition". It does not —
`GlobalAlloc` has a **strictly stronger** contract (non-zero size, non-zero
`new_size`, no `isize` overflow) than the `RawAllocator` trait doc at
`:85-103` states, and `drive` relies on the weaker one.

Neither front-end can generate a violating op (`strategy.rs:15-17` and
`arbitrary_stream.rs:26-28` both floor sizes at 1), which is why this has
never fired — but `drive` is public, takes an arbitrary `&[Op]`, and the
crate's own two hand-built test files construct `Op` values literally.

*Fix:* pick one and do it in `drive`, not in the generators — (a) skip or
clamp: `let size = size.max(1);` and `let new_size = new_size.max(1);` before
the allocator call, or (b) add the three missing clauses to
`RawAllocator::realloc`/`alloc`'s `# Safety` sections **and** make `drive`'s
`# Panics` section state that a zero size is rejected, then `assert!(size >
0)`. (a) keeps `drive` total over all `Op` values, which is the better shape
for a fuzz-facing API. Either way, correct the blanket impl's SAFETY comment
at `:130-133` to say `GlobalAlloc`'s contract is stronger and name what
`drive` guarantees.

---

## P1 — must-fix before anything else

### P1-2 · improvement · `crates/globalalloc-model/tests/` (all four files)

**The crate's own test suite never verifies that any oracle detects
anything.** All four test files drive a *correct* allocator (`System`, or a
leak-everything wrapper around `System`) and assert only that `drive` returns.
Delete every `assert!`/`assert_eq!` inside `drive` and all four test files
still pass. For a crate whose entire product is detection, that is the
definition of a vacuous suite, and it is exactly the counterfactual test
CLAUDE.md's zero-trust rule demands ("would they fail without the fix").

It is not hypothetical that this matters: **P2-3 below is a real
missing-detection hole that a negative suite would have caught on the first
run.**

*Fix:* one new test file with a deliberately-broken `RawAllocator` per oracle
and `#[should_panic(expected = "...")]` per case. Each is a few lines given
the `LeakyAllocator` pattern already in `tests/double_free_no_op.rs`:
- returns a pointer overlapping a live block → expect `"M3: new alloc
  overlaps a live block"`;
- returns `ptr.add(1)` for an align-8 request → expect `"M1/M4: pointer not
  aligned"`;
- `alloc_zeroed` forwards to `alloc` without zeroing (after a free of a
  dirtied block) → expect `"alloc_zeroed: byte not zeroed"`;
- `realloc` = fresh `alloc` + no copy → expect `"realloc lost a prefix
  byte"`;
- `alloc` returns a block one byte short and a second alloc lands inside it
  → expect the M3 or the read-back message;
- `alloc` returns null → expect `"M1: alloc returned null"`.

This is the single highest-value change in this review: it pins the oracle
messages as behaviour, and it makes P2-3 impossible to reintroduce.

---

## P2 — should-fix

### P2-3 · bug · `src/lib.rs:303-331`, `:352-391`, `:13-15`, `README.md:13-14`

**The M3 incremental overlap check runs only on `Op::Alloc`.** The
`for other in &live { assert!(!ranges_overlap(...)) }` loop exists at
`:278-283` and appears **nowhere else**:

- the `AllocZeroed` arm (`:303-331`) pushes its new block into `live`
  (`:325-330`) having checked null, alignment and zeroing — but never
  overlap;
- the `Realloc` arm writes `live[i] = Live { ptr: new_ptr, ... }`
  (`:385-390`) with no overlap check on `new_ptr` at all.

Both the crate doc (`:13-15`: "checked against every live block") and the
README (`:13-14`) state the check as unconditional. It is not.

This is more than a weaker oracle. `drive`'s teardown walk (`:409-414`)
frees every entry of `live` exactly once, which is only sound if `live` holds
no duplicate pointers. For `Op::Alloc` that is *guaranteed* by the overlap
check (a returned pointer equal to a live one overlaps, so it panics first).
For `AllocZeroed` and `Realloc` it is not: a broken allocator returning an
already-live pointer gets it pushed into `live` twice, and `drive` then
double-frees it in its own teardown — the harness commits UB instead of
reporting a clean panic, which is precisely the failure mode it exists to
prevent.

*Fix:* extract `fn assert_no_overlap(live: &[Live], skip: Option<usize>, ptr:
*mut u8, size: usize)` and call it from all three insertion sites. The
`Realloc` call passes `skip: Some(i)` — the old block at index `i` is consumed
by a non-null `realloc` return, so it must be excluded from the comparison.
(See P3-16: doing the helper extraction is also what stops this class of
omission recurring.)

### P2-4 · bug · `README.md:38-45`, `src/lib.rs:64-72`, `Cargo.toml:34`

**The headline `no_std` claim is false for the `proptest` front-end as a
reader will take it.** `README.md:40-42` says "The core model **and the
`proptest` front-end** need only `core` + `alloc`, so this crate can
differential-test a `no_std` allocator's own test suite without pulling in
`std`", under a section titled "`no_std` by default", and then cites a
bare-metal verification that covers **only the default build**
(`ci.yml:2047`, `cargo build -p globalalloc-model --target
thumbv7em-none-eabi`, no features).

The crate's own `strategy.rs` source does only use `core` + `alloc` — that
part is true. But `Cargo.toml:34` declares `proptest = { version = "1",
optional = true }` **with default features**, and proptest's defaults
(`std`, `fork`, `timeout`, `rusty-fork`) require `std`. So
`cargo build -p globalalloc-model --features proptest --target
thumbv7em-none-eabi` cannot succeed, and no CI row would catch the
discrepancy because none builds that combination for a bare-metal target.

The section also makes an asymmetric disclosure: it carefully names
`arbitrary` as "the one exception" that pulls `std` back in, which invites the
reader to conclude `proptest` does not.

*Fix:* two options, and they are not equivalent.
- **Cheap and honest:** reword to scope the guarantee to the default build —
  "the core model needs only `core` + `alloc` (verified on
  `thumbv7em-none-eabi`); both optional front-ends pull in `std` through
  their dependencies."
- **Actually deliver it:** `proptest = { version = "1", optional = true,
  default-features = false, features = ["alloc"] }` — proptest does support
  `no_std`+`alloc`, and `strategy.rs` uses only `prop_oneof!`,
  `proptest::sample::select`, `prop::collection::vec` and `any::<usize>()`,
  all available in that mode. Then add a `--features proptest --target
  thumbv7em-none-eabi` CI row to prove it. Note the feature-unification
  consequence is benign here: the root crate has its own full-featured
  `proptest` dev-dependency, so its builds are unaffected.

### P2-5 · smell · `src/lib.rs:169-187`

**`pub struct Live` and `unsafe impl Send for Live` are public API with zero
public use, and the `Send` justification does not describe any real code.**
`Live` appears only inside `drive`'s body (`:266`, `:296`, `:325`, `:385`,
`:398`, `:409`) — verified: no consumer in `src/`, `tests/`, `benches/`,
`examples/`, or `fuzz/` names the type. `drive` neither takes nor returns it,
and no bound in the crate requires `Send`.

The comment at `:183-187` gives two reasons. The second — "mirroring the
`unsafe impl Send for Live` in the original in-tree copies" — is factually
correct (`git show b420d39^:tests/heap_differential.rs:26` has it), but there
`Live` was a *private, test-local* struct, where blessing `Send` costs
nothing. The first — "`Send` lets a proptest harness hold a `Vec<Live>`
across its (single-threaded) closure boundary" — describes nothing that
exists: the `Vec<Live>` is a local inside `drive`, no proptest closure holds
one, and proptest imposes no `Send` bound on test-body locals.

Published as-is, this makes a permanent public promise that a struct carrying
an *owned allocation handle* is safe to move across threads — the exact
operation that is unsound for a thread-local or per-thread-cached allocator,
which is a large share of this crate's target audience. Removing it after
0.1.0 is a semver break; removing it now is free.

*Fix:* make `Live` private (`struct Live`) and delete the `unsafe impl Send`
along with it. If it is deliberately public as a documented extension point,
say so in its doc comment and replace the `Send` rationale with a real one —
but there is currently nothing to extend.

### P2-6 · bug/improvement · `src/arbitrary_stream.rs:16-33`, `src/lib.rs:189-194`

**`OpStream` ignores `Config` entirely, and its hardcoded distribution is the
wrong shape for fuzzing.** `Config`'s own doc (`lib.rs:189`) reads
"Size-distribution knobs for the op-stream **generators**" (plural) and
"the shape every in-tree copy used", and the crate doc (`:21-25`) frames the
two front-ends as peers over one model. But the `arbitrary` front-end never
sees a `Config`: `MAX_OPS`, `SIZE_MOD` and `ALIGN_POW_MOD`
(`arbitrary_stream.rs:16-23`) are compile-time constants. Only
`small_weight`'s doc ("in the proptest size strategy") admits the asymmetry.

The consequence is not just an API wart. `bound_size` (`:26-28`) yields a
**uniform** distribution over `1..=2 MiB`, so the mean generated allocation is
~1 MiB — while `drive` writes and reads back **every byte individually**
(`lib.rs:288-295`). At `MAX_OPS = 2048` a single fuzz input can therefore ask
for gigabytes of byte-at-a-time traffic, so nearly the whole fuzzing budget
goes into memset-by-loop rather than into allocator state space. The proptest
front-end gets this right with its 9:1 small:large weighting
(`lib.rs:220-235`); the fuzz front-end, which needs it far more, does not.
The module doc at `:5-7` claims the bounds exist "so a single input cannot ask
the OS for gigabytes (which would OOM the fuzzer, not find a bug)" — the
bound stops the OOM but leaves the throughput problem in place.

*Fix:* two independent pieces. (a) Give `OpStream` a config-aware
constructor, e.g. `OpStream::arbitrary_with_config(u: &mut Unstructured,
config: Config) -> arbitrary::Result<Self>`, keeping the `Arbitrary` impl as
`arbitrary_with_config(u, Config::default())` — the `Arbitrary` trait
signature is fixed, so a plain inherent constructor is the only route, and
`fuzz_target!(|stream: OpStream|)` keeps working unchanged. (b) Make the
default size distribution weighted rather than uniform (e.g. use two low bits
of the raw `u32` to select a small/large arm), and state in `Config`'s doc
which fields each front-end actually reads. See also P3-21 (the byte-at-a-time
loops), which is the other half of this cost.

### P2-7 · improvement · `crates/globalalloc-model/tests/double_free_no_op.rs:47-72`

**The M2 test is vacuous with respect to its stated purpose.** The file's own
header (`:1-6`) says it exists because "`drive()`'s `if config.double_free {
... }` branch was therefore dead from this crate's own test suite's
perspective before this file". But `LeakyAllocator::dealloc` (`:37-40`) does
nothing at all, and the test asserts nothing about how many times it was
called — so the test passes identically whether `drive` issues the second
`dealloc` at `lib.rs:348` or not. Deleting the `if config.double_free` block
from `drive` leaves this test green.

It does achieve line coverage of the branch, and the file is honest that
`LeakyAllocator` "trivially satisfies the M2 contract by construction" — but
it does not pin the behaviour it was written to pin.

*Fix:* give `LeakyAllocator` a `Cell<usize>` (or `AtomicUsize`) dealloc
counter and, better, a `RefCell<Vec<*mut u8>>` of freed pointers; then assert
after `drive` that the count is exactly the expected number **and** that the
first `Dealloc` produced two consecutive entries for the same pointer. That
turns a coverage smoke test into a counterfactual one.

### P2-8 · improvement · `.github/workflows/ci.yml:2147-2245` (the `msrv` job)

**`rust-version = "1.88"` is unverified for this crate's own tests and for its
`arbitrary` feature.** The `msrv` job pins `dtolnay/rust-toolchain@1.88` and
carries explicit per-crate rows for `sefer-region` (`:2181-2184`),
`aligned-vmem` (`:2202-2203`), `numa-shim` (`:2210-2216`), `once-ptr-cell`
(`:2229-2230`), `size-classes` (`:2241-2242`) and `tagged-index-stack`
(`:2258-2260`) — each added with a comment explaining that the job's
workspace-root `--all-features` rows reach member crates "only as a
path-dependency LIBRARY … never as its own tested workspace member".
**There is no `-p globalalloc-model` row.**

The gap is exactly the one those comments describe. The root's
`cargo test --no-run --all-features` (`:2175`) does compile this crate's
*library* with `proptest` on 1.88 (root `Cargo.toml:977` is a dev-dependency
with `features = ["proptest"]`), but it never compiles the crate's own four
`tests/` files, and it never compiles the `arbitrary` feature at all —
`arbitrary_stream.rs` and `derive_arbitrary` reach 1.88 through no job (the
`fuzz-build` job at `:3474` is nightly, and `fuzz/Cargo.toml` is its own
workspace).

*Fix:* two rows, matching the shape and placement of the five precedents in
the same job:
```yaml
- run: cargo check -p globalalloc-model --all-features
- run: cargo test -p globalalloc-model --no-run --all-features
```
These also answer the open question the declared MSRV assumes but nothing
checks: whether `arbitrary` 1.x + `derive_arbitrary` still build on 1.88.

### P2-9 · bug · `crates/globalalloc-model/tests/miri_bounded.rs:1-5`

**The file claims miri coverage that no CI job provides.** Its header reads
"the bounded-miri coverage for the shared `drive` loop + M1–M4 oracle code
**under strict provenance**", and the crate's doc (`lib.rs:22-23`) and
CHANGELOG (`:32-33`) both advertise "a bounded miri run". No job in
`.github/workflows/ci.yml` runs `cargo miri test -p globalalloc-model` — the
five miri jobs (`aligned-vmem-miri:640`, `once-ptr-cell-miri:861`,
`miri-core:2314`, `miri-alloc-core:2376`, `miri-fastbin:2404`) name other
packages only. This file runs exclusively as an ordinary native `cargo test`
via `ci.yml:2053-2054`, where it proves nothing about UB.

That matters more here than for a typical test: `drive`'s pointer arithmetic
and the M1/M3 read-back loops are the crate's entire `unsafe` surface, and
native execution cannot detect an out-of-bounds `ptr.add(b)`.

*Fix:* add a job modelled on `once-ptr-cell-miri` (`:861-882`, the workflow's
existing small-crate per-PR miri pattern):
```yaml
- run: cargo miri test -p globalalloc-model --all-features
```
Two caveats to settle before landing it, both readable from the tree:
1. `tests/double_free_no_op.rs` **leaks by design** (`LeakyAllocator::dealloc`
   is a no-op), so miri's default leak checker will fail the run. Either add
   `MIRIFLAGS: -Zmiri-ignore-leaks` — the same accommodation
   `aligned-vmem-miri` already makes for its own reason (`:656-662`) — or
   `#![cfg(not(miri))]` that one file.
2. If the row uses `-Zmiri-strict-provenance` as every other miri job in this
   workflow does, check the `ptr as usize` casts at `lib.rs:277`, `:280`,
   `:309`, `:365` first (see P4-40).

### P2-10 · improvement · `src/lib.rs:276-277`, `:293`, `:308-309`, `:313`, `:365`, `:371`, `:402`

**Every oracle-failure message is context-free.** For a crate whose deliverable
*is* the failure report, these are the strings a user gets when a real bug is
found:

- `"M1: alloc returned null"` — no size, no align.
- `"M1/M4: pointer not aligned"` — no pointer value, no requested align.
- `"M1: byte did not read back"` — no byte offset, no block size, no pointer.
- `"M3: new alloc overlaps a live block"` — neither range printed.
- `"alloc_zeroed: byte not zeroed"` — no offset.
- `"realloc lost a prefix byte"` — no offset, no old/new size.
- `"M3: live block clobbered"` — no offset, and no indication of *which* block.

None names the op's index in the stream either, so reproducing a libFuzzer
artifact means re-instrumenting the crate by hand. `assert!`/`assert_eq!`
format arguments are evaluated **only on failure**, so adding them costs
nothing in the hot loops.

*Fix:* thread an op index (`for (op_idx, op) in ops.iter().enumerate()`) and
put it plus the operands into each message, e.g.
`assert_eq!(ptr.add(b).read(), fill, "M1: op #{op_idx} alloc({size},{align})
-> {ptr:p}: byte {b} read {:#04x}, expected {fill:#04x}", ptr.add(b).read())`.
Deriving `Debug` on `Live` (see P3-14) lets the M3 messages print the offending
block directly.

---

## P3 — nice-to-fix

### P3-11 · bug · `src/lib.rs:253-258` vs `:361-364`

`drive`'s `# Panics` section states the harness panics on "a null return (M1
validity — the harness does not model allocator OOM, so **ANY** null is a
failure)". That is contradicted eleven lines below by the code it documents:
`if new_ptr.is_null() { continue; }` at `:361-364` deliberately *tolerates* a
null `realloc` return and leaves the old block live — correct behaviour, and
correctly matched by `RawAllocator::realloc`'s own contract at `:100-103`, but
the reverse of what the `# Panics` section promises. *Fix:* qualify it — "any
null from `alloc`/`alloc_zeroed`; a null from `realloc` is the documented
failure signal and is skipped, leaving the old block live".

### P3-12 · bug · `Cargo.toml:7`

The crates.io description says the front-ends sit "behind optional **dev-only**
features". They are not dev-only: `proptest` and `arbitrary` are ordinary
optional `[dependencies]` (`:33-35`), so a downstream crate that enables
either gets a real, non-dev dependency in its build graph. (The neighbouring
claim in the same sentence, "a normal build has zero non-dev deps", is
accurate.) *Fix:* drop the word "dev-only", or say "optional features (off by
default)".

### P3-13 · bug · `src/lib.rs:22`, `:24`

Two crate-level intra-doc links do not resolve in the **default** feature
configuration: `[`Strategy`](op_strategy)` (`:22`) and
`[`impl Arbitrary for OpStream`](OpStream)` (`:24`) both target items behind
`#[cfg(feature = ...)]` (`:418-426`). `cargo doc -p globalalloc-model` with no
features emits two `broken_intra_doc_links` warnings — and would fail under
the `-D warnings` this repo uses everywhere. docs.rs itself is safe
(`[package.metadata.docs.rs] features = ["proptest", "arbitrary"]`,
`Cargo.toml:19-20`, equals `--all-features`), and CI's only doc row is
`--all-features` (`ci.yml:2059`), so nothing currently catches it. That is the
same gate-blindness shape CLAUDE.md's feature-set doc-lint rule describes,
inverted: the *unbuilt* configuration here is the crate's advertised default,
not its published one. *Fix:* add `- run: RUSTDOCFLAGS="-D warnings" cargo doc
-p globalalloc-model --no-deps` (default features) next to the existing row,
and resolve the two links — either wrap the bullets in
`#[cfg_attr(doc, ...)]`-free prose (name the items as inline code, the
resolution the root crate already uses for the same problem — root
`Cargo.toml:31-35` comment), or gate the two lines with
`#![cfg_attr(feature = "proptest", doc = "...")]`.

### P3-14 · smell · `src/lib.rs:16-17`, `:277`, `:309`, `:365`

**M4 is not a distinct oracle.** The docs list six checks and label the fourth
"M4 (alignment & size fidelity): the returned pointer always satisfies the
requested size and align". In the code there is no size-fidelity assertion
separate from M1's fill/read-back — the two labels name the same
`(ptr as usize) % align == 0` check plus the same byte loop. The labelling is
also inconsistent across the three sites that use it: `"M1/M4: pointer not
aligned"` (`:277`), `"M1/M4: not aligned"` (`:309`), `"M1: realloc not
aligned"` (`:365` — no M4 at all, for the same class of check). *Fix:* pick
one string form and use it in all three places, and either state in the docs
that size fidelity is established *indirectly* (by the read-back plus M3's
overlap check over the requested extents — which does genuinely catch an
undersized block once a neighbour lands inside the requested span), or drop
the separate M4 bullet.

### P3-15 · smell · `README.md:11-12`

The README's oracle list presents M2 without qualification — "a second
`dealloc` of the same pointer is a no-op that must not corrupt the allocator"
— alongside M1/M3/M4, as if it were an always-on check. It is opt-in and
**off by default** (`Config::double_free`, `lib.rs:207-217`, `:232`), because
it is a *stronger-than-`GlobalAlloc`* guarantee that a real system `malloc`
does not provide. `lib.rs:10-12` has the same omission; only `Config`'s own
doc and `CHANGELOG.md:18-20` state it correctly. Since the README is the
crates.io landing page, a reader's first impression is that the crate
double-frees against their allocator by default. *Fix:* append "(opt-in via
`Config::double_free`; off by default)" to the README and crate-doc bullets.

### P3-16 · smell · `src/lib.rs:259-416`

`drive` is a ~157-line function with four inline arms and substantial
duplication: the fill-counter bump `let fill = next_fill; next_fill =
next_fill.wrapping_add(1).max(1);` appears three times verbatim (`:284-285`,
`:316-317`, `:377-378`); the fill-write loop three times (`:289-291`,
`:321-323`, `:381-383`); `Layout::from_size_align(...).expect("valid layout")`
four times (`:272`, `:304`, `:336`, `:357`, `:410`); the read-back verify loop
twice in different shapes (`:292-294`, `:401-403`). *Fix:* extract
`fn next_fill(counter: &mut u8) -> u8`, `unsafe fn fill_block(ptr, size, byte)`,
`unsafe fn verify_block(ptr, size, byte, ctx)` and the
`assert_no_overlap` helper from P2-3. This is worth doing not for tidiness but
because it is the mechanism that prevents P2-3 from recurring: with an
`assert_no_overlap` helper, omitting it from one of three insertion sites is a
visible asymmetry rather than an invisible one.

### P3-17 · smell · `src/lib.rs` (whole file)

The crate has three source files, and `src/lib.rs` defines **six** public items
(`RawAllocator`, `Op`, `Live`, `Config`, `ranges_overlap`, `drive`) plus the
entire `drive` implementation. CLAUDE.md's "One file — one export" rule
sanctions a multi-item file for "single-file seam crates in `crates/`" — a
crate that *is* one file. This crate is not, so the exception does not apply.
The in-repo precedent for a multi-file member crate is unambiguous:
`crates/once-ptr-cell/src/lib.rs` and `crates/tagged-index-stack/src/lib.rs`
each define **zero** public items and act as pure module wiring over an
`imp.rs`; only the genuinely single-file crates (`size-classes`, `proc-probe`)
have six. *Fix:* `src/raw_allocator.rs`, `src/op.rs`, `src/config.rs`,
`src/drive.rs` (holding `Live` and `ranges_overlap` as its private helpers,
which P2-5 makes possible), with `lib.rs` reduced to the crate doc plus
`mod`/`pub use`.

### P3-18 · improvement · `src/lib.rs:57-74`

No lint floor is declared. The code already satisfies both of the two that
matter here — every public item is documented, and every `unsafe fn` body uses
an explicit inner `unsafe {}` block (`:134-149`, `:109-127` call sites) rather
than relying on edition-2021's implicit `unsafe fn` body. Declaring them
locks the discipline in for a crate that will be read as an `unsafe` exemplar:
```rust
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
```
(`unsafe_op_in_unsafe_fn` is allow-by-default in edition 2021, so nothing
enforces the existing good practice today.)

### P3-19 · improvement · `Cargo.toml:19-20`

docs.rs will render `op_strategy` and `OpStream` with no indication that they
require a feature, because the crate does not opt into `doc(cfg)`. *Fix:* add
`rustdoc-args = ["--cfg", "docsrs"]` to `[package.metadata.docs.rs]`,
`#![cfg_attr(docsrs, feature(doc_cfg))]` to `lib.rs`, and
`#[cfg_attr(docsrs, doc(cfg(feature = "proptest")))]` /
`(feature = "arbitrary")` on the two re-exports at `:421` and `:426`. This
also pairs with P3-13 — the badge is the reader-facing half of the same
"which items need which feature" question.

### P3-20 · improvement · `src/lib.rs:267`, `:284-285`, `:316-317`, `:377-378`, `:396-405`

**The run-end M3 sweep is blind to cross-clobbers between same-fill blocks.**
`fill` is a `u8` cycling `1..=255` (`:267` + `wrapping_add(1).max(1)`), so
after 255 allocations two simultaneously-live blocks are guaranteed to share a
fill byte, and a clobber between exactly those two is undetectable by the
`:398-405` sweep. Fuzz streams reach 2048 ops (`arbitrary_stream.rs:16`), so
this is the common case there, not an edge. The oracle is still sound (no
false positives) — just weaker than the doc's "a per-block fill that would
detect cross-contamination" (`:14-15`) implies. *Fix:* either document the
limit, or strengthen cheaply: keep the `u8` body fill but additionally write a
`u64` block id at offset 0 when `size >= 8` and verify it in the sweep; that
makes the identity check exact at ~zero cost.

### P3-21 · improvement · `src/lib.rs:288-295`, `:311-315`, `:320-324`, `:369-373`, `:380-384`, `:400-404`

Every fill and verify is a byte-at-a-time loop. With the fuzz front-end's
~1 MiB mean allocation (P2-6), an `Op::Alloc` costs 2× size individual
`write`/`read` calls and a `Realloc` costs another `new_size`. `write_bytes`
plus a slice comparison touches exactly the same bytes with identical oracle
strength and identical miri/ASan visibility, at a small fraction of the cost:
```rust
core::ptr::write_bytes(ptr, fill, size);
let s = core::slice::from_raw_parts(ptr, size);
if let Some(b) = s.iter().position(|&x| x != fill) { panic!(/* ctx per P2-10 */) }
```
The `position` form also hands P2-10 the byte offset for free.

### P3-22 · improvement · `src/lib.rs:244-266`

**Undocumented reentrancy footgun.** `drive` allocates its own `Vec<Live>`
bookkeeping (`:266`) through the *global* allocator. If the allocator under
test is also the installed `#[global_allocator]` — the most obvious thing a
reader of "Differential-test **any** Rust allocator" will try — the model's own
allocations interleave with the ops under test, and a reentrant allocator will
deadlock or recurse. The in-repo fuzz target already knows this and documents
why it drives `AllocCore` rather than the installed `SeferAlloc`
(`fuzz/fuzz_targets/global_alloc_ops.rs:14-24`), but that reasoning lives in a
consumer, not in the crate. *Fix:* one paragraph in `drive`'s doc and in the
README's "allocator seam" section — drive the *engine* behind your
`GlobalAlloc`, not the installed global allocator itself.

### P3-23 · improvement · `Cargo.toml:5`

`rust-version = "1.88"` is inherited from the root crate, but nothing in this
crate needs it. The default build uses `Layout::from_size_align`, `ptr::add`,
`wrapping_add`, `Ord::max`, `#[must_use]` and `cfg_attr(..., no_std)` — the
real floor is edition 2021's 1.56. For a *testing utility*, whose whole value
is being droppable into someone else's project, gratuitously demanding 1.88
excludes users for nothing. *Fix:* lower to the true floor (bounded by
proptest's and arbitrary's own MSRVs when those features are enabled — ~1.65
is a defensible declared value), or, if the workspace prefers a uniform floor,
say so in a comment so the number is a decision rather than a copy.

### P3-24 · improvement · `src/arbitrary_stream.rs:38-43`

`RawOp::Dealloc(usize)` and `RawOp::Realloc { i: usize, .. }` consume a full 8
bytes of fuzzer entropy each (on 64-bit) for an index that `drive` immediately
reduces `% live.len()` (`lib.rs:334`, `:354`). At ~9–17 bytes per op, a typical
4 KiB libFuzzer input decodes only a few hundred of the `MAX_OPS = 2048`
allowed. *Fix:* make the raw fields `u16`/`u32` and widen in `bound()`; the
public `Op::Dealloc(usize)` shape need not change, and the modulo makes the
narrowing lossless in effect.

### P3-25 · improvement · `src/lib.rs:156-167`, `:195-218`, `arbitrary_stream.rs:67-71`

`Op`, `Config` and `OpStream` are all exhaustive public types with public
fields. Post-publication, adding a `Config` knob breaks every downstream
exhaustive struct literal (the in-repo `config()` helpers in
`tests/alloc_core_differential.rs:58-66` and `tests/heap_differential.rs:58-66`
are exactly that shape), and adding an `Op` variant breaks every exhaustive
`match`. This is a decision to make deliberately *before* 0.1.0, not to
discover at 0.2.0. Note `#[non_exhaustive]` on `Config` is not free either —
it forbids struct literals downstream entirely, including the ergonomic
`Config { double_free: true, ..Config::default() }` that
`fuzz_targets/global_alloc_ops.rs:82` uses. *Fix:* either `#[non_exhaustive]`
+ builder-style setters (`Config::default().with_double_free(true)`), or state
explicitly in the README/CHANGELOG that field/variant additions are breaking
and will bump the minor version under 0.x.

### P3-26 · improvement · `crates/globalalloc-model/tests/system_arbitrary.rs:17-29`

The test asserts nothing about what it decoded. Its doc claims it "proves the
fuzz front-end decodes into a valid op stream", but if `OpStream::arbitrary`
returned `ops: vec![]` for all 32 seeds, `drive` would do nothing and the test
would pass. Given P3-24 (each `RawOp` consumes 9–17 bytes from a 512-byte
buffer), the streams are short enough that a future change to `RawOp`'s field
widths could silently empty them. *Fix:* accumulate across seeds and assert
non-vacuity — `assert!(total_ops > 0)` at minimum, better a per-variant
counter asserting all four `Op` variants appear at least once over the 32
seeds.

### P3-27 · process · `CHANGELOG.md:7`

`## 0.1.0 - Unreleased` will **hard-fail** the release workflow. `release.yml`'s
"CHANGELOG must be consolidated before publish" guard resolves this crate's
changelog by `cargo metadata` and greps the version section
case-insensitively for "unreleased", exiting 1 with "Stamp the real release
date". This is the guard working as designed, but it is a publish-blocking
item on the checklist right now. *Fix:* at tag time, `## 0.1.0 - YYYY-MM-DD`.

### P3-28 · improvement · `.github/workflows/release.yml` ("Test gate" step)

The pre-publish test gate runs `cargo test -p "$NAME" --no-fail-fast` with
**default** features. For this crate that is `default = []`, so
`system_proptest.rs` and `system_arbitrary.rs` are `#![cfg]`-ed to empty files
and only 2 of the 4 test files actually execute — the gate cannot see a break
in either front-end. It is partly mitigated by the CI-status guard in the same
workflow (which requires `ci.yml`'s green `--all-features` rows for the same
SHA), so this is redundancy rather than a hole. *Fix:* since the step already
special-cases the root crate's feature bundle, add a matching branch: for
`globalalloc-model`, also run `cargo test -p "$NAME" --all-features
--no-fail-fast`.

### P3-29 · bug · `crates/globalalloc-model/tests/double_free_no_op.rs:65`

The trailing comment is wrong, and the coverage it claims does not exist:
```rust
Op::Dealloc(0), // model empties, drive()'s Dealloc no-ops on an empty `live`
```
Tracing the stream: after `Realloc { i: 0, new_size: 256 }` (`:60-63`) the
model holds two blocks; the first `Dealloc(0)` (`:64`) removes one, leaving
**one**; so this final `Dealloc(0)` finds `live.len() == 1`, takes the
`if !live.is_empty()` branch at `lib.rs:333`, and performs a real (double-)
free — it empties the model rather than no-op-ing on an already-empty one.
The `live.is_empty()` early-out is never reached by this file. *Fix:* correct
the comment, and if the empty-model branch is meant to be covered here, add
one more `Op::Dealloc(0)`.

---

## P4 — optional polish

- **P4-30 · smell · `src/strategy.rs:15`, `:17`** — both `.prop_map(|s|
  s.max(1))` calls are dead code. The small arm's range already starts at `1`,
  and the large arm's starts at `small_max.saturating_add(1) >= 1`; neither
  can yield 0. Delete both.
- **P4-31 · smell · `src/strategy.rs:16`** — one expression mixes
  `config.small_max.saturating_add(1)` (overflow-guarded) with
  `config.small_max + 1` (not) for the *same* quantity. Only the second can
  panic. Use `saturating_add` in both places.
- **P4-32 · bug · `src/strategy.rs:26-32`** — `align_strategy` has two
  undocumented degenerate inputs: `max_align == 0` yields an empty `aligns`
  vec and `proptest::sample::select` panics on it; `max_align >= 1 << 63`
  makes `a <<= 1` reach 0 and the `while a <= config.max_align` loop never
  terminates, pushing 0 forever. Neither is reachable from `Config::default()`.
  *Fix:* `debug_assert!(config.max_align.is_power_of_two())` plus a loop bound
  on the shift, or document the precondition on `Config::max_align`.
- **P4-33 · bug · `src/lib.rs:241`** — `ranges_overlap` is a public
  `#[must_use] pub fn` computing `a + asize` and `b + bsize` with no overflow
  guard and no documented precondition. `ranges_overlap(usize::MAX, 1, 0, 1)`
  panics in debug and silently wraps in release. Not reachable from `drive`
  (real allocations cannot wrap the address space), but it is public API.
  *Fix:* `saturating_add`, or state the precondition in the doc comment.
- **P4-34 · bug · `src/arbitrary_stream.rs:75-76`** — the comment says "stop at
  the first decode error", but `.filter_map(Result::ok)` (`:80`) *skips* errors
  and keeps going. Reword to "skip undecodable items".
- **P4-35 · improvement · `src/lib.rs:272`, `:304`, `:336`, `:357`, `:410`** —
  `.expect("valid layout")` gives no indication of which op, size or align was
  rejected, and `drive`'s `# Panics` section (`:255-258`) does not mention this
  panic class at all. Include the operands in the message and add the case to
  the docs.
- **P4-36 · smell · `src/lib.rs:37`** — the published doc heading reads
  `# Example (text — not a doctest)`. The parenthetical is an internal
  repository convention (CLAUDE.md's no-doctests rule) leaking onto the
  docs.rs page, where it is noise to every reader. Use `# Example`; the
  ```` ```text ```` fence already does the work.
- **P4-37 · smell · `src/lib.rs:39-55`, `README.md:56-72`** — the example is not
  compilable and mixes styles: it `use`s `drive, Config` but then writes
  `globalalloc_model::op_strategy(...)` fully qualified, and it shows
  `proptest!`/`fuzz_target!` without importing either. Make it consistent (a
  reader will copy it).
- **P4-38 · smell · `README.md:22-23`, `CHANGELOG.md:67-69`** — "Nothing else on
  crates.io offers a ready … kit" is an unverifiable competitive claim that
  ages badly and cannot be re-checked at review time. Soften to a description
  of what the crate does.
- **P4-39 · improvement · `Cargo.toml:7`** — the crates.io `description` is a
  ~490-character paragraph. Search results and `cargo search` show the
  beginning only; the useful discriminator ("The correctness twin of
  malloc-bench-rs") is at the very end. Lead with one sentence and move the
  detail to the README.
- **P4-40 · improvement · `README.md:56`** — the usage block uses ```` ```text
  ````, so crates.io and docs.rs render it without syntax highlighting. The
  no-doctests rule targets doc comments in `src/**/*.rs`; `README.md` is not
  compiled as doctests here (no `#![doc = include_str!("../README.md")]`), so
  ```` ```rust ```` is safe today. Note the tradeoff: it stops being safe if
  the README is ever inlined into the crate docs.
- **P4-41 · smell · `src/lib.rs:259`** — `drive(alloc, config: Config, ops)`
  takes the whole `Config` but reads exactly one field, `config.double_free`
  (`:341`). A caller with a hand-built op list must therefore construct five
  irrelevant generator knobs. The doc at `:250-252` acknowledges this ("the
  other `config` fields shape the generators, not `drive`"). Keeping `Config`
  is defensible for API cohesion and forward-compat; it is worth a second look
  if any of P2-6/P3-25 reshapes `Config` anyway.
- **P4-42 · improvement · `src/lib.rs:277`, `:280`, `:309`, `:365`** — `ptr as
  usize` is an exposing pointer-to-integer cast; `ptr.addr()` (stable since
  1.84, below this crate's declared 1.88 floor) is the non-exposing
  equivalent, and is what the workspace's own `aligned-vmem` README documents
  as the preferred form. Worth settling before adding the strict-provenance
  miri row from P2-9. Note the repo already runs `ptr as usize` code under
  `-Zmiri-strict-provenance` elsewhere (e.g. `src/alloc_core/alloc_core.rs:2213`),
  so this is hygiene, not a known blocker — confirm empirically when the row
  is added.
- **P4-43 · improvement · `src/lib.rs:133`** — the blanket
  `unsafe impl<A: GlobalAlloc> RawAllocator for A` means any type implementing
  `GlobalAlloc` can *never* supply its own `RawAllocator` impl (coherence
  conflict) — e.g. to add instrumentation around the calls. That is the right
  tradeoff for ergonomics (it is why `System` works with no adapter), but it is
  invisible until a user hits E0119. One sentence in the trait doc at `:82-83`
  would save that.
- **P4-44 · smell · `src/lib.rs:162`** — `Op::Dealloc`'s doc says "index reduced
  modulo the live count" and omits the other half of the behaviour: when the
  model is empty the op is silently skipped entirely (`:333`). Same for
  `Op::Realloc` at `:165-166` / `:353`. Worth one clause, since a user
  reasoning about op-stream coverage will assume every op does something.
- **P4-45 · improvement · `src/arbitrary_stream.rs:73-85`** — `Arbitrary for
  OpStream` does not implement `size_hint`, so it defaults to `(0, None)`.
  Low impact while `OpStream` is only ever a top-level `fuzz_target!` input,
  but it degrades if anyone nests it in a larger `Arbitrary` structure.
- **P4-46 · smell · `crates/globalalloc-model/tests/system_proptest.rs:14`** —
  `failure_persistence: None` discards any counterexample proptest finds
  instead of writing a regressions file. The in-repo rationale for this
  (hermetic runs, avoiding a `SourceParallel` abort) is documented in
  `tests/heap_differential.rs:69-71` but not here, so in this crate it reads
  as unexplained. Either copy the one-line rationale or drop the override.
- **P4-47 · smell · `docs/correctness-open-items/TRACKED_publish_readiness.md:70-73`**
  — out of the crate but touching its publish readiness: that card states
  `globalalloc-model` (with `proc-memstat`/`proc-probe`) still has "no
  release-workflow entry at all". That is now stale for this crate —
  `.github/workflows/release.yml` carries both the `globalalloc-model-v*` tag
  pattern and the `workflow_dispatch` dropdown option, and the same card's
  later addendum about a missing `CHANGELOG.md` is also closed
  (`crates/globalalloc-model/CHANGELOG.md` exists). Per CLAUDE.md's
  current-state rule for the open-items indexes, that card's Status block
  should be refreshed in whichever commit lands this crate's publish.

---

## Summary table

| Severity | Count | Items |
|---|---|---|
| **P0** | 1 | P0-1 (zero-size / zero-`new_size` → documented UB from safe code) |
| **P1** | 1 | P1-2 (no negative tests — the oracles are never proven to fire) |
| **P2** | 8 | P2-3 … P2-10 |
| **P3** | 19 | P3-11 … P3-29 |
| **P4** | 18 | P4-30 … P4-47 |

**No severity level is empty.** The recommended minimum before tagging
`globalalloc-model-v0.1.0`: P0-1, P1-2, P2-3, P2-4, P2-5, and P3-27 (the
CHANGELOG date, which the release workflow enforces anyway). P2-5 in
particular is cheap now and a semver break later.
