# tagged-index-stack — pre-release static review

**Reviewer:** Sol-codex
**Run:** Codex run 11
**User marker:** 2026-09-03 09:58
**Review captured:** 2026-09-03 09:59:04 +02:00
**Revision reviewed:** `ad742cd56c79986e843372a860f9ddf02e66e20a`
**Scope:** full static review of the current crate and all changes after run 10

## Verdict

**GO for publication from the static code/soundness perspective.** I found no
P1/P2 defect, no new release blocker, and no reason to delay publication on
the production implementation inspected at this revision. The P2 blocker from
run 10 is fixed: the codegen wrapper now matches the unsafe, fallible `push`
API and the ordinary build-check covers that independent template. The
packaged-artifact feature gap and stale production-wrap narrative are also
closed.

Four P3 cleanup/hardening groups remain. They concern stale prose and names,
one incorrect CI path, warning-policy robustness for a build-only wrapper, and
continued documentation density. None demonstrates a runtime correctness or
soundness defect. They are worth fixing, but they do not reverse the GO.

This is deliberately a bounded single-context static review. Per the requested
mode I did **not** run `cargo`, `rustc`, Node scripts, tests, clippy, rustdoc,
loom, Miri, benchmarks, examples, package/publish commands, or generated
binaries. The verdict therefore does not substitute for the normal release
pipeline's build and runtime evidence.

## Findings

### P3-1 — consolidated compile-fail helper still hard-codes the old count

**Locations:**
`crates/tagged-index-stack/tests/common/compile_fail.rs:2,11,121-122` and
`crates/tagged-index-stack/tests/compile_fail.rs:108-493`.

The helper says “all seven tests”, “all seven fixtures”, and “the seven
pre-consolidation drivers”. The consolidated driver currently contains eight
`#[test]` functions and `tests/compile_fail/` contains eight fixture
directories. This is factual drift in code-adjacent documentation. It also
repeats a class the manifest already avoids at `Cargo.toml:17-19`, where it
explicitly refuses to quote a fixture count because the count changes.

**Recommendation:** replace all three fixed counts with “all consolidated
tests/fixtures” or another count-free phrase. If a numeric count is useful in a
gate, derive it mechanically rather than duplicating it in prose.

### P3-2 — CI comment names a compile-fail driver that no longer exists

**Location:** `.github/workflows/ci.yml:2104`.

The no-atomics error-shape explanation cites
`tests/compile_fail_loom_cfg_without_feature.rs`. That standalone driver does
not exist. The current path is the consolidated
`crates/tagged-index-stack/tests/compile_fail.rs`, with the fixture under
`tests/compile_fail/loom_cfg_without_feature/`.

This does not alter what the CI step executes, but it sends maintainers to a
dead path while explaining a subtle cfg/error-shape contract.

**Recommendation:** cite the consolidated driver and fixture directory by
their current paths.

### P3-3 — direct-rustc wrapper check does not enforce warning cleanliness

**Locations:**

- `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs:910-914`;
- `crates/tagged-index-stack/scripts/tis_p3_ab/codegen_wrapper.rs.tmpl:23-50`.

The new direct-`rustc` metadata check correctly catches API/type drift in the
codegen wrapper, but it does not pass `-D warnings`. The wrapper denies
`unsafe_code` and now explicitly handles `push` and `pop`, so there is no
current API error visible by inspection. Still, a future warning-only drift can
remain green in this gate even though the crate's normal policy is strict.

**Recommendation:** add `-D warnings` to the wrapper's direct `rustc`
build-check, or put an equivalent deny policy in the wrapper. This is
defense-in-depth against future template drift, not a current compile error.

### P3-4 — historical “wrap” naming and reviewer-facing prose remain noisy

**Locations:**
`crates/tagged-index-stack/tests/stack_unit.rs:302-344`, especially the test
name at line 310; `crates/tagged-index-stack/src/lib.rs:256-355`; and the long
contract/history sections in `src/imp.rs`.

Run 10's stale behavioral claims were corrected: the text now consistently
says production `push` seals at `TAG_MAX` and never wraps. However, the test is
still named `empty_word_with_running_tag_reads_empty_across_wrap`, and its
comment has to explain that the name predates the seal fix. Search output and
test filters therefore still advertise behavior that no longer exists.

The run-10 deduplication of the unsafe inventory is a real improvement, and
the remaining memory-ordering and unsafe contracts contain useful proof
material. The crate nevertheless continues to mix normative contracts with
review archaeology, exact counts, audit instructions, historical filenames,
and repeated defensive explanations. P3-1 and P3-2 are concrete examples of
the resulting maintenance cost: apparently authoritative counts and paths
have already drifted.

**Recommendation:** rename the test around the actual boundary, for example
`empty_word_with_running_tag_reads_empty_through_tag_max`, and continue moving
historical review/process material out of source and CI comments. Keep the
normative safety contract, ordering proof, panic behavior, and public examples
next to the code; keep audit history and mechanical inventories in review/ADR
documents.

## Review of changes after run 10

