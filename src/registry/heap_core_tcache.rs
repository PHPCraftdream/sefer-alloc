//! Tcache (magazine) lifecycle for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R6-CQ-7b).
//!
//! This file holds the `impl HeapCore { .. }` block for the per-heap
//! magazine hit-counter binding/reads and the tcache-flush teardown-trim
//! primitive (`flush_all_tcache`, and its `dbg_flush_all` test hook).
//! Pure code-movement sibling of `heap_core.rs`; no behavior changed.

#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use core::sync::atomic::Ordering;

#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::os;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::segment_header::SegmentMeta;

use super::heap_core::HeapCore;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use super::heap_core::TcacheHitCounter;

impl HeapCore {
    /// TEST/DIAGNOSTIC-ONLY (task #133): this heap's own magazine-hit count.
    /// Relaxed load of [`tcache_hits`](Self::tcache_hits) — sound for a
    /// cross-thread diagnostic read (see the field's doc comment). Used by
    /// [`super::heap_registry::tcache_hits_total`] to aggregate across every
    /// LIVE slot into the process-wide view `stats()` exposes.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[doc(hidden)]
    #[must_use]
    pub fn tcache_hits(&self) -> u64 {
        // W3: read THIS heap's counter out of its owning slot (via the stable
        // `&'static` handle planted by `claim`). Reads the SAME `AtomicU64`
        // the aggregator reads, so per-heap and process-wide views agree.
        // `None` only in the pre-bind window (never on an alloc path) — 0.
        self.tcache_hits.map_or(0, |c| c.load(Ordering::Relaxed))
    }

    /// W3: plant the stable handle to THIS heap's slot-resident magazine
    /// (tcache) hit counter. Called by
    /// [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim) once,
    /// right after the slot is bound (and the `HeapCore` materialised), before
    /// any allocation on this heap runs. `counter` is a `&'static` reference to
    /// the owning slot's `tcache_hits`. Idempotent — on a slot re-claim the
    /// handle already references the same `'static` slot counter, so
    /// re-planting is a harmless no-op store.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) fn bind_tcache_hits(&mut self, counter: &'static TcacheHitCounter) {
        self.tcache_hits = Some(counter);
    }

    /// W3: plant the stable handle to THIS heap's slot-resident large-segment
    /// cache hit counter (forwarded into the inner `AllocCore`). Same contract
    /// as [`bind_tcache_hits`](Self::bind_tcache_hits). Gated on
    /// `alloc-decommit` (independent of `fastbin`), mirroring
    /// `AllocCore::large_cache_hits`'s gate.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn bind_large_cache_hits(
        &mut self,
        counter: &'static core::sync::atomic::AtomicU64,
    ) {
        self.core.bind_large_cache_hits(counter);
    }

    /// Flush every tcache class's magazine back to the substrate via
    /// `flush_class` → `dealloc_small` → `dec_live` → `maybe_decommit`.
    ///
    /// After this call, every magazine slot is empty (`count[c] == 0` for
    /// all classes) and the blocks have been returned to their owning
    /// segments. If any segment reaches `live_count == 0` during the flush,
    /// decommit/release fires (or the segment is pooled, subject to the
    /// pool's cap).
    ///
    /// This is the production teardown-trim primitive (task #95 / N1),
    /// called from [`trim_for_recycle`](Self::trim_for_recycle). The
    /// `#[doc(hidden)] pub dbg_flush_all` test hook delegates here so test
    /// coverage is preserved.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) fn flush_all_tcache(&mut self) {
        use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;
        for c in 0..SMALL_CLASS_COUNT {
            let n = self.tcache.classes[c].count as usize;
            if n == 0 {
                continue;
            }
            // RAD-5 (E4) GO/NO-GO EXPERIMENT: clear every flushed block's
            // magazine-residency bit BEFORE the flush, mirroring the
            // production overflow-flush site in `dealloc_own_thread_with_base`.
            for &flushed in &self.tcache.classes[c].slots[0..n] {
                let fbase = os::segment_base_of_ptr(flushed);
                let foff = (flushed as usize - fbase as usize) as u32;
                SegmentMeta::new(fbase)
                    .magazine_bitmap()
                    .clear_magazine(foff);
            }
            // SAFETY (R6-MS-3): every slot in `slots[0..n]` is a valid live
            // small-class-`c` allocation owned by this core's magazine; the
            // teardown trim returns each to the substrate exactly once.
            #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
            unsafe {
                self.core
                    .flush_class(c, &self.tcache.classes[c].slots[0..n])
            };
            self.tcache.classes[c].count = 0;
            // R13-3 (task #273): every slot in this class's magazine was just
            // returned to the substrate — reset the virgin mask to empty,
            // maintaining "bits >= count are 0" for `count == 0`.
            #[cfg(feature = "virgin-zero-skip")]
            {
                self.tcache.classes[c].virgin_mask = 0;
            }
        }
    }

    /// TEST-ONLY (P5): force-flush every class's magazine back to the
    /// substrate. Delegates to the production [`flush_all_tcache`](Self::flush_all_tcache)
    /// teardown-trim primitive. Used by decommit-soak tests to drain
    /// magazine-buffered blocks before asserting decommit invariants.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_flush_all(&mut self) {
        self.flush_all_tcache();
    }
}
