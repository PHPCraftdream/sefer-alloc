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
- **fallible without blocking** — on winner OOM the sentinel rolls back to
  `null` and losers re-race the CAS themselves. `OnceLock::get_or_init` cannot
  fail at all, its `get_or_try_init` is still unstable, and both may block the
  losing threads for the duration of the winner's init (blocking is the
  documented contract; the mechanism `std` uses is an implementation detail).

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

**The rule for a multithreaded POSIX process is the POSIX rule, and this
crate adds nothing to it: after `fork()`, the child may call only
async-signal-safe functions until a successful `exec()`; if `exec()` fails,
terminate through an async-signal-safe path such as `_exit`.** That means no
Rust allocator, no `get_or_try_init`, no `init` closure, no panic path, and
no other ordinary Rust code in the child before `exec()` — the child inherits
the whole address space, including every lock and resource state left behind
by threads that do not exist in it, and POSIX specifies that a function is
not async-signal-safe unless explicitly documented to be
([POSIX `fork()`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html),
[async-signal-safety](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap03.html)).

There is a narrower, cell-local invariant worth stating separately, because
it is the part this crate can speak to at all: **`fork()` must not race any
thread's `init`, anywhere in the process** — not just once, before some
notional "first" fork; every subsequent `fork()`, and every cell created or
reset afterward, is bound by it. A process-wide barrier establishes it: every
initializer holds the barrier's shared side for the whole duration of its
`init`, and the forking thread takes it exclusively — which by construction
both waits for quiescence and blocks new inits — calls `fork()` **while still
holding it**, and releases it only after `fork()` returns. (Acquiring,
observing quiescence, releasing, and only then forking leaves a window in
which a fresh `init` starts before the fork; holding across the call is the
load-bearing part.)

**That barrier prevents exactly one thing: a child snapshotting a cell wedged
at `INITIALIZING` with no thread alive to finish it. It does NOT make the
allocator, this cell, or Rust runtime code callable in the child before
`exec()`** — inherited allocator and runtime locks are untouched by it, and a
`get_or_try_init` call in the child is a non-async-signal-safe call regardless
of what any cell's state word says. Anything broader than the POSIX rule above
is an environment-specific contract you own, and owes a fully proven `atfork`
protocol covering every affected resource, not just these cells.

Do not allocate in a signal handler.

The panic sites, independently:

| # | Panic site | Whose code | Reaches the panic runtime? | Message shape | Allocations before a non-allocating hook (measured — see below) |
|---|---|---|---|---|---|
| 1 | sentinel-collision `assert!` in `get_or_try_init` | this crate | yes | bare `&'static str` | 0 |
| 2 | an unwinding `init` closure | **yours** | yes | whatever you wrote | 0 if a bare literal, ≥ 2 if formatted |
| 3 | `align_of::<T>() >= 2` in `new`, `static` form | this crate | **no** — const-eval failure, compile time | n/a | n/a |
| 3 | `align_of::<T>() >= 2` in `new`/`default`, non-const form | this crate | yes | bare `&'static str` | 0 |

**Normative contract, separate from the measurement below: `init` must not
panic, full stop.** The `std` panic path *may* allocate before any hook
runs — especially for a formatted message — so the absence of an allocation
is never something to rely on. The numbers in this table and the paragraph
below are a measurement (rustc 1.97, `x86_64-pc-windows-msvc`, `--release`,
`RUST_BACKTRACE=0`, one specific non-allocating hook), not an API guarantee
about the panic runtime, this crate's MSRV, other `std` implementations/
targets, or future toolchains.

The two link environments need genuinely different mitigations, **not a
shared recipe**:

- A `no_std` binary supplies its own `#[panic_handler]`. Written not to
  allocate, it closes the hazard completely: the whole panic path is yours,
  so nothing on it can re-enter the allocator.
