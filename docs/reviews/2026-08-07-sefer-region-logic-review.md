# `sefer-region` — logic & correctness review (read-only)

**Date:** 2026-08-07
**Scope:** `crates/region` (crate `sefer-region` 0.1.0), invariants I1–I5, `SyncRegion`
poison policy, API contract edges, test coverage, feature gating, doc-vs-code drift —
ahead of the recommended 0.1.1 republish.
**Mode:** read-only investigation. No repository file was modified except this report.
Three throwaway empirical probes were built and run in a temp directory **outside** the
repo (`%TEMP%\sefer-region-aba-probe`, path-dependency on `crates/region`, deleted after
use); their exact source is reproduced verbatim in this report so every measured claim is
reproducible without any committed artifact.
**Reviewed tree:** `main` @ `2fcf1201819834943f584528ff6c8231d0d629c8`;
`git status --porcelain -- crates/region/` is empty (the crate itself is clean).
**Dependency examined at source level:** `slotmap 1.1.1`
(`Cargo.lock` checksum `bdd58c3c…`, registry copy at
`$CARGO_HOME/registry/src/*/slotmap-1.1.1/`).

---

## Verdict

One **confirmed, empirically reproduced doc-vs-code contradiction on a headline safety
claim** (F1 — the "Generation saturation / no ABA ever" story is false; a full ABA alias
reproduces in **12 seconds** of hot churn), one real CI gap on an advertised feature
configuration (F2), one confirmed-but-nuanced gap in the poison-recovery story (F3), two
verified low-severity contract-edge inaccuracies (F4, F5), and a cluster of concrete test
coverage gaps (F6) — most notably that **I5 (drop-once) has zero tests**. All are
docs/tests-level fixes; **no memory-unsafety and no wrong-result bug in the ordinary
(non-wrapped, non-panicking) operating envelope was found** — I1 and I4 verified clean
against slotmap 1.1.1 source, including its panic-mid-insert exception safety.

Three prior doc-accuracy passes (`2e88262`, `aab617a`, `b17ffab`) fixed the
cross-instance overclaim but **missed F1 entirely** — the 2026-08-06 publish-readiness
review even cites the Generation-saturation section as a *positive*
(`docs/reviews/2026-08-06-sefer-region-publish-readiness-review.md:253-256`) without
checking it against slotmap's source. Since a 0.1.1 docs-only patch is already planned,
F1's rewrite should ride in it.

---

## F1 — CONFIRMED (empirical): the "Generation saturation" doc is factually false — slotmap does NOT retire slots; versions WRAP, and full ABA is reproducible in ~12 s

**Severity: high as a published-claim defect; low-probability as a field bug — but the
window is minutes, not "astronomical".** Memory safety is unaffected (slotmap's own doc:
"in all circumstances is the behavior safe"); this is a logic/contract defect.

### The claim (three places)

- `crates/region/src/region.rs:34-41` — "## Generation saturation … `DefaultKey`
  **retires such a slot rather than wrapping** a generation into alias, so a handle can
  **never** alias a future value — the classic generational-arena ABA caveat **stays
  closed** at the astronomically rare cost of one slot per **2^32** reuses."
- `crates/region/src/lib.rs:29-31` and `crates/region/src/region.rs:24-27` — I3: a stale
  handle "**never** resolves to a live value."
- `crates/region/README.md:65-68` — same I3 "never".

(The task brief listed generation saturation as "handled by slotmap itself (retires the
slot)" — a known/documented item. The finding here is that **the documented claim itself
is wrong about what slotmap does**, which is squarely doc-vs-code drift, not a re-report.)

### What slotmap 1.1.1 actually does

There is **no retirement code anywhere in `SlotMap`**:

- `basic.rs:436-448` (`remove_from_slot`): the freed slot is pushed onto the freelist
  **unconditionally** (`slot.u.next_free = self.free_head; self.free_head = idx`), and
  the version is bumped with `slot.version = slot.version.wrapping_add(1)` — explicitly
  wrapping.
- `basic.rs:396-397` (`try_insert_with_key`, reuse path): re-occupation sets
  `occupied_version = slot.version | 1` — no upper-bound check, ever.
