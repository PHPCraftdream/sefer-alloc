//! Grouped helpers whose defining implementations are owned by this crate.

// Loom seam for `reservation_state_impl` (see its header comment): the crate
// itself always binds `core` atomics; an actual-source loom harness mounts the
// file under its own shim module whose `super` provides loom's atomics.
pub(crate) use core::sync::atomic::{AtomicU64, Ordering};

#[path = "facade/geometry.rs"]
pub mod geometry;

#[path = "continuation_bounds.rs"]
#[doc(hidden)]
pub mod continuation_bounds_impl;
/// Counters are shared identities so root call sites update the very same atomics.
#[path = "facade/counters.rs"]
#[doc(hidden)]
pub mod counters_impl;
#[path = "../segment/support/segment_directory/directory_stats.rs"]
pub mod directory_stats;
#[path = "node.rs"]
#[doc(hidden)]
pub mod node_impl;
#[path = "facade/platform.rs"]
pub(crate) mod platform;
#[path = "reservation_state.rs"]
pub mod reservation_state_impl;
#[path = "size_classes.rs"]
#[doc(hidden)]
pub mod size_classes_impl;
// Root-compat aliases: each binds a historical name to the *_impl module.
#[allow(unused_imports)]
pub(crate) use crate::alloc_core as alloc_core_impl;
#[allow(unused_imports)]
pub(crate) use continuation_bounds_impl as continuation_bounds;
#[allow(unused_imports)]
pub(crate) use counters_impl as counters;
#[allow(unused_imports)]
pub(crate) use node_impl as node;
#[allow(unused_imports)]
pub(crate) use reservation_state_impl as reservation_state;
#[allow(unused_imports)]
pub(crate) use size_classes_impl as size_classes;

// P2.1: the root compatibility wrapper for continuation bounds is
// `#[derive(Clone, Copy)]` over this type (the root `config/continuation_bounds.rs`
// pre-split type was `Copy`; the drain loop passes the snapshot by value per
// link). Two-`usize` value type: Copy is layout/behavior-neutral and adds no
// new API surface.
impl Clone for crate::support::continuation_bounds_impl::ContinuationBounds {
    fn clone(&self) -> Self {
        *self
    }
}
impl Copy for crate::support::continuation_bounds_impl::ContinuationBounds {}

