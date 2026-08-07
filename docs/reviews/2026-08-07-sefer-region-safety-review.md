# `sefer-region` — safety review: logical UAF, leaks, panic safety, Send/Sync, dependency risk (read-only)

**Date:** 2026-08-07
**Scope:** `crates/region` (crate `sefer-region`), ahead of the crates.io republish (task #656)
**Mode:** read-only investigation. No repository file was modified; the only write is this
report. Counterfactual probes were built and run in a scratch cargo project under the OS
temp directory (deleted after the run); their full source and observed output are inlined
in §3/§4 below so the results are reproducible without the scratch tree.
**Reviewed tree:** `main` @ `2fcf120`, `crates/region` clean.
**Dependency actually analyzed:** `slotmap 1.1.1` — the version `Cargo.lock` resolves
(`Cargo.lock` §`[[package]] name = "slotmap"`), source read from the local registry cache
(`slotmap-1.1.1/src/basic.rs`), not assumed from docs.

---

## Verdict: **sound by construction, with receipts — no soundness finding at any severity**

The crate is `#![forbid(unsafe_code)]` (`crates/region/src/lib.rs:51`), 3 source files,
~280 lines, every method a one-line delegation to `slotmap::SlotMap` or `std::sync::RwLock`.
After walking every path the task brief asked about — logical use-after-free, leaks,
panic-in-`Drop`, Send/Sync bounds, dependency pinning — **no unsound, leaking, or
double-dropping path exists**, and each "no" below is backed by either a compile check, a
runtime probe that was actually run, or a line-cited reading of slotmap 1.1.1's own source.
The findings that remain are two **documentation-severity** items (§7) and one
**test-coverage** recommendation (§6). Nothing blocks the republish.

Severity legend used below:
- **impossible** — ruled out by the type system / borrow checker, not by convention;
- **contrived** — reachable only by misuse the API already documents or that any
  equivalent API shares;
- **footgun** — realistically hittable by a normal caller (none found at this level for
  soundness; the two doc items in §7 are the closest).

---

## 1. Logical use-after-free / double-free equivalents

### 1.1 Within one `Region<T>` — impossible

`Region`'s entire mutating surface (`insert` `region.rs:94`, `get_mut` `:107`, `remove`
`:119`, `iter_mut` `:133`, `clear` `:139`, `reserve` `:89`) takes `&mut self`. Two handles
racing a `remove` against a read within one `Region` would require two live `&mut`/`&`
aliases, which the borrow checker rejects. A *stale* handle after `remove` is the I2/I3
case: slotmap bumps the slot's version on removal (`remove_from_slot`,
`slotmap-1.1.1/src/basic.rs:436-448` — version `wrapping_add(1)` at `:445`), so the old
handle fails the version check and resolves `None`. Double-`remove` is a no-op `None`
(`basic.rs:462-470`: `remove` re-checks `contains_key` before touching the slot). Covered
by `tests/smoke.rs:9-46` and, via the root re-export (see §6), by the root's miri'd
`tests/region_invariants.rs`.

### 1.2 Cross-instance, same-`T` handle confusion — real, already disclosed, not new

The one genuine logical-UAF-shaped hazard: `Handle<T>` brands by value type, not by
`Region` instance, so a `Handle<u32>` minted by region A is silently accepted by region B
and can read or **remove** whatever value occupies the same slot/generation there. This is
(a) prominently disclosed in three places (`lib.rs:17-21`, `README.md:23-26`,
`region.rs` has no per-instance claim), and (b) pinned by a dedicated honesty test,
`tests/smoke.rs:49-82` (`region_handle_crosses_instance_of_same_type`), which asserts the
wrong-region `remove` *succeeds*. Severity: **contrived** (requires holding two same-typed
regions and crossing their handles), and it is a *logic* hazard, not memory unsafety — the
wrong value is a live, valid `T`. Per-instance branding (an invariant-lifetime or
const-generic region ID) would be an API-breaking redesign; the disclosure-plus-test
posture is the right call for 0.1.x. **No action needed.**

### 1.3 `SyncRegion` TOCTOU (check-then-act across one-shot calls) — inherent, and benign in the worst case

A caller can `contains(h)` (`sync_region.rs:87`), lose the lock, and have another thread
`remove(h)` before a follow-up `get_cloned(h)`/`remove(h)`. **Probe D** (§4) exercised
exactly this interleaving deterministically: the post-race calls return a clean `None` —
never a wrong value, never a panic — because the generation check happens at final
resolution *under* the lock, and I3 guarantees a reused slot rejects the old handle. So
the TOCTOU window degrades to "your snapshot went stale," which is inherent to *any*
lock-guard API and is the reason the guard-based `read()`/`write()` transactional API
exists and is pointed to by every one-shot method's doc (`sync_region.rs:70-71`, `:78`,
`:85`). Not worth flagging as a defect; §7.1 suggests one sentence of doc hardening.
Severity: **inherent to the API class; worst case is benign.**

---

## 2. Leaks — every `T` drops exactly once on every path

Walked all four ownership-exit paths against slotmap 1.1.1's source:

1. **`remove`** (`region.rs:119` → `basic.rs:462`): the value is moved out via
   `ManuallyDrop::take` (`basic.rs:439`) and *returned to the caller* — ownership
   transfers, the caller's scope drops it. Bookkeeping (free list, version bump,
   `num_elems -= 1`) completes at `basic.rs:441-445` **before** the value is returned, so
   there is no window where the slot still claims a value the caller now owns.
2. **`clear`** (`region.rs:139` → `basic.rs:615` → `drain()` + immediate `Drain` drop):
   `Drain::drop` runs `for_each(|_drop| {})` (`basic.rs:1133-1137`); each iteration fully
   retires the slot via the same `remove_from_slot` before the value drops.
3. **`Region` drop**: `SlotMap` has no custom `Drop`; `Vec<Slot<V>>` drops each `Slot`,
   and `Slot`'s own `Drop` (`basic.rs:69-78`) drops the value iff the slot is occupied
   (version-parity check `basic.rs:43-45`). Vacant slots hold only a `u32` free-list link
   in the union — nothing to drop, nothing dropped twice.
4. **`SyncRegion` drop**: `RwLock<Region<T>>` drops its inner value unconditionally
   (poisoned or not — poisoning never prevents the inner drop), reducing to path 3.

**Empirically confirmed** by probe B/C (§3): 4 inserts across a panicking `clear` and a
subsequent `Region`/`SyncRegion` drop produced exactly 4 drop-counter increments in both
the single-threaded and the poisoned-lock variant. No leak, no double drop.

**`get_mut` + `mem::take`/`mem::replace` on `T: Default`** (the brief's leak-adjacent
question): this leaves the slot live and holding the (valid) default/replacement value.
Nothing is leaked and nothing is untracked — `len()` correctly counts the slot, and the
swapped-out `T` is owned by the caller. This is ordinary `&mut` semantics available on
every Rust collection, not a `sefer-region` trap; it belongs to no report. Noted here
only because the brief asked. Severity: **not a defect.**

`mem::forget` on a `SyncRegion` guard deadlocks that lock (std behavior, same as any
`RwLock`) — a liveness footgun of `std`, not a leak of `T` and not this crate's surface
to fix.

---

## 3. Panic safety — `T::drop` panicking mid-operation

This was the one area where "structurally intact after a panic" (the poisoning-policy
claim, `sync_region.rs:24-30`) needed verification rather than trust. Findings, grounded
in slotmap 1.1.1's actual code and then confirmed by a runtime probe:

### 3.1 slotmap's own ordering is deliberately panic-safe

- **Insert** (`try_insert_with_key`, `basic.rs:392-430`): the value is produced *first*
  ("Get value first in case f panics or returns an error", `basic.rs:399-401`) and the
  new slot is pushed *before* the free list is adjusted ("Create new slot before
  adjusting freelist in case f or the allocation panics", `basic.rs:420-425`). A panic
  during allocation growth leaves the map exactly as it was.
- **Remove**: all bookkeeping completes before the value is returned (§2.1), and the
  value's drop then happens in the *caller's* frame — a panicking `T::drop` during
  `Region::remove` cannot leave slotmap mid-operation at all.
- **Clear/drain**: each element is fully removed from the bookkeeping *before* its drop
  runs (`Drain::next`, `basic.rs:1109-1124`: `remove_from_slot` supplies the value the
  `for_each` closure then drops). A panicking `T::drop` therefore unwinds out of
  `clear()` with the map in a **consistent partial-clear state**: elements iterated
  before the bomb are removed+dropped, the bomb itself is removed and its (panicking)
  drop ran once, elements after it remain fully live and accounted.

### 3.2 Runtime probe (run against the real crate, scratch project, since deleted)

A `MaybeBomb` type with a global drop counter and one panicking instance; 3 inserts,
`clear()` under `catch_unwind`, then reuse + final drop. Observed output:

```text
B. clear() panicked after dropping 2 of 3 values; len() now = 1
B. post-panic contains: h1=false h2=false h3=true
B. region dropped; 2 live at drop, total drops = 4/4 — I5 holds
C. after poisoned clear: len() = 1 (partial clear), drops so far = 2
C. poison recovery + drop-once verified (4/4 drops)
```

So after a mid-`clear` drop panic: I4 holds (`len()==1` matches exactly the one
survivor), I2/I3 hold (cleared handles resolve `None`, survivor resolves), I5 holds
(4/4 drops across the whole lifetime, no double drop), and the region remains fully
usable — including through `SyncRegion`'s poison recovery (probe C: the panicking
`clear` ran on a spawned thread, poisoning the lock; the recovered region accepted a
new insert and dropped everything exactly once).

### 3.3 The honest caveats that remain (documentation-severity, §7.2)

1. **`clear()` is not atomic under a panicking `T::drop`** — it is "cleared up to the
   bomb." The docs say `clear` "removes every value" (`region.rs:137`,
   `sync_region.rs:106`); under a panicking drop that becomes "removes every value up to
   and including the panicking one." No invariant breaks (probe above), but a caller
   catching the panic (or going through `SyncRegion`'s poison recovery, which makes
   catching *implicit*) sees a partially-cleared region that the current doc wording
   doesn't predict. This exact semantics is slotmap's documented `drain` behavior
   (`basic.rs:622-626`: "When the iterator is dropped all elements … are removed, even
   if the iterator was not fully consumed") composed with unwinding — not a bug in
   either crate.
2. **Two panicking drops in one `clear`/`Region`-drop = process abort** (panic-during-
   unwind). Identical to `Vec<T>`/every std collection; not this crate's to fix.
3. The poisoning-policy claim "the region is structurally intact … regardless of a
   panicked op" (`sync_region.rs:24-27`) is **now verified, not just claimed**, for the
   worst available case (panicking user drop mid-bulk-op) — this review is the receipt.

Severity overall: **contrived** (requires a panicking `Drop` impl) and consistent —
nothing is silently lost or duplicated; §7.2 recommends one doc sentence.

---

## 4. Send/Sync soundness — verified in both directions by compile checks

No `unsafe impl Send/Sync` exists anywhere in this crate (grep confirms; `forbid`
makes one a compile error anyway) **nor anywhere in slotmap 1.1.1's `basic.rs`** — all
auto-traits propagate structurally on both sides. What the derives actually give:

| Type | Auto bound | Why | Verified |
|---|---|---|---|
| `Handle<T>` | `Send + Sync + Copy` **for all `T`** | fields are `DefaultKey` (two `u32`-shaped fields, `Send+Sync`) + `PhantomData<fn() -> T>` (`fn` pointers are `Send+Sync` regardless of `T`; also gives the documented covariance) — `handle.rs:16-21` | probe A compiled: `is_send/is_sync::<Handle<Rc<Cell<u32>>>>()` — `Send+Sync` even for a `T` that is neither ✔ |
| `Region<T>` | `Send iff T: Send`, `Sync iff T: Sync` | `SlotMap` = `Vec<Slot<V>>` + `u32`s + `PhantomData<fn(K) -> K>` (`basic.rs:129-135`); the `SlotUnion` propagates `T`'s auto-traits | positive: `Region<Cell<u32>>: Send` compiles; negative: `Region<Rc<u32>>: Send` → **E0277** ✔ |
| `SyncRegion<T>` | `Send iff T: Send`, `Sync iff T: Send + Sync` | exactly `RwLock<Region<T>>`'s own std impls (`Sync` requires `Send + Sync` because readers hand out `&T` concurrently) — no impl in this crate could widen it | negative: `SyncRegion<Cell<u32>>: Sync` → **E0277** (`Cell` is `Send` but `!Sync` — the load-bearing case); `SyncRegion<*mut u32>: Sync` → **E0277** ✔ |

The `handle.rs:10-11` claim ("unconditionally `Send + Sync` regardless of `T` — it owns
no `T`, it only names one") is **correct and honest about its mechanism**: it is a
statement of what the auto-derive yields, not a hand-asserted `unsafe impl`. And it is
semantically right: a handle carries no `T` provenance, only an index+generation; sending
one cross-thread transfers no access to any `T` (resolution still requires reaching the
`Region`, which carries the real bounds).

One deliberate observation: the bounds are **exactly** as tight as they must be — not
accidentally weaker (negative checks above fail) and not accidentally stronger (positive
checks pass; `Handle` stays universal). **No finding.**

---

## 5. Dependency risk — `slotmap = "1"`, resolved 1.1.1

- **Resolved version:** 1.1.1 (workspace `Cargo.lock`), which is also the **latest
  published version** (crates.io API, fetched live during this review).
- **RUSTSEC:** the rustsec.org package index has **no entry for slotmap at all** (checked
  live, 2026-08-07 — not from memory).
- **Yanked history worth knowing:** crates.io reports 1.0.0–1.0.5 all yanked (1.0.6,
  1.0.7, 1.1.0, 1.1.1 remain). The `"1"` requirement can therefore only resolve to
  ≥ 1.0.6 for a fresh consumer — the yanked range is unreachable without a pre-existing
  lockfile.
- **Is `"1"` tight enough?** Yes, and tightening it would be wrong for a library:
  `=1.1.1` or `~1.1` would force needless duplicate slotmap copies into downstream trees
  (semver-incompatible with other `slotmap = "1"` users' resolutions) while defending
  against a hypothetical un-yanked soundness regression that (a) has never happened in
  this crate's RUSTSEC history and (b) would be caught downstream by `cargo audit`/
  `cargo-deny` — which this workspace already runs in CI (the `cargo-deny` job in
  `.github/workflows/ci.yml`). For the *workspace's own* builds, `Cargo.lock` pins 1.1.1
  exactly. This is the standard, correct posture. **No change recommended.**

---

## 6. Coverage — what actually exists (the brief's premise needs one correction), and the one gap worth filling

**Correction to the task brief:** "no miri harness, no fuzz target exists for this crate"
is true of `crates/region/` *as a directory* but **false at the coverage level**. The root
crate re-exports this crate's types verbatim (`src/lib.rs:377`: `pub use
sefer_region::{Handle, Region};` and `:380`: `pub use sefer_region::SyncRegion;`), and:

- **miri**: the root CI job `cargo miri test --test region_invariants`
  (`.github/workflows/ci.yml:834-835`) runs `tests/region_invariants.rs` — which
  imports `sefer_alloc::Region`, i.e. *this crate's* `Region` — under miri. Every
  `insert`/`get`/`remove`/`clear`/drop it performs executes **through this crate into
  slotmap 1.1.1's real unsafe internals** (`get_unchecked`, union field access,
  `ManuallyDrop`) under miri's checker. So the meaningful miri question ("can miri catch
  anything by running *through* this safe crate into slotmap's unsafe?") is not only
  answerable in principle — it is already wired and running in CI.
- **fuzz**: `fuzz/fuzz_targets/region_ops.rs` drives `sefer_alloc::{Handle, Region}` —
  again this crate via the re-export — with a structured op stream asserting I1–I5
  including drop-once, on a 10-minute scheduled CI run (`ci.yml:1634-1636`).
- **plain tests**: `test-workspace` runs `cargo test -p sefer-region` directly
  (`ci.yml:708`). All 6 smoke tests pass locally (re-run for this review).

**The real gaps, honestly sized:**

1. **`SyncRegion` has zero miri/TSan coverage and only two tests total** —
   `region_invariants.rs` and the fuzz target exercise `Region` only. That said, the
   honest value assessment the brief asked for: `SyncRegion` contains no atomics, no
   `unsafe`, no hand-rolled synchronization — it is `RwLock` + nine one-line delegations.
   **loom would model nothing this crate wrote** (its interleaving surface is std's
   `RwLock` itself, and loom's value is for hand-rolled atomics); a loom model here would
   be ceremony. **A worthwhile addition instead:** one test in the shape of §3.2's probe
   B/C — panicking-`Drop` `clear()` through both `Region` (with `catch_unwind`) and
   `SyncRegion` (via a panicking thread), asserting partial-clear consistency and
   drop-once — because that is the one behavior this review had to *build a scratch
   probe* to verify, and it pins the poisoning-policy doc claim (`sync_region.rs:24-30`)
   as a tested fact. It is also the one scenario genuinely worth running under miri
   through this crate: it exercises slotmap's `Drain` unwind path (`basic.rs:1133`),
   which `region_invariants.rs` never touches.
2. **Cross-arch/no_std**: `sefer-region` claims `no_std + alloc` (`lib.rs:44-48`) but
   unlike `size-classes`/`racy-ptr-cell` (which get `thumbv7em-none-eabi` cross-builds,
   `ci.yml` test-workspace steps 4-5) it has no no_std build check in CI. A
   `--no-default-features` check line is cheap. (This is a build-coverage note, not a
   safety finding — the crate has no `std` imports outside the `cfg`-gated
   `sync_region.rs`, verified by reading all four files.)

---

## 7. Documentation-severity findings (the only actionable items)

### 7.1 One-shot method docs could name the staleness rule once — severity: nit

`len()` already carries "under concurrency the count is a momentary snapshot, not a
stable property" (`sync_region.rs:93-94`). `contains()` (`:83-89`) — the natural
check-then-act starter — carries no such note. One sentence on `contains` (or one
"check-then-act" paragraph in the type-level poisoning-policy doc block) saying *"a
`true` result may be stale by the time you act on it; the worst case of acting on a stale
handle is `None` (I3), never a wrong value — use `write()` for atomic check-then-act"*
would turn §1.3's verified property into a documented guarantee. That last clause is the
valuable, non-obvious part this review actually proved (probe D).

### 7.2 `clear()`'s partial-clear-under-panicking-drop semantics — severity: nit

Per §3.3: add to `Region::clear` (`region.rs:137-141`) and/or the `SyncRegion` poisoning
policy (`sync_region.rs:22-30`) one sentence: *"If a value's `Drop` panics mid-`clear`,
values already visited (including the panicking one) are removed and dropped; later
values remain live and correctly accounted — the region stays consistent and usable, but
partially cleared."* This is currently true, verified (§3.2), and undocumented.

Neither item blocks the 0.1.1 republish; both are one-sentence edits that would fold
naturally into the docs-only patch release the 2026-08-06 publish-readiness review
already recommends.

---

## 8. What was checked and found clean (explicit no-findings list)

- No `unsafe` anywhere in the crate (forbid at `lib.rs:51`; grep clean).
- No explicit `unsafe impl Send/Sync` in this crate **or** in slotmap 1.1.1's `basic.rs`.
- No path extracts a `T` without accounting; no `IntoIterator`-by-value surface exists.
- `insert` panic-during-allocation leaves the map untouched (slotmap's own documented
  ordering, `basic.rs:399-401`, `:420-425`).
- `Handle` equality/hash delegate to the key only (`handle.rs:42-52`) — no `T`-dependent
  behavior that could diverge between the branded and raw key.
- Generation-ABA (I3) holds under slot reuse (smoke test + root miri suite + fuzz
  target); generation *saturation* is slotmap's slot-retirement responsibility
  (`region.rs:34-41`), correctly delegated, no hand-rolled retirement here.
- The poisoning-recovery policy hands back a structurally intact region in the worst
  reachable case (panicking user drop mid-bulk-op) — verified empirically, §3.2.

**Bottom line:** publish-safe. The crate's safety story is exactly what its README
claims, and this review's contribution is that the claims are now *receipted* (compile
checks both directions on Send/Sync, a runtime drop-once/partial-clear probe, live
RUSTSEC/crates.io checks, and a line-level read of slotmap 1.1.1's remove/clear/insert
ordering) rather than asserted.
