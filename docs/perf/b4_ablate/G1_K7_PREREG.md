# G1-K7 — pre-registration (fixed before any J1′ v2 run of this change)

Shipping candidate: the budgeted depot overflow
(`LargeCacheConfig::depot_budget_bytes`, default 1 MiB per heap), design
`docs/design/2026-10-28-depot-budget-design.md` rev. 2 on branch `g1-k7`.
Base: `g1-k5`@`2bc830cc` (main `37780bc7` + G1-K1 + G1-K5). Measurement
branch `g1-k7-meas` = the ablation harness of `g1-p0-meas` (`aab1a2b0`) +
the two new judge kernels (`bulk64k`, `bulk16k_32`).

Variant (patch in `docs/perf/b4_ablate/patches/`):

- `k7_depot_overflow` — `git diff 2bc830cc 2680410a -- src` (the whole
  K7 change: overflow storage, decay, inline gate, diagnostics, Kani).
  Patch sha256 `84ac238d50e833bd636e1330d21b2b5f7127ffe4c8baa035f4946c6524f941f8`.

Decision source: the owner's acceptance rule (2026-10-28) — «приём — любой
выигрыш против mimalloc без проигрышей на замерах других типов», and
without a safety loss.

## Runs, in this order

1. **Ablation, J1′ v2** (`scripts/b4_ablate_run.mjs`, 20 processes × 2000
   blocks, all 13 kernels: pair16, pair64, pair_rt, bulk64, bulk16k,
   churn16, churn64, churn256, churn1024, vec512, larson, bulk64k,
   bulk16k_32), effect = variant median − null median
   (`scripts/b4_ablate_vs_null.mjs`), 95 % CI by process bootstrap. Arm A
   = K5, arm B = K7 at the default budget, same process.
2. **Main judge against mimalloc** (`scripts/b4_interleave_run.mjs
   --reps 20 --blocks 2000`, all 13 kernels) on the `g1-k7` tree and, in
   the same session right after it, on the `g1-k5` tree with the two new
   kernels (so both trees are judged against mimalloc on the same day).
3. **RSS judge** — subprocess per arm (K5; K7 at E = 0, 512 KiB, 1 MiB,
   2 MiB; mimalloc), only if run 1 and run 2 show a win.

## Decision rule

GO (ship as `perf(runtime)`) only if ALL hold:

1. Win: at least one admitted kernel of run 2 where K7's ratio
   t_mimalloc / t_sefer is higher than K5's with the two process-bootstrap
   CIs not overlapping, and in run 1 the same kernel's effect has CI upper
   < 0.
2. No loss on speed: no kernel of run 1 with CI lower > 0 (K7 slower than
   K5); no kernel of run 2 where K7's ratio is lower than K5's with
   non-overlapping CIs.
3. No loss on RSS (run 3): no snapshot worse than K5 outside noise, except
   retention inside the declared budget (E + overflow metadata), which is
   published as a number.
4. No loss on Ir (`npm run iai` against the base) beyond the run-1
   evidence; the default change refreshes `bench:table` and `iai`.
5. Per-arm evidence (R26-4 / R30-8): requested, resolved and effective
   budget read back through `#[global_allocator]`
   (`SeferAlloc::dbg_current_depot_overflow`), `config_conflicts_total`
   delta 0, process boundary; activation oracle — overflow parks > 0 and
   reloads ≈ parks on bulk16k / bulk16k_32, budget refusals or ring-full
   gating on bulk64k, decay flushes 0 on the bulk kernels.

NO-GO otherwise; a partial result (win with a loss elsewhere) goes to the
@oh and rush consultation, not to a merge.

Expected (engineering target, not a gate): bulk16k ≥ 75 % of the P0
ceiling effect (P0: −56.13 in 100·ln units); bulk64k ≈ K5 (4 MiB live is
over the budget: one class's ring fills and the rest flushes as in K5).
Published caveat: E = 1 MiB covers bulk16k with ≈ 9 KiB to spare (needs
≈ 1,039,360 B) — tuned to the judge; the RSS sweep shows the behaviour
past the threshold.
