# `SeferAlloc` — benchmark & honest verdict

> ## 0.3.0 post-X-arc re-measurement (2026-07-06) — the realloc breakthrough
>
> Full re-run after the X-arc (#182–188: X1 OPT-G in-place Large realloc, X2
> #164 drain-side magazine check + magazine-routed realloc alloc-leg). Same
> host/profile/caveats as the 2026-07-05 section below (noisy Windows dev box,
> ±15–20 %, ratios are the signal). **The realloc rows are the news** — the
> X-arc turned realloc from parity into a rout:
>
> | Bench | SeferAlloc | mimalloc | System | vs mimalloc | vs System | pre-X was |
> |---|---|---|---|---|---|---|
> | `realloc_grow_geometric` (64 B→4 MiB) | **9.67 µs** | 382.7 µs | 2.78 ms | **39.6× faster** | **288× faster** | ~323 µs (1.1× faster) |
> | `realloc_in_place_unfavorable` | **906 ns** | 1.355 ms | 7.26 ms | **~1,500× faster** | **~8,000× faster** | ~1.68 ms (1.1× SLOWER) |
>
> SeferAlloc improved on ITSELF 33× / 1,850× on these two benches: every
> Large→Large growth step that fits the already-committed 4 MiB span is now a
> header update returning the same pointer (OPT-G), and the small-step alloc
> leg rides the magazine (X2). The deterministic proof is the Ir judge:
> `realloc_grow` 1,520,714 → 561,912 Ir (−63 %), Estimated Cycles −47 %.
>
> **Large alloc+free** (unchanged flagship): 4 MiB **58.6 ns** vs mi 716 ns
> (12.2×) vs Sys 17.7 µs (302×); 16 MiB **61.9 ns** (13.5× / 237×); 64 MiB
> **60.8 ns** (33× / 258×).
>
> **Churn** (writing, realistic): 16 B **22.3 µs** vs 38.8 (1.74×); 64 B
> **22.3** vs 38.0 (1.71×); 256 B **22.9** vs 23.2 (≈parity-plus); 1024 B
> **22.8** vs 165.1 (**7.2×**). Non-writing: 1.81× / 1.83× / 1.07× / 7.29×.
> Sefer leads at every size on both patterns — X2's drain-side check did not
> regress the won front (its refill cost is one-time-bootstrap + refill-miss
> only, invisible at wall-clock).
>
> **Cold/bulk direct** (documented magazine worst-case, unchanged story):
> 16 B 2.5× / 64 B 2.1× / 256 B 1.8× slower than mimalloc; 1024 B 1.12×
> faster; `Vec_push` 557 ns ≈ mi 498 ns. vs System 3.9–4.1× faster.
>
> Where this section and the 2026-07-05 one disagree, THIS one is current.
>
> ## 0.3.0 pre-X-arc re-measurement (2026-07-05) — vs mimalloc & System
>
> Fresh full re-run on the clean post-W7 tree (`origin/main`), taken as the
> reference before the perf/correctness X-arc (#182–187). Same host and quick
> criterion profile (`sample_size(10)`, 150 ms warm-up / 600 ms measurement),
> noisy Windows 10 dev machine (±15–20 %). All three allocators are driven
> directly through their `GlobalAlloc` impl in ONE binary — a true
> apples-to-apples comparison (`SeferAlloc` is NOT installed as the bench
> binary's global allocator; it is called directly, exactly like `mimalloc 0.1`
> and `System`). Medians shown; trust the ratio and order of magnitude, not the
> exact µs. The deterministic per-op judge remains `perf_gate_iai` (`Ir`, Linux
> CI / `npm run iai` — see [`perf/IAI_BASELINE.md`](perf/IAI_BASELINE.md), whose
> "Pre-X-arc full baseline" section is byte-identical to post-W7a). Ratio =
> Sefer/mimalloc; **< 1.0 = Sefer faster**.
>
> **Large alloc+free — flagship large-cache (`alloc-decommit`):**
>
> | size   | SeferAlloc |  mimalloc |  System | vs mimalloc | vs System |
> | ------ | ---------: | --------: | ------: | ----------: | --------: |
> | 4 MiB  | **59.0 ns** |   735 ns | 15.9 µs | **12.5× faster** | **269× faster** |
> | 16 MiB | **75.5 ns** |  1.13 µs | 17.7 µs | **15.0× faster** | **234× faster** |
> | 64 MiB | **73.8 ns** |  2.58 µs | 18.8 µs | **35.0× faster** | **255× faster** |
>
> **Steady-state churn** (256-block working set, non-writing):
>
> | size   | SeferAlloc | mimalloc |  System | vs mimalloc | vs System |
> | ------ | ---------: | -------: | ------: | ----------: | --------: |
> | 16 B   | **21.0 µs** |  39.8 µs | 141.9 µs | **1.90× faster** | 6.75× faster |
> | 64 B   | **21.1 µs** |  39.4 µs | 157.9 µs | **1.87× faster** | 7.48× faster |
> | 256 B  | **21.6 µs** |  22.9 µs | 130.5 µs | **1.06× faster** | 6.05× faster |
> | 1024 B | **21.6 µs** | 165.4 µs | 118.9 µs | **7.66× faster** | 5.50× faster |
>
> **Writing churn** (each block written after alloc — the realistic pattern, headline):
>
> | size   | SeferAlloc | mimalloc |  System | vs mimalloc | vs System |
> | ------ | ---------: | -------: | ------: | ----------: | --------: |
> | 16 B   | **22.3 µs** |  39.6 µs | 129.4 µs | **1.77× faster** | 5.79× faster |
> | 64 B   | **23.3 µs** | 52.7 µs¹ | 200.3 µs | **2.26× faster** | 8.61× faster |
> | 256 B  | **29.2 µs** |  32.6 µs | 220.8 µs | **1.12× faster** | 7.56× faster |
> | 1024 B | **33.2 µs** | 230.7 µs | 238.7 µs | **6.96× faster** | 7.19× faster |
>
> ¹ mimalloc's 64 B write row was noisy this run (samples 43–65 µs); the "Sefer
> leads" signal is solid, the exact multiplier is inflated by the high tail.
>
> **Cold/bulk direct** (`alloc N → free N`, no reuse — the documented magazine worst-case):
>
> | size     | SeferAlloc | mimalloc |  System | vs mimalloc | vs System |
> | -------- | ---------: | -------: | ------: | ----------: | --------: |
> | 16 B     | 25.6 µs | **10.6 µs** | 105.5 µs | 2.41× slower | 4.12× faster |
> | 64 B     | 27.6 µs | **17.9 µs** | 146.1 µs | 1.54× slower | 5.30× faster |
> | 256 B    | 39.3 µs | **22.9 µs** | 143.7 µs | 1.71× slower | 3.66× faster |
> | 1024 B   | **41.4 µs** | 48.1 µs | 193.9 µs | **1.16× faster** | 4.68× faster |
> | Vec_push | **625 ns** | 631 ns | 570 ns | ≈parity (1.01× faster) | 1.10× slower |
>
> **Verdict (2026-07-05):** large-object work is a decisive win (12–35× vs
> mimalloc, 234–269× vs System — the large-cache flagship). On the realistic
> WRITING churn `SeferAlloc` leads mimalloc at **every** size (1.12–6.96×); on
> non-writing churn it also leads everywhere (parity-plus at 256 B). The one
> loss is **cold/bulk tiny (16–64 B)** where mimalloc's cheaper first-touch path
> leads ~1.5–2.4× — the documented magazine trade-off, kept as a regression
> guard, not a representative workload. `System` trails 3.7–269× throughout
> (the sole exception: `Vec_push`, where all three are within noise). Consistent
> with the P0–P7 history below; where absolute µs disagree, THIS section is
> current.
>
> ## 0.3.x re-measurement (2026-07-03) — after the P0–P5 perf arc
> (256 B churn caveat below is superseded by P6 — see the "P5 → P6" section)
>
> Re-run on the post-P5 tree (perf campaign #144–#149, on top of the 0.3.0
> post-review hardening #129–#143), same host and quick criterion profile
> (`sample_size(10)`, ~1 s warm-up / ~1 s measurement), noisy Windows dev
> machine (±15–20 %). `SeferAlloc` is called directly through its `GlobalAlloc`
> impl — apples-to-apples with `mimalloc 0.1` and `System`. Medians (two runs
> where a range is shown); trust the relative shape and order of magnitude, not
> exact percentages. The rigorous deterministic gate is `perf_gate_iai`
> (instruction counts, Linux CI — #127/#128/#144). The detailed Phase-11/13
> commentary below is retained for context; where it disagrees on absolute
> numbers, THIS section is the current one.
>
> **Large alloc+free — the flagship large-cache (`alloc-decommit`):**
>
> | Size | SeferAlloc | mimalloc | System | vs mimalloc | vs System |
> |---|---|---|---|---|---|
> | 4 MiB  | **~58 ns** | ~779 ns  | ~18.0 µs | ~13× | ~309× |
> | 16 MiB | **~63 ns** | ~890 ns  | ~15.3 µs | ~14× | ~242× |
> | 64 MiB | **~62 ns** | ~2.14 µs | ~18.3 µs | ~34× | ~295× |
>
> **Small class — churn (reuse) vs cold direct (first touch), post-P5:**
>
> | Size | Churn: Sefer | mi | Cold: Sefer | mi |
> |---|---|---|---|---|
> | 16 B   | ~24 µs (**1.63× faster**) | ~39 µs  | ~16–20 µs (~1.5× slower) | ~12 µs    |
> | 64 B   | ~32 µs (**1.68× faster**) | ~53 µs  | ~21–25 µs (~1.3× slower) | ~17–19 µs |
> | 256 B  | ~32 µs (1.16× slower)     | ~28 µs  | ~24 µs (≈ parity)        | ~24 µs    |
> | 1024 B | ~33 µs (**~5.9× faster**) | ~196 µs | ~25–26 µs (**~1.9× faster**) | ~46–49 µs |
>
> (Small rows are per-iteration batches — identical batch for all three
> allocators, so ratios are the signal. vs `System`: 3–6× faster throughout.)
>
> **The P0 → P5 delta story (what moved and why):**
>
> | Front | P0 baseline gap | P5 gap | lever |
> |---|---|---|---|
> | cold 16 B  | 2.6× slower | ~1.5× slower | Э1 bump-direct carve (P3) |
> | cold 64 B  | 2.0× slower | ~1.3× slower | Э1 bump-direct carve (P3) |
> | cold 256 B | 1.5× slower | ≈ parity     | Э1 + exact-256 class |
> | cold 1024 B | 1.2× faster | ~1.9× faster | (cold path, bytes-bound) |
> | churn 16 B | 1.26× faster | **1.63× faster** | Э2 + Э4 + Э5 (P1) |
> | churn 64 B | 1.23× faster | **1.68× faster** | Э2 + Э4 + Э5 (P1) |
> | churn 256 B | 1.25× slower | 1.16× slower | exact-256 class (P1) — **not overtaken** |
> | churn 1024 B | 5.8× faster | ~5.9× faster | (retained) |
>
> **What each eureka removed (all tautologies, never a guard):**
>
> - **Э1 (P3) — bump-direct batched carve, front A's main lever.** A freshly
>   bump-carved block already satisfies the M2 bitmap invariant
>   (`bit 0 = allocated`); the old refill drove every virgin block through a
>   `carve → BinTable → pop` round-trip (~40 metadata-touch instructions) that
>   moved it to "free" and instantly back to "allocated" — a tautology. The new
>   `refill_class_bump` carves a batch straight from the bump cursor into the
>   magazine (~6–8 instr/block) **without touching the bitmap** (bit 0 is
>   already correct). Freelist / ring-drain are still tried BEFORE bump-carve,
>   so freed blocks never go stale (no RSS drift). M2 byte-identical; D1 exact.
>   This roughly **halved the cold tiny-block gap** and brought cold 256 B to
>   parity. Э1 killed the carve→BinTable→pop round-trip for VIRGIN carves only;
>   what remains on the tiniest cold sizes is honest per-block metadata work on
>   the steady-state freelist-drain path (`pop_free`'s dependent `read_next` load
>   + bitmap `mark_alloc` RMW + `inc_live` per block), not page faults — the
>   criterion instance is reused across iterations, so after warm-up nothing
>   faults; the gap is instruction/ceremony on the refill path, which is exactly
>   why the P7 batch-drain (Э7) can still close it.
> - **Э2 / Э4 / Э5 (P1) — churn hit-path.** One-branch teardown resolver
>   (collapsing the `TORN` + `null` compare), classify-once (thread the size
>   class `c` through instead of recomputing `class_for` 2–3× per op), and a
>   lock-free hit counter (`load;store` instead of `lock xadd`) together
>   **widened the tiny-block churn lead** (16 B 1.26× → 1.63×, 64 B 1.23× →
>   1.68×).
> - **Exact-256 B class (P1).** `SMALL_CLASS_COUNT` 48 → 49 narrowed 256 B churn
>   from 1.25× → 1.16× slower — but did **not** overtake (see the honest ceiling
>   note below).
>
> **The honest ceiling — 256 B churn stays ~16 % behind mimalloc, by design.**
> The residual is the M2 alloc-bitmap read-modify-write on the *real* free path
> — the price of the exact double-free / foreign-free guarantee mimalloc does
> not offer, paid in full and deliberately NOT removed. Fully catching
> mimalloc's free path while keeping M2 on every substrate free would require a
> feature-gated `fast`/`hardened` split — a separate product decision (0.4+),
> not this arc. See
> [`perf/PERF_PLAN_beat_mimalloc_small_medium.md`](perf/PERF_PLAN_beat_mimalloc_small_medium.md)
> §"Honest ceiling".
>
> **Deterministic proof.** These are noisy single-host wall-clock numbers.
> The per-op instruction-count deltas of Э1–Э5 are proven deterministically by
> the `perf_gate_iai` gate (Valgrind, Linux-only CI): the P0 benches
> (`cold_alloc_free_256x16b` / `_256x64b`, `churn_256b`, #144) were added for
> exactly this; their `Ir` baseline is captured on the first Linux perf-gate
> run.
>
> **realloc / Vec:**
>
> | Bench | SeferAlloc | mimalloc | System |
> |---|---|---|---|
> | `realloc_grow_geometric` (64 B→4 MiB) | ~323 µs (**1.1× faster than mi, 8.8× than System**) | ~360 µs | ~2.85 ms |
> | `realloc_in_place_unfavorable` | ~1.68 ms (1.1× slower than mi, 4.9× faster than System) | ~1.55 ms | ~8.15 ms |
> | `Vec_push` | ~547 ns (~par) | ~496 ns | ~535 ns |
>
> **Verdict (as of P5):** large-object work is a decisive win (the shamir-db
> strength); small-object *reuse* beats mimalloc except at 256 B; cold
> first-touch of tiny blocks (16–64 B) is the documented worst-case where
> mimalloc's cheaper first-touch path leads. `System` trails everywhere by
> 3–300×. **This 256 B churn caveat was overturned in P6 — see the next
> section.**

---

## P5 → P6 — the Э6 magazine-oracle rewrite (#150–#152): the 256 B churn loss is eliminated

Same host, same quick criterion profile (`sample_size(10)`, noisy Windows dev
machine, ±15–20 %). Ratios within a run are the signal — host noise hits Sefer
and mimalloc equally; absolute µs are rough. Ratio = Sefer/mimalloc; **< 1.0 =
Sefer faster**.

### Two churn patterns — non-writing vs writing

Churn is now measured two ways:

- **`global_alloc_churn` (non-writing)** — the original bench; blocks are
  allocated and freed but **never written**. This is the artificial pattern
  where the old stale-key slow path bit hardest (the per-heap key stamped into
  the freed block's body survived untouched across the free).
- **`global_alloc_churn_write` (writing, new in P6.0)** — each block is written
  after alloc. This is **the realistic pattern**: real code writes to the
  memory it allocates. The writing table is the headline.

**Non-writing churn** (`global_alloc_churn`), median µs:

| size | Sefer (Э6) | mimalloc | System | ratio | pre-Э6 was |
|---|---|---|---|---|---|
| 16 B   | ~24 | ~41  | ~176 | 1.70× faster | 1.26× faster (P0) / 1.63× (P5) |
| 64 B   | ~26 | ~41  | ~156 | 1.57× faster | 1.23× faster (P0) |
| 256 B  | ~27 | ~28  | ~123 | **≈ parity (1.03× faster)** | **1.16–1.25× SLOWER** — loss GONE |
| 1024 B | ~27 | ~166 | ~142 | 6.24× faster | 5.8× faster |

**Writing churn** (`global_alloc_churn_write`), median µs — the realistic one:

| size | Sefer (Э6) | mimalloc | System | ratio |
|---|---|---|---|---|
| 16 B   | ~26 | ~42  | ~161 | **1.63× faster** |
| 64 B   | ~24 | ~41  | ~164 | **1.69× faster** |
| 256 B  | ~26 | ~29  | ~134 | **1.14× faster** |
| 1024 B | ~38 | ~207 | ~147 | **5.42× faster** |

On the realistic writing pattern sefer-alloc now **leads mimalloc at every
size**; even the artificial non-writing 256 B reached parity (it was 1.16–1.25×
slower through P5).

**Cold direct** (`global_alloc`, alloc N then free N, no reuse) is **unchanged
by Э6** — Э6 targets the churn free path. Note the criterion instance is reused
across iterations, so at steady state this is the freelist-refill path
(`pop_free`'s dependent `read_next` + `mark_alloc` RMW + `inc_live` per block),
not page faults; that per-block refill ceremony is what P7's batch-drain (Э7)
targets — see the two-round `recycle_*` iai benches.
Noisy medians: 16 B ~17.0 / mi ~10.6 (1.60× slower), 64 B ~22.1 / ~19.2
(1.15× slower), 256 B ~24.1 / ~23.4 (≈ parity, 1.03×), 1024 B ~23.6 / ~43.3
(1.84× faster). No claim of a cold improvement here.

### The Э6 anatomy — what actually cost us the 256 B churn (the honest eureka)

The P5 writeup above blamed the residual 256 B loss on "the M2 bitmap price".
**That framing was incomplete.** The real cost was a stale per-heap key
(`TCACHE_KEY`) stamped into the freed block's **body** (word1) and read back as
a magazine double-free fast-path filter. Two consequences:

1. **Block-body touch on every free.** Writing the key touched a cold / conflict
   cache line at the 256 B stride — the "256 B churn loss". On a non-writing
   bench the key survived across the free, so the fast-path filter matched and
   forced a **slow-path scan on every free**.
2. **Unsound under user writes.** A double-free after the user overwrote word1
   could slip past the key filter and double-issue.

**Э6 removes `TCACHE_KEY` entirely.** The two exact oracles — the in-magazine
array scan and the `BinTable` `is_free` bitmap line, **both hot metadata** — now
run unconditionally, and **the free path never touches the block body**. Net:

- **The 256 B churn loss is eliminated** (parity non-writing, 1.14× lead
  writing).
- **M2 was STRENGTHENED, not traded.** The pre-Э6 flushed-double-free-after-
  user-write hole is now CLOSED, because the oracle no longer depends on
  block-body contents. Counterfactual proof:
  `tests/regression_magazine_oracles.rs` test (c) is RED pre-Э6, GREEN on Э6.
- **Our free path is now cheaper than mimalloc's** on this pattern — mimalloc
  writes `next` into the block body on every free; we write nothing to it.

Every P0–P6 speedup removed a tautology; here the guard got *stronger*, not
weaker.

### Caveat (unchanged)

These are noisy single-host wall-clock numbers. The deterministic per-op proof
is the `perf_gate_iai` instruction-count gate on Linux CI; the new
`churn_write_256b` bench (#150) joins the P0 benches for the `Ir` baseline. The
one remaining place mimalloc leads is **cold tiny (16–64 B)** — but that gap is
per-block metadata ceremony on the steady-state freelist-drain path, NOT page
faults. At criterion steady state the SeferAlloc instance is reused across
iterations, so `bench_direct_alloc` (alloc N then free N per iteration) never
faults after warm-up: each iteration's frees flush to the BinTable and the next
iteration's allocs `pop_free` them back — the carve→BinTable→pop round-trip Э1
killed for VIRGIN carves lives on in steady state as `pop_free`'s dependent
`read_next` load + bitmap `mark_alloc` RMW + `inc_live` per block. Э6 targets the
free path, so this refill-side ceremony is untouched by it — it is what the P7
batch-drain (Э7/Э8/Э10) targets. The new two-round `recycle_alloc_free_256x16b` /
`_256x64b` iai benches (P7.0) isolate exactly this freelist-drain path (round 2
drains what round 1 freed); the single-round `cold_*` benches measure only the
virgin bump path and are blind to it. No P7 speedup is claimed yet — not measured.

---

## P6 → P7 — cold-recycle instruction batching (Э7–Э11): an instruction-count optimization, wall-clock modest on this host

Same host, same quick criterion profile (`sample_size(10)`, noisy Windows dev
machine, ±15–20 %). Ratios within a run are the signal.

**Read this section for what it is.** P7 is an **instruction-count**
optimization of the steady-state cold recycle path — the freelist round-trip
P7.0's discovery isolated (NOT page faults: at criterion steady state the
instance is reused, so after warm-up nothing faults; the cost is per-block
metadata ceremony on the refill/flush path). It batches that ceremony,
classifies once, and makes one scan branchless. **On this noisy single-host
wall-clock the cold-tiny improvement is MODEST and within noise** — we do NOT
claim the plan's projected ~1.1–1.2× as achieved. The deterministic proof is
the iai-callgrind `Ir` gate on Linux CI, and the honest verdict is framed as
**pending that gate**.

### The five eurekas that landed (all counterfactually verified byte-identical)

- **Э7 (P7.2) — batch freelist drain in `refill_class_bump`, the main cold
  lever.** The freelist of a single segment is now drained in **one walk**:
  the head-read, `set_head`, and `inc_live` are hoisted out of the per-block
  loop (one head-store and one live-count update for the whole run instead of
  per block). The genuinely per-block work — the dependent `read_next` load
  and the `mark_alloc` bitmap RMW — **stays per block** (they are the M2/D1
  guards, not ceremony). Instruction-count reduction on the recycle path; the
  drained blocks are byte-identical to what the per-block loop produced.
- **Э8 (P7.3) — batch flush in `flush_class`.** Symmetric to Э7 on the dealloc
  side: same-segment runs flush in one pass with `set_head` and the bump-load
  hoisted out of the loop. The per-block guards **stay per block**: `is_free`,
  `off >= bump`, and `dec_live` all still run once per flushed block. No guard
  was collapsed — only the shared head/bump bookkeeping was pulled out of the
  loop.
- **Э9 (P7.1) — classify-once + base-once on the `HeapCore` faces.** The
  alloc/free faces recomputed `class_for` and `segment_base_of` once more than
  necessary per op; those are now resolved once and threaded through. Same
  values, fewer loads.
- **Э10 (P7.4) — branchless chunked in-magazine M2 scan.** The in-magazine
  double-free oracle (the Э6 array scan) is now a branchless chunked scan —
  same exact membership test, no per-element branch. M2 membership is
  byte-identical; the scan bounds are counterfactually pinned.
- **Э11 (P7.2) — stamp-dedupe.** A redundant owner-stamp on the batched drain
  path was de-duplicated (the segment is stamped once for the drained run, not
  per block). Same stamp result.

### Measured — cold direct (`global_alloc`, the steady-state recycle path Э7/Э8 target), median µs

Ratio = Sefer/mimalloc; **< 1.0 = Sefer faster**. These are noisy medians —
the 16 B row bounced 18–24 µs across samples on this host.

| size   | Sefer (post-P7) | mimalloc | vs mimalloc | pre-P7 was |
|---|---|---|---|---|
| 16 B   | ~21 (noisy 18–24) | ~14   | ~1.5× slower | 1.60× slower |
| 64 B   | ~23             | ~20     | ~1.16× slower | 1.15× slower |
| 256 B  | ~25             | ~26.5   | **~1.06× faster** | ≈ parity |
| 1024 B | ~34             | ~54     | **~1.6× faster** | 1.84× faster |

**Honest reading:** cold 16 B moved from 1.60× → ~1.5× slower and cold 256 B
from parity to a slight lead — but **both are within this host's run-to-run
noise** (16 B alone spanned 18–24 µs). 64 B is unchanged (~1.15×). The
wall-clock on this machine **cannot cleanly resolve** the per-op instruction
savings Э7/Э8/Э9/Э10 make. We do NOT claim the projected ~1.1–1.2× cold-tiny
figure as demonstrated by these numbers — it is not.

### Churn (the won front) — UNREGRESSED (must-not-regress gate)

Non-writing churn (`global_alloc_churn`), median µs, spot-checked post-P7:

| size  | Sefer | mimalloc | ratio | was (P6) |
|---|---|---|---|---|
| 16 B  | ~26 | ~42 | **~1.64× faster** | 1.70× faster |
| 256 B | ~34 | ~34 | ≈ parity | ≈ parity (1.03×) |

Churn stands where P6 left it (16 B still ~1.6× faster, 256 B still ≈ parity) —
P7 did not regress the front we already won.

### The verdict — instruction-count win, deterministic proof PENDING the Linux Ir gate

- **What P7 is:** an instruction-count reduction on the steady-state cold
  recycle path (batch the freelist drain/flush, classify once, branchless
  scan). Every change is proven **byte-identical** by counterfactual regression
  tests — D1 exactness, M2 in-magazine guards, splice correctness, scan bounds.
- **What the wall-clock shows on THIS host:** only a modest, within-noise
  cold-tiny improvement (16 B 1.60× → ~1.5× slower; 256 B parity → slight lead;
  64 B unchanged). Said plainly: **the noisy single-host wall-clock cannot
  resolve the per-op savings.** No cold-tiny speedup is claimed as
  demonstrated.
- **The real proof is deterministic and pending.** The iai-callgrind `Ir` gate
  on Linux CI, via the two-round `recycle_alloc_free_256x16b` / `_256x64b`
  benches (P7.0), isolates exactly this path (round 2 drains what round 1
  freed) and will show the per-op `Ir` delta after push. Frame the P7 verdict
  as **"pending the Linux Ir gate"** — that is the honest state.
- **Churn is unregressed** (16 B ~1.6× faster, 256 B ≈ parity, as above).
- **Guarantees intact.** The batching removed only shared-bookkeeping
  tautologies and kept every guard per-block: `is_free`, `off >= bump`,
  `mark_alloc`, `dec_live` all still run once per block; M2 / D1 / A1 /
  `#![forbid(unsafe_code)]` at the top level are all intact.

### Caveat (unchanged)

Noisy single-host wall-clock. The deterministic per-op proof is the
`perf_gate_iai` instruction-count gate on Linux CI (the `recycle_*` benches
above). No P7 wall-clock speedup is claimed on this host beyond "modest,
within noise".

---

## Historical detail (Phase 11 / 13.4a)

`SeferMalloc` (feature `alloc-global`) is the **malloc face** of `sefer-alloc`:
an `unsafe impl GlobalAlloc` over the per-thread segment heap (Phase 8 segment
substrate + Phase 9 intrusive free-list hot path + Phase 10 cross-thread free
under `alloc-xthread`). One substrate, two faces — the typed `Handle` face and
this raw `*mut u8` drop-in face.

This is the honest measurement the campaign promised: *"as fast as the best, and
safe."* Stated plainly, win or lose.

## What was measured — single-threaded churn

`benches/global_alloc.rs` (quick criterion profile: `sample_size(10)`, 150 ms
warm-up, 600 ms measurement). All three allocators are driven **through their
`GlobalAlloc` API directly** in one binary — a true apples-to-apples comparison
of the alloc/dealloc hot path (`SeferMalloc` is not installed as the bench
binary's global allocator; it is called directly, exactly like `mimalloc` and
`System`).

- **direct churn:** `OPS = 1024` alloc+dealloc pairs of a fixed layout.
- **`Vec_push`:** a realistic growing-vector pattern (repeated alloc + realloc +
  copy as a 512-element `i64` vector doubles its capacity).

Host: Windows 10, dev machine. Numbers are medians; the quick profile is for
*relative standing*, not precision. **Re-measured at Phase 13.4a** (see the
evolution note below) — these are current numbers, not the stale Phase-11 ones.

| workload          | SeferMalloc | mimalloc | System (Win) |
| ----------------- | ----------: | -------: | -----------: |
| 16 B churn        |     ~24.5 µs |  ~11.0 µs |     ~110 µs |
| 64 B churn        |     ~30.6 µs |  ~20.2 µs |     ~144 µs |
| 256 B churn       |     ~28.7 µs |  ~22.8 µs |     ~132 µs |
| 1024 B churn      | **~30.5 µs** |  ~34.8 µs |     ~116 µs |
| `Vec` push/grow   |  **~496 ns** |  ~515 ns |     ~543 ns |

### Evolution of the single-thread numbers (why they moved since Phase 11)

The Phase-11 table claimed 16 B churn at ~21.5 µs. That number predated two
substrate changes: (a) the **registry inversion** of Phase 12.5 (raw-pointer TLS
→ a process-global heap registry with a never-null fallback; the per-call
`current_for_alloc()` now does a registry hop), and (b) the **Phase 13.4a**
double-free guard rework (an O(1) bitmap that replaced an accidental O(N²)
scan — a *correctness/perf fix*, not a regression). Net, steady-state 16 B churn
is now ~22–25 µs (the TLS/registry hop is a small constant added per call). The
1024 B and `Vec_push` standings — where `SeferMalloc` is at or ahead of
mimalloc — are unchanged in character. The old 21.5 µs figure is retired; the
table above is the honest current state.

### Bulk-vs-churn: what the "churn" row above actually measures

The table above uses `bench_direct_alloc` (`alloc 1024 → free 1024`). Strictly
speaking that is a **bulk** pattern, not a steady-state churn. Real workloads
interleave allocs and frees over a bounded working set. The bulk pattern is
the documented **worst case** for the per-thread magazine (`fastbin`,
default-on in `production`): every free overflows the magazine, every alloc
empties it. The 16 B/64 B/256 B bulk regression (~1.8–2.9× slower than
`mimalloc`) is the magazine's design trade-off, kept as a regression guard
rather than a representative workload.

A true steady-state churn microbench (`bench_churn_alloc`, added in P0 of
the `fastbin` project — `benches/global_alloc.rs`, group `global_alloc_churn`)
maintains a working set of 256 live blocks and frees/replaces one at random
per iteration. On that workload `SeferMalloc` is competitive-to-strongly-ahead:

| size | SeferMalloc | mimalloc | vs mimalloc |
| ---- | ----------: | -------: | -----------: |
|   16 B |  ~21.8 µs |  ~36.9 µs | **1.7× faster** |
|   64 B |  ~22.3 µs |  ~37.2 µs | **1.7× faster** |
|  256 B |  ~21.9 µs |  ~22.1 µs | parity |
| 1024 B |  ~21.9 µs |   ~159 µs | **7.3× faster** |

See [`FASTBIN_DESIGN.md`](FASTBIN_DESIGN.md) for the design and the full
P0 → P6 measurement history (sweep, win/loss ledger, production
decision).

## What was measured — multi-threaded macro-benchmark (Phase 13.7)

`examples/malloc_macro.rs` (run:
`cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"`).
This is a **standalone harness**, NOT a criterion micro-loop: criterion's
per-iter model mis-measures MT work (thread-spawn inside the timed closure
dominates). Threads are pre-spawned, aligned on a `Barrier`, then a fixed op
budget (400 k steps/thread) is run and the steady-state region is timed with
`Instant::elapsed`. PRNG is a dependency-free xorshift with fixed per-thread
seeds (reproducible; no `rand` crate added).

Two workloads, both reporting **aggregate ops/sec** (an op = one alloc+free
pair), sweeping T = 1, 2, 4 threads:

- **larson** (server-churn): each thread keeps a ~768-slot working set; each
  step frees a random slot and re-allocates a random small-skewed size
  (16..512 B, ~3% up to 8 KiB). Every 16th step a block is **handed to another
  thread and freed there** (cross-thread free, routed through the Phase-10/12.6
  remote path under `alloc-xthread`).
- **mstress**: rounds of "fill 512 mixed blocks → free half in random order
  (~1/8 cross-thread) → refill → free all".

Cross-thread handoff is leak/UAF-free by construction: a handed-off block is
moved out of the producer's slot before being sent over an `mpsc` channel; the
consumer is its sole owner and frees it exactly once; each worker drains its
mailbox before joining. Every block is freed exactly once by exactly one thread.

Numbers are **million ops/sec** (higher is better), Windows 10 dev machine,
representative run (run-to-run ±10%; the *ordering* is stable):

**larson**

| T | SeferMalloc | mimalloc | System |
| -: | ----------: | -------: | -----: |
| 1 |   ~21 M     |  ~27 M   | ~6.6 M |
| 2 | **~25 M**   |  ~19 M   | ~7.2 M |
| 4 | **~40 M**   |  ~32 M   | ~14 M  |

**mstress**

| T | SeferMalloc | mimalloc | System |
| -: | ----------: | -------: | -----: |
| 1 |   ~25 M     |  ~33 M   | ~4.5 M |
| 2 |   ~43 M     |  ~43 M   | ~6.1 M |
| 4 |   ~65 M     |  ~65 M   | ~9.5 M |

**RSS:** not measured here. There is no portable, dependency-free peak-RSS probe
across Windows/Linux/macOS without pulling in platform syscalls, so the harness
honestly reports N/A rather than inventing a number. (RSS comparison is a
hardening-gate item. Note: as of 0.3.0 the `alloc-decommit` feature — included
in the `production` set — DOES decommit empty small segments to the OS; the
prose above is stale from 0.2.0 when decommit was not yet shipped.)

### Honest reading of the MT numbers

- **Single thread, mimalloc leads** (both larson and mstress): ~25 % faster on
  larson, ~30 % on mstress. Same story as the single-thread churn table — the
  per-call `classify` + TLS/registry hop is a constant mimalloc's hand-tuned
  inlining shaves.
- **At T = 2 and T = 4, `SeferMalloc` catches and on larson *passes* mimalloc.**
  On larson it is ahead at both 2 and 4 threads (the per-thread heap means the
  fast path takes no shared lock; the cross-thread frees route through the
  per-segment remote path without contending the producer). On mstress the two
  are within noise of each other at 2 and 4 threads.
- **`SeferMalloc` scales cleanly with thread count** — larson goes ~21→25→40 M
  (1→2→4 T), mstress ~25→43→65 M. mimalloc's larson number actually *dips* at
  T = 2 on this host (~27→19 M) before recovering at T = 4; `SeferMalloc` does
  not show that dip. System is flat and ~3–7× behind throughout.
- This is the payoff of the per-thread-heap architecture: the single-thread
  constant is a real gap, but it does **not** compound under contention, so by a
  handful of threads the safe-by-construction allocator is competitive-to-ahead.

### Heap == core pinning (Phase 13.6) — honest verdict: no win on this host

A heap is bound to its thread via TLS (`current_for_alloc`), so pinning the
**worker thread** to a fixed core keeps that heap's segments warm in one core's
cache without the allocator having to change anything — the per-thread-heap
analogue of the Phase-7c sharded thread-per-core topology. The macro-bench grew
an opt-in **pinned mode** (feature `pinning`; reuses the Phase-7c
`core_affinity` organ through `PinnedRunner::pin_current_thread_to_core` /
`available_cores` — no new dependency, no `unsafe`). Run:

```
cargo run --release --example malloc_macro \
  --features "alloc-global alloc-xthread pinning"
```

It runs the **whole sweep twice in one process** (unpinned baseline, then
pinned) so the two modes see the same warm machine state. Without the `pinning`
feature the example is byte-for-byte the pre-13.6 single (unpinned) sweep.

Measured on this host (**Windows 10, 16 logical cores**, 1/2/4 workers,
representative of two consecutive runs; M ops/sec, higher better):

**larson — SeferMalloc**

| T | unpinned | pinned |
| -: | -------: | -----: |
| 1 |   ~19 M  | ~20 M  |
| 2 | **~24 M**| ~21 M  |
| 4 | **~42 M**| ~32 M  |

**mstress — SeferMalloc**

| T | unpinned | pinned |
| -: | -------: | -----: |
| 1 |   ~25 M  | ~23 M  |
| 2 | **~41 M**| ~27 M  |
| 4 | **~63 M**| ~48 M  |

**Pinning does NOT help here — it hurts, the more so as T grows** (the same
trend holds for mimalloc and System in the pinned tables). This is the expected
outcome on a **16-core box running only 1–4 workers**: the OS scheduler has
plenty of idle cores and places the few hot threads well on its own. Forcing
worker *i* onto a fixed core id removes that freedom — round-robin from core 0
on an SMT machine can co-schedule two workers on the two hyperthreads of one
physical core (sharing its L1/L2), and pinning also blocks the scheduler from
migrating a worker off a core that the OS or another process is using. The
"keep the heap cache-warm" benefit a pinned topology *can* give (many workers,
core-count-saturated, cache-hot per-thread working sets) is simply not the
regime a 16-core dev box with ≤4 workers is in.

**Decision (honest keep-or-document):** the pinned mode is **kept as an opt-in
tool**, not made the default — exactly because the measurement says it does not
help *on this machine*. It is the right primitive to reach for on a
**core-saturated** deployment (workers ≈ cores, thread-per-core executors like
glommio/monoio, NUMA boxes) where keeping a heap's segments resident on one
core's cache and avoiding cross-NUMA migration genuinely pays — but that regime
must be measured on the target host, not assumed. On this 16-core Windows dev
machine with few workers, leaving the OS scheduler in charge (the default,
unpinned path) is the faster choice. No numbers were tuned to flatter the
feature; the slowdown is reported as found.

## Honest verdict

**The architecture delivers: `SeferMalloc` is competitive with `mimalloc` and
consistently far faster than the Windows system allocator — while being safe by
construction** (the intelligence is pure safe integer arithmetic; `unsafe` lives
only in the two audited seams + the `GlobalAlloc` impl).

1. **Competitive with mimalloc, and it wins on real patterns.** On small
   fixed-size churn (16–256 B) `SeferMalloc` is ~1.4–2.2× behind mimalloc — the
   gap is the per-call `classify` + TLS/registry hop that mimalloc shaves with
   hand-tuned inlining. But at **1024 B it is faster than mimalloc**, and on the
   realistic **`Vec` push/grow** pattern (the allocation shape real programs
   actually produce) it **beats both mimalloc and System**. The single-writer
   per-thread free-list pop is the same shape as the best allocators, so the
   remaining gap is a small constant, not an order of magnitude.
1b. **Multi-threaded: the single-thread gap does NOT compound (Phase 13.7).**
   The new MT macro-bench (`examples/malloc_macro.rs`, larson + mstress, 1/2/4
   threads, with cross-thread free) shows mimalloc ahead at T = 1, but at
   **T = 2 and T = 4 `SeferMalloc` catches it — and on larson passes it** (e.g.
   ~40 M vs ~32 M ops/sec at 4 threads). `SeferMalloc` scales monotonically with
   thread count; the per-thread heap takes no shared lock on the fast path, so
   contention does not magnify the per-call constant. System trails ~3–7×
   throughout. RSS is not yet measured (no decommit; see the macro-bench section).
2. **Dramatically faster than the OS allocator.** Across every size, `SeferMalloc`
   is **~2.5–5× faster than the Windows system allocator** — the practical win
   for a Windows service that currently pays the system-allocator tax.
3. This is a huge improvement over the Phase-9 `Heap` micro-bench (which showed
   ~7–12× behind mimalloc); measuring the steady-state hot path through the
   `GlobalAlloc` face shows the true standing.

## NOT yet production-trusted — the remaining hardening gate

> **Historical note (dated 2026-07, retained for context)** — this
> section describes the Phase-12.5/12.6 hardening gate as it stood
> in 0.2.0. As of 0.3.0 the gate is **closed** — fuzz, aarch64,
> ThreadSanitizer, and miri are all wired in CI; the `production`
> feature is the recommended set for long-running multi-thread
> deployments. The historical body below is kept for context.

Per `ALLOC_PLAN.md` §5 P11 / §8, production trust is earned only after the full
hardening gate. What works today and what remains:

**Works and is verified:**
- The `GlobalAlloc` face serves correct, aligned, non-overlapping, reusable
  memory — proven by `tests/global_alloc.rs` (direct-API, pattern write/read-back,
  no-overlap, realloc-prefix, 20 k churn) and the Phase 8/9 differential proptests
  + miri.
- It runs as a real `#[global_allocator]` for a single-threaded application
  workload end-to-end — `examples/global_allocator.rs` sums a 100 k-element `Vec`
  and fills a 10 k-entry `HashMap` entirely through `SeferMalloc`.
- Reentrancy-freedom (M5) and no-panic on the alloc path: the substrate's panic
  sites were hardened (the `alloc_small` `.expect`, `SegmentTable::register` and
  `Segment::reserve` now return gracefully; the TLS binding has a no-panic
  `with_heap_try` variant).

**Remaining (NOT done — do not deploy as a process-wide allocator yet):**
- **Robust process-wide `#[global_allocator]` under hostile runtimes.**
  RESOLVED in Phase 12.5 (the shard-model turn). The raw-pointer TLS + global
  heap registry + never-null fallback heap make the face reentrancy-safe (M5)
  and never-null (M10) under the multithreaded libtest harness — parallel test
  threads, panic hooks, thread teardown. The headline MT gate
  (`tests/global_alloc_mt.rs`) installs `SeferMalloc` as the
  `#[global_allocator]` and runs `Vec`/`String`/`HashMap`/`Box` churn across
  threads that spawn AND exit mid-allocation, plus a cross-thread free channel
  test (under `alloc-xthread`), all green. This removes the "abort under
  reentrant/teardown access" caveat. **Caveat that REMAINS:** the heavy
  hardening gate (fuzz / aarch64 weak-memory CI / TSan) has not been run, so
  "production-trusted on every target" is still pending that gate (§4 of
  `ALLOC_PLAN_PHASE12-13.md`).
- Cross-thread free requires the `alloc-xthread` feature. Phase 12.5 ships the
  shard model (a heap is a shard; thread death releases the slot, the HeapCore
  stays whole for the next claimant); **Phase 12.6 makes cross-thread free
  RECLAIM** — the freer pushes the block's `offset | class` into a non-intrusive
  per-segment ring; the owner reclaims it into the `BinTable` lazily on its
  alloc-slow-path (`find_segment_with_free`). See `RACE_DRAIN_RECLAIM.md`
  §13/§14: the true root was that `page_map`'s per-page class is unreliable for
  the mixed-class pages a shared bump cursor produces, so cross-thread reclaim
  must carry the class from the freer's `Layout` (not derive it). The
  Phase-12.5 discard-leak is **gone**: cross-thread-freed blocks are reused; RSS
  is bounded (only an in-flight ring's worth may sit un-reclaimed until the next
  slow-path drain). Verified on Windows + Linux (ThreadSanitizer-clean — it was
  a class-derivation logic bug, not a data race). M6 decommit (return empty
  segments to the OS) + M11 remain deferred behind a future `alloc-decommit`
  flag (#35). The single-thread `Heap` path fully reuses.
- The heavy correctness gate — `cargo-fuzz` (CPU-hours over adversarial
  alloc/free/realloc streams), **aarch64** multi-arch CI (the weak memory model
  x86 hides), and **ThreadSanitizer** under stress — needs CI / non-Windows
  hosts and has not been run.

## Bottom line

`SeferMalloc` proves the thesis: a safe-by-construction allocator can be
*competitive with mimalloc* (and beat it on realistic patterns) while being
*much faster than the OS allocator*. As of Phase 12.5 it is a **working,
multithreaded drop-in `#[global_allocator]`** — the reentrancy/teardown abort
that blocked MT use is closed, and the headline MT gate runs green with thread
churn + cross-thread free, and (Phase 12.6) cross-thread-freed blocks are now
**reclaimed** (no discard-leak). One honest remainder: the heavy hardening gate
(fuzz / aarch64 / TSan-under-stress) is still pending, so "production-trusted on
every target" is not yet claimed — though the cross-thread reclaim path is now
ThreadSanitizer-verified on Linux. For a process-wide allocator right now,
`SeferMalloc` is a viable MT choice; the remaining gap to `mimalloc` is the
multi-arch / CPU-hours hardening gate, not a known correctness or RSS issue.

---

## NUMA — opt-in locality steering (Phase B–E of #58)

### What was added

Feature flag `numa-aware = ["alloc-core"]`, **default OFF**.  Enabling it
steers new segment reservations to the NUMA node of the calling thread:

- **Linux**: `mbind(2)` with `MPOL_PREFERRED` — called after `mmap`, before
  first page access, so page-faults bring physical pages from the preferred
  node.
- **Windows**: `VirtualAllocExNuma` replaces `VirtualAlloc` — the preferred
  node is passed at reservation time (Windows has no `mbind` equivalent for
  already-reserved ranges).
- **macOS / miri**: no-op — Apple Silicon is UMA; there is no public NUMA
  syscall.  `current_node()` returns `NO_NODE` (`u32::MAX`), `bind_segment`
  is a no-op compile-time constant.

Without the flag the build is **byte-for-byte unchanged** — no new code
executes, no layout shifts (the `node_id: u32` field in `SegmentHeader` is
always compiled in to keep the struct layout stable across feature configs, but
is initialised to `NO_NODE` and never read without `numa-aware`).

### Integration points

| Location | Change |
|---|---|
| `src/alloc_core/numa.rs` | Confined-`unsafe` OS seam: `current_node()`, `bind_segment()`, `reserve_aligned_on_node()` |
| `src/alloc_core/segment_header.rs` | `node_id: u32` field (+4 bytes; still ≪ PAGE) |
| `src/alloc_core/alloc_core.rs` | `reserve_small_segment` stamps `node_id`; `find_segment_with_free` prefers same-node segments; `alloc_large` steers large segments |

### What is verified

| Test file | What it checks |
|---|---|
| `tests/numa_seam.rs` | `current_node()` returns `NO_NODE` or `< 64`; `bind_segment` with `NO_NODE` / zero len is a no-op; `reserve_aligned_on_node` returns a SEGMENT-aligned pointer |
| `tests/numa_segment_id.rs` | Small and large `AllocCore` allocations carry `node_id == current_node()` at allocation time |
| `tests/numa_alloc.rs` | Integration: same-thread segments share `node_id`; cross-thread free across potential node boundaries does not panic or corrupt; two-thread consistency of `stamped == observed` |

`tests/numa_alloc.rs` is **ENV-guarded** (`SEFER_NUMA_TEST=1`) — without the
variable every test body returns immediately (passes on CI single-NUMA
machines).  To execute the full test:

```sh
SEFER_NUMA_TEST=1 \
  cargo test \
    --features "alloc-core alloc-global alloc-xthread alloc-decommit numa-aware" \
    --test numa_alloc
```

Run inside a QEMU VM with `-numa node,...` or on a kernel booted with
`numa=fake=N` to exercise a real multi-node topology.

### What is NOT verified — honest N/A

**Latency-reduction numbers are not in this table.** The benefit of local-node
page allocation is a *memory-access latency* reduction — measurable only when
the workload's data fits in a NUMA node's local DRAM and accesses to remote
DRAM are the bottleneck.  That regime requires **real multi-socket hardware**:

- AWS `c5n.metal` / `i3.metal` (Xeon, 2 physical sockets)
- AWS `r6g.metal` (Graviton 2, multiple NUMA domains)
- Dual-socket development box

QEMU `-numa` / `numa=fake` verify correctness of the `mbind` / stamping path
but cannot reproduce the physical latency asymmetry (all "nodes" share the same
physical socket and DRAM controllers).  Until measurements on real hardware are
available the NUMA latency column is **N/A**.

### Synergy with the `pinning` feature

`numa-aware` alone is **best-effort**: if the OS migrates a thread to a
different NUMA node after its initial segment was reserved, subsequent accesses
to that segment will be cross-node.  The allocator uses strategy (a) — ignore
migration, steer only NEW reservations — as the MVP policy.

Combining `numa-aware + pinning` makes the locality **deterministic**: the
`pinning` feature (via `core_affinity`) pins each worker thread to a specific
core for the duration of its run.  A pinned thread on core *k* always resides
on the same NUMA node, so every new segment it reserves is local.

```sh
cargo run --release --example malloc_macro \
  --features "alloc-global alloc-xthread pinning numa-aware"
```

Without `pinning`, `numa-aware` helps under workloads with low thread
migration (e.g. long-lived worker threads, DBMS executors) but is not a
guarantee.  With `pinning`, locality is guaranteed for the lifetime of the
pinned run.

---

## Large-cache (OPT-E) — `alloc-decommit` required

### What was added

Feature-gated on `alloc-decommit`, `AllocCore` holds a small fixed-size
free-cache for large segments (`LARGE_CACHE_SLOTS = 8`). When a large
allocation is freed, instead of releasing the OS reservation immediately
the segment is deposited into the cache (reservation stays live, pages
stay committed — no decommit on deposit, so no recommit is needed on hit).
The next `alloc_large` of a compatible size
(`needed <= cached_size <= needed * 2`) hits the cache, skipping the OS
mmap/VirtualAlloc entirely.

**Admission policy (per-shard byte budget, #90–#95):** the original
per-span cap `MAX_CACHED_LARGE_BYTES = 64 MiB` was removed in #90 — it
prevented caching large spans on machines that have the headroom. The
cache is now byte-budget'd per shard (default unbounded — clients
override via the `LargeCacheConfig` const builder,
`.budget_bytes(N)`, passed through `SeferAlloc::with_config(...)` /
`AllocCore::new_with_config(...)`). Lazy 10 %/sec exponential decay
back to `live + headroom` (headroom default 256 MiB, override via
`.headroom_bytes(N)`) keeps the cache bounded
without a background thread. FIFO eviction on budget overflow.

The OS reservation is released either on the next `Drop` of `AllocCore`
(if the cached segment is never reused) or when the decay/budget logic
evicts the slot.

### Numbers — `benches/large_realloc.rs`, `large_alloc_free` group

Run: `cargo bench --bench large_realloc --features "alloc-global alloc-decommit" -- large_alloc_free`

Host: Windows 10, dev machine. Numbers are medians from criterion `sample_size(10)`.

**Before OPT-E** (`--features alloc-global`, no cache):

| size  | SeferMalloc | mimalloc  | System    |
| ----- | ----------: | --------: | --------: |
| 4 MiB |   ~237 µs   |  ~753 ns  |  ~18.7 µs |
| 16 MiB|   ~657 µs   |  ~851 ns  |  ~17.5 µs |
| 64 MiB|  ~1.97 ms   |  ~2.0 µs  |  ~18.3 µs |

**After OPT-E + byte-budget admission (current state, post-#90/#94/#95):**
`--features "alloc-global alloc-decommit"`:

| size  | SeferMalloc (cache hit) | mimalloc | System   | vs mimalloc | speedup vs before |
| ----- | -----------------------: | -------: | -------: | ----------: | ----------------: |
| 4 MiB |                **~46 ns** | ~743 ns  | ~17.5 µs |  **~16× faster** |        **~5,200×** |
| 16 MiB|                **~46 ns** | ~861 ns  | ~14.6 µs |  **~19× faster** |       **~14,300×** |
| 64 MiB|                **~63 ns** | ~2.43 µs | ~16.9 µs |  **~39× faster** |       **~31,300×** |

At all three sizes the cache eliminates the OS round-trip entirely. The
cached path is: scan the `LARGE_CACHE_SLOTS` (8) cache slots (bounded, O(1)), call `table.register` (O(1) for the
recycled NULL slot), write a 96-byte `SegmentHeader` struct, return a
pointer. No syscall, no page-table work.

**64 MiB is now cached** (it was not under the original per-span cap;
#90 removed the cap). The per-shard byte budget admits any single span
as long as the budget allows it; clients who want a hard cap can set
`LargeCacheConfig::budget_bytes(N)` via
`SeferAlloc::with_config(...)` (env vars were removed in 0.2.0).
See [`ALLOC_PLAN_PHASE12-13.md`] /
checkpoint notes for the redesign rationale.

### Why pages are kept committed (no decommit on deposit)

An earlier version decommitted the payload pages on cache deposit
(`VirtualFree(MEM_DECOMMIT)`) and recommitted on cache hit
(`VirtualAlloc(MEM_COMMIT)`). On Windows, committing 8 MiB of pages costs
~50 µs regardless of the warm/cold state — essentially the same as a full
mmap round-trip. Removing the decommit/recommit pair dropped the 4 MiB hit
from ~50 µs to ~45 ns: a 1,100× additional improvement.

Trade-off: cached segments hold their pages committed between uses, increasing
RSS by `usable_size` per cached slot. There is no fixed per-span size cap:
admission is governed by the configurable `LargeCacheConfig::budget_bytes`
(default unbounded); the fixed count is the `LARGE_CACHE_SLOTS = 8` slot
array. For workloads that alloc/free large blocks infrequently, the
`alloc-decommit` feature without OPT-E (or a future time-based eviction) is
preferable. OPT-E is optimal for workloads with repeated large-allocation churn
at the same size class.

### Known limitation — `MPOL_PREFERRED` not `MPOL_BIND`

The current Linux implementation uses `MPOL_PREFERRED` (mode 1): the kernel
*prefers* to allocate physical pages from the requested node but falls back to
any available node under memory pressure.  This avoids OOM-abort on saturated
NUMA nodes at the cost of occasional cross-node pages.

If strict binding (`MPOL_BIND`, mode 2 — pages must come from the requested
node or OOM) is needed, switching is a one-line change in `numa.rs` and is a
planned follow-up, not part of this phase.

### RSS impact

NUMA steering does not increase or decrease the number of segments reserved —
it only influences *which physical pages* back those segments.  The RSS profile
is unchanged.  The `alloc-decommit` feature (Phase 35) remains the correct
lever for RSS reduction.
