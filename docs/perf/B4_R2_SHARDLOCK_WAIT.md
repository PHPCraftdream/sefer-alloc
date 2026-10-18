# B4-R2 — ShardLock waits on the OS after the bounded spin phase

Task: b4-r2-shardlock. Deliverable report for the rewrite of
`src/registry/segment_route/shard_lock.rs`.

## 1. Problem and hypothesis

`b50f04f4` introduced `ShardLock` as a 2-state (`AtomicBool`) TTAS spin lock
with bounded exponential `spin_loop` backoff and then `yield_now()`, to avoid
`std::sync::Mutex`'s first-lock pthread-mutex allocation on macOS re-entering
the allocator being constructed. The single-thread uncontended path is
optimal, but under multi-thread contention a waiter that exhausts the spin
budget degrades to `yield_now()`: it keeps consuming its full scheduler
quantum per round-trip, it burns CPU that the actual holder (or the
application threads) need, and under background load (another process
saturating the cores) the waiter effectively busy-polls through the entire
critical section. `docs/perf/B1_MT_REGRESSION_ANALYSIS.md` §4.3/§6.1 item R2
identifies this as a multi-thread regression source: the larson/mstress MT
gap (≈×1.7 / ≈×2.0 against mimalloc) and the 100× collapse under background
load both route through the route-directory shards' `ShardLock`. Hypothesis
(B1 §7.4 R2 row): parking the waiter in the OS after the bounded spin phase
recovers MT +5–15% and eliminates the background-load collapse, without
touching the single-thread path.

## 2. Design

### 2.1 The 3-state word

`state: AtomicU32` (first field of `ShardLock`, offset 0, 4-byte aligned —
the address handed to the kernel):

- `UNLOCKED (0)` — free;
- `LOCKED (1)` — held, nobody parked on it;
- `CONTENDED (2)` — held AND at least one waiter has marked the word, so
  unlock must wake the wait queue.

`CONTENDED` is only ever written on OS-wait targets; on macOS/miri/loom the
word only ever holds 0/1, exactly like the old `AtomicBool`.

### 2.2 Acquire protocol (`lock_slow`, shared by `lock()` and
`lock_unless_exiting`)

1. **Fast path and retry** — `compare_exchange_weak(UNLOCKED, LOCKED,
   Acquire, Relaxed)`. Byte-for-byte the old uncontended acquire: one CAS,
   no preceding read.
2. **Bounded TTAS spin phase** (`SPINS_BEFORE_WAIT = 64` rounds, exponential
   `spin_loop` backoff capped at `1 << 6`) — identical cost profile to
   b50f04f4; the cache line stays shared instead of ping-ponging RMW.
3. **Spin exhausted, OS-wait targets** (Windows x64/ARM64, Linux
   x86_64/aarch64/riscv64 — all `not(miri), not(loom)`):
   - the exit probe (`exiting()`) is checked BEFORE every blocking wait, so
     exit refusal never waits more than one `WAIT_TIMEOUT_MS = 4 ms`;
   - mark-and-wait on `state`:
     - `UNLOCKED` → `compare_exchange(UNLOCKED, CONTENDED)`; on success
       **return the guard immediately** — a slow-path acquire always takes
       the word with the waiter bit set, so its own unlock wakes anyone
       still queued (a plain `UNLOCKED→LOCKED` acquire here could strand a
       queued waiter behind a plain-store unlock);
     - `LOCKED` → mark `LOCKED→CONTENDED` (failure just means the state
       moved; the loop re-reads), then `os_wait::wait_contended(&state, 4)`;
     - `CONTENDED` → wait directly.
4. **Spin exhausted, other targets** (macOS, miri, loom) — b50f04f4 behavior
   verbatim: `yield_now()` with the exit probe once per
   `EXIT_PROBE_YIELDS = 64` yield rounds.

### 2.3 Unlock (`ShardGuard::drop`) — fetch_sub

```rust
if self.lock.state.fetch_sub(1, Ordering::Release) != LOCKED {
    self.lock.state.store(UNLOCKED, Ordering::Release);
    // OS-wait targets only:
    os_wait::wake_all(&self.lock.state);
}
```

- Uncontended (word == `LOCKED`): exactly **one** atomic op, a Release RMW.
  The b50f04f4 operation count is preserved: one Acquire CAS in, one atomic
  op out (the old code did a release *store*; this is a release RMW — same
  count, slightly stronger, still a single instruction `lock dec`/`lock
  xadd`-class op on x86_64).
- Contended (`old == CONTENDED`): publish `UNLOCKED` first (the wake's value
  re-check must observe the store), then wake.

### 2.4 Why wake-ALL, not wake-one

