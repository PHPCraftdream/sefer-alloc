# T-1 — linear class index for sizes up to 80 B: pre-registration

Written and committed before the confirmatory data.

**Change (this branch).** `SizeClasses::class_for_small_align`
(`src/alloc_core/platform/size_classes.rs`) returns `(size - 1) >> 4` for
`size <= LINEAR_MAX = 80` instead of loading `SIZE2CLASS`; a const assert pins
`SIZE_CLASS_TABLE[i] == (i + 1) * 16` for every class below 80 B. The result is
the same class for every size, so alloc and free keep their exact semantics.

**Mechanism (screening, WSL disassembly of `b4_pmu_probe`).** For sizes up to
80 B both `sefer_alloc` and `sefer_dealloc` lose the table load (and on Linux
its GOT load) and the `classes[c]` bounds check: −3 instructions and two
dependent loads off the path to the magazine count load. Sizes above 80 B pay
one more compare and branch.

**Judge.** `node scripts/bench-table-paired.mjs --features production`
(3 processes × 200 rounds), aligned code
(`-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5`),
order base, t1, t1, base. Per cell and pair, d = (t1 sefer/mi) / (base sefer/mi) − 1.

**Rule.** A cell is WIN if d < −1.5 % in both pairs, LOSS if d > +1.5 % in both
pairs, otherwise no change. GO if at least one WIN and no LOSS; otherwise NO-GO.
