// gp_c3_report.md — chunk 3 (b4-gap): Ir function breakdown from EXISTING chunk-2 .out
// All numbers ИЗМЕРЕНО (callgrind_annotate --inclusive=no --auto=no --threshold=99
// on ~/ab/gp-a1-out/gp_cg.{k}.{arm}.{40,80}.out); delta = p80 - p40 per function.
// Paths sanitized: <repo> = <repo>.
// No production files modified; git diff = 0 bytes (identity in gp_c3_identity.txt).

Command: python3 <repo>/.rush/wsl/gp_c3_funcs.py ; rc=0
Coverage check (sum of per-func self_ir_delta vs marginal Ir delta from
gp_callgrind_marginals.csv): all 10 (kernel,arm) pairs dev < 1.06% — OK (<2%).

Artifacts:
  gp_func_breakdown.csv  7921 bytes  (64 rows: top10 sefer + top3 mi per kernel)
  gp_c3_funcs.py         5025 bytes  (collector, asserts sum-vs-marginal)
  gp_c3_identity.txt      199 bytes  (git diff sha256 = empty-tree hash; status clean)

TOP-5 sefer, churn16 (identical ИЗМЕРЕНО for churn64/churn256 — same marginal delta 1,027,812 Ir):
  1 sefer_a_dealloc            delta=409,600  share=39.85%  <repo>/examples/_shared/b4_interleave_core.rs:181 (inst.:191)
      probe shim, #[inline(never)]: SHIM_CALLS fetch_add + SeferAlloc::__rust_dealloc face
  2 sefer_a_alloc              delta=230,400  share=22.42%  <repo>/examples/_shared/b4_interleave_core.rs:149 (inst.:191)
      probe shim: fetch_add + __rust_alloc face
  3 kernel_churn               delta=216,600  share=21.07%  <repo>/examples/_shared/b4_interleave_core.rs:600
      probe ring-buffer churn loop (dealloc old, alloc new, 16-byte write)
  4 main                       delta= 95,760  share= 9.32%  <repo>/examples/gp_kernel_probe.rs:137
      harness/reporting loop (probe output, not allocator)
  5 memrchr                    delta= 21,712  share= 2.11%  libc
      libc memrchr (std fmt/stdio path around harness output)

TOP-5 sefer, bulk16k:
  1 sefer_a_dealloc                     delta=53,983,760  share=29.85%  shim (см. выше)
  2 try_drain_freelist_batch            delta=39,323,760  share=21.74%  <repo>/src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs:525
      AllocCore: bulk-pop of segment free list into an out-batch, commits LIVE
      state byte per block; the magazine-refill/alloc-batch hot face
  3 flush_class_internal                delta=38,219,440  share=21.13%  <repo>/src/alloc_core/small/alloc_core_small_magazine.rs:565
      AllocCore: magazine flush — groups freed blocks into same-segment runs,
      pushes them back on the segment free list, recycles empty segments
  4 sefer_a_alloc                       delta=29,041,080  share=16.06%  shim
  5 kernel_bulk                         delta=13,109,400  share= 7.25%  <repo>/examples/_shared/b4_interleave_core.rs:555
      probe bulk loop (n allocs then n frees per pass)

bulk64 sefer top-5 (shares): sefer_a_dealloc 33.03%, try_drain_freelist_batch 18.14%,
  sefer_a_alloc 17.97%, flush_class_internal 17.71%, kernel_bulk 8.25%.
mi contrast (bulk16k): mi_free 21.56%, mi_theap_malloc_aligned 21.52%, kernel_bulk 19.60% —
  mimalloc spread is ~2 named functions plus the kernel; sefer spreads over 4+ allocator
  functions with drain/flush pair ≈ 43% of marginal Ir on bulk.

Conclusions (ИЗМЕРЕНО):
  - bulk*: sefer's marginal Ir is dominated by the AllocCore batch pair
    try_drain_freelist_batch + flush_class_internal (~40-43%) on top of the shim
    overhead (~46-51%); mimalloc's equivalent work sits in mi_free/mi_theap_malloc_*.
  - churn16/64/256: no batch refill/flush functions appear at all in top-10 — the
    churn path stays inside the magazine fast face attributed to the probe shims;
    sefer shims + kernel = ~83% of marginal Ir.
  - churn16/64/256 deltas are IDENTICAL per function (same marginal 1,027,812) —
    size 16/64/256 does not change the Ir shape on the sefer face (ИЗМЕРЕНО).

Zero-event check (chunk-2 counters = 0) — ИЗМЕРЕНО/OЦЕНКА mix:
  - tcache_hits_total: increments live on the tcache-hit face of
    <repo>/src/registry/heap_core/alloc/hot.rs:282 and :436 (per-slot
    HeapSlot::tcache_hits, folded in heap_registry/counters.rs:187). The probe
    hot path never enters the tcache-hit face (no hit counters in top-10), so 0
    is consistent, and this hot path (batch freelist) has NO dedicated
    bench-internals counter. Only S1_REFILL_EVENTS (magazine refill, MAG-state
    only) exists: <repo>/src/alloc_core/small/alloc_core_small_magazine.rs:253.
  - large_cache_hits_total: same design (hot.rs face + counters.rs:266); the
    large-cache-hit face did not fire on bulk16k/bulk64 paths sampled here
    (drain_large_sidecar_ingress_hot_bounded shows in top-10 at 1.2-1.5% with
    no counter of its own — uncovered observer: sidecar_drain.rs:572).
  - config_conflicts_total / heaps_claimed_high_water: claim-path aggregators
    (heap_registry/counters.rs:99 / :80, claim.rs walk). Single-threaded probe
    claims once per thread — 0 ОЦЕНКА-consistent.
  - Uncovered hot paths (candidates for future counters, NO src edits here):
    try_drain_freelist_batch (alloc_core_small_impl.rs:525),
    flush_class_internal (alloc_core_small_magazine.rs:565),
    drain_large_sidecar_ingress_hot_bounded (src/alloc_core/alloc_core/sidecar_drain.rs:572).
