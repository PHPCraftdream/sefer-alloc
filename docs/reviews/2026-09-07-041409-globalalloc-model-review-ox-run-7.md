# globalalloc-model — independent read-only review, run 7

**Verdict: NOT clean — the P0–P3 finding count is 5, not ZERO. 1 P0, 0 P1,
1 P2, 3 P3, 30 P4.** The P0 is a genuine safe-code-reaches-UB hole that has
been present since the crate's first commit and that all six prior rounds
missed: `drive` is a safe `pub fn`, `Config::double_free` is a public `bool`
field, and the blanket `unsafe impl<A: GlobalAlloc> RawAllocator for A` means
`drive(&System, Config { double_free: true, ..Config::default() }, &ops)` —
100% safe code, no `unsafe` token anywhere on the caller's side — issues two
`System::dealloc` calls on the same pointer. This is the *same class* as run
1's P0-1 ("a safe `pub fn` can invoke documented undefined behaviour"), left
standing after that fix closed only the size/overflow half of it.

Reviewed at `6243b54` (`main`), read-only. Scope as briefed: `src/lib.rs`,
`src/raw_allocator.rs`, `src/op.rs`, `src/config.rs`, `src/drive.rs`,
`src/strategy.rs`, `src/arbitrary_stream.rs`, `Cargo.toml`, `README.md`,
`CHANGELOG.md`, all six `tests/*.rs` read in full,
`fuzz/fuzz_targets/global_alloc_ops.rs`, and every `globalalloc-model` section
of `.github/workflows/ci.yml` and `release.yml`. No `cargo`/build/test/lint
command was run (same discipline as runs 1–6; also avoids `target/` lock
contention with concurrent agents). Two claims below were verified by
executing shell/text pipelines against scratch copies in a temp directory
(no repo file touched), and three upstream behaviours were re-derived by
reading the vendored crate sources under
`$CARGO_HOME/registry/src/.../{proptest-1.11.0,arbitrary-1.4.2,derive_arbitrary-1.4.2}`.

Trend at P0–P3: **47 → 14 → 10 → 5 → 2 → 1 → 5.** The uptick is not a
regression in `6243b54` — it is one long-standing P0 plus one long-standing
P2 that six rounds of increasingly narrow, test-and-CI-focused review walked
past, plus three P3s of which one (P3-2) IS newly reachable because
`6243b54` extended exactly the CI check it lives in.

---

## 0. Round-6 fix verification — what I re-derived independently

`6243b54`'s three claims all hold:

- **`arm_of` really is the missing `Op → Arm` compile-time link.**
  `tests/oracle_negative.rs:497-504` matches exhaustively on `Op`; adding a
  fifth `Op` variant is now an E0004 in this file. The doc comments
  (`:20-36`, `:491-496`, `:506-516`) were rewritten to describe what is
  actually enforced, and no longer claim the stronger guarantee run 6 flagged.
- **`grid_axis!` (`:441-467`) genuinely removes the hand-kept `ALL` array.**
  The enum and its `const ALL: &'static [Self]` are generated from ONE variant
  list, so they cannot drift; `null_align_cell`'s match stays exhaustive over
  `Arm × Shape`. (The `'static` on an *associated* const is not a
  `clippy::redundant_static_lifetimes` hazard — that lint's `check_item`
  explicitly skips associated consts, and the item is macro-expanded anyway.)
- **The per-cell stream/arm assertion (`:600-603`) is real, not decorative.**
  All six cells satisfy it; a copy-pasted cell whose stream never issues its
  own arm's op now fails by name.
- **The per-step MIRIFLAGS pinning (round-6 P4-1) works.** I ran the new
  `awk` extractor plus `expect_flags 1..4` against the real `ci.yml`:
  `found=4`, `stepcount=4`, all four shapes match. The script's own lines
  cannot self-match either pattern (they begin with `/`, `|`, `rec`, `sub(`,
  `echo`, or `#` before the token), and `rec != ""` gives a second layer of
  protection since the check step precedes every run line in the job block.
- **Round-6 P4-2 is closed.** `panic_message`'s comment (`:636-643`) now
  states only the checkable fact — `&*err` is `&(dyn Any + Send)` at the
  payload, and both downcast arms cover the two real payload shapes. The
  speculative coercion-trap claim is gone.

