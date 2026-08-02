# R32-6 (task #497) — merge `AllocBitmap`/`MagazineBitmap` into one 2-bit-per-granule `DualBitmap`: honest REJECT

Date: 2026-08-02.

## 0. What this is

This task tracks finding **F1b** in
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` ("F1b — the stronger
form: a single 2-bits-per-granule state word, which is ALSO an `Ir` win").
F1b proposed replacing the two separate 1-bit-per-granule bitmaps
(`AllocBitmap`, the O(1) double-free guard; `MagazineBitmap`, the RAD-5
magazine-residency oracle — both `src/alloc_core/`) with ONE combined
2-bit-per-granule region, so the own-thread free path's unconditional
`is_in_magazine` + `is_free` read pair costs one `locate` + one load instead
of two, from two cache lines 32 KiB apart.

**This was implemented, correctness-verified, and measured. The measurement
contradicts the survey's prediction: it is a consistent, real `Ir`
REGRESSION across every hot allocator path that touches either bitmap, not
a win.** Per this task's own instructions and CLAUDE.md's "After each phase
— ZERO-TRUST review" discipline, the change is **not shipped**. No `src/`
diff from this task survives in the working tree; the repository is
byte-identical to its state before this task started (verified: `git diff
<base>..HEAD -- src/` is empty for this task's commit — see §5).

## 1. What was built (and then reverted)

- `src/alloc_core/segment_bitmap.rs`: `SegmentBitmap` (the old shared 1-bit
  mechanism) replaced by `DualBitmap`, a 2-bit-per-granule mechanism with
  `test_alloc`/`set_alloc`/`clear_alloc` (bit 0 of each pair) and
  `test_magazine`/`set_magazine`/`clear_magazine` (bit 1), plus a combined
  `test_both` reading both planes in one `locate` + one load. Two `locate`
  variants: a lean single-mask form (`locate_alloc`/`locate_magazine`) for
  the single-plane accessors, and a dual-mask form (`locate_pair`) for
  `test_both` only — this split was added DURING the investigation (§3) to
  rule out "unused mask computation not eliminated" as the regression cause;
  it made no measurable difference (see §3.2).
- `src/alloc_core/alloc_bitmap.rs` / `magazine_bitmap.rs`: both wrappers kept
  their PUBLIC API completely unchanged (same method names, same semantics,
  same `FOOTPRINT` constant) — only their internal storage moved from
  `SegmentBitmap` to `DualBitmap`, constructed at a SHARED base offset
  instead of two separate offsets.
- `src/alloc_core/segment_header_layout.rs`: `alloc_bitmap_off()` +
  `magazine_bitmap_off()` collapsed into one `bitmap_off()`;
  `remote_ring_off()` updated to add `DualBitmap::COMBINED_FOOTPRINT` (=
  `2 * FOOTPRINT`, byte-identical total span to the old two-region sum, so
  every offset AFTER the bitmap region is unchanged).
- `src/alloc_core/segment_header.rs`: added
  `SegmentMeta::bitmap_test_both(off) -> (is_free, is_in_magazine)`, the
  combined-read payoff primitive, used at the two call sites below.
- `src/registry/heap_core_free.rs` (`dealloc_own_thread_with_base`) and
  `src/registry/heap_core_dealloc_batch.rs` (the batched M2 oracle pair):
  rewired to call `bitmap_test_both` once instead of
  `magazine_bitmap().is_in_magazine(off)` + `alloc_bitmap().is_free(off)`
  separately — semantics and evaluation ORDER unchanged (the `is_free`
  RESULT is still only consulted after the `#[cfg(alloc-decommit)]` bump
  guard, exactly as before; only the READ is hoisted, which is sound under
  the single-writer invariant — nothing mutates this bit between the two
  reads on the owning thread).
