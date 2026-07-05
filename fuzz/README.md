# sefer-alloc fuzz targets

libFuzzer harnesses for `sefer-alloc` — Phase 5 hardening. Three structure-aware
targets, from the lowest layer to the flagship face:

| Target | Under test | Layer / features | Covers |
|---|---|---|---|
| `region_ops` | `Region<u64>` | default (single-threaded core) | slotmap membrane invariants I1–I5 |
| `global_alloc_ops` | `AllocCore` | `alloc-core` | segment substrate M1–M4; align corridor `1 .. <SEGMENT` (2^0..2^21) exercising the #130-hardened `align_up` / `alloc_large` large-align math |
| `heap_core_ops` | `SeferAlloc` (`GlobalAlloc` face) | `production` | the **fastbin magazine** (tcache) fill / flush / refill + M2 oracles over mixed small size-classes; same M1–M4 + zeroed/realloc invariants |

These are **CI / Linux-only**: libFuzzer requires the nightly toolchain and does
not run on Windows. The main `cargo build` / `cargo test` in the repository root
are completely unaffected by this crate (it is its own workspace root). The
targets are BUILD-checked per-push by the `fuzz-build` CI job; the CPU-hour
running campaigns are scheduled/manual, not per-PR.

## The target: `region_ops`

Interprets the fuzz input as a bounded sequence of ops (`insert` / `get` /
`remove` / `get_mut` / `clear`) against a `Region<u64>`, checked against the
**same reference-model invariants** as [`tests/differential.rs`](../tests/differential.rs)
(I1–I5 from [`docs/INVARIANTS.md`](../docs/INVARIANTS.md)):

- **I1** a fresh handle resolves to its value until removed.
- **I2** a removed handle is `None` forever; a second remove is a no-op.
- **I3** a stale handle (slot reused) never resolves to a live value.
- **I4** `len()` tracks the live count exactly.
- **I5** drop-once — at run end, `drops == inserts` (no double-free, no leak).

`arbitrary::Arbitrary` derives a structured op stream from the raw fuzzer bytes
(structure-aware feedback), and the run length is capped at 4096 ops so a single
input cannot OOM the fuzzer. Any invariant violation panics, which libFuzzer
reports as a finding.

## The target: `global_alloc_ops`

Fuzzes the Phase 8–11 allocator descent. Interprets the input as a bounded
stream of `alloc` / `alloc_zeroed` / `dealloc` / `realloc` ops of random sizes
(1 B .. ~2 MiB, spanning the small free-list classes and the large
dedicated-segment path) and random power-of-two alignments (`1 .. 2 MiB`, i.e.
`2^0 .. 2^21` — up to but NOT including `SEGMENT = 4 MiB`; alignments above 4096
route to `alloc_large`, exercising the #130-hardened over-reserve/trim large-align
arithmetic, while `align >= SEGMENT` is the rejected corridor covered by unit
tests) against
[`sefer_alloc::AllocCore`](../src/alloc_core/alloc_core.rs) — the single-threaded
segment substrate the `SeferAlloc` / `GlobalAlloc` face is built on. It checks
the **M-invariants** from [`docs/INVARIANTS.md`](../docs/INVARIANTS.md):

- **M1** every returned pointer is non-null, aligned, and writable for its size
  (pattern write + read-back).
- **M2** double-free / use-after-free never corrupts the allocator (a second
  `dealloc` is a no-op).
- **M3** two live allocations never share a byte (overlap check + per-block fill
  so contamination is detected at run end).
- **M4** the chosen size class always satisfies the requested size and align.
- `alloc_zeroed` returns all-zero memory; `realloc` preserves the
  `min(old, new)` prefix.

It targets `AllocCore` (owned, single-threaded, drops cleanly per input) rather
than the installed `SeferAlloc` `#[global_allocator]`: routing libFuzzer's own
harness allocations through the process-wide allocator is out of scope for
op-stream invariant fuzzing (the installed path is proven separately by
`tests/global_alloc_installed.rs` and `examples/tokio_burn_in.rs`). The
cross-thread
ordering path is covered instead by the TSan + aarch64 CI gates and the loom
harnesses. The run length is capped at 2048 ops; any invariant violation panics
and is reported as a finding.

## The target: `heap_core_ops`

Fuzzes the **fastbin magazine** (tcache) path — the 0.3.0 churn hot path that
`global_alloc_ops` does NOT reach (`AllocCore` sits *below* the magazine). It
drives [`sefer_alloc::SeferAlloc`](../src/global/sefer_alloc.rs), the flagship
`production` `GlobalAlloc` face, with a bounded stream of `alloc` /
`alloc_zeroed` / `dealloc` / `realloc` ops of MIXED SMALL size-classes
(1 B .. 8 KiB, so allocations stay on the magazine-managed classes) and small
power-of-two alignments (1 .. 4096, keeping routing on the magazine rather than
the large path). The magazine's fill / flush / refill machinery and its M2
oracles get random coverage this way.

`SeferAlloc`'s `GlobalAlloc` methods are called **directly** — it is NOT
installed as the process `#[global_allocator]`, so the harness's own allocations
still flow through the system allocator; a fuzz input maps to a clean owned op
stream against one thread's magazine + heap. It checks the same **M-invariants**
as `global_alloc_ops` (M1 validity, M2 no double-free, M3 no overlap, M4
size/align fidelity, `alloc_zeroed` all-zero, `realloc` prefix preserved).
Single-threaded on purpose: the cross-thread ordering path is covered by the
TSan + aarch64 CI gates and the loom harnesses. Run length capped at 2048 ops.

## Prerequisites

```sh
rustup toolchain install nightly
cargo +nightly install cargo-fuzz
```

## Running

From this `fuzz/` directory:

```sh
# Quick smoke run (pick a target: region_ops / global_alloc_ops / heap_core_ops).
cargo +nightly fuzz run region_ops
cargo +nightly fuzz run global_alloc_ops
cargo +nightly fuzz run heap_core_ops

# Build ALL targets without running (what the `fuzz-build` CI job does).
cargo +nightly fuzz build

# Long overnight run (wall-clock bounded, libFuzzer picks the time budget).
cargo +nightly fuzz run region_ops -- -max_total_time=3600

# Reproduce / minimize a crash from a saved artifact.
cargo +nightly fuzz run region_ops -- artifact.bin
cargo +nightly fuzz tmin region_ops -- artifact.bin
```

Corpora and crash artifacts are written under `fuzz/artifacts/region_ops/` and
`fuzz/corpus/region_ops/`.

## Status

This target is part of Phase 5's hardening gate (see
[`docs/PLAN.md`](../docs/PLAN.md)). It must run clean for a release; the heavy
CPU-hour fuzzing campaigns live here, not in the everyday dev loop.