- A `std` binary's `panic = "abort"` profile setting removes the **unwind**
  (the UB when the frame below is `GlobalAlloc::alloc`), but it does **not**
  stop the panic runtime from allocating: with the DEFAULT hook, every panic
  sampled here allocated before it could print anything (measured: 2
  allocations under `panic = "abort"`, `RUST_BACKTRACE=0`, `--release`,
  rustc 1.97, x86_64-pc-windows-msvc). Inside a `#[global_allocator]` that
  allocation re-enters the very cell that is mid-`init`, and the thread
  deadlocks on its own sentinel instead of aborting. A `std` consumer
  therefore needs `panic = "abort"` **and** a `std::panic::set_hook` that
  goes straight to `std::process::abort` without formatting — or, better, an
  `init` that cannot panic at all. **Residual limit: a hook cannot help if
  the panic message is formatted.** `std` materialises the message as an
  *argument* to the hook call, so `unwrap`/`expect`/`assert_eq!`/
  `panic!("{}", …)` allocate before any hook runs — measured: 2 allocations
  for `Result::unwrap`, 4 for `assert_eq!`, with the same non-allocating
  hook that reaches 0 for a bare-`&'static str` panic. Only a panic whose
  message is a bare `&'static str` was measured allocation-free under that
  hook. The crate's own two `assert!`s (the sentinel-collision check and
  the `align_of::<T>() >= 2` check) are of that shape and measure 0
  allocations before the hook; **an unwinding `init` is your code, and its
  message is whatever you wrote**, so it is covered only if you keep it a
  bare literal — and even then only as a measured observation on one
  toolchain, not a promise. **The only mitigation the contract actually
  rests on is an `init` that cannot panic at all.** Note also that
  `panic = "abort"` compiles this crate's internal rollback guard out
  entirely (it is unwind-only) — under this profile the cell-consistency
  guarantee above comes from the process dying, not from the guard.

## Portability limit — requires pointer-width atomic CAS

The whole cell is one `AtomicPtr<T>` driven by `compare_exchange`, so this
crate needs `target_has_atomic = "ptr"` and will **not compile** on a target
without it. `thumbv6m-none-eabi` (Cortex-M0/M0+) and
`riscv32imc-unknown-none-elf` (no `A` extension) have load/store atomics but
no CAS; `msp430-none-elf` has no atomics at all. `no_std` and
allocation-free do not imply pointer-width CAS. A build on an unsupported
target fails with an explicit `compile_error!` naming the requirement, and
with **nothing else**: the implementation carries the positive
`#[cfg(target_has_atomic = "ptr")]`, so its body is not compiled there at
all. That replaces the "no method named `compare_exchange`" cascade an
unguarded build would produce on
`thumbv6m-none-eabi`/`riscv32imc-unknown-none-elf`, and the unresolved
`AtomicPtr` import on `msp430-none-elf` (which has no atomics for `core` to
define it from), with one sentence naming the real requirement.

## Layout — `#[repr(transparent)]`

`RacyPtrCell<T>` carries `#[repr(transparent)]`: its layout is guaranteed
identical to `AtomicPtr<T>` — same size, same alignment. This is a real
contract, not merely an observation about the current compiler: the "one
`AtomicPtr`"/"one word" language throughout this crate's docs would
otherwise describe an unstated detail of plain `repr(Rust)` layout (field
order, padding, and single-field size equivalence are not guaranteed
there), which is not something to leave implicit for a type meant to sit in
allocator metadata or an array of cells.

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
RUSTFLAGS="--cfg loom" cargo test -p racy-ptr-cell --release --test loom_racy_ptr_cell
```

**`--cfg loom` is a global `RUSTFLAGS` cfg — it applies to every crate in the
build, not only this one.** Under it, this crate's atomics become
`loom::sync::atomic`, so `RacyPtrCell::new` is **not** `const`, and a
`static CELL: RacyPtrCell<T> = RacyPtrCell::new();` — this README's own
usage example — fails to compile anywhere in that build. Always scope the
flag to this crate (`-p racy-ptr-cell`, as above), or supply a `#[cfg(loom)]`
const-capable stand-in in your own crate if you need to set the flag
workspace-wide — see [`sefer-alloc`'s own
`loom_shim`](https://github.com/PHPCraftdream/sefer-alloc/blob/main/src/registry/bootstrap.rs)
for a worked example.

## Test-probe API stability

`RacyPtrCell` exposes two `dbg_`-prefixed methods — `dbg_is_ready` and
`dbg_rollback_reenterable` — that are NOT hidden from `rustdoc`. This is a
deliberate posture decision, not an oversight:

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
  Cargo feature (e.g. `test-probes`) — was rejected. The cost is small
  (`[[test]] required-features` exists; the real cost is only the
  corresponding CI matrix addition), but so is the benefit: neither method
  accepts a raw pointer or touches allocator metadata, which is the hazard
  a feature gate would be protecting against.
- `dbg_is_ready` is functionally identical to `get().is_some()` — same
  single `Acquire` load, same predicate. It stays public because it has a
  real, exercised caller downstream, not because it offers any capability
  `get` lacks: it reads as a self-documenting boolean assertion
  (`assert!(cell.dbg_is_ready())`) at a call site that would otherwise need
  an `.is_some()`/`.is_none()` match.

Both methods carry the crate's ordinary semver guarantee: they are public
API, not "test-only, may change or vanish any time" hidden internals. Their
own doc comments each carry a "# Stability" section stating this
explicitly.

## License

MIT OR Apache-2.0.