- slotmap's **own crate doc admits the wrap** (`lib.rs:110-113` of slotmap 1.1.1):
  "After 2^31 deletions and insertions to the **same underlying slot** the version wraps
  around and such a spurious reference could potentially occur."

So even the *count* in region.rs is wrong twice over: it is 2^31 cycles (each
occupy/free cycle advances the version by 2 through a u32), not 2^32, and the outcome at
saturation is **alias**, not retirement.

### Empirical reproduction (12.1 seconds, release, ordinary dev host)

slotmap's freelist is LIFO, so a tight `insert; remove` loop on an otherwise-empty
region reuses **the same physical slot every iteration** — the worst case is the common
case for single-entry churn. Probe (built outside the repo, deleted after; slotmap
1.1.1, `--release`, default profile):

```text
use sefer_region::Region;
use std::time::Instant;

fn main() {
    let mut r: Region<u64> = Region::new();
    let h_old = r.insert(111);
    r.remove(h_old);
    let t = Instant::now();
    let cycles: u64 = (1u64 << 31) - 1;
    for i in 0..cycles {
        let h = r.insert(i);
        r.remove(h);
    }
    let h_new = r.insert(999);
    println!("churn of {} cycles took {:?}", cycles, t.elapsed());
    println!("get(h_old) after wrap = {:?}", r.get(h_old));
    println!("h_old == h_new ? {}",
        format!("{:?}", h_old) == format!("{:?}", h_new));
    assert_eq!(r.get(h_old).copied(), Some(999), "ABA alias did NOT occur");
    let stolen = r.remove(h_old);
    println!("remove(stale h_old) = {:?}", stolen);
}
```

Output (verbatim):

```text
churn of 2147483647 cycles took 12.1154819s
get(h_old) after wrap = Some(999)
h_old == h_new ? true
remove(stale h_old) = Some(999)
```

