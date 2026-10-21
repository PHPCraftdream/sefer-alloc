use super::directory::EntryHandle;
use super::small_sidecar::SmallSidecar;
use super::RouteKind;

/// Counted descriptor and sidecar capability, not a reservation credit.
/// No allocator-origin root is accessible through a producer pin.
pub struct RoutePin {
    pub(super) entry: EntryHandle,
}

impl RoutePin {
    pub fn owner(&self) -> usize {
        self.entry.owner()
    }
    pub fn kind(&self) -> RouteKind {
        self.entry.kind()
    }
    pub fn incarnation(&self) -> u64 {
        self.entry.incarnation()
    }
    /// Read-only link-state probe. The cached-pin fast path checks this
    /// (Acquire) before publishing; `false` means the route was unlinked
    /// (cleared under the shard lock) and the pin must not be used for
    /// publication.
    pub fn linked(&self) -> bool {
        self.entry.entry().linked.load(super::Ordering::Acquire)
    }
    /// Allocator-origin usable root from the immutable counted descriptor.
    /// `HeapCore::realloc`'s foreign leg uses it to read the segment's live
    /// commit frontier (R6-T50); no reservation byte is touched.
    pub fn root(&self) -> *mut u8 {
        self.entry.root()
    }
    /// Capacity validation reads only the independent immutable descriptor.
    pub fn contains_payload(&self, ptr: *mut u8, size: usize) -> bool {
        self.entry.contains_payload(ptr.addr(), size)
    }
    /// Span validation mirroring `RouteDirectory::lookup` for a single
    /// address; used by the cached-pin fast path to re-derive the same
    /// verdict a fresh lookup would produce.
    pub fn span_contains_addr(&self, addr: usize) -> bool {
        let e = self.entry.entry();
        addr >= e.payload
            && addr < e.end
            && (if matches!(e.kind, RouteKind::Large) {
                crate::alloc_core::AllocCore::large_payload_address_matches(addr, e.payload, 0)
            } else {
                true
            })
    }
    /// Read-only terminal-publication oracle; does not grant owner mutation.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn pending_for_test(&self, ptr: *mut u8) -> bool {
        if let Some(state) = self.entry.large_state() {
            state.pending_for_test()
        } else {
            let offset = (ptr.addr() & (crate::alloc_core::os::SEGMENT - 1)) as u32;
            self.entry
                .small_sidecar()
                .is_some_and(|sidecar| sidecar.pending_for_test(offset))
        }
    }
    /// Doc-hidden test-only forwarder to the pinned route's `SmallSidecar`
    /// (D1′ phase-1b), whose own test surface is exclusively doc-hidden
    /// (`issue_for_test`). Grants no capability beyond those reviewed hooks:
    /// the consuming `publish_small` path stays the only producer entry, and
    /// the owner-only `issue` remains `pub(crate)`.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_small_sidecar_for_test(&self) -> Option<&SmallSidecar> {
        self.entry.small_sidecar()
    }
    /// Terminal publication consumes the producer capability. Drop then
    /// touches only the independently allocated descriptor/sidecar.
    ///
    /// # Safety
    /// This pin and `offset` identify the caller's current issued Small or
    /// Primordial allocation. The caller transfers its unique ownership once
    /// and must not access or free that allocation after successful publication.
    /// Address lookup alone does not establish this ownership.
    // The cached path publishes via `publish_small_borrowed`; the consuming
    // form remains for external/tests publication sites.
    #[cfg_attr(
        not(any(feature = "internals", feature = "bench-internals")),
        allow(dead_code)
    )]
    #[allow(unsafe_code)]
    pub unsafe fn publish_small(self, offset: u32) -> bool {
        self.publish_small_borrowed(offset)
    }
    /// Borrowing variant of [`RoutePin::publish_small`]; the pin stays alive
    /// for the per-thread route pin cache.
    ///
    /// # Safety
    /// This pin and `offset` identify the caller's current issued Small or
    /// Primordial allocation. The caller transfers its unique ownership once
    /// and must not access or free that allocation after successful publication.
    /// Address lookup alone does not establish this ownership. The pin is NOT
    /// consumed; each call transfers the caller's unique ownership of one
    /// current allocation instance published through this route, and the pin
    /// may serve further instances of the same route (this is what the
    /// per-thread pin cache relies on).
    #[allow(unsafe_code)]
    pub unsafe fn publish_small_borrowed(&self, offset: u32) -> bool {
        // OPT: read the immutable descriptor ONCE — the payload-filter bound
        // (the route kind's metadata end) and the sidecar pointer both come
        // from the same Entry. Metadata offsets are rejected before any
        // sidecar access (D1′ phase-1b semantics unchanged).
        let entry = self.entry.entry();
        let payload_start = match entry.kind {
            super::RouteKind::Primordial => {
                crate::alloc_core::segment_header::Layout::primordial_meta_end()
            }
            _ => crate::alloc_core::segment_header::Layout::small_meta_end(),
        };
        if matches!(entry.kind, super::RouteKind::Large) {
            return false;
        }
        // SAFETY: this counted handle retains the System allocation; only
        // atomic fields are accessed through the shared reference.
        let sidecar = unsafe {
            &*entry
                .sidecar
                .load(super::Ordering::Relaxed)
                .cast::<SmallSidecar>()
        };
        sidecar.publish(offset, payload_start)
    }
    /// Terminal LIVE(g) -> PENDING(g) CAS, consuming this pin.
    ///
    /// # Safety
    /// This pin identifies the caller's current issued Large allocation.
    /// The caller transfers its unique ownership once and must not access or
    /// free that allocation after successful publication.
    #[allow(unsafe_code)]
    pub unsafe fn publish_large(self) -> bool {
        self.entry
            .large_state()
            .is_some_and(|state| state.publish_pending())
    }
    /// Test/diagnostic totals of the per-thread route pin cache
    /// (hits, misses, inserts, evictions, unlink fallbacks, thread releases,
    /// sweep-reclaimed pins).
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_route_pin_cache_totals() -> (u64, u64, u64, u64, u64, u64, u64) {
        crate::registry::heap_core_xthread::route_pin_cache::dbg_totals()
    }
    /// Test-only seam that inserts a caller-looked-up pin into the per-thread
    /// route pin cache through the production insert path, so tests can
    /// exercise the cached fast path and its unlink handling deterministically
    /// without a second live heap.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_cache_insert_for_test(self) {
        let base = self.entry.key();
        let incarnation = self.incarnation();
        crate::registry::heap_core_xthread::route_pin_cache::cache_insert(self, base, incarnation);
    }
    /// Test-only entry into the production cached fast path. The input is
    /// address-only (no provenance); unlike the read-only
    /// `RouteRegistration::start_state_at_global_address_for_test`
    /// sidecar-banks probe, this
    /// publication CREATES a reclaim obligation — hence `pub unsafe fn` (see
    /// `# Safety`). The sidecar RMW never dereferences the pointer.
    ///
    /// # Safety
    /// `address` must be the address of the caller's CURRENT issued Small or
    /// Primordial allocation instance. Publication consumes ownership: after
    /// the call returns `true` that instance must never be accessed or freed
    /// again. A bare address does not establish ownership: publishing a live
    /// allocation owned by someone else forces the owner to treat the
    /// instance as freed (reclaim writes the freelist and releases the
    /// reservation credit) while the owner still uses it — use-after-free by
    /// construction.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    #[allow(unsafe_code)]
    pub unsafe fn dbg_publish_cached_for_test(address: usize) -> bool {
        use crate::registry::heap_core_xthread::route_pin_cache::CachedPublication;
        let ptr = core::ptr::without_provenance_mut::<u8>(address);
        // SAFETY: this hook's `# Safety` contract supplies the unique
        // ownership of the current issued instance; forwarded verbatim to
        // `publish_cached`.
        match unsafe { crate::registry::heap_core_xthread::route_pin_cache::publish_cached(ptr) } {
            CachedPublication::Published(published) => published,
            CachedPublication::Miss => false,
        }
    }
}
