# `globalalloc-model` — pre-publish quality review, run 2 (read-only)

**Verdict: GO-WITH-FIXES.** Every round-1 P0/P1/P2 fix holds up under independent
re-derivation — `drive()`'s clamps are correct *and* tight, `assert_no_overlap`'s
`skip` semantics are right at all three call sites, and the new
`tests/oracle_negative.rs` faults genuinely fire and stay inside their arenas — so
there is **no P0 and no P1**. Four P2 findings remain, all of them introduced or
left open *by the round-1 fixes themselves*: the `no_std` proptest wiring silently
turned the crate's own property test into a fixed-seed constant, the Config-aware
fuzz front-end silently narrowed the only real fuzz consumer's size/align range by
~9 octaves, `RawAllocator`'s safety contract forbids the crate's own headline use
case, and `drive()`'s "total over every hand-built `Op`" claim is contradicted by
its own `# Panics` section three paragraphs later. None blocks the tarball; all
four are cheap.

Scope: `crates/globalalloc-model/` at `e707d76` (`main`), static read only — no
`cargo` command was run. Round-1 report:
`docs/reviews/2026-09-06-211721-globalalloc-model-review-ox-run-1.md`.

---

## 0. Round-1 fix verification (what I re-derived, independently)

Recorded explicitly because this is a fix-verification round; every item below was
derived from the code, not taken from the commit message.

**P0-1 (`drive()` clamping) — holds, and the bound is exactly right.**
`src/drive.rs:211` / `:243` apply `size.max(1)` before `layout_for`, so
`GlobalAlloc`'s non-zero-size precondition cannot be violated on either
block-creating arm. `src/drive.rs:302-305` clamps `new_size` into
`1 ..= (isize::MAX / align) * align`. That upper bound is neither loose nor
unsafe: `(isize::MAX / align) * align` is the largest multiple of `align` that is
`<= isize::MAX`, and `round_up(n, align) <= isize::MAX` holds for exactly the `n`
at or below it — i.e. it is `Layout::from_size_align`'s admissible ceiling
verbatim, so the teardown `Layout::from_size_align(...).expect(...)` at
`src/drive.rs:371` provably cannot fire for a realloc'd block.
The clamp's own hidden precondition (`clamp(1, hi)` panics if `hi < 1`, and
`hi == 0` when `align > isize::MAX`) is also unreachable: a `Live` entry only
exists after `layout_for` accepted `(size >= 1, align)`, and
`round_up(size >= 1, align) <= isize::MAX` forces `align <= isize::MAX`. Clean.
The `% align` divisions at `:221`, `:253`, `:314` are likewise unreachable with
`align == 0`, because `layout_for` runs first on every path.

**P2-3 (`assert_no_overlap` at all three sites) — holds, with correct `skip`.**
Called with `skip: None` at `:226` (alloc, before `live.push`) and `:258`
(alloc_zeroed, before `live.push`), and with `skip: Some(i)` at `:327` (realloc,
before any read/write through `new_ptr` and before `live[i]` is replaced). The
`Some(i)` exclusion is the right one and only that one: the old extent at index
`i` is the single region a legal realloc may legitimately overlap (in-place, or a
move into the just-freed old block), and index `i` is overwritten immediately
afterwards, so no duplicate pointer can survive into the teardown free-walk.
Ordering is right everywhere — overlap is asserted *before* `verify_zeroed_block`,
`verify_prefix_block`, and `fill_block` on every path. Zero-size ranges cannot
mask an overlap either, since every `Live.size` is `>= 1` after the P0-1 clamp.

