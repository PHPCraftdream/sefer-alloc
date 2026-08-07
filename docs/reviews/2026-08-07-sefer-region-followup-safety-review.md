# `sefer-region` — follow-up safety review: new ground beyond the round's settled findings (read-only)

**Date:** 2026-08-07
**Scope:** `crates/region` (crate `sefer-region`) — memory safety, panic-safety, Send/Sync
soundness, dependency risk. One of three parallel lanes (this one: safety; siblings:
performance, logic/correctness).
**Mode:** read-only investigation. No repository file was modified; the only write is this
report. All runtime/compile probes were built and run in a scratch cargo project under the
OS temp directory (`%TEMP%\region_safety_probe`, path-dependency on `crates/region`,
**deleted after the run**); their observed output is inlined verbatim below.
**Reviewed tree:** `main` @ `aa24f84`, `crates/region` clean (`git status --porcelain` empty
for the crate before and after).
**Relationship to prior reviews:** this report deliberately does NOT re-litigate the ground
settled by `docs/reviews/2026-08-07-sefer-region-safety-review.md` (original audit) and
`docs/reviews/2026-08-07-sefer-region-round-closing-review.md` (GO-WITH-FIXES closure):
zero-`unsafe` confirmation, `clear()` partial-clear-under-panicking-`Drop` (fixed + tested),
`Handle`'s unconditional Send+Sync (pinned by `tests/handle_static_asserts.rs`), the
poisoning-recovery policy as policy, and ABA-after-~2^31-cycles. Each section below is
either genuinely new ground or an explicitly-invited independent re-verification.

---

## Verdict: **no memory-safety finding at any severity; one NEW liveness footgun (documentation-severity) and one NEW doc-accuracy nit**

After probing the angles the settled round left open — dev-dependency supply chain,
panic-safety in `SyncRegion` methods *other than* `clear()`, genuinely contended
multi-threaded use with panicking drops (task #673's unexplored territory, safety lens),
reentrancy, and `Handle` covariance — the honest conclusion is that **the crate's
memory-safety story holds under everything thrown at it**, including a 9-thread contended
stress with drop bombs that preserved drop-once exactly (20,000/20,000). The two items
worth acting on are both about what the docs *say*, not what the code *does*:

1. **§4 — reentrant one-shot self-deadlock** (footgun, liveness-only): calling any one-shot
   `SyncRegion` method while the same thread holds a guard from the same `SyncRegion`
   deadlocks — empirically confirmed in **both** directions on Windows — and no doc
   sentence warns about it.
2. **§2.2 — the poisoning-policy sentence is overbroad**: read-guard panics do **not**
   poison an `RwLock` (empirically confirmed against std), so `sync_region.rs:23`'s
   "A panic while a guard is held poisons the `RwLock`" overstates — in the *favorable*
   direction, but a safety-policy paragraph should be exact.

Severity legend (same as the original review): **impossible** / **contrived** / **footgun**.

---

## 1. Dependency risk beyond `slotmap` — the dev-dependency tree, checked for the first time

The original review's §5 analyzed `slotmap` only (resolved 1.1.1, no RUSTSEC entry, `"1"`
range posture correct — settled, not re-derived here). What it did **not** look at:
`crates/region/Cargo.toml:19-31` also pulls two registry **dev-dependencies** that no
review in this round has examined:

- **`bench-scale-tool 0.1.0`** (`Cargo.lock`: registry, checksum `745fdc4…`) — no
  transitive dependencies of its own in the lock graph. Bench harness only
  (`benches/region_bench.rs`).
- **`captrack 0.1.1`** (`Cargo.lock`: registry, checksum `9e21ed7…`) — a substantially
  larger tree: `bytes`, `captrack-macros` (proc-macro), **`ctor`**, `dashmap`, `fastrand`,
  `hashbrown 0.15.5`, `indexmap`, `scc`, `serde`, `serde_json`, `smallvec`.

**Assessment — informational, no action needed, with one caveat worth knowing:**

- Neither reaches published consumers: dev-dependencies do not ship downstream, so the
  crates.io artifact's dependency tree remains exactly `slotmap` (matching the crate
  description's "no C/C++ libraries pulled in" claim for consumers).
