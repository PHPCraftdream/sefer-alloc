# Correctness / CI-debt open items -- [T] Tracked tier -- residual -- does not fit any other category

**Part of the split index.** This file holds the full text of every **[T]**
(tracked, not yet actioned) card whose subject matches this file's own
criterion (below). Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for the
purpose/scope/convention header and the round-start reading order, and for
the complete item-number to file lookup table; come here for these specific
card bodies. See `docs/correctness-open-items/ACTIVE.md` for the **[A]**
tier, `docs/correctness-open-items/RESOLVED.md` for the closure trail, and
the sibling `[T]`-tier files (`TRACKED_hook_safety.md`, `TRACKED_verification_coverage.md`, `TRACKED_platform_contracts.md`, `TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`, `TRACKED_correctness_residuals.md`, `TRACKED_publish_readiness.md`, `TRACKED_process_record.md`) for the rest of
the tier.

**Criterion for this file:** A card lands here only if it does not share the defining criterion of any category above. Each card here is a genuine one-off: item 45 is a numa-shim RefCell-vs-Cell defensive-coding/panic-safety nit (not an OS-contract question, not a hook, not flakiness); item 49 is an aligned-vmem edition-2021-vs-2024 explicit-unsafe{}-block hygiene item (about FFI call-site annotation style, not about a dbg_* hook, a platform contract, or CI wiring).