Two upstream facts round 6 asserted, re-verified against the vendored source
rather than assumed: proptest 1.11.0 declares `rust-version = "1.85"`
(`Cargo.toml:14`), and its two-arm `prop_oneof!` expands to
`TupleUnion::new` (`src/sugar.rs:344-350`) whose `pick_weighted`
(`src/strategy/unions.rs:104-117`) has no non-zero-weight assertion — with
`[0, 1]` it deterministically selects index 1, with `[9, 0]` index 0. No
front-end divergence on a single zero weight. `proptest::sample::select`
does panic `"Cannot select from empty collection"`
(`src/sample.rs:156-161`), matching `strategy.rs:29-38`'s comment.

I also checked one thing round 5 raised in passing and no round settled:
`op_strategy(config, 0..0)` does NOT underflow or hang — `collection::vec`
calls `SizeRange::assert_nonempty()` (`src/collection.rs:210`) and panics
with proptest's own hint message. Not a finding.

---

## P0

### P0-1 — `drive` is a safe `pub fn` that performs a real double-free against any `GlobalAlloc`, because `Config::double_free` is a public `bool` and the blanket impl enrolls every `GlobalAlloc` without its author's consent

*Axis: ошибки (soundness — safe code reaches documented UB).*

**Files:** `crates/globalalloc-model/src/drive.rs:368-376` (the double-free
site), `:220` (`pub fn drive`, safe); `crates/globalalloc-model/src/config.rs:87`
(`pub double_free: bool`) and `:20-23` (the exhaustive-struct rationale);
`crates/globalalloc-model/src/raw_allocator.rs:39-45` (the conditional caller
obligation), `:102-112` (the blanket impl and its `# Safety` note);
`crates/globalalloc-model/README.md:85-88`.

**The reproducer, with no `unsafe` token on the caller's side:**

```rust
use globalalloc_model::{drive, Config, Op};
use std::alloc::System;

let cfg = Config { double_free: true, ..Config::default() };
let ops = [Op::Alloc { size: 32, align: 8 }, Op::Dealloc(0)];
drive(&System, cfg, &ops);   // System::dealloc twice on the same pointer
```

