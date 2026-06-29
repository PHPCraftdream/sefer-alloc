# Integration Guide

How to attach `sefer-alloc` to a project as the global allocator and tune
its three operational knobs at compile time: **size limit**, **release period**,
**release trigger**.

This guide is a consolidated answer to "how do I use it in production".
For the wider documentation map see [`README.md`](../README.md) §
*Documentation map*.

---

## 1. Add the dependency

Pick the feature set that matches the workload. The recommended starter
for multi-thread / long-running processes is `production`:

```toml
[dependencies]
sefer-alloc = { version = "0.1", features = ["production"] }
```

`production` is an alias for `alloc-global + alloc-xthread + alloc-decommit
+ fastbin` — the drop-in `GlobalAlloc` face, lock-free cross-thread
free, M6 decommit (returns empty segments to the OS), and the per-thread
fast-bin magazine.

Other valid feature shapes:

| Shape | `features = [...]` | Use case |
|---|---|---|
| Handle store only (default) | _omit_ | `Region<T>` / `Handle<T>` for typed slot storage |
| `no_std` + `alloc` core | `default-features = false` | embedded targets |
| Single-thread allocator | `["alloc-global"]` | single-thread process |
| Multi-thread allocator | `["alloc-global", "alloc-xthread"]` | multi-thread, no segment recycling (1024-segment ceiling) |
| **Recommended for servers** | `["production"]` | long-running multi-thread (DBMS, async runtime) |
| `production` + NUMA | `["production", "numa-aware"]` | multi-socket NUMA hardware |