- Both are inside `cargo-deny`'s scope (`deny.toml` checks the full lock graph;
  `[sources] unknown-registry = "deny"` pins both to crates.io). **Re-run for this review,
  today, on this tree: `cargo deny check advisories` → `advisories ok`.** So no RUSTSEC
  advisory currently touches either crate or anything they pull in.
- The caveat: `captrack` pulls **`ctor`** — life-before-main code execution in any test
  binary that links it. Here that is only `tests/captrack_probe.rs` (the one `#[ignore]`d
  telemetry probe; unused dev-deps are not linked into the other test binaries). This is
  the standard risk shape of any young 0.1.x registry crate with a proc-macro + `ctor`
  surface running at developer/CI test time: the *mechanism* for a supply-chain problem
  exists, the *evidence* of one does not, and the existing `cargo-deny` CI job is the
  correct standing control. Named here so the exposure is a recorded decision rather than
  an unexamined default. Severity: **informational**.

## 2. Panic-safety in `SyncRegion` methods other than `clear()` — new ground, probed

The round's panic-safety work (`tests/clear_partial_under_panic.rs`) covers `clear()`
exclusively. The remaining lock-holding surfaces:

### 2.1 `get_cloned` with a panicking `T::Clone` — container fully unaffected (verified)

`get_cloned` (`sync_region.rs:139-144`) runs the user's `Clone` while the **read** guard
is held (the guard temporary lives to the end of the statement). Probe P2 inserted a value
whose `Clone` panics and called `get_cloned` on it from a spawned thread:

```text
P2. get_cloned(bomb) panicked = true
P2. post-panic: len = 2, get_cloned(good) = Some(7), contains(bad) = true
P2. post-panic insert works: Some(99)
```

The panic unwinds through the read guard's drop (the reader count is correctly
decremented — the subsequent `insert`, which needs the *write* lock, succeeds, proving no
stuck reader), the container is byte-for-byte untouched (a `Clone` only ever sees `&T`),
both values remain live, and — see §2.2 — the lock is not even poisoned. The bomb value
itself stays live and will panic again on the next `get_cloned`; that is the caller's own
type's behavior, not this crate's. **No finding**; this is the best possible outcome for
this path.

### 2.2 NEW doc-accuracy nit: read-guard panics do not poison — the policy paragraph overstates

`sync_region.rs:23` opens the poisoning policy with: *"A panic while a guard is held
poisons the `RwLock`."* That is true only for **write** guards. std documents — and probe
P1 confirmed empirically against `std::sync::RwLock` directly — that a panic while holding
a **read** guard does not poison:

```text
P1. after read-guard panic:  is_poisoned = false
P1. after write-guard panic: is_poisoned = true
```

Consequence for this crate: a panic inside `get_cloned`/`contains`/`len`/`is_empty` (the
read-locking one-shots) never even engages the recovery path — which is strictly
*favorable* (nothing to recover; readers cannot have mutated anything). But the poisoning
policy is the crate's central safety-story paragraph, and it should not claim a stronger
trigger condition than std provides. One-word fix: "A panic while a **write** guard is
held poisons the `RwLock` (read-guard panics do not poison — readers cannot mutate)."
Severity: **nit** (doc-only; the inaccuracy is in the safe direction).

### 2.3 `insert`/`remove` panicking while holding the write guard — already sound, cited not re-derived

A panic inside `SyncRegion::insert` (allocation failure, or the documented full-map panic
at 2^32 − 2 entries, `sync_region.rs:81-83`) or inside slotmap's `remove` bookkeeping
occurs under the write guard → poison → recovery. The original review's §3.1 already
line-verified slotmap 1.1.1's insert ordering ("create new slot before adjusting freelist
in case f or the allocation panics") and that `remove` completes all bookkeeping before
the value leaves — so the recovered `Region` is consistent by slotmap's own construction.
Nothing new to add; recorded here only so this surface is explicitly listed as walked.

## 3. Contended `SyncRegion` under a safety lens (task #673's unexplored angle) — no soundness gap found

