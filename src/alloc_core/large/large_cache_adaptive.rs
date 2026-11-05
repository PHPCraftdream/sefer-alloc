//! J14 (`large-cache-adaptive`): the Large cache learns the program's reuse
//! habit (`docs/perf/J14_ADAPTIVE_LARGE_CACHE_PREREG.md`).
//!
//! A *mistake* is a Large allocation that misses the cache while a span the
//! cache recently gave back to the OS (a *ghost*) would have served it. Its
//! *distance* is the bytes the cache gave back since that span left, the span
//! included: the extra retention that would have turned the miss into a hit.
//! Distances go into a decaying histogram of log2 buckets; the learned extra
//! is the smallest bucket bound whose cumulative weight (the integral) covers
//! `COVER_PCT` of all mistakes. Each decay interval forgets 1/16 of every
//! weight, a quiet one (no mistake, and hits that used under 1/8 of the learned
//! extra) half and moves every distance down one bucket (the extra halves),
//! while the decay releases 3/4 of the excess per step. The extra never
//! exceeds the heap's recent peak of live Large bytes above the headroom, so
//! the cache never holds more than the program itself recently had live. The
//! 8 base slots behave as in `production` (no byte budget unless the caller
//! sets one); extension slots open only while the learned target has room.
//!
//! All state lives in the extension sidecar ([`AdaptiveState`] inside
//! `LargeCacheExtension`), so `AllocCore`'s size does not change. Without the
//! feature every hook is an empty inline function.

#[cfg(feature = "large-cache-adaptive")]
use super::large_cache_extended::{self, LARGE_CACHE_EXTENDED_SLOTS};
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(feature = "large-cache-adaptive")]
use crate::alloc_core::alloc_core::{LARGE_CACHE_SIZE_FACTOR, LARGE_CACHE_SLOTS};

/// Spans remembered after the cache gave them back to the OS under pressure.
#[cfg(feature = "large-cache-adaptive")]
pub(crate) const GHOST_N: usize = 256;

/// Extension slots opened per growth step.
#[cfg(feature = "large-cache-adaptive")]
const SLOT_STEP: usize = 8;

/// log2 buckets of the distance histogram (bucket `b` holds `[2^b, 2^(b+1))`).
#[cfg(feature = "large-cache-adaptive")]
const BUCKETS: usize = 48;

/// Weight one mistake adds to its bucket (fixed point, so decay keeps resolution).
#[cfg(feature = "large-cache-adaptive")]
const UNIT: u32 = 1 << 8;

/// Share of the excess over the target the decay releases in a quiet interval, in basis points.
#[cfg(feature = "large-cache-adaptive")]
pub(in crate::alloc_core) const QUIET_RELEASE_BP: usize = 7_500;

/// Share of the mistake weight the learned extra must cover.
#[cfg(feature = "large-cache-adaptive")]
const COVER_PCT: u64 = 90;

/// The learned habit of one heap. Owner-only, inside the extension sidecar.
#[cfg(feature = "large-cache-adaptive")]
pub(crate) struct AdaptiveState {
    /// `(usable, gone)` of spans given back under slot or budget pressure:
    /// `gone` = `given_back` right after the span left; usable 0 = empty.
    ghost: [(usize, u64); GHOST_N],
    ghost_next: usize,
    /// Bytes given back under pressure since the sidecar exists.
    given_back: u64,
    /// Decaying weights of mistake distances by log2 bucket.
    hist: [u32; BUCKETS],
    /// Learned retention above the decay headroom, in bytes.
    pub(in crate::alloc_core) extra: usize,
    /// Live Large bytes since the sidecar exists (frees of older blocks saturate at 0).
    pub(in crate::alloc_core) live: usize,
    /// Recent peak of `live`, decaying by 1/8 of its distance to `live` per interval.
    pub(in crate::alloc_core) peak: usize,
    mistakes: u32,
    /// The last interval was quiet (the retention went unused).
    pub(in crate::alloc_core) quiet: bool,
    /// Bytes served by cache hits in the current interval.
    hit_bytes: usize,
    /// Mistakes since the sidecar exists (diagnostics).
    pub(in crate::alloc_core) mistakes_total: u64,
}

