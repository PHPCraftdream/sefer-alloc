# alloc_core perf/quality review — 2026-09-20

Scope: `src/alloc_core/` only (hot-path allocator core). Review-only static
analysis; no file under `src/` was modified. Method: full read of the small,
large, segment-table, remote-free-ring, segment-header, directory, bitmap,
pool/decommit, deferred-large and mem (alloc/dealloc/realloc) clusters,
plus targeted scans of the `alloc_core_core_diag` seams and the platform
(`os`/`node`/`sidecar`/`dirty_by_class`) confined-unsafe seams. Framed against
`CLAUDE.md`'s conventions and `docs/INVARIANTS.md`'s M1–M8. No benchmarks were
run; every perf claim below is either anchored to an existing
`docs/perf/*.md` report or explicitly labelled "expected to help, not
measured".

## Summary

No P0 (correctness/soundness) findings: on every path I traced, the
soundness arguments in the code hold — the single-writer metadata discipline,
the ring's F10 protocol, the UBFIX guard set (H-1/M-1/M-2/UBFIX-6/7/11), and
the `contains_base` cache-invalidation structure are all correct as written,
and every residual risk I could construct is already documented, bounded, and
pinned by a test or const-assert (enumerated below so the null verdict is
evidence-backed, not an absence of effort). I found 3 P1 (hot-path cost),
1 P2 (representation), and 8 P3 (duplication / stale docs / dead code)
findings. The dominant theme: the substrate is in excellent shape
algorithmically (O(1) classification, O(1) membership, O(1) double-free
guard, bitmap-indexed directory); the remaining measurable costs are
concentrated in a handful of redundant atomic RMWs and re-validations that
this crate's own gate infrastructure is well positioned to measure.

### Documented residuals verified (not new findings)

For the record, these are the known soundness residuals I re-checked and
confirm are correctly fenced:

- Ring↔magazine cross-thread double-free residual (M2 scope note,
  `docs/INVARIANTS.md`): closed on the drain side by
  `reclaim_offset_checked`'s `is_in_magazine` predicate
  (`src/alloc_core/small/alloc_core_small_reclaim.rs:166-174`) and the
  out-membership wrapper at
  `src/alloc_core/small/alloc_core_small_magazine.rs:245-269`; the own-thread
  `alloc_small` step-2 scan's unreachability argument is pinned by a
  `debug_assert!` tripwire (`src/alloc_core/small/alloc_core_small/mod.rs:194-206`).
- F10 shadow-head staleness is declared a scheduler **assumption**, not a
  theorem (`src/alloc_core/segment/remote_free_ring/mod.rs:203-239`), with the
  wrap-continuity pin (`RING_CAP.is_power_of_two()`,
  `src/alloc_core/segment/remote_free_ring/mod.rs:434-440`) and the
  head-write-site drift test named in the same doc.
- `AllocCore::drop`'s quiescence pin (UBFIX-12/L-8,
  `src/alloc_core/alloc_core/lifecycle.rs:305-353`) — reachable-but-moot
  while registry heaps are never dropped and `AllocCore` stays `!Sync`.
- Un-`hardened` builds trust a free-list `next` word that user memory could
  have corrupted (UBFIX-7 rationale,
  `src/alloc_core/small/alloc_core_small/mod.rs:378-404`); this is the
  documented, `GlobalAlloc`-contract-scoped posture, gated under `hardened`.

## P0 — correctness/soundness

None found. See the verified-residuals list above for what was checked and
why each known hazard is contained.

## P1 — complexity / hot-path cost

### P1-1. `drain_dirty_segments` pays an unconditional atomic RMW per bitmap word even when nothing is dirty

