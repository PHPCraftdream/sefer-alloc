//! The R29-3/task #504 segment-lifecycle decomposition measurement hooks —
//! `pub unsafe`/`pub` `dbg_decomp_*` fns, all `internals` + `alloc-decommit` +
//! `bench-internals`-gated (mechanical split of the former flat
//! `alloc_core_small_pool.rs`; pure code movement, no behavior changed).

use crate::alloc_core::os::{self, SEGMENT};
use crate::alloc_core::segment_header::Layout as SegLayout;

use crate::alloc_core::alloc_core::AllocCore;
#[cfg(feature = "bench-internals")]
use crate::alloc_core::reserved_small_segment::ReservedSmallSegment;

impl AllocCore {
    // ── R29-3 (task #434) — segment-lifecycle decomposition hooks ─────────────
    //
    // Measurement-only hooks for decomposing one decommit→reserve segment-
    // lifecycle cycle into its component costs (wall-clock, not iai — see the
    // gate report for why Ir is blind to the kernel-time-dominated OS syscalls
    // + page faults this decomposition hinges on). Each hook calls an EXISTING
    // production function verbatim; they exist solely so a
    // `std::time::Instant`-instrumented example (`examples/r29_3_*`) can reach
    // crate-internal functions. All `bench-internals`-gated (no production
    // caller → CLAUDE.md benchmark-hook rule 2). Hooks accepting a raw pointer
    // are `pub unsafe fn` with `# Safety` (rule 1).