The reviewed range is `fb5614f..ad742cd`: five files, 167 insertions and 117
deletions. No production algorithm or public runtime behavior changed.

### `9a59578` — codegen wrapper/API drift

This correctly closes run 10's P2 blocker.

- `push` is now typed as
  `unsafe fn(&ArrayIndexStack<16, 256>, u32) -> Result<(), TagExhausted>`.
- The unsafe call is localized, documented with both domain and liveness
  arguments, and its `Result` is handled.
- `pop` is explicitly discarded.
- `--mode build-check` now materializes and type-checks the separate codegen
  wrapper, closing the regular-CI blind spot.

The only residual is P3-3's warning-policy hardening; it does not reopen the
type-contract blocker.

### `3ddcca6` — packaged `test-internals` coverage

This correctly closes the packaged-artifact gap. The extracted `.crate` now
runs both the default test surface and `cargo test --features test-internals`
before the packaged bench build. The check is performed against the artifact,
not merely against the workspace source tree.

### `c9bbcca` — seal/wrap narrative

The current behavioral narrative is now accurate: `push` refuses at
`TAG_MAX`, never computes the post-ceiling tag in production, and the checked
`pack` rejection test is described as an isolated representation boundary.
The historical filename/test-name residue is only P3-4.

### `ad742cd` — unsafe inventory deduplication

This materially reduces duplication without weakening the crate-level lint
boundary. A static inventory still agrees with the documented layout: eight
item-scoped `#[allow(unsafe_code)]` regions in `src/imp.rs`, one unsafe trait,
ten unsafe functions, no unsafe impl in production source, and six unsafe
blocks. `#![deny(unsafe_op_in_unsafe_fn)]` continues to require local unsafe
blocks inside unsafe functions.

## Correctness and soundness assessment

The production implementation remains internally coherent on static
inspection:

- `TaggedIndex<INDEX_BITS>` restricts widths to `1..=16`; checked packing
  rejects over-wide index/tag halves, while the private truncating helper is
  reached only after bounds are established.
- `push_index_impl` validates the index, checks `TAG_MAX` before changing a
  link or publishing a head, writes the new link before the Release CAS, and
  uses the failed-CAS value only as a value under `Relaxed` failure ordering.
- `pop_index_impl` starts from an Acquire head load, reads and validates the
  next link before publication, preserves the running tag on the H-2 empty
  transition, and uses Acquire on both success and failure of the head CAS.
- Every head write is a CAS/RMW. The documented release-sequence proof is
  therefore consistent with the current code; a plain store has not been
  introduced into the head path.
- `StackStorage` is an unsafe trait with explicit binding, backing identity,
  disjoint-population, domain, atomic-link, and hook-call obligations.
  `StackOps` owns the algorithm through a blanket implementation, preventing a
  downstream implementor from silently replacing the protocol.
- `ArrayIndexStack` fuses head and links and intentionally does not implement
  the public `StackStorage`, closing the old competing-backing construction.
- The normal `test-internals` surface is read-only; the mutating link hook is
  restricted to the loom cfg.
- `TagExhausted` is a permanent push seal while pops remain able to drain the
  current chain. `pushes_remaining` is correctly documented as advisory, not
  synchronization.

Static scans found no production raw-pointer ownership, FFI, `transmute`,
manual `Send`/`Sync`, async state machine, cryptographic primitive, or resource
drop protocol requiring an additional mechanism-specific finding.

## Performance assessment

No production performance code changed after run 10, and this read-only pass
produced no measurement that would justify changing atomics or backoff.

Three real candidates are already tracked in `docs/perf/OPEN_ITEMS.md` items
61-63:

1. Avoid reissuing `store_next` after a failed push CAS when the observed head
   index — and therefore the link value — is unchanged. This trades a Release
   store for an extra branch/comparison and is not statically guaranteed to
   win.
2. Measure Relaxed link-cell loads/stores. Existing evidence says x86-64
   codegen is identical, while AArch64 removes real `ldar`/`stlr`
   instructions; native wall-clock evidence is still pending.
3. Measure a Relaxed successful `pop` CAS. The on-paper release-sequence
   argument is plausible, but it remains a proof-sensitive, unmeasured change.

Strong versus weak CAS is currently a measured codegen null in the repository,
and changing the per-call backoff cap of 6 or the dense `ArrayLinks` layout
without workload evidence would be guesswork. Dense links can false-share, but
padding costs memory and slot-resident custom storage already gives specialized
consumers a layout escape hatch.

**Recommendation:** do not weaken atomics or alter retry/backoff behavior as a
cleanup patch. Run the existing native ARM64 A/B gate first, add the third
ordering variant before that run if it is still being considered, then land
only a measured win with the concurrency proof and loom coverage updated.

## Publication conclusion

The previous blocking wrapper mismatch is closed, the production protocol has
no newly discovered soundness/correctness defect, and the current public
unsafe boundary is explicit and defensible. The crate is **GO** on this static
review. P3-1 through P3-4 should be treated as cleanup/hardening work, not as
reasons to claim the runtime implementation is unpublishable.