#[cfg(feature = "large-cache-adaptive")]
impl AdaptiveState {
    /// Whether any mistake weight is left (the habit is still remembered).
    fn learning(&self) -> bool {
        self.hist.iter().any(|&w| w != 0)
    }

    /// The smallest bucket bound whose cumulative weight covers `COVER_PCT`
    /// of the total, capped at `cap`; 0 without mistakes.
    fn learned_extra(&self, cap: usize) -> usize {
        let total: u64 = self.hist.iter().map(|&w| u64::from(w)).sum();
        if total == 0 {
            return 0;
        }
        let mut acc = 0u64;
        for (b, &w) in self.hist.iter().enumerate() {
            acc += u64::from(w);
            if acc * 100 >= total * COVER_PCT {
                let bound = 1usize.checked_shl(b as u32 + 1).unwrap_or(usize::MAX);
                return bound.min(cap);
            }
        }
        cap
    }

    pub(crate) const NEW: Self = Self {
        ghost: [(0, 0); GHOST_N],
        ghost_next: 0,
        given_back: 0,
        hist: [0; BUCKETS],
        extra: 0,
        live: 0,
        peak: 0,
        mistakes: 0,
        quiet: false,
        hit_bytes: 0,
        mistakes_total: 0,
    };
}

#[cfg(feature = "large-cache-adaptive")]
impl AllocCore {
    /// The extension's adaptive state, `None` while the sidecar does not exist
    /// or adaptation is switched off for this heap.
    #[inline]
    #[allow(unsafe_code)] // J14: calls the `unsafe fn deref_large_cache_extension_mut` boundary right
                          // after `self.large_cache_extension` is proven non-null. Sound: the pointer
                          // was produced only by `reserve_large_cache_extension` (typed-initialised),
                          // `AllocCore`'s owner-only discipline (neither `Send` nor `Sync`) rules out a
                          // concurrent reader or writer, and the returned `&mut` is tied to this
                          // `&mut self` borrow, so no other reference to the sidecar is live.
    fn adaptive_ext(&mut self) -> Option<&mut large_cache_extended::LargeCacheExtension> {
        if self.large_cache_extension.is_null() || !self.decay_config.adaptive {
            return None;
        }
        let p = self.large_cache_extension;
        // SAFETY: see the `#[allow(unsafe_code)]` justification above this function.
        Some(unsafe { large_cache_extended::deref_large_cache_extension_mut(p, &*self) })
    }

    /// Read-only twin of [`Self::adaptive_ext`].
    #[inline]
    #[allow(unsafe_code)] // J14: calls the `unsafe fn deref_large_cache_extension` boundary right after
                          // `self.large_cache_extension` is proven non-null. Sound: the pointer was
                          // produced only by `reserve_large_cache_extension` (typed-initialised),
                          // `AllocCore`'s owner-only discipline rules out a concurrent writer, and the
                          // returned `&` is tied to this `&self` borrow.
    fn adaptive_ref(&self) -> Option<&large_cache_extended::LargeCacheExtension> {
        if self.large_cache_extension.is_null() || !self.decay_config.adaptive {
            return None;
        }
        // SAFETY: see the `#[allow(unsafe_code)]` justification above this function.
        Some(unsafe {
            large_cache_extended::deref_large_cache_extension(self.large_cache_extension, self)
        })
    }

    /// Bytes the cache keeps without decay: the headroom plus the learned extra.
    #[inline]
    pub(in crate::alloc_core) fn large_cache_retain_target(&self) -> usize {
        let headroom = self.decay_config.headroom_bytes;
        headroom.saturating_add(self.adaptive_ref().map_or(0, |e| e.adapt.extra))
    }

