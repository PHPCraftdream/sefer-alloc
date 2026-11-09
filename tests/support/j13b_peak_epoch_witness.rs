//! Native-witness signal contract, not an accuracy bound for Linux RSS counters.
#[derive(Clone, Copy)]
pub(crate) struct PeakEpochWitness {
    pub old_peak: u64,
    pub reset_peak: u64,
    pub start_rss: u64,
    pub live_peak: u64,
    pub end_rss: u64,
    pub retained_peak: u64,
}

impl PeakEpochWitness {
    pub(crate) fn validate(self) -> Result<(), &'static str> {
        const MIB: u64 = 1024 * 1024;
        let gap = |high: u64, low: u64, minimum: u64| {
            high.checked_sub(low).is_some_and(|value| value > minimum)
        };
        // The 32/16 MiB mapping signals and 16/8 MiB gaps were fixed before
        // measurements. No tolerance is inferred from a failing reading.
        if !gap(self.old_peak, self.reset_peak, 16 * MIB) {
            return Err("old peak was not reset by the existing signal gap");
        }
        if !gap(self.live_peak, self.start_rss, 8 * MIB) {
            return Err("live mapping did not produce the existing signal gap");
        }
        if !gap(self.old_peak, self.retained_peak, 8 * MIB) {
            return Err("prior lifetime peak leaked into the window");
        }
        if !gap(self.retained_peak, self.end_rss, 8 * MIB) {
            return Err("freed in-window peak was lost or replaced by end RSS");
        }
        Ok(())
    }
}
