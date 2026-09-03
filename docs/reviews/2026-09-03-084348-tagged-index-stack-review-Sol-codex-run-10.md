# tagged-index-stack — pre-release code review

**Reviewer:** Sol-codex  
**Run:** Codex run 10  
**User marker:** 2026-09-03 08:41  
**Review captured:** 2026-09-03 08:43:48 +02:00  
**Revision reviewed:** `ee4501d2a00df6e30e8949a9466dde11312a7ee3`  
**Scope:** full static review of the current crate and the changes after run 9

## Verdict

**NO-GO for the final publication checklist until the codegen A/B wrapper is
fixed.** The production stack algorithm has no new P1 soundness finding in this
static pass, and the four commits after run 9 correctly close the earlier
safe-write-hook, feature-CI, A/B-`Result`, and production-documentation issues.
However, the manually dispatched arm64 codegen gate still contains a hard Rust
API mismatch and cannot compile as written. That leaves an advertised release
performance gate unusable and, because the normal build-check does not compile
that template, the defect is not caught by the regular workflow.

After the wrapper is corrected, the remaining findings are P3 maintenance and
coverage issues. They do not indicate a new production algorithm failure, but
they should be cleaned up before treating the crate and its release evidence as
finished.

## Scope and method

This was a read-only, single-agent review. I inspected the current source,
manifest, tests, scripts, workflow, documentation, recent commit history, and
the diff from the previous review. I used only repository-reading and VCS
inspection commands.

I deliberately did **not** run `cargo`, `rustc`, `node`, tests, clippy,
rustdoc, loom, Miri, benchmarks, examples, or repository scripts. Therefore
this report makes no claim that the current tree builds or that any test is
green. The codegen finding is a static type-contract conclusion from the
declared Rust signatures and call sites.

## Findings

### P2 — codegen A/B wrapper still uses a safe function-pointer type

**Location:**
`crates/tagged-index-stack/scripts/tis_p3_ab/codegen_wrapper.rs.tmpl:24-27`
and `crates/tagged-index-stack/src/imp.rs:1708-1714`.

The wrapper declares:

```rust
let push: fn(&tis::ArrayIndexStack<16, 256>, u32) = tis::ArrayIndexStack::push;
black_box(push)(black_box(&stack), black_box(1u32));
```

The current API is:

```rust
pub unsafe fn push(&self, index: u32) -> Result<(), TagExhausted>
```

An `unsafe fn` item does not coerce to a safe `fn` pointer. The wrapper thus
has a hard compile-time type mismatch. Even after changing the pointer type,
the returned `Result` is still ignored; the `pop` result is also discarded.
The latter are `must_use` issues and should be handled explicitly.

The affected path is real: the arm64 workflow invokes
`tis_p3_ab_runner.mjs --mode codegen`, and that mode invokes `rustc` directly
on the materialized `codegen_wrapper.rs.tmpl`. The runner's build-check mode
materializes `harness_bin.rs` instead, so the new Group B fix does not cover
this second wrapper. The normal per-PR path also does not run the manual
workflow-dispatch job.

**Impact:** the static codegen identity/delta gate cannot provide its promised
evidence. This is primarily a release-process/performance-gate blocker rather
than a demonstrated defect in the runtime stack, but it makes the current
publication evidence incomplete.

**Recommended fix:** migrate the declaration to the actual unsafe, fallible
signature and discharge both contracts explicitly, for example:

```rust
let push: unsafe fn(
    &tis::ArrayIndexStack<16, 256>,
    u32,
) -> Result<(), tis::TagExhausted> = tis::ArrayIndexStack::push;
// SAFETY: the fresh ArrayIndexStack owns its links; index 1 is in range and
// is not live in the stack.
let _ = unsafe { black_box(push)(black_box(&stack), black_box(1u32)) }
    .expect("codegen probe push must not hit tag seal");
let _ = black_box(pop)(black_box(&stack));
```

The exact safety argument should match the public `push` contract. The
build-check should also materialize and compile the codegen wrapper, or the
codegen template should be made the single source of the forced
instantiations, so future API changes cannot bypass the regular drift gate.

