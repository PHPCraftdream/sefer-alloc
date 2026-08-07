# `sefer-region` — consolidated work plan (arbitration of three independent reviews)

**Date:** 2026-08-07
**Reviewed tree:** `main` @ `2fcf1201819834943f584528ff6c8231d0d629c8`, `crates/region` clean.
**Inputs:** `docs/reviews/2026-08-07-sefer-region-performance-review.md`,
`docs/reviews/2026-08-07-sefer-region-logic-review.md`,
`docs/reviews/2026-08-07-sefer-region-safety-review.md`.
**Mode:** read-only arbitration. Four throwaway probes were built and run in a scratch cargo
project **outside** the repo (`%TEMP%\sr-probe-x`, path-dependency on `crates/region`,
deleted after use; `git status --porcelain` confirms no stray file). Verbatim outputs are
reproduced in the task descriptions (#664–#673) so every measured claim is reproducible
without a committed artifact. `slotmap 1.1.1` was read at source level from the registry
cache (`D:\system_artefact\cargo\registry\src\index.crates.io-1949cf8c6b5b557f\slotmap-1.1.1\`).

The plan itself lives in the **TaskList (#664–#673)** — each task's description is the
long-form, self-contained plan for that item (evidence, `file:line` cites, fix approach,
verification, and the user-decision gate where one exists). This file is the map.

---

## 1. Arbitration: the `clear()`-panic "contradiction" (logic §F3 vs safety §3.2/§7.2)

**Verdict: both reports are factually right; there is no contradiction to resolve.** They
measured the same behavior and judged it against different bars — the logic review against
`clear`'s *published API contract*, the safety review against *memory/invariant soundness*.

My own probe (5 values, `Drop` panics on the 3rd, release; verbatim):

```text
A. Region::clear with bomb at 3rd of 5
   clear() panicked = true
   len() after = 2
   handles still resolving = [3, 4]
   drops so far = 3
   reuse ok, new handle resolves = true
   total drops after region drop = 6 (expected 6 = 5 inserted + 1 reused)
B. SyncRegion::clear on a spawned thread, same shape
   clear thread panicked = true
   len() after (poison-recovered read) = 2
   handles still contained = [false, false, false, true, true]
   reuse after poison ok = true;  2nd clear() -> len() = 0;  total drops = 6
```

- The **logic review is right** that `clear()` silently becomes a *partial* clear — its doc
  says "removes **every** value, invalidating **all** outstanding handles"
  (`crates/region/src/region.rs:137-138`, `src/sync_region.rs:106`) and two values survived
  with live handles — and that through `SyncRegion` **no caller anywhere gets a signal**:
  every accessor recovers from poison (`sync_region.rs:57`, `:65`) and the type exposes no
  `is_poisoned`/`try_*` surface at all.
- The **safety review is right** that nothing is unsound: I2/I3 hold for cleared handles,
  I4's `len()` matches the survivors exactly, I5 holds (6/6 drops, none double, none leaked),
  and the region stays fully reusable — including through poison recovery.
- Mechanism, from slotmap 1.1.1's source: `clear()` is `self.drain()` (`basic.rs:615-617`);
  `Drain::drop` is `for_each(|_drop| {})` (`:1133-1137`); `Drain::next` (`:1106-1125`)
  completes `remove_from_slot` **before** handing the value to the closure that drops it — so
  a `T::Drop` panic always lands *between* element removals, never inside slot bookkeeping,
  and unwinds out of `Drain::drop` leaving `cur` where it stopped.
- The safety review's "4/4 drops" and the logic review's "drops that ran = 2" are not in
  conflict either: the first counts the **whole lifetime** (including the final region drop
  and a post-recovery reuse), the second counts **at the moment of the panic**. Both are
  consistent with my 3-at-panic / 6-over-lifetime numbers at a different value count.
- Note the safety review's own §3.3.1 and §7.2 already flag the partial-clear semantics as an
  undocumented caveat — it never claimed the opposite of the logic review's observation, only
  a different severity for it.

→ **Task #666** carries the resolution: document the partial clear at three sites, and pin it
with a test (which is also the one `sefer-region` scenario genuinely worth running under
miri — it exercises slotmap's `Drain` unwind path that `tests/region_invariants.rs` never
touches).

## 2. Arbitration: the ABA claim (logic §F1)

**CONFIRMED — re-reproduced at full scale, not scaled down.** 2^31−1 churn cycles, release,
12.42 s on this host (the logic review reported 12.12 s):

```text
h_old  = Handle { key: DefaultKey(1v1) }
churn of 2147483647 cycles took 12.421058s
h_new  = Handle { key: DefaultKey(1v1) }
bit-identical? true
get(h_old) after wrap = Some(999)
remove(stale h_old) = Some(999)
get(h_new) = None
```

The stale handle became bit-identical to a fresh one, resolved to a value it never named
(**I3 violated**), and `remove(stale)` **stole the live value** out from under the legitimate
handle, which then resolves `None` (**I2's "None forever" violated**). Memory safety is
unaffected. The mechanism is in slotmap's source, not inferred:
`remove_from_slot` pushes the freed slot onto the freelist unconditionally and bumps
`slot.version = slot.version.wrapping_add(1)` (`slotmap-1.1.1/src/basic.rs:436-446`) — there
is **no retirement code anywhere in `SlotMap`**, and slotmap's own crate doc admits the wrap.
So `crates/region/src/region.rs:34-41` is wrong twice over: the outcome at saturation is
**alias, not retirement**, and the budget is **2^31** occupy/free cycles, not 2^32.

→ **Task #664**, the highest-priority item.

---

## 3. Priority order and task map

### (a) Confirmed real — worth a task now

| # | Task | Severity | Why first |
|---|---|---|---|
| 1 | **#664** — rewrite the false "Generation saturation" doc; soften I2/I3 "never/forever" | **High** | A published *safety* claim that is false, and the real behavior violates two headline invariants in 12 s of churn. Three prior doc passes missed it; the 2026-08-06 publish-readiness review cited it as a *positive*. |
| 2 | **#665** — fix the bench-harness fidelity defect before the README perf table ships | **High** | `bench_batched` drops the fixture *inside* the timed window (`bench-scale-tool-0.1.0/src/lib.rs:276-291`), so `insert`/`remove` rows publish a cold-lifecycle number. Steady-state churn measured at **6.23 ns/cycle** vs the README's 290 ns / 97 ns. Becomes the crate's public face the moment 0.1.1 ships. |
| 3 | **#666** — document `clear()`'s partial clear under a panicking `T::Drop` + test it | Medium | §1 above. Silent contract failure, invisible through poison recovery. |
| 4 | **#667** — add the missing `no_std` CI build | Medium | `--no-default-features` is claimed in README, `lib.rs`, keywords **and** categories, and built nowhere (`ci.yml:708`, `:753-754`, `:799`). Zero signal on an advertised config. |
| 5 | **#668** — close the two zero-coverage gaps: I5 (drop-once) and `clear()` | Medium | Two of five headline invariants rest on reading slotmap's source. As a *standalone* crate (what a crates.io consumer runs) they are untested. |
| 6 | **#669** — correct the panic contracts on `reserve`/`with_capacity`/`insert` | Low | Verified wrong: `with_capacity(usize::MAX)` → capacity 3 silently; `reserve(usize::MAX)` → silent no-op in release; `insert`'s "SlotMap is full" panic undocumented. |
| 7 | **#670** — de-vacuum the I3 test; static-assert `Handle`'s layout and auto-traits | Low | `smoke.rs:31-46` never asserts the slot was actually reused — it would pass while testing nothing. Same path-activation-oracle discipline CLAUDE.md mandates for benches. |
| 8 | **#671** — measure iteration over tombstones; document that capacity never shrinks | Medium-low | The crate's one unbounded-cost surface, currently unmeasured and undocumented. slotmap has no `shrink_to_fit` at all. |
| 9 | **#672** — docs.rs-facing polish (dangling `BENCHMARKS.md` link, "Phase 3b" jargon, `get_cloned` wording, `contains` staleness note) | Low | The crate's docs.rs front page, being finalized for publish. |

### (b) Plausible but not independently verified — filed as such

| # | Task | Note |
|---|---|---|
| 10 | **#673** — one contended `SyncRegion` measurement as a future decision gate | **No defect claimed or found.** All `sync/*` workloads are uncontended; they measure lock *overhead*, not lock *behavior*. Filed so the next "should we shard?" question finds a named experiment. Recommendation: **defer** — it blocks nothing. |

### (c) Deliberately NOT filed

- **`Ord`/`PartialOrd` on `Handle<T>`; widening `iter()`/`iter_mut()` to
  `ExactSizeIterator + FusedIterator`** (perf §4, §5) — additive API polish, no consumer
  blocked, ~zero runtime effect (`size_hint` already flows through the opaque type, so
  `collect` already pre-sizes). Can land opportunistically at any time; not worth a task.
- **Cross-instance handle confusion** (safety §1.2) — real but already disclosed in three
  places and pinned by a dedicated honesty test (`smoke.rs:49-82`). Per-instance branding
  would be an API-breaking redesign. The current posture is correct.
- **`slotmap = "1"` version pinning** (safety §5) — resolved 1.1.1 (latest), no RUSTSEC
  entry, yanked 1.0.0–1.0.5 unreachable for a fresh consumer. Tightening to `=1.1.1`/`~1.1`
  would force duplicate slotmap copies downstream. Correct as is.
- **loom for `SyncRegion`** (safety §6.1) — it contains no atomics, no `unsafe`, no
  hand-rolled synchronization. loom would model std's `RwLock`, not anything this crate
  wrote. Ceremony, not coverage.
- **"Two panicking drops in one `clear` = abort"** (safety §3.3.2) — std-universal
  (identical for `Vec<T>`), not this crate's to fix.
- **`get_mut` + `mem::take` "leak"** (safety §2) — ordinary `&mut` semantics; nothing leaked,
  `len()` stays correct. Not a defect.
- **Perf §6's no-action confirmations** — capacity growth (~2.3% overshoot at n=1000),
  `#[inline]` (negative A/B), compile-time/binary-size, `st/get_hit`'s hot-handle shape,
  a large-`T` workload. All explicitly checked and closed.
- **The reported "contradiction" itself** — see §1: nothing to file beyond #666.

---

## 4. Suggested landing order

#664 and #665 before the 0.1.1 republish (both are public-facing claims that become the
crate's face on crates.io). #667 is a one-line CI change that can land immediately and
independently. #666 + #668 pair naturally (same test file, same `Drop`-counter fixture) —
land #666's doc first so #668's tests pin an accurate contract. #669 + #672 fold into the
same docs pass as #664. #670, #671 and #673 are not publish-blocking. Note the ordering
constraint recorded in #672: its `contains()` staleness sentence must be qualified
consistently with #664's wording, or it recreates the same class of false absolute claim in
a new place.