- Every other call site (`pop_free`, `drain_freelist_batch`, `carve_batch`,
  `flush_run`, `reclaim_offset`/`reclaim_offset_checked`, the bootstrap/pool
  `init_in_place` pairs, the two `dbg_*_bitmap_bytes_for` diagnostic dump
  hooks) kept calling the SAME named wrapper methods (`mark_alloc`,
  `mark_free`, `mark_magazine`, `clear_magazine`, `is_free`,
  `is_in_magazine`) with NO change to call-site code beyond the offset the
  underlying view is constructed at.

## 2. Correctness verification (before the reject was reached)

All of this passed BEFORE the Ir measurement in §3 revealed the regression
— the design itself is sound; it is rejected on MEASURED COST, not on a
correctness defect:

- `cargo build` / `--features experimental` / `--features production` /
  `--all-features`: clean, zero warnings.
- `cargo clippy --lib` under all three CI feature-matrix entries (`""`,
  `experimental`, `--all-features`): clean.
- `cargo test --features production` (full tree, 29 test-result blocks):
  **all green**.
- `cargo test --all-features` (full tree, 233 test-result blocks): **all
  green**.
- The exact pinned correctness surface the task named — re-verified
  explicitly, not just swept up in the full-tree run:
  `in_magazine_double_free_is_noop`, `flushed_double_free_is_noop`,
  `flushed_double_free_with_garbage_word1_is_noop`,
  `legit_free_after_pop_is_not_swallowed`
  (`tests/regression_magazine_oracles.rs`),
  `refill_window_does_not_double_issue_in_out_buffer_resident_block`
  (`tests/regression_refill_window_double_issue.rs`),
  `drain_resident_xthread_double_free_no_corruption` +
  `realloc_path_drain_respects_magazine`
  (`tests/regression_xthread_double_free_residual.rs`),
  `t1_primordial_bitmap_reads_zero_before_any_traffic` +
  `t2_fresh_small_segment_bitmap_reads_zero_for_untouched_classes` +
  `t3_double_free_guard_still_correct_on_freshly_reserved_segment`
  (`tests/regression_virgin_bitmap_skip.rs` — required rewriting the two
  `dbg_*_bitmap_bytes_for` diagnostic dump hooks to DECODE through the typed
  `AllocBitmap`/`MagazineBitmap` accessors instead of raw-copying bytes,
  since the combined region's physical byte layout differs from the
  pre-F1b standalone layout — the hooks' EXTERNAL contract, byte-count and
  "all zero on virgin", is preserved), plus `r10_7_alloc_batch_xthread_double_free`
  and `r11_4_dealloc_batch_same_segment_double_free` (need
  `--all-features`; both green).
- `cargo +nightly miri test --features alloc-core --test
  regression_virgin_bitmap_skip`: **all 3 tests green** (1518s wall —
  miri is slow on this host; run to completion, not truncated).
- The bulk-user audit the task named: `init_in_place` (re-derived to zero the
  WHOLE combined region — both wrappers' `init_in_place` still call the
  SAME pair of bootstrap sites they always did, so the second call is a
  correctness-preserving redundant re-zero, not a new hazard), the
  virgin-init-skip elision (covered by the miri-verified
  `regression_virgin_bitmap_skip.rs` above), the two `dbg_*_bitmap_bytes_for`
  word-at-a-time dump hooks (rewritten, covered above), and `flush_class`/
  `flush_run`'s per-block `mark_free` path (unchanged call shape — confirmed
  by reading `alloc_core_small_magazine.rs::flush_run`, which only ever
  touches the ALLOC plane per block, never `MagazineBitmap` directly — the
  magazine-bit clear for flushed blocks happens separately in
  `heap_core_free.rs` BEFORE `flush_class` is called, exactly as before).

