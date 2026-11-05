# J13 — owner decision on the Large-block THP hint (2026-11-05)

Campaign 2 (`J13_LARGE_BLOCK_THP_C2.md`, generated, not edited) is **NO-GO by the letter of the preregistered gates**: five resident-memory (T2) limits were exceeded by
2 KiB, 4 KiB, 8 KiB, 50 KiB and 142 KiB (0.02 % to 0.9 % of limits that are exact multiples of 2 MiB, with no tolerance for the few tens of KiB of unrelated allocations in a median RSS).
The mechanism (T0) and the benefit (T1: cold x / base 0.43 to 0.57) pass, as does the control; no cell was slower except `edge`, which passes its absolute limit (2 ms).

**Decision (owner, 2026-11-05): accepted.** The hint landed as a `perf(runtime)` change (the patch of the tag `archive/g1-j13-thp`, `5a861a6f`, plus README and CHANGELOG). The preregistered gates and the two
published campaign reports are not edited; this file is the record of the exception.

Open after the decision: the extra resident memory over many live, barely touched hinted blocks is 2 MiB per block with no ceiling; the bound is preregistered as J13b
(`J13B_BOUNDED_THP_PREREG.md`, a counter of live big reservations, cap 128) and is measured separately.