    /// Release rate of the decay step in basis points: [`QUIET_RELEASE_BP`] while the last interval was quiet,
    /// the configured rate otherwise.
    #[inline]
    pub(in crate::alloc_core) fn large_cache_decay_rate_bp(&self) -> usize {
        let rate = self.decay_config.decay_rate_bp as usize;
        if self.adaptive_ref().is_some_and(|e| e.adapt.quiet) {
            rate.max(QUIET_RELEASE_BP)
        } else {
            rate
        }
    }

    /// Whether quiet intervals still have learned retention to unlearn (the
    /// decay must keep ticking although the cache is under its target).
    #[inline]
    pub(in crate::alloc_core) fn large_cache_adaptive_pending(&self) -> bool {
        self.adaptive_ref().is_some_and(|e| e.adapt.learning())
    }

    /// A Large allocation of `usable` bytes was served by the cache.
    #[inline]
    pub(in crate::alloc_core) fn large_cache_note_hit(&mut self, usable: usize) {
        if let Some(e) = self.adaptive_ext() {
            let a = &mut e.adapt;
            a.hit_bytes = a.hit_bytes.saturating_add(usable);
            a.live = a.live.saturating_add(usable);
            a.peak = a.peak.max(a.live);
        }
    }

    /// A Large allocation of `usable` bytes missed the cache: if a ghost would
    /// have served it, that is a mistake; its distance enters the histogram.
    #[inline]
    pub(in crate::alloc_core) fn large_cache_note_miss(&mut self, usable: usize) {
        let headroom = self.decay_config.headroom_bytes;
        if let Some(e) = self.adaptive_ext() {
            let a = &mut e.adapt;
            a.live = a.live.saturating_add(usable);
            a.peak = a.peak.max(a.live);
            let hi = usable.saturating_mul(LARGE_CACHE_SIZE_FACTOR);
            let now = a.given_back;
            if let Some(g) = a.ghost.iter_mut().find(|g| g.0 >= usable && g.0 <= hi) {
                // Bytes given back since this span left, the span included.
                let distance = (now - g.1).saturating_add(g.0 as u64).max(1);
                *g = (0, 0);
                let b = (63 - distance.leading_zeros() as usize).min(BUCKETS - 1);
                a.hist[b] = a.hist[b].saturating_add(UNIT);
                a.mistakes = a.mistakes.saturating_add(1);
                a.mistakes_total = a.mistakes_total.saturating_add(1);
                a.extra = a.learned_extra(a.peak.saturating_sub(headroom));
            }
        }
    }

    /// A live Large span of `usable` bytes was freed.
    #[inline]
    pub(in crate::alloc_core) fn large_cache_note_free(&mut self, usable: usize) {
        if let Some(e) = self.adaptive_ext() {
            e.adapt.live = e.adapt.live.saturating_sub(usable);
        }
    }

    /// A span of `usable` bytes left the cache (or was refused) under slot or
    /// budget pressure: remember it.
    #[inline]
    pub(in crate::alloc_core) fn large_cache_note_ghost(&mut self, usable: usize) {
        if let Some(e) = self.adaptive_ext() {
            let a = &mut e.adapt;
            a.given_back = a.given_back.saturating_add(usable as u64);
            a.ghost[a.ghost_next] = (usable, a.given_back);
            a.ghost_next = (a.ghost_next + 1) % GHOST_N;
        }
    }

    /// Open more extension slots while mistakes are remembered and the learned
    /// target has room for a deposit of `usable` bytes on top of `used`.
    /// Returns whether it grew.
    #[inline]
    pub(in crate::alloc_core) fn large_cache_try_grow_slots(
        &mut self,
        used: usize,
        usable: usize,
    ) -> bool {
        let headroom = self.decay_config.headroom_bytes;
        let Some(e) = self.adaptive_ext() else {
            return false;
        };
        let target = headroom.saturating_add(e.adapt.extra);
        if !e.adapt.learning()
            || used.saturating_add(usable) > target
            || e.active >= LARGE_CACHE_EXTENDED_SLOTS
        {
            return false;
        }
        e.active = (e.active + SLOT_STEP).min(LARGE_CACHE_EXTENDED_SLOTS);
        true
    }