### P3 — packaged-artifact gate does not exercise the published feature

**Location:** `.github/workflows/ci.yml:784-797`.

The extracted `.crate` is tested with plain `cargo test`, followed by
`cargo bench --no-run`. The published `test-internals` feature is not tested
in the extracted package. The workspace has the useful whole-feature row at
`.github/workflows/ci.yml:1881`, but that does not prove that the feature-gated
tests and their files survived packaging.

This is not a production-code failure because the feature is explicitly
test-only. It is nevertheless a release-coverage gap: a packaging change
could drop or alter the gated test surface while the isolated package gate
remained green.

**Recommended fix:** in the extracted package, run both plain and feature
enabled test commands, for example:

```text
cargo test
cargo test --features test-internals
```

If the project intentionally does not consider `test-internals` part of the
packaged-artifact contract, document that decision in the gate instead of
silently implying full feature coverage.

### P3 — stale wrap-boundary narrative remains in tests and CI comments

**Locations:** `crates/tagged-index-stack/tests/stack_unit.rs:1-7,
172-203, 247-262, 312-337`; `.github/workflows/ci.yml:1822-1834` and
`1848-1881`.

The current production path checks `tag == TAG_MAX` and returns
`Err(TagExhausted)` before writing the link or performing the next publication
CAS (`src/imp.rs:1381-1383`). The `wrapping_add(1)` at `src/imp.rs:1416` is
reachable only after that guard and therefore does not wrap a live production
tag.

Several test comments nevertheless say that the wrap “happens inside push”,
describe “wrap-boundary coverage”, and refer to the retired
`tests/regression_counter_wrap.rs`. The actual tests shown there exercise
packing arithmetic and sentinel properties, not a production push through the
tag ceiling. The CI comments likewise retain retired filenames and old
“2^48 tag-wrap” wording in otherwise current coverage explanations.

This is documentation/test-narrative drift, not evidence that the seal is
broken. It is risky because it teaches a future maintainer the opposite of the
current non-wrapping contract and can cause an incorrect regression test to be
reintroduced.

**Recommended fix:** rewrite the comments around the actual invariant:
`pack` rejects out-of-range tags; `push` seals at `TAG_MAX`; no production tag
wrap occurs; the empty sentinel preserves the running tag. Remove retired
file references and distinguish historical changelog material from current
behavior.

### P3 — excessive reviewer-facing prose obscures normative contracts

The crate remains unusually comment-heavy. A rough static line count gives
approximately 1,622 comment/doc lines out of 2,096 in `src/imp.rs` and about
470 out of 490 in `src/lib.rs`; the `StackStorage` contract alone spans roughly
`src/imp.rs:599-996`. The exact ratio is only a heuristic, but the maintenance
signal is clear.

The detailed unsafe and memory-ordering explanations are valuable. The issue
is the mixture of those contracts with repeated review IDs, process notes,
“re-verify mechanically” instructions, exact grep recipes, all-caps emphasis,
and repeated restatements of the same decision. The stale wrap comments above
show the cost: large explanatory surfaces can drift while still looking
authoritative.

**Recommended improvement:** keep one concise normative contract next to each
unsafe boundary, especially the `StackStorage` implementor obligations and
the push/pop memory-order proof. Move audit archaeology, review history,
mechanical grep recipes, and task references to the review/ADR documentation.
Then make tests and CI comments describe behavior rather than the history of
how it was discovered.

## Correctness and safety assessment

The following parts look internally coherent on static inspection:

- `TaggedIndex<INDEX_BITS>` restricts the legal width to `1..=16`; the index
  mask, tag width, and tag maximum fit the packed `u64` representation.
- `push_index_impl` reads the head without following it, derives the link from
  that observed head, writes the link before the publishing CAS, and checks
  the terminal tag before any new publication-side effect. CAS failure is
  `Relaxed`, which is appropriate for push because the failed value is used as
  a value only.
- `pop_index_impl` performs an initial `Acquire` head load, reads the link
  before its CAS, validates the link domain and self-link condition, and uses
  `Acquire` on its successful head CAS. The documented release-sequence
  reasoning is consistent with the link publication ordering; changing these
  orderings casually would reopen the proof.