// P2.1 boundary-transition compatibility surface for the root allocator
// (`sefer-alloc`). Doc-hidden, not stable public API; each name forwards to
// an existing crate-internal item (aliases/wrappers only — no other crate
// file is modified, no behavior/layout change). See `.rush/p21-support-facade.md`.
/// Doc-hidden root-compat surface; see the module note above.
#[doc(hidden)]
#[allow(private_interfaces)] // aliases intentionally expose crate-private types
pub mod root_compat {
    /// Layout constant (same value as `Layout::primordial_meta_end()`).
    pub const PRIMORDIAL_META_END: usize =
        crate::segment::segment_header::Layout::primordial_meta_end();
    /// Layout constant (same value as `Layout::small_meta_end()`).
    pub const SMALL_META_END: usize = crate::segment::segment_header::Layout::small_meta_end();
    /// Extension-trait facade (see `crate::support::traits`).
    pub use crate::support::traits;
    /// Directory ingress mask type + geometry (root sidecar drain/discovery).
    #[cfg(feature = "alloc-global")]
    pub type HeapIngress = crate::alloc_core::segment_route::HeapIngress;
    /// Ingress geometry constants (same values as the crate originals).
    #[cfg(feature = "alloc-global")]
    pub const INGRESS_CELLS: usize = crate::alloc_core::segment_route::INGRESS_CELLS;
    /// Ingress geometry constants (same values as the crate originals).
    #[cfg(feature = "alloc-global")]
    pub const INGRESS_WORDS: usize = crate::alloc_core::segment_route::INGRESS_WORDS;
    /// Route pin cache publication result (root foreign-free path).
    #[cfg(feature = "alloc-global")]
    pub type CachedPublication =
        crate::alloc_core::segment_route::route_pin_cache::CachedPublication;
    /// Forwards `publish_cached` (same unique-transfer unsafe contract).
    ///
    /// # Safety
    /// Same contract as the crate item: `ptr` names one current issued
    /// allocation whose unique ownership is transferred exactly once.
    #[cfg(feature = "alloc-global")]
    #[allow(unsafe_code)]
    pub unsafe fn publish_cached(ptr: *mut u8) -> CachedPublication {
        // SAFETY: forwarded verbatim to the crate `publish_cached`.
        unsafe { crate::alloc_core::segment_route::route_pin_cache::publish_cached(ptr) }
    }
    /// Forwards `cache_evict` (address-only key, no dereference).
    #[cfg(feature = "alloc-global")]
    pub fn cache_evict(base: usize) {
        crate::alloc_core::segment_route::route_pin_cache::cache_evict(base)
    }
    /// Forwards `cache_insert` (retains the pin per the root free protocol).
    #[cfg(feature = "alloc-global")]
    pub fn cache_insert(
        pin: crate::alloc_core::segment_route::RoutePin,
        base: usize,
        incarnation: u64,
    ) {
        crate::alloc_core::segment_route::route_pin_cache::cache_insert(pin, base, incarnation)
    }
    /// Forwards `publish_configured_slots` (first call wins).
    #[cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]
    pub fn publish_configured_slots(slots: usize) -> bool {
        crate::alloc_core::segment_route::route_pin_cache::publish_configured_slots(slots)
    }
    /// Live state-byte tag of a small class (root dealloc fast lane).
    pub type LiveTag = crate::segment::bitmap::live_tag::LiveTag;
    /// Sidecar word geometry (same value as the crate original).
    pub const SIDECAR_WORD_BYTES: usize = crate::segment::segment_layout::SIDECAR_WORD_BYTES;
    /// Slot-index bit split (same function as the crate original).
    #[inline(always)]
    pub fn word_bit(index: usize) -> (usize, usize) {
        crate::segment::segment_layout::word_bit(index)
    }
    /// Directory diagnostic counters (shared atomic identities, read-only).
    pub use super::directory_counters as directory_stats;
    /// Field-specific header `segment_id` read.
    ///
    /// # Safety
    ///
    /// `base` must be the aligned base of a live segment with an initialized
    /// header; the caller must hold the owner's read permission for the
    /// plain `segment_id` field (write-once at registration).
    #[allow(unsafe_code)]
    #[inline(always)]
    pub unsafe fn segment_id_at(base: *mut u8) -> u32 {
        // SAFETY: caller contract — see `# Safety` above.
        unsafe { crate::segment::segment_header::SegmentHeader::segment_id_at(base) }
    }
    /// Field-specific header `segment_id` write (test-only, `internals`).
    ///
    /// # Safety
    ///
    /// `base` must be the aligned base of a live segment with an initialized
    /// header; the caller must hold exclusive owner access (test-only hook).
    #[cfg(feature = "internals")]
    #[allow(unsafe_code)]
    #[inline(always)]
    pub unsafe fn set_segment_id_at(base: *mut u8, id: u32) {
        // SAFETY: caller contract — see `# Safety` above.
        unsafe { crate::segment::segment_header::SegmentHeader::set_segment_id_at(base, id) }
    }
    /// Root diag wrapper over the crate-private hash-only membership probe.
    #[inline(always)]
    pub fn dbg_hash_contains_only(
        table: &crate::segment::segment_table::SegmentTable,
        base: *mut u8,
    ) -> bool {
        table.dbg_hash_contains_only(base)
    }
}

/// Doc-hidden directory counter accessors (same atomics as `directory_stats`).
#[doc(hidden)]
pub mod directory_counters {
    use crate::support::directory_stats as ds;
    use core::sync::atomic::AtomicU64;
    pub const fn directory_hits() -> &'static AtomicU64 {
        &ds::DIRECTORY_HITS
    }
    pub const fn directory_stale_hits() -> &'static AtomicU64 {
        &ds::DIRECTORY_STALE_HITS
    }
    pub const fn directory_fallback_scans() -> &'static AtomicU64 {
        &ds::DIRECTORY_FALLBACK_SCANS
    }
    pub const fn directory_words_examined() -> &'static AtomicU64 {
        &ds::DIRECTORY_WORDS_EXAMINED
    }
    pub const fn full_scan_slots_examined() -> &'static AtomicU64 {
        &ds::FULL_SCAN_SLOTS_EXAMINED
    }
    pub const fn directory_authoritative_miss() -> &'static AtomicU64 {
        &ds::DIRECTORY_AUTHORITATIVE_MISS
    }
    pub const fn directory_miss_self_heal() -> &'static AtomicU64 {
        &ds::DIRECTORY_MISS_SELF_HEAL
    }
    pub const fn directory_rescue_oom_avoided() -> &'static AtomicU64 {
        &ds::DIRECTORY_RESCUE_OOM_AVOIDED
    }
    pub const fn routed_miss_scans() -> &'static AtomicU64 {
        &ds::ROUTED_MISS_SCANS
    }
    pub const fn routed_miss_scan_drain_created_free() -> &'static AtomicU64 {
        &ds::ROUTED_MISS_SCAN_DRAIN_CREATED_FREE
    }
    pub const fn routed_miss_scan_bin_already_nonempty() -> &'static AtomicU64 {
        &ds::ROUTED_MISS_SCAN_BIN_ALREADY_NONEMPTY
    }
    pub const fn routed_miss_scan_nothing() -> &'static AtomicU64 {
        &ds::ROUTED_MISS_SCAN_NOTHING
    }
}