Task #673 deferred a contended *measurement*; no prior test in this crate exercises
genuinely concurrent readers and writers at all (every existing multi-thread test is
sequential: spawn one thread, join it, then assert). Probe P3 ran the first genuinely
contended mix: **4 writer threads** (5,000 inserts each, every ~97th value a drop bomb;
interleaved removes whose returned bombs detonate in the writer's own frame under
`catch_unwind`), **4 reader threads** (20,000 iterations each of `read()` +
`len`/`iter().take(64)`/`is_empty`), and **1 clearer thread** (50 `clear()` calls, each
racing live bombs → repeated mid-`clear` panics, poison, and recovery), all on one shared
`Arc<SyncRegion<D>>` with global construction/drop counters. Release build, Windows:

```text
P3. after stress: constructed = 20000, dropped = 11830, live = 8170, c-d == live: true
P3. final: constructed = 20000, dropped = 20000, drop-once holds: true
```

- **I5 (drop-once) held exactly** across ~50 poison/recovery cycles and 9 threads:
  20,000 constructions, 20,000 drops, zero double-drops, zero leaks.
- **I4 held at quiescence:** `constructed − dropped == len()` exactly after joins.
- No wrong-value resolution, no panic outside the intentional bombs, no wedged lock.

**On the loom question the brief raised:** this run does not *prove* absence of races the
way loom would, but the original review's §6.1 reasoning survives contact with this
experiment: `SyncRegion` contains no atomics, no `unsafe`, no hand-rolled
synchronization — its entire interleaving surface is `std::sync::RwLock` itself, which
loom does not model any better than std tests do (loom's value is for *hand-rolled*
atomics). The probe above is the shape a permanent contended test would take if task #673
is ever promoted from measurement to test; from a pure-safety standpoint, **nothing here
blocks anything**. Severity: **no finding**.

## 4. NEW finding — reentrant one-shot call while holding a guard: self-deadlock, undocumented

**The one genuinely new behavioral hazard this review found.** Every one-shot convenience
method acquires the lock internally (`insert` `sync_region.rs:84`, `remove` `:91`,
`contains` `:104`, `len` `:113`, `is_empty` `:119`, `clear` `:130`, `get_cloned` `:139`).
If the calling thread already holds a guard from the *same* `SyncRegion` — obtained via
`read()`/`write()` (`:64`/`:72`) for a multi-op transaction, exactly as the docs
recommend — the one-shot call deadlocks that thread. Probe P5 confirmed **both** shapes
empirically (watchdog pattern, 500 ms grace, release build, Windows/SRWLock):

```text
P5a. write-guard + one-shot len() on same thread completed = false (false = deadlocked)
P5b. read-guard + one-shot insert() on same thread completed = false (false = deadlocked)
```

P5b is the sneakier one: std's `RwLock` documents that even a *read* lock "might" block
behind the acquiring thread's own pending write — and on this platform it simply does,
with no panic and no error, just a silent hang. The concrete trap for a normal caller: a
helper function that takes `&SyncRegion<T>` and calls `sr.len()` internally, invoked from
inside a `let g = sr.write();` transaction block — compiles cleanly, hangs at runtime.

- **Not memory unsafety** — no UB, no corruption; the process hangs, it does not
  misbehave. And it is inherent to *any* non-reentrant-lock API (std's own `RwLock` docs
  carry the same caveat), so there is nothing to fix in code short of a redesign nobody
  should want for this crate.
