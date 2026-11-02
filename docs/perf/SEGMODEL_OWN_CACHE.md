# Own-thread free lane model — what a different `own_cache` index or size would change (`bench`, measurement-only, counts, no code change in `src/`)

Measurement-only: no `perf(runtime)` or `perf(opt-in)` change. Only the benchmark judge `examples/g1_wild_judge.rs` gained
an opt-in counting mode (`WJ_SEGMODEL=1`). The question comes from the independent consultation after
`FAIRAXIS_WINDOWS.md`: could a folded index or a larger `own_cache` remove the hash probes that the own-thread free lane
still takes in `wild` (WILD-2 left 29 % of the lane frees on the hash probe), and does the same hold for `growmix`?

**Entry point under test:** a static `SeferAlloc` instance reached through `#[inline(never)]` `GlobalAlloc` shims at the
`__rust_alloc` boundary (header of `examples/g1_wild_judge.rs`), arm `sefer`, one process per run, the process's own global
allocator stays `System`. It is not the process's registered `#[global_allocator]`; it is the same `GlobalAlloc` methods
on the same allocator type. The counted stream is the real address stream of that run's small-block frees; the lane itself is
a model (below), not the production code.

## Design

* The judge feeds every free of a block up to 1 KiB to a model of the lane's resolution order: the one-entry segment memo,
  then the direct-mapped `own_cache` (an exact-base hit arms the memo), then a hash probe that fills the cache and does not
  arm the memo — the order of `dealloc_fast` with WILD-2 (`src/alloc_core/segment/segment_table/segment_table_impl.rs`).
  A segment is 4 MiB aligned and its key is `address >> 22`.
* Six shapes: 16, 32 and 64 entries, each indexed by `key & (n-1)` (the production index for 16) and by
  `(key ^ (key >> 4)) & (n-1)` ("fold"). Counts only.
* Windows 10, i7-11800H, shared machine; source `docs/perf/segmodel/ident.txt`; 3 runs per scenario, the median of each
  count; each run's three counts add up to its small-free count (the report script asserts it).
* Not a timing measurement; no A/A is needed for counts. Everything in the table follows from the address stream of the
  judge's workloads on this machine and OS (the addresses differ run to run; the three runs agree to about 2 %).

## Result (`node scripts/segmodel_report.mjs`, derived from `docs/perf/segmodel/win_raw.jsonl`)

| Scenario | small frees | distinct segments | odd keys | occupied slots of 16 | shape | memo | own_cache hit | hash probe (count) |
|---|---:|---:|---:|---:|---|---:|---:|---:|
| wild | 4260419 | 23 | 13 | 13 | 16 | 11.2 % | 58.6 % | 30.2 % (1284678) |
| wild | 4260419 | 23 | 13 | 13 | 16fold | 12.2 % | 57.1 % | 30.7 % (1308521) |
| wild | 4260419 | 23 | 13 | 13 | 32 | 11.2 % | 78.7 % | 10.1 % (429950) |
| wild | 4260419 | 23 | 13 | 13 | 32fold | 11.5 % | 75.0 % | 13.5 % (576648) |
| wild | 4260419 | 23 | 13 | 13 | 64 | 11.1 % | 82.7 % | 6.5 % (278403) |
| wild | 4260419 | 23 | 13 | 13 | 64fold | 11.1 % | 82.8 % | 6.1 % (258851) |
| growmix | 1095209 | 311 | 155 | 16 | 16 | 0.3 % | 5.0 % | 94.7 % (1036858) |
| growmix | 1095209 | 311 | 155 | 16 | 16fold | 0.3 % | 5.0 % | 94.7 % (1037020) |
| growmix | 1095209 | 311 | 155 | 16 | 32 | 0.3 % | 10.0 % | 89.6 % (981746) |
| growmix | 1095209 | 311 | 155 | 16 | 32fold | 0.3 % | 10.0 % | 89.6 % (981558) |
| growmix | 1095209 | 311 | 155 | 16 | 64 | 0.3 % | 20.0 % | 79.7 % (872684) |
| growmix | 1095209 | 311 | 155 | 16 | 64fold | 0.3 % | 20.3 % | 79.4 % (869163) |
| phase | 18127987 | 5 | 4 | 4 | 16 | 98.0 % | 2.0 % | 0.0 % (136) |
| phase | 18127987 | 5 | 4 | 4 | 16fold | 98.0 % | 1.7 % | 0.3 % (56104) |
| phase | 18127987 | 5 | 4 | 4 | 32 | 98.0 % | 2.0 % | 0.0 % (5) |
| phase | 18127987 | 5 | 4 | 4 | 32fold | 98.0 % | 2.0 % | 0.0 % (4630) |
| phase | 18127987 | 5 | 4 | 4 | 64 | 98.0 % | 2.0 % | 0.0 % (5) |
| phase | 18127987 | 5 | 4 | 4 | 64fold | 98.0 % | 2.0 % | 0.0 % (5) |

