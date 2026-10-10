use core::sync::atomic::{AtomicU64, Ordering};

use super::{sidecar_bitmap::ClassMap, BitmapCut};

#[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
std::thread_local! {
    static SCAN_COUNTS: core::cell::Cell<Option<[u64; 5]>> = const { core::cell::Cell::new(None) };
    static BASELINE_SCAN: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
}

/// A fixed high-water scan. One cut is taken per word, including empty words.
#[must_use = "finish the fixed scan or persist the remaining cursor"]
pub(crate) struct BitmapScan<'a> {
    pub(super) pending: core::slice::Iter<'a, AtomicU64>,
    pub(super) classes: ClassMap<'a>,
    pub(super) next_word: usize,
    pub(super) end_word: usize,
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

    pub(crate) fn next_cut(&mut self) -> Option<BitmapCut<'a>> {
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
