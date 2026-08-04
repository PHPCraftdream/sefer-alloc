# Release-stabilization readiness audit (READONLY)

Date: 2026-08-04
Scope: `sefer-alloc` @ `main`, working tree at audit time (`58d59d9` + the
uncommitted changes listed in `git status`: `benches/r32_0_…`,
`scripts/capture-measurement-identity.mjs`, `scripts/verify-gate-report.mjs`,
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`, `.claude/`).
Mode: read-only. **No repository file was modified except this report.**
Nothing was committed.

Starting base (read end-to-end before auditing, so nothing below re-reports a
known item): `CLAUDE.md`, `docs/perf/OPEN_ITEMS.md`,
`docs/CORRECTNESS_OPEN_ITEMS.md`,
`docs/reviews/2026-08-03-round33-readonly-review.md`, `CHANGELOG.md`
`[Unreleased]` head, `README.md` §"Where unsafe lives", `Cargo.toml`,
`deny.toml`, `.github/workflows/{ci,kani,perf-gate,release}.yml`.

Commands actually run (all read-only):

| command | result |
|---|---|
| `cargo test --features production` | **PASS**, exit 0. No failures, no `FAILED` lines. The two known flakes (`docs/CORRECTNESS_OPEN_ITEMS.md` items 12/14) did not fire. |
| `cargo clippy --all-targets --features production -- -D warnings` | **PASS**, exit 0. |
| `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/` | 20 tier-1 module seams (13 `src/`, 7 `crates/`), 71 tier-2 item-level sites across 18 files. |
| `cargo audit` | not run — `cargo-audit` is not installed on this host. Advisory coverage is via `cargo deny` in CI (see §5). |

---

## Executive verdict

**Not yet ready to declare a stabilized (1.0-shaped) release — but nothing
found here is a memory-safety blocker in the shipped `production`
configuration.** The tree is green, the unsafe confinement is real and
compiler-enforced, and the concurrency protocols are the most carefully
documented I have seen in a crate this size.

What blocks *stabilization* is not a bug list; it is three structural gaps:

1. **B1 — there is no public-API boundary.** `alloc_core`, `global`, and
   `registry` are `#[doc(hidden)] pub mod`s gated only on `alloc-core` /
   `alloc-global`, both of which are inside `production`. Every `pub` item
   inside them — including **50 `pub unsafe fn`s** and the whole `dbg_*`
   surface — is therefore on the semver-public API of an ordinary
   `--features production` build. The crate's own `batch-api` doc
   (`src/global/sefer_alloc.rs:749-768`) states the exact principle
   ("`#[doc(hidden)]` alone is NOT a real API boundary … it hides the item
   from rustdoc but leaves it on the public semver/ABI surface") and applies
   a feature gate to *two functions* — but not to the three modules. There
   is no `cargo-semver-checks` job and no public-API snapshot test. You
   cannot promise semver stability over a surface you have not delimited.
2. **B2 — the one advisory that is genuinely in the runtime dependency graph
   is suppressed, not fixed.** `RUSTSEC-2026-0204` (`crossbeam-epoch`
   0.9.18, `Cargo.lock:222-224`) reaches the tree as a *direct optional
   runtime dep* under `experimental` (`Cargo.toml:117`), not only through
   dev-deps. `deny.toml` suppresses it with a good code-level argument, but
   shipping a release with a live advisory ignored in the runtime graph is a
   decision that should be made explicitly (fix = bump to ≥ 0.9.20, which
   this project's rules require you to authorize).
3. **B3 — the highest-contention `unsafe` seam has the thinnest verification
   coverage.** The small-block cross-thread free path
   (`RemoteFreeRing::push` from N remote threads through
   `Node::atomic_u32_at`) has **no miri coverage under concurrency at all**,
   and the F10 shadow-head fast path added in R32 (task #502) has **no loom
   model that exercises a recycled slot** — see F-1 and §4. TSan covers data
   races there; nothing covers aliasing/provenance or the slot-reuse
   ordering.

Below, findings are ordered by category as requested. Severity key:
**blocker** / **high** / **medium** / **low** / **cosmetic**.

---

## 1. UB / soundness

### Full seam inventory (verified against the canonical grep, not the docs)

**Tier 1 — module-level `#![allow(unsafe_code)]` (20):**

`src/`: `alloc_core/{os, node, numa, sidecar, dirty_by_class,
large_cache_extended}`, `concurrent/hand`, `global/{sefer_alloc, tls_heap,
fallback}`, `registry/{bootstrap, heap_registry, heap_slot}` (13).
`crates/`: `vmem`, `numa`, `racy-ptr-cell`, `ring-mpsc`, `malloc-bench`,
`globalalloc-model`, `proc-memstat` (7).

**Tier 2 — item-level `#[allow(unsafe_code)]` (71 sites / 18 files):**
`alloc_core.rs` 3, `alloc_core_core_diag.rs` 5, `alloc_core_large_cache.rs`
5, `alloc_core_small.rs` 6, `alloc_core_small_diag.rs` 5,
`alloc_core_small_magazine.rs` 1, `alloc_core_small_pool.rs` 6,
`alloc_core_small_reclaim.rs` 3, `bootstrap.rs` 1, `remote_free_ring.rs` 2,
`segment_directory.rs` 2, `segment_header_gen_table.rs` 3,
`heap_core_alloc.rs` 6, `heap_core_dealloc_batch.rs` 7, `heap_core_diag.rs`
10, `heap_core_free.rs` 6, `heap_core_tcache.rs` 1, `heap_core_xthread.rs` 1.

I read every tier-1 module in full and every tier-2 site that is reachable
from `GlobalAlloc::{alloc, dealloc, realloc, alloc_zeroed}`. The `# Safety`
doc / `// SAFETY:` discipline is genuinely upheld: I did not find a single
tier-2 `unsafe fn` without a documented contract, nor an internal `unsafe {}`
without a `// SAFETY:` justification.

---

### F-1 [medium] The F10 shadow-head fast path removes the only happens-before edge that ordered the consumer's slot-clear before a producer's publish into the same recycled slot

**Files:**
- `src/alloc_core/remote_free_ring.rs:1004-1025` — `full_check`, fast path
  returns `Ok` after only `cached_head.load(Relaxed)`.
- `:1063` (`push`) / `:1113` (`try_push_uncounted`) — producer publish
  `slot.store(offset, Release)`.
- `:1181` — consumer clear `slot.store(RING_SLOT_EMPTY, Relaxed)`.
- `:1186` — consumer `head.store(h, Release)`.
- `:142-221` — the F10 soundness section this finding is about.

**What the module doc proves, and what it does not.** The F10 argument
(`:142-194`) is a *value-domain* proof: `cached_head <= head` always, so the
fast path can only under-estimate available room, never over-estimate it. I
re-derived it and it is correct. What it never addresses is the *ordering*
role the load it removed used to play. Pre-F10, every push did
`head.load(Acquire)`; when that load observed the consumer's
`head.store(h', Release)` at `:1186`, it created a synchronizes-with edge,
hence a happens-before edge, hence — by the coherence rule for same-object
modifications — a guarantee that the consumer's `slot.store(EMPTY)` at
`:1181` precedes the producer's `slot.store(offset)` at `:1063` in that
slot's modification order. The fast path issues no such load.

**Concrete failure scenario** (the interleaving; `C = RING_CAP`):

1. Consumer drains reservation `h`, slot index `i = h mod C`: `reclaim(off)`,
   `slot_i.store(EMPTY, Relaxed)` (`:1181`), `head.store(h+1, Release)`
   (`:1186`).
2. Producer **X** enters `push`: reads `tail = t` where `t = h + C`; its
   `cached_head` is stale, so it takes the slow path, does
   `head.load(Acquire) → h+1` and `cached_head.store(h+1, Relaxed)`
   (`:1019-1020`). It then attempts `CAS(tail, t → t+1)`.
3. Producer **P** enters `push`: reads `tail = t`; `cached_head.load(Relaxed)`
   returns `h+1` (X's store); `t.wrapping_sub(h+1) = C-1 < C`, so P takes the
   **fast path** — no `head` load at all. P races X on the CAS and **wins**;
   X loses and retries at `t+1`.
4. P publishes: `slot_i.store(offset, Release)` (`:1063`).

Now trace the happens-before edges available to P. P's `cached_head` read is
`Relaxed` reading a `Relaxed` store — no synchronizes-with. P's `tail` CAS is
`AcqRel` and does acquire from the release sequence of every *earlier* `tail`
RMW — but X's successful CAS is at `t+1`, i.e. **after** P's in `tail`'s
modification order, so X's history (which does contain the edge to the
consumer) is not in P's past. No other chain exists. Therefore
`slot_i.store(EMPTY)` and `slot_i.store(offset)` are unordered by
happens-before, and the modification order of `slot_i` between them is
unconstrained by the abstract machine.

**Consequence if the abstract machine's freedom were realized** (i.e. the
`EMPTY` store lands last in `slot_i`'s modification order): the published
offset is lost, and — worse than a lost block — the drain at `:1165-1183`
reaches `h' = t`, reads `EMPTY`, `break`s, and `head` never advances past
`t` again. Occupancy saturates at `C`, every subsequent cross-thread free to
that segment overflows, and the segment's `live_count` never reaches 0, so it
is never released: a *permanent per-segment leak plus a dead ring*. Note this
is **not UB** — both accesses are atomic on the same `AtomicU32`, so there is
no data race; it is a lost-update / liveness defect.

**Honest limits of this finding.** I could **not** construct a scenario in
which this is realizable on any hardware Rust actually targets. On x86-TSO
and on the multi-copy-atomic weak models (ARMv8, RISC-V RVWMO), the physical
chain `clear → head(stlr) → X's head(ldar) → X's store → P's load → P's
store(stlr)` makes the clear globally visible before P's store is issued. On
POWER the same chain is the "ISA2" shape with an lwsync at each hop, which
POWER's cumulativity forbids from reordering. So this is a **proof gap and a
verification gap, not a demonstrated bug** — but it is a real gap, because
(a) the module doc presents the F10 argument as complete and it is not, and
(b) nothing in the test suite would catch a regression here (see §4/G2).

**Suggested direction (cheap and provable).** Promote the two `cached_head`
accesses in `full_check`:

- `:1005` `cached_head.load(Ordering::Relaxed)` → `Acquire`
- `:1020` `cached_head.store(h, Ordering::Relaxed)` → `Release`

X's `Release` store on `cached_head` happens-after the consumer's clear (via
X's own `head.load(Acquire)` at `:1019`), and P's `Acquire` load then
synchronizes-with it — restoring exactly the edge the removed `head` load
supplied, on a word that is already on the producer's own cache line
(so no new cross-core traffic; the cost is an acquire/release *fence
strength*, not a fence *instruction*, on x86, and one `ldapr`/`stlr` on
aarch64). R32-11 measured that a *locked RMW* on this path regressed;
an acquire/release on an already-hot line is a materially different cost and
would need its own (cheap) measurement, not a re-use of that verdict.
If the ordering promotion is rejected on cost grounds, the minimum action is
to **write the missing half of the argument down** in the F10 section and
state explicitly that it rests on hardware cumulativity rather than on a
happens-before chain in the abstract machine.

---

### F-2 [low — hypothesis, no repro constructed] Provenance asymmetry: the task-#142 exposed-provenance fix was applied to `atomic_ptr_ref` only, not to the ring's `atomic_u32_at`

**Files:** `src/alloc_core/node.rs:561-593` (`atomic_ptr_ref`, uses
`expose_provenance` / `with_exposed_provenance_mut`) vs. `:406-427`
(`atomic_u32_at`), `:491-505` (`atomic_u64_at`), `:377-395` (`atomic_u8_at`)
— all three do a plain `&*ptr`.

`atomic_ptr_ref`'s own comment (`:564-578`) states the reason for the
exposed-provenance form precisely: *"if a REMOTE thread reconstructed `&*ptr`
under a reference tag and wrote …, that write would be 'foreign' to the
stamp's tag and DISABLE it, so a SECOND remote reading through a sibling
`&*ptr` would hit UB (Stacked/Tree Borrows)"*. That is a **remote-vs-remote**
argument. But `atomic_u32_at` backs `RemoteFreeRing`'s `head`/`tail`/
`cached_head`/`slots` (`remote_free_ring.rs:953-982`), which are written
concurrently by an *arbitrary number* of remote producer threads via
`push`/`try_push_uncounted` — the identical remote-vs-remote shape — and
`atomic_u64_at` backs `SegmentHeader::owner_state`, also read cross-thread.
Neither the code nor the docs give a reason for the split.

I did not construct a repro, and I do not claim one exists: under Tree
Borrows an `&AtomicU32` derived from a raw pointer gets the `Cell`
permission, which is immune to foreign accesses, so the concern is
Stacked-Borrows-specific at most. What makes it worth listing is that the
question is **currently unanswerable by this repo's own tooling**: there is
no miri test that drives ≥2 concurrent remote small-block ring pushes (§4,
G1), so the pattern has never actually been run under the model that would
decide it. Suggested direction: add the miri test (G1) first; only if it
flags, apply the `atomic_ptr_ref` treatment to `atomic_u32_at`.

---

### F-3 [low — documented residual, restated for the release notes] Cross-thread routing reads and writes foreign segment memory under a "magic != 0" guard only

**Files:** `src/registry/heap_core_xthread.rs:858-1007`
(`dealloc_foreign_routing`), specifically `:903` (null-base guard), `:906`
(magic guard), `:960` (`push_large_deferred_free` writes the foreign
segment's header), `:999`/`:1005` (`push_with_overflow_retry` writes the
foreign segment's ring).

The code documents this honestly (`:864-885`: cases (a) live-foreign and (b)
already-released are O(1)-indistinguishable; a double free of a released
segment is "fundamentally UB … not fixed by this change"). I verified there
is no *additional* window: for a **single, legitimate** cross-thread free the
segment cannot be released underneath the freer, because the block being
freed keeps `live_count ≥ 1` until the owner's drain reclaims it, and the
owner only releases at `live_count == 0`. So the residual is exactly, and
only, the caller-contract-violation surface (double free / stale pointer),
which every allocator has.

Two smaller things in the same neighbourhood worth naming:

- `set_dirty_bit_for_segment` (`:335-350`) and `resolve_heap_overflow`
  (`:1526-1541`) both read an `owner_id` **out of foreign segment memory**
  and, after only an `idx < MAX_HEAPS` range check, call `Registry::slot(idx)`
  → `ensure_chunk(idx / CHUNK_SLOTS)`. A garbled-but-in-range id therefore
  causes a *fresh OS reservation of a registry chunk* on a `dealloc` path.
  Harmless in isolation, but see F-4.
- `dealloc_foreign_routing`'s bind-less caller (`SeferAlloc::dealloc`'s
  `ForeignNoBind` arm, `src/global/sefer_alloc.rs:895-937`) skips the
  `owner_tf == our_head` half of the self-check by design; that is correct
  and documented.

---

### F-4 [medium] `std::process::abort()` is reachable from `GlobalAlloc::dealloc`

**File:** `src/registry/bootstrap.rs:775` (`ensure_chunk_slow`'s
chunk-materialisation-OOM branch), reached from `Registry::slot`
(`:547-555`) ← `resolve_heap_overflow` / `set_dirty_bit_for_segment` ←
`HeapCore::dealloc_foreign_routing` ← `SeferAlloc::dealloc`.

**Scenario:** a cross-thread `dealloc` whose target segment's owner lives in
a registry chunk this process has not yet materialised, executed while the OS
refuses a small VM reservation → the process aborts inside `dealloc`.
`GlobalAlloc::dealloc` is infallible by contract; aborting is a legal but
very blunt way to honour that. The code's own comment
(`:757-774`) says the abort is kept only because
`Registry::slot` returns `&'static HeapSlot` rather than `Option<..>` and
"widening `slot()` to `Option` is deliberately out of scope".

Practically this is very rare (both preconditions must hold at once), but for
a stabilized release the exposure should be a conscious, documented decision
rather than an out-of-scope note: either widen `slot()` to `Option` on the
*free* path only (both call sites already have a graceful "defensive: return"
branch two lines above — `heap_core_xthread.rs:344-346` and `:1531-1533` —
so the plumbing is trivial), or document "sefer-alloc aborts the process on
registry-chunk OOM" in `SECURITY.md`/README's operational section.

---

### F-5 [low] Release-surviving panic sites are reachable from the allocator entry points, contradicting the module's own "NEVER panics" claim

**Claim:** `src/global/sefer_alloc.rs:43-58` — *"A panic in
`#[global_allocator]` aborts the process … Every entry point here returns
null on failure and NEVER panics."*

**Counterexamples (release builds, not `debug_assert!`):**

- `src/alloc_core/alloc_core.rs:2158` —
  `assert!(self.table.contains_base_ro(base), "known-base realloc called for
  a segment not owned by this core")`, inside
  `realloc_inplace_fast_path_known_base`, reachable from
  `HeapCore::realloc` ← `SeferAlloc::realloc`.
- `src/alloc_core/alloc_core_large_cache.rs:128`, `:141` — `.expect(
  "large_cache_slot_take: empty base slot"/"…extension slot")`, and `:147`,
  `:302` — `unreachable!(…)`, on the `alloc_large` cache-hit and
  `dealloc`/eviction paths. R32-12 (task #503) made the free-slot search read
  the `large_cache_occupied` bitmask (`:239`) while the take/set sites
  maintain it in lockstep (`:129/:142/:287/:298`); a desync between the
  bitmask and the array now surfaces as a **panic inside the global
  allocator** rather than a graceful miss.

All of these are "cannot happen" invariant checks and I could not construct a
reachable violation. The finding is the **divergence between the documented
contract and the code**, which matters for a stabilization release because
the doc is what downstream readers will rely on. Note also that the *good*
pattern already exists in this codebase and is worth pointing the asserts at:
`AllocCore::reclaim_offset` (`alloc_core_small_reclaim.rs:208-217`)
explicitly says *"would … panic inside the global allocator → process abort.
Bounds-check FIRST and no-op … honouring the no-panic alloc-path
discipline"* and does exactly that.

Suggested direction: either soften the three sites to graceful no-ops in the
same style as `reclaim_offset` (keeping a `debug_assert!` as the loud
development signal), or amend the `sefer_alloc.rs` module doc to say "returns
null on failure; the remaining release-surviving asserts are invariant
tripwires whose firing aborts the process by design", and enumerate them.

---

### Checked and found sound (category 1)

- **Integer overflow in size/offset arithmetic.** `align_up`
  (`segment_header.rs:873-878`) deliberately uses `n.div_ceil(a) * a` rather
  than `n + a - 1` — the comment names the overflow motive. `alloc_large`
  guards the header+payload sum with `checked_add`
  (`alloc_core_large.rs:153-156`) and returns null on wrap;
  `realloc_inplace_fast_path_known_base` uses `payload_off.checked_add(new_eff)`
  (`alloc_core.rs:2172`). `LARGE_CACHE_SIZE_FACTOR` multiplication uses
  `saturating_mul` (`alloc_core_large.rs:220`), `reserved_capacity_target`
  uses `saturating_mul(..).min(..).max(..)` (`:479-482`),
  `large_cache_used_bytes` decrements with `saturating_sub` (`:261`).
  `SegmentMeta::dec_live` uses `saturating_sub` with a documented rationale.
  I found **no** unguarded `+`/`*` on a caller-controlled size that feeds a
  reservation length or an offset.
- **Ring entry packing cannot collide with the sentinel.** Both the
  non-hardened `[class:10|off:22]` and the hardened `[gen:8|class:6|off16:18]`
  packings are pinned by compile-time asserts that
  `SMALL_CLASS_COUNT` stays strictly below the class field's all-ones value
  (`remote_free_ring.rs:409-414`, `:577-581`), and `RING_CAP.is_power_of_two()`
  is pinned (`:370-376`) with the exact wrap argument spelled out. The
  `SMALL_CLASS_COUNT <= 64` pin for the `u64` touched-class bitmask is at
  `:681-685`; the `LARGE_CACHE_SLOTS + LARGE_CACHE_EXTENDED_SLOTS <= 64` pin
  for `large_cache_occupied` is at `alloc_core.rs:107-112`.
- **`reclaim_offset` is genuinely hardened against a garbled ring entry**
  (`alloc_core_small_reclaim.rs:202-280`): class-index bound, magic, kind,
  block-size-multiple, metadata-region floor (`off < payload_start`), and the
  `off >= bump` stale-into-decommitted guard (made unconditional by M-1/UBFIX-3,
  `:274-278`). Every rejection is a `return false`, never a panic.
- **`unaligned` reads.** `Node::{read_next, write_next}` use
  `read_unaligned`/`write_unaligned` with the reason stated
  (`node.rs:100-108`, `:74-91`); `atomic_u32_at`/`atomic_u64_at` require and
  document 4-/8-byte alignment derived from `offset_of!` on `#[repr(C)]`
  headers.
- **No `transmute` anywhere in `src/` or `crates/`.**
- **Registry claim/recycle protocol.** `claim`/`claim_with_config`
  (`heap_registry.rs:118-314`) publish `initialised` with `Release` strictly
  after `heap_ptr.write(hc)` and pair it with `Acquire` on the read side;
  `recycle` (`:342-375`) is a `LIVE→FREE` CAS whose failure branch is a
  documented no-op so a double-recycle cannot double-push the free stack. The
  OOM-on-materialisation push-back (`:140-146`, `:232-238`) closes the leaked-
  slot case; the `ConflictRollback` guard (`:264-298`) closes the
  panic-during-`debug_assert!` case.
- **TLS teardown / TORN.** `global/tls_heap.rs` does **not** rely on TLS
  destructor ordering; the three independent reasons at `:47-67` are correct,
  and the `Э2` single-branch sentinel collapse (`:356`, `:410`, `:499`,
  `:565`) is arithmetically right for `null = 0` / `TORN = usize::MAX`.
  `finish_bind` arms the guard *before* publishing to `LOCAL` and rolls the
  claimed slot back if arming fails (`:678-695`) — a real fix for a real
  leak.
- **`unsafe impl Sync for HeapSlot`** (`heap_slot.rs:527`) with no
  `unsafe impl Send`, and every slot field `pub(crate)` (the M7 note,
  `:58-74`) — the soundness-boundary reasoning is stated and the visibility
  actually matches it.

---

## 2. Stack overflow

### F-6 [low] `HeapCore` (~7 KB) is constructed by value on the stack on a thread's first allocation

**Files:** `src/registry/heap_registry.rs:137-139` (`HeapCore::new(idx)` →
`heap_ptr.cast::<HeapCore>().write(hc)`), `:229-231` (same for
`new_with_config`), `src/global/fallback.rs:189-199` (same into the
`static mut MaybeUninit<HeapCore>`).

`HeapCore` lives inline in `HeapSlot`; the in-tree `-Zprint-type-sizes`
note at `src/registry/heap_slot.rs` (the PERF-PASS-4/G8/ML2 comment block,
~`:100-125`) cites concrete field offsets **6976..7040** *inside*
`heap: HeapCore`, so `size_of::<HeapCore>()` is ≈ 7 KB. (I did not
independently measure it — there is no `size_of::<HeapCore>()` assertion
anywhere in `src/` or `tests/`, which is itself worth adding.)

Rust does not guarantee return-value / move elision. In a debug build, or on
a toolchain/backend that materialises the temporary,
`HeapCore::new(..) → write(hc)` can put one or two ~7 KB copies on the stack
of whichever frame triggers a thread's **first** allocation. That frame is
often very early in a thread's life (or inside another thread-local's
initialiser), and threads with small stacks (embedded-ish 16–64 KB, or a
constrained thread pool) are a realistic deployment. This is the only
stack-pressure item I found, and it is the one worth a pin.

Suggested direction: add `const _: () = assert!(size_of::<HeapCore>() <= N);`
next to the existing layout pins (the project already does this for
`SegmentHeader` — `segment_header.rs:1324`), so the figure cannot grow
silently; optionally convert `HeapCore::new` to an in-place initialiser
(`fn new_in_place(dst: *mut HeapCore, id: u32) -> bool`) to remove the
temporary entirely.

### Checked and found sound (category 2)

- **No unbounded or data-dependent recursion.** The only self-recursion on
  the allocator paths is `AllocCore::realloc` → `self.alloc(new_layout)`
  (`alloc_core.rs:1994`) → `alloc_small`/`alloc_large`, which is exactly one
  level and cannot re-enter `realloc`. `alloc_large` → `alloc_large_slow` is
  a tail call, not a cycle (`alloc_core_large.rs:395`, `:417`).
- **No recursive drop glue.** Free lists are intrusive singly-linked lists
  traversed with `while` loops (`Node::read_next`), never dropped
  recursively; `HeapCore` is never dropped at all (registry slots are reused
  whole, the fallback is a leaked `static`), so there is no drop-chain depth
  that scales with anything user-controlled.
- **No large stack buffers in the hot path.** The largest stack arrays in the
  whole tree are `emptied_bases: [*mut u8; 64]` (512 B, on the cold
  `drain_heap_overflow` path, `heap_core_xthread.rs:593-594`) and
  `[0u8; 256]` in `crates/numa/src/lib.rs:380`. A full-tree grep for arrays
  of ≥1000 elements returns nothing else.
- **No `alloca`, no VLA-equivalent, no unbounded `SmallVec`-style inline
  buffer.**

---

## 3. Panic safety

### F-7 [low] `RemoteFreeRing::drain` has no unwind guard, unlike its two sibling paths that do

**File:** `src/alloc_core/remote_free_ring.rs:1137-1188`.

The loop order is: `reclaim(off)` (`:1177`) → `slot.store(EMPTY)` (`:1181`) →
`h = h.wrapping_add(1)` (`:1182`); `head.store(h, Release)` happens **only
after the loop completes** (`:1186`). If the `reclaim` callback unwinds after
a successful reclaim, `head` is never published, so the *next* drain
re-reads the same reservation index and re-reclaims an offset that is already
in the `BinTable` — a double insertion into the free list, i.e. the same
block handed out twice.

`AllocCore::reclaim_offset` itself is explicitly panic-hardened (see §1), so
the closure's *first* call is safe. But the closures at
`alloc_core_small.rs:905-921`, `:1138`, `:2691`,
`alloc_core_small_reclaim.rs:528-539` and `heap_core_xthread.rs:613-675` also
call `dec_live_and_maybe_decommit`, `sync_directory_for_segment_classes`, and
(under `fastbin`) a magazine-residency predicate — none of which carries that
stated no-panic contract, and `debug_assert!`s exist along those paths
(e.g. `segment_header.rs:1045`, `alloc_core_small_pool.rs:349`, `:550`,
`:641`). **I could not identify a currently-reachable panic inside the
closure in a release build**, so this is a hardening/consistency item, not a
live bug. It is listed because the project already fixed exactly this class
twice elsewhere — `fallback::LockGuard` (`fallback.rs:283-315`, with a
dedicated `dbg_panic_in_with_heap_releases_lock` test) and
`ConflictRollback` (`heap_registry.rs:264-298`) — and `drain` is the one
remaining place with the same shape and no guard.

Suggested direction: hoist `head.store(h, Release)` into a small RAII guard
(the `LockGuard` pattern) so a partial drain still publishes the progress it
actually made.

### F-8 [low] `fallback::heap_ptr`'s init race can wedge the whole process if `HeapCore::new` panics

**File:** `src/global/fallback.rs:176-250`.

The winner CASes `INIT_STATE` to `INITIALIZING` (`:176-183`) and only stores
`READY` (`:223`) or rolls back to `UNINIT` (`:237`) on the two *normal*
outcomes. There is no guard. If `HeapCore::new(u32::MAX)` (`:189`) unwinds,
`INIT_STATE` stays `INITIALIZING` forever and every other thread spins
without bound in `while INIT_STATE.load(Acquire) == STATE_INITIALIZING`
(`:247-249`) — a process-wide livelock, the exact failure mode the L4
`LockGuard` fix eliminated one function down. Same caveat as F-7: `HeapCore::new`
is designed not to panic; the finding is the missing structural guard, one
level up from where the project already added one.

### Checked and found sound (category 3)

- **The fallback spinlock is panic-safe** and *proven* so: `LockGuard`'s
  `Drop` releases on unwind (`fallback.rs:311-315`), with
  `dbg_panic_in_with_heap_releases_lock` (`:334-351`) driving a real
  `catch_unwind` through it and `tests/` running that hook.
- **`claim_with_config`'s `debug_assert!` cannot leak a slot**: the
  `ConflictRollback` guard is armed before the assert and `mem::forget`ed
  only on the release path (`heap_registry.rs:276-298`).
- **No unwind across an FFI boundary.** The only FFI is inside
  `crates/vmem` / `crates/numa` / `crates/proc-memstat`; no Rust callback is
  ever passed to a C function, so there is no `extern "C"` frame a panic
  could cross.
- **`catch_unwind` appears in exactly one place** — the `#[doc(hidden)]`
  test hook above — and never on a production path.
- **No poisoned-state-read-as-valid vector found.** The two structures with a
  "published" flag (`HeapSlot::initialised`, `RacyPtrCell`'s three-state
  word) both publish with `Release` after the payload write and are read with
  `Acquire`; a failed init rolls the state back rather than leaving a
  half-published value (`heap_registry.rs:140-146`, `bootstrap.rs:757-775`).

**One caveat on the whole category:** a panic escaping `GlobalAlloc::alloc`/
`dealloc`/`realloc` is, on current Rust, an abort (the `__rust_alloc*` shims
are `#[rustc_nounwind]`), not UB. Nothing in this crate documents that it
*relies* on that, and nothing requires `panic = "abort"` downstream. That is
fine, but it should be stated in the release notes rather than left implicit
in a "NEVER panics" claim (F-5).

---

## 4. Test coverage of the critical invariants — what exists and what is missing

**What exists (verified against `.github/workflows/ci.yml` + `kani.yml`):**

| Tool | Coverage |
|---|---|
| miri (strict provenance) | `region_invariants`; `decommit_miri_cycle` (os/vmem decommit cycle); `reclaim_offset_unit` (node arithmetic); 4 `alloc-core` align/boundary tests; 2 `fastbin` tests (`regression_magazine_oracles`, `regression_bump_direct_refill`) |
| miri (plain / Stacked Borrows) | `regression_xthread_large_free_no_leak` (the `deferred_large` exposed-provenance push/drain stack), `regression_xthread_thread_free_alias_miri` (the H1 `AtomicPtr` aliasing guard), at `-Zmiri-preemption-rate=0.5` |
| loom | 13 in-tree models (`loom_remote_ring`, `loom_remote_ring_drain_guard`, `loom_xthread_protocol`, `loom_deferred_large`, `loom_dirty_publish`, `loom_dirty_multi_segment`, `loom_class_aware_dirty`, `loom_heap_overflow`, `loom_heap_overflow_drain_guard`, `loom_overflow_first_retry`, `loom_magazine_ring_compose`, `loom_thread_free`, `loom_sharded`/`loom_epoch`) + 3 real-type crate suites (`racy-ptr-cell`, `tagged-index-stack`, `ring-mpsc`) |
| kani | `src/kani_proofs.rs` — `alloc_core::node` primitives (`--features alloc-core`) and `concurrent::hand` (`--features experimental`) |
| TSan | 2 steps: `alloc-global alloc-xthread` (race_repro, race_norecycle, global_alloc_mt) and `production` (global_alloc_mt, tls_heap_teardown_ordering_stress, regression_percounter_perheap_aggregation, regression_realloc_xthread_stamp, stress_concurrent_boundaries) |
| multi-arch | aarch64 via `cross`, including `--features production` |

That is a genuinely strong matrix. The gaps below are specific, not general.

### G1 [high, for a stabilization release] The small-block cross-thread free path has **zero** miri coverage under concurrency

The crate's most-contended `unsafe` seam is `Node::atomic_u32_at` →
`RemoteFreeRing::{push, drain}` with N remote producers and one owner. The
miri matrix covers: single-threaded reclaim arithmetic (`reclaim_offset_unit`),
and the **Large** cross-thread path (`AtomicPtr` deferred stack) in
`miri-plain`. It does **not** cover a concurrent multi-producer small-block
ring push at all, under either provenance mode. TSan covers *data races*
there but says nothing about Stacked/Tree Borrows aliasing or provenance —
which is exactly the question F-2 raises and cannot answer.

Suggested: one small `miri-plain` test — 2 spawned threads doing a handful of
cross-thread frees of small blocks against an owner thread that drains — in
the same style as `regression_xthread_thread_free_alias_miri` (small N, so it
stays miri-affordable).

### G2 [medium] No loom model exercises the F10 fast path over a **recycled** slot

`tests/loom_remote_ring.rs` has two shadow models, and neither reaches the
interleaving F-1 describes:

- `shadow_ring_never_loses_or_duplicates` uses `CAP = 4` with **2 pushes and
  a single post-join drain** — the cursor never wraps, so no slot is ever
  reused.
- `RingModelShadow1` is `CAP = 1`, and its own doc says so explicitly: *"a
  `CAP = 1` ring is full after exactly one live reservation, so the shadow
  fast path can never prove room for the second producer — every
  second-producer attempt MUST take the real-Acquire-load slow path"*. By
  construction it models the **slow** path only.

So the one thing F10 actually changed — "a producer proves room from the
shadow alone and reserves a slot the consumer just cleared" — is modelled by
nothing. (Honest caveat: loom's store history is append-only per atomic, so
even with that interleaving added loom would probably not *detect* F-1's
lost-update outcome; the value of the model would be regression-pinning the
protocol, and the ordering question needs the doc fix or the
`Acquire`/`Release` promotion, not a test.)

### G3 [medium] Five tier-1 seams have no miri, no loom, and no kani harness

`global::sefer_alloc` (the `unsafe impl GlobalAlloc` itself),
`global::fallback` (the `static mut MaybeUninit<HeapCore>` + spinlock),
`registry::heap_slot` (the `unsafe impl Sync` — the single load-bearing
`Sync` proof in the crate), `alloc_core::sidecar` (the shared
lazily-materialised sidecar `deref`/`deref_mut` boundary, on the
`production` path via `alloc-segment-directory` and `class-aware-dirty`), and
`alloc_core::large_cache_extended` are covered by ordinary integration tests
only. `alloc_core::dirty_by_class` has `loom_class_aware_dirty`, but per
ci.yml's own note that file *"re-models the `class-aware-dirty` protocol on
its own hand-rolled `loom::sync` atomics, not the real
`PerClassDirty`/`RacyPtrCell` types"* — so the real sidecar deref is
unmodelled too.

### G4 [low] kani proves only the smallest seam and a deprecated tier

`src/kani_proofs.rs` covers `alloc_core::node`'s primitives and
`concurrent::hand` (the research tier). The two highest-value CBMC-reachable
properties are unproven: (a) the ring's wrap arithmetic — that
`t.wrapping_sub(h) < RING_CAP` is an invariant of the push/drain pair across
the `u32::MAX → 0` boundary; and (b) `pack_entry`/`unpack_entry` (both
packings) round-trip and never produce `RING_SLOT_EMPTY` over the full real
input ranges. Both are pure arithmetic with no pointers — ideal kani targets,
and both are currently protected only by unit tests plus `const _: () =
assert!` on the *bounds*, not on the *round trip*.

### G5 [low] The fuzz targets are never actually run

`ci.yml`'s `fuzz-build` job compiles all three libFuzzer targets per push and
runs none. `fuzz/README.md` says the campaigns are "scheduled/manual", but
there is no scheduled fuzz job in any workflow — the only `schedule:` trigger
in `ci.yml` drives `numa-real-kernel` and `feature-powerset`. So "scheduled"
is currently aspirational. For a stabilization release, a bounded nightly or
weekly run (even 10 minutes per target with a committed corpus) is the
cheapest remaining coverage.

### G6 [low] ASan is wired but not in CI

`tests/asan_alloc_core.rs` and `scripts/asan.mjs` (`npm run asan`) exist and
are well-designed (the harness deliberately drives `AllocCore` directly
rather than installing it as `#[global_allocator]`, to avoid fighting ASan's
interceptors). No workflow runs them; `grep -rn "asan" .github/` returns
nothing. Linux-only, so it belongs in the existing `ubuntu-latest` matrix at
essentially zero marginal cost.

---

## 5. Other release-readiness items (status, briefly)

| Item | Status |
|---|---|
| **`cargo deny`** | In CI (`deny` job), `cargo deny check` with all four categories, `all-features = true` graph. **Runs per push/PR.** Three suppressed advisories, each with a written rationale. Two (`RUSTSEC-2025-0141` bincode, `RUSTSEC-2026-0173` proc-macro-error2) are dev-only via `iai-callgrind` — genuinely not shipped. **`RUSTSEC-2026-0204` (`crossbeam-epoch` 0.9.18) is in the runtime graph** under the `experimental` feature — see B2. |
| **`cargo audit`** | Not installed locally; not in CI. Redundant with `cargo deny check advisories`, so no gap — noted only because the audit brief asked. |
| **MSRV** | `rust-version = "1.88"`, enforced by the `msrv` job as `cargo check --all-features` on a pinned 1.88 toolchain. **Check-only, never `cargo test`** — an MSRV-incompatible construct in a `#[cfg(test)]`-only or dev-dependency path would not be caught. Acceptable, but worth stating in the release notes. |
| **`no_std`** | Claim is narrow and correct: only `Region<T>`/`Handle<T>` are `no_std + alloc`; the whole allocator stack is `std`-only, stated at `src/lib.rs:64-67` and `sefer_alloc.rs:158-165`. CI proves it properly — `cargo build --no-default-features --target thumbv7em-none-eabi` (a real bare-metal target, not a host build). Sound. |
| **Semver / public API** | **The main structural blocker (B1).** No `cargo-semver-checks`, no public-API snapshot test, and `alloc_core`/`global`/`registry` are `#[doc(hidden)] pub` inside `production`. `tests/r31_10_trim_current_thread_api.rs` is the only API-shaped test and covers one method. |
| **README vs. code** | Mostly excellent — `tests/no_stale_doc_references.rs` pins the aggregate tier-1/tier-2 counts and the test-file count, and the README's tier-1 seam table is complete and correct (all 20, including `sidecar` and `large_cache_extended`). Two drifts found, below. |
| **Fuzzing / ASan** | See G5, G6. |
| **`release.yml`** | Exists; not audited in depth (out of the brief's core scope). |

### F-9 [low] `src/lib.rs`'s tier-1 seam inventory is missing two seams that the README lists

`CLAUDE.md` states the seams are *"inventoried in README §'Where unsafe lives
— the complete list' and mirrored in the `src/lib.rs` header"*. The mirror
has drifted: `src/lib.rs:173-224` enumerates **11** internal tier-1 seams
(`os`, `node`, `sefer_alloc`, `tls_heap`, `fallback`, `registry::bootstrap`,
`heap_slot`, `heap_registry`, `numa`, `dirty_by_class`, `hand`) — omitting
**`alloc_core::sidecar`** (`sidecar.rs:98`) and
**`alloc_core::large_cache_extended`** (`large_cache_extended.rs:105`), both
of which the README's table at `README.md:595-596` lists correctly, and both
of which the canonical grep returns. `alloc_core::sidecar` is on the
`production` path (it backs `SegmentDirectory`'s reservation under
`alloc-segment-directory`, which is in `production`), so the crate root's
"complete, verifiable picture" is understating the production seam set by
one module. `tests/no_stale_doc_references.rs` pins README's aggregate
numbers, not `lib.rs`'s prose list, so nothing catches this.

### F-10 [cosmetic] Two doc sites describe `production` as a 4-/5-feature bundle; it has been 7 since R13-9

`Cargo.toml:399` — `production = ["alloc-global", "alloc-xthread",
"alloc-decommit", "fastbin", "alloc-segment-directory",
"primordial-lazy-commit", "class-aware-dirty"]`.

- `src/global/sefer_alloc.rs:154` — *"the `production` feature bundle
  (`alloc-global + alloc-xthread + alloc-decommit + fastbin`)"* — missing
  **three**.
- `src/lib.rs:185` — *"`production` = alloc-global + alloc-xthread +
  alloc-decommit + fastbin + alloc-segment-directory"* — missing **two**.
- `README.md:39`, `:606`, `:1311` — same 5-feature list, missing **two**.

Harmless in isolation, but it is exactly the kind of drift a release
announcement copies. Cheap mechanical fix: extend
`tests/no_stale_doc_references.rs` to parse `Cargo.toml`'s `production =
[...]` and assert the rendered list appears verbatim wherever the bundle is
spelled out.

---

## 6. Explicit list of what was checked and found reliable

This is as much of the result as the findings are.

- `cargo test --features production` — green, exit 0, on this exact tree.
- `cargo clippy --all-targets --features production -- -D warnings` — clean,
  exit 0. (R33-1 genuinely fixed the five red rows; the local gate matches
  ci.yml byte-for-byte via `scripts/check-matrix.mjs`, pinned by
  `tests/ci_clippy_matrix_consistency.rs`.)
- The unsafe-confinement claim is **real and compiler-enforced**:
  `#![cfg_attr(not(any(experimental, alloc-core)), forbid(unsafe_code))]` +
  `#![deny(unsafe_code)]` at the crate root (`src/lib.rs:254-258`), with
  `fastbin`-without-`alloc-xthread` additionally rejected by a
  `compile_error!` (`:278-284`). A stray `unsafe` outside a tier-1 module or
  a tier-2 item allow does not compile in any configuration. I verified the
  canonical grep is comment-proof as documented (`^\s*#!?\[`).
- Every tier-2 `unsafe fn` I read carries a `# Safety` section, and every
  internal `unsafe {}` block carries a `// SAFETY:` comment naming the
  invariant it relies on. This is not boilerplate — several of them
  (`node.rs:459-489`'s `'static` lifetime note, `heap_core_xthread.rs:864-885`'s
  (a)/(b) indistinguishability, `tls_heap.rs:47-67`'s three independent
  destructor-ordering arguments) are genuinely load-bearing analysis that a
  reviewer can check.
- Memory orderings on the ring, the registry, the dirty bitmaps, and the
  deferred-large stack are **logically** correct for the invariants they
  protect, not merely present — I re-derived each pairing rather than
  pattern-matching. The one exception is F-1, which is an *omission* in the
  proof, not a wrong ordering.
- Cross-thread UAF: for a **single, legitimate** cross-thread free there is
  no window in which the target segment can be released underneath the freer
  (live-count argument, §1 F-3). The documented residual is the
  double-free/stale-pointer case, which is caller-contract UB in every
  allocator.
- `alloc_zeroed` correctness across the large-cache reuse path is right: a
  cache **hit** returns freshness `false` (`alloc_core_large.rs:413`), so the
  caller zeroes; only a genuinely fresh OS reservation returns
  `cfg!(not(miri))` (`:587`), with the miri-specific reason documented.
- The `RemoteFreeRing` wrap/sentinel/packing invariants are all pinned at
  compile time (listed in §1).
- Panic-safety of the fallback spinlock and of the config-conflict path is
  both implemented **and** proven by dedicated hooks/tests.
- `HeapSlot`'s `Sync` proof, the absence of `Send`, and the `pub(crate)`
  field visibility that the proof depends on are consistent with each other.
- CI breadth is genuinely good: 5 clippy rows, ~13 feature-isolation test
  rows, Windows + macOS + aarch64(cross), 4 miri jobs, 4 loom jobs, 2 TSan
  steps, kani in its own workflow, `cargo deny`, a weekly
  `cargo hack --feature-powerset --depth 2`, and rustdoc.
- The two known flaky tests (`docs/CORRECTNESS_OPEN_ITEMS.md` items 12 and
  14) did **not** reproduce in this run; I did not attempt to reproduce them
  and they remain correctly filed.

---

## 7. Prioritized next steps for release stabilization

**Must do before calling a release "stable":**

1. **B1 — draw the public-API boundary.** Put `alloc_core`, `global`, and
   `registry` behind an `internals` (or `unstable-internals`) Cargo feature,
   exactly the way `batch-api` already gates `alloc_batch`/`dealloc_batch`
   and for exactly the reason that doc comment gives. Keep
   `SeferAlloc`/`AllocStats`/`Profile`/`LargeCacheConfig`/`Region`/`Handle`
   as the stable surface. Then add `cargo-semver-checks` to CI against that
   surface. This is the single highest-leverage item on the list; it is also
   the one that gets harder the longer it waits.
2. **B2 — decide `RUSTSEC-2026-0204` explicitly.** Either authorize a
   `crossbeam-epoch` bump to ≥ 0.9.20 (per project rules, this needs your
   explicit go-ahead) or record in `SECURITY.md` that a release ships with it
   ignored and why. Right now the decision lives only in a `deny.toml`
   comment.
3. **G1 — add the concurrent small-block ring miri test.** One small
   `miri-plain` test closes the largest single verification hole and is the
   prerequisite for answering F-2 one way or the other.
4. **F-1 — close the F10 proof gap.** Either promote `cached_head`'s
   accesses to `Acquire`/`Release` in `full_check` (2 lines, provable,
   near-zero cost — measure it), or write down the hardware-cumulativity
   argument and mark the F10 section as resting on it. Do not leave the
   section reading as a complete proof.

**Should do:**

5. **F-4** — make `Registry::slot` fallible on the *free* path (both call
   sites already have a graceful branch), or document the abort in
   `SECURITY.md`.
6. **F-5** — reconcile the "NEVER panics" claim with the three
   release-surviving assert sites (soften them like `reclaim_offset`, or
   enumerate them in the doc).
7. **G2** — add a loom model with `CAP = 2` (or `CAP = 4` with a wrap) that
   drives the shadow fast path onto a just-drained slot, to pin the protocol
   against future regression.
8. **G6** — add the ASan job to CI (it is already scripted and Linux-only).
9. **F-6** — pin `size_of::<HeapCore>()` with a `const _: () = assert!(...)`
   next to the existing `SegmentHeader` layout pin.

**Nice to have / cheap:**

10. **F-9 / F-10** — fix the `src/lib.rs` seam list (add `sidecar`,
    `large_cache_extended`) and the three stale `production` feature lists;
    extend `tests/no_stale_doc_references.rs` to pin both mechanically.
11. **F-7 / F-8** — add the two missing RAII guards (`drain`'s `head`
    publish; `fallback::heap_ptr`'s `INITIALIZING` rollback), for consistency
    with the two the project already has.
12. **G4** — add kani harnesses for the ring wrap invariant and the
    `pack_entry`/`unpack_entry` round trip.
13. **G5** — wire a bounded scheduled fuzz run with a committed corpus.
14. **Process (inherited, not new):** the two P2 items the Round-33 review
    left open — G1 (the false "never before committed" claim now in
    `CHANGELOG.md:10`) and G5 (Round 33 never touched
    `docs/perf/OPEN_ITEMS.md`, so the Round-32 review's F4–F11 and the
    `R30_7` CSV-name mismatch are tracked nowhere durable) — should be closed
    before a release, because `CHANGELOG.md` is the durable record a release
    announcement is built from.

---

## 8. Caveats on this audit

- I read the code, not just the docstrings, for every tier-1 seam and every
  tier-2 site reachable from the `GlobalAlloc` entry points. I did **not**
  exhaustively read `crates/vmem`, `crates/numa`, or `crates/proc-memstat`
  line by line — their OS-FFI seams were spot-checked only.
- F-1 is the only finding for which I built a full interleaving. F-2 is
  explicitly a hypothesis with **no repro constructed**, and I say so in the
  finding. F-7 and F-8 are structural-consistency findings with **no
  currently-reachable panic identified**; they are listed as hardening, not
  as bugs.
- I did not run miri, loom, kani, TSan, or the aarch64 cross matrix locally
  (no toolchain/host for several of them); the coverage claims in §4 are read
  off `.github/workflows/ci.yml` and `kani.yml` plus the test sources, not
  from executing them.
- `size_of::<HeapCore>() ≈ 7 KB` (F-6) is **inferred** from the in-tree
  `-Zprint-type-sizes` field offsets recorded in `src/registry/heap_slot.rs`,
  not measured by me.