`src/alloc_core/small/alloc_core_small/directory.rs:396-408` — the scan loop
does `let dirty = ds_word.swap(0, core::sync::atomic::Ordering::Acquire);` for
**every** word of `scan_source`, then `continue`s on `dirty == 0`. The slice is
`WORDS_PER_CLASS = MAX_SEGMENTS/64 = 64` words
(`src/alloc_core/segment/segment_directory/mod.rs:170-172`) in every
configuration — both the shared per-segment `dirty_segments` bitmap and, under
`class-aware-dirty`, the per-class sidecar slice (`directory.rs:375-399`).
This path runs at the top of every `find_segment_with_free_impl`
(`src/alloc_core/small/alloc_core_small/find_segment.rs:262-268`), i.e. on
every free-list-miss small allocation in `production` (which turns
`alloc-xthread` + `alloc-segment-directory` on). On x86-64 an atomic `swap`
whose old value is consumed compiles to a `lock xchg`-class RMW that forces
Read-For-Ownership on the bitmap's cache line even when the line is clean and
the word is zero — up to 64 locked RMWs (4 lines) per lookup where a plain
load would do. This is exactly the "atomic RMW where a plain load + conditional
store would do" pattern.

Why a load-filter is sound: the swap is used only as "read-and-clear". A
`Relaxed`/`Acquire` load that reads 0, followed by skipping the swap, can race
a producer setting a bit between the load and the skip — but the bit then
simply stays set until a *later* drain, which is precisely the module's own
documented P4 visibility contract ("bounded deferral … the linear-scan
fallback … a later drain picks it up",
`src/alloc_core/segment/remote_free_ring/mod.rs:320-345`). When the load reads
non-zero, the existing `swap(0, Acquire)` runs unchanged, preserving the
Release/Acquire pairing with the producer's `fetch_or`. Suggested direction:
`if ds_word.load(Relaxed) == 0 { continue; }` before the swap (the Acquire on
the swap remains the pairing edge; no additional ordering is needed on the
skip path, which reads no payload).

Caveat, honestly stated: the win is largest when the bitmap lines are
contended (a producer recently wrote them from another core); when the lines
are locally cached and clean, `xchg` on an owned line is cheaper. **Expected
to help under cross-thread churn; not measured.** Note also that iai
(instruction counts) is largely blind to this — a locked RMW and a plain load
are both ~1 instruction — so validating this needs the wall-clock paired-A/B
infrastructure (see Follow-up ideas).

### P1-2. Release-`assert!` in the realloc fast path re-proves what the caller already proved — and can panic inside the allocator

`src/alloc_core/alloc_core/mem/realloc_fastpath.rs:169-172`:

```rust
assert!(
    self.table.contains_base_ro(base),
    "known-base realloc called for a segment not owned by this core"
);
```

Three problems, in increasing order of importance:

1. **Redundant re-validation on the hot path.** `AllocCore::realloc` already
   runs `self.table.contains_base(base)` immediately before calling this
   function (`src/alloc_core/alloc_core/mem/mod.rs:534-545`), so every
   substrate-level realloc pays the membership probe twice.
   `try_realloc_inplace_known_base`
   (`realloc_fastpath.rs:428-444`) exists *specifically* so `HeapCore::realloc`
   can "reuse its own `contains_base(base)` proof instead of probing the
   segment table again" (its own doc, `realloc_fastpath.rs:428-431`) — the
   `assert!` defeats that purpose for the registry caller too: the probe
   (`contains_base_ro`, Tier-1 hit = one indexed load + compare, but still a
   full call + branch under `assert!`) runs again inside the callee on every
   in-place realloc.
2. **A release-surviving panic on the alloc path.** This is a plain `assert!`,
   not `debug_assert!`. If it ever fires (only possible on a caller bug, but
   the entire defensive-check discipline of this crate is built around
   caller-bug containment), it panics inside a `GlobalAlloc`-face entry chain —
   contradicting the module's own stated guarantee "None of the entry points
   panic or recurse" (`src/alloc_core/alloc_core/mod.rs:31`) and M5's spirit.
   Every other internal trust boundary in this file uses graceful
   `None`/no-op returns.
3. The `debug_assertions`-gated falsification style used elsewhere in this
   same file's siblings (e.g. the F12 pin in
   `src/alloc_core/large/alloc_core_large.rs:351-370`) shows the established
   pattern for exactly this kind of "caller proved it, callee re-checks in
   debug" invariant.

