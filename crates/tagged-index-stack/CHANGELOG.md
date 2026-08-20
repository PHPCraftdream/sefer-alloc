# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped
before it.

### Added

- **`TaggedIndexStack<INDEX_BITS>`** — an allocation-free, `no_std`,
  `#![forbid(unsafe_code)]` lock-free LIFO free-list of small **indices** (a
  slot recycler): the canonical "recycle a small integer id" primitive that
  slab allocators, object pools, entity-component stores, and connection
  tables reinvent. `push(links, index)` / `pop(links)` are Treiber-style
  CAS loops over a single `AtomicU64` head.
- **`TaggedIndex<INDEX_BITS>`** — the packed head word: low `INDEX_BITS` bits
  carry a slot index, the high `64 - INDEX_BITS` bits a monotonic **tag**
  bumped on every successful push, which is what defeats the ABA problem (a
  pop-then-re-push of the same index bumps the tag, so a parked CAS on the
  stale `(index, tag)` pair fails and retries). `INDEX_BITS` is a const
  generic (capped at 32 so the packed index can never collide with the `TAIL`
  link sentinel); helpers `pack`/`unpack`/`empty`/`empty_index`/`is_empty`,
  all `const fn`. The index half's all-ones value is the reserved
  "stack empty" sentinel.
- **ABA-defeating empty transition (the H-2 rule)** — when a `pop` drains the
  last element, the empty sentinel is packed with the **running tag** the
  draining pop observed, not reset to `0`: a tag reset would reopen the ABA
  window for a popper parked across a drain-and-refill. The shipped loom
  counterfactual `counterfactual_empty_transition_tag_reset_lets_aba_recur`
  proves this is load-bearing — with the tag reset restored, loom finds the
  collision.
- **Tag-width budget analysis** — with `INDEX_BITS = 16` the tag gets 48 bits,
  so a wrap that could reopen ABA requires a victim parked across ~2^48
  pushes on a single slot: at a sustained 100k pushes/sec that is ~89 years, a
  structural non-hazard (a 32-bit tag gives only ~12 hours under the same
  assumptions). Documented so a consumer choosing `INDEX_BITS` knows the
  trade.
- **Caller-owned links — `Links` trait** — the stack stores only the HEAD;
  each index's `next` link lives in caller storage (`load_next` /
  `store_next`), so a production allocator keeps links **slot-resident** (an
  `AtomicU32` inside a slot it already owns) instead of paying for a second
  array. **`ArrayLinks<N>`** provides an owned `[AtomicU32; N]` backing for
  standalone use.
- **Lazy link discipline (RAD-1)** — links are never eagerly initialised:
  only a `push` writes a link, immediately before publishing that index as
  head. A caller whose link backing is OS-zeroed memory (a fresh `mmap`, a
  zeroed slot array) never first-touches pages merely to set up the free-list;
  links commit lazily on first push of each index. A fresh stack is therefore
  **empty** — deliberately no "start with `0..N` pushed" constructor, which
  would require exactly the eager chaining pass this discipline forbids.
- **Correct CAS orderings** — push's success ordering and pop's retry
  ordering were both chosen so a popper can never read a link through a stale
  head (pop's CAS-failure load is `Acquire`; a `Relaxed` retry was the
  regression the shipped counterfactual pins, task #698). Push's
  index-validity and sentinel guards are release-active `assert!`s, not
  `debug_assert!`s (task #703).
- **64-bit-atomic portability gate** — the head is one `AtomicU64`, so the
  crate fails fast with a named `compile_error!` on targets without native
  64-bit atomics (`thumbv6m-none-eabi`, `riscv32imc-…`, `armv5te-…`) rather
  than a cryptic unresolved-import error. `no_std`-compatible, but `no_std`
  alone does not imply `AtomicU64`.
- **Executable loom proofs against the real type**: under `--cfg loom` the
  stack's atomics alias to `loom::sync::atomic`, so the shipped loom suite
  (`tests/loom_aba.rs`) model-checks the actual `TaggedIndexStack` /
  `TaggedIndex` code, with `#[should_panic]` counterfactuals (untagged
  corruption, the H-2 empty-transition tag-reset ABA, and the
  Relaxed-CAS-failure-ordering regression) proving the harness is
  non-vacuous. `loom` is a `cfg(loom)`-gated library dependency only; a
  normal build pulls in zero non-`std` dependencies.
- **`raw_head()`** — `#[doc(hidden)]` test-probe accessor for the packed head
  word, its API posture settled before first publish (task #704).
