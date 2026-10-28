//! Tcache (magazine) lifecycle for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R6-CQ-7b).
//!
//! This file holds the `impl HeapCore { .. }` block for the per-heap
//! magazine hit-counter binding/reads and the tcache-flush teardown-trim
//! primitive (`flush_all_tcache`, and its `dbg_flush_all` test hook).
//! Pure code-movement sibling of `heap_core.rs`; no behavior changed.

#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use ::core::sync::atomic::Ordering;

use crate::registry::heap_core::HeapCore;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::registry::heap_core::TcacheHitCounter;

impl HeapCore {
    /// TEST/DIAGNOSTIC-ONLY (task #133): this heap's own magazine-hit count.
    /// Relaxed load of [`tcache_hits`](Self::tcache_hits) — sound for a
    /// cross-thread diagnostic read (see the field's doc comment). Used by
    /// [`crate::registry::heap_registry::tcache_hits_total`] to aggregate across every
    /// LIVE slot into the process-wide view `stats()` exposes.
    // R6-T16: internals-test reader only — the stats aggregator reads the
    // slot-resident field directly, not this method.
    #[cfg_attr(
        not(any(feature = "alloc-stats", feature = "internals")),
        allow(dead_code)
    )]
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
    /// [`HeapRegistry::claim_lease`](crate::registry::heap_registry::HeapRegistry::claim_lease) once,
    /// right after the slot is bound (and the `HeapCore` materialised), before
    /// any allocation on this heap runs. `counter` is a `&'static` reference to
    /// the owning slot's `tcache_hits`. Idempotent — on a slot re-claim the
    /// handle already references the same `'static` slot counter, so
    /// re-planting is a harmless no-op store.
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) fn bind_tcache_hits(&mut self, counter: &'static TcacheHitCounter) {
        self.tcache_hits = Some(counter);
    }

    /// B4 depot (DP-3): bind this heap to its owning slot's depot storage.
    /// Called once per materialised slot by `bind_slot_counters`; the binding
    /// survives recycle (the `HeapCore` stays in the slot).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub(crate) fn bind_depot(&mut self, depot: &'static mut super::depot::Depot) {
        self.depot = Some(depot);
    }

    /// W3: plant the stable handle to THIS heap's slot-resident large-segment
    /// cache hit counter (forwarded into the inner `AllocCore`). Same contract
    /// as [`bind_tcache_hits`](Self::bind_tcache_hits). Gated on
    /// `alloc-decommit` (independent of `fastbin`), mirroring
    /// `AllocCore::large_cache_hits`'s gate.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn bind_large_cache_hits(
        &mut self,
        counter: &'static ::core::sync::atomic::AtomicU64,
    ) {
        self.core.bind_large_cache_hits(counter);
    }

    /// Flush every tcache class's magazine back to the substrate via
    /// `flush_class` → `dealloc_small` → `dec_live` → `maybe_decommit`.
    ///
    /// After this call, every magazine slot and every parked depot magazine
    /// is empty (`count[c] == 0` for all classes) and the blocks have been
    /// returned to their owning
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
            // B4 depot (DP-3): parked magazines are reloaded one by one into the
            // emptied magazine and flushed the same way.
            loop {
                let n = self.tcache.classes[c].count as usize;
                if n != 0 {
                    // B4 (в): the former magazine-bitmap clear pre-pass is gone —
                    // `flush_class` itself commits LIVE/MAG → FREE via the state byte.
                    // SAFETY (R6-MS-3): every slot in `slots[0..n]` is a valid live
                    // small-class-`c` allocation owned by this core's magazine; the
                    // teardown trim returns each to the substrate exactly once.
                    #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
                    unsafe {
                        self.core
                            .flush_class_internal(c, &self.tcache.classes[c].slots[0..n])
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
                // Teardown reload: `depot_reload` counts no decay tick.
                if !self.depot_reload(c) {
                    break;
                }
            }
        }
        // G1-K7: the depot (floor and overflow) is now empty; return the
        // overflow's committed record pages and reset its rings.
        #[cfg(feature = "alloc-decommit")]
        self.depot_release_overflow_pages();
    }

    /// G1-K7: decommit the depot overflow's record pages and reset its rings
    /// once the teardown drain has emptied the depot.
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "alloc-decommit"
    ))]
    fn depot_release_overflow_pages(&mut self) {
        if let Some(depot) = self.depot.as_deref_mut() {
            depot.ovf.release_pages();
        }
    }

    /// G1-K7: count one cold event for the depot-overflow decay and pay up to
    /// `DECAY_WORK` records of decay debt — the oldest overflow records of
    /// classes that parked nothing during the last epoch go back to their
    /// segments. Touches only long-parked records, never the live magazine.
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "alloc-decommit"
    ))]
    pub(crate) fn depot_tick(&mut self) {
        use crate::registry::heap_core::state::depot::{DEPOT_CLASSES, DEPOT_K};
        use crate::registry::heap_core::state::depot_overflow::DECAY_WORK;
        use crate::registry::heap_core::state::tcache::TCACHE_CAP;
        let Some(depot) = self.depot.as_deref_mut() else {
            return;
        };
        // Empty overflow: no record to decay, every watermark and debt is 0.
        if !depot.ovf.active() || depot.ovf.bytes() == 0 {
            return;
        }
        let mut lens = [0usize; DEPOT_CLASSES];
        for (c, len) in lens.iter_mut().enumerate() {
            *len = (self.tcache.classes[c].parked as usize).saturating_sub(DEPOT_K[c] as usize);
        }
        depot.ovf.note_event(&lens);
        let mut work = 0;
        while work < DECAY_WORK {
            let Some(depot) = self.depot.as_deref_mut() else {
                return;
            };
            let Some(c) = depot.ovf.debt_class() else {
                return;
            };
            let n = (self.tcache.classes[c].parked as usize).saturating_sub(DEPOT_K[c] as usize);
            if n == 0 {
                depot.ovf.clear_debt(c);
                continue;
            }
            let mut mag = [core::ptr::null_mut(); TCACHE_CAP];
            depot.ovf.pop_oldest(c, n, &mut mag);
            self.tcache.classes[c].parked -= 1;
            // SAFETY (R6-MS-3): `mag` is a full parked magazine of class `c`
            // (16 blocks issued by this heap, state MAG, each held exactly
            // once — the record just left the ring), returned to the
            // substrate exactly once here.
            #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
            unsafe {
                self.core.flush_class_internal(c, &mag);
            }
            work += 1;
        }
    }

    /// TEST-ONLY (P5): force-flush every class's magazine back to the
    /// substrate. Delegates to the production [`flush_all_tcache`](Self::flush_all_tcache)
    /// teardown-trim primitive. Used by decommit-soak tests to drain
    /// magazine-buffered blocks before asserting decommit invariants.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin", feature = "internals"))]
    pub fn dbg_flush_all(&mut self) {
        self.flush_all_tcache();
    }
}
