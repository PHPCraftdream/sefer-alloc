use core::sync::atomic::{AtomicU64, Ordering};

use super::{sidecar_bitmap::ClassMap, BitmapCut};

#[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
std::thread_local! {
    static SCAN_COUNTS: core::cell::Cell<Option<[u64; 5]>> = const { core::cell::Cell::new(None) };
    static BASELINE_SCAN: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
}

/// A fixed high-water scan, or internal discovery-only summarized scan.
/// Fixed mode takes one cut per word, including empty words.
#[must_use = "finish the fixed scan or persist the remaining cursor"]
pub(crate) struct BitmapScan<'a> {
    pub(super) pending: core::slice::Iter<'a, AtomicU64>,
    pub(super) classes: ClassMap<'a>,
    pub(super) next_word: usize,
    pub(super) end_word: usize,
    // (summary, pending backing, next summary word, current word, mask, start).
    #[allow(clippy::type_complexity)] // One private scan cursor; no heap allocation or public API.
    pub(super) summarized: Option<(&'a [AtomicU64], &'a [AtomicU64], usize, usize, u64, usize)>,
    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    pub(super) summary_visited: u64,
}

impl<'a> BitmapScan<'a> {
    #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
    pub(crate) fn measure_scans(enable: bool, baseline: bool) -> [u64; 5] {
        BASELINE_SCAN.with(|mode| mode.set(baseline));
        SCAN_COUNTS.with(|counts| counts.replace(enable.then_some([0; 5])).unwrap_or([0; 5]))
    }

    /// Count only successful allocation-discovery route scan constructions.
    #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
    pub(crate) fn count_routed_scan() {
        SCAN_COUNTS.with(|counts| {
            if let Some(mut current) = counts.get() {
                current[4] += 1;
                counts.set(Some(current));
            }
        });
    }

    #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
    pub(crate) fn summary_words_visited(&self) -> u64 {
        self.summary_visited
    }

    pub(crate) fn next_cut(&mut self) -> Option<BitmapCut<'a>> {
        if let Some((summary, pending, next, current, mask, start)) = &mut self.summarized {
            loop {
                if *mask != 0 {
                    let word = *current * 64 + mask.trailing_zeros() as usize;
                    *mask &= *mask - 1;
                    return Some(BitmapCut {
                        classes: self.classes,
                        word,
                        bits: pending[word].swap(0, Ordering::AcqRel),
                    });
                }
                if *next >= self.end_word.div_ceil(64) || *start == self.end_word {
                    return None;
                }
                *current = *next;
                *next += 1;
                // Clear BEFORE pending cuts: a publisher after a word cut
                // acquires that cut's release sequence. This clear then happens
                // before its conditional load, which sees zero or a newer hint.
                let selected = summary[*current].swap(0, Ordering::AcqRel);
                #[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
                {
                    self.summary_visited += 1;
                }
                let base = *current * 64;
                let lo = start.saturating_sub(base).min(64);
                let hi = self.end_word.saturating_sub(base).min(64);
                let low_mask = if lo == 64 { 0 } else { u64::MAX << lo };
                let high_mask = if hi == 64 { u64::MAX } else { (1u64 << hi) - 1 };
                // Out-of-range bits are dropped, never exchanged or reinserted.
                *mask = selected & low_mask & high_mask;
            }
        }
        if self.next_word == self.end_word {
            return None;
        }
        let word = self.next_word;
        self.next_word += 1;
        let pending = self.pending.next().unwrap_or_else(|| std::process::abort());
        #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
        let baseline = BASELINE_SCAN.with(core::cell::Cell::get);
        #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
        let (bits, empty, exchanged) = if baseline {
            let bits = pending.swap(0, Ordering::AcqRel);
            (bits, bits == 0, true)
        } else if pending.load(Ordering::Acquire) == 0 {
            (0, true, false)
        } else {
            (pending.swap(0, Ordering::AcqRel), false, true)
        };
        #[cfg(not(all(feature = "bench-internals", r18_sidecar_scan_bench)))]
        let bits = pending.swap(0, Ordering::AcqRel);
        #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
        SCAN_COUNTS.with(|counts| {
            if let Some(mut current) = counts.get() {
                current[0] += 1;
                current[1] += u64::from(empty);
                current[2] += u64::from(exchanged);
                current[3] += u64::from(bits != 0);
                counts.set(Some(current));
            }
        });
        Some(BitmapCut {
            classes: self.classes,
            word,
            bits,
        })
    }
}