    /// The byte budget a deposit is checked against: the configured budget, or
    /// none while the policy is on and the caller did not set one.
    #[inline]
    pub(in crate::alloc_core) fn large_cache_effective_budget(&self) -> Option<usize> {
        let budget = self.large_cache_budget_bytes?;
        if self.decay_config.adaptive && self.decay_config.adaptive_budget {
            // No budget the caller did not ask for: the base slots keep what the
            // production cache keeps, the extension slots open only under the
            // learned target.
            return None;
        }
        Some(budget)
    }

    /// One decay interval passed: forget 1/16 of every histogram weight (half
    /// if the interval was quiet: no mistake, hits under 1/8 of the extra),
    /// let the peak follow `live` down by 1/8 of the distance, recompute the
    /// extra, close slots the cache no longer needs.
    #[inline]
    pub(in crate::alloc_core) fn large_cache_adaptive_interval(&mut self) {
        let occupied_hi = self.large_cache_highest_occupied();
        let headroom = self.decay_config.headroom_bytes;
        if let Some(e) = self.adaptive_ext() {
            let a = &mut e.adapt;
            // Quiet: no mistake and the hits used under 1/8 of the learned extra.
            let quiet = a.mistakes == 0 && a.hit_bytes.saturating_mul(8) <= a.extra;
            for w in &mut a.hist {
                *w -= if quiet { w.div_ceil(2) } else { w.div_ceil(16) };
            }
            if quiet {
                // Unused retention: every distance moves down one bucket (the extra halves).
                a.hist[0] = a.hist[0].saturating_add(a.hist[1]);
                a.hist.copy_within(2.., 1);
                a.hist[BUCKETS - 1] = 0;
            }
            a.quiet = quiet;
            a.mistakes = 0;
            a.hit_bytes = 0;
            a.peak -= (a.peak - a.live.min(a.peak)) / 8;
            a.extra = a.learned_extra(a.peak.saturating_sub(headroom));
            if !a.learning() {
                // Back to the base slots once nothing above them is occupied.
                let floor = occupied_hi.map_or(0, |i| (i + 1).saturating_sub(LARGE_CACHE_SLOTS));
                e.active = e.active.min(floor.next_multiple_of(SLOT_STEP));
            }
        }
    }

    /// Diagnostics: `(extra, live, peak, mistakes_total, active extension slots)`.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_large_cache_adaptive(&self) -> Option<(usize, usize, usize, u64, usize)> {
        let e = self.adaptive_ref()?;
        Some((
            e.adapt.extra,
            e.adapt.live,
            e.adapt.peak,
            e.adapt.mistakes_total,
            e.active,
        ))
    }

    /// TEST-ONLY: switch the learned policy off (or back on) for this heap, for
    /// tests that pin the fixed 40-slot extension. Inert bookkeeping only.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_set_large_cache_adaptive(&mut self, on: bool) {
        self.decay_config.adaptive = on;
    }
}

#[cfg(all(feature = "alloc-decommit", not(feature = "large-cache-adaptive")))]
impl AllocCore {
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_retain_target(&self) -> usize {
        self.decay_config.headroom_bytes
    }
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_adaptive_pending(&self) -> bool {
        false
    }
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_note_hit(&mut self, _usable: usize) {}
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_note_miss(&mut self, _usable: usize) {}
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_note_free(&mut self, _usable: usize) {}
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_note_ghost(&mut self, _usable: usize) {}
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_try_grow_slots(
        &mut self,
        _used: usize,
        _usable: usize,
    ) -> bool {
        false
    }
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_effective_budget(&self) -> Option<usize> {
        self.large_cache_budget_bytes
    }
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_adaptive_interval(&mut self) {}
    #[inline(always)]
    pub(in crate::alloc_core) fn large_cache_decay_rate_bp(&self) -> usize {
        self.decay_config.decay_rate_bp as usize
    }
}
