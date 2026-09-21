//! GlobalAlloc-face entry points of [`AllocCore`] (mechanical split of the
//! former flat `alloc_core.rs`).
//!
//! This file holds the `impl AllocCore { .. }` block for `alloc`,
//! `alloc_zeroed`, `dealloc`, `realloc`, `safe_payload_read_span`, and the
//! in-place realloc fast-path family. Pure code movement; no behavior
//! changed.

use core::alloc::Layout;

mod realloc_fastpath;

#[cfg(all(feature = "alloc-stats", feature = "virgin-zero-skip"))]
use super::counters::SMALL_ZERO_PASS_CALLS;
#[cfg(feature = "alloc-stats")]
use super::counters::{FOREIGN_OR_UNROUTABLE_FREES, LARGE_ZERO_PASS_CALLS};
#[cfg(feature = "alloc-decommit")]
use super::CachedLarge;
use crate::alloc_core::alloc_core::AllocCore;
use crate::alloc_core::node::Node;
use crate::alloc_core::os;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind};
use crate::alloc_core::size_classes::AllocKind;

impl AllocCore {
    /// Allocate `layout.size()` bytes satisfying `layout.align()`.
    ///
    /// Returns a non-null `*mut u8` on success, or null on OOM. The memory is
    /// **uninitialised** (matching `GlobalAlloc::alloc`); see
    /// [`alloc_zeroed`](Self::alloc_zeroed) for zeroed memory.
    ///
    /// Zero-size layouts are not supported (they violate the `GlobalAlloc`
    /// contract; we round up to `MIN_BLOCK` and serve normally).
    #[must_use]
    #[inline(always)]
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        let align = layout.align();
        match Self::classify(size, align) {
            AllocKind::Small { class_idx } => self.alloc_small(class_idx),
            // `.0` discards the freshness bool — the plain `alloc` path is
            // uninitialised-memory by contract and must NOT observe it; only
            // `alloc_zeroed` below consults the bool. Behaviour is byte-
            // identical to the pre-tuple `alloc_large` call.
            AllocKind::Large => self.alloc_large(size, align).0,
        }
    }

    /// Allocate `layout.size()` bytes of **zeroed** memory.
    ///
    /// # Fresh-reservation skip (task #221 / R8-8)
    ///
    /// For a Large-classified request, consults `alloc_large`'s freshness
    /// signal: a genuinely fresh OS reservation is already zero-filled by the
    /// OS (Windows `VirtualAlloc` MEM_COMMIT / Unix zero-filled `mmap`), so the
    /// explicit `Node::zero` pass is SKIPPED — this is the win for large
    /// calloc-heavy requests. A `large_cache` HIT (a reused segment that may
    /// hold the prior occupant's bytes) is NOT fresh and is zeroed explicitly.
    /// Small-classified requests are always zeroed explicitly (the small path
    /// is out of scope for the freshness skip — see the task header) UNLESS
    /// the opt-in `virgin-zero-skip` feature (R12-10, task #261) is enabled,
    /// in which case the identical freshness-skip discipline applies to a
    /// genuinely virgin (never-before-served) bump-carved block — see
    /// [`alloc_small_with_virgin`](Self::alloc_small_with_virgin)'s doc for
    /// the exact virginity predicate. A free-list-served (reused) block is
    /// NEVER treated as virgin and is always zeroed explicitly, exactly as
    /// before this feature existed.
    #[must_use]
    pub fn alloc_zeroed(&mut self, layout: Layout) -> *mut u8 {
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        let align = layout.align();
        match Self::classify(size, align) {
            AllocKind::Small { class_idx } => {
                #[cfg(feature = "virgin-zero-skip")]
                {
                    let (ptr, is_virgin) = self.alloc_small_with_virgin(class_idx);
                    if !ptr.is_null() && !is_virgin {
                        #[cfg(feature = "alloc-stats")]
                        SMALL_ZERO_PASS_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        Node::zero(ptr, size);
                    }
                    ptr
                }
                #[cfg(not(feature = "virgin-zero-skip"))]
                {
                    let ptr = self.alloc_small(class_idx);
                    if !ptr.is_null() {
                        Node::zero(ptr, size);
                    }
                    ptr
                }
            }
            AllocKind::Large => {
                let (ptr, is_fresh) = self.alloc_large(size, align);
                if !ptr.is_null() && !is_fresh {
                    #[cfg(feature = "alloc-stats")]
                    LARGE_ZERO_PASS_CALLS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    Node::zero(ptr, size);
                }
                ptr
            }
        }
    }

    /// Deallocate memory previously returned by [`alloc`](Self::alloc) (or
    /// `alloc_zeroed`/`realloc`).
    ///
    /// This is an **`unsafe fn`**: it trusts its caller to honour the
    /// [`GlobalAlloc::dealloc`](crate::global::SeferAlloc)-shaped contract (see
    /// `# Safety`). The crate's former posture was a *safe* `pub fn` with an
    /// M2 "defensive-free" guard (matching `free()` in mimalloc/glibc); that
    /// posture was **reversed in R6-MS-1/2** after the round5
    /// `memory_safety_review` produced concrete safe-Rust counterexamples
    /// proving the defensive checks insufficient — a same-class in-place
    /// realloc that *resurrects* a freed block (two live allocations at one
    /// address), a fully-overlapping `copy_nonoverlapping(p, p, n)` in the
    /// realloc move leg, and an interior `dealloc` of a Large segment
    /// releasing the whole reservation of a still-live neighbour. Marking the
    /// entry point `unsafe fn` makes a contract violation *documented caller
    /// UB* rather than an unsound *safe* API. The M2 defensive paths
    /// (foreign-pointer no-op, bitmap guard) are **retained as
    /// defence-in-depth** — they no longer carry the soundness argument, but
    /// they still make many accidental misuses benign at runtime.
    ///
    /// See `docs/agent_reviews_round5/memory_safety_review.md` (R5-MS-1/MS-2)
    /// for the full exploit catalogue and `CHANGELOG.md` (R6-MS-1/2) for the
    /// migration.
    ///
    /// **Phase 13.3 — arithmetic own-thread free.** The hot path is now pure
    /// arithmetic + (at most) one field-specific header byte read, NOT a
    /// full-struct `SegmentHeader::read_at`. Specifically:
    ///   - `segment_base_of(ptr)` — one mask (already the case).
    ///   - `self.table.contains_base(base)` — the foreign-pointer guard (this
    ///     is the load-bearing defence-in-depth check, NOT the `magic` word:
    ///     a foreign pointer's computed base is simply not in our registry,
    ///     so we never touch its bytes).
    ///   - `SegmentHeader::kind_at(base)` — ONE byte field read (via
    ///     `offset_of!`) to distinguish Large from Small/Primordial. This is
    ///     the minimum read necessary: Large blocks are freed by marking the
    ///     segment (no class free list), Small/Primordial go to the BinTable;
    ///     without distinguishing them we'd misroute. `kind` is written once
    ///     at segment init and immutable thereafter, so this byte read cannot
    ///     race an owner write on the disjoint `bump` field (the §11
    ///     root-cause analysis).
    ///   - the size class is derived from the caller-supplied `Layout` via
    ///     `Self::classify` — pure arithmetic, no `page_map` lookup (§13:
    ///     `page_map` is unreliable for mixed-class pages, and own-thread
    ///     free HAS the `Layout`, so deriving from it is both cheaper AND
    ///     correct).
    ///
    /// The `SEGMENT_MAGIC` full-struct sanity check is intentionally absent
    /// here: it lives ONLY on the defensive cross-thread routing path
    /// (`HeapCore::dealloc_routing` under `alloc-xthread`), where a foreign
    /// pointer could in principle resolve to a registered-but-not-ours base.
    /// On the trusted own-thread path, `contains_base` is the sole guard and
    /// the `Layout` is authoritative for the class — a full header load would
    /// be a dependent load on the free critical path with no correctness gain.
    ///
    /// # Safety
    ///
    /// The caller must uphold the [`GlobalAlloc::dealloc`] contract for `ptr`
    /// and `layout`. Concretely:
    ///
    /// - `ptr` is **null** OR the exact **start** pointer of a currently-LIVE
    ///   allocation owned by *this* `AllocCore`, returned by a prior
    ///   [`alloc`](Self::alloc)/[`alloc_zeroed`](Self::alloc_zeroed)/
    ///   [`realloc`](Self::realloc). It MUST NOT be an interior pointer
    ///   (`base + interior_offset`): the foreign/interior defences are
    ///   best-effort, not a soundness guarantee, and an interior Large free
    ///   would release the whole reservation of a still-live neighbour.
    /// - `layout` exactly matches the layout the allocation was made with.
    /// - The allocation is freed **at most once**: a double-free, and any
    ///   re-issue of `ptr` after this call (before a later `alloc` happens to
    ///   reuse its address for a new owner), is UB.
    /// - `ptr` is not a foreign / already-released-unmapped base (a pointer
    ///   from another allocator or a segment whose OS reservation has been
    ///   released).
    ///
    /// Null `ptr` is always safe (early return). The M2 defensive paths
    /// (foreign-pointer no-op, bitmap guard) make several of these accidental
    /// violations benign *at runtime*, but they are NOT a substitute for
    /// honouring the contract — a violation this method cannot detect is UB.
    #[inline]
    #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let base = os::segment_base_of_ptr(ptr);
        // Foreign-pointer check: if the computed segment base is NOT one of our
        // registered segments, this pointer is not one of ours — no-op (do not
        // touch foreign memory, do not even read a header that may be unmapped).
        if !self.table.contains_base(base) {
            // Review finding 2.3: make the drop OBSERVABLE. Without
            // `alloc-xthread` this branch is the sole guard, and a cross-thread
            // free lands here as a PERMANENT leak — the misconfiguration
            // signature this counter exists to expose (see
            // `FOREIGN_OR_UNROUTABLE_FREES`). Gated behind `alloc-stats` so the
            // free hot path pays nothing by default, matching the crate's other
            // per-event stat counters. Relaxed: diagnostic only.
            #[cfg(feature = "alloc-stats")]
            FOREIGN_OR_UNROUTABLE_FREES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return;
        }
        // Field-specific `kind` read (Phase 13.3): a single byte at its
        // `offset_of!` offset, NOT a full-struct `read_at`. Distinguishes
        // Large (free = mark segment) from Small/Primordial (free = push to
        // BinTable). `kind` is immutable after init, so this byte read is
        // race-free against the owner's disjoint `bump` writes.
        match SegmentHeader::kind_at(base) {
            SegmentKind::Large => {
                // Large/huge: the segment is being freed. The full header read
                // here is on the cold Large path (one allocation per segment,
                // rare), so the dependent-load cost does not matter.
                //
                // OPT-E (alloc-decommit): if the segment is small enough to
                // cache AND a free slot exists, decommit its payload pages and
                // deposit it into the large_cache so the next alloc_large of a
                // compatible size can reuse it without an OS round-trip.
                //
                // Without alloc-decommit (or when the cache admission is
                // declined): the OS reservation is released EAGERLY, right
                // here — NOT deferred to `Drop` (task #125; see the release
                // branches below for the full rationale). `unregister(base)`
                // runs first so `Drop`'s `table.bases()` walk never sees
                // `base` again — no double-free of the reservation.
                //
                // Phase 2: run a lazy decay tick on large free (same cheap
                // Instant check as on the alloc path).
                #[cfg(feature = "alloc-decommit")]
                self.maybe_decay_large_cache();

                let stale = SegmentHeader::read_at(base);

                #[cfg(feature = "alloc-decommit")]
                {
                    // The physical usable span is read from the header's
                    // stable `span_usable` field — NOT recomputed from
                    // `large_size`/`large_align`. Bug #134: on a cache-hit
                    // reuse the header's logical size/align can be smaller
                    // than the segment's actual physical footprint (the OS
                    // reservation is reused as-is for a smaller request), so
                    // recomputing "usable size" from size/align here
                    // under-reports the true span and corrupts the
                    // large-cache byte-budget accounting. `span_usable` is
                    // set once at the segment's original OS reservation and
                    // carried forward verbatim through every cache-hit reuse
                    // (see `SegmentHeader::span_usable` doc). R12-4:
                    // `reserved_capacity` is carried forward the same way
                    // (never recomputed) — see `CachedLarge::reserved_capacity`'s
                    // doc. The field is present in every build's layout
                    // (inert, equal to `span_usable`, when the feature is
                    // off), so this read needs no `#[cfg]` split.
                    let usable_size = stale.span_usable;
                    let reserved_capacity = stale.reserved_capacity;

                    // Phase 1 large-cache admission: byte-budget enforcement.
                    //
                    // Strategy:
                    //   1. Find a free slot (None). If none is free, try FIFO eviction.
                    //   2. Check that depositing this span would not exceed the budget
                    //      (if one is set). If the budget would overflow, evict the
                    //      oldest occupied slot to make room. If eviction still can't
                    //      satisfy the budget (budget < usable_size), skip caching.
                    //   3. Deposit into the (now-free) slot.
                    //
                    // FIFO definition (task D1): the "oldest" slot is the
                    // occupied one with the smallest insertion `seq`, found via
                    // `oldest_occupied_slot` (a seq-based min-by scan) — NOT slot
                    // index 0. Each deposit stamps `large_cache_seq` into
                    // `CachedLarge::seq`, so eviction picks the true FIFO-oldest
                    // entry regardless of slot order. This holds for any
                    // LARGE_CACHE_SLOTS (currently 8), not just the old 2-slot
                    // minimal implementation that happened to fill slots in order.
                    // Two independent admission constraints: (1) there must be a
                    // free slot, (2) the byte-budget (if set) must accommodate
                    // `usable_size`. Either failing means we evict the oldest and
                    // retry. Bug #94 history: an earlier version short-circuited
                    // `try_evict_to_fit` to `true` when budget=None and missed
                    // the "slots full" case entirely, silently releasing every
                    // span beyond the first two to the OS. The loop below treats
                    // both constraints uniformly.
                    //
                    // R13-7 (task #277): "free slot" now searches the COMBINED
                    // base+extension index space via `large_cache_find_free_slot`
                    // (base 8 first, then the lazily-materialised
                    // `large-cache-extended` sidecar once the base is full) —
                    // the byte-budget check is UNCHANGED and remains the
                    // primary control regardless of how many slots exist.
                    //
                    // R14-5 (task #290, review finding @fm P3): budget
                    // feasibility is checked BEFORE ever calling
                    // `large_cache_find_free_slot` — a deposit this large can
                    // NEVER fit under the configured budget (even against a
                    // fully-evicted cache), so there is no point paying for a
                    // sidecar materialisation (a real OS page reservation) to
                    // go looking for a free slot the budget will reject
                    // regardless. See `large_cache_deposit_budget_infeasible`'s
                    // doc for the exact rationale and its scope (this is a
                    // single-deposit-vs-budget check, not a full feasibility
                    // predictor for the eviction loop below).
                    let mut admitted: Option<usize> = None;
                    if !self.large_cache_deposit_budget_infeasible(usable_size) {
                        loop {
                            let free_slot = self.large_cache_find_free_slot();
                            let budget_ok = self.large_cache_budget_bytes.is_none_or(|budget| {
                                self.large_cache_used_bytes + usable_size <= budget
                            });
                            if let Some(idx) = free_slot {
                                if budget_ok {
                                    admitted = Some(idx);
                                    break;
                                }
                            }
                            // Either no free slot, or budget would overflow → evict
                            // the oldest entry and retry. If the cache is already
                            // empty there is nothing more we can do.
                            if !self.evict_one_oldest() {
                                break;
                            }
                        }
                    }

                    if let Some(slot_idx) = admitted {
                        // We keep the pages COMMITTED in the cache (no decommit
                        // on deposit). On Windows, `VirtualAlloc(MEM_DECOMMIT)`
                        // followed immediately by `VirtualAlloc(MEM_COMMIT)` on
                        // the next cache hit costs more than just leaving the
                        // pages mapped — the entire purpose of the cache is to
                        // amortise the OS round-trip cost. Decommitting here
                        // would reduce RSS by the usable payload size, but at the
                        // cost of an expensive recommit on every hit, negating
                        // the speedup. We intentionally trade RSS for latency:
                        // a cached large segment keeps its pages warm between uses.
                        //
                        // NULL the table slot WITHOUT releasing the OS reservation.
                        // The cached entry owns the reservation; AllocCore::drop
                        // releases it explicitly from the large_cache array.
                        self.table.unregister(base);
                        // Zero the magic so that if something reads the header
                        // while it's in the cache, it won't be confused as a
                        // live registered segment.
                        //
                        // UBFIX-6 (M-2, docs/reviews/2026-07-10-ub-audit-final-
                        // synthesis.md): this used to be `hdr_zero = stale;
                        // hdr_zero.magic = 0; Node::write_struct(base, hdr_zero)`
                        // — a non-atomic FULL-STRUCT write that races with
                        // `SegmentHeader::magic_at`/`kind_at`/`large_size_at`/
                        // `span_usable_at` (remote defensive field reads that can
                        // observe a live header concurrently with this owner
                        // write under a stale/duplicate remote free — misuse of
                        // the `GlobalAlloc` contract the defensive reads exist to
                        // survive without UB). `stale` is a fresh `read_at(base)`
                        // taken just above, so every OTHER field of `hdr_zero`
                        // is byte-identical to what is already in memory — the
                        // full-struct write's only REAL effect was zeroing
                        // `magic`. Restoring this file's own §11 discipline
                        // ("remote-readable field ⇒ atomic single-word access",
                        // the same pattern `SegmentMeta::owner_state_atomic`
                        // already uses for cross-thread owner-state reads): write only the
                        // `magic` field, through an `&AtomicU32` view at its
                        // `offset_of!` offset, so a concurrent remote
                        // `magic_at`/`kind_at`/`large_size_at`/`span_usable_at`
                        // read never races a torn/non-atomic store — those other
                        // three fields are untouched here, so no write to them is
                        // needed at all.
                        let magic_off = core::mem::offset_of!(SegmentHeader, magic);
                        Node::atomic_u32_at(base, magic_off)
                            .store(0, core::sync::atomic::Ordering::Release);
                        // Deposit into cache and update the byte-budget counter.
                        let seq = self.large_cache_seq;
                        self.large_cache_seq = self.large_cache_seq.wrapping_add(1);
                        self.large_cache_slot_set(
                            slot_idx,
                            CachedLarge {
                                reservation: stale.reservation,
                                reservation_len: stale.reservation_len,
                                base,
                                usable_size,
                                reserved_capacity,
                                seq,
                            },
                        );
                        self.large_cache_used_bytes += usable_size;
                        return;
                    }
                    // Not admitted (no free slot after eviction, or budget too small):
                    // release the OS reservation EAGERLY right now rather than
                    // deferring to `AllocCore::drop` (task #125 / same leak
                    // class as A1/#114). In the Phase 12.5 shard model, the
                    // per-thread `AllocCore` living in a registry slot is
                    // effectively never dropped mid-process (the slot is
                    // recycled between threads, but the `AllocCore` itself
                    // persists) — "defer release to Drop" is therefore a
                    // PERMANENT leak of both the OS reservation and the
                    // `SegmentTable` slot on the own-thread admission-reject
                    // path, eventually exhausting `MAX_SEGMENTS` and forcing
                    // `alloc_large` to return null. `unregister` FIRST (frees
                    // the slot for reuse; mirrors `reclaim_large_segment`'s
                    // ordering), THEN release — Drop's `table.bases()` walk
                    // will no longer see `base`, so there is no double-free.
                    self.table.unregister(base);
                    os::release_segment(stale.reservation, stale.reservation_len);
                }
                #[cfg(not(feature = "alloc-decommit"))]
                {
                    // No large-cache at all: every own-thread large free must
                    // release eagerly for the same reason as the
                    // admission-reject branch above (task #125) — deferring
                    // to `Drop` leaks the reservation AND the `SegmentTable`
                    // slot for the remaining process lifetime.
                    self.table.unregister(base);
                    os::release_segment(stale.reservation, stale.reservation_len);
                }
            }
            SegmentKind::Small | SegmentKind::Primordial => {
                // Derive the class from the caller's `Layout` (pure
                // arithmetic via `SIZE2CLASS`) — NOT from `page_map`. §13 of
                // RACE_DRAIN_RECLAIM.md: `page_map` records only the FIRST
                // class to touch a page, so it returns the wrong class for
                // any later block of a different class in the same page. The
                // own-thread freer HAS the original `Layout`, so classifying
                // from it is both cheaper (no page_map load) AND correct.
                let size = layout
                    .size()
                    .max(crate::alloc_core::size_classes::MIN_BLOCK);
                let align = layout.align();
                let kind = Self::classify(size, align);
                let class_idx = match kind {
                    AllocKind::Small { class_idx } => class_idx,
                    // Layout mismatch: the original allocation was small but
                    // the dealloc layout classifies as large. This is a
                    // contract violation; no-op (do not corrupt).
                    AllocKind::Large => return,
                };
                self.dealloc_small(base, ptr, class_idx);
            }
            // L-5 (UBFIX-11): `contains_base` already proved `base` is one of
            // OUR registered segments, but the `kind` BYTE at that base has
            // been corrupted to something other than the three legitimate
            // discriminants (0/1/2) — `kind_at`'s strict decode maps that to
            // `Unknown` rather than guessing. Neither the Large branch (which
            // would release/cache the OS reservation) nor the Small/
            // Primordial branch (which would write a BinTable/free-list
            // header into the payload) is safe to run against a segment
            // whose real kind we cannot trust — no-op is the only sound
            // choice: do not touch this segment's payload or reservation at
            // all. Same reject-not-guess posture as the H-1 payload
            // lower-bound guard (UBFIX-3).
            SegmentKind::Unknown => {}
        }
    }

    /// Shrink/grow an allocation in place or by alloc + copy + dealloc.
    ///
    /// Two in-place fast paths are attempted first (shared with
    /// [`try_realloc_inplace_known_base`](Self::try_realloc_inplace_known_base), which [`HeapCore::realloc`](crate::registry::HeapCore)
    /// calls so its alloc leg can route through the magazine-aware
    /// `HeapCore::alloc`):
    ///
    /// **OPT-F — in-place small→small realloc:** when both the old and new
    /// sizes resolve to the SAME size class (`new_class_idx == old_class_idx`),
    /// the block physically fits the new size without any data movement, so we
    /// return the original pointer unchanged: no alloc, no copy, no dealloc.
    /// The block's live-count and alloc-bitmap stay intact. The `==` (not
    /// `<=`) rule is load-bearing — see `realloc_inplace_fast_path`'s comment
    /// and `tests/regression_realloc_cross_class_shrink.rs`.
    ///
    /// **OPT-G — in-place Large→Large realloc:** when the block lives in a
    /// Large segment and the grown size (clamped to `MIN_BLOCK`) still fits
    /// the segment's `span_usable`, we update the header's `large_size` and
    /// return the same pointer. Shrinks fall through to the slow path
    /// (reclaims RSS). The stored size is clamped to `MIN_BLOCK` to stay
    /// symmetric with the alloc path and the #138 cross-thread consistency
    /// check (`large_layout_consistent`).
    ///
    /// On growth the new tail is **uninitialised** (matching `GlobalAlloc`).
    /// Returns null on failure, leaving the old allocation intact. Null `ptr`
    /// returns null without touching state.
    ///
    /// A **foreign pointer** (its computed segment base is not one of ours)
    /// also returns null without touching state, symmetric with
    /// [`dealloc`](Self::dealloc)'s foreign-pointer no-op. This is a
    /// substrate-level (`AllocCore`) entry point with no cross-heap concept:
    /// unlike `HeapCore::realloc` (which has a design-load-bearing foreign-leg
    /// for `alloc-xthread` — a pointer from another live heap in the SAME
    /// process is legitimate there and `dealloc` routes it cross-thread), a
    /// pointer this `AllocCore` does not recognise is never legitimate.
    ///
    /// This is an **`unsafe fn`** (R6-MS-1/2): the move leg's
    /// [`Node::copy_nonoverlapping`](crate::alloc_core::node::Node) reads
    /// `old_layout.size()` bytes out of `ptr`, and trusts the caller's
    /// `old_layout`/`ptr` exactly as `GlobalAlloc::realloc` does. The crate's
    /// former posture was a safe `pub fn` bounded by
    /// [`safe_payload_read_span`](Self::safe_payload_read_span); that was
    /// reversed after the round5 review showed the same-class in-place branch
    /// resurrecting a freed block and the foreign/move legs being reachable
    /// from safe code. See `# Safety` and `CHANGELOG.md` (R6-MS-1/2).
    ///
    /// # Safety
    ///
    /// The caller must uphold the [`GlobalAlloc::realloc`] contract for `ptr`
    /// and `old_layout`:
    ///
    /// - `ptr` is **null** OR the exact **start** pointer of a currently-LIVE
    ///   allocation owned by *this* `AllocCore`, made with a `Layout` whose
    ///   size/align match `old_layout`. It MUST NOT be an interior pointer.
    /// - `old_layout` exactly matches the allocation's layout; in particular
    ///   `old_layout.size()` must not exceed the block's true size (the move
    ///   leg copies that many bytes out of `ptr`).
    /// - On success (`!null` return) the OLD `ptr` is freed by this call — it
    ///   MUST NOT be used or re-freed afterwards. On null return `ptr` is left
    ///   intact and still owned by the caller.
    /// - `ptr` is not a foreign / already-released-unmapped base.
    ///
    /// Null `ptr` is always safe (early return).
    #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
    pub unsafe fn realloc(&mut self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        // OPT-F / OPT-G: try the in-place fast paths first (Large grow-in-span
        // and Small same-class). The detection logic lives in ONE place —
        // `realloc_inplace_fast_path` — shared with `try_realloc_inplace`
        // (which `HeapCore::realloc` calls so its alloc leg can route through
        // the magazine-aware `HeapCore::alloc`). Keeping a single source of
        // truth here closes the unmarked duplication/divergence hazard flagged
        // in the X-arc retrospective (C2): a bugfix applied to one copy but
        // not the other would silently disagree.
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base(base) {
            // Foreign/unregistered pointer: NOT a legitimate move-leg
            // candidate at the substrate level (see doc comment above and F1
            // in the UB/memory-safety audit). Symmetric with `dealloc`'s
            // foreign-pointer no-op — return null instead of falling through
            // to `self.alloc` + `Node::copy_nonoverlapping(ptr, ..)`, which
            // would read `old_layout.size()` bytes from an address we never
            // registered.
            return core::ptr::null_mut();
        }
        if let Some(p) = self.realloc_inplace_fast_path_known_base(base, ptr, old_layout, new_size)
        {
            return p;
        }
        // In-place fast paths did not apply: alloc a fresh block, copy the
        // preserved prefix, and free the old block.
        //
        // R2-1 (soundness): the move leg copies `old_layout.size().min(
        // new_size)` bytes OUT of `ptr`. `contains_base(base)` proved the
        // segment is ours & mapped, but NOT that the block is as large as
        // `old_layout` claims. This is defence-in-depth under the `unsafe fn`
        // contract above (R6-MS-1/2): the signature already trusts the
        // caller's `old_layout`, but a caller bug (e.g. 8 MiB claimed for a
        // 16-byte block) must not turn into an out-of-bounds read here. The
        // write side is always safe (`copy <= new_size <= the fresh
        // allocation`); the unsound half this guards is the READ.
        // Reject (return null, `ptr` untouched) when the claimed old size
        // exceeds the segment's actual committed span.
        if old_layout.size() > AllocCore::safe_payload_read_span(base, ptr) {
            return core::ptr::null_mut();
        }
        let new_layout = match Layout::from_size_align(new_size, old_layout.align()) {
            Ok(l) => l,
            Err(_) => return core::ptr::null_mut(),
        };
        let new_ptr = self.alloc(new_layout);
        if new_ptr.is_null() {
            return core::ptr::null_mut();
        }
        let copy = old_layout.size().min(new_size);
        Node::copy_nonoverlapping(ptr, new_ptr, copy);
        // SAFETY: `ptr` is a live own-segment allocation (proven by
        // `contains_base(base)` above) whose true size bounds the move-leg
        // read (`old_layout.size() <= safe_payload_read_span`), made with
        // `old_layout`; the fresh `new_ptr` holds the copied prefix, so
        // freeing the old block once here completes the contract-honouring
        // realloc move leg.
        unsafe { self.dealloc(ptr, old_layout) };
        new_ptr
    }
}
