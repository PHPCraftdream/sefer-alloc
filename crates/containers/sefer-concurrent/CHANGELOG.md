# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - 2026-11-15

First release. Everything below is new in this version; nothing has shipped
before it.

### Added
- `EpochRegion<T>` / `EpochHandle<T>` — epoch-based reclamation tier over
  `crossbeam-epoch`, with the confined `unsafe` seam `src/epoch/hand.rs`
  (`AtomicSlot<T>`). Extracted verbatim from `sefer-alloc`'s
  `src/concurrent` (workspace-restructure plan step P1.2); no logic changed.
- `LockFreeRegion<T>` / `LockFreeHandle<T>` — RCU page-snapshot tier over
  `arc-swap`. Extracted verbatim; no logic changed.
- `ShardedRegion<T>` / `ShardedHandle<T>` — sharded region tier routing
  handles to per-shard `EpochRegion`s. Extracted verbatim; no logic changed.
- `PinnedRunner` — thread-per-core runner over `core_affinity` (feature
  `pinning`). Extracted verbatim; no logic changed.
- Loom shadow models (`tests/loom_epoch.rs`, `tests/loom_sharded.rs`,
  `tests/loom_r11_epoch_false_full.rs`) and the sequential Kani vacant-slot
  proofs (`src/kani_proofs.rs`) moved in from the root crate together with
  the tier they cover.