Wake-one can strand waiters. With wake-one: an unlocker U fetch_subs
`old == CONTENDED`, stores `UNLOCKED`, and wakes exactly ONE queued waiter
(W1); a second waiter W2 remains queued while the value now reads
`UNLOCKED`. If a fresh thread T fast-path-acquires the word as
`UNLOCKED→LOCKED` before W1 (or anyone) re-marks it, then when T unlocks,
`fetch_sub` sees `old == LOCKED` and T never calls wake — W2 stays parked
until some UNRELATED future contention wakes the same address. That is a
starvation/liveness hazard, not just a performance question. Wake-all closes
it: every unlock that moves the value away from `CONTENDED` wakes every
queued waiter, so a waiter can only be sleeping while the value is
`CONTENDED`, and the very unlock that changes that value wakes it; surplus
woken threads re-check the value and go back to sleep. The herd is bounded
by the waiter count and only occurs under contention.

### 2.5 Timeout and exit-probe semantics

`WAIT_TIMEOUT_MS = 4` only re-arms the wait: actual wakeups arrive
immediately via the paired wake. The timeout bounds exit-refusal latency —
the `exiting()` probe runs before every blocking wait, so worst case refusal
latency after the spin phase is one `WAIT_TIMEOUT_MS`, not unbounded. On the
fallback tier the old `EXIT_PROBE_YIELDS` yield-round probe is kept verbatim
(the constant is compiled out on OS-wait targets).

### 2.6 Platform matrix

| Target | Wait primitive | Notes |
|---|---|---|
| Windows (not miri/loom) | `WaitOnAddress` + `WakeByAddressAll` (kernel32, direct `extern "system"`, per project FFI convention — no new dependency) | BOOL result deliberately ignored: timeout/error just falls back to the caller's value-rechecking loop. Needs Win8+; std itself requires Win10+ for its futex, so this is strictly inside the supported floor. |
| Linux x86_64/aarch64/riscv64 (not miri/loom) | `futex(FUTEX_WAIT_PRIVATE/FUTEX_WAKE_PRIVATE)` via glibc/musl `syscall(2)` (variadic `extern "C"` — no libc crate) | `SYS_FUTEX` 202 (x86_64), 98 (aarch64/riscv64, asm-generic). `Timespec { i64, i64 }` — all three arches LP64. Result ignored: EAGAIN/EINTR/ETIMEDOUT all return to the value-rechecking loop. |
| macOS and other OSes; miri; loom | unchanged b50f04f4 spin+yield tier | `os_wait` module compiled out; `CONTENDED` never written; word stays 0/1. |

Every `unsafe` block carries a `// SAFETY:` proof (valid aligned atomic for
the lock's lifetime; the kernel re-checks the compare value under its
internal wait lock before sleeping, so a store landing after the check
cannot be missed by the paired wake; no allocation, no ownership transfer).
The module keeps its single `#![allow(unsafe_code)]` tier-1 seam; the
module-level allow count is unchanged (no new allows added).

## 3. Guarantees preserved

- **Non-allocating**: the wait word is inline in `ShardLock`; WaitOnAddress
  / futex / WakeByAddressAll allocate nothing in the kernel path that
  concerns us.
- **Exit refusal**: `lock_unless_exiting(process_exit_in_progress,
  record_exit_lock_giveup)` semantics unchanged; bounded latency (spin phase
  + ≤ `WAIT_TIMEOUT_MS` per probe on OS-wait targets, the old 64-yield probe
  on the fallback tier). `src/global/process_exit.rs` untouched — the
  exit-probe FFI still never re-enters the loader.
- **Unwinding-safe guard**: `Drop` performs the release (and wake) during
  unwind; covered by test 3.
- **Auto traits untouched**: `ShardGuard` keeps field `_unique:
  PhantomData<&'a mut T>` and its doc contract, so
  `tests/r13_shard_guard_auto_traits.rs` and the compile-fail copy keep
  passing / failing identically (E0277).
- **Callers unchanged**: directory.rs's 8 call sites keep the identical API
  surface; critical sections untouched.
- **No Loom model**: loom 0.7.2 (the pinned workspace loom) has no public
  futex/wait API — verified against the registry source
  (`loom-0.7.2/src/sync/` has no futex module), so the OS wait cannot be
  modeled; on `loom` cfg the file keeps the pure spin+yield tier. B1 §7.4 R2
  row agrees: Loom is not required for this change ("обёртка ожидания").
- **No new dbg_* hooks**: the activation counter is a `#[cfg(all(test,
  <predicate>))] pub(super) static` — `scripts/verify-dbg-hook-safety.mjs`
  only scans `pub ... fn` items and cannot see it; it is absent from every
  production build.
- One private `mod os_wait` inside the file; no new file under `src/`
  (segment_route entry count unchanged); no new dependencies; no doctests.

