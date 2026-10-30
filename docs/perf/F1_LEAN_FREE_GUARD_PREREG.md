# F-1 — lean small-free guard: pre-registration

Written and committed before the confirmatory data.

**Change (this branch).** `small_free_guard` (`src/registry/heap_core/free/dealloc_own_base.rs`)
reads the state byte first and folds the misalignment and byte checks into one
branch; the metadata-offset (`payload_start`) reject and the reject-reason split
move into an inlined classifier on the mismatch branch, in the former order.
The accepted set is unchanged: the state bytes of offsets below the payload
start lie inside the never-written prefix of the state map (zero), so an
`encode(c, LIVE)` match already implies `off >= payload_start`; the whole map is
committed with the metadata. The bump check stays on the hot path.

**Mechanism evidence (screening, WSL perf/callgrind, `b4_pmu_probe inter`).**
The J1 churn kernels share one slot ring, so churn16 is a cross-class load
(free 64/256 B, alloc 16 B). There sefer pays ~108–112 Ir per alloc+free pair
against mimalloc's 70–96; free alone is 68–70 Ir against 23–25. The base
`sefer_dealloc` saves three registers on entry; this variant saves one.

**Judge.** `node scripts/bench-table-paired.mjs --features production`
(3 processes × 200 rounds), aligned code
(`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`),
order base, f1c, f1c, base. Per cell and pair, d = (f1c sefer/mi) / (base sefer/mi) − 1.

**Rule.** A cell is WIN if d < −1.5 % in both pairs, LOSS if d > +1.5 % in both
pairs, otherwise no change. GO if at least one WIN and no LOSS; otherwise NO-GO.