**Card count (re-derived 2026-10-08, src-review round 16):** 8 numbered cards in this file — 6 open [T] cards (items 154, 158, 159, 160, 161, 176) + 2 CLOSED/resolved pointers retained for lookup (items 45 and 49; full closure records are in `RESOLVED.md` and `ARCHIVE.md`). Item 157 was superseded by the current terminal-sidecar architecture; see `RESOLVED.md`.

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every citation of this index that points at ONE SPECIFIC ITEM carries
that item's number, in the form `` `docs/CORRECTNESS_OPEN_ITEMS.md`
item N `` -- task #1227 repaired the seven in `aligned-vmem` that did
not, and two outside it were still open as of that task (both are
recorded in the thin index's Structure section). Citations that point
at the FILE as a whole, at a named SECTION, or at a CLASS of items
rather than one item carry no item number and never needed one (task
#1227's finding; until #1236 these headers overclaimed it as a
universal, asserting that no citation ever pointed at anything but
an item number). Only the numbered citations depend on where item
numbers live, and `docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index)
carries the complete, mechanically generated item-N -> file lookup
table covering EVERY `[T]`-tier number (including the `59a`/`59b`
sub-items) that keeps them resolving -- that table, not this file's
name, is what makes the thematic split safe: the lookup is two-hop
(index table, then this file), but mechanical and always correct. No citing-file
count is typed in this header on purpose: the "42+" typed here at
the split was already 43 (census against the split commit) -- #1230
removed it from one of these nine headers, #1236 from the other
eight; compare against this command's output, never a hardcoded
count:

```text
git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l
```

(Split 2026-08-20, task #1222, superseding task #1221's number-range
split the same day.)

---
45. **CLOSED** by task #1342 (twentieth review F3, `docs/reviews/2026-08-25-021741-numa-shim-publication-audit-run-17-Sol-codex.md`; fired the item's own "fold into any future edit that touches the `mock` module's thread-locals" trigger). See 'Recently resolved' in RESOLVED.md for the full closure narrative.

49. **CLOSED** by task #997 (P3-8 pass 2). See 'Recently resolved' in RESOLVED.md for the full closure narrative.

154. **[T] Task-history prose in `src/` doc comments outweighs the code.** (Filed 2026-09-28, src review round 1 finding R1-11, residual part.)

    - **Status:** OPEN — broad task-history prose debt remains deferred. R18 corrected targeted source contracts/counter documentation and removed two dead surfaces, but did not perform the dedicated docs migration.
    - **Current-number-or-verdict:** no current source-wide prose/code census was run. The earlier ~20,000 comment lines / ~15,600 code lines are filing-time measurements, not current totals. R18 corrected specific allocator lifecycle, `INVARIANTS.md`, counter, module and inventory claims; the complete remaining prose debt was not inventoried.
    - **Next trigger:** a dedicated docs round that moves history into `docs/` (ADR-style) and leaves invariants and SAFETY reasoning in the code, one module per commit. Pure doc diffs, but several tests `include_str!` source files and pin phrases, so rerun the doc tripwires after each module.
    - **Evidence:** review §R1-11; R18's scoped changes and residuals are recorded in `docs/reviews/2026-10-08-src-review-xxs-round-17-P4.md` and `docs/reviews/2026-10-08-src-review-xxs-round-17-P5.md`.

    - **R15 evidence update (2026-10-07):** R15-03 confirms current examples of the existing prose debt: `sharded_region.rs:644–651` overstates binding/locking guarantees, and `registry/mod.rs:50–54` still labels live production routing unconnected. These do not satisfy the dedicated documentation-round trigger; status remains OPEN.

    - **R16 evidence update (2026-10-08):** further current examples, not a closure: hidden `dbg_stamp_*` docs still cite the removed `dbg_push_to_ring` as an `unsafe fn` "in this file" (`src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs:138`, `table_diag.rs:260`; strict rustdoc skips `#[doc(hidden)]` items); `registry/mod.rs:53` keeps `#[allow(dead_code)]` on the live production `segment_route` module; `SeferAlloc::trim_current_thread`'s doc still contrasts itself with a "legacy foreign `dealloc` ring/overflow/stack" (`src/global/sefer_alloc/diag.rs:252–254`); `dec_live_and_maybe_decommit` no longer decrements. Status and trigger unchanged. Evidence: `docs/reviews/2026-10-08-src-review-oxx-round-16.md` §4.

    - **R16 follow-up update (2026-10-08):** commit `c8b9344a` fixes the specific examples listed above and in the R15 update: the sharded binding docs, the `registry/mod.rs` module note, `SeferAlloc::trim_current_thread`, the batch/refill comments, the sidecar/directory notes and the two hidden `dbg_stamp_*` docs that cited the removed `dbg_push_to_ring` now describe the live terminal-sidecar protocol, and the blanket `#[allow(dead_code)]` on `registry::segment_route` is removed (it hid no warning in 6 clippy rows, 7 narrow `--lib` sets, `--all-features` and strict rustdoc with `--document-private-items`). The structural prose debt (task history outweighing code across `src/`) is untouched, so the status stays OPEN with the same trigger. Separate residual noted: the parent `allow(dead_code, unused_imports)` around `src/lib.rs:482` still masks dead code in builds without `internals`.

158. **[T] xa XA-18: under `numa-aware`, a node promoted from the unknown bucket to a dedicated bucket leaves its old directory bits in the unknown bucket.** (Filed 2026-09-28. Found by `docs/reviews/2026-09-21-200601-src-review-xa.md` XA-18 (P3), which no index recorded; re-flagged by `docs/reviews/2026-09-28-201530-src-review-fxx-round-2.md` §3.)

    - **Status:** OPEN — directory accuracy/perf residual, not memory corruption. Established by code reading in both reviews; no test has run it.
    - **Current-number-or-verdict:** not measured. With all eight dedicated NUMA buckets taken, a node's bits can land in the unknown bucket. After the node later gets a dedicated bucket, `sync_directory_for_segment_classes` clears an empty class only from the current bucket (`src/alloc_core/small/alloc_core_small/directory.rs:238–259`), so that path can leave the old unknown-bucket bit while the `(class, segment slot)` remains live. By contrast, `publish_empty` clears the addressed `(class_idx, slot_idx)` across all node buckets (`directory.rs:184–205`); allocation and stale-candidate validation call it (`alloc_core_small_impl.rs:430–431,625–626`; `find_segment.rs:727–736`). The remaining sync-only stale-bit path and scan cost are unmeasured. The `directory_diag.rs` APPEND-ONLY mapping comment is stale.
    - **Next trigger:** run a deterministic `numa-aware` witness that fills eight buckets, leaves a ninth node's bit in the unknown bucket, promotes that node, then exercises the current-bucket-only clear branch in `sync_directory_for_segment_classes` while the slot remains live. Check each bucket and `DIRECTORY_STALE_HITS` against a fresh rebuild; keep a separate all-bucket `publish_empty` control. Fix options: stable per-segment bucket ownership or clearing the old unknown bucket when identity is known.
    - **Evidence:** xa review §XA-18; fxx review §3.

    - **R15 evidence update (2026-10-07):** `publish_empty` clears all buckets for its addressed class/slot (`directory.rs:184–205`); the allocation and stale-candidate paths call it. The post-drain `sync_directory_for_segment_classes` empty branch still calls current-node-only `clear_bit` (`:238–259`). The original broad stale-bit/scan claim is therefore narrowed to the sync-only path; no `numa-aware` runtime witness was run, so the item remains OPEN under its revised trigger.

159. **[T] `pinning` constructors return `Some` with zero workers if `core_affinity::get_core_ids()` returns `Some(empty)`.** (Filed 2026-09-29, xxs round 5, unconfirmed risk.)

    - **Status:** OPEN — unconfirmed; no supported platform is known to return an empty list.
    - **Current-number-or-verdict:** `src/concurrent/pinning.rs` (~lines 122-141) builds a pool from the core-id list without checking it is non-empty, so an empty list would give a "working" pool with no workers. Nobody has reproduced it.
    - **Next trigger:** a platform or container (restricted cpuset) that reports an empty list. The fix is to return `None`, with a test through the seam that supplies the ids.
    - **Evidence:** `docs/reviews/2026-09-29-091221-src-review-xxs-sol-round-5.md` "Неподтверждённые риски".

160. **[T] `EpochRegion::remote_free_pending` is a Relaxed hint; a shadow Loom model exists, but actual-type coverage/refinement remains open.** (Filed 2026-09-29, xxs round 5, unconfirmed risk.)

    - **Status:** OPEN — two shadow models execute, but implementation-to-model refinement remains unproved; no production lost-index or liveness proof follows.
    - **Current-number-or-verdict:** `src/concurrent/epoch/epoch_region.rs` reads the Relaxed hint before the queue mutex, clears it under that mutex, and publishes a positive hint after enqueue releases the lock. Insertion forces queue inspection before reporting full when the local free list is empty and the hint-only drain skipped inspection. `tests/loom_r11_epoch_false_full.rs` is a handwritten `QueueProtocol` with Relaxed `pending`/`done` atomics and Loom Mutex/Vec state, not actual `EpochRegion`/`AtomicSlot` instrumentation. The four scenarios cover completed enqueue with a negative hint, the expected-panic hint-only control, overlapping enqueue/reuse, and benign false/spurious-true hints. `tests/loom_epoch.rs` is a separate generation/value shadow; R18 corrected its eviction order to generation CAS before value swap. R18 executed both targets: 2 epoch-shadow tests and 4 queue-shadow tests passed, including their expected-panic controls. Neither run refines the shadows to production types.
    - **Next trigger:** actual-type Loom instrumentation, or an explicit reviewed implementation-to-model refinement mapping including retirement/reuse and forced queue inspection. Keep any liveness claim separate; bounded shadow scenarios do not constitute a liveness proof.
    - **Evidence:** xxs round 5 review "Неподтверждённые риски"; `docs/reviews/2026-10-08-src-review-xxs-round-17-P4.md` R17-CON-03 and its R18 update; `tests/loom_r11_epoch_false_full.rs`; `tests/loom_epoch.rs`; `.github/workflows/ci.yml` selects the queue shadow and sentinels (wiring, not an execution receipt).

161. **[T] Warnings in feature combinations that no `-D warnings` row builds.** (Filed 2026-09-29, found by the R5-01 design consultation; reproduced on the pre-R5-01 tree too.)

    - **Status:** OPEN — cosmetic, not behavior.
    - **Current-number-or-verdict:** with `alloc-xthread` and no `fastbin` (for example `--features "alloc-global alloc-decommit internals" --lib`): `src/registry/heap_core/diag/queries.rs` lines 14 and 17 (unused imports `Layout`, `SegmentMeta`), `find_segment.rs` ~891 (`is_in_magazine` unused), and two needless `mut` at `alloc_core_large.rs` ~584 and `reserve.rs` ~181. `-D warnings` turns the first two into errors, but no per-PR row builds that combination.
    - **Next trigger:** the next edit of those files, or adding a per-PR clippy row for `alloc-global` without `fastbin` (which R5-01 made a normal, supported set).
    - **Evidence:** consultation notes in the xxs round 5 session; `cargo clippy --features "alloc-global alloc-decommit internals" --lib -- -D warnings`.
