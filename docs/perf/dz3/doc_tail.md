
## 3. Reading

- **Why direct-mapped.** The first prototypes kept the linear search: 16 or 64 slots fix the cyclic and random orders but
  make every hit scan the array (the sequential order is slower than the 4-slot base at m = 16, section B). A last-hit
  memo helped the sequential order but not the others. The direct-mapped index makes the hit path independent of the
  slot count.
- **Why 32.** Direct-mapped 16 loses at m = 16 cyclic (1.03 of base: two of the 16 segments of the probe collide), 32
  and 64 slots hold m = 16; 32 keeps the retention bound at about 9.3 MiB instead of 18.6 MiB. Collisions between
  segments whose indices are congruent modulo 32 remain possible; consecutive segments never collide.
- **Why a sweep budget of 4.** With a budget of 8 occupied slots per call, the thrash cases (m = 64) paid more per
  miss than the base (cyclic 1.21 of base, section C); 4 matches the base's cost per miss. The sweep must still reclaim
  a dead pin on the first miss after its route is unlinked when the cache holds few pins: the existing test
  `sweep_reclaims_unlinked_cached_pin_after_next_miss` pins that, and a cache with at most 4 pins is swept completely
  by every call.
- **The sequential order.** Program instructions differ by {{SEQ_IR}}; the time moves by {{SEQ_RANGE}} of base, which is
  noise plus cache-line effects of a 768 B thread-local array against 96 B. It is the only cost visible on the hit
  path.
- **Retention.** A cached pin keeps the route descriptor and its sidecar alive after the segment's reservation is gone;
  the descriptor of a segment with every class in use is about 0.29 MiB, so 32 slots bound the dead descriptors an idle
  thread holds at about 9.3 MiB (4 slots: about 1.16 MiB). The sweep reclaims dead pins on the next miss or insert of that
  thread; a thread that never misses again keeps them until it exits, as before.
- **Safety.** Every site that stores or drops a pin keeps its ghost event (`pin-cache/held` +1 / -1 at the same
  places; the catalog counts per function are unchanged), so the loom ledger of `tests/loom_b4_r1_pin_cache.rs` still
  sees exactly one reference per occupied slot. The unlink fallback and the incarnation check of `publish_cached` are
  untouched.

## 4. Limits

- Linux only (WSL2), one machine shared with other sessions; the foreign-free path was not measured on Windows.
- m = 64 is not fixed and its cells are noisy (section A and section C differ by {{C64DIFF}} in change / base on the
  same cell).
- Collisions: bases whose segment indices are congruent modulo 32 evict each other. The new test uses a bound that
  tolerates a few colliding pairs; a 4-slot cache has hit rate 0.00 on it.
- The MT controls (section D) are underpowered for a 1-2 % effect: ranges overlap in every cell.
- The prototypes of section B were measured in separate runs, each against the base of its own run.

## 5. Reproduction

- `pc2.sh`, `pc2t.sh` (baseline of PC-2), `pc2e.sh`, `pc2m.sh`, `pc2d.sh`, `pc2d32.sh` (prototypes of section B),
  `pc2b4.sh`, `pc2fir.sh` (sweep budget of section C), `pc2f2.sh` (section A), `pc2mt.sh` (section D). They are kept as
  run, with the scratch directory replaced by `<scratch dir>`; they need WSL2 with valgrind.
- Tables and CSV: `node scripts/dz3_report.mjs` (writes `docs/perf/dz3/tables.md`,
  `docs/perf/DZ3_PIN_CACHE_DIRECT_MAPPED_summary.csv` and this file from `doc_head.md`, `doc_tail.md` and the tables).
