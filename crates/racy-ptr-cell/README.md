# racy-ptr-cell

A lazy, CAS-published pointer cell — `UNINIT → INITIALIZING → READY` over a
single `AtomicPtr<T>` — with **fallible init, OOM rollback, and loser re-race**.

It fills the niche `std::sync::OnceLock` cannot:

- **`no_std`, allocation-free** — the cell is one `AtomicPtr`; it never touches
  the heap.
- **safe inside a `#[global_allocator]`** — no `std` sync primitive, no
  parking, no reentrancy, so it can publish a process-`'static` pointer *before
  any heap exists* without re-entering the allocator it is bootstrapping.
- **fallible with rollback + re-race** — on winner OOM the sentinel rolls back
  to `null` and losers re-race the CAS (unlike `OnceLock`, which poisons/blocks
  a failed initialiser).

```text
static CHUNK: RacyPtrCell<Chunk> = RacyPtrCell::new();

let chunk: Option<NonNull<Chunk>> = CHUNK.get_or_try_init(|| {
    // OS reservation etc.; return None on OOM to roll back and let losers re-race.
    reserve_and_init() // -> Option<NonNull<Chunk>>
});
```

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
under `--cfg loom`), including `#[should_panic]` counterfactuals that fail
without the correct code:

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
  Cargo feature (e.g. `test-probes`) — was rejected as disproportionate:
  both methods are already exercised unconditionally by this crate's own
  `tests/cell_unit.rs` and `tests/loom_racy_ptr_cell.rs`, so feature-gating
  them would require restructuring the whole existing test suite behind an
  opt-in flag (plus a corresponding CI matrix addition) for two methods
  whose only cost is a documented, semver-guaranteed public surface.

Both methods carry the crate's ordinary semver guarantee: they are public
API, not "test-only, may change or vanish any time" hidden internals. Their
own doc comments each carry a "# Stability" section stating this
explicitly.

## License

MIT OR Apache-2.0.