**P1-2 (`tests/oracle_negative.rs`) — the harness is arithmetically sound today.**
I re-derived every fault's offsets against its own arena capacity:
`overlap_on_alloc_zeroed_panics` touches at most byte 80 of 4096;
`overlap_on_realloc_panics` at most byte 144 of 4096 (`ReallocToForeign{off:80}`
copies 64 bytes to `base+80`, and `[80,144)` does overlap live block 1's
`[64,128)` while staying disjoint from the old block `[0,64)` — the comment at
`:218` is correct); `undersized_block_overlap_panics` reaches byte 120 of 4096
(`ShortBlock` at op 1 returns offset `((63-1)/8)*8 = 56`, and `[56,120)` overlaps
`[0,64)`); `realloc_without_copy_loses_prefix` reaches byte 192 of 4096;
`in_place_realloc_inside_own_old_block_passes` reaches byte 72 of 4096 (the
overlapping `ptr::copy` is a `memmove`, so it is well-defined);
`honest_arena_passes_drive` reaches byte 512 of 65536. `Fault::ShortBlock` never
underflows (`size - 1` is safe because `drive` guarantees `size >= 1`).
`Fault::MisalignedBy(1)` and `Fault::NullAlloc` panic before any write.
**No arena is undersized, and no pointer arithmetic goes out of bounds.** (Two
*latent* fragilities in this harness are filed as P3-5 and P3-6 below — they are
about future edits, not present behaviour.)

**The 4-way file split — clean.** `lib.rs` is `mod`/`pub use` + docs only;
each new file has exactly one `pub` item (`RawAllocator` + its blanket impl,
`Op`, `Config`, `drive`), matching this repo's one-file-one-export rule. All
intra-doc links resolve: `crate::Config::double_free`,
`RawAllocator#safety`, `RawAllocator::alloc`, `[`Layout`]`, `[`GlobalAlloc`]`,
`[`Strategy`]`, `[`Arbitrary`]` are each in scope at their site. I specifically
checked the `crate::drive` links (`src/raw_allocator.rs:1`, `:7`;
`src/strategy.rs:41`; `src/arbitrary_stream.rs:2`) for a private-module /
public-function namespace ambiguity — this repo already ships the identical shape
in `crates/aligned-vmem` (`mod page_size;` + `pub use page_size::page_size;` +
bare `crate::page_size` doc links at `api/commit_range.rs:19` and
`api/recommit.rs:55`) under a green `RUSTDOCFLAGS="-D warnings" cargo doc` gate,
so this is **not** a finding.

**CI/release wiring — correctly wired.** `release.yml`'s `globalalloc-model`
branch in the test gate closes the `default = []` blind spot exactly as intended;
`ci.yml:2085` (bare-metal `--features proptest`), `:2103` (default-feature doc
row), `:2091-2093` and the new `globalalloc-model-miri` job are all real, wired to
the right package, and use the right flags. Three secondary gaps in that wiring
are filed as P3-7, P3-8 and P3-9.

---

## P0 — none