Suggested direction: demote to `debug_assert!` (or delete entirely — the
callers' proofs are two lines away and pinned by tests). No release-path
behavior change is intended or needed; this is a pure cost/policy cleanup.

### P1-3. Large-cache best-fit and FIFO-oldest scans still walk every slot although the R32-12 occupancy bitmask already encodes occupancy

Two scans iterate `0..large_cache_scan_bound()` and consult
`large_cache_slot_get(i)` (an `Option` discriminant load per slot, 56 B/slot
stride per `docs/perf/R32_12_LARGE_CACHE_OCCUPANCY_BITMASK_GATE.md`):

- Best-fit scan on **every** `alloc_large` cache lookup:
  `src/alloc_core/large/alloc_core_large.rs:224-236`
  (`for i in 0..self.large_cache_scan_bound() { if let Some(slot) =
  self.large_cache_slot_get(i) … }`).
- `oldest_occupied_slot` on every eviction/decay step:
  `src/alloc_core/large/alloc_core_large_cache.rs:745-751`.

`large_cache_occupied` (`src/alloc_core/alloc_core/mod.rs:389-390`) was added
in R32-12 precisely to avoid walking the slot array, and is already used by
`large_cache_find_free_slot` (`alloc_core_large_cache.rs:266-291`) — but the
other two scans predate the bitmask and were never converted. Both could
iterate only set bits (`trailing_zeros` + clear-lowest over
`large_cache_occupied & ((1 << bound) - 1)`), turning O(bound) slot reads
into O(popcount) and skipping the empty-slot Option loads entirely (8 slots
normally, up to 40 with `large-cache-extended` materialised). With `seq` in
hand, `oldest_occupied_slot` could additionally short-circuit on `seq == 0`
(the initial monotonic counter) instead of completing the min-by scan.

Impact is honestly bounded: ≤ 40 iterations on a path that is warm only for
large-heavy workloads (best-fit) or cold (eviction). **Expected small win,
not measured** — but it is a natural, cheap A/B for the existing
`docs/perf/R32_12_LARGE_CACHE_OCCUPANCY_BITMASK_GATE.md` harness, which
already built the falsification tests for "bit `i` ⟺ slot `i` is `Some`".

## P2 — types / representation

### P2-1. `pooled_count` / `pool_cap` are `usize` where `u32` provably suffices — 8 bytes per heap × 4096 slots

`src/alloc_core/alloc_core/mod.rs:569-587`: `pooled_count: usize` (571-572)
and `pool_cap: usize` (586-587). `pool_cap` is resolved once at construction
as `min(pool_segments, pool_byte_cap / SEGMENT)`; `pooled_count <= pool_cap`
by the admission rule (`src/alloc_core/small/alloc_core_small_pool/mod.rs:386`).
A `u32` pair would need only a documented clamp at resolution time (a pool cap
above `u32::MAX` segments is not a real configuration — it would mean ≥ 16 EiB
of pooled segments), and would shrink the inline-per-`HeapSlot` `AllocCore` by
8 bytes (`MAX_HEAPS = 4096` ⇒ ~32 KiB of registry footprint). This is exactly
the cost discipline the project itself applies — the `pool_head` doc
(`alloc_core/mod.rs:518-542`) and `dbg_reservation_owner_id`'s doc
(`alloc_core/mod.rs:690-701`) both argue per-field × 4096 costs. Honest
framing: 8 B of a struct whose `large_cache` array alone is ~448 B; worth doing
only alongside the next time `AllocCore`'s footprint is actually measured.
No other `usize` field in the struct shrinks as safely (`large_cache_*`
counters and `seq` genuinely need the width; `small_cur`/pool pointers are
pointers).

## P3 — code quality / duplication / dead code

### P3-1. The ring-drain body exists in three near-identical copies

The "guarded pre-drain check → `ring.drain(|off| reclaim (+dec_live,
changed_classes))` → directory sync → decommit/pool hysteresis →
`set_ring_drain_head(new_head)`" cluster is copy-pasted at:

1. `src/alloc_core/small/alloc_core_small/find_segment.rs:553-650` (linear
   scan),
2. `src/alloc_core/small/alloc_core_small/find_segment.rs:806-849`
   (`validate_directory_candidate`),
3. `src/alloc_core/small/alloc_core_small/directory.rs:437-503`
   (`drain_dirty_segments`).

The drift hazard is not hypothetical: the R10-3 fix ("gate the class bit on
`reclaimed`", with its full rationale comment) is written out in copy 3
(`directory.rs:459-470`) but copies 1 and 2 carry the same
`if reclaimed { … changed_classes |= … }` shape with only a vestigial comment
(`find_segment.rs:595-602`, `826-833`). Suggested direction: a single
`fn drain_segment_ring(&mut self, base, class_ctx, is_in_magazine) -> DrainOutcome`
helper; all three call sites are cold (free-list miss), so a non-`#[inline]`
helper costs nothing measurable.