    /// R29-3: ONE full reserve→release cycle (`reserve_small_segment_impl` +
    /// `release_or_pool_empty_segment`) without touching the payload.
    /// Measures components (1+2+3): OS reserve+release, SegmentTable
    /// register+recycle, metadata init — everything a reservation-only
    /// overflow tier could avoid.
    ///
    /// R30-1 (task #450): routes through
    /// [`reserve_small_segment_impl`](Self::reserve_small_segment_impl) —
    /// the cursor-free half of `reserve_small_segment` — NOT
    /// `reserve_small_segment` itself. `reserve_small_segment`'s last
    /// statement publishes the freshly reserved segment as the live
    /// `self.small_cur` bump-carve cursor; this hook immediately releases
    /// that same segment via `release_or_pool_empty_segment`, which (once
    /// the hysteresis pool is full) genuinely returns the OS reservation and
    /// recycles the table slot. Going through the cursor-publishing wrapper
    /// left `small_cur` dangling at an unmapped segment with nothing to
    /// restore it — the very next ordinary small alloc on this heap would
    /// read through it (`pop_free(self.small_cur, ...)`), a use-after-free.
    /// See `docs/CORRECTNESS_OPEN_ITEMS.md` item 5 for the full confirmed
    /// trace. `reserve_small_segment_impl` performs the identical OS/table/
    /// metadata work this hook measures, but never touches `small_cur` —
    /// so this hook cannot disturb any other in-flight allocation on the
    /// heap, however many times it is called.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_full_cycle(&mut self) -> bool {
        match self.reserve_small_segment_impl() {
            Some(base) => {
                self.release_or_pool_empty_segment(base);
                true
            }
            None => false,
        }
    }

    /// R29-3: ONE raw OS reserve+release round-trip (`Segment::reserve` +
    /// `os::release_segment`) with NO table bookkeeping and NO metadata
    /// initialization. Isolates component (1): the OS-level VMA setup/teardown
    /// alone.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_os_roundtrip() -> bool {
        let seg = match os::Segment::reserve(SEGMENT) {
            Some(s) => s,
            None => return false,
        };
        let r = seg.reservation();
        let rl = seg.reservation_len();
        core::mem::forget(seg);
        os::release_segment(r.as_ptr(), rl);
        true
    }

    // ── task #504 (F11 step 2) — reserve-vs-commit SPLIT for the Windows
    // decomposition gate ────────────────────────────────────────────────────
    //
    // `dbg_decomp_os_roundtrip` above lumps reserve+commit into ONE timed
    // region — correct for R29-3's Linux question (where the eager path is
    // the only one that exists) but too coarse for F11's Windows question:
    // on Windows `win_reserve_commit` unconditionally issues TWO separate
    // syscalls (`VirtualAlloc(MEM_RESERVE)` then `VirtualAlloc(MEM_COMMIT)`),
    // and knowing their relative cost is exactly what step 2 needs. These two
    // hooks reuse `os::Segment::reserve_lazy_for_measurement` (reserve the
    // full segment, commit only 1 page up front, via `aligned_vmem::
    // reserve_aligned_lazy`) + `os::commit_pages_for_measurement` (commit the
    // REMAINING pages via `aligned_vmem::commit_range`) — the SAME crate-level
    // `lazy-commit` primitive the opt-in `primordial-lazy-commit`/
    // `small-segment-lazy-commit` POLICY features already call in production,
    // just driven directly by a measurement hook instead of by a policy
    // decision. Both are gated on `bench-internals` alone (forwarding
    // `aligned-vmem/lazy-commit`, NOT any sefer-level lazy-commit policy
    // feature — see `bench-internals`'s own `Cargo.toml` doc), so a plain
    // `production` build never partially-commits a segment via this path.
    //
    // Deliberately raw `os::Segment`-based, NOT `ReservedSmallSegment` — like
    // `dbg_decomp_os_roundtrip` above (NO table bookkeeping, NO metadata
    // init, NO owner-binding), these two hooks isolate PURE OS-level cost.
    // `ReservedSmallSegment` exists to guard against a cross-`AllocCore`
    // release once a segment participates in `self.small_cur`/pool/table
    // state (`dbg_decomp_reserve_and_keep`'s contract); these hooks never
    // publish the segment anywhere, so the caller is trusted to pair one
    // `dbg_decomp_win_reserve_only` with exactly one
    // `dbg_decomp_win_commit_only` and one `dbg_decomp_win_release_only`,
    // the same "measurement code, not production metadata" trust level
    // `dbg_decomp_os_roundtrip` already has.
    //
    // On Unix/miri, `aligned_vmem::reserve_aligned_lazy` falls back to the
    // eager fully-committed path and `commit_range` is a no-op (both crate-
    // documented) — so `dbg_decomp_win_commit_only` measures ~0 ns there,
    // which is the expected, honestly-reported cross-platform behavior, not
    // a bug: there is no separate commit syscall to time on Unix.

    /// task #504 (F11 step 2): reserve a `SEGMENT`-sized, `SEGMENT`-aligned
    /// span with only the FIRST RUNTIME page committed. On Windows this is exactly
    /// `VirtualAlloc(MEM_RESERVE)` (over-reserve + trim) followed by ONE
    /// `VirtualAlloc(MEM_COMMIT, len=page_size())` — the same two-call shape
    /// `win_reserve_commit` always takes, except the commit length here is
    /// deliberately tiny so [`dbg_decomp_win_commit_only`] below can
    /// separately time committing the (large) remainder. (Task #1074: the
    /// committed prefix is a RUNTIME `aligned_vmem::page_size()` multiple,
    /// not the compile-time `PAGE` — the vmem lazy contract rejects 4
    /// KiB-only multiples on 16/64 KiB-page hosts.) Returns
    /// `(base, reservation_ptr, reservation_len)` — the caller MUST later
    /// release via [`dbg_decomp_win_release_only`], passing back the SAME
    /// `(reservation_ptr, reservation_len)` pair.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_win_reserve_only() -> Option<(*mut u8, *mut u8, usize)> {
        let seg = os::Segment::reserve_lazy_for_measurement(aligned_vmem::page_size())?;
        let base = seg.as_ptr();
        let r = seg.reservation();
        let rl = seg.reservation_len();
        core::mem::forget(seg);
        Some((base, r.as_ptr(), rl))
    }

    /// task #504 (F11 step 2): commit the remaining `[page_size(), SEGMENT)` range
    /// of a segment previously reserved via [`dbg_decomp_win_reserve_only`]
    /// (which left only the first RUNTIME page committed). On Windows this is
    /// exactly ONE `VirtualAlloc(MEM_COMMIT, len=SEGMENT-page_size())` call —
    /// isolating that syscall's cost alone, with NO reserve and NO
    /// first-touch page-fault cost mixed in (unlike
    /// [`dbg_decomp_os_roundtrip`], which lumps reserve+commit, or
    /// Measurement B's decommit/recommit/re-touch loop, which mixes commit
    /// with faulting). On Unix/miri this is a documented no-op
    /// (`aligned_vmem::commit_range`'s own fallback) — expected to measure
    /// ~0 ns there, honestly reflecting that Unix has no separate commit
    /// syscall to pay.
    ///
    /// Returns `true` if the range is now committed, `false` on genuine OS
    /// refusal (commit-charge exhaustion).
    ///
    /// # Safety
    ///
    /// `base` MUST be the `base` returned by a [`dbg_decomp_win_reserve_only`]
    /// call whose `[page_size(), SEGMENT)` range is still uncommitted (not yet
    /// committed by a prior call to this same hook), and must not have been
    /// released yet.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    #[allow(unsafe_code)] // task #504: unsafe fn boundary, mirrors dbg_decomp_recommit_payload.
    pub unsafe fn dbg_decomp_win_commit_only(base: *mut u8) -> bool {
        // SAFETY: forwarded from this function's own `# Safety` contract —
        // `base`'s `[page_size(), SEGMENT)` range is within the live reservation and
        // currently reserved-but-uncommitted, matching `commit_pages_for_
        // measurement`'s own contract.
        unsafe { os::commit_pages_for_measurement(base, aligned_vmem::page_size(), SEGMENT) }
    }

    /// task #504 (F11 step 2): release a segment reserved via
    /// [`dbg_decomp_win_reserve_only`] — thin wrapper over
    /// [`os::release_segment`], mirroring [`dbg_decomp_os_roundtrip`]'s own
    /// release call.
    ///
    /// # Safety
    ///
    /// `(reservation_ptr, reservation_len)` MUST be the pair returned by a
    /// [`dbg_decomp_win_reserve_only`] call not yet released.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // task #504: unsafe fn boundary, forwarded contract.
    pub unsafe fn dbg_decomp_win_release_only(reservation_ptr: *mut u8, reservation_len: usize) {
        os::release_segment(reservation_ptr, reservation_len);
    }

    /// R29-3: reserve a small segment and return a typed handle so the
    /// caller can measure first-touch page-fault cost on the payload. The
    /// caller MUST later release it via
    /// [`dbg_decomp_release`](Self::dbg_decomp_release).
    ///
    /// R30-1 (task #450): routes through
    /// [`reserve_small_segment_impl`](Self::reserve_small_segment_impl),
    /// NOT `reserve_small_segment` — same reasoning as
    /// [`dbg_decomp_full_cycle`](Self::dbg_decomp_full_cycle)'s doc comment.
    /// This hook's own paired release
    /// ([`dbg_decomp_release`](Self::dbg_decomp_release)) can genuinely
    /// release the OS reservation; a version of this hook that published
    /// `self.small_cur` first would leave it dangling with no restore point
    /// once the paired release fires.
    ///
    /// R31-4 (task #467): returns [`ReservedSmallSegment`] instead of a
    /// bare `*mut u8` — see that type's module doc
    /// (`reserved_small_segment.rs`) for why. Same underlying reservation
    /// mechanism as before; only the return type changed.
    ///
    /// R31-15 (task #486): the returned handle is now stamped with THIS
    /// `AllocCore`'s `dbg_reservation_owner_id`, so the paired
    /// [`dbg_decomp_release`](Self::dbg_decomp_release) can reject a
    /// cross-core release. See `reserved_small_segment.rs`'s module doc,
    /// "Owner-binding" section, for the full rationale.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_reserve_and_keep(&mut self) -> Option<ReservedSmallSegment> {
        let owner_id = self.dbg_reservation_owner_id;
        self.reserve_small_segment_impl()
            .map(|base| ReservedSmallSegment::new_from_reservation(base, owner_id))
    }

    /// R29-3: release a previously-reserved small segment.
    ///
    /// R31-4 (task #467): takes [`ReservedSmallSegment`] BY VALUE instead of
    /// a bare `*mut u8` — the handle can only have been produced by
    /// [`dbg_decomp_reserve_and_keep`](Self::dbg_decomp_reserve_and_keep) on
    /// SOME `AllocCore` (private field + `pub(super)` constructor forecloses
    /// forging one), and consuming it here by value makes a second release
    /// of the SAME handle a compile error (E0382, use of moved value)
    /// instead of an unchecked runtime hazard.
    ///
    /// R31-15 (task #486, CONFIRMED P0 soundness defect): R31-4 closed
    /// unforgeability and double-release but NOT owner-binding — until this
    /// fix, this was a **safe** `pub fn`, and nothing stopped a caller from
    /// reserving a handle on one `AllocCore` and releasing it on a
    /// DIFFERENT `AllocCore`, mutating the wrong heap's pool/directory/
    /// `SegmentTable` state for a segment it never registered while the
    /// true owner's registration of that same base went stale. Fixed two
    /// ways, layered (see `reserved_small_segment.rs`'s module doc,
    /// "Owner-binding" section, for the full writeup):
    ///
    /// 1. A release-build (non-`debug_assert!`) owner-id check below,
    ///    rejecting a cross-core handle before it ever reaches
    ///    `release_or_pool_empty_segment`.
    /// 2. `unsafe fn` — defence-in-depth for preconditions the owner-id
    ///    check cannot see (the segment must still be live/unreleased on
    ///    the same core that reserved it), matching the established
    ///    `unsafe fn` + `# Safety` pattern this crate uses for every other
    ///    hook of this shape (e.g. `HeapCore::dbg_dealloc_own_thread_with_base`).
    ///
    /// # Safety
    ///
    /// `handle` MUST have been produced by a paired
    /// [`dbg_decomp_reserve_and_keep`](Self::dbg_decomp_reserve_and_keep)
    /// call on THIS SAME `AllocCore` (not merely THE SAME logical owner
    /// under an address-based check — this is enforced structurally by the
    /// owner-id check below, which panics on mismatch even in `--release`),
    /// and the reserved segment must still be live/unreleased (not already
    /// released, unregistered, or otherwise invalidated by another `dbg_*`
    /// hook in the interim).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // R31-15: unsafe fn boundary, mirrors dbg_decomp_decommit_payload.
    pub unsafe fn dbg_decomp_release(&mut self, handle: ReservedSmallSegment) {
        // R31-15 (task #486): PRIMARY guard against a cross-core release —
        // a release-build assert (NOT debug_assert!), because this is
        // exactly the "safe-looking but touches a foreign heap's metadata"
        // hazard CLAUDE.md's benchmark-hook rule targets; a check compiled
        // out in --release would defeat the whole point of adding it.
        //
        // Ordering note: `owner_id` is read out and `into_base()` is called
        // to disarm `ReservedSmallSegment`'s leak-detecting `Drop` impl
        // BEFORE the `assert_eq!` below runs — asserting first, while
        // `handle` is still a live local with its `Drop` impl armed, would
        // unwind straight through `handle`'s own scope, firing its
        // `debug_assert!(false, "...dropped without going through
        // release...")` DURING that unwind — a panic-while-panicking, which
        // Rust aborts on unconditionally (not `--release`-specific), taking
        // down the whole process (observed as a raw
        // STATUS_STACK_BUFFER_OVERRUN abort on Windows) instead of
        // propagating a single clean panic. `into_base()` only extracts the
        // raw pointer value and disarms `Drop` — it does NOT touch any
        // allocator metadata itself, so calling it before the check is safe
        // even on the mismatch path: `base` is then discarded by the
        // `assert_eq!` panic below, WITHOUT `self.release_or_pool_empty_segment`
        // (the actual pool/directory/`SegmentTable` mutation) ever running —
        // the true owner's registration of that base is left exactly as it
        // was, untouched by this call.
        let owner_id = handle.owner_id();
        let base = handle.into_base();
        assert_eq!(
            owner_id, self.dbg_reservation_owner_id,
            "dbg_decomp_release: handle was reserved by a DIFFERENT AllocCore (owner_id \
             mismatch) — releasing it here would mutate the wrong heap's pool/directory/\
             SegmentTable state for a segment this AllocCore never registered"
        );
        // Defence-in-depth (R30-1): releasing the segment the live cursor
        // currently points at would immediately dangle `small_cur`, exactly
        // the hazard this task fixed. Not reachable today (the paired
        // `dbg_decomp_reserve_and_keep` never publishes its result as
        // `small_cur`), but cheap to assert locally rather than rely solely
        // on that non-local invariant holding forever. This is now
        // secondary defence-in-depth, not the primary guard — the primary
        // guard against double-release is the move-consuming signature
        // above (a compile error, not a runtime check).
        debug_assert!(
            base != self.small_cur,
            "dbg_decomp_release: base is the live small_cur cursor — release would dangle it"
        );
        self.release_or_pool_empty_segment(base);
    }

    /// R29-3: decommit (`MADV_DONTNEED`) the payload pages of a live segment,
    /// simulating the decommit a reservation-only tier would perform. After
    /// this call, touching the payload re-faults the pages (the irreducible
    /// recommit+first-touch cost the reservation-only design still pays).
    ///
    /// # Safety
    ///
    /// `base` MUST be a live segment base whose payload is fully committed.
    /// The payload pages are returned to the OS; any live data is discarded.
    ///
    /// Task #1081 (F6): the decommit starts at the RUNTIME-page-safe boundary
    /// `small_decommit_start()`, not the tight `small_meta_end()` — the same
    /// R8-6 (task #219) rule the production decommit call sites in this same
    /// file already follow. `small_meta_end()` is a `const fn` aligned only to
    /// the compile-time `PAGE` (4 KiB; non-hardened value 73728), and
    /// `os::decommit_pages`'s contract ("offsets MUST be page-aligned", per
    /// `aligned_vmem::decommit`'s range-contract `debug_assert!`, task #1072)
    /// is checked against the RUNTIME `page_size()` — so on a 16/64 KiB-page
    /// host the old value panicked the hook (73728 % 16384 == 8192). Task
    /// #1074 converted this hook's two neighbours to the runtime query and
    /// missed this pair; task #1072 then made the previously-silent violation
    /// loud — neither commit considered the combination. `SEGMENT` (4 MiB, the
    /// end offset) is a multiple of every supported page size (4/16/64 KiB),
    /// so only the start offset needed fixing.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[allow(unsafe_code)] // R29-3: unsafe fn boundary (raw-pointer precondition).
    pub unsafe fn dbg_decomp_decommit_payload(base: *mut u8) {
        let payload_start = SegLayout::small_decommit_start();
        os::decommit_pages(base, payload_start, SEGMENT);
    }

    /// R31-6 (task #469): re-commit the payload pages of a segment previously
    /// decommitted via [`dbg_decomp_decommit_payload`](Self::dbg_decomp_decommit_payload) —
    /// thin wrapper over [`os::recommit_pages`]. On Windows this is a REAL
    /// `VirtualAlloc(MEM_COMMIT)` (Windows `MEM_DECOMMIT` actually unmaps the
    /// backing pages, unlike POSIX `MADV_DONTNEED`, which leaves the mapping
    /// intact and merely drops the physical backing — re-access is implicitly
    /// safe on Unix); on Unix/miri it is a documented no-op (`os::
    /// recommit_pages` / `aligned_vmem::recommit` already fall back that way).
    /// A caller measuring the re-fault cost after a decommit MUST call this
    /// first on every platform — omitting it is exactly the bug this hook
    /// closes (`examples/r29_3_decomposition_gate.rs`'s Measurement B used to
    /// `write_volatile` straight into the just-decommitted range with no
    /// intervening recommit, which crashes on Windows because the pages are
    /// genuinely unmapped there).
    ///
    /// Returns `true` if the range is now committed (writes are safe),
    /// `false` on genuine OS refusal (commit-charge exhaustion) — the caller
    /// MUST NOT write into the range on `false`.
    ///
    /// # Safety
    ///
    /// `base` MUST be a live segment base whose payload was previously
    /// decommitted via [`dbg_decomp_decommit_payload`](Self::dbg_decomp_decommit_payload)
    /// (or was never committed at all — recommit is idempotent on an
    /// already-committed range on every backend this crate supports).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    #[allow(unsafe_code)] // R31-6: unsafe fn boundary, mirrors dbg_decomp_decommit_payload.
    pub unsafe fn dbg_decomp_recommit_payload(base: *mut u8) -> bool {
        // Task #1081 (F6): same fix as `dbg_decomp_decommit_payload` above —
        // recommit must start at the runtime-page-safe boundary, or the twin
        // silently returns `false` on a 16/64 KiB-page host (the vmem layer
        // skips a contract-violating range), leaving the caller's write loop
        // writing into genuinely unmapped pages under Windows
        // `VirtualFree(MEM_DECOMMIT)` semantics.
        let payload_start = SegLayout::small_decommit_start();
        os::recommit_pages(base, payload_start, SEGMENT)
    }

    /// R29-3: the `[payload_start, payload_end)` byte range a small segment's
    /// DECOMMIT/RECOMMIT hooks actually touch —
    /// `[small_decommit_start(), SEGMENT)`, the runtime-page-safe boundary.
    ///
    /// Task #1081 (F6 sweep): was the TIGHT `[small_meta_end(), SEGMENT)`. On
    /// a >4 KiB-page host the hooks (post-F6) decommit from
    /// `small_decommit_start()`, so reporting the tight start made the
    /// consumers (`examples/r29_3_decomposition_gate.rs`,
    /// `examples/r32_13_windows_reserve_commit_decomposition_gate.rs`)
    /// first-touch and re-fault a range that disagrees with what was actually
    /// decommitted, and `payload_pages` (computed as `(end - start) /
    /// dbg_decomp_page_size()`) mixed a non-page-multiple start with the
    /// runtime page size. Value-identical on 4 KiB-page hosts (where
    /// `small_decommit_start() == small_meta_end()`).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_payload_range() -> (usize, usize) {
        (SegLayout::small_decommit_start(), SEGMENT)
    }

    /// R29-3: the OS page size. Task #1081 (F10b): the RUNTIME page size
    /// (`aligned_vmem::page_size()`), matching the two neighbours task #1074
    /// already converted — was the compile-time `os::PAGE` (4 KiB), which on a
    /// 16 KiB host made consumers report `payload_pages` 4x too high and step
    /// their touch loops at 4 KiB granularity while the hooks decommitted at
    /// 16 KiB granularity. Value-identical on 4 KiB-page hosts, so no
    /// published measurement's basis changes (the R29-3 / R32-13 reports were
    /// generated on 4 KiB-page hosts).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    pub fn dbg_decomp_page_size() -> usize {
        aligned_vmem::page_size()
    }
}