No soundness bug and nothing that blocks publication. The one safe-`pub fn`
reachable-UB path round 1 found (`drive()` trusting a hand-built `Op`'s raw size)
is genuinely closed, per §0 above.

## P1 — none

Nothing at this severity. The counterfactual coverage round 1 asked for exists,
is real, and fires for the right reason in 8 of 9 cases (the ninth,
`overlap_on_alloc_zeroed_panics`, still fires and still fails without the fix, but
for a different reason than its comment states — P3-4).

---

## P2

### P2-1 — the crate's own proptest suite runs on a hardcoded constant seed
*axis: bug (coverage regression, introduced by round-1's P2-4 fix)*

**Where:** `crates/globalalloc-model/Cargo.toml:43`, exercised by
`crates/globalalloc-model/tests/system_proptest.rs:18-27`,
`.github/workflows/ci.yml:2091-2092`, `.github/workflows/ci.yml:908`/`:911`,
`.github/workflows/release.yml` (the new `--all-features` test-gate branch).

**What's wrong:** the dependency is declared unconditionally as

```toml
proptest = { version = "1", optional = true, default-features = false, features = ["alloc", "no_std"] }
```

— i.e. **without proptest's `std` feature**. proptest 1.11's RNG bootstrap is
`TestRng::default_rng`, whose `#[cfg(not(feature = "std"))]` branch
(`proptest-1.11.0/src/test_runner/rng.rs:423-426`) returns
`Self::deterministic_rng(algorithm)`, which uses the hardcoded constants
`SEED_FOR_XOR_SHIFT` / `SEED_FOR_CHA_CHA` (`rng.rs:504-516`, `:429-441`) and
ignores the requested seed entirely. There is no `hardware-rng` feature enabled
to rescue it.

Consequently, in every configuration where the crate is built *as its own
package* — `cargo test -p globalalloc-model --all-features` (both CI rows), the
new miri job's two invocations, and release.yml's pre-publish gate —
`system_matches_reference_model` explores **the same 64 op streams (4 under miri)
on every run, on every machine, forever**. It is a fixed regression suite wearing
a property test's name. The std-gated env knobs (`PROPTEST_CASES`,
`PROPTEST_DISABLE_FAILURE_PERSISTENCE`) are also inert, and
`failure_persistence: None` at `system_proptest.rs:19` is a no-op because the
no-`std` default is already `None`.

Downstream impact is small and worth stating fairly: a user of the `proptest`
front-end must also depend on `proptest` themselves (the `proptest!` macro is not
re-exported), and their default-featured dep unifies `std` back in — so their
runs stay randomised. The loss is entirely this crate's own CI. That does not
make it cosmetic: it silently removes the randomised exploration that is the
*only* reason the proptest front-end exists, in the exact place the project would
notice a regression.

**Fix:** add `[dev-dependencies] proptest = { version = "1" }` (default features)
to `crates/globalalloc-model/Cargo.toml`. Resolver v2 keeps dev-dependency
features out of non-test builds, so `cargo build -p globalalloc-model --features
proptest --target thumbv7em-none-eabi` (`ci.yml:2085`) stays an honest `no_std`
proof. Separately, document in `Cargo.toml`/README that a consumer who enables
`proptest` *without* their own std-featured proptest gets deterministic seeding.

### P2-2 — the Config-aware fuzz front-end silently narrowed the fuzz range ~9 octaves, and `ALIGN_POW_CAP_EXP` is now dead
*axis: bug (coverage regression, introduced by round-1's P2-6 fix)*

**Where:** `crates/globalalloc-model/src/arbitrary_stream.rs:20-23` (the const),
`:47-52` (`bound_align`), `:140-144` (the `Arbitrary` impl);
`fuzz/fuzz_targets/global_alloc_ops.rs` (the `fuzz_target!` body and its
trailing comment); `crates/globalalloc-model/CHANGELOG.md:40-44`.

**What's wrong:** `impl Arbitrary for OpStream` — which is what
`fuzz_target!(|stream: OpStream|)` actually drives — delegates to
`arbitrary_with_config(u, Config::default())`. `Config::default()` has
`max_align: 4096` and `large_max: 128 * 1024`. `bound_align` computes
`cap.trailing_zeros().min(ALIGN_POW_CAP_EXP)` = `min(12, 21)` = **12**.

Net effect on the only real fuzz consumer in the tree:

| | before `e707d76` | after `e707d76` |
|---|---|---|
| size | uniform `1 ..= 2 MiB` | `1 ..= 128 KiB`, 9:1 small-weighted |
| align | `2^0 ..= 2^21` (1 ..= 2 MiB) | `2^0 ..= 2^12` (1 ..= 4 KiB) |

The size re-weighting was the *intended* fix (round-1 P2-6) and is a genuine
improvement. The **align collapse was not requested by round 1 and is not
mentioned anywhere in the commit message** — it is a side effect of routing
`bound_align` through `Config::max_align`, whose default was chosen for the
proptest front-end. Four concrete consequences:

1. `ALIGN_POW_CAP_EXP: u32 = 21` (`:23`) is now dead under *every* `Config` whose
   `max_align <= 2 MiB` — the default and both in-tree consumers
   (`tests/alloc_core_differential.rs:63`, `tests/heap_differential.rs:63`, both
   `max_align: 4096`). Its own doc comment's stated purpose ("staying below a
   typical 4 MiB segment so large-align routing is exercised") can no longer
   happen by default.
2. `fuzz/fuzz_targets/global_alloc_ops.rs`'s comment — "the crate's `OpStream`
   front end already bounds sizes (1..=2 MiB) and aligns (2^0..2^21)" — is now
   false, and `fuzz/` is its own workspace (`fuzz/Cargo.toml`'s empty
   `[workspace]`) so nothing type-checks the claim.
3. `CHANGELOG.md:40-41` repeats the same now-false bounds ("`1..=2 MiB` size,
   `2^0..2^21` align") *in the same bullet* that then says the distribution is
   "weighted small-heavy ... rather than uniform `1..=2 MiB`" — self-contradictory
   in published release notes.
4. `OpStream::arbitrary_with_config` — the API round-1 P2-6 asked for — has **zero
   consumers anywhere in the repo** (grepped: only its own definition, the
   `Arbitrary` delegation, `config.rs:9`'s prose, and the CHANGELOG). The
   config-aware route was added and never wired up, so the only thing the change
   actually did in practice was shrink the default.

**Fix:** switch `global_alloc_ops.rs` to `fuzz_target!(|data: &[u8]| { ... })` +
`OpStream::arbitrary_with_config(&mut Unstructured::new(data), cfg)` with a `cfg`
restoring the historical 2 MiB size / 2 MiB align reach (this also finally gives
`arbitrary_with_config` its consumer), or decouple `bound_align`'s cap from
`Config::max_align` when the caller left it at the default. Then correct the three
stale bound citations.

### P2-3 — `RawAllocator`'s safety contract forbids the crate's own headline use case (and its own test suite)
*axis: smell / improvement (contract design, on the crate's only `unsafe trait`)*

**Where:** `crates/globalalloc-model/src/raw_allocator.rs:14-34`.

**What's wrong:** the `# Safety` section folds **oracle** properties into the
**safety** contract. It requires an implementor to "behave as a correct
allocator", and then specifically that a returned pointer is "aligned to
`layout.align()`", that "a non-null pointer from `alloc_zeroed` points at
`layout.size()` zero bytes", and that `realloc`'s result's "first
`min(old_size, new_size)` bytes equal the old block's".

None of those are load-bearing for `drive`'s soundness. `drive` checks alignment
(`:221`, `:253`, `:314`) and overlap (`:226`, `:258`, `:327`) *before* any access
through the pointer, and a non-zeroed or prefix-losing block is only *read* at a
size the allocator itself vouched for. The requirement that actually matters is
narrower: *a non-null return must be valid for reads and writes of the requested
byte count, and must be accepted by `dealloc`.*

As written, the contract makes the crate's entire reason to exist — pointing the
harness at a **broken** allocator — formally UB, and it makes this crate's own
`tests/oracle_negative.rs` a deliberate contract violation:
`unsafe impl RawAllocator for Faulty` returns misaligned pointers (`:113`),
non-zeroed memory (`:141`), overlapping blocks (`:134`, `:157`) and
prefix-losing blocks (`:170`). The code already contradicts the doc explicitly —
`src/drive.rs:112-115` says "a broken allocator's `alloc_zeroed` may return
never-written (uninitialized) memory", which the trait doc declares impossible.

**Fix:** split the section — a minimal `# Safety` (valid for N bytes /
`dealloc`-able / `realloc` consumes-on-non-null) plus a separate non-safety
"What the oracles check" heading holding alignment, zeroing, prefix preservation
and non-overlap. Worth doing pre-publish because it is the crate's semantic
centre, even though *narrowing* an `unsafe` contract later is not a breaking
change.

### P2-4 — `drive()` is documented as "total over every hand-built `Op`" and is not
*axis: bug (doc vs. behaviour, on the single public entry point)*

**Where:** `crates/globalalloc-model/src/drive.rs:166-168` and
`crates/globalalloc-model/CHANGELOG.md:23-25`.

**What's wrong:** the rustdoc says

> `drive` is total over every hand-built `Op` value: sizes of `0`, `new_size` of
> `0`, and overflowing `new_size`s are clamped into the range `GlobalAlloc`'s own
> contract permits (P0-1), so the allocator is never invoked outside its
> documented preconditions.

and the CHANGELOG generalises it further to "zero/**oversized** sizes are clamped".
Only `Op::Realloc::new_size` is clamped at both ends (`:302-305`).
`Op::Alloc`/`Op::AllocZeroed` get `size.max(1)` only (`:211`, `:243`); an
oversized `size` is **not** clamped — it reaches `layout_for` (`:48-52`) and
panics with `op #N: Layout::from_size_align(size=..., align=...) rejected: ...`.
The same doc's own `# Panics` section admits this three paragraphs later
(`:192-194`), so the rustdoc contradicts itself on the page docs.rs will render.

This matters beyond pedantry because `Op` is a **public exhaustive type with
public fields** and the crate ships two front-ends over it — a consumer with its
own generator (an explicitly supported shape) gets a harness-rejection panic whose
message reads like an allocator failure, on an input the docs promised was
handled.

Secondary note on the realloc side: clamping *upward* to `(isize::MAX/align)*align`
converts an obviously-bogus `new_size` into a maximally hostile ~8 EiB real
allocation request. `drive` tolerates the resulting null (`:309-312`), but not
every allocator-under-test returns null gracefully at that magnitude.

**Fix:** either clamp symmetrically on the alloc arms (validate `align` first,
then `size.clamp(1, (isize::MAX / align) * align)`), or delete the "total" claim
from both the rustdoc and the CHANGELOG and state the rejection explicitly.

---

## P3

### P3-1 — `strategy.rs` claims `max_align == 0` is "handled"; it panics
*axis: bug (wrong comment about a degenerate-input path)*

`crates/globalalloc-model/src/strategy.rs:29-30`:

> Both degenerate inputs are handled: `max_align == 0` yields an empty vec, and a
> `max_align >= 1<<63` stops the shift before it overflows to 0.

An empty vec is **not** handled. `proptest::sample::select` asserts
`!cow.is_empty()` at *construction* time
(`proptest-1.11.0/src/sample.rs:161`, `"Cannot select from empty collection"`), so
`op_strategy(Config { max_align: 0, .. }, ..)` panics — in release too, where the
`debug_assert!` at `:26` is compiled out. (The second half of the comment is
correct; I verified the shift loop terminates cleanly at `max_align == 1<<63`,
producing 64 entries.) `Config::max_align`'s own doc (`config.rs:34-35`) correctly
calls a non-zero power of two a *precondition* — the strategy comment contradicts
it. **Fix:** say the input is *rejected* (debug_assert in debug, proptest's own
assert in release), not handled.

### P3-2 — `Config`'s preconditions are undocumented and enforced differently by the two front-ends
*axis: improvement*

`crates/globalalloc-model/src/config.rs:23-48` vs.
`src/strategy.rs:14-36` vs. `src/arbitrary_stream.rs:29-52`. Three fields:

- **`max_align`** — proptest `debug_assert!`s power-of-two (`strategy.rs:26`,
  compiled out in release); `bound_align` (`arbitrary_stream.rs:50`) silently
  takes `cap.trailing_zeros()`, so `max_align: 3000` silently caps aligns at 8,
  and `max_align: usize::MAX` (the natural spelling of "no limit") silently pins
  **every** align to 1. `config.rs:35` mentions only the debug-assert, not the
  arbitrary path's silent reinterpretation.
- **`small_weight` / `large_weight`** — no precondition documented. Both zero:
  `bound_size` clamps `total` to 1 (`arbitrary_stream.rs:31-35`) and then routes
  100% of ops to the **large** arm, while `prop_oneof!` hands proptest's
  `pick_weighted` a zero weight sum
  (`proptest-1.11.0/src/strategy/unions.rs:104-118`) — undefined territory, not
  documented behaviour. The two front-ends disagree.
- **`small_max` / `large_max`** — no documented ceiling. Any value whose round-up
  to `max_align` exceeds `isize::MAX` (e.g. `small_max: usize::MAX`, which makes
  `bound_size`'s large arm return `usize::MAX` at
  `arbitrary_stream.rs:39-43`) makes both front-ends emit sizes
  `Layout::from_size_align` rejects, so `drive` panics as a harness rejection
  rather than reporting an oracle failure. (The `saturating_*` guard the commit
  message added does correctly prevent a division-by-zero — it just doesn't
  prevent an unusable size.)

**Fix:** document each field's precondition on `Config`, and enforce them once —
e.g. a `Config::validate()` (or `debug_assert!` pair) both front-ends call — so
the two generators cannot disagree about what a degenerate config means.

### P3-3 — the stated rationale for raw byte reads over slices is wrong
*axis: bug (incorrect `unsafe`-adjacent justification)*

`crates/globalalloc-model/src/drive.rs:112-115` and `:132-133` both justify
per-byte `ptr.add(b).read()` over `slice::from_raw_parts` on the grounds that
"a slice over uninitialized memory would make the harness itself unsound".

Both forms are unsound over uninitialised memory: producing a `u8` value from
uninit bytes via `read()` is UB exactly as constructing `&[u8]` over them is —
and here the value is immediately consumed by `assert_eq!`/`assert!`, so it is
not a dead read that could be argued away. The raw-read form buys nothing on the
soundness axis; the *only* thing making these reads defined is the `RawAllocator`
contract (which P2-3 above shows is itself overstated). **Fix:** replace the
rationale with the true one (raw reads let the message name the exact byte offset
without materialising a reference to the whole block), or state plainly that the
reads rely on the trait contract.

### P3-4 — `Fault::OverlapZeroedAt`'s counterfactual mechanism is misdescribed
*axis: bug (wrong comment in the negative-oracle suite)*

`crates/globalalloc-model/tests/oracle_negative.rs:82-85`:

> The arena is zero-filled, so if the overlap check were missing the zero-check
> would pass and only the run-end sweep would notice.

False. `drive` fills block 0 with fill byte `0x01` across `[base+0, base+64)` at
op 0 (`drive.rs:231`), so the overlapping `alloc_zeroed` returning `base+16`
sees `0x01` at byte 0 and `verify_zeroed_block` would fire *immediately* if the
overlap assert were deleted. The test remains genuinely counterfactual (the panic
message changes from `"M3: op #"` to `"alloc_zeroed:"`, so `#[should_panic]`
still fails) — but the stated isolation argument is wrong, and a maintainer
reading it would wrongly conclude the zero-check is not a confound here.

### P3-5 — `Arena::at()` doesn't bound `off + len`, so a future fault can write past the arena
*axis: improvement (latent UB in the test harness)*

`crates/globalalloc-model/tests/oracle_negative.rs:52-56` asserts only
`off <= cap`. Five faults return `at(off)` at an arbitrary offset that `drive`
then writes `size` bytes through without any `off + size <= cap` check:
`MisalignedBy` (`:113`), `ShortBlock` (`:114-123`), `OverlapZeroedAt`
(`:133-135`), `ReallocToForeign` (`:157-163`), `ReallocInPlace` (`:164-169`).
Only `Arena::bump` (`:45-50`) is bounds-checked. As shown in §0 every *current*
test stays inside its arena, but nothing enforces it — a longer op stream against
`ShortBlock` writes past the allocation, which is real UB in the harness rather
than a detected oracle failure, and this file is exactly what the new miri job
exists to police. **Fix:** replace `at(off)` with `at_len(off, len)` asserting
`off.checked_add(len).is_some_and(|e| e <= self.cap)`.

### P3-6 — the honest arena ignores `layout.align()`, so every test is hand-tuned to avoid a spurious M1/M4 failure
*axis: improvement (latent fragility)*

`crates/globalalloc-model/tests/oracle_negative.rs:124-127` (and the honest
`alloc_zeroed`/`realloc` paths at `:136`, `:172`, `:176`) bump by `size` only and
never align the cursor; `Arena::new` fixes the base at align 16 (`:29`). Every op
in the file happens to land at an offset satisfying its requested align — e.g.
`honest_arena_passes_drive`'s third alloc lands at offset 96 and asks for align 8
(`:283-286`); had it asked for align 64, as its two neighbours' shapes suggest it
might, `drive` would panic `M1/M4` at op 2 with no fault injected. Any future op
with `align > 16`, or any edit to an earlier op's size, produces a spurious
failure that reads like an oracle bug. **Fix:** round the bump cursor up to
`layout.align()` on the honest path and give the arena a large base alignment.

### P3-7 — the declared MSRV (1.85) is never verified by CI
*axis: improvement (CI gap; the round-1 P2-8 fix used the wrong toolchain)*

`crates/globalalloc-model/Cargo.toml:5` declares `rust-version = "1.85"`, and the
CHANGELOG (`:84-86`) publishes that number. The rows added by `e707d76`
(`.github/workflows/ci.yml:2316-2317`) live in the `msrv` job pinned to
`dtolnay/rust-toolchain@1.88` — the job's own comment concedes "1.88 here is
above that floor". So the published floor is verified by exactly one manual local
run, never by CI, and `proptest = "1"` will float to a 1.x whose own
`rust-version` may rise above 1.85 (1.11.0 declares exactly 1.85 today —
verified). The same job already carries the per-crate floor-toolchain pattern for
`tagged-index-stack` (`dtolnay/rust-toolchain@1.79` at `ci.yml:2325-2329`).
**Fix:** mirror that block — a `dtolnay/rust-toolchain@1.85` step plus
`cargo check -p globalalloc-model --all-features`.

### P3-8 — the miri job disables leak checking crate-wide for one test file's benefit
*axis: improvement*

`.github/workflows/ci.yml:908`/`:911` apply `-Zmiri-ignore-leaks` to
`cargo miri test -p globalalloc-model --all-features` as a whole, but only
`tests/double_free_no_op.rs`'s `LeakyAllocator` needs it. The property it
suppresses is precisely the one `drive` advertises — "All survivors are freed and
the model dropped before returning (no UAF in a teardown walk)"
(`drive.rs:161-162`, `:368-369`, `CHANGELOG.md:28-30`) — so a break in the
teardown free-walk at `drive.rs:370-376` would now go unreported by the very job
added to police that code. **Fix:** run `--test double_free_no_op` with
`-Zmiri-ignore-leaks` and the other four test targets without it.

### P3-9 — `system_arbitrary.rs` has no `cfg!(miri)` bound while its sibling does
*axis: improvement*

`crates/globalalloc-model/tests/system_proptest.rs:15-16` carefully reduces to
4 cases / len 24 under miri. `tests/system_arbitrary.rs` has no such reduction:
it decodes 32 seeds of ~60 ops each with blocks up to 128 KiB, byte-fills and
byte-verifies every one (and `verify_zeroed_block`/`verify_prefix_block` read one
byte at a time — the slowest possible shape under the interpreter), and the new
job runs the whole thing **twice** (plain, then strict-provenance). That makes an
unbounded test the dominant cost of the miri job, against this repo's own
convention ("miri: run on specific invariant tests and a tiny bounded proptest,
not the full suite"). **Fix:** gate the seed count and/or the `Config` sizes on
`cfg!(miri)`, matching `system_proptest.rs`.

### P3-10 — two `Fault` variants have byte-identical bodies
*axis: smell (duplication)*

`crates/globalalloc-model/tests/oracle_negative.rs:88-95` declares
`ReallocToForeign { off: usize }` and `ReallocInPlace(usize)`; their match arms
(`:157-163` and `:164-169`) are the same three lines
(`let dst = self.arena.at(off); ptr::copy(ptr, dst, keep); dst`). They differ only
in the offset each test passes, and they even disagree on shape (named field vs.
tuple). **Fix:** one `Fault::ReallocAt(usize)`; the two tests' intent is already
carried by their names and their offsets.

---

## P4

1. **`drive.rs:354-366`** *(smell)* — the run-end sweep passes `block_idx` into
   `verify_block`'s `step` parameter, so the message reads `M3: step #2 ...` for
   what is a live-**block** index, not an op index. This is the one place the
   "every failure message names the op index" claim (`drive.rs:192`) does not
   hold. Rename the parameter, or label this call's index distinctly.
2. **`drive.rs:370-372`** *(smell)* — teardown re-implements `layout_for` inline
   with a different message instead of calling it; and that message ("every model
   layout was validated when its block was created") is not literally true for a
   realloc'd block, whose `(new_size, align)` pair was never passed to
   `Layout::from_size_align` — only the `:302-305` clamp guarantees it.
3. **`drive.rs:33`** *(smell)* — `#[must_use]` on the private `ranges_overlap` is
   inert: its only call site is the condition of an `assert!`.
4. **`drive.rs:66-81`** *(smell)* — `assert_no_overlap` computes `end` and
   `other_end` with saturating adds purely for the failure message, while
   `ranges_overlap` recomputes the same two sums. Harmless, but duplicated.
5. **`op.rs:10` / `config.rs:22` / `arbitrary_stream.rs:108`** *(improvement)* —
   `Op` derives `Clone, Debug, PartialEq, Eq` but not `Copy` although every field
   is a `usize`; `Config` derives `Clone, Copy, Debug` but not `PartialEq, Eq`
   (blocking trivial `assert_eq!` in a consumer's own tests); `OpStream` derives
   only `Debug`, not `Clone` (useful for fuzz-corpus manipulation). All three are
   free ergonomics wins and none is breaking.
6. **`tests/double_free_no_op.rs:26-29`, `:93`, `:95`** *(smell)* —
   `dealloc_count: Cell<usize>` duplicates `frees.borrow().len()`, and the test
   asserts both against the same `6`. One of the two fields is redundant.
7. **`tests/oracle_negative.rs:78-81`** *(smell)* — `Fault::ShortBlock`'s enum doc
   says `alloc` "returns the old cursor, so the NEXT allocation starts 1 byte
   inside the previous block's model extent". The code (`:120`) returns
   `(cursor - 1)` rounded **down to the requested align**, and in the actual test
   the next block starts 8 bytes inside, not 1. The inline comment at `:115-118`
   is accurate; the enum doc is not.
8. **`tests/system_arbitrary.rs:26-28`** *(smell)* —
   `(0u16..512).map(|i| (i as u8)...)` truncates, so the "512-byte" seed buffer is
   the same 256 bytes twice. Half the buffer carries no new entropy; either use
   `(0u16..512)` bytes derived from `i` without truncation, or say 256.
9. **`arbitrary_stream.rs:29-45`** *(improvement)* — `bound_size` takes
   `bucket = raw % total` and `magnitude = raw / total` from the same `u32`, so
   the large arm's value is `small_max + 1 + (raw/10) % range`. For the small `u32`
   values a fuzzer most often produces, that pins the large arm at ~4097..4200 and
   `large_max` is effectively unreachable. The docstring's "driving the magnitude
   from the remaining high bits" is directionally right, but the large arm's
   *practical* range is far narrower than `small_max+1 ..= large_max`.
10. **`arbitrary_stream.rs:56-84`** *(smell)* — `RawOp` is private, yet every
    variant and field carries a `///` doc comment. `#![deny(missing_docs)]` does
    not require them (private items are exempt); it is noise.
11. **`arbitrary_stream.rs:106-107`** *(smell)* — the pre-split text linked
    `[`OpStream::ops`]` and `[`crate::drive`]`; the current text demotes both to
    plain code spans while the module header two lines up (`:2`) still links
    `[`crate::drive`]`. Inconsistent within one file.
12. **`README.md:81`** *(improvement)* — the usage block is fenced ` ```rust ` but
    is not compilable as written (`proptest!` and `fuzz_target!` are unimported,
    and the two front-ends cannot coexist in one file). It is not run as a doctest
    today (nothing does `#![doc = include_str!("../README.md")]`), but ` ```text `
    — the fence `src/lib.rs:68` already uses for the identical snippet — is more
    honest and stays safe if the README is ever pulled into the crate docs.
13. **`README.md:30-31` and `:42-43`** *(smell)* — "A normal build (no features)
    has **zero dependencies** — both front-ends are optional" appears verbatim
    twice, twelve lines apart.
14. **`CHANGELOG.md:84-86`, `Cargo.toml:6-10`, `src/drive.rs:168`/`:208`/`:242`/
    `:297`** *(smell)* — published artifacts (the crates.io CHANGELOG, the
    manifest, and rustdoc that docs.rs renders) cite this repo's internal review
    finding numbers ("review P3-23", "review P2-8", "(P0-1)") which mean nothing
    to a reader outside this repository. Keep the reasoning, drop the private
    identifiers — or move them to a code comment rather than a doc comment.
15. **`tests/oracle_negative.rs:45-49`** *(smell)* — `Arena::bump` uses `p + n`
    (debug-panics on overflow, wraps in release) instead of `checked_add`. Not
    reachable from any current test, but it is the same class of unchecked
    arithmetic P3-5 flags one line below.

---

## Summary

| Severity | Count |
|---|---|
| P0 | 0 |
| P1 | 0 |
| P2 | 4 |
| P3 | 10 |
| P4 | 15 |

The two findings worth acting on before the tag are **P2-1** and **P2-2** — both
are *silent coverage regressions introduced by round-1's own fixes*, and both are
invisible to every existing gate (a fixed-seed proptest still passes; a narrowed
fuzz range still compiles). **P2-3** and **P2-4** are documentation-of-contract
defects on the crate's two most-read pages and cost one edit each. Everything at
P3 and below can land after 0.1.0 without embarrassment.