### P3-2. `reclaim_offset` vs `reclaim_offset_checked` are ~95% the same function

`src/alloc_core/small/alloc_core_small_reclaim.rs:95-225` and `:227-344`.
The checked variant differs only in (a) hardened unpacking, (b) the
`is_in_magazine` consult, (c) the hardened generation guard. The shared guard
chain (class bounds → magic → kind → alignment → H-1 payload lower bound →
M-1 bump bound → M2 bitmap) has been *fixed twice independently* — both files'
comments record H-1 and M-1 being applied to each copy separately
(`:134-160` and `:271-306`). A shared core taking `Option<&F>` (or a
zero-cost `Option<&F>` under `fastbin`) would make the next guard fix
one-edit instead of two.

### P3-3. The numa-aware / non-numa-aware hit arms and the self-heal block are duplicated

`src/alloc_core/small/alloc_core_small/find_segment.rs:655-729` (two
parallel hit arms) and `:732-758` (foreign fallback) repeat the same
`unpool_if_present` → `publish_nonempty` self-heal → counter → `return
Some(base)` sequence three times. The differences are genuinely cfg-shaped
(`fallback` bookkeeping), but the self-heal sub-block (publish + counter,
7 lines × 3) could be one small `#[inline]` helper.

### P3-4. Stale "SAFE `pub fn`" comments contradict `realloc`'s actual `unsafe fn` signature

- `src/alloc_core/alloc_core/mem/mod.rs:556-558` (inside `realloc`'s own
  R2-1 rationale): "…and this is a SAFE `pub fn` (no `unsafe` marker), so
  unlike `GlobalAlloc::realloc` (whose `unsafe` signature makes the caller's
  `old_layout` a trusted precondition)…".
