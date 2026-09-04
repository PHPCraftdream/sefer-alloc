# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped before it.

### Added

- **`TaggedIndex<INDEX_BITS>`** — the packed `(index | tag)` head-word format: a slot index in the low `INDEX_BITS` bits, a strictly monotonic generation tag above them; `INDEX_BITS` is a const generic, compile-time capped at `1..=16`. Helpers `pack` (checked — returns `Option<u64>`, never silently truncates), `unpack`, `is_empty`, `empty_index`; constants `INDEX_MASK`, `TAG_BITS`, `TAG_MAX`.
- **`TagExhausted`** — the error `push` returns when a head's tag budget is spent and the head seals (see Notes).
- **`StackHead<INDEX_BITS>`** — the tagged head word: `new`, advisory `is_empty` (a `Relaxed` peek, for diagnostics/monitoring — not a correctness decision), `pushes_remaining` (advisory readback of the remaining push budget).
- **`StackStorage<INDEX_BITS>`** — the `pub unsafe trait` for implementors owning the head AND the links (`head`, `load_next`, `store_next`, all `unsafe fn`): one impl expresses the head↔links binding once and enables slot-resident links.
- **`StackOps<INDEX_BITS>`** — blanket-implemented for every `StackStorage` implementor (trait coherence makes downstream overrides impossible): `unsafe fn push_index(index) -> Result<(), TagExhausted>` and safe `fn pop_index() -> Option<u32>`.
- **`ArrayIndexStack<INDEX_BITS, N>`** — the owned standalone stack fusing `StackHead` + `ArrayLinks<N>`: `unsafe fn push`, `pop`, `is_empty`, `pushes_remaining`.
- **`ArrayLinks<N>`** — the public links building block (`Acquire` `load_next` / `Release` `store_next`) that `ArrayIndexStack` composes.
- **`pub const TAIL: u32`** — the link end-of-chain sentinel, part of the `StackStorage` contract.
- **`Default` and `Debug`** for `StackHead`, `ArrayIndexStack`, and `ArrayLinks`.
- **Off-by-default test instrumentation** — `#[doc(hidden)]` probes (`raw_head`, `cas_head_for_test`, `load_next_for_test`, `retry_counts_for_test`, `backoff_cap_reached_for_test`, ...) compiled only under the `test-internals` feature or a `--cfg loom` build, carrying no semver guarantee and absent from default builds entirely.

### Notes

- **ABA eliminated, not mitigated** — the tag never wraps: each push installs tag + 1, and a push observing `TAG_MAX` is refused with `Err(TagExhausted)`, sealing that head (pops keep draining; pushes stop loudly; there is no reset API). Every legal width guarantees at least `2^48 - 1` pushes per head.
- **Empty-transition tag preservation (H-2)** — draining the last element packs the empty sentinel with the running tag, not 0; resetting it would reopen the ABA window.
- **Lazy links (RAD-1)** — a link is written only by `push`, immediately before that index is published as head; a fresh stack is empty, and OS-zeroed backings are never first-touched merely to set up the free-list.
- **Release-active guards** — push's index-range check and pop's link sanity check (a link neither `TAIL` nor in range, or a self-link) panic in every build profile, not only debug; measured at no measurable cost next to the head CAS.
- **Three-clause caller contract** — `push_index`/`push` are `unsafe fn` (link domain; liveness — no double push; exclusive ownership epoch); `pop_index`/`pop` deliberately stay safe: an unauthorized pop can only leak an index, never double-issue one.
- **Lock-free, not starvation-free** — CAS retries use capped exponential backoff; the throughput-vs-fairness trade-off is documented in the crate docs.
- **Loom-checked** — under `--cfg loom` the stack's atomics alias to loom and the shipped suite model-checks the real stack code with no preemption bound, with `#[should_panic]` counterfactuals proving the harness non-vacuous; `loom` is an optional dependency, so a normal build has zero loom entries in `Cargo.lock`.
- **`no_std`, allocation-free, zero dependencies** in a default build; requires a target with native 64-bit atomics (unsupported targets fail fast with a named `compile_error!`). The library is `#![deny(unsafe_code)]` with audited, item-scoped lint-exception regions — see the crate docs' "Where unsafe lives" section for the self-verifying inventory.
- **MSRV** Rust 1.79 (library surface). **License** MIT OR Apache-2.0.
- **Pending measurement** — the arm64 wall-clock A/B of the link-ordering and CAS-strength candidates has not run; until it does, no ordering/CAS change is made on that basis.
