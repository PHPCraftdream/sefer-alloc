
## 3. Reading and the default

- **What the table says about the reach.** Hit rate 1.00 for a cyclic order needs the `m` owner segments to occupy `m`
  different slots, and the measured reach is about half the slot count: 8 slots serve 4 segments (m = 5: hit rate 0.60,
  that is 3 of 5 segments), 16 serve 8, 32 serve 16, 64 serve 32, and at `m == S` the cyclic order already misses on every
  free (section A). Inference, not measured here: on 64-bit Unix a reservation keeps `size + align` of address space
  (`docs/perf/OPEN_ITEMS.md` item 53), so the segments of one owner sit 8 MiB apart, the index `(address / 4 MiB) mod S`
  only ever lands on even slots, and the effective slot count is `S / 2`. On Windows the spacing was not measured. An index
  that spreads such a layout (a fold or a multiplicative hash) would double the reach for one to three more instructions
  on the hit path; it was not evaluated and is recorded as a follow-up. A sequential order never needs more than one slot.
- **Why the default stays 32.** 32 slots reproduce the shipped behaviour exactly (the 32-slot arm matches base's hit
  rate in every cell, asserted) and serve the shape that was measured to matter (a producer freeing into up to ~16 owner
  segments in any order, DZ-3). 64 slots add the m = 32 cyclic and random cells (section B) for twice the worst-case
  retention (section D); 16 slots and below lose the m = 16 cyclic cell (and part of the random one). A workload that fans in from more than ~16 owner
  segments per producer thread should set 64; a workload with few owner segments per producer and many producer
  threads can set 8 or 16 to cut the retention bound (sections A, D).
- **What does not work.** m = 64 in a cyclic order is not served by any count up to 64; at that size the cache is a
  loss of the lookup it was meant to save, with or without the setting.
- **The cost of the setting.** One more load on the hit path and none measurable: {{SEQ32}} at the default against base
  (section C), the time of the sequential order moves by {{SEQ_NS32}} of base (noise of this machine). In the miss regime
  (every free misses) the 32-slot arm is {{MISS32_NS}} of base in time (section B, cells m = 32 and 64, cyclic and random).
  The runtime-configurable count is therefore paid for with nothing the harness can see; the array grew from 768 B to
  1,536 B of thread-local storage per thread.
- **Safety.** Every site that stores or drops a pin keeps its ghost event (`pin-cache/held` +1 / -1 at the same places;
  the catalog counts per function are unchanged), the mask is written only while the cache holds no pin and never changes
  afterwards, the index is masked with the array capacity so it is in range for every value, and the sweep and `Drop`
  walk the slots in use or all of them respectively. `tests/route_pin_cache_slots.rs` pins the resolution (rounding,
  clamping, default) and the effect through a real `#[global_allocator]` built with 8 slots (a producer that only
  frees adopts the value and cannot hold more than 7 of 16 colliding segments hot; with the default 32 it would hold
  all of them).

## 4. Limits

- Linux only (WSL2), one machine shared with other sessions; the foreign-free path was not measured on Windows.
- One owner segment shape (1 KiB blocks, 2048 freed per segment), one producer thread; the retention column is the
  DZ-3 worst case (every class in use), not a measurement of this run.
- The MT controls of DZ-3 are not repeated: the arm with 32 slots differs from base by the mask load only.
- Section C counts instructions of the whole probe process; the set-up phase is identical across arms, so the difference
  is the frees plus the start-up noise of the process.

## 5. Reproduction

- `cf2sweep.sh` (kept as run, scratch directory replaced by `<scratch dir>`; needs WSL2 with valgrind): builds the arms and
  base from `git archive` tars of the two trees, writes `counters.txt`, `ir.txt`, `time.txt` (kept gzip-compressed as
  `time.txt.gz`) and `ident.txt`.
- Tables and CSV: `node scripts/cf2_report.mjs` (writes `docs/perf/cf2/tables.md`,
  `docs/perf/CF2_ROUTE_PIN_CACHE_SLOTS_summary.csv` and this file from `doc_head.md`, `doc_tail.md` and the tables).