- `src/alloc_core/alloc_core/mem/realloc_fastpath.rs:26-28`
  (`safe_payload_read_span`'s doc): "`realloc` and `HeapCore::realloc` are
  SAFE `pub fn`s (no `unsafe` marker)…".

Both predate R6-MS-1/2, which made `AllocCore::realloc` `pub unsafe fn`
(`src/alloc_core/alloc_core/mem/mod.rs:521-522`). The `safe_payload_read_span`
bound itself remains valid defence-in-depth (and HeapCore::realloc, in the
registry, is out of this review's scope), but the *stated justification*
("safe fn must not read OOB") no longer describes why the code is there. The
comments should be reworded to "defence-in-depth under the `unsafe fn`
contract" so the next reader does not have to re-derive the R6 history.

### P3-5. `pop_free`'s `block_size` parameter is dead

`src/alloc_core/small/alloc_core_small/mod.rs:360-365` takes `block_size`,
never uses it, and disposes of it with `let _ = block_size;` at `:453`
("block_size is the caller's invariant; not needed here"). Every call site
(`alloc_small` ×2, `alloc_small_with_virgin` ×2, the rescue arms ×2, the
post-reserve retry) computes and passes it for nothing. Removing the
parameter simplifies five call sites and deletes the `let _` line. (If the
parameter is kept as forward documentation for a future use, a doc note on
the parameter would be more honest than the `let _`.)

### P3-6. `carve_batch` computes the batch extent twice under lazy-commit

`src/alloc_core/small/alloc_core_small/mod.rs:833-836` (inside the
`primordial-lazy-commit`/`small-segment-lazy-commit` block) computes
`batch_room`/`batch_n`/`batch_end`; lines `851-854` then recompute the same
quantities as `room`/`n` unconditionally. Hoisting the single
`(SEGMENT - aligned_start) / block_size` + `min(out.len(), …)` computation
above the lazy-commit block and reusing it in both places removes the
duplication and one extra division on the Windows lazy path. Cold path;
purely tidiness.

### P3-7. `Node::deref`'s SAFETY comment overstates `ptr::add`'s soundness

`src/alloc_core/platform/node.rs:110-129`: "…`add` on a raw pointer computes
the address (no dereference), which is **always sound**…". Per std's own
contract, `ptr::add` requires the offset to stay in-bounds (or one-past-end)
of the same allocated object; `wrapping_add` is the variant that is
unconditionally sound. Today every caller does bound offsets within a
SEGMENT-span reservation (so the contract holds in practice — the seam's
caller-side bound text at `:114-118` and `:351-356` is what actually carries
the proof), but the phrase "always sound" invites a future caller to skip
that bound. Suggested direction: either switch the body to
`base.wrapping_add(offset)` (making the comment true as written) or tighten
the comment to cite the in-bounds precondition as load-bearing. Zero
codegen impact either way. (Not a live bug — a seam-contract precision fix
in the rust-intel §B5 spirit.)

### P3-8. `alloc_small` and `alloc_small_with_virgin` duplicate their skeletons

`src/alloc_core/small/alloc_core_small/mod.rs:145-264` vs `:293-354`: the
four-step structure (pop current → scan others → carve → reserve-and-retry →
rescue) is written out twice, differing only in the `(ptr, is_virgin)` tuple
and the two pre-carve `payload_virgin_of()` reads. The separation is
deliberate and documented (the plain contract "must never observe the virgin
bit", `:266-284`), so this is accepted-structure, not a defect — but a
`macro_rules!` skeleton (or an internal enum-dispatch on `WantVirgin`)
parameterised by the virgin reporting would keep the step ordering from
having to be maintained twice. Only worth doing if the bodies are expected
to keep evolving; hot-path codegen should be checked with iai before
accepting any such change.

## Follow-up measurement ideas

- **P1-1 (dirty-bitmap load-filter):** paired-A/B wall-clock under
  cross-thread churn (larson/mstress-shaped), not iai — a `lock xchg` vs a
  plain load is invisible to instruction counts. The existing
  `paired-ab-runner.mjs` + fixed-work AB harness pattern (per
  `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB_GATE.md`) fits directly;
  assert the per-arm mechanism oracle (drain counts /
  `DIRTY_SEGMENTS_DRAINED`) per CLAUDE.md's path-activation rule.
- **P1-3 (bitmask-driven best-fit / oldest-slot scans):** reuse
  `docs/perf/R32_12_LARGE_CACHE_OCCUPANCY_BITMASK_GATE.md`'s harness and
  falsification tests; large-object realloc/alloc-cycle bench (the R32-10
  observational workload already exercises exactly this path).
- **P1-2 (assert demotion):** a strict Ir A/B on the realloc benches
  (`benches/perf_gate_iai.rs`'s `realloc_grow`, as used by
  `docs/perf/R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md`) should show a small
  Ir drop; the primary justification is the no-panic policy, so even a null
  Ir result should not block the cleanup.
- **P3-6 (carve_batch dedup):** measurable only on the Windows lazy-commit
  path (the division runs per batch there); a Windows wall-clock gate in the
  `small-segment-lazy-commit` configuration, or skip measurement and land it
  as tidiness.
- **Size-class `align_up` division on the carve path** (observation, not a
  finding above): `carve_block`/`carve_batch` pay one `div_ceil`
  (`src/alloc_core/segment/segment_header/descriptors.rs:14-19`) per
  carve because class block sizes are not powers of two (1.25× spacing).
  `carve_batch` already hoists it to once per run; if iai ever flags the
  single-block carve, a per-class precomputed "align padding LUT"
  (bump-relative next-aligned-offset hint) is the obvious direction —
  flagging as a future idea only; no current evidence it is hot.

---

*Reviewer: independent readonly review, branch `review-alloc-core`.
No `src/` files were modified; no benchmarks were executed.*