/// P2.1 extension-trait facade: re-exposes crate-private header/meta/layout/
/// state methods to root call sites under historical names (`compat_` prefixed
/// so the private inherent methods still win resolution inside the crate).
/// Forwarding only; no behavior or layout change. Not stable public API.
#[doc(hidden)]
#[allow(private_interfaces)] // traits mention crate-private types
pub mod traits {
    use crate::segment::bitmap::{live_tag::LiveTag, state_byte::StateByte};
    use crate::segment::segment_header::{Layout, SegmentHeader, SegmentMeta};

    /// Provenance-preserving pointer round-trip (`expose_provenance` on the
    /// way out, `with_exposed_provenance_mut` on the way back in). Bit-for-bit
    /// the same pointer; used by the `compat_` header seams so the raw-pointer
    /// argument is re-attached to its own exposed provenance before the unsafe
    /// header accessor dereferences it (clippy `not_unsafe_ptr_arg_deref`
    /// sees a minted pointer, the safety obligation stays on the accessor's
    /// `# Safety` contract, mirrored in each caller's SAFETY comment).
    fn rt(p: *mut u8) -> *mut u8 {
        // Provenance was exposed by `expose_provenance()` on the same line;
        // re-attaching it is the defined (safe) round-trip.
        core::ptr::with_exposed_provenance_mut::<u8>(p.expose_provenance())
    }

