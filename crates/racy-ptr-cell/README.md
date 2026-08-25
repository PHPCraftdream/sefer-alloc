# racy-ptr-cell

A lazy, CAS-published pointer cell — `UNINIT → INITIALIZING → READY` over a
single `AtomicPtr<T>` — with **fallible init, OOM rollback, and loser re-race**.

It fills the niche `std::sync::OnceLock` cannot:

- **`no_std`, allocation-free** — the cell is one `AtomicPtr`; it never touches
  the heap.
- **usable inside a `#[global_allocator]`** — the cell's own non-panicking
  operations use no `std` sync primitive, no parking and no allocation, so it
  can publish a process-`'static` pointer *before any heap exists* without
  itself re-entering the allocator it is bootstrapping. That is a property of
  the cell, not of a whole call: your `init` closure runs inside it and must
  not allocate, block, or unwind — see
  [Using this inside a `#[global_allocator]`](#using-this-inside-a-global_allocator)
  below before adopting it there.
- **fallible without parking** — on winner OOM the sentinel rolls back to
  `null` and losers re-race the CAS themselves. `OnceLock::get_or_init` cannot
  fail at all, its `get_or_try_init` is still unstable, and both park the
  losing threads for the duration of the winner's init.

```text
static CHUNK: RacyPtrCell<Chunk> = RacyPtrCell::new();

let chunk: Option<NonNull<Chunk>> = CHUNK.get_or_try_init(|| {
    // OS reservation etc.; return None on OOM to roll back and let losers re-race.
    // Inside a #[global_allocator], this closure must not allocate, block, or
    // unwind -- see the section below.
    reserve_and_init() // -> Option<NonNull<Chunk>>
});
```

**One rule applies to every user, allocator or not:** the `init` closure runs
while your thread holds the `INITIALIZING` sentinel, so it must not wait on
that same cell — and the restriction is **transitive**. Several cells form a
lock-order graph: thread 1 wins `A` and its `init` initialises `B` while
thread 2 wins `B` and its `init` initialises `A`, and both spin forever at
100% CPU with no direct self-recursion anywhere. Acquire multiple cells in a
fixed global order, exactly as you would locks.

## Using this inside a `#[global_allocator]`

The cell is built for this niche, but the niche has hard rules that are
**yours** to keep — the cell can enforce none of them:

- **`init` must not allocate**, directly or transitively, and must not
  otherwise re-enter the allocator being bootstrapped. It runs while your
  thread holds the `INITIALIZING` sentinel.
- **`init` must not block** — every loser thread spins for exactly as long as
  `init` runs, provided a winner is currently running at all. There is no
  bounded-latency guarantee.
- **`init` must not panic, and no panic may unwind through a `GlobalAlloc`
  method** — [unwinding out of a global allocator is undefined
  behaviour](https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html#safety).
  The crate's rollback guard keeps the *cell* consistent across an unwinding
  `init`, but it cannot make the unwind itself sound.
- **An `init` that returns the sentinel address (`1`) is a caller bug**, caught
  by a release-active `assert!` — a violated precondition, not a recoverable
  allocator error.

**The cell is neither fork-safe nor async-signal-safe.** `INITIALIZING` is
owned by a specific thread — no misbehaving `init` is needed to break it:

- **`fork()` in a multithreaded process.** If a thread holds the sentinel when
  another thread calls `fork()`, the child inherits a cell stuck at
  `INITIALIZING` with no thread able to publish or roll it back — every
  subsequent caller in the child spins forever, and there is no reset API.
- **An allocating signal handler.** If a signal interrupts the thread holding
  the sentinel and the handler allocates, the allocator re-enters the same
  cell from inside the handler and spins on a sentinel owned by the very
  thread it interrupted — an unrecoverable self-deadlock.

In a forking process, publish every cell before the first `fork()`, or
`fork()` only from a thread you know holds no cell and `exec()` in the child.
Do not allocate in a signal handler.

Three panic sites exist in total: that sentinel-collision `assert!`, an
unwinding `init`, and the `align_of::<T>() >= 2` check in `new`/`default`. All
three go through the panic runtime, which allocates in a `std` build. Make them
fatal instead — and note these are **different mechanisms for different link
environments, not two halves of one recipe**: a `std` binary sets
`panic = "abort"` in its profile (the panic runtime belongs to `std`, so no
crate can supply its own `#[panic_handler]` there); a `no_std` binary supplies
a non-allocating `#[panic_handler]` itself.

## The two rules people get wrong

1. **Publish with `Release`.** The winner stores the real pointer with
   `Release`; losers/readers `Acquire`. A `Relaxed` publish breaks the
   happens-before and lets a reader observe an uninitialised pointee.
2. **Losers spin `while == INITIALIZING`, not `while != READY`.** Spinning on
   `!= READY` deadlocks against the OOM-rollback path: a winner that hits OOM
   rolls the sentinel back to `null` and never publishes `READY`, so a
   `!= READY` spinner waits forever. Spinning on `== INITIALIZING` lets a loser
   observe the rollback and re-race.

Both rules are pinned by **executable loom proofs that run against the real
`RacyPtrCell` type** (the crate aliases its atomics to `loom::sync::atomic`
under `--cfg loom`) — `real_exactly_once_two_threads`/`real_exactly_once_three_threads`
for rule 1, `real_survives_oom_rollback_two_threads` for rule 2. The
`#[should_panic]` counterfactuals in the same test module are a separate,
complementary check: they run against small shadow models of the same two
rules (an `AtomicPtr`/`AtomicU8` standing in for `RacyPtrCell`'s own
internals, since loom cannot rebuild this crate with a deliberately-broken
ordering baked in), proving the loom harness itself is sensitive to each
protocol violation rather than passing vacuously:

```sh
RUSTFLAGS="--cfg loom" cargo test --release --test loom_racy_ptr_cell
```

## Test-probe API stability

`RacyPtrCell` exposes two `dbg_`-prefixed methods — `dbg_is_ready` and
`dbg_rollback_reenterable` — that are NOT hidden from `rustdoc`. This is a
deliberate posture decision (task #710), not an oversight:

- `dbg_rollback_reenterable`'s own doc explicitly invites downstream
  consumers to call it FROM their own test suites, to drive the OOM-rollback
  protocol on a real, live cell (e.g. a process-global registry chunk)
  without a process-terminating OOM. A `#[doc(hidden)]` posture — the usual
  default for a `dbg_*` diagnostic hook — would have hidden this function
  from the very rustdoc a consumer needs to discover it in, while
  simultaneously advertising it for their use: an unresolvable
  contradiction, since a published `pub` item is callable regardless of
  `#[doc(hidden)]`, which only affects documentation visibility, not the
  semver surface.
- The alternative considered — gating both methods behind a non-default
  Cargo feature (e.g. `test-probes`), the default CLAUDE.md's benchmark-hook
  rule (2) recommends for any hook with no production caller — was rejected.
  Cost, restated precisely (the earlier "would require restructuring the
  whole existing test suite" framing overstated it): `[[test]]
  required-features` exists, so the real cost is only the corresponding CI
  matrix addition. The rejection is still correct on its actual merits:
  neither method accepts a raw pointer or touches allocator metadata (the
  hazard CLAUDE.md's rule targets), and this crate's `dbg_*` surface is
  independently policed by the root repo's
  `tests/dbg_hook_safety_tripwire.rs` allowlist, which requires a reviewed
  justification per hook and already covers both.
- `dbg_is_ready` is functionally identical to `get().is_some()` — same
  single `Acquire` load, same predicate. It is not exempt from CLAUDE.md's
  "no production caller" framing on a technicality, either: the root
  `sefer-alloc` crate's `Registry::dbg_chunk_is_materialised`
  (`src/registry/bootstrap.rs`) is a real, exercised caller of this exact
  method (via `RacyPtrCell::dbg_is_ready`), used to assert chunk-
  materialisation state in that crate's own regression tests. It stays
  public for that reason, not because it offers any capability `get`
  lacks.

Both methods carry the crate's ordinary semver guarantee: they are public
API, not "test-only, may change or vanish any time" hidden internals. Their
own doc comments each carry a "# Stability" section stating this
explicitly.

## License

MIT OR Apache-2.0.
