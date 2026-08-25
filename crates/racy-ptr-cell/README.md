# racy-ptr-cell

A lazy, CAS-published pointer cell — `UNINIT → INITIALIZING → READY` over a
single `AtomicPtr<T>` — with **fallible init, OOM rollback, and loser re-race**.

It fills the niche `std::sync::OnceLock` cannot:

- **`no_std`, allocation-free** — the cell is one `AtomicPtr`; it never touches
  the heap.
- **safe inside a `#[global_allocator]`, on its success paths** — no `std`
  sync primitive, no parking, no allocation, so it can publish a
  process-`'static` pointer *before any heap exists* without re-entering the
  allocator it is bootstrapping. Its two panic paths (sentinel-collision,
  unwinding `init`) allocate under `std` and are NOT reentrancy-safe — treat
  both as fatal-by-construction.
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
