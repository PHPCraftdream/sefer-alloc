//! Single-control evidence only; aggregate residency is never split into samples.
use super::control_snapshot::ControlSnapshot;
use std::io;

fn error(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Decoder, not an authenticity or quiescence proof. The caller must execute
/// a same-group, same-size fresh eligible interval (one request or a complete
/// aligned eight-request cohort), with no competing allocator traffic, then
/// consume every control at a quiescent terminal boundary.
/// Actual captured tickets establish uniqueness/selection; ordinal reads alone do not.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ControlInterval {
    pub eligible_fresh: u64,
    pub reasons: [u64; 5],
    pub resident_pages: u64,
    pub sampled_pages: u64,
    pub applied: u64,
    pub dropped: u64,
    pub target_share_q31: u64,
}

impl ControlInterval {
    pub(crate) fn close(
        group: usize,
        page: usize,
        before: &ControlSnapshot,
        after: &ControlSnapshot,
        tickets: &[u64],
    ) -> io::Result<Self> {
        Self::decode(group, page, before, after, tickets, 8)
    }

    /// Exactly one fresh request, selected or not, at any global ordinal.
    /// This avoids padding or discarding traffic at training phase boundaries.
    pub(crate) fn close_request(
        group: usize,
        page: usize,
        before: &ControlSnapshot,
        after: &ControlSnapshot,
        tickets: &[u64],
    ) -> io::Result<Self> {
        Self::decode(group, page, before, after, tickets, 1)
    }

    fn decode(
        group: usize,
        page: usize,
        before: &ControlSnapshot,
        after: &ControlSnapshot,
        tickets: &[u64],
        requests: usize,
    ) -> io::Result<Self> {
        const ONE: u64 = 1 << 31;
        let issued = u64::try_from(requests).map_err(|_| error("request count overflow"))?;
        if !matches!(requests, 1 | 8)
            || group >= 6
            || !page.is_power_of_two()
            || tickets.len() != requests
            || requests == 8 && !before.ordinal.is_multiple_of(8)
            || after.ordinal.checked_sub(before.ordinal) != Some(issued)
        {
            return Err(error("unsupported control cohort or page geometry"));
        }
        let mut seen = [false; 8];
        let mut selected = 0u64;
        for &entry in tickets {
            if entry & 1 == 0 {
                return Err(error("unsupported or invalid control ticket capture"));
            }
            let offset = (entry >> 2)
                .checked_sub(before.ordinal)
                .ok_or_else(|| error("ticket precedes observed cohort"))?;
            let index = usize::try_from(offset).map_err(|_| error("ticket index overflow"))?;
            if index >= requests || seen[index] {
                return Err(error("ticket outside cohort or duplicate ticket"));
            }
            seen[index] = true;
            selected += (entry >> 1) & 1;
        }
        if !seen[..requests].iter().all(|value| *value)
            || selected > 1
            || requests == 8 && selected != 1
        {
            return Err(error(
                "captured cohort does not contain exactly one control",
            ));
        }
        for index in 0..6 {
            if index != group
                && (before.controls[index] != after.controls[index]
                    || before.learning[index] != after.learning[index])
            {
                return Err(error("another request group changed during the interval"));
            }
            for row in [before.learning[index], after.learning[index]] {
                if row[..3].iter().any(|value| *value > ONE) {
                    return Err(error("learning pair/share outside Q31 range"));
                }
            }
        }
        let mut delta = [0u64; 8];
        for (index, value) in delta.iter_mut().enumerate() {
            *value = after.controls[group][index]
                .checked_sub(before.controls[group][index])
                .ok_or_else(|| error("control counter decreased or wrapped"))?;
        }
        let reasons = [delta[1], delta[2], delta[3], delta[4], delta[5]];
        let reason_count = reasons
            .iter()
            .try_fold(0u64, |sum, value| sum.checked_add(*value))
            .ok_or_else(|| error("control reason sum overflow"))?;
        if delta[0] != selected || reason_count != selected {
            return Err(error("selection and terminal reason closure failed"));
        }
        let valid = delta[2];
        let (resident_pages, sampled_pages) = (delta[6], delta[7]);
        let page = u64::try_from(page).map_err(|_| error("page conversion overflow"))?;
        let maximum_pages = 16 * 2 * 1024 * 1024 / page;
        if valid == 0 && (resident_pages != 0 || sampled_pages != 0)
            || valid == 1
                && (sampled_pages == 0
                    || resident_pages > sampled_pages
                    || sampled_pages > maximum_pages)
        {
            return Err(error("unsupported single-control residency evidence"));
        }
        let applied = after.learning[group][3]
            .checked_sub(before.learning[group][3])
            .ok_or_else(|| error("applied counter decreased or wrapped"))?;
        let dropped = after.learning[group][4]
            .checked_sub(before.learning[group][4])
            .ok_or_else(|| error("dropped counter decreased or wrapped"))?;
        if applied.checked_add(dropped) != Some(valid) {
            return Err(error(
                "valid observation and adaptive admission closure failed",
            ));
        }
        if applied == 0 && before.learning[group][..3] != after.learning[group][..3] {
            return Err(error(
                "learning state changed without an applied observation",
            ));
        }
        Ok(Self {
            eligible_fresh: issued,
            reasons,
            resident_pages,
            sampled_pages,
            applied,
            dropped,
            target_share_q31: after.learning[group][2],
        })
    }

    /// Exact class thresholds; a dropped estimator update is not a fake application.
    pub(crate) fn class_valid(&self, sparse: bool) -> bool {
        if self.reasons[1] != 1 {
            return false;
        }
        let resident_tenths = u128::from(self.resident_pages) * 10;
        if sparse {
            resident_tenths < u128::from(self.sampled_pages)
        } else {
            resident_tenths > u128::from(self.sampled_pages) * 9
        }
    }
}
