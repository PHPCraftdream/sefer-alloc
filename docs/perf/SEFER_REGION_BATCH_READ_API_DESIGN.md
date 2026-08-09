# sefer-region: batch-read convenience API — design note

Status: **design note only — not implemented.** Closes task #798 (perf
design note flagged in
`docs/reviews/2026-08-09-sefer-region-static-release-audit.md`, "what
can be sped up" item 2). NOT blocked on the F2 identity-model decision
— independent of it, and written against the current (post-#802/#803)
`SyncRegion<T>` API, which is unchanged by this note's proposal.

## Motivation

`SyncRegion<T>::read()` returns a `RwLockReadGuard<'_, Region<T>>` per
call — already the correct, available primitive for batching multiple
reads under one lock acquisition (`let g = sync_region.read(); g.get(h1);
g.get(h2); ...`). The static-release-audit's own measurement: one-shot
reads (a fresh `read()` call per lookup) at 8 concurrent readers cost
~1221 ns/op, versus ~38.7 ns/op for 64 reads batched under one held
guard — roughly a 31.6x difference. **This win is already fully
achievable today** via the existing `read()` method; the gap is purely
ergonomic — nothing in the current API steers a caller toward the
batched pattern, so the natural, discoverable way to write "read a few
handles" code is the slow one-shot-per-call form.

## What NOT to change

Per finding F26 in the same audit (the caution against rushing API
additions that interact with the crate's already-published guard
types): **the concrete `RwLockReadGuard`/`RwLockWriteGuard` return types
of `read()`/`write()` must not change in any 0.1.x release.** Those
types are already part of the published 0.1.0 API surface (per the
static-release-audit's own F1 finding, 0.1.0 is live on crates.io) —
changing them is a breaking change regardless of how it's dressed up.
This note's proposal is strictly additive.

## Proposed shape

A convenience method that takes a closure operating on `&Region<T>` (or
`&mut Region<T>` for the write variant) under a single lock acquisition,
returning whatever the closure returns:

```text
impl<T> SyncRegion<T> {
    pub fn with_read<R>(&self, f: impl FnOnce(&Region<T>) -> R) -> R {
        f(&self.read())
    }

    pub fn with_write<R>(&self, f: impl FnOnce(&mut Region<T>) -> R) -> R {
        f(&mut self.write())
    }
}
```

(Illustrative only — not a final signature; see open questions below
before implementing.) This does not remove or change `read()`/`write()`;
it adds a second, more discoverable entry point that structurally
prevents the one-shot-per-call pattern for the common case: a caller who
reaches for `with_read` naturally writes `sync_region.with_read(|r| {
r.get(h1).copied().zip(r.get(h2).copied()) })` and gets one lock
acquisition by construction, rather than needing to know to hoist
`read()` out of a loop themselves.

## Open questions before implementation

1. **Method naming** — `with_read`/`with_write` follow the `Mutex::with_lock`-style
   naming some crates use, but this codebase has no existing precedent
   for that pattern (`SyncRegion` is std-`RwLock`-based, and
   `std::sync::RwLock` itself has no such convenience method) — a
   naming choice, not a technical constraint. Alternatives: `read_with`/
   `write_with`, `batch_read`/`batch_write` (more literally matches the
   audit's own "batch-read" framing), or something else — pick one and
   apply consistently, don't ship two names for the same shape.
2. **Does the closure form actually change measured throughput, or
   only ergonomics?** The 31.6x number was measured for CODE that
   already holds one guard across 64 reads (via `read()` directly) —
   this proposal doesn't invent a faster mechanism, it makes the
   already-fast mechanism the natural one to reach for. A follow-up
   gate report, if this is implemented, should measure `with_read`
   itself against the equivalent hand-written `let g = read(); ...`
   form to confirm the closure wrapper adds no measurable overhead of
   its own (expected: none, since it should inline to the same code —
   but per this repo's own evidence-discipline rules, "expected" is not
   "measured").
3. **Should `with_read`'s closure be allowed to call back into the SAME
   `SyncRegion`?** A closure that (accidentally or intentionally) calls
   `sync_region.read()`/`.write()` again from inside `with_read`'s
   closure is the crate's already-documented reentrancy self-deadlock
   hazard (see `SyncRegion`'s own doc, "reentrancy" — landed in an
   earlier round, task #687). `with_read`/`with_write` don't introduce
   a NEW hazard here (the same hazard already exists via `read()`/
   `write()` directly), but the convenience API's own doc comment
   should point at the existing reentrancy warning explicitly, since a
   closure-based API can visually suggest more isolation than it
   actually provides.
4. **`get_cloned`-shaped convenience?** `SyncRegion` already has
   `get_cloned` (single-lookup, clone-and-release). Does `with_read`
   make `get_cloned` redundant for the batched case, or do both remain
   useful for their respective single-vs-batch shapes? Likely both stay
   (different use cases), but worth an explicit doc cross-reference
   once implemented so a reader lands on the right one for their
   access pattern.

## Status

Not implemented. No follow-up implementation task filed — per this
note's own instruction (task #798's description), implementation
requires an explicit maintainer decision on the open questions above,
most importantly the naming (item 1) and the reentrancy-doc
cross-reference (item 3).