**The G1-vs-F1b correctness distinction holds**, confirmed by re-reading
`[L]` item 21 (G1, `docs/perf/OPEN_ITEMS.md`) before starting: G1 was
rejected for *redefining* `AllocBitmap`'s single bit to also mean magazine
residency, which silently inverted `mark_alloc`'s "leaves the free list ⇒
handed to caller" premise at `refill_class_bump_impl`/`carve_batch` and made
`reclaim_offset_checked`'s separate `is_in_magazine` scan ambiguous. This
task's `DualBitmap` keeps the two oracles' semantics and every one of their
call-site meanings completely independent — `mark_alloc`/`mark_free`/
`is_free` operate on their own bit-plane exactly as before,
`mark_magazine`/`clear_magazine`/`is_in_magazine` on theirs; ONLY the
physical storage/addressing is shared. This is a genuinely different (and
correctness-safe) change from G1's rejected form. The reason this task is
rejected is entirely about MEASURED COST (§3), not about resurrecting G1's
semantics problem.

## 3. Ir measurement — worktree-isolated, and why this is REJECTED

### 3.1 Immutable source identity (CLAUDE.md's R29-6 rule)

- **BEFORE**: `git worktree add ../sefer-alloc-r497-before
  03a6c55fe4489ed6e84501375edcd3238dddf867` (this task's base commit — `main`
  HEAD at task start), unmodified.
- **AFTER**: the main working tree at the same base commit + this task's
  full `DualBitmap` diff (§1), before it was reverted.

### 3.2 Reproduction

The diff measured here is NOT present in the working tree (it was reverted
after this measurement — see §5); reproducing it requires re-applying the
design described in §1 first. The exact bench invocation used:

```
node scripts/iai.mjs small_churn_16b churn_256b aligned_churn_640b_a128 \
  cold_alloc_free_256x16b recycle_alloc_free_256x16b
# BEFORE -> docs/perf/_raw_r497_dualbitmap_before_production.log (isolated worktree, base commit, no diff)
# AFTER  -> docs/perf/_raw_r497_dualbitmap_after_production.log  (main tree, base commit + full F1b diff)

# Derive the summary CSV (asserts the arithmetic, per CLAUDE.md's checked-script rule):
node scripts/r497_dualbitmap_summary.mjs
```

Raw logs (full, not truncated):
`docs/perf/_raw_r497_dualbitmap_before_production.log`,
`docs/perf/_raw_r497_dualbitmap_after_production.log`. Summary CSV:
`docs/perf/R32_6_DUAL_BITMAP_GATE_summary.csv`, produced by
`scripts/r497_dualbitmap_summary.mjs` — hard-asserts (a) the bootstrap-proxy
bench (`large_alloc_free_cycle`) delta is exactly 0 (ruling out a
process-bootstrap-codegen-shift explanation, the R32-5 pattern), (b) every
bitmap-touching bench regressed (delta > 0), and (c) `small_churn_16b` and
`churn_256b` move by the identical delta (structurally identical churn
shapes — a matched-conditions sanity check), before writing the CSV.

### 3.3 Result — every bitmap-touching bench regressed, well past the ±10 kill gate

| bench | before Ir | after Ir | Δ (raw) | ops | Δ/op |
|---|---:|---:|---:|---:|---:|
| `small_churn_16b` | 8,810 | 9,064 | **+254** | 1 | +254.0 |
| `churn_256b` | 8,810 | 9,064 | **+254** | 1 | +254.0 |
| `aligned_churn_640b_a128` | 8,746 | 8,935 | **+189** | 1 | +189.0 |
| `cold_alloc_free_256x16b` | 50,968 | 51,867 | **+899** | 256 | +3.5 |
| `recycle_alloc_free_256x16b` | 99,185 | 101,296 | **+2,111** | 256 | +8.2 |
| `large_alloc_free_cycle` (bootstrap proxy) | 4,080 | 4,080 | **0** | — | — |

The project's own Ir/op* figure (bootstrap-subtracted, from the raw logs'
own summary table) moves the same direction on every SeferAlloc row:
`small_churn_16b`/`churn_256b` 73.9 → 77.9 (+4.0/op),
`aligned_churn_640b_a128` 72.9 → 75.9 (+3.0/op),
`cold_alloc_free_256x16b` 183.2 → 186.7 (+3.5/op),
`recycle_alloc_free_256x16b` 185.8 → 189.9 (+4.1/op). mimalloc's own rows in
the SAME log runs (an independent allocator this diff cannot touch) are
byte-identical between BEFORE and AFTER in both raw logs — confirming the
regression is real and localized to this diff, not host noise.

