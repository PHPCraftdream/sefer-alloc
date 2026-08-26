# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped
before it.

### Added

- **`RacyPtrCell<T>`** — a lazy, CAS-published pointer cell implementing the
  three-state machine `UNINIT(null) → INITIALIZING(sentinel) → READY(real
  pointer)` over a **single** `AtomicPtr<T>`. `#[repr(transparent)]`: the
  cell's layout is guaranteed identical to `AtomicPtr<T>`, not merely
  observed to be one word on the current compiler. `no_std`,
  allocation-free (never touches the heap), and usable inside a
  `#[global_allocator]`: the cell's own non-panicking operations use no `std`
  sync primitive — no `Mutex`, no parking, no `OnceLock` — so the cell itself
  cannot re-enter the allocator it is bootstrapping, and can publish a
  process-`'static` pointer before any heap exists. That is a property of the
  cell, not of a whole call: the caller's `init` closure runs inside
  `get_or_try_init` and must not allocate, block, or unwind — unwinding out of
  a `GlobalAlloc` method is undefined behaviour. See the crate docs'
  "Using this inside a `#[global_allocator]`" section for the full caller
  contract.
- **`RacyPtrCell::get_or_try_init(init: impl FnOnce() -> Option<NonNull<T>>)`**
  — `#[must_use]`, fallible initialisation with OOM rollback and loser
  re-race. The thread that wins the `null → sentinel` CAS runs the caller's
  init closure exactly once and publishes the resulting pointer with
  `Release`; `FnOnce` (not `FnMut`) states that at-most-once bound directly
  and admits consuming closures. When init reports failure (`None`, e.g.
  OOM), the sentinel is rolled back to `null` and concurrent losers
  **re-race the CAS** — a later attempt can succeed. This is the deliberate
  contrast with `std::sync::OnceLock`, whose `get_or_init` cannot fail at
  all, whose `get_or_try_init` is still unstable (`once_cell_try`), and
  which may block the losing threads for the duration of the winner's init
  (the documented `std` contract) instead of letting them re-race.
- **The anti-livelock loser-spin rule** — losers spin with `Acquire` loads
  only *while the state is `INITIALIZING`*, never `while != READY`. Spinning
  on `!= READY` deadlocks against the rollback path (a rolled-back cell never
  publishes `READY`); spinning on `== INITIALIZING` lets a loser observe the
  rollback and fall out to re-race. Losers busy-wait with
  `core::hint::spin_loop` — no OS park/unpark, by the same no-`std`-sync
  design constraint.
- **`RacyPtrCell::get()`** — a pure `Acquire` load returning the published
  pointer as `Option<NonNull<T>>` (no CAS, no init, no spin); `None` means
  `UNINIT` or `INITIALIZING` right now.
- **`RacyPtrCell::new()`** — `const fn` on normal builds (so the cell can
  live in a `static`; under `--cfg loom` it is non-`const` because loom's
  atomics have no const constructor), plus a `Default` impl. Panics — at
  compile time in the documented `static` usage — if `align_of::<T>() < 2`:
  the `INITIALIZING` sentinel is the address `1`, which must never be a valid
  aligned address for `T` (the sentinel is never dereferenced, only compared;
  built via `core::ptr::without_provenance_mut`, strict-provenance-clean).
- **Panic-safe init**: an init closure that *unwinds* rolls the sentinel back
  through an RAII guard (`Drop` stores `null` with `Release`), so a panicking
  init leaves the cell in `UNINIT` and retryable rather than wedged in
  `INITIALIZING` with every loser spinning forever.
- **Release-active sentinel-collision guard**: `get_or_try_init` asserts (in
  every profile, not `debug_assert!`) that a successful init did not return
  the null/sentinel address — a safe closure can construct that address, and
  publishing it would make every reader misclassify the cell as
  still-initialising forever.
- **Unconditional `Send + Sync`** for `RacyPtrCell<T>`, exactly like the
  `AtomicPtr<T>` it wraps: the cell only stores and hands back a raw
  `*mut T` / `NonNull<T>` and never dereferences it — whether the pointee is
  safe to access across threads is the caller's `unsafe` contract.
- **Stable test-probe API**: `dbg_is_ready()` (single-`Acquire`-load
  readiness probe) and `dbg_rollback_reenterable() -> RollbackProbe`
  (drives a live cell through the exact `null → sentinel → rollback →
  re-CAS` sequence, without a process-terminating OOM). Restores `UNINIT`
  when it returns `Proven`; on `NotApplicable` it never clobbers a
  concurrent owner's state, but that state is not required to match what
  the probe observed on entry. `RollbackProbe` is a closed two-variant enum
  (`Proven` / `NotApplicable`) — the probe has exactly two possible answers,
  and the type says so. Documented, semver-stable public surface intended
  for downstream consumers' own test suites.
- **Executable loom proofs against the real type**: under `--cfg loom` the
  cell's atomics alias to `loom::sync::atomic`, so the shipped loom suite
  (`tests/loom_racy_ptr_cell.rs`) model-checks the actual `RacyPtrCell` —
  exactly-once init under two and three threads, OOM-rollback survival — with
  `#[should_panic]` counterfactuals proving the harness is non-vacuous.
  `loom` is a `cfg(loom)`-gated library dependency only; a normal build pulls
  in zero non-`std` dependencies.
- **Portability requirement**: the cell is one `AtomicPtr<T>` driven by
  `compare_exchange`, so the crate requires `target_has_atomic = "ptr"` and
  fails at compile time with an explicit `compile_error!` on targets without
  it (`thumbv6m-none-eabi`, `riscv32imc-unknown-none-elf`, `msp430-none-elf`).
  `no_std` and allocation-free do not imply pointer-width CAS.
