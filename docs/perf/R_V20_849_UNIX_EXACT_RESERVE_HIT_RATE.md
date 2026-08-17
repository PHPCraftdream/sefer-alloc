# R-V20-849 — Unix exact-reserve hit rate measurement (aligned-vmem)

**Task:** task #849 (bench-only, observation-only). Measurement of `try_reserve_aligned_exact`'s hit rate across 4 alignment regimes on Unix/WSL2, to inform whether the V20/P17 align-threshold guard proposal should be implemented. This is the "measure first" half of the project's two-step convention — no guard was implemented, only diagnostic counters were read. The ~57% measured hit rate (not the predicted ~0.1%) means the fast path is very likely still a net win even for large aligns on this platform+kernel, so the guard's premise does not hold here.

**Date:** 2026-08-12. **Base revision:** `main` @ `76ac08f` (task #842) plus this task's own uncommitted diff (`crates/aligned-vmem/examples/v20_849_unix_exact_reserve_hit_rate.rs`, `crates/aligned-vmem/Cargo.toml`). (Resolved: this diff was committed as `35d51e6` — see the reproduction section for the exact commit hash.)

---

## 1. What was measured

Added a new `bench-internals`-gated example, `crates/aligned-vmem/examples/v20_849_unix_exact_reserve_hit_rate.rs`, using the existing `unix_exact_reserve_attempts()`/`unix_exact_reserve_hits()`/`reset_bench_internals_counters()` diagnostic counters (added in task #504). The example measures `try_reserve_aligned_exact`'s hit rate across 4 alignment regimes:

1. **page-size (4 KiB)** — the trivial 100%-by-construction case (any mmap result is already page-aligned).
2. **existing default arm (64 KiB)** — the same alignment the existing benchmark uses.
3. **existing large arm (1 MiB)** — another alignment already in the benchmark suite.
4. **V20/P17 flagship regime (4 MiB)** — align = size = 4 MiB, matching sefer-alloc's own `SEGMENT`-aligned-`SEGMENT` allocation pattern.

Each run performs `ITERS=16` reservations, holds them all before releasing (to avoid address-reuse artifacts), prints hits/attempts per regime, and resets the counters via `reset_bench_internals_counters()` before exiting.

---

## 2. Methodology corrections (two false starts before a valid measurement)

### 2.1 First attempt: alloc-then-free loop — DISCARDED

The initial design allocated a reservation, immediately freed it, and repeated 2000 times in a single process. This measured ONE address's alignment residue 2000 times, not 2000 independent trials — every call reused the address just freed by the previous call, since nothing else allocated VA in between. This produced a spurious steady-state 100% hit rate for 1 MiB/4 MiB regimes that contradicted the review's own prediction and was visibly an artifact.

### 2.2 Second attempt: batch-allocate before release — DISCOVERED A DEEPER ISSUE

Fixed the reuse issue by holding all 16 reservations in a `Vec<Option<Reservation>>` across the whole measurement window, so each mmap call sees a genuinely different, still-growing free region. This revealed a second, more fundamental issue: for `align == size` (the fast path's own precondition), Linux places consecutive reservations at `base - k*size` under top-down mmap, so ALL k share the same alignment residue modulo `size` — a single process is ONE Bernoulli trial (the residue is set once by ASLR's mmap_base draw), not N independent trials.

Confirmed empirically: single-run hit rates for 64 KiB/1 MiB/4 MiB were each uniformly 0% or 100% within one process, never in-between (unlike 4 KiB, which is 100% by construction regardless of ASLR).

### 2.3 Final methodology: 30 independent process launches

To sample the ASLR mmap_base distribution properly, the final design aggregates hits/attempts across 30 independent process launches (each with `ITERS=16` reservations). This follows the project's own "measure under the real workload regime, not a harness artifact" rule (CLAUDE.md's R30-8-family evidence rules — same failure class as R29-16's virgin-batch dilution, just found before shipping a number instead of after).

---

## 3. Result (30-run aggregate, ITERS=16 reservations/run)

| Alignment Regime | Hits | Attempts | Hit Rate |
|-----------------|------|----------|----------|
| page-size (4 KiB) | 480 | 480 | 100.0000% |
| existing default arm (64 KiB) | 165 | 480 | 34.3750% |
| existing large arm (1 MiB) | 224 | 480 | 46.6667% |
| V20/P17 flagship regime (4 MiB) | 272 | 480 | 56.6667% |

See `docs/perf/_raw_r_v20_849_unix_exact_reserve_hit_rate.log` for the full per-run raw output, and `docs/perf/R_V20_849_UNIX_EXACT_RESERVE_HIT_RATE_summary.csv` for the machine-readable summary.

---

## 4. Interpretation

### 4.1 Contradicts the review's prediction

The review's hand-derived prediction for the 4 MiB regime was ~1/1024 (~0.1%), based on:
- Align = 2^22 (4 MiB), page = 2^12 (4 KiB)
- Low 10 "page-granularity" ASLR bits must all be 0
- Given ~28 bits of mmap_rnd_bits entropy, P = 2^-10

The measured result (56.6667%, roughly 17/30 processes hit) contradicts this by **3 orders of magnitude**. The review's math is sound in isolation, but the empirical result says the real distribution is NOT close to uniform over that many bits on this host+kernel.

### 4.2 Likely mechanism (not confirmed against kernel source)

Linux's Transparent Huge Pages (THP) alignment nudge for anonymous mappings ≥ `HPAGE_PMD_SIZE` (2 MiB) biases `get_unmapped_area`'s placement toward 2 MiB-ish boundaries for THP-collapse opportunities, independent of any `MADV_HUGEPAGE` hint from userspace. This would naturally increase the probability that a 4 MiB-aligned reservation succeeds.

Not confirmed against kernel source in this task (out of scope for a measurement-only task) — noted as the likely mechanism, not asserted as proven.

### 4.3 Caveat — platform

Measured on **WSL2 (Ubuntu, Hyper-V-backed kernel)**, not bare-metal Linux. WSL2's VA layout/ASLR entropy could differ from a native Linux kernel's — this number should not be treated as definitive for the review's "bare-metal Linux production host" framing without a bare-metal re-measurement.

See §5 for the open item tracking this caveat.

---

## 5. Implication for the review's proposed guard (NOT implemented)

The review's V20/P17 proposal (skip the exact-mmap fast-path attempt for `align > threshold`, since a miss costs 5 syscalls vs 3) assumed a ~1/1024 hit rate made the attempt a near-pure loss for the 4 MiB regime. At the measured ~57% hit rate, the fast path is very likely still a net win even for large aligns on this platform+kernel — the premise motivating the guard does not hold here.

**Recommendation:** Do NOT implement the align-threshold guard without a bare-metal Linux re-measurement first. The WSL2-only number is not strong enough evidence to close V20/P17 outright in either direction.

---

## 6. Verification (Windows host, all required per task #849's prompt)

```bash
cargo test -p aligned-vmem --all-features        # 18/18 passed
cargo clippy --all-targets -D warnings           # clean (default features)
cargo clippy --all-features --all-targets -D warnings   # clean
cargo fmt --check -p aligned-vmem                # clean
cargo doc --features "lazy-commit huge-pages fault-injection" --no-deps   # clean
```

The new example is gated on `[[example.required-features = ["bench-internals"]]]` in `crates/aligned-vmem/Cargo.toml`, so `cargo clippy --all-targets` (default features) does not try to compile it against cfg'd-out accessor functions — this is the same "default clippy row must actually build" hazard task #846 found with a backwards cfg_attr, caught here before it shipped.

---

## 7. Reproduction

### 7.1 Toolchain / platform identity

```
cargo 1.98.0-nightly (a595d0da2 2026-06-20)
rustc 1.98.0-nightly (bd08c9e71 2026-06-25)
WSL2 Ubuntu (RUSTC_WRAPPER= override needed — a Windows sccache.exe
path otherwise leaks into the WSL environment, unrelated to this
measurement, documented in the prior b228e69 commit)
```

### 7.2 Exact reproduction commands

```bash
cd crates/aligned-vmem
cargo build --release --features bench-internals \
  --example v20_849_unix_exact_reserve_hit_rate
for i in $(seq 1 30); do \
  ./target/release/examples/v20_849_unix_exact_reserve_hit_rate; \
done
# aggregate hits/attempts per regime across the 30 lines of output
```

### 7.3 Source identity

Measured from commit `35d51e6` (task #849) — see `git show 35d51e6` for the full diff.

---

## 8. Open items

**Item 46 in `docs/perf/OPEN_ITEMS.md`** — bare-metal Linux re-measurement required before the V20/P17 align-threshold guard decision can be closed definitively. This measurement was on WSL2/Hyper-V; a native Linux kernel's VA layout/ASLR entropy may differ.

---

## 9. Files touched by this task

- `crates/aligned-vmem/Cargo.toml` — new `[[example]]` entry with `required-features = ["bench-internals"]`.
- `crates/aligned-vmem/examples/v20_849_unix_exact_reserve_hit_rate.rs` — new measurement example (bench-internals-gated).
- `docs/perf/R_V20_849_UNIX_EXACT_RESERVE_HIT_RATE.md` — this report.
- `docs/perf/_raw_r_v20_849_unix_exact_reserve_hit_rate.log` — raw per-run output (gitignore exception).
- `docs/perf/R_V20_849_UNIX_EXACT_RESERVE_HIT_RATE_summary.csv` — machine-readable summary.
- `docs/perf/OPEN_ITEMS.md` — open item 31 added (bare-metal Linux re-measurement).

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>