# `sefer-region` — follow-up logic & correctness review (read-only)

**Date:** 2026-08-07
**Scope:** `crates/region` (crate `sefer-region` 0.1.0) — a second logic/correctness pass
*after* the 14-commit remediation round, deliberately building past everything the
2026-08-07 logic review (F1–F7), the round-closing review (findings A–H), and their fix
commits (`39704e1`/`9fcbbf1`/`aa24f84`) already settled. Focus areas per the brief:
`SyncRegion` TOCTOU/check-then-act hazards, untested I1–I5 corner shapes,
`get_cloned`'s contract under a panicking `T::clone`, `Region`↔`SyncRegion` contract
symmetry, and residual doc-vs-code drift.
**Mode:** read-only investigation. No repository file was modified except this report.
One throwaway empirical probe was built and run in a temp directory **outside** the repo
(`%TEMP%\sefer-region-logic-probe2`, path-dependency on `crates/region`, deleted after
use); its exact source is reproduced verbatim below so every measured claim is
reproducible without any committed artifact.
**Reviewed tree:** `main` @ `aa24f844dc0b8df46ca928f643055d2f28fc03aa`;
`cargo test -p sefer-region` independently re-run at this SHA — **28 passed, 0 failed,
1 ignored** (the captrack probe, correctly `#[ignore]`d), matching the round-closing
review's own gate re-run.

---

## Verdict

Two genuinely new findings, both empirically confirmed, both fixable at the docs level:
**N1** — `SyncRegion` runs arbitrary user code (`T::clone` inside `get_cloned`,
`T::Drop` inside `clear`) *while holding the lock*, so a `T` that re-enters the same
`SyncRegion` self-deadlocks, reproducibly, with zero documentation of the hazard; and
**N2** — the poisoning-policy doc's opening sentence ("A panic while a guard is held
poisons the `RwLock`") is factually false for **read** guards, which std never poisons.
Plus one low-severity cluster of `Region`↔`SyncRegion` doc asymmetries (N3) and three
minor nits. **No wrong-result bug, no invariant violation, and no memory-safety issue
was found anywhere in the ordinary operating envelope** — the check-then-act surface,
the ZST/nesting corner shapes, and the post-round doc text all came back clean (details
in the "checked and found clean" section).

---

## N1 — CONFIRMED (empirical): `SyncRegion` executes user code under the lock in exactly two places (`get_cloned`'s `T::clone`, `clear`'s `T::Drop`); a reentrant `T` self-deadlocks, and the hazard is undocumented

**Severity: medium-low as a field risk (requires a `T` that holds an
`Arc<SyncRegion<T>>` back to its own region — a real but uncommon shape); medium as a
doc gap, because the struct-level doc's framing actively points the other way.**

### The claim being tested

`sync_region.rs:11-12` frames `SyncRegion` as "the *always-shippable* concurrent
answer: **correct under any interleaving** because every mutation serialises through
the lock." That is true for data correctness (nothing below contradicts it), but the
serialisation itself creates a liveness hazard the docs never mention: two public
one-shot methods run **caller-supplied code** while the internal guard is held:

- `get_cloned` (`sync_region.rs:139-144`): `self.read().get(handle).cloned()` — the
  read guard is a temporary alive until the end of the full expression, so `T::clone`
  executes with the read lock held.
- `clear` (`sync_region.rs:130-132`): `self.write().clear()` — every removed value's
  `T::Drop` executes with the write lock held (slotmap's `Drain` drops values one by
  one inside `Region::clear`, which is entirely inside the guard's lifetime).

If that user code touches the *same* `SyncRegion` (any one-shot method — they all lock
internally), the thread blocks on a lock it itself holds. Per std's own documentation,
same-thread reacquisition of an `RwLock` "might panic … or deadlock"; on this Windows
host it deadlocks silently in both directions.

### Empirical reproduction (probe run at `aa24f84`, release)

Probe source (built outside the repo, deleted after; verbatim):

```text
use sefer_region::SyncRegion;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

// ── Stage 2: get_cloned() whose T::clone re-enters the same SyncRegion (write op) ──
static DONE2: AtomicBool = AtomicBool::new(false);

struct Reentrant {
    home: Option<Arc<SyncRegion<Reentrant>>>,
}
impl Clone for Reentrant {
    fn clone(&self) -> Self {
        if let Some(h) = &self.home {
            // insert() takes the WRITE lock while get_cloned() holds the READ lock
            // on the same RwLock, on the same thread.
            h.insert(Reentrant { home: None });
        }
        Reentrant { home: None }
    }
}

fn stage2() {
    let sr: Arc<SyncRegion<Reentrant>> = Arc::new(SyncRegion::new());
    let h = sr.insert(Reentrant { home: Some(Arc::clone(&sr)) });
    let sr2 = Arc::clone(&sr);
    std::thread::spawn(move || {
        let _ = sr2.get_cloned(h);
        DONE2.store(true, Ordering::SeqCst);
    });
    std::thread::sleep(Duration::from_secs(2));
    println!(
        "stage2: get_cloned(T::clone re-enters insert) completed within 2s? {}",
        DONE2.load(Ordering::SeqCst)
    );
}

// ── Stage 3: clear() whose T::drop re-enters the same SyncRegion (read op) ──
static DONE3: AtomicBool = AtomicBool::new(false);

struct DropReentrant {
    home: Option<Arc<SyncRegion<DropReentrant>>>,
}
impl Drop for DropReentrant {
    fn drop(&mut self) {
        if let Some(h) = self.home.take() {
            // len() takes the READ lock while clear() holds the WRITE lock
            // on the same RwLock, on the same thread.
            let _ = h.len();
        }
    }
}

fn stage3() {
    let sr: Arc<SyncRegion<DropReentrant>> = Arc::new(SyncRegion::new());
    sr.insert(DropReentrant { home: Some(Arc::clone(&sr)) });
    let sr2 = Arc::clone(&sr);
    std::thread::spawn(move || {
        sr2.clear();
        DONE3.store(true, Ordering::SeqCst);
    });
    std::thread::sleep(Duration::from_secs(2));
    println!(
        "stage3: clear(T::drop re-enters len) completed within 2s? {}",
        DONE3.load(Ordering::SeqCst)
    );
}

// ── Stage 4 (control): remove() does NOT drop T under the lock ──
static DONE4: AtomicBool = AtomicBool::new(false);

fn stage4() {
    let sr: Arc<SyncRegion<DropReentrant>> = Arc::new(SyncRegion::new());
    let h = sr.insert(DropReentrant { home: Some(Arc::clone(&sr)) });
    let sr2 = Arc::clone(&sr);
    std::thread::spawn(move || {
        let v = sr2.remove(h); // guard released before v is dropped by the caller
        drop(v);               // reentrant len() runs OUTSIDE the lock -> fine
        DONE4.store(true, Ordering::SeqCst);
    });
    std::thread::sleep(Duration::from_secs(2));
    println!(
        "stage4 (control): remove + caller-side drop (reentrant len in Drop) \
         completed within 2s? {}",
        DONE4.load(Ordering::SeqCst)
    );
}
```

Output (verbatim):

```text
stage2: get_cloned(T::clone re-enters insert) completed within 2s? false
stage3: clear(T::drop re-enters len) completed within 2s? false
stage4 (control): remove + caller-side drop (reentrant len in Drop) completed within 2s? true
```

Both reentrant shapes hang forever (the probe abandons the deadlocked threads and
exits); the control confirms the previously-reviewed claim that `remove`'s temporary
guard is released **before** the caller can drop the returned value — so `remove` is
immune, and the hazard is confined to exactly the two methods named above (plus,
trivially, any user code a caller runs inside their own held `read()`/`write()` guard,
which is the caller's own transaction and their own responsibility).

Note the two failing shapes are **single-threaded** failures — no interleaving with any
other thread is required — which is why the "correct under any interleaving" framing,
while not literally false (a deadlock is not incorrect data), leaves the reader with no
warning about the one way this type can lock up all by itself.

### Two secondary consequences worth stating in the same doc paragraph

1. **`clear` holds the write lock across *all* `T::Drop` calls** — even a
   non-reentrant but slow `Drop` (e.g. one that flushes a file per value) blocks every
   reader and writer of the region for the entire linear sweep. This is inherent to the
   design and fine, but it is a latency property a user of a "concurrent" container
   would want stated.
2. **`get_cloned` holds the read lock across `T::clone`** — same reasoning, milder
   (readers are unaffected, writers block).

### Recommended fix (docs-only, one paragraph)

Add a short "Reentrancy" note to the `SyncRegion` struct-level doc (adjacent to the
poisoning policy, `sync_region.rs:21-38`): `get_cloned` runs `T::clone`, and `clear`
runs each `T::Drop`, while the internal lock is held; a `T` whose `Clone`/`Drop`
re-enters the same `SyncRegion` (directly or transitively) will deadlock or panic per
`std::sync::RwLock`'s same-thread reacquisition behavior; long-running `Clone`/`Drop`
code delays all other users of the region for the duration. Mention that `remove` is
deliberately shaped so the value drops *outside* the lock (that guarantee already
exists in the code and was verified — it deserves to be a stated contract, since it is
exactly what makes `remove` safe against this class). No API change is needed for
0.1.x; a `get_cloned`-style method that clones outside the lock is impossible
(the clone *is* the read), and `clear`'s alternative (drain to a Vec under the lock,
drop outside) is a real option but a behavior change beyond this review's scope.

---

## N2 — CONFIRMED (empirical): "A panic while a guard is held poisons the `RwLock`" is false for read guards — std poisons only on panics under the *write* lock

**Severity: low — doc-only; observable behavior through this crate's API is unchanged
(everything recovers from poison anyway), but the stated mechanism is wrong, and it is
the premise the whole poisoning-policy paragraph rests on.**

### The claim

`sync_region.rs:23`: "A panic while a guard is held poisons the `RwLock`." — stated
unconditionally, for both guard kinds (the paragraph goes on to describe recovery in
`read` and `write` alike, `sync_region.rs:63,71`).

### What std actually does

`std::sync::RwLock`'s own documentation: "An `RwLock` … will become poisoned on a
panic. Note, however, that an `RwLock` may only be poisoned if a panic occurs while it
is locked exclusively (write mode)." Empirically confirmed (probe stage 1, verbatim
output):

```text
stage1: lock.is_poisoned() after read-guard panic = false
stage1: lock.read().is_err() = false
stage1: lock.is_poisoned() after WRITE-guard panic = true
```

### Why this matters here (and answers the brief's `get_cloned` question)

The one place *this crate itself* panics while holding a **read** guard is
`get_cloned` with a panicking `T::clone` (`sync_region.rs:143`). The actual guarantee,
now verified: the panic unwinds through the read guard, the guard is released, **no
poison is ever set**, the stored value remains live and untouched, and the region is
fully usable — a strictly *cleaner* outcome than the doc implies (the doc's mechanism
would have this panic poison the lock and be silently recovered; in reality there is
nothing to recover from). So `get_cloned`'s contract is sound; only the policy
paragraph's first sentence misdescribes the machinery. The recovery code itself is
still correct and still needed — for the write-side panics (`insert` on a full map,
`T::Drop` in `clear`, user panics inside `write()`), which the existing tests
(`smoke.rs:179-204`, `clear_partial_under_panic.rs:140`) already cover.

### Recommended fix (one clause)

`sync_region.rs:23` → "A panic while the **write** guard is held poisons the
`RwLock` (std never poisons on a read-guard panic — e.g. a panicking `T::clone` inside
[`get_cloned`](Self::get_cloned) releases the read lock cleanly with no poison and no
effect on the stored value)." Optionally mirror one sentence onto `get_cloned`'s own
doc. `README.md:81-84`'s poison wording ("a panicked op leaves the slotmap structurally
intact") is loose enough to survive unchanged.

---

## N3 — `Region`↔`SyncRegion` contract asymmetries (all doc-level, all low severity)

The brief's symmetry check (point 4) found no behavioral divergence — every
`SyncRegion` method delegates to the identical `Region` code path — but three places
where the *documented* contract did not travel with the delegation:

1. **`SyncRegion::with_capacity` (`sync_region.rs:52-58`) lacks the extreme-argument
   caveat `Region::with_capacity` carries (`region.rs:78-82`).** The release-build
   arithmetic-wrap behavior (a `capacity` near `usize::MAX` silently yielding a tiny
   reservation) applies identically through the wrapper, and the `Region` doc's own
   remedy — "use `capacity()` to verify" — is not even reachable one-shot on
   `SyncRegion`, which exposes no `capacity()` accessor (a caller must know to write
   `sr.read().capacity()`, a route documented nowhere on the type). Concrete scenario:
   a user who reads only `SyncRegion`'s docs.rs page (the plausible entry point for the
   concurrent type) sees an unqualified "space pre-reserved for `capacity` entries" —
   the exact overclaim `ecc5138` (task #669) fixed on `Region`.
2. **`SyncRegion::is_empty` (`sync_region.rs:117-121`) lacks the momentary-snapshot
   note its sibling `len` carries (`sync_region.rs:110-111`).** Same TOCTOU character,
   same fix: one sentence (or fold both under one shared note).
3. **`SyncRegion` has no one-shot `reserve`** — `sr.write().reserve(n)` is the intended
   route (the crate's own test uses it, `coverage_gaps.rs:494`), but nothing on the
   type says so. A single line in the struct doc's guard-vs-one-shot paragraph
   (`sync_region.rs:16-19`) listing `reserve`/`capacity` as guard-only operations
   closes this.

---

## Minor nits (no action required, recorded for completeness)

- **`region.rs:15` — "Individual lookup, insertion, and removal are `O(1)`":**
  insertion is *amortised* O(1) (it may reallocate the slot array on growth, exactly
  like the `reserve` sentence two lines later admits). One word.
- **`sync_region.rs:101-102` — `contains`'s check-then-act guidance names only
  `write()`:** for check-then-**read** compositions (`contains` then a read of the
  value) a `read()` guard is sufficient and cheaper; `write()` is needed only when the
  "act" mutates. The current text is not wrong (its "check-then-act" plausibly means
  "act = modify"), just narrower than the escape hatch it advertises.
- **`smoke.rs:38-51`'s `slot_index` helper** depends on slotmap's `{idx}v{version}`
  `Debug` format — a fragility the test itself already documents and accepts
  explicitly; noted here only so a future slotmap bump that breaks this test is
  recognized as the test's documented tradeoff firing, not a regression in the crate.

---

## Checked and found clean (for the record)

- **TOCTOU sweep of the one-shot surface (brief point 1):** every one-shot method is a
  single atomic operation under its internal lock; the only compositional hazards are
  (a) the staleness `contains`/`len` already document and (b) N1's reentrancy. The
  `write()` escape hatch is sufficient for atomic check-then-modify and is correctly
  advertised on `contains`; no missing primitive rises to a defect (an atomic
  `remove_if`/`get_or_insert` would be conveniences, not correctness needs). Acting on
  a stale handle degrades to `None` exactly as documented, within the 2^31 budget.
- **ZST and nesting shapes (brief point 2):** `Region<()>` and `Region<Region<u8>>`
  are sound by construction — the crate imposes no bounds on `T`, contains no
  size-dependent or type-dependent branches of its own (pure delegation), and slotmap
  stores `T` in a union alongside its `u32` freelist link, which handles ZSTs by
  design. `T`'s own `PartialEq`/`Hash` (brief point 2's "custom impls" angle) are
  **never consulted anywhere in the crate** — `Handle`'s hand-written impls delegate to
  `DefaultKey` only (`handle.rs:42-52`) — so no interaction is possible. No test was
  added (read-only mode) and none is urgent; a two-line ZST smoke test would be cheap
  insurance if a future round wants it.
- **I1–I5 corner-case re-read of all 27 non-ignored tests:** the assertions (not just
  the names) were read critically; each test discriminates what it claims (the
  re-fixed I3 test now genuinely proves slot reuse via the index oracle;
  `clear_partial_under_panic` pins both the drop count *and* survivor identity; the
  drop-once suite's counterfactuals are documented and plausible). No vacuous assertion
  found.
- **`remove` drop-outside-lock:** the round-1 review verified this by reading; probe
  stage 4 now confirms it empirically (a reentrant `Drop` on the removed value runs
  clean). Worth promoting from implementation detail to documented contract per N1.
- **Doc-vs-code drift sweep (brief point 5):** the F7 items are all genuinely fixed
  (full GitHub URLs at `region.rs:10`/`lib.rs:6`; no parent-workspace phase jargon
  left in `sync_region.rs`; `get_cloned`'s "without leaving the caller holding a
  guard" wording). A grep for residual absolutes (`never`/`forever`/`always`/
  `guarantee`) across `crates/region/src/` found only the genuinely-absolute claims
  (I5 drop-once, keys-never-escape, memory-safety-never-affected) — the 2^31
  qualification is present at every I2/I3 site. `README.md`'s ` ```rust ` fences do
  not execute (the README is not `include_str!`-ed into `lib.rs` — re-verified), so
  they are consistent with the no-doctest rule.
- **Poison-recovery behavior itself** (as opposed to its N2 description): correct and
  tested on the write side; nothing to add beyond N2's wording fix.

---

## Recommended action order (all doc-level; fits any future 0.1.x patch)

1. N1 reentrancy paragraph on `SyncRegion` (+ promote `remove`'s drop-outside-lock to
   a stated contract) — the only finding with a runtime failure mode.
2. N2 one-clause poison correction (`sync_region.rs:23`), optionally echoed on
   `get_cloned`.
3. N3 symmetry pass: `with_capacity` caveat, `is_empty` snapshot note, one line on the
   guard-only route for `reserve`/`capacity`.
4. Nits at will.