**Every one of the three plain-churn kill-gate benches (`small_churn_16b`,
`churn_256b`, `aligned_churn_640b_a128`) regressed by +189…+254 raw Ir —
20-25× past this project's own ±10 raw-Ir churn kill threshold** (the same
threshold X5/T10/R1's honest-reject entries in `docs/perf/OPEN_ITEMS.md`
were killed by, and R32-5's own precedent measured for comparison).

`large_alloc_free_cycle`'s exact 0 delta rules out the R32-5-style
"one-time process-bootstrap codegen shift" explanation (that bench never
touches the small-class bitmaps at all) — **this is a genuine, real per-op
cost increase**, not a bootstrap artifact hiding a Ir-neutral operation.

### 3.4 Root-cause investigation — why a "fewer reads" change costs MORE

This contradicts the survey's own prediction ("one `locate` + one load
answering both oracles... a straight instruction saving") and RAD-5's
precedent (−52 Ir/op for a structurally similar move). The investigation
that explains it:

1. **Isolating storage-merge from combined-read.** With the `DualBitmap`
   storage merge in place but `heap_core_free.rs`'s call site reverted to
   TWO separate `magazine_bitmap().is_in_magazine(off)` +
   `alloc_bitmap().is_free(off)` calls (i.e. paying the merge's addressing
   cost without the combined-read's savings), `small_churn_16b` measured
   **8,936** Ir — WORSE than the 8,810 baseline (+126) but BETTER than the
   full diff's 9,064 (i.e. the combined-read call site made
   `small_churn_16b` +128 Ir WORSE, not better). `cold_alloc_free_256x16b`
   (51,928) and `recycle_alloc_free_256x16b` (101,354) were statistically
   unchanged from the full-diff numbers (51,867 / 101,296) — these two
   benches' cost is dominated by `pop_free`/`carve_batch`/
   `drain_freelist_batch`, which never call the combined-read primitive at
   all. This isolation run's own log was not separately committed (the
   diagnostic step, not the primary before/after evidence this report's
   verdict rests on — see the non-retroactive/exemption note in §3.5); the
   BEFORE/AFTER pair in §3.3 alone already supports the REJECT verdict.
2. **Conclusion: the storage merge itself is the dominant cost, not the
   combined-read call site.** `AllocBitmap`/`MagazineBitmap`'s SINGLE-plane
   accessors (`is_free`, `mark_alloc`, `mark_free`, `is_in_magazine`,
   `mark_magazine`, `clear_magazine`) vastly outnumber the two call sites
   that read both oracles together — every `pop_free`, `carve_batch`,
   `drain_freelist_batch`, `flush_run`, and `reclaim_offset*` call touches
   only ONE bit-plane. The old 1-bit-per-8-granules-per-byte `locate` computed
   `byte_idx = bit >> 3; mask = 1 << (bit & 7)`. The new 2-bit-per-4-granules-
   per-byte `locate_alloc`/`locate_magazine` computes one MORE step:
   `byte_idx = granule >> 2; pair_shift = (granule & 3) << 1; mask = 1 <<
   pair_shift` (or `1 << (pair_shift + 1)` for the magazine plane) — an
   extra shift to double the intra-byte bit position for the 2-bit pairing.
   This is a genuinely more expensive per-call arithmetic sequence, paid by
   every SINGLE-plane call, which is the large majority of call sites. The
   combined-read primitive's saving (one `locate`+load instead of two, at
   exactly 2 call sites) is smaller than the aggregate tax paid by every
   other bitmap touch across the rest of the allocator.