Every link is safe and public: `Config` is a `pub struct` with `pub` fields
(deliberately — `README.md:85-88` and `config.rs:20-23` name
`Config { double_free: true, ..Config::default() }` as the *ergonomics
justification* for that shape, i.e. the crate's own documentation showcases
the exact UB-triggering expression); `Op` is a `pub enum` with public fields;
`drive` is a safe `pub fn`; and `unsafe impl<A: GlobalAlloc> RawAllocator for A`
(`raw_allocator.rs:112`) makes `System` — and every third-party `GlobalAlloc`
in the ecosystem — an in-scope allocator without its author ever writing an
`unsafe impl` or reading this crate's contract. `drive.rs:368-376` then calls
`alloc.dealloc(l.ptr, layout)` a second time on a pointer it just freed,
which for glibc/msvcrt `free()` is heap corruption and for
`GlobalAlloc::dealloc` is explicitly UB ("`ptr` must denote a block of memory
currently allocated via this allocator").

**Why the existing documentation does not close it.** `RawAllocator`'s
contract is honest about the behaviour but places it in the wrong half of the
split:

> `dealloc` is called only with a pointer previously returned by a matching
> `alloc`/`alloc_zeroed`/`realloc` on `self` with the same `layout`. When
> `Config::double_free` is set the harness deliberately frees the SAME pointer
> a second time (the M2 no-op oracle) — enable it only for an allocator whose
> contract makes that a safe no-op. — `raw_allocator.rs:39-45`

That is a **caller obligation conditioned on a runtime flag**, and the caller
(`drive`) is safe. For a *direct* implementor (`LeakyAllocator` in
`tests/double_free_no_op.rs:35`, `CoreUnderTest` in
`fuzz/fuzz_targets/global_alloc_ops.rs:60` and
`tests/alloc_core_differential.rs`) this is fine: they wrote `unsafe impl` and
accepted the conditional contract. For the blanket impl there is no such
consent, and the blanket impl's own `# Safety` note (`:102-111`) justifies
forwarding **only** against the size/`new_size`/`isize`-overflow triple
("Forwarding is sound only because … `drive` … clamps every op into that
stricter range before calling") — it never mentions `dealloc`'s
matching-pointer precondition, which is precisely the one `drive` breaks on
purpose. The safety argument for the blanket impl is therefore incomplete as
written, not merely under-documented.

**Why this is P0 and not a documented footgun.** Rust's soundness rule has no
"documented danger" exemption for safe functions — that is what `unsafe fn`
and unforgeable tokens exist for. Being off by default reduces the blast
radius but does not change the classification: the value is constructible in
safe code and the API accepts it. This is the identical shape run 1 rated P0
(`P0-1`, run-1 report §P0) — that fix clamped sizes and closed the
`alloc`/`realloc` half of the safe-code-to-UB surface, and this half was never
examined. Nothing in-tree currently triggers it (all three `double_free: true`
call sites use direct `unsafe impl`s), which is exactly why six rounds of
tests, miri and clippy stayed green: **this is an API-shape defect, not a
behavioural one.** It is also the highest-leverage moment to fix it — the
crate is unpublished, so no downstream compatibility is at stake.

**Suggested fix (recommended: an unforgeable token; ~15 lines + 3 call sites).**
Replace the plain `bool` with a value that cannot be produced in safe code,
keeping `Config`'s public-field ergonomics intact:

```rust
/// Proof that the allocator under test treats a redundant `dealloc` of an
/// already-freed pointer as a safe no-op. Unforgeable in safe code: the M2
/// oracle is UB against a real `malloc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoubleFreeOk(());

impl DoubleFreeOk {
    /// # Safety
    /// The allocator subsequently passed to [`drive`] must document that a
    /// second `dealloc` of an already-freed pointer is a safe no-op (e.g.
    /// sefer's `AllocCore`). This is STRONGER than `GlobalAlloc`, which
    /// makes it undefined behaviour — never construct this for `System` or
    /// any allocator reached through the blanket `GlobalAlloc` impl.
    pub const unsafe fn new() -> Self { Self(()) }
}
```

then `pub double_free: Option<DoubleFreeOk>` in `Config`,
`if config.double_free.is_some()` in `drive.rs:368`, `double_free: None` in
`Config::default()`. `Config { double_free: None, ..Config::default() }` still
reads exactly as the README promises. Exactly three in-tree call sites change
(`crates/globalalloc-model/tests/double_free_no_op.rs:87`,
`tests/alloc_core_differential.rs:64`,
`fuzz/fuzz_targets/global_alloc_ops.rs:102`), each already inside a file that
holds `unsafe impl RawAllocator`.

The smaller-diff alternative is to keep `double_free: bool`, have safe `drive`
`assert!(!config.double_free, ...)`, and add
`pub unsafe fn drive_allowing_double_free(...)` — but that trades an
unforgeable compile-time proof for a runtime assert plus a second entry point,
and leaves the misleading `bool` in a public struct.

**Either way, two doc corrections belong in the same change:** (1) move the
double-free clause out of `RawAllocator`'s caller-obligations list into a
statement that the *blanket `GlobalAlloc` impl never permits it* (it is a
direct-implementor-only opt-in), and (2) extend the blanket impl's `# Safety`
note (`:102-111`) to cover `dealloc`'s matching-pointer precondition, not just
the size/overflow triple.

---

## P1 — none

---

## P2

### P2-1 — `drive` reads uninitialized memory on exactly the two paths whose oracles it exists to check, contradicting the trait doc's explicit "detecting one must not itself be undefined behavior"

*Axis: ошибки (a latent UB path) + улучшение (an implementor contract that is
one clause short of supporting its own reads).*

**Files:** `crates/globalalloc-model/src/drive.rs:124-148` (`verify_zeroed_block`)
called at `:340`; `:150-175` (`verify_prefix_block`) called at `:428`;
`crates/globalalloc-model/src/raw_allocator.rs:19-22`, `:26-35`, `:64-67`.

`RawAllocator`'s `# Safety` opens with an explicit promise:

> A broken allocator that violates the *behavioral* oracles further down is
> precisely what this crate exists to detect, and detecting one must not
> itself be undefined behavior. — `raw_allocator.rs:19-22`

and then lists **Zeroing** (`:64-65`) and **Prefix preservation** (`:66-67`)
among those violable oracles. But `drive` reads bytes it has not itself written in
exactly two places, and in both the bytes are initialized *only if the
allocator honours the oracle being tested*:

1. **`verify_zeroed_block` (`:138-148`, called at `:340`)** reads
   `ptr.add(b).read()` over `[0, size)` **before** any write. A broken
   `alloc_zeroed` that simply forwards to `alloc` (the single most likely
   real-world break, and the one the Zeroing oracle exists for) returns
   allocated-but-uninitialized memory. Producing a `u8` from an uninit byte
   is UB in the abstract machine; miri reports *"using uninitialized data, but
   this operation requires initialized memory"* — inside `globalalloc-model`,
   which a consumer will read as a bug in the harness rather than a report
   about their allocator.
2. **`verify_prefix_block` (`:158-175`, called at `:428`)** reads
   `[0, min(old_size, new_size))` of the realloc result **before** the
   re-fill at `:435`. A correct `realloc` copies exactly those bytes, so they
   are initialized; a `realloc` that moves the block without copying (the
   `Fault::ReallocNoCopy` shape the Prefix-preservation oracle exists for)
   leaves them uninitialized.

The implementor-guarantee list (`:26-35`) promises only "a pointer valid for
reads and writes of `layout.size()` bytes". In Rust's terminology "valid for
reads" means *dereferenceable*, not *initialized* — so the contract as written
genuinely admits both of the above inputs, and `drive` performs UB on them.
The `verify_zeroed_block` doc comment states the opposite conclusion:

> The definedness of these reads rests on exactly one thing: the
> `RawAllocator` contract, which makes a non-null return valid for reads of
> `layout.size()` bytes. — `drive.rs:129-131`

which is the precise point where "valid for reads" is being read as
"initialized". Note this is *not* the out-of-bounds case — a short block
violates the implementor guarantee, so that one is correctly the implementor's
fault.

**Why six rounds of miri never surfaced it:** every negative fake in
`tests/oracle_negative.rs` writes before returning (`NotZeroed` scribbles
`0xAA` at `:305`, `ReallocNoCopy` returns arena memory that `Arena::new`
pre-zeroed at `:72`), and `system_proptest`/`system_arbitrary`/`miri_bounded`
drive the correct `System`. The gap is only reachable by a *consumer's* broken
allocator — which is the crate's entire audience.

**Suggested fix (contract, not code — the reads cannot be made sound in
today's Rust).** Add the missing initialization clause to the
implementor-guarantee list at `raw_allocator.rs:26-35`, keeping the *values*
as oracles:

- `alloc_zeroed`'s non-null return must point at `layout.size()` **initialized**
  bytes. Whether they are ZERO is the oracle `drive` checks and may fail;
  whether they are initialized is a safety obligation `drive` relies on.
- `realloc`'s non-null return must leave its first
  `min(old_layout.size(), new_size)` bytes **initialized**. Whether they equal
  the old block's contents is the oracle; initializedness is the obligation.

and add a short "Limitations" note to the crate doc: an allocator whose
`alloc_zeroed` hands back genuinely uninitialized memory cannot be checked
soundly by this harness — natively `drive` will still report the oracle
failure, but under miri it reports UB in `drive` instead. (`read_volatile` is
*not* a fix: it is equally UB on uninit in the abstract machine and still
flagged by miri; it would only paper over the report.) Optionally, state the
same clause inline on `RawAllocator::alloc_zeroed`/`realloc`'s per-method
`# Safety` sections, which is the shape run 3's P3-7 and run 4's P3-5
established for this trait.

---

## P3

### P3-1 — `validate_align` is load-bearing (it is the only thing standing between a hand-built `Op` and a divide-by-zero / `clamp` assertion inside `drive`) and has ZERO test coverage anywhere in the crate

*Axis: улучшение (missing coverage of a documented `# Panics` clause) +
ошибки (its counterfactual failure mode is an internal arithmetic panic, not
the named rejection).*

**Files:** `crates/globalalloc-model/src/drive.rs:54-66` (`validate_align`),
called at `:243` and `:301`; the documented contract at `:216-219`. No test:
`grep -rn "align: 0\|align: 3\|not a usable Layout"` over
`crates/globalalloc-model/tests/` returns only `Config::max_align` cases in
`config_validate.rs:19`/`:29` — never an `Op`. The full set of `Op` aligns used
across all six test files is `{1, 8, 16, 64, 4096}`, all valid.

`drive`'s `# Panics` section promises:

> Also panics when an op carries an align `Layout` can never admit (zero,
> non-power-of-two, or one whose round-up overflows `isize`): unlike sizes,
> an align cannot be clamped into range. — `drive.rs:216-219`

Nothing tests any of the three shapes, and `validate_align` is not merely a
message-quality nicety — it is the guard that keeps the very next line from
misbehaving. With it deleted, all three inputs produce a *different and much
worse* failure:

| hand-built op | with `validate_align` | without it |
|---|---|---|
| `Op::Alloc { size: 32, align: 0 }` | `"op #0: align 0 is not a usable Layout alignment"` | `(isize::MAX as usize / 0)` at `:245` → **"attempt to divide by zero"** |
| `Op::Alloc { size: 32, align: 3 }` | same named rejection | `layout_for` at `:247` → `"Layout::from_size_align(...) rejected"` (the message run 4's clamp fix explicitly moved *away* from) |
| `Op::Alloc { size: 32, align: 1 << 63 }` | same named rejection | `(isize::MAX/2^63)*2^63 == 0`, so `size.clamp(1, 0)` at `:245` → **`Ord::clamp`'s `min <= max` assertion** |

Two of the three are internal arithmetic panics with no op index and no
explanation — the exact class of harness-blames-itself failure the whole
"totality" design (run 1 P0-1 → run 2 P2-4 → run 4 P3-1) was built to
eliminate. The crate has 20 tests pinning the *clamp* half of that design in
both directions on all three arms, and zero pinning the *rejection* half.

**Suggested fix.** Three `#[should_panic(expected = "is not a usable Layout
alignment")]` tests in `tests/oracle_negative.rs`, one per shape, against
`Fault::Honest`; e.g.

```rust
#[test]
#[should_panic(expected = "op #0: align 0 is not a usable Layout alignment")]
fn zero_align_is_rejected_not_divided_by() {
    drive(&faulty(4096, Fault::Honest), Config::default(),
          &[Op::Alloc { size: 32, align: 0 }]);
}
```

plus the `align: 3` and `align: 1 << 63` twins. Each is genuinely
counterfactual (deleting `validate_align` changes every one of the three
messages), and the `align: 0` case in particular pins that the guard runs
*before* the clamp's division — an ordering the comment at `:56-58` already
claims ("Called before any size is clamped against a ceiling derived from the
align") but nothing enforces. Consider also adding one for the `AllocZeroed`
arm's call site at `:301`, which is a separate `validate_align` invocation
with its own deletion risk (this is the same per-arm asymmetry run 5's P3-1
and run 6's P3-1 each found once already).

### P3-2 — the miri drift-check pins each step's `MIRIFLAGS` but not its `--all-features`; ONE edit silently drops half the crate's test targets from both miri passes while the check stays green (reproduced)

*Axis: улучшение (a gate that does not enforce the property its own text
states).*

**Files:** `.github/workflows/ci.yml:933-1011` (the check step),
`:1015`/`:1019`/`:1020`/`:1024` (the four run lines).

The job's comment states the whole-suite claim is "ENFORCED, not
hand-asserted" (`:894-901`), and `6243b54` correctly closed round-6's P4-1 by
pinning each step's `MIRIFLAGS` shape individually. But the check extracts only
two things from a run line: the `--test` target names
(`grep -oE -- '--test[= ][A-Za-z0-9_-]+'`, `:963`) and the following
`MIRIFLAGS:` value (the `awk` extractor at `:981-993`). **`--all-features` is
checked by nothing** — not by the `found` count grep at `:945` (its pattern
stops at `-p globalalloc-model`), not by the per-pass `diff` at `:966`, not by
`expect_flags` (`:999-1011`).

That matters here specifically because this crate's `default = []` cfg-gates
its front-end test files to *empty* — a fact `release.yml:444-449` already
records as a hazard in its own words ("`default = []` cfg-gates both front-end
test files to empty, so the default suite alone cannot see a break in
either"). Drop `--all-features` from one run line and:

- `tests/system_proptest.rs` (`#![cfg(feature = "proptest")]`) compiles to an
  empty test binary — 0 tests, exit 0;
- `tests/system_arbitrary.rs` (`#![cfg(feature = "arbitrary")]`) — likewise;
- `tests/config_validate.rs` loses its two front-end tests (`:92-104`,
  `:106-119`).

so that miri pass silently covers 3 of 6 targets while reporting success.

**Reproduced, not inferred.** I copied `ci.yml` to a temp directory, removed
` --all-features` from the plain multi-target run line (`:1019`) only, and ran
the step's own pipeline verbatim against both files:

```
### real ci.yml ###          CHECK PASSED (found=4, steps=4)   exit=0
### sabotaged copy ###       CHECK PASSED (found=4, steps=4)   exit=0
```

This is a strictly stronger defect than the P4-1 it sits next to: that one
needed a *compensating pair* of edits, this one needs **one**, and
single-edit drift is precisely the class every prior version of this check
did catch.

**Suggested fix (~4 lines, reusing the machinery `6243b54` already built).**
Fold the flag into the per-step record the `awk` extractor already produces —
capture the run line's tail alongside its `MIRIFLAGS`, and assert
`--all-features` is present in each of the four:

```sh
missing=$(printf '%s\n' "$job" \
  | grep -E '^[[:space:]]*- run: cargo miri test -p globalalloc-model' \
  | grep -cv -- '--all-features' || true)
if [ "$missing" -ne 0 ]; then
  echo "$missing miri run line(s) lack --all-features; the proptest/arbitrary" >&2
  echo "test files are cfg-gated to EMPTY without it (default = [])." >&2
  exit 1
fi
```

### P3-3 — half of `system_arbitrary`'s 32-seed sweep decodes to an EMPTY op stream by construction, in the one test whose stated job is proving decoding is non-vacuous

*Axis: ошибки (a test that does not do what its own doc says) + улучшение
(coverage silently halved).*

**File:** `crates/globalalloc-model/tests/system_arbitrary.rs:43-48` (with the
non-vacuity contract stated at `:4-5` and `:61-70`).

The module doc says the test proves "that decoding is non-vacuous — a decode
change that silently empties the streams (or drops a variant) fails the
per-variant asserts below". But the byte generator interacts with
`arbitrary`'s continuation bit so that **every even seed yields zero ops**:

```rust
let bytes: Vec<u8> = (0u16..512)
    .map(|i| (i as u8).wrapping_add(seed).wrapping_mul(31))
    .collect();
```

`ArbitraryIter::next` (`arbitrary-1.4.2/src/unstructured.rs`) begins each
iteration with `self.u.arbitrary::<bool>().unwrap_or(false)`;
`bool::arbitrary` is `u8::arbitrary(u)? & 1 == 1`
(`src/foreign/core/bool.rs:4-7`) and `u8::arbitrary` takes the **front** byte
via `fill_buffer` (`src/foreign/core/num.rs`, `unstructured.rs`). So the very
first decision reads `bytes[0] = (0 + seed).wrapping_mul(31)`, and since 31 is
odd, `bytes[0] & 1 == seed & 1`. For all 16 even seeds in `0u8..32` the
iterator returns `None` immediately and `drive(&System, config, &[])` runs on
an empty slice — a trivial pass.

The existing asserts cannot notice: `total_ops > 0` and the four
`seen[idx] > 0` checks are aggregates over all 32 seeds, and the 16 odd seeds
satisfy them on their own. Consequences: the sweep's real coverage is 16
seeds, not 32; and the module doc's miri-cost rationale at `:20-28` (this test
being "the dominant cost of the CI miri job") is calibrated against a workload
half the size it describes.

**Suggested fix.** Add the per-seed assertion that would have caught this —
it fails today, which is the point —

```rust
assert!(
    !stream.ops.is_empty(),
    "seed {seed} decoded to an empty stream (arbitrary's continue-bit is the \
     low bit of bytes[0]; keep it odd for every seed)"
);
```

and make `bytes[0]`'s low bit seed-independent, e.g. by seeding a small LCG
(`x = x.wrapping_mul(1664525).wrapping_add(1013904223 ^ seed as u32)`) or by
simply prepending a constant odd byte before the 512-byte body. Either keeps
the buffers deterministic while restoring all 32 seeds. (Sizing note: with the
fix, per-seed cost roughly doubles the current miri wall-clock for this
target; the existing `cfg!(miri)` shrink at `:30-38` already bounds it, and
the `MAX_OPS`/512-byte input cap bounds it further.)

---

## P4

### New this round

1. **`crates/globalalloc-model/tests/oracle_negative.rs:143-144`, `:151-153`**
   *(smell — an undocumented invariant a future cell will break)*.
   `Fault::MisalignedBy`/`MisalignedZeroedBy` return `at_len(k, size)` **without
   advancing the arena cursor**, so they are only safe to use in a stream where
   nothing else is live. Both variant docs explain the length check but not the
   non-reservation. A future editor moving either fault into a multi-op stream
   gets a block aliasing an honest one, and `assert_no_overlap` would then fire
   *before* the align assert — silently changing what the cell proves, which is
   exactly the failure mode the grid exists to prevent. One clause per variant.
   (`ReallocAt` has the same property and the `Realloc × Misaligned` cell
   already depends on drive's internal check ORDER to survive it; that cell's
   comment at `:571-575` does say so, which is the pattern to copy.)
2. **`crates/globalalloc-model/tests/oracle_negative.rs:552-569`**
   *(improvement — a weak counterfactual)*. The `Realloc × Null` cell's only
   observable is "drive completed", so it catches `continue` being *deleted*
   (null deref → crash) but not `continue` being changed to `break`, nor a
   variant that drops the block from `live` instead of keeping it — both leave
   the run completing. `Faulty` already owns interior-mutable state; a
   `Cell<usize>` dealloc counter asserted at `== 2` after the run would pin the
   "both blocks survive to teardown" half the cell's own comment claims to test.
3. **`crates/globalalloc-model/src/drive.rs:227`** *(smell)*.
   `Vec::with_capacity(ops.len())` is an exact upper bound, but for a
   dealloc-heavy stream it over-commits 32 bytes per op (a 2048-op fuzz input
   reserves 64 KiB even if one block is ever live), and that memory comes from
   the *global* allocator the reentrancy note at `:196-201` warns about. Harmless
   today; worth one clause acknowledging the trade rather than only the
   grow-once benefit.
4. **`crates/globalalloc-model/README.md:92-108`** *(improvement)*. The usage
   example uses a ` ```text ` fence. CLAUDE.md's no-doctests rule scopes to
   `src/**/*.rs` doc comments, and this README is not pulled in by
   `#![doc = include_str!]` anywhere — so ` ```rust ` is safe here and would
   restore syntax highlighting on what is the crates.io landing page. (Keeping
   ` ```text ` is defensible only as insurance against a future
   `include_str!`; if that is the intent, say so in a comment.)
5. **`crates/globalalloc-model/tests/oracle_negative.rs:497-504`**
   *(improvement — one clause)*. `arm_of` forces the *decision* for a new `Op`
   variant, as its rewritten doc now correctly says; but classifying a genuinely
   block-creating variant as `None` is still silent. Naming that in the doc
   ("returning `None` for a block-creating variant is the one way to skip the
   grid — do not") costs a line and closes the last soft edge of round-6's fix.
6. **`crates/globalalloc-model/src/lib.rs:106`, `Cargo.toml:42`**
   *(improvement — a feature-unification hazard stated only as an upstream
   limitation)*. `#![cfg_attr(not(feature = "arbitrary"), no_std)]` means the
   crate silently becomes `std` for *every* consumer in the graph the moment
   **any** crate enables `arbitrary` — Cargo features are additive and unified.
   The docs (`lib.rs:96-105`, `README.md:64-67`) explain *why* `arbitrary` needs
   `std` (`derive_arbitrary`'s `::std::thread_local!`, verified at
   `derive_arbitrary-1.4.2/src/lib.rs:62`) but never state the consequence for a
   `no_std` consumer sharing a workspace with an `arbitrary`-enabling crate.
   One sentence in the README's `no_std` section.
7. **Correction to round-6 P4-29** *(not a new defect — a precision fix to a
   carried item)*. That item lists `Cargo.toml:6`'s "review P3-23" as
   "crates.io-rendered". It is not: `cargo publish` normalizes the manifest and
   strips comments; the verbatim original ships as `Cargo.toml.orig`, which no
   registry page renders. `CHANGELOG.md:91-93` remains genuinely reader-facing
   for anyone browsing the repo. The item stands, at lower stakes.

### Carried from round 6 — re-verified as still present at `6243b54`

`6243b54` closed round-6's **P3-1**, **P4-1** and **P4-2**. The other 26
remain, with the same reasoning as run 6 §P4 (numbers are round-6's, then
round-5's for the ones it carried):

round-6 **#3** the three `"M1/M4:"` pins (`:533`, `:550`, `:581`) still do not
name which arm fired · **#4** `MisalignedZeroedBy`'s doc still omits its
reliance on `Arena::new`'s pre-zeroing · **#5** no CI row builds this (or any)
crate with `--cfg docsrs`, so `lib.rs:120`/`:127`'s `doc(cfg)` attributes are
never compiled · **#6** the per-pass `listed=$(…)` pipeline (`ci.yml:959-964`)
still has no `|| true`, so a zero-`--test` shape exits opaquely under
`pipefail`.

round-5 **#1** `config.rs:36-37`/`:52-53` "guaranteed M1 null report"
overstatement · **#2** `arbitrary_stream.rs`'s `u32` size cap (~429 MiB reach)
undocumented in `Config::small_max`/`large_max` · **#4** `raw_allocator.rs:49-55`
scopes the `GlobalAlloc` triple to blanket-impl callers while the crate's own
direct implementors assert it · **#6** `lib.rs:92`'s crate-level
`#![allow(unsafe_code)]` is inert (no `[lints]` table in this crate's
`Cargo.toml`, and `unsafe_code` is allow-by-default) · **#8**
`config_validate.rs` pins two of `validate()`'s three `max_align` clauses
(`1 << 63` untested) · **#9** `ci.yml:2424` still says "its **four** tests/
files" (there are six) · **#10** `ClobberOnLaterAlloc` / `WritesDoNotStick` are
one mechanism differing only in the byte written · **#11** `CHANGELOG.md` still
never mentions `Config::validate` · **#12** `system_proptest.rs:20-21` shrinks
`CASES`/`MAX_LEN` under miri but not `large_max`, unlike its `system_arbitrary`
sibling · **#13** `CHANGELOG.md:23-25`'s totality claim omits the
never-admissible-align caveat (the same clause P3-1 above is about) · **#14**
`config.rs:68-76`'s `max_align` doc omits the `arbitrary` front-end's absolute
`2^21` cap · **#15** `ALIGN_POW_CAP_EXP`'s sefer-specific 4 MiB rationale sits
in a non-configurable constant · **#16** `strategy.rs:26`'s unreachable
`debug_assert!` · **#17** `arbitrary_stream.rs:56`'s dead `.max(1)` · **#18**
`drive.rs:48-52` `layout_for`'s panic is unreachable at all four call sites ·
**#19** proptest's `no_std` feature forces `num-traits/libm` into every
consumer graph · **#20** `drive.rs:451-462` passes a **block** index into
`verify_block`'s `step` parameter (`M3: step #0 …`) · **#21** teardown
re-implements `layout_for` inline (`drive.rs:468-469`) · **#22** inert
`#[must_use]` on `ranges_overlap` · **#23** `assert_no_overlap`/`ranges_overlap`
compute the same saturating sums twice · **#24** `double_free_no_op.rs`'s
`dealloc_count` duplicates `frees.len()` · **#25** `system_arbitrary.rs:44`'s
`(0u16..512).map(|i| (i as u8)…)` truncates — the "512-byte" buffer is 256
bytes twice (and see **P3-3**, which is the same line for a sharper reason) ·
**#26** `bucket`/`magnitude` derive from the same `u32`, clustering the large
arm near `small_max + 1` · **#27** `RawOp` is private yet fully `///`-documented ·
**#28** `OpStream`'s doc demotes links the module header makes · **#29**
internal review identifiers ship in `CHANGELOG.md:91-93` (see new #7 above).

(30 P4 total: 7 new, 23 carried.)

---

## Summary

| Severity | Count |
|---|---|
| P0 | 1 |
| P1 | 0 |
| P2 | 1 |
| P3 | 3 |
| P4 | 30 (7 new, 23 carried) |

**P0–P3 count is 5 — NOT zero. The stop condition for this review-fix cycle is
not met.**

**P0–P3 trend: 47 → 14 → 10 → 5 → 2 → 1 → 5.**

The three prior rounds each closed a real gap and each ended with a
recommendation to conclude; run 6 in particular verified the crate's *runtime
behaviour* thoroughly and correctly, and I found nothing wrong with it either.
What this round found instead is that rounds 3–6 progressively narrowed onto
the test suite and the CI wiring — the surfaces that were changing — and never
re-audited the **API shape** after run 1's P0-1 fix landed. `drive` is safe,
`Config` is a public-field struct by deliberate design, and `RawAllocator` has
a blanket impl over the entire `GlobalAlloc` ecosystem; those three facts
compose into exactly one remaining safe-code-to-UB path (**P0-1**), and into a
contract that is one clause short of justifying its own reads (**P2-1**). Both
are API/contract defects with zero behavioural symptom, which is why a green
`cargo test`, a green miri job and six rounds of `#[should_panic]` pins could
not see them.

The three P3s are each a "the gate does not enforce what its text claims"
finding in a different layer: an untested guard whose deletion yields a
divide-by-zero (**P3-1**), a CI check one flag short of its stated scope
(**P3-2**), and a non-vacuity test that is half vacuous (**P3-3**). All three
are small, mechanical fixes; **P3-2** is newly reachable because `6243b54`
extended precisely that check, so it is the one item that genuinely belongs to
the last round rather than to the crate's history.

**Recommendation: do not tag 0.1.0 until P0-1 is fixed.** It is the only
finding here that blocks publication — an unpublished crate can change
`Config::double_free`'s type for free, a published one cannot. P2-1 is a
docs-and-contract change that should ship in the same commit (it touches the
same trait). The three P3s are worth landing before the tag but do not block
it. A run 8 is warranted, scoped narrowly to the P0/P2 fix's diff and its three
in-tree call sites — not another full pass.