    /// Doc-hidden root-compat methods for [`SegmentMeta`].
    pub trait SegmentMetaCompat {
        fn compat_header(&self) -> SegmentHeader;
        fn compat_write_header(&mut self, hdr: SegmentHeader);
        fn compat_bin_table(&self) -> crate::segment::segment_header::BinTable;
        fn compat_state_byte(&self) -> StateByte;
        fn compat_init_small_terminal(&self);
        fn compat_init_large_terminal(&self);
        fn compat_set_bump(&mut self, value: usize);
        #[cfg(feature = "alloc-decommit")]
        fn compat_set_decommitted(&mut self, value: bool);
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        fn compat_set_committed_payload_end(&mut self, value: usize);
        #[cfg(feature = "alloc-decommit")]
        fn compat_set_pool_prev(&mut self, value: *mut u8);
        #[cfg(feature = "alloc-decommit")]
        fn compat_set_pool_next(&mut self, value: *mut u8);
        #[cfg(feature = "alloc-decommit")]
        fn compat_pool_prev_of(&self) -> *mut u8;
        #[cfg(feature = "alloc-decommit")]
        fn compat_pool_next_of(&self) -> *mut u8;
        fn compat_live_count_of(&self) -> u32;
        #[cfg(feature = "alloc-decommit")]
        fn compat_is_decommitted(&self) -> bool;
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        fn compat_committed_payload_end_of(&self) -> usize;
        #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
        fn compat_sidecar_defer_window_of(&self) -> u32;
        #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
        fn compat_set_sidecar_defer_window(&mut self, value: u32);
        fn compat_large_state_atomic(&self) -> &core::sync::atomic::AtomicU64;
        #[cfg(feature = "alloc-decommit")]
        fn compat_begin_large_reuse(&self) -> Option<u64>;
        #[cfg(feature = "alloc-decommit")]
        fn compat_finish_large_reuse(&self, generation: u64);
        #[cfg(any(feature = "alloc-decommit", feature = "bench-internals"))]
        fn compat_terminal_snapshot(&self) -> crate::segment::segment_header::TerminalSnapshot;
    }
    impl SegmentMetaCompat for SegmentMeta {
        fn compat_header(&self) -> SegmentHeader {
            SegmentMeta::header(self)
        }
        fn compat_write_header(&mut self, hdr: SegmentHeader) {
            SegmentMeta::write_header(self, hdr)
        }
        fn compat_bin_table(&self) -> crate::segment::segment_header::BinTable {
            SegmentMeta::bin_table(self)
        }
        fn compat_state_byte(&self) -> StateByte {
            SegmentMeta::state_byte(self)
        }
        fn compat_init_small_terminal(&self) {
            SegmentMeta::init_small_terminal(self)
        }
        fn compat_init_large_terminal(&self) {
            SegmentMeta::init_large_terminal(self)
        }
        fn compat_set_bump(&mut self, value: usize) {
            SegmentMeta::set_bump(self, value)
        }
        #[cfg(feature = "alloc-decommit")]
        fn compat_set_decommitted(&mut self, value: bool) {
            SegmentMeta::set_decommitted(self, value)
        }
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        fn compat_set_committed_payload_end(&mut self, value: usize) {
            SegmentMeta::set_committed_payload_end(self, value)
        }
        #[cfg(feature = "alloc-decommit")]
        fn compat_set_pool_prev(&mut self, value: *mut u8) {
            SegmentMeta::set_pool_prev(self, value)
        }
        #[cfg(feature = "alloc-decommit")]
        fn compat_set_pool_next(&mut self, value: *mut u8) {
            SegmentMeta::set_pool_next(self, value)
        }
        #[cfg(feature = "alloc-decommit")]
        fn compat_pool_prev_of(&self) -> *mut u8 {
            SegmentMeta::pool_prev_of(self)
        }
        #[cfg(feature = "alloc-decommit")]
        fn compat_pool_next_of(&self) -> *mut u8 {
            SegmentMeta::pool_next_of(self)
        }
        fn compat_live_count_of(&self) -> u32 {
            SegmentMeta::live_count_of(self)
        }
        #[cfg(feature = "alloc-decommit")]
        fn compat_is_decommitted(&self) -> bool {
            SegmentMeta::is_decommitted(self)
        }
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        fn compat_committed_payload_end_of(&self) -> usize {
            SegmentMeta::committed_payload_end_of(self)
        }
        #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
        fn compat_sidecar_defer_window_of(&self) -> u32 {
            SegmentMeta::sidecar_defer_window_of(self)
        }
        #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
        fn compat_set_sidecar_defer_window(&mut self, value: u32) {
            SegmentMeta::set_sidecar_defer_window(self, value)
        }
        fn compat_large_state_atomic(&self) -> &core::sync::atomic::AtomicU64 {
            SegmentMeta::large_state_atomic(self)
        }
        #[cfg(feature = "alloc-decommit")]
        fn compat_begin_large_reuse(&self) -> Option<u64> {
            SegmentMeta::begin_large_reuse(self)
        }
        #[cfg(feature = "alloc-decommit")]
        fn compat_finish_large_reuse(&self, generation: u64) {
            SegmentMeta::finish_large_reuse(self, generation)
        }
        #[cfg(any(feature = "alloc-decommit", feature = "bench-internals"))]
        fn compat_terminal_snapshot(&self) -> crate::segment::segment_header::TerminalSnapshot {
            SegmentMeta::terminal_snapshot(self)
        }
    }

