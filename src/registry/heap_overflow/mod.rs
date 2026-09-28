//! [`HeapOverflow`] — a per-heap MPSC second-chance ring plus R2-09's
//! intrusive spill for legal frees that outlive both bounded rings.
//! The historical RAD-4 owner-starvation case lost 744/1000 blocks in its
//! measured burst; the spill now retains frees beyond the ring capacities.
//!
//! [`RemoteFreeRing`]: crate::alloc_core::remote_free_ring::RemoteFreeRing
//! [`HeapCore`]: super::heap_core::HeapCore
//!
//! ## Why this exists — the gap RAD-4 left open, closed here
//!
//! `RemoteFreeRing` is *per-segment* (`RING_CAP = 256`) and only the segment's
//! OWNER may drain it (single-writer `BinTable`). RAD-4's bounded retry
//! (`RING_PUSH_RETRY_SPINS`) buys a producer time for the owner to drain
//! *while it is spinning*, but if the owner performs ZERO `alloc()` calls for
//! the producer's entire retry window — the deliberately pathological shape
//! `remote_fanin_owner_starved_residual_is_bounded` exercises — there is
//! nothing for the retry to wait on, and the original design's only recourse
//! was the bounded leak (drop the block; `HeapCore`'s historical module doc
//! walks the rejected full-durability designs: writing into the block's
//! own bytes was previously rejected without an
//! ownership/publication proof (the H1-class UAF risk); R2-09 supplies that
//! proof for exclusively transferred, still-live spill blocks;
//! `Box::new` reopens the `#[global_allocator]` reentrancy hazard; reusing
//! `deferred_next` widens the M-7 dormant reactivation hazard).
//!
//! This module is the FOURTH design this task's investigation considered (see
//! `docs/perf/IAI_BASELINE.md`'s "RAD-4b" entry for the full comparison
//! against the three candidates the task brief posed — real backpressure/
//! blocking `dealloc`, a slot-resident buffer keyed by segment pointer +
//! provenance-exposed header stamp, and properly tagging `deferred_next`).
//! It keeps option 2's SHAPE (slot-resident, pre-reserved at claim time, no
//! `Box`, no block-byte writes in the ring tiers) but resolves the "how does a remote producer
//! find the owning `HeapSlot`" question WITHOUT any new `SegmentHeader`
//! field or provenance-exposed pointer: every segment ALREADY carries its
//! owner's heap-slot **index** in `owner_state` (`unpack_owner_id`, stamped
//! by `HeapCore::stamp_segment_owner` on every alloc — the same field
//! `dbg_owner_id_for` and the M-7 audit note already document as the 12.3
//! "owner stamping" mechanism). A remote producer that already reads
//! `owner_state` (it does, for the M-8/M-9-adjacent ownership checks
//! elsewhere) can resolve the owning `&'static HeapSlot` with a single call
//! into the process-`'static` registry — `bootstrap::ensure().slot(owner_id)`
//! (R6-OPT-P0-2: the slot array is chunked and lazily materialised; `slot()`
//! is the single accessor that resolves an index, materialising the owning
//! chunk first if needed) — a **safe** call, not a raw-pointer `container_of`
//! trick and not a new provenance surface. `SegmentHeader` is untouched (zero
//! layout risk to that already-heavily-audited struct); `owner_thread_free`'s
//! existing provenance-exposure machinery is not reused or extended.
//!
//! ## What this queue IS and IS NOT
//!
//! - IS: a bounded (`HEAP_OVERFLOW_CAP` entries) MPSC ring, structurally
//!   IDENTICAL in protocol to [`RemoteFreeRing`] (the same Vyukov-style
//!   CAS-reserve push / single-consumer drain — a proven, loom-verified
//!   shape reused rather than reinvented), but built from plain safe-Rust
//!   `AtomicUsize`/`AtomicU32` array fields on [`HeapSlot`] instead of a
//!   byte-offset view over segment metadata (there is no segment to carve
//!   bytes from here — the slot is an ordinary `'static` Rust struct).
//! - IS per-HEAP (one ring absorbs overflow from ANY of the heap's owned
//!   segments — a heap may own many), unlike `RemoteFreeRing` (one ring per
//!   segment). Each entry therefore carries the segment `base` alongside the
//!   packed `(offset, class)` word `RemoteFreeRing` already produces at its
//!   call sites (`HeapCore::dealloc_foreign_slow` computes `packed` before
//!   ever touching the ring — this queue reuses that SAME value verbatim).
//! - IS bounded as a ring. R2-09 adds an intrusive third tier when both
//!   rings saturate; the number of pending notes is bounded by the number
//!   of still-live small blocks, not by `HEAP_OVERFLOW_CAP`.
//! - The two rings never write freed-block bytes. Only the third tier does:
//!   its note occupies the first 16 bytes of the exclusively transferred
//!   block until the owner reclaims it.
//!
//! ## R2-09: lossless intrusive spill beyond both fixed rings
//!
//! A legal small-block free transfers exclusive use of that block to the
//! allocator exactly once. A duplicate free is outside the unsafe caller's
//! contract even if the segment remains mapped: there is no atomic per-block
//! pending claim. Concurrent duplicate spill writes can race, and a later
//! duplicate publication of the same block can self-link the stack. The
//! magazine/bitmap oracles and `hardened` generation check do not make that
//! misuse safe. When both fixed rings are full, the producer writes a
//! `SpillNode { next: null, packed, ready: 0 }` into the block itself, then
//! atomically swaps its address into this heap slot's `spill_head`. The swap
//! returns the *actual* preceding head pointer with its current provenance;
//! only then does the producer write that pointer to `next` and Release-store
//! `ready = 1`. The swap is the ownership-transfer/stack-order linearization
//! point; the ready store completes publication. A consumer Acquire-checks
//! ready before reading next, so it cannot pop a node while its producer is
//! still completing the link. A newer producer can prepend during this gap,
//! but the sole consumer cannot pass the unready node to pop an older head.
//! Thus even if a preceding node's virtual address was reused earlier, the
//! stored link came from the swap's actual return value, not an address-equal
//! stale CAS expectation. The token-holding consumer CAS-pops each ready node
//! before reclaiming it; no node bytes are accessed after reclaim.
//!
//! Each pending note keeps its block logically live in the segment's
//! owner-only `live_count`. A segment therefore cannot decommit/release while
//! *any* of its blocks are still pending in either ring or the spill, or
//! while a producer is preparing an unpublished note. Once the last note is
//! consumed, `drain_heap_overflow` defers empty-segment finalization until
//! the entire drain pass returns. Recycle only hands the whole `HeapCore`
//! and its stable slot-resident `HeapOverflow` to the next claimant; it does
//! not destroy either or reset `spill_head`. Paused and exited owners have
//! the same lossless publication path.
//! The spill block is never in an owner free list or magazine while a
//! producer writes it; only after the pop/reclaim does the owner write its
//! normal free-list link. Thus this exception to the older "remote frees
//! never write block bodies" description does not race those owner writes.
//!
//! The spill allocates no metadata: a pending node consumes bytes already
//! owned by its freed block. Its memory use is at most one node per
//! outstanding legal small allocation, with no OS call, `GlobalAlloc`
//! recursion, OOM branch, or producer backpressure. The fixed ring sidecar
//! may still fail to materialize on OS OOM, but that simply selects spill
//! earlier; it can no longer discard the free. The owner drains at most
//! `HEAP_OVERFLOW_CAP` spill nodes per pass so continuous producers cannot
//! make one drain call unbounded; remaining notes retrigger the next drain.
//! Production reclaim callbacks must not unwind after a spill pop: a node
//! may be decommitted during reclaim and cannot be requeued on panic.
//! A panic through `GlobalAlloc` is outside its contract.
//!
//! The historical bounded-capacity argument below describes the *ring*,
//! not the complete post-R2-09 delivery protocol.
//!
//! A fixed ring alone could only promise no loss for bursts fitting its
//! capacity. Its former terminal leak is now replaced by the intrusive
//! spill above; no larger fixed constant is being treated as a proof.
//!
//! ## R6-OPT-P0-2 (round 2) — two-tier storage: inline emergency + lazy sidecar
//!
//! Round 1 chunked the REGISTRY's slot array (`registry_chunk.rs`), cutting
//! the first-heap-claim commit floor from ~125 MiB to ~6 MiB. The remaining
//! ~6 MiB is one materialised chunk of `CHUNK_SLOTS` (64) `HeapSlot`s, and
//! this ring — inline in EVERY `HeapSlot` at the full `HEAP_OVERFLOW_CAP =
//! 2048` (24 KiB/slot) — was the dominant remaining cost: 64 slots × 24 KiB
//! ≈ 1.5 MiB of that ~6 MiB was this one field alone, paid by every process
//! that claims even a single heap, whether or not it EVER overflows.
//!
//! The fix splits storage into two tiers sharing ONE logical index space
//! (`0..HEAP_OVERFLOW_CAP`), with a SINGLE `tail`/`head` cursor pair spanning
//! both:
//!
//! 1. **Inline "emergency" tier** (`INLINE_CAP` entries, `bases`/`packed`
//!    below) — ALWAYS present on `HeapSlot`, exactly like the pre-round-2
//!    array but much smaller. Indices `0..INLINE_CAP`.
//! 2. **Lazily-materialised sidecar** (`sidecar: AtomicPtr<HeapOverflowSidecar>`,
//!    null until first genuine overflow past the inline tier) covering
//!    indices `INLINE_CAP..HEAP_OVERFLOW_CAP`. Reserved via
//!    `aligned_vmem::reserve_aligned` — the SAME M5-clean direct-syscall path
//!    `bootstrap` already uses for the registry's chunks — via a THIRD
//!    instance of round 1's CAS(null→SENTINEL)→reserve→publish(Release)/
//!    spin(Acquire) protocol, this time keyed on ONE ring's sidecar pointer
//!    instead of a chunk-array slot. See [`super::bootstrap::ensure_overflow_sidecar`]
//!    for the materialisation function and the module doc there for why this
//!    lives in `bootstrap` rather than here (the unsafe-seam placement
//!    decision).
//!
//! **`INLINE_CAP = 64`** — sized against three concrete anchors, mirroring
//! `HEAP_OVERFLOW_CAP`'s own "sized to a judge, not merely large" discipline:
//! (a) matches `registry_chunk::CHUNK_SLOTS` (64), the codebase's own
//! established "one lazily-materialised unit" scale, so the inline tier reads
//! as "one chunk's worth of emergency capacity" rather than an arbitrary
//! number; (b) comfortably covers `tests/miri_heap_overflow_unit.rs`'s own
//! workload (32 total pushes across two producers, asserted to never
//! overflow) with 2x headroom, so that harness's miri run never needs to
//! exercise the sidecar path at all; (c) absorbs a burst large enough that
//! the OWNER's own opportunistic drain (which runs on every one of the
//! owner's `alloc()` slow-path calls — magazine-miss refill, segment scan —
//! not merely "eventually") gets many chances to drain the inline tier before
//! a sustained producer population could ever force sidecar materialisation:
//! at `INLINE_CAP = 64` entries, a producer population needs to sustain a
//! burst 64 entries deep with the owner making LITERALLY ZERO alloc calls in
//! that whole window before the sidecar is ever touched — the same
//! "genuinely pathological, not merely busy" bar `HEAP_OVERFLOW_CAP`'s own
//! `2048` is calibrated against, one tier down. Once past `INLINE_CAP`, the
//! sidecar's remaining `HEAP_OVERFLOW_CAP - INLINE_CAP = 1984` entries still
//! deliver this ring's full original capacity — round 2 does not shrink the
//! WORST-CASE bound `remote_fanin_owner_starved_residual_is_bounded` judges,
//! only defers most of its cost behind first-touch.
//!
//! **RSS discipline, one level deeper.** Exactly as `HEAP_OVERFLOW_CAP`'s own
//! doc documents for the whole (pre-round-2) array: a slot that never
//! overflows never writes a byte of the inline tier (all-zero OS-provided
//! state), and now ALSO never touches the sidecar `AtomicPtr` beyond its
//! zero-initial `null` (a single word, not a 96 KiB array) — the "never
//! first-touched, never paid for" discipline round 1 already established for
//! the slot array applies here one level down, inside the ring itself.
//!
//! ## The wedge hazard — why a naive lazy sidecar would be UNSOUND
//!
//! `push`'s protocol is CAS-reserve a tail index FIRST (an irreversible
//! ratchet — there is no "give back my reservation"), THEN publish
//! `(base, packed)` into that index's slot. If a producer won the CAS
//! reservation for an index `i >= INLINE_CAP` and THEN discovered the sidecar
//! could not be materialised (OS OOM), it would have advanced `tail` past
//! index `i` with NO way to ever publish into it. `drain`'s stop condition —
//! "if this index's `base` is still `ENTRY_EMPTY_BASE`, STOP; a later drain
//! will pick it up" — assumes an unpublished slot is a TRANSIENT race (the
//! producer publishes microseconds later), not a PERMANENT gap. An
//! unreachable sidecar makes it permanent: `drain` would wedge at index `i`
//! forever, and EVERY subsequent entry (`i+1, i+2, ...`) — even ones from
//! producers whose own sidecar materialisation attempts SUCCEED — becomes
//! unreachable. It would silently and permanently disable the ring; the
//! third-tier spill does not excuse a stranded ring reservation.
//!
//! **The fix:** never let a producer WIN the tail-CAS reservation for an
//! index it cannot honour. [`push`](Self::push)/[`push_uncounted`](
//! Self::push_uncounted) check, BEFORE attempting the CAS on the
//! currently-observed `t`, whether `t >= INLINE_CAP`; if so they call
//! `ensure_sidecar` FIRST. If that fails (OOM), the push returns `false`
//! immediately — WITHOUT ever attempting the CAS — exactly the same outcome
//! as "the ring is full right now". The caller retries, then publishes to
//! the intrusive spill if necessary. `tail` never
//! advances past an index whose backing store does not exist, so no wedge is
//! possible. See `push`'s doc comment for the exact ordering.

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on; this module is
// deliberately built from PLAIN SAFE-RUST atomics (`AtomicUsize`/`AtomicU32`/
// `AtomicPtr` array/scalar fields on a `'static` struct) precisely so it needs
// NO seam at all — unlike `RemoteFreeRing` (which views raw bytes carved out
// of a dynamically `mmap`'d segment and therefore MUST live in the `node`/`os`
// `unsafe` seam), a `HeapSlot` is an ordinary Rust struct living in the
// process-`'static` registry array, so its fields are reachable through
// ordinary safe references. The sidecar's OS reservation and raw-pointer
// dereference (round 2) live in `bootstrap::overflow_sidecar`'s own
// tier-1 `#![allow(unsafe_code)]` seam instead of a new one here — see
// `super::bootstrap::ensure_overflow_sidecar` and its module doc's
// "unsafe-seam placement" note for why. There is no `#![allow(unsafe_code)]`
// in this file.

mod heap_overflow_impl;
pub use heap_overflow_impl::*;
