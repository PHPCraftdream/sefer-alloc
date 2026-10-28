// expect: is not preceded
impl AllocCore {
    pub(crate) fn finish_small_drain(
        &mut self,
        base: *mut u8,
        index: usize,
        changed_classes: u64,
    ) -> DrainEnd {
        #[cfg(feature = "alloc-decommit")]
        if changed_classes != 0 && Self::dec_live_and_maybe_decommit(base, self.small_cur) {
            let pooled = self.release_or_pool_empty_segment(base);
            return if pooled {
                DrainEnd::Pooled
            } else {
                DrainEnd::Released
            };
        }
        DrainEnd::Live
    }

    pub(crate) fn drain_sidecar_ingress_bounded(&mut self, cursor: &mut (usize, usize), budget: usize) -> (usize, usize) {
        while units < budget {
            if !matches!(self.finish_small_drain(base, index, changed_classes), DrainEnd::Live) {
                cursor.0 += 1;
            }
            let (changed_classes, cut_reclaimed) = {
                let scan = self.scan_small_route_or_fatal(index, base, high_water, cursor.1, b"x\n");
                Self::consume_one_small_cut(base, scan, NoSmallCutObserver)
            };
            reclaimed += cut_reclaimed;
        }
        (reclaimed, units)
    }

    pub(crate) fn drain_sidecar_ingress(&mut self) -> usize {
        for index in 0..end {
            let (changed_classes, drained_records) = {
                let scan = self.scan_small_route_or_fatal(index, base, high_water, start, b"x\n");
                Self::consume_small_cuts(base, scan, StrictTrimObserver::default())
            };
            reclaimed += drained_records;
            self.finish_small_drain(base, index, changed_classes);
        }
        reclaimed
    }

    fn drain_segment_sidecar(&mut self, base: *mut u8) -> SidecarDrainOutcome {
        let (changed_classes, _reclaimed) = {
            let scan = self.scan_small_route_or_fatal(index, base, high_water, start_word, b"x\n");
            Self::consume_small_cuts(base, scan, U31NoPendingObserver::default())
        };
        if changed_classes == 0 {
            return SidecarDrainOutcome::Skipped;
        }
        match self.finish_small_drain(base, index, changed_classes) {
            DrainEnd::Live => SidecarDrainOutcome::Drained,
            _ => SidecarDrainOutcome::Released,
        }
    }
}