    /// Doc-hidden root-compat associated functions for [`SegmentHeader`].
    pub trait SegmentHeaderCompat {
        fn compat_small(
            segment_id: u32,
            bump: usize,
            reservation: *mut u8,
            reservation_len: usize,
        ) -> Self;
        #[allow(clippy::too_many_arguments)]
        fn compat_large(
            segment_id: u32,
            size: usize,
            align: usize,
            span_usable: usize,
            reserved_capacity: usize,
            bump: usize,
            reservation: *mut u8,
            reservation_len: usize,
        ) -> Self;
        fn compat_segment_id_at(base: *mut u8) -> u32;
        #[cfg(all(
            feature = "alloc-global",
            feature = "fastbin",
            not(feature = "hardened")
        ))]
        fn compat_kind_byte_at(base: *mut u8) -> u8;
        fn compat_span_usable_at(base: *mut u8) -> usize;
        fn compat_thp_charge_at(base: *mut u8) -> usize;
        fn compat_set_thp_charge_at(base: *mut u8, charge: usize);
        /// # Safety
        ///
        /// Same contract as `SegmentHeader::thp_observation_at`: `base` is
        /// the aligned base of a live segment with an initialized header, the
        /// caller holds exclusive owner access; the field is plain (not
        /// atomic) and remote free never touches it.
        #[allow(unsafe_code)]
        unsafe fn compat_thp_observation_at(base: *mut u8) -> u64;
        /// # Safety
        ///
        /// Same contract as `SegmentHeader::set_thp_observation_at`: `base`
        /// is the aligned base of a live segment with an initialized header,
        /// the caller holds exclusive owner access (sole writer); the field is
        /// plain (not atomic).
        #[allow(unsafe_code)]
        unsafe fn compat_set_thp_observation_at(base: *mut u8, token: u64);
        fn compat_set_large_size_at(base: *mut u8, size: usize);
        fn compat_set_large_align_at(base: *mut u8, align: usize);
        fn compat_set_bump_at(base: *mut u8, bump: usize);
        fn compat_set_magic_at(base: *mut u8, magic: u32);
    }
    impl SegmentHeaderCompat for SegmentHeader {
        fn compat_small(
            segment_id: u32,
            bump: usize,
            reservation: *mut u8,
            reservation_len: usize,
        ) -> Self {
            SegmentHeader::small(segment_id, bump, reservation, reservation_len)
        }
        #[allow(clippy::too_many_arguments)]
        fn compat_large(
            segment_id: u32,
            size: usize,
            align: usize,
            span_usable: usize,
            reserved_capacity: usize,
            bump: usize,
            reservation: *mut u8,
            reservation_len: usize,
        ) -> Self {
            SegmentHeader::large(
                segment_id,
                size,
                align,
                span_usable,
                reserved_capacity,
                bump,
                reservation,
                reservation_len,
            )
        }
        #[allow(unsafe_code)]
        fn compat_segment_id_at(base: *mut u8) -> u32 {
            // SAFETY: `base` is the aligned base of a live registered segment with an
            // initialized header (write-once `segment_id` discipline). The
            // expose/reattach round-trip preserves provenance bit-for-bit; the
            // safety obligation is `SegmentHeader::segment_id_at`'s `# Safety`.
            // SAFETY: `base` is the aligned base of a live registered segment
            // with an initialized header; the read follows the header's
            // write-once `segment_id` field discipline (seam lifts the unsafe
            // for the safe root callers).
            unsafe { SegmentHeader::segment_id_at(rt(base)) }
        }
        #[cfg(all(
            feature = "alloc-global",
            feature = "fastbin",
            not(feature = "hardened")
        ))]
        fn compat_kind_byte_at(base: *mut u8) -> u8 {
            SegmentHeader::kind_byte_at(base)
        }
        #[allow(unsafe_code)]
        fn compat_span_usable_at(base: *mut u8) -> usize {
            // SAFETY: aligned base of a live registered segment (header
            // `span_usable` discipline); expose/reattach preserves provenance;
            // obligation is `SegmentHeader::span_usable_at`'s `# Safety`.
            // SAFETY: `base` is the aligned base of a live registered segment
            // with an initialized header; the read follows the header's
            // `span_usable` field discipline (seam lifts the unsafe for the
            // safe root callers).
            unsafe { SegmentHeader::span_usable_at(rt(base)) }
        }
        #[allow(unsafe_code)]
        fn compat_thp_charge_at(base: *mut u8) -> usize {
            // SAFETY: aligned base of a live segment (owner-only `thp_charge`
            // discipline); expose/reattach preserves provenance; obligation is
            // `SegmentHeader::thp_charge_at`'s `# Safety`.
            // SAFETY: `base` is the aligned base of a live segment with an
            // initialized header; the read follows the owner-only
            // `thp_charge` field discipline (seam lifts the unsafe for the
            // safe root callers).
            unsafe { SegmentHeader::thp_charge_at(rt(base)) }
        }
        fn compat_set_thp_charge_at(base: *mut u8, charge: usize) {
            SegmentHeader::set_thp_charge_at(base, charge)
        }
        /// Field-specific header `thp_observation` read (root compat seam).
        ///
        /// # Safety
        ///
        /// Same contract as `SegmentHeader::thp_observation_at`: `base` is
        /// the aligned base of a live segment with an initialized header, the
        /// caller holds exclusive owner access; the field is plain (not
        /// atomic) and remote free never touches it.
        #[allow(unsafe_code)]
        unsafe fn compat_thp_observation_at(base: *mut u8) -> u64 {
            // SAFETY: caller contract — see `# Safety` above; expose/reattach
            // preserves provenance; obligation is the header accessor's.
            unsafe { SegmentHeader::thp_observation_at(rt(base)) }
        }
        /// Field-specific header `thp_observation` write (root compat seam).
        ///
        /// # Safety
        ///
        /// Same contract as `SegmentHeader::set_thp_observation_at`: `base`
        /// is the aligned base of a live segment with an initialized header,
        /// the caller holds exclusive owner access (sole writer); the field is
        /// plain (not atomic).
        #[allow(unsafe_code)]
        unsafe fn compat_set_thp_observation_at(base: *mut u8, token: u64) {
            // SAFETY: caller contract — see `# Safety` above; expose/reattach
            // preserves provenance; obligation is the header accessor's.
            unsafe { SegmentHeader::set_thp_observation_at(rt(base), token) }
        }
        #[allow(unsafe_code)]
        fn compat_set_large_size_at(base: *mut u8, size: usize) {
            // SAFETY: aligned base of a live segment under exclusive owner
            // access (root realloc grow path); expose/reattach preserves
            // provenance; obligation is `set_large_size_at`'s `# Safety`.
            // SAFETY: `base` is the aligned base of a live segment with an
            // initialized header and the caller holds exclusive owner access
            // (root realloc grow path; seam lifts the unsafe).
            unsafe { SegmentHeader::set_large_size_at(rt(base), size) }
        }
        fn compat_set_large_align_at(base: *mut u8, align: usize) {
            SegmentHeader::set_large_align_at(base, align)
        }
        fn compat_set_bump_at(base: *mut u8, bump: usize) {
            SegmentHeader::set_bump_at(base, bump)
        }
        fn compat_set_magic_at(base: *mut u8, magic: u32) {
            SegmentHeader::set_magic_at(base, magic)
        }
    }

    /// Doc-hidden root-compat associated functions for [`Layout`].
    pub trait LayoutCompat {
        fn compat_primordial_meta_end() -> usize;
        fn compat_small_meta_end() -> usize;
        fn compat_bin_table_off() -> usize;
        fn compat_primordial_registry_off() -> usize;
        fn compat_primordial_hash_off() -> usize;
        fn compat_primordial_free_list_off() -> usize;
        fn compat_primordial_free_top_off() -> usize;
        fn compat_primordial_active_kind_off() -> usize;
        fn compat_primordial_active_kind_end() -> usize;
        fn compat_state_byte_off() -> usize;
        fn compat_small_decommit_start() -> usize;
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        fn compat_lazy_initial_commit(meta_end: usize, page_size: usize) -> usize;
    }
    impl LayoutCompat for Layout {
        fn compat_primordial_meta_end() -> usize {
            Layout::primordial_meta_end()
        }
        fn compat_small_meta_end() -> usize {
            Layout::small_meta_end()
        }
        fn compat_bin_table_off() -> usize {
            Layout::bin_table_off()
        }
        fn compat_primordial_registry_off() -> usize {
            Layout::primordial_registry_off()
        }
        fn compat_primordial_hash_off() -> usize {
            Layout::primordial_hash_off()
        }
        fn compat_primordial_free_list_off() -> usize {
            Layout::primordial_free_list_off()
        }
        fn compat_primordial_free_top_off() -> usize {
            Layout::primordial_free_top_off()
        }
        fn compat_primordial_active_kind_off() -> usize {
            Layout::primordial_active_kind_off()
        }
        fn compat_primordial_active_kind_end() -> usize {
            Layout::primordial_active_kind_end()
        }
        fn compat_state_byte_off() -> usize {
            Layout::state_byte_off()
        }
        fn compat_small_decommit_start() -> usize {
            Layout::small_decommit_start()
        }
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        fn compat_lazy_initial_commit(meta_end: usize, page_size: usize) -> usize {
            Layout::lazy_initial_commit(meta_end, page_size)
        }
    }

    /// Doc-hidden root-compat methods for [`LiveTag`].
    pub trait LiveTagCompat {
        fn compat_of_seed(seed: usize) -> Self;
        fn compat_class(self) -> usize;
        fn compat_byte(self) -> u8;
        fn compat_mag_byte(self) -> u8;
    }
    impl LiveTagCompat for LiveTag {
        fn compat_of_seed(seed: usize) -> Self {
            LiveTag::of_seed(seed)
        }
        fn compat_class(self) -> usize {
            LiveTag::class(self)
        }
        fn compat_byte(self) -> u8 {
            LiveTag::byte(self)
        }
        fn compat_mag_byte(self) -> u8 {
            LiveTag::mag_byte(self)
        }
    }

    /// Doc-hidden root-compat associated functions for [`StateByte`].
    pub trait StateByteCompat {
        fn compat_encode(class_idx: usize, state: u8) -> u8;
        fn compat_shadow_reject(site: &'static [u8]);
        fn compat_read(&self, off: u32) -> u8;
        fn compat_write(&self, off: u32, class_idx: usize, state: u8);
        fn compat_set_state(&self, off: u32, state: u8);
        #[cfg(all(
            feature = "alloc-global",
            feature = "fastbin",
            not(feature = "hardened")
        ))]
        fn compat_read_granule(&self, g: usize) -> u8;
        #[cfg(all(
            feature = "alloc-global",
            feature = "fastbin",
            not(feature = "hardened")
        ))]
        fn compat_write_granule(&self, g: usize, byte: u8);
    }
    impl StateByteCompat for StateByte {
        fn compat_encode(class_idx: usize, state: u8) -> u8 {
            StateByte::encode(class_idx, state)
        }
        fn compat_shadow_reject(site: &'static [u8]) {
            StateByte::shadow_reject(site)
        }
        fn compat_read(&self, off: u32) -> u8 {
            StateByte::read(self, off)
        }
        fn compat_write(&self, off: u32, class_idx: usize, state: u8) {
            StateByte::write(self, off, class_idx, state)
        }
        fn compat_set_state(&self, off: u32, state: u8) {
            StateByte::set_state(self, off, state)
        }
        #[cfg(all(
            feature = "alloc-global",
            feature = "fastbin",
            not(feature = "hardened")
        ))]
        fn compat_read_granule(&self, g: usize) -> u8 {
            StateByte::read_granule(self, g)
        }
        #[cfg(all(
            feature = "alloc-global",
            feature = "fastbin",
            not(feature = "hardened")
        ))]
        fn compat_write_granule(&self, g: usize, byte: u8) {
            StateByte::write_granule(self, g, byte)
        }
    }

    /// Doc-hidden root-compat methods for `BinTable`.
    pub trait BinTableCompat {
        fn compat_head(&self, c: usize) -> u32;
        fn compat_set_head(&mut self, c: usize, off: u32);
    }
    impl BinTableCompat for crate::segment::segment_header::BinTable {
        fn compat_head(&self, c: usize) -> u32 {
            crate::segment::segment_header::BinTable::head(self, c)
        }
        fn compat_set_head(&mut self, c: usize, off: u32) {
            crate::segment::segment_header::BinTable::set_head(self, c, off)
        }
    }

    /// Doc-hidden root-compat associated functions for `BlockKind`.
    pub trait BlockKindCompat {
        fn compat_of(base: *mut u8, class: Option<usize>) -> Self;
    }
    impl BlockKindCompat for crate::alloc_core::segment_header::BlockKind {
        #[allow(unsafe_code)]
        fn compat_of(base: *mut u8, class: Option<usize>) -> Self {
            // SAFETY: `base` is the aligned base of a live segment with an
            // initialized header (owner-side classification path);
            // expose/reattach preserves provenance; obligation is
            // `BlockKind::of`'s `# Safety`.
            // SAFETY: `base` is the aligned base of a live segment with an
            // initialized header (owner-side free/realloc classification
            // path; seam lifts the unsafe for the safe root callers).
            unsafe { crate::alloc_core::segment_header::BlockKind::of(rt(base), class) }
        }
    }

    /// Doc-hidden root-compat associated functions for the route ingress.
    #[cfg(feature = "alloc-global")]
    pub trait HeapIngressCompat {
        fn compat_of(owner: usize) -> Option<&'static Self>;
        fn compat_take(&self, each: impl FnMut(usize));
    }
    #[cfg(feature = "alloc-global")]
    impl HeapIngressCompat for crate::alloc_core::segment_route::HeapIngress {
        fn compat_of(owner: usize) -> Option<&'static Self> {
            crate::alloc_core::segment_route::HeapIngress::of(owner)
        }
        fn compat_take(&self, each: impl FnMut(usize)) {
            crate::alloc_core::segment_route::HeapIngress::take(self, each)
        }
    }
}
