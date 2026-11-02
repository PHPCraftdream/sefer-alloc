
## 3. Reading

- **Why the owner phase is expensive.** The foreign frees are cheap for the freeing threads (bits in a sidecar and a
  notification), so the whole price is paid by the owner at its first discovery: all notified segments are drained in
  one call, record by record (each record validates geometry, checks the state byte, links the cold block into the
  freelist, rewrites the state byte), and a segment that becomes empty is pooled or released. Section C2 prices this at
  66.3 Ir per block for the drain alone. mimalloc keeps the remote frees of a page on one list and collects it with one
  swap.
- **Why the owner then carves.** With a pool cap of 4 and 16 emptied segments, 4 are pooled and 10 released (the
  counters call the other 2 drains "kept": they reclaimed blocks and left the segment live). From about the 21st
  thousand refill (section B2) no freelist has blocks left in reach and every refill is a bump-carve of a fresh segment, preceded by a discovery call
  that fails (38325 calls, 5 hits). Fresh segments also bring page faults, which callgrind does not count; the larger
  gain of E1 on 1024 B blocks (0.671 against 0.866 on 64 B) is consistent with that, but page faults were not measured.
- **Why a bigger pool did not help.** Pool 8 reuses more blocks from freelists, but the owner phase did not get
  cheaper (section D, E0): reusing a block through drain + freelist + magazine costs about as many instructions as
  carving it. The loss is dominated by the length of the reclaim path, not by where the block comes from.
- **The pause.** The longest call is the first discovery, which drains every notified segment before it returns;
  it scales with S. E1 does not remove it (drain is still done), E2 is unrelated to it.
- **Design options seen in review (not implemented, no measurement behind them yet).** (a) Do not release or pool a
  segment that a drain emptied while there is demand for its class: E1 is its upper bound for the release part and it
  belongs to the open `Retain` policy decision (RET-1, owner). (b) When a drain would empty the whole segment, reset it
  in bulk instead of linking record by record: it targets the 66.3 Ir per block and the pause, and needs a new
  invariant set (state bytes, `BinTable` heads, `live_count`, virgin / committed state, class cursor and pool
  membership) with Loom and ghost coverage. (c) Replace the discovery miss by a cheaper miss (a summary word of the
  non-empty directory words, an `ingress` summary check): E2 bounds its value. (d) Stop `process_ingress` at the first
  hit and keep the rest in an owner-side bitmap: a protocol change. (e) Merge the two carve loops.

## 4. Limits

- Linux only; one machine shared with other sessions; Windows (where the growth losses of LM-1 are larger) is not
  measured here.
- The latency sweep was run with an earlier build of the probe than the counter and experiment runs (the probe gained
  the `sefer8` arm and the `RP1_NOSTATS` switch later); the measured allocation path is the same.
- Counters were measured on a scratch-patched copy; the experiments change one condition each and are not shipped
  candidates (E2 is explicitly unsafe: it can hide free blocks).
- RSS and page faults were not measured; the 3-thread wave is joined before the owner phase (no overlap), which is the
  hand-off shape, not a steady-state pipeline (larson T=2 / T=4 are wins for sefer, LM-1).
- Section C2 call counts were cross-checked against the counters of section B (the script asserts equality).

## 5. What is left

- Decide with the owner whether a drain-emptied segment may be retained under demand (RET-1) and what RSS it may cost.
- If a structural change is wanted: prototype (b) behind a feature, with an activation oracle, and judge it on this
  shape plus growth, larson, mstress and the cross-thread cells with the magnitude rule.
- Measure the same shape on Windows and with RSS.

## 6. Reproduction

- `rp1.sh` (latency sweep), `rp1cg2.sh` (callgrind of the owner phase), `tree.sh`, `tree0.sh`, `tree2.sh` (call-tree
  extracts for the run with allocations and the set-up-only run), `rp1cnt.sh` + `patch_counters.py` (refill-path counters
  on a scratch copy), `rp1b.sh` (pool-8 arm), `rp1e1.sh`, `rp1e2.sh` (experiments). They are kept as run, with the
  scratch directory replaced by `<scratch dir>`; they need WSL2 with valgrind.
- Tables and CSV: `node scripts/rp2_report.mjs` (writes `docs/perf/rp2/tables.md`,
  `docs/perf/RP2_REMOTE_FREE_RECLAIM_summary.csv` and this file from `doc_head.md`, `doc_tail.md` and the tables).