- **But it is currently undocumented** in a crate whose docs *actively steer* users toward
  mixing the two API styles ("use `read`/`write` for multi-operation transactions … or the
  one-shot convenience methods", `sync_region.rs:16-19`) without saying they must not be
  nested on one thread. The original review's §2 noted only the `mem::forget`-a-guard
  variant; the reentrancy variant is new.

**Recommendation (doc-only, one sentence in the type-level doc block):** *"Do not call a
one-shot method while the same thread holds a `read()`/`write()` guard from the same
`SyncRegion` — the one-shots lock internally and the nested acquisition deadlocks (std's
`RwLock` is not reentrant); finish the transaction through the guard you already hold."*
Severity: **footgun (liveness), documentation-severity fix**. Folds naturally into the
same docs-only patch release as the round's other doc items.

## 5. Cross-instance handle acceptance — verified no safety-adjacent escalation exists

The brief asked for explicit verification, not assumption, that the known cross-`Region`
handle-confusion logic hazard (`tests/smoke.rs:98-131`, disclosed at `lib.rs:17-21`)
cannot compose with anything else into something worse than "wrong value." Walked
explicitly:

- **Wrong-region `remove` cannot double-drop:** `remove` transfers ownership of whichever
  (valid, live) `T` it resolves — slotmap retires that slot's bookkeeping before returning
  the value (original review §2.1). The value drops once in the caller's frame; the
  victim region's accounting is decremented for the same slot. Both regions remain
  internally consistent; only the *caller's beliefs* are wrong.
- **Combined with poison recovery:** a wrong-region `remove` through `SyncRegion` behaves
  identically — the lock serializes it; recovery hands back a region that is consistent
  about the (wrong) removal.
- **Combined with `Handle` covariance (new check):** `PhantomData<fn() -> T>` makes
  `Handle<T>` covariant in `T` (documented, `handle.rs:12-13`). Probe P4 confirmed
  `Handle<&'static str>` coerces to `Handle<&'a str>` and resolves fine. This adds no
  hazard: a handle carries only an index + generation, zero `T` data or provenance;
  variance on a pure ID type cannot launder lifetimes or types into or out of any
  `Region`, whose own access surface (`get`/`get_mut`/`remove`) only ever produces the
  region's *own* `T` under the region's *own* borrows.
- **The structural ceiling:** with `#![forbid(unsafe_code)]` (`lib.rs:55`) and no path
  that trusts a handle for anything but a checked slotmap lookup, the worst reachable
  outcome of *any* handle confusion is a valid `T` from the wrong logical slot. There is
  no pointer arithmetic, no index trusted without a generation check, no capacity
  assumption keyed off a handle. Severity: **impossible** (as a safety issue; the logic
  hazard itself remains the logic lane's settled ground).

## 6. Negative Send/Sync direction — independently re-verified (invited by the brief)

The round pinned only the *positive* direction in-tree (`tests/handle_static_asserts.rs`);
the negative direction was manually verified once in the original review's §4 and cited
since. Re-verified here from scratch in the scratch project (two `src/bin/` probes expected
to fail compilation, both failed exactly as required):

```text
error[E0277]: `Cell<u32>` cannot be shared between threads safely
  = help: within `Region<Cell<u32>>`, the trait `Sync` is not implemented for `Cell<u32>`
  (assert_sync::<SyncRegion<Cell<u32>>>() — the load-bearing case: Cell is Send but !Sync)

error[E0277]: `Rc<u32>` cannot be sent between threads safely
  = help: within `Region<Rc<u32>>`, the trait `Send` is not implemented for `Rc<u32>`
  (assert_send::<SyncRegion<Rc<u32>>>())
```

`SyncRegion<T>: Sync` still correctly requires `T: Send + Sync`; the bounds have not
silently widened since the original review. **Independent confirmation, no finding.**

## 7. Gates re-run + explicit no-findings list

- `cargo test -p sefer-region` @ `aa24f84`: **28 passed, 0 failed, 1 ignored** (all seven
  binaries; matches the round-closing review's count). Run fresh for this review.
- `cargo deny check advisories`: **`advisories ok`** (full workspace lock graph, including
  the §1 dev-deps). Run fresh for this review.
- Checked and found clean, beyond the sections above: no `unsafe` token anywhere in
  `crates/region/src/` (re-grepped; `forbid` at `lib.rs:55` makes one a compile error);
  `extern crate alloc` + no_std path adds no `std`-only assumption outside the
  `cfg`-gated `sync_region.rs`; `Handle`'s `Eq`/`Hash` remain key-delegating with no
  `T`-dependent behavior; no new public surface accepts anything pointer-shaped.

---

## Bottom line

**Publish-safe from the memory-safety lane, with receipts on previously-unprobed ground.**
The crate survived its first genuinely contended multi-threaded exercise (9 threads,
drop bombs, ~50 poison/recovery cycles) with drop-once and accounting exact; panicking
`Clone` under a read guard leaves the container untouched and the lock unpoisoned; the
negative Send/Sync direction still fails to compile; and the dev-dependency tree is
advisory-clean under the standing `cargo-deny` control. The two actionable items are
doc-only: the reentrancy deadlock warning (§4 — the one real footgun found, liveness not
safety) and the read-guard poisoning precision (§2.2), both one-sentence edits that fold
into the docs-only patch release already planned for this crate.
