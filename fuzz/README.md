# sefer-alloc fuzz targets

libFuzzer harnesses for `sefer-alloc`'s `Region` op-stream invariants — Phase 5
hardening. These are **CI / Linux-only**: libFuzzer requires the nightly
toolchain and does not run on Windows. The main `cargo build` / `cargo test` in
the repository root are completely unaffected by this crate (it is its own
workspace root).

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
dedicated-segment path) and random power-of-two alignments (1 .. 4096) against
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
than the installed `SeferAlloc` `#[global_allocator]`: installing it process-wide
would route libFuzzer's own allocations through the not-yet-hardened TLS init
(see [`tests/global_alloc.rs`](../tests/global_alloc.rs)). The cross-thread
ordering path is covered instead by the TSan + aarch64 CI gates and the loom
harnesses. The run length is capped at 2048 ops; any invariant violation panics
and is reported as a finding.

## Prerequisites

```sh
rustup toolchain install nightly
cargo +nightly install cargo-fuzz
```

## Running

From this `fuzz/` directory:

```sh
# Quick smoke run (swap in `global_alloc_ops` for the allocator target).
cargo +nightly fuzz run region_ops
cargo +nightly fuzz run global_alloc_ops

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