## 4. Tests — `tests/b4_r2_shard_lock_wait.rs`

Standalone (`#[path]` include of the real file, the
`r13_shard_guard_auto_traits.rs` convention); **no crate features required**.
Every test owns its lock/flags and tolerates parallel `cargo test`.

1. `parked_holder_waiters_activate_os_wait_and_all_acquire` — holder parked;
   3 waiters each assert their own `DBG_OS_WAIT_BLOCKS` delta ≥ 1 (R30-8
   path-activation oracle: red-by-absence against the old pure spin lock,
   red-on-value if the wait path never runs) and all acquire exactly once
   (payload sum). On non-OS-wait targets only liveness is asserted — the
   unchanged spin+yield tier is the accepted implementation there, so there
   is no OS-wait path to activate.
2. `waiters_do_not_burn_cpu_while_holder_parks` (`#[cfg(windows)]`) —
   **RED-ON-REVERT**: measures the WAITER THREAD's own CPU time via
   test-local `GetThreadTimes`/`GetCurrentThread` FFI (pseudo-handle, 100 ns
   units). Asserts wall ≥ 350 ms and waiter CPU delta < 150 ms; a
   spinning/yielding waiter burns ≈wall CPU, an OS-wait waiter spends
   microseconds per 4 ms re-arm round (~100 rounds in 400 ms). This test
   fails if the lock is reverted to spinning.
3. `unwind_while_holding_releases_and_wakes_waiters` — guard owned inside a
   `catch_unwind` closure, so it drops during unwind; both queued waiters
   must acquire afterwards (behavior preservation, r6_t26-style).
4. `exit_refusal_returns_promptly_while_holder_holds` — `exiting() == true`
   returns None and fires giveup immediately while the holder is still
   parked (probe precedes the first blocking wait).
5. `stress_mutex_holds_with_deep_contention` — 4 threads × 512 alternating
   `lock()`/`lock_unless_exiting` rounds with mutual-exclusion flag assert
   and exact payload sum, plus a sleeper thread that holds the lock 1 ms
   every 16th iteration, forcing waiters into the OS-wait phase repeatedly.

### CI coverage

- Linux futex branch: compiles and runs only on a Linux runner. The default
  cargo-test rows in `.github/workflows/ci.yml` (e.g. `test (core tiers)` /
  `test (xthread progression)` jobs, and `test-windows` for the
  WaitOnAddress branch) pick up `tests/b4_r2_shard_lock_wait.rs` on any
  default-feature test pass; the Windows-specific CPU-burn test only runs in
  the `test-windows` job. This worktree's dev machine is Windows, so the
  Linux branch was verified by cross-checking only (below).
- Miri cells: unaffected — `os_wait` is compiled out under `not(miri)`.
- `cargo check --target x86_64-unknown-linux-gnu` **was run locally** (the
  target is installed) and compiles the futex branch cleanly.

## 5. What the ORCHESTRATOR must measure (no benchmarks were run here)

- larson + mstress T=4 (and T=8 if cheap) via `examples/malloc_macro` on
  GitHub runners (mt-bench label), plus the owner's background-load scenario
  (the 100× collapse case).
- Acceptance: MT geomean ≥ baseline, and specifically **no collapse under
  background load**.
- Single-thread iai cells should be unchanged: the lock only affects the
  foreign-free route-directory path; cold single-thread paths never contend,
  and the uncontended acquire/release op count is preserved.

## 6. Expected effect and honest limits

Per B1 §7.4 R2: closes part of the MT larson ×1.7 / mstress ×2.0 gap
(hypothesis MT +5–15%) and eliminates the background-load collapse. Limits:
the bounded spin phase is unchanged, so very short critical sections still
spin (that is the desired fast path); the wake-all herd adds contention
delay proportional to queued waiters; gains depend on how much of the MT gap
is actually attributable to `ShardLock` (R2 is one of several B1 items).

## 7. Risks

- Linux `SYS_FUTEX` table covers x86_64/aarch64/riscv64 only; other Linux
  arches fall back to the spin+yield tier — correct, just not optimal.
- `WaitOnAddress` needs Win8+; std itself requires Win10+ for its futex, so
  no supported target is lost.
- Spurious-wakeup herd bounded by the waiter count; thundering wake-all cost
  only materializes under contention (where spinning was worse).
- `syscall(2)` is a stable glibc/musl ABI, but it is an implementation
  surface: if a future target drops it, that target falls into the cfg
  predicate's arch list being extended or the fallback tier.

## 8. Suggested commit (NOT committed)

Prefix `perf(runtime)`:

```
perf(runtime): route-directory ShardLock waits on the OS after the bounded spin phase (R2)
```