The stale handle became **bit-identical** to a fresh live handle: it resolves to a value
it never named (I3 violated), and `remove(h_old)` — a handle removed 2^31 cycles earlier
— **steals the live value** (I2's "None forever" violated). Deterministic, single
threaded, 12 seconds on this host. With a heavier `T` the window stretches (at 1 µs per
cycle it is ~36 minutes), but "a long-running process with one hot churn point and one
long-lived cached handle" is a real consumer shape, not an astronomical one.

### Recommended fix (docs-only, fits the planned 0.1.1)

Rewrite `region.rs:34-41` to state the truth slotmap itself states: versions are 32-bit
and **wrap after 2^31 occupy/free cycles of the same slot**, after which a
sufficiently-stale handle **can** spuriously resolve to (or remove) a live value; memory
safety is never affected. Soften the absolute "never" in I3 at all three cites
(`region.rs:24-27`, `lib.rs:29-31`, `README.md:65-68`) to "never within a slot's 2^31
reuse budget" or equivalent. Optionally note the LIFO-freelist hot-slot worst case,
since it is exactly the scenario a naive reader would dismiss.

---

## F2 — CI gap: `no_std` is a headline claim (README, keywords, categories) but `sefer-region --no-default-features` is never built anywhere in CI

**Severity: medium — advertised configuration with zero CI signal.**

- Claim: `README.md:31` ("With `default-features = false` the crate builds under
  `no_std + alloc`"), `README.md:102-112` (feature-flag section), `lib.rs:44-48`,
  `Cargo.toml:12-13` (`keywords = [... "no-std"]`, `categories = [... "no-std"]`).
- Reality in `.github/workflows/ci.yml`:
  - The `no_std` job (`ci.yml:785-799`) builds **only the root crate**
    (`cargo build --no-default-features --target thumbv7em-none-eabi`, no `-p`).
  - The `test-workspace` job tests `sefer-region` **default-features only**
    (`ci.yml:708`).
  - The P5 publish-readiness pass (task #639) added exactly this kind of bare-metal
    build for `size-classes` and `racy-ptr-cell` (`ci.yml:753-754`) — and its own
    comment (`ci.yml:741-752`) explains the gap class — but **skipped `sefer-region`,
    the one workspace member that actually has a `std` feature to disable**.
- Verified locally: `cargo build -p sefer-region --no-default-features` compiles clean
  (host target; the `#![no_std]` cfg_attr is active, so in-crate `std::` paths would
  fail). A bare-metal proof was not run locally (target not installed), but the crate is
  structurally identical to the root-crate pattern the existing job already proves.

**Fix:** one line in the `test-workspace` (or `no_std`) job:
`cargo build -p sefer-region --no-default-features --target thumbv7em-none-eabi`.

---

## F3 — CONFIRMED (empirical): poison recovery is structurally sound as documented, but silently converts a panicking `clear()` into a *partial* clear, and silently exposes half-completed `write()` transactions

**Severity: medium-low — the documented policy's justification is true but incomplete;
the caller-visible contract of `clear` can silently fail.**

### What is provably safe (the doc's claim holds)

`sync_region.rs:24-30` claims a poisoned `Region` is "still structurally valid — no
broken memory invariants." Verified against slotmap 1.1.1 source, this is **true**, and
not merely assumed:

- `remove` (`basic.rs:436-448`): all bookkeeping (freelist push, `num_elems -= 1`,
  version bump) completes **before** the value is returned; the value's `Drop` runs in
  the caller after the map is already consistent.
- `insert` (`basic.rs:394-431`): the value is materialized **before** any state is
  touched ("Get value first in case f panics"; "Create new slot before adjusting
  freelist") — a panicking constructor/clone cannot tear the map.
- `clear` (`basic.rs:615-617`) is `self.drain()`; `Drain::next` (`basic.rs:1106-1125`)
  fully completes `remove_from_slot` before returning the value, and `Drain`'s `Drop`
  (`basic.rs:1133-1137`) drops values one at a time via `for_each`. A `T::Drop` panic
  therefore lands **between** element removals, never inside slot bookkeeping. No torn
  state is reachable through `Region`'s API. (`mem::forget(drain())`, the one path
  slotmap documents as partially-clearing, is not exposed by `Region`.)

### What the policy papers over anyway (the gap)

Probe (same temp-dir setup as F1, `--release`):

```text
use sefer_region::SyncRegion;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

static DROPS: AtomicU32 = AtomicU32::new(0);
struct Panicky(u32);
impl Drop for Panicky {
    fn drop(&mut self) {
        if DROPS.fetch_add(1, Ordering::SeqCst) == 1 {
            panic!("intentional panic in 2nd Drop during clear");
        }
    }
}

fn main() {
    let sr: Arc<SyncRegion<Panicky>> = Arc::new(SyncRegion::new());
    sr.insert(Panicky(1));
    sr.insert(Panicky(2));
    sr.insert(Panicky(3));
    let sr2 = Arc::clone(&sr);
    let join = std::thread::spawn(move || sr2.clear());
    assert!(join.join().is_err());
    println!("len() after panicking clear() = {}", sr.len());
    println!("drops that ran = {}", DROPS.load(Ordering::SeqCst));
}
```

Output: `len() after panicking clear() = 1`, `drops that ran = 2`.

So `SyncRegion::clear`'s contract — "Removes **every** value, invalidating **all**
outstanding handles" (`sync_region.rs:106`) — **silently failed**: one value survived,
its handles still resolve, and because every accessor recovers from poison
(`sync_region.rs:57,65`), **no thread anywhere ever receives a signal** that the clear
was partial. The same reasoning applies to any multi-op `write()` transaction that
panics midway (e.g. inserted 1 of 2 paired values): the region is structurally intact
but *application-level* invariants of the stored data are torn, and the standard poison
signal — whose entire purpose is flagging exactly that — is unconditionally discarded.
I5 itself survives (the panicking value was dropped exactly once; the survivor drops
later on region drop).

**Fix (docs-level, honest option):** extend the poisoning-policy doc
(`sync_region.rs:22-30`) and `README.md:76-78` to say recovery guarantees *container*
integrity only — a panicking `T::Drop` during `clear` leaves later values live, and a
panicked `write()` transaction leaves its partial effects visible; callers whose `T`
carries cross-value invariants must add their own signal. An API-level option (a
`try_`/poison-propagating variant) exists but is not required for 0.1.1.

---

## F4 — CONFIRMED (empirical): `reserve`/`with_capacity` overflow edges contradict the documented panic in a default release build

**Severity: low (extreme-argument edge), but the doc claim is verifiably wrong as
stated.**

`region.rs:88` documents `reserve`: "Panics if the new allocation size overflows
`usize`" (inherited verbatim from slotmap's doc, `basic.rs:268-271`). slotmap computes
`(self.len() + additional).saturating_sub(self.slots.len() - 1)` (`basic.rs:282-285`) —
the `len + additional` sum is unchecked arithmetic. Probe results (default profiles,
slotmap compiled as dependency):

| Call | release (overflow-checks off) | debug |
|---|---|---|
| `Region::with_capacity(usize::MAX)` | **silently succeeds, `capacity()` = 3** (slotmap's `Vec::with_capacity(capacity + 1)`, `basic.rs:204`, wraps to 0) | panics "attempt to add with overflow" |
| `r.insert(1); r.reserve(usize::MAX)` | **silent no-op, capacity stays 3** (sum wraps, `saturating_sub` → 0) | panics "attempt to add with overflow" |
| `Region::new().reserve(usize::MAX / 2)` | panics "capacity overflow" (Vec) | panics "capacity overflow" |

So the documented panic *does* fire for a genuine allocation-size overflow (row 3), but
the parameter-arithmetic wrap (rows 1–2) **silently does nothing** in release — and row
1 also breaks `README.md:184-186`'s "**`with_capacity(n)` reserves exactly `n` up
front**" at the extreme (the returned region has effectively no pre-reservation, with no
panic and no signal). This is an upstream slotmap quirk that `Region`'s doc inherits and
amplifies. **Fix:** qualify the `reserve` doc (panic applies to allocation-size
overflow; near-`usize::MAX` arguments may silently no-op in release under slotmap 1.1)
or drop the panic sentence in favor of "delegates to slotmap's `reserve`."

---

## F5 — `Region::insert` can panic ("SlotMap is full") — undocumented

**Severity: low.** `region.rs:93-96` has no panic note; slotmap panics when the map
holds 2^32 − 2 elements (`basic.rs:413-414`). At 8 bytes/slot for small `T` that is a
~34 GiB backing vec — borderline reachable on large hosts, and `#[deny(missing_docs)]`
discipline elsewhere in this crate suggests the panic contract should be stated.
(`SyncRegion::insert`, `sync_region.rs:68-74`, inherits the same.)

---

## F6 — Test coverage gaps (claims with zero exercising tests)

All in `crates/region/tests/` (`smoke.rs`, `captrack_probe.rs`, `bench_ids_isolatable.rs`):

1. **I5 (drop-once) has NO test at all** — claimed at `region.rs:30-32`, `lib.rs:34-35`,
   `README.md:71-72`, yet no test in the crate instantiates a `Drop`-implementing type
   with a counter. Neither "dropped exactly once on `remove`," "dropped exactly once on
   `Region` drop," nor "never leaked" is verified. A 15-line counter test covers all
   three.
2. **`clear()` has NO test** — neither `Region::clear` (`region.rs:139-141`) nor
   `SyncRegion::clear` (`sync_region.rs:109-111`) is called by any test. Combined with
   F3, the crate's only linear-time mutation is entirely unexercised.
3. **`iter`, `iter_mut`, `get_mut`, `Default` (both types), `capacity`,
   `with_capacity`, `reserve` have no non-ignored test.** The capacity paths are touched
   only by `captrack_probe.rs`, which is `#[ignore]`d (`captrack_probe.rs:39`) and never
   runs in CI; `iter` is exercised only by the bench binary, which CI never executes.
4. **The I3 test can pass vacuously.** `smoke.rs:31-46` never asserts the slot was
   actually *reused* — if slotmap's freelist policy changed to not reuse, the test would
   still pass while testing nothing. `Handle`'s `Debug` output exposes `DefaultKey`'s
   `{idx}v{version}` form (slotmap `lib.rs:301`), so the test can cheaply assert the
   index component of `h_old` and `h_new` match (the same technique F1's probe used).
   This is the same path-activation-oracle discipline CLAUDE.md already mandates for
   benches.
5. **`Handle`'s "unconditionally `Send + Sync` regardless of `T`" claim
   (`handle.rs:10-11`) has no static assertion.** The claim is correct today
   (`fn() -> T` + `DefaultKey` are both unconditionally `Send + Sync`), but a
   `fn assert_send_sync<T: Send + Sync>() { assert_send_sync::<Handle<Rc<()>>>() }`
   compile-time check would pin it against a future field change.
6. `reserve`'s documented panic is untested — and per F4, wrong as documented; whichever
   way the doc lands, a `catch_unwind` test of the real behavior should pin it.

---

## F7 — Minor doc nits (docs.rs-facing)

1. `region.rs:10` and `lib.rs:6` cite `docs/BENCHMARKS.md` bare — a workspace-root file
   **not shipped in the crate tarball**; on docs.rs this is a dangling reference (the
   README version, `README.md:136`, correctly uses the full GitHub URL).
2. `sync_region.rs:12-14` — "Lock-free tiers (Phase 3b) are a later opt-in upgrade …
   until those land and clear loom/TSan" references the parent workspace's phase plan;
   meaningless (and mildly misleading, since nothing will "land" in this crate) to a
   standalone crates.io consumer.
3. `sync_region.rs:113-114` — `get_cloned` "Clones the value … **without holding a
   guard**" — it does hold a read guard internally for the duration of the clone; the
   intended meaning ("the *caller* doesn't hold one") is stated in the next sentence,
   but the opening line reads as a stronger claim. Wording only.

---

## Invariants I1–I5: verification summary

| Invariant | Verdict | Basis |
|---|---|---|
| I1 resolution | **Holds** | Pure delegation; slotmap version-match `get` (`basic.rs:656-662`); insert exception-safety verified (`basic.rs:394-431`). |
| I2 tombstone | **Holds within a slot's 2^31 reuse budget; "forever" is false** | F1 — `remove(h_old)` returned `Some(999)` after wrap. |
| I3 no ABA | **Holds within 2^31 reuses; "never" is false** | F1 — empirically aliased in 12 s. |
| I4 accounting | **Holds** | `num_elems` maintained atomically w.r.t. panic windows in insert/remove/drain (verified in slotmap source); F3 probe's `len() = 1` after partial clear was *correct* accounting. |
| I5 drop-once | **Holds by construction (ManuallyDrop::take + occupied-only drop), including under panicking `Drop` in `clear`** — but has **zero test coverage** (F6.1). | slotmap `basic.rs:436-448`, `1106-1137`; F3 probe (panicking value dropped once, survivor retained, not leaked or double-dropped). |

## What was checked and found clean (for the record)

- `Handle`'s hand-written `Clone/Copy/PartialEq/Eq/Hash/Debug` impls: correct,
  unconditional in `T` as claimed; `PhantomData<fn() -> T>` covariance claim correct.
- `SyncRegion` one-shot methods: the temporary write guard in `self.write().remove(h)`
  is released before the returned value can be dropped by the caller — no
  drop-under-lock hazard.
- `bench_ids_isolatable.rs`'s extractor and non-substring property: sound for the
  current 8 workload ids (`.bench_batched(` does not false-match the `.bench(` marker
  scan's ids since `"st/…"`/`"sync/…"`/`"raw/…"` never nest).
- `cargo metadata`/feature graph: `std = ["slotmap/std"]` is the only feature; no
  accidental default-on transitive std.
- No doctests in `src/**` (CLAUDE.md rule) — confirmed; all illustrative fences are
  prose or in README (which is not `include_str!`-ed, so its fences never compile).

## Recommended action order for the 0.1.1 patch

1. F1 doc rewrite (region.rs §Generation saturation + I3/I2 "never/forever" softening in
   region.rs, lib.rs, README) — this is the correction the release exists for.
2. F2 CI line (`-p sefer-region --no-default-features --target thumbv7em-none-eabi`).
3. F3 poisoning-policy caveat paragraph (sync_region.rs + README).
4. F6.1/F6.2 tests (drop-counter for I5, `clear` tests) — cheap, closes the two
   zero-coverage invariant/method gaps.
5. F4/F5/F7 doc touch-ups in the same pass.