The full table, including `regrow`, `grow64` and `grow1k` (hash probes under 0.1 % of small frees in all three, so no
room for any change), is in `docs/perf/SEGMODEL_OWN_CACHE_summary.csv`.

## Reading

* **The model reproduces the known figure.** The 16-entry model has 30.2 % of `wild`'s small frees on the hash probe;
  WILD-2 measured 29 % in the production build on Linux (`WILD2_LANE_SECOND_CHANCE.md` section 4). The two are different OSes,
  so this is a consistency check, not a validation against `tier1_misses` of the same build.
* **The fold index is NO-GO.** It changes `wild` from 30.2 % to 30.7 % on the hash probe at 16 entries and from 10.1 % to
  13.5 % at 32. The premise behind it — that segment keys of one heap sit two slots apart and leave half of the 16 slots
  empty (the Unix finding of CF-2) — does not hold on Windows: 13 of 23 `wild` keys are odd and 13 of the 16 slots are
  occupied. `wild` has 23 live segments for 16 slots, so the misses are capacity misses, which an index function cannot
  remove.
* **Size is where the hash probes go.** `wild`: 30.2 % at 16, 10.1 % at 32 (a 67 % relative cut), 6.5 % at 64. That is the
  ceiling for any layout that keeps a segment-key cache in front of the hash; it is a count, not a time. The cost side is
  known from `WILD2_LANE_SECOND_CHANCE.md`: 32 entries of the current `(key, root)` pair need 256 bytes more in `HeapCore`,
  which is at 9208 of 9216 bytes under `--all-features`; a root-only layout would hold 32 entries in the same 256 bytes
  but would stop caching biased Large payload keys. An earlier sweep of 64/256/512 entries (task OC-1) ended NO-GO; that
  result is recorded only in the session task list, not in the repository, so this report does not rest on it.
* **`growmix` cannot be reached by the cache.** 311 live segments: even 64 entries leave 79.7 % of small frees on the hash
  probe (94.7 % at 16). The gap measured in `FAIRAXIS_WINDOWS.md` (user CPU 1.4–1.5 of mimalloc's) is not a cache-size
  problem; it is the cost of resolving one of hundreds of segments per free, consistent with the metadata cache misses
  attributed in `GM-1`.
* **`phase`, `regrow`, `grow64`, `grow1k` need no change**: the memo and the cache already resolve at least 99.9 % (`phase` 98.0 %
  memo plus 2.0 % cache).

## Verdict

NO-GO for the fold index (no effect; its premise is a Unix layout). The size lever is bounded by a count (`wild`: 30 % → 10 %
of small frees leave the hash probe at 32 entries) and by a memory budget in `HeapCore`; whether it pays in time is not measured
here, and `growmix` is out of its reach. No code candidate follows from this measurement. The remaining `wild` and `growmix`
gap sits in the per-free resolution of one segment among many, which the safety design requires (a hash probe validates that
the segment belongs to the heap, the state byte validates that the block is live).

## Limits

* Windows only, one machine; counts depend on the address stream of the judge's workloads; no production-code counter was
  compared in the same build (`tier1_hits`/`tier1_misses` need `bench-internals`).
* Blocks up to 1 KiB are treated as small; blocks larger than that are not fed to the model.
* The model has one thread and one heap, as the judge does.

## Identity and reproduction

* `docs/perf/segmodel/ident.txt` (source SHA, judge file hash, binary hash), raw `docs/perf/segmodel/win_raw.jsonl`,
  summary `docs/perf/SEGMODEL_OWN_CACHE_summary.csv`
  (`node scripts/segmodel_report.mjs --csv docs/perf/SEGMODEL_OWN_CACHE_summary.csv`).
* Re-run: build `examples/g1_wild_judge.rs` (`--release --features production`), then
  `node docs/perf/segmodel/run.mjs <exe> <out.jsonl> 3`.
