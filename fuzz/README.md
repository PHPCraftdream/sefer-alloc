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

## Prerequisites

```sh
rustup toolchain install nightly
cargo +nightly install cargo-fuzz
```

## Running

From this `fuzz/` directory:

```sh
# Quick smoke run.
cargo +nightly fuzz run region_ops

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
