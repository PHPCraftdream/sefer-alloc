# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - 2026-11-15

First version. Everything below is new in this version; nothing has shipped
before it.

### Added

- Initial extraction of the sefer allocator's policy layer from the
  `sefer-alloc` root crate (`src/alloc_core/config`): the const-buildable
  configuration types (`LargeCacheConfig`, `LargeCacheMode`,
  `SmallSegmentPoolConfig`), the configuration-policy types (`Profile`,
  `SmallPoolPolicy`, `LargeCachePolicy`), the medium-promotion
  reachability predicate (`medium_promotion`), and the Large-THP policy
  runtime (tuple codec, curves, estimators, observation planner, cadence,
  budget ledger, `ThpRuntime` statics). Pure move — no behavior change.
- `#![forbid(unsafe_code)]`, `no_std`; one dependency (`aligned-vmem`, for the `ThpAdvice::classify` error type).
- Feature mesh mirroring the root crate's gate names 1:1 (all
  default-off): `alloc-decommit`, `alloc-global`, `bench-internals`,
  `internals`, `large-cache-extended`, `medium-classes`,
  `exact-span-large`, `large-reserved-capacity`, `numa-aware`.