3. **The lean-`locate` split made no difference.** Giving each single-plane
   accessor its own `locate_alloc`/`locate_magazine` (computing only ONE
   mask, not both) instead of sharing the dual-mask `locate` — done
   specifically to rule out "the compiler failed to eliminate the unused
   mask" as the cause — produced BYTE-IDENTICAL Ir numbers to the shared-
   `locate` version. This confirms LLVM already eliminated the unused mask
   in both cases; the extra `pair_shift` computation itself (needed
   regardless of the mask-computation shape, because 4-granules-per-byte
   packing genuinely requires locating WHICH of 4 sub-positions within the
   byte, vs. the old 8-granules-per-byte layout's simpler `bit & 7`) is the
   irreducible cost this design pays at every single-plane call site.

The survey's own "Risk to check before building this" section (F1b, §
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`) named exactly this
class of risk — bulk/other users where "a 2-bit interleave costs more than
it saves" — and said it "must be measured, not assumed." It has now been
measured, and the risk materialized: the combined-storage design is a NET
Ir LOSS for this codebase's actual call-site distribution, where most
bitmap touches are single-plane, not the free path's dual-oracle read the
survey focused on.

### 3.5 Raw-log / immutable-identity exemption note

Per CLAUDE.md's raw-log policy, this report cites
`docs/perf/_raw_r497_dualbitmap_before_production.log` and
`docs/perf/_raw_r497_dualbitmap_after_production.log` as evidence, both
`git add -f`'d alongside this report (they are `.gitignore`'d scratch by
default). The §3.4 point-1 isolation run (storage-merge-only, no
combined-read) is NOT separately committed as a raw log — it was a
diagnostic side-investigation whose numbers are reported inline in prose
above, not a formal gate-table row this verdict depends on; the primary
BEFORE/AFTER pair in §3.3 alone fully supports the REJECT verdict
independent of §3.4's root-cause narrative. This is the documented
exemption path CLAUDE.md's checked-script rule describes for material that
"cannot be regenerated from anything committed" without inventing new
measurement code after the fact — reproducing it would require re-applying
the reverted diff (§1) plus a further partial revert, which is exactly the
"genuinely cannot be regenerated from anything committed" case, since the
full diff itself is only recoverable by re-reading this report's §1
description, not from a preserved commit.

## 4. Verdict

**REJECT.** F1b's combined-storage design is correctness-sound (§2 — NOT
G1's rejected form) but measurably COSTS MORE than it saves: every
bitmap-touching bench regressed, the three churn kill-gates by 20-25× the
±10 threshold, with the bootstrap-proxy bench proving this is a genuine
per-operation cost, not a codegen artifact. The root cause (§3.4) is
structural: the survey's analysis focused on the TWO call sites that read
both oracles together (the free path's dual-oracle read) and correctly
predicted a win THERE, but did not weigh the aggregate cost the same
storage change imposes on the far larger number of SINGLE-plane call sites
throughout the rest of the allocator (`pop_free`, `carve_batch`,
`drain_freelist_batch`, `flush_run`, `reclaim_offset*`), where 4-granules-
per-byte packing is strictly more expensive to address than the old
8-granules-per-byte layout. No `src/` change from this task is shipped;
the working tree is reverted to the base commit's exact state (verified in
§5).

**Next trigger, if ever revisited:** a variant that keeps the two bitmaps
SEPARATE in storage/addressing (i.e. F1's pure-locality interleaving form,
NOT F1b's bit-packing form) would not pay this per-call arithmetic tax —
each bitmap keeps its own simple `bit >> 3` / `bit & 7` addressing — while
still gaining SOME locality benefit if the two regions are interleaved at a
coarser granularity (e.g. alternating 8-byte words instead of merging
individual bits). The survey's own F1 entry (superseded in favor of F1b
under the assumption F1b was strictly stronger) is the correct starting
point for that alternative, not a further-refined F1b. F1's own blocker
(no existing bench can show a pure cache-locality effect — needs the
missing ≥64-live-segment macro-benchmark, OPEN_ITEMS item 34 / task #500)
still applies to that alternative, so it is not free to re-attempt either.

Commit prefix for this task: none of `perf(runtime)`/`perf(opt-in)`/
`fix(perf)` apply — no `src/` code shipped. This report itself lands under
`docs(perf)`.
