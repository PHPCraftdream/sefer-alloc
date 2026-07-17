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

## License

MIT OR Apache-2.0.
