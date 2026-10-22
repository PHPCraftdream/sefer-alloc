use super::sidecar_bitmap::ChunkedBank;
use super::{AtomicU64, BitmapCut, Ordering};
use crate::alloc_core::fatal::fatal;

/// Positions of the [`SCAN_COUNTS`] benchmark array (`measure_scans`'s
/// return shape, positional and pinned by tests — do not reorder).
#[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
#[repr(usize)]
enum ScanCount {
    Cut = 0,
    Empty = 1,
    Exchanged = 2,
    NonEmptyBits = 3,
    Routed = 4,
}

#[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
std::thread_local! {
    static SCAN_COUNTS: core::cell::Cell<Option<[u64; 5]>> = const { core::cell::Cell::new(None) };
    static BASELINE_SCAN: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
}

// B4 production-branch RMW counter (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS):
// counts the `swap(0, AcqRel)` executions of the production `next_cut`
// branch, so an integration test can prove the B4 load-gate actually skips
// the RMW on zero words. Lives OUTSIDE `r18_sidecar_scan_bench` because the
// production branch does not exist under it; readers are source-included
// integration tests (same pattern as r18 `SCAN_COUNTS`, read by the bench
// harness), hence the point-wise `allow(dead_code)` below for lib builds
// with `bench-internals` and no inside-crate reader.
#[cfg(all(feature = "bench-internals", not(r18_sidecar_scan_bench)))]
std::thread_local! {
    static B4_PROD_SCAN_RMW: core::cell::Cell<Option<u64>> = const { core::cell::Cell::new(None) };
}

/// A fixed high-water scan. One cut is taken per word, including empty words.
#[must_use = "finish the fixed scan or persist the remaining cursor"]
pub(crate) struct BitmapScan<'a> {
    pub(super) pending: core::slice::Iter<'a, AtomicU64>,
    pub(super) chunked: Option<&'a ChunkedBank>,
    pub(super) next_word: usize,
    pub(super) end_word: usize,
}

impl<'a> BitmapScan<'a> {
    pub(crate) fn from_chunked(
        bank: &'a ChunkedBank,
        high_water: usize,
        word: usize,
    ) -> Option<Self> {
        let end_word = ChunkedBank::window_end(high_water, word)?;
        Some(Self {
            pending: [].iter(),
            chunked: Some(bank),
            next_word: word,
            end_word,
        })
    }

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
                current[ScanCount::Routed as usize] += 1;
                counts.set(Some(current));
            }
        });
    }

    /// B4: arm/disarm the production-branch RMW-swap counter ([`B4_PROD_SCAN_RMW`]).
    /// Mirrors [`Self::measure_scans`]: returns the previous value, `Some(0)`
    /// while armed, `None` (counter inert) when disarmed.
    #[cfg(all(feature = "bench-internals", not(r18_sidecar_scan_bench)))]
    #[cfg_attr(
        all(feature = "bench-internals", not(r18_sidecar_scan_bench)),
        allow(dead_code)
    )]
    // readers are source-included integration tests (same pattern as the r18
    // SCAN_COUNTS, read by the bench harness), not lib-crate code.
    pub(crate) fn measure_rmw_swaps(enable: bool) -> u64 {
        B4_PROD_SCAN_RMW.with(|count| count.replace(enable.then_some(0)).unwrap_or(0))
    }

    pub(crate) fn next_cut(&mut self) -> Option<BitmapCut<'a>> {
        if self.next_word == self.end_word {
            return None;
        }
        let word = self.next_word;
        self.next_word += 1;
        let pending =
            if self.chunked.is_none() {
                Some(self.pending.next().unwrap_or_else(|| {
                    fatal(b"bitmap_scan: sidecar scan iterator exhausted early\n")
                }))
            } else {
                None
            };
        let load = || match self.chunked {
            Some(bank) => bank.pending_load(word),
            None => pending.unwrap().load(Ordering::Acquire),
        };
        let swap = || match self.chunked {
            Some(bank) => bank.pending_swap(word),
            None => pending.unwrap().swap(0, Ordering::AcqRel),
        };
        #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
        let baseline = BASELINE_SCAN.with(core::cell::Cell::get);
        #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
        let (bits, empty, exchanged) = if baseline {
            let bits = swap();
            (bits, bits == 0, true)
        } else if load() == 0 {
            (0, true, false)
        } else {
            (swap(), false, true)
        };
        #[cfg(not(all(feature = "bench-internals", r18_sidecar_scan_bench)))]
        // B4 (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS): a zero word can never yield
        // a record; read it plain and pay the RMW only when bits are present.
        // The discovery scan is one-pass, so a bit published after this read
        // was already outside this scan's reach under the unconditional swap —
        // it stays pending for the next discovery scan or strict trim. Release
        // is credit-gated (`dec_live_and_maybe_decommit`), so a skipped word is
        // never lost.
        let bits = if load() == 0 {
            0
        } else {
            #[cfg(all(feature = "bench-internals", not(r18_sidecar_scan_bench)))]
            B4_PROD_SCAN_RMW.with(|count| {
                if let Some(current) = count.get() {
                    count.set(Some(current + 1));
                }
            });
            swap()
        };
        #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
        SCAN_COUNTS.with(|counts| {
            if let Some(mut current) = counts.get() {
                current[ScanCount::Cut as usize] += 1;
                current[ScanCount::Empty as usize] += u64::from(empty);
                current[ScanCount::Exchanged as usize] += u64::from(exchanged);
                current[ScanCount::NonEmptyBits as usize] += u64::from(bits != 0);
                counts.set(Some(current));
            }
        });
        Some(BitmapCut {
            _marker: core::marker::PhantomData,
            word,
            bits,
        })
    }
}