Full feature matrix: [`README.md`](../README.md#features-matrix).

## 2. Install as `#[global_allocator]`

One declaration at the crate root (binary or library top-level):

```rust
use sefer_alloc::SeferMalloc;

#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::new();

fn main() {
    // Every Vec/Box/String/HashMap allocation in this process — including
    // those made by tokio, async-std, std collections, third-party crates —
    // now goes through sefer-alloc.
    let v: Vec<u8> = (0..1024).collect();
    println!("{}", v.len());
}
```

That's the full integration. Everything below is **optional tuning** via
the `LargeCacheConfig` const builder — a code change, no recompile needed.

---

## 3. The three compile-time knobs (`alloc-decommit`)

The large-segment cache is the only piece of the allocator with tunable
policy. It governs **how aggressively empty large blocks are returned to
the OS** versus held in a per-shard free-list for reuse. Configuration is
via `LargeCacheConfig` — a `Copy + Clone + const fn` builder — passed to
`SeferMalloc::with_config(...)`. All methods are `const fn`, so the config
lives in a `static` initialiser and is resolved at compile time (zero
runtime overhead, no env reads, no parse errors).

### 3.1 Size limit — *how much memory the cache may hold*

Two knobs work together. The cache admits any large free until the
**budget** is hit; **headroom** is the level below which the decay does
not pull memory back to the OS (anti-thrashing floor).

| Builder method | Default | Meaning |
|---|---|---|
| `.budget_bytes(n)` | `None` (**unbounded**) | Per-shard hard ceiling on cached bytes. `0` = unbounded. No admission limit when unset; FIFO eviction fires only when this is set and the new span would exceed it. |
| `.headroom_bytes(n)` | `256 MiB` | Floor below which the periodic decay is a no-op. Above this, `excess = cached − headroom` is the amount eligible for release. |

**Containers / RSS-sensitive deployments**: call `.budget_bytes(512 * 1024 * 1024)`
(or whatever your RSS ceiling is). Without it the cache will retain
whatever the workload churned through until the OS or the decay clock
pulls it back.

### 3.2 Release period — *how often the cache releases excess*

| Builder method | Default | Meaning |
|---|---|---|
| `.decay_interval_ms(n)` | `1000` (1 s) | Minimum wall-clock ms between two consecutive decay ticks. A tick computes `excess = cached − headroom` and releases `excess × rate` back to the OS. |
| `.decay_rate_percent(n)` | `10` (10 %/tick) | Fraction of the excess released on each tick, in integer percent `[1, 100]`. Values outside the range are clamped. |

The model is **self-damping exponential decay**: each tick removes a
constant *fraction* of the current excess, so the cache approaches
`headroom` aggressively when far above it and gently when near it. No
oscillation, no spike. An idle process pays nothing — the tick is gated
by the very next alloc/free that happens to be a large one (the "lazy"
trigger below).

### 3.3 Release trigger — *event-driven or thread-driven*

| Builder method | Default | Meaning |
|---|---|---|
| `.mode(m)` | `LargeCacheMode::Lazy` | Selects how the decay tick fires. |

- **`LargeCacheMode::Lazy` (default, fully implemented).** Event-driven:
  each large `alloc` and `free` checks whether `decay_interval_ms` has
  elapsed since the previous tick; if so, exactly one decay step runs
  **inline on that call**. No background thread, no extra syscall on the
  common path, no allocation. Idle process → no work. This is the
  mobile/embedded/serverless-friendly mode.

- **`LargeCacheMode::Background` (reserved).** Intended to spawn a
  dedicated thread that visits every shard's large-cache on a timer, so
  a quiescent shard (no alloc/free activity) still gets decay. **In the
  current release this behaves identically to `Lazy`.** The mode-selector
  plumbing is in place; the thread itself is intentionally deferred.

- **`LargeCacheMode::Both`.** Alias for `Background` today. Reserved for
  the future distinction "lazy hooks AND background thread".

**Practical recommendation:** keep the default `Lazy` unless your workload
has long quiescent periods where you actively want background trimming.
`Lazy` already covers DBMS / tokio servers / async runtimes where
allocation pressure is continuous.

---

## 4. Worked example — RSS-bounded server

A containerised tokio server with a 512 MiB RSS ceiling, aggressive
trimming every 200 ms:

```toml
# Cargo.toml
[dependencies]
sefer-alloc = { version = "0.1", features = ["production"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

```rust
// src/main.rs
use sefer_alloc::{SeferMalloc, LargeCacheConfig, LargeCacheMode};

const CACHE_CONFIG: LargeCacheConfig = LargeCacheConfig::new()
    .budget_bytes(512 * 1024 * 1024)      // hard ceiling: 512 MiB per shard
    .headroom_bytes(64 * 1024 * 1024)     // floor: don't decay below 64 MiB
    .decay_interval_ms(200)               // tick every 200 ms
    .decay_rate_percent(25)               // release 25 % of excess per tick
    .mode(LargeCacheMode::Lazy);          // event-driven (no background thread)

#[global_allocator]
static GLOBAL: SeferMalloc = SeferMalloc::with_config(CACHE_CONFIG);

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // ... your server ...
}
```

What this means in practice:
- Cache will never hold more than 512 MiB of large segments per shard.
- Below 64 MiB the cache is left alone (anti-thrashing).
- Every 200 ms, on the next large alloc/free, 25 % of the excess above
  64 MiB is returned to the OS.
- Idle process → no ticks, no work, no syscalls.

For a desktop / dev profile that retains memory more aggressively for
throughput, use `SeferMalloc::new()` (equivalent to
`SeferMalloc::with_config(LargeCacheConfig::DEFAULT)`): the defaults
(`headroom=256 MiB`, `interval=1 s`, `rate=10 %/tick`, unbounded budget)
are already tuned for the throughput-first case.

---

## 5. Verifying the configuration is live

Use the in-process debug seam (tests / diagnostic builds). With the
`alloc-decommit` feature on:
- `AllocCore::dbg_decay_config()` returns the active
  `(rate_bp, interval_ms, headroom_bytes)` tuple.
- `AllocCore::dbg_large_cache_mode()` returns the active `LargeCacheMode`.

These are intended for tests but accessible from any caller of the public
crate API. They are the canonical way to verify that a `LargeCacheConfig`
was applied as expected.

**Track RSS over time** with `ps`/`top`/`docker stats`. The decay
profile becomes visible: aggressive settings produce a sawtooth that
damps toward `headroom`; default settings produce a gentle slope.

---

## 6. The sub-crates (optional)

If you only want one of `sefer-alloc`'s building blocks, you can `cargo
add` it independently — these are real crates.io packages, not just
internal modules:

| Crate | What it gives you | When to use |
|---|---|---|
| [`sefer-region`](https://docs.rs/sefer-region) | `Region<T>` / `Handle<T>` / `SyncRegion<T>` typed handle store | typed slot storage without an allocator stack |
| [`aligned-vmem`](https://docs.rs/aligned-vmem) | SEGMENT-aligned `mmap` / `VirtualAlloc` + page decommit/recommit | building your own allocator on top of a verified OS aperture |
| [`numa-shim`](https://docs.rs/numa-shim) | NUMA detection + binding (`mbind` / `VirtualAllocExNuma`, no `libnuma`) | NUMA-aware code without C dependencies |
| [`malloc-bench-rs`](https://docs.rs/malloc-bench-rs) | portable `GlobalAlloc` benchmark harness (larson + mstress) | benchmarking your own allocator |

`sefer-alloc` re-exports `sefer-region`'s surface, so existing code
using `use sefer_alloc::{Region, Handle, SyncRegion};` continues to
work unchanged.

---

## 7. Common pitfalls

- **Forgetting `#[global_allocator]`.** The dependency builds and the
  type compiles, but every allocation still goes through the system
  allocator. The declaration must be on a `static` at the crate root,
  not inside a function.

- **Wrong feature set.** `cargo add sefer-alloc` without
  `--features production` (or at least `--features alloc-global`) gives
  you the handle store only — `SeferMalloc` is not exported.

- **`with_config` only available under `alloc-decommit`.** The
  `LargeCacheConfig` type and `SeferMalloc::with_config` are only
  compiled when the `alloc-decommit` feature is on (included in
  `production`). Without it, use `SeferMalloc::new()` (the only
  constructor).

- **Expecting `LargeCacheMode::Background` to spawn a thread.** Today
  it behaves identically to `Lazy`. Use `LargeCacheMode::Lazy` (the
  default) — it covers virtually all real workloads, including idle ones
  (a tick is gated by the next large op, but `headroom` already bounds
  how much can accumulate before that op fires).

- **Setting `budget_bytes` smaller than `headroom_bytes`.** Legal but
  pointless — the cache will be forced into FIFO eviction at the budget
  ceiling before decay ever kicks in. Keep `headroom < budget` (or
  leave `budget` unset for the throughput-first default).