- The H-2 behavior is preserved: removing the last item returns the empty
  index with the running tag rather than resetting the tag.
- `StackStorage` and its hook methods place the custom-storage obligations at
  an explicit unsafe boundary. `ArrayIndexStack` keeps its head and links
  fused and no longer exposes the earlier competing standalone
  `StackStorage` binding route.
- `ArrayLinks` uses dense atomic link cells and correctly warns that this may
  create false sharing. That is a useful trade-off disclosure, not a generic
  optimization defect.
- `TagExhausted` is represented and documented as a permanent push seal while
  allowing pops to drain the existing chain. The advisory
  `pushes_remaining` count is appropriately not treated as synchronization.

Static scans found no raw-pointer ownership scheme, FFI, `transmute`, manual
`Send`/`Sync`, async, cryptographic, or similar mechanism that would create an
additional review class in this crate. This is a statement about inspected
source, not an execution result.

## Performance opportunities

No safe, evidence-free ordering relaxation should be landed from this review.
The current Release/Acquire link publication is the conservative choice on
weakly ordered targets. The repository's own A/B documentation identifies
these as measurement candidates:

1. `ArrayLinks::load_next` / `store_next` could be investigated with
   `Relaxed` link accesses on AArch64, but only after the codegen wrapper is
   repaired and native arm64 wall-clock evidence plus the concurrency proof
   remain intact.
2. The successful `pop` CAS could be investigated for `Relaxed` if the
   release-sequence proof is preserved. This is a proof-sensitive change, not
   a mechanical speedup.
3. Strong CAS versus weak CAS is a reasonable codegen/perf experiment, but
   the current evidence says generated code is identical for the inspected
   toolchain; there is no static reason to change it.
4. The per-call exponential backoff cap of 6 is an explicit contention
   trade-off. Nothing in this source-only pass justifies changing it.
5. False sharing in dense `ArrayLinks` is workload- and layout-dependent.
   Optional padded storage could help a specialized workload but would cost
   memory and should not replace the compact default without measurements.

The main actionable performance improvement at this point is repairing the
measurement gate itself. Until it can instantiate the current unsafe/fallible
API, new ordering or CAS conclusions are not reproducibly auditable.

## Review of changes since run 9

- `76d37f3` (`tis-sol9-GroupA`) narrows `store_next_for_test` to `cfg(loom)`.
  This correctly removes the normal `test-internals` safe write hook that was
  a potential public chain-corruption path.
- `bfe7084` (`tis-sol9-GroupC`) changes CI to run the complete
  `test-internals` feature set instead of a hand-maintained test-file list.
  This closes the earlier omission of `tag_seal.rs` and reduces future list
  drift.
- `6dc0ebe` (`tis-sol9-GroupB`) handles `TagExhausted` in the wall-clock A/B
  harness and makes `unused_must_use` deny-level in its scratch build. It does
  not update the separate codegen wrapper, which is the P2 finding above.
- `ee4501d` (`tis-sol9-GroupD`) updates current production descriptions to the
  strictly monotonic/non-wrapping seal contract. The remaining stale prose is
  in test/CI commentary rather than the main public algorithm documentation.

## Publication checklist

Before final release sign-off:

1. Fix `codegen_wrapper.rs.tmpl`'s unsafe/fallible `push` call and make the
   regular build-check compile that wrapper or otherwise cover both A/B
   templates.
2. Reconcile the wrap/seal narrative in `tests/stack_unit.rs` and the stale
   CI references.
3. Decide whether packaged `test-internals` execution is required; if yes,
   add it to the extracted-package gate, and if no, state the exclusion.
4. Reduce reviewer/process prose around the core contracts.
5. Only then obtain the prohibited-by-scope runtime/build evidence in the
   normal release pipeline; this review intentionally did not run it.

**Bottom line:** the runtime design looks materially improved and the prior
normal-feature corruption risk is closed, but the current repository is not
ready for an unconditional publication GO because its arm64 codegen gate still
cannot compile against the current public API.
