//! Bounded inert metadata only; owns no mapping and dereferences no address.
#![forbid(unsafe_code)]
use super::snapshot::AdviceLifetimeSnapshot;

type EvidenceResult<T> = Result<T, &'static str>;

#[derive(Clone, Copy)]
struct Entry {
    reservation: usize,
    reservation_len: usize,
    advised: u64,
    group: usize,
}
impl Entry {
    const EMPTY: Self = Self {
        reservation: 0,
        reservation_len: 0,
        advised: 0,
        group: 0,
    };
}

/// Allocation-free pure accounting core, not a native event producer.
///
/// The caller supplies one ordered event stream: accepted advice before exposing
/// that mapping, and exactly one terminal release outcome with the original
/// reservation geometry. Cache/reuse/decommit are not terminal release events.
/// Integers here are inert keys, never pointer capabilities. This safe API cannot
/// establish that events occurred, or that a reported release was truly native.
/// Unknown outcomes or any unsupported stream permanently reject snapshots.
pub struct AdviceLifetimeLedger {
    entries: [Entry; Self::CAPACITY],
    totals: AdviceLifetimeSnapshot,
    failure: Option<&'static str>,
}

impl Default for AdviceLifetimeLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl AdviceLifetimeLedger {
    pub const CAPACITY: usize = 256;

    pub const fn new() -> Self {
        Self {
            entries: [Entry::EMPTY; Self::CAPACITY],
            totals: AdviceLifetimeSnapshot::EMPTY,
            failure: None,
        }
    }

    fn supported(&self) -> EvidenceResult<()> {
        match self.failure {
            Some(message) => Err(message),
            None => Ok(()),
        }
    }

    fn reject<T>(&mut self, message: &'static str) -> EvidenceResult<T> {
        if self.failure.is_none() {
            self.failure = Some(message);
        }
        Err(self.failure.unwrap_or(message))
    }

    /// A failed/unknown stream yields no zero-valued substitute or partial row.
    pub fn snapshot(&self) -> EvidenceResult<AdviceLifetimeSnapshot> {
        self.supported()?;
        Ok(self.totals)
    }

    /// Raw retention diagnostic, including after rejection; not readable evidence.
    pub fn retained_mapping_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.reservation != 0)
            .count()
    }

    fn accept_row(mut row: [u64; 4], bytes: u64) -> EvidenceResult<[u64; 4]> {
        row[AdviceLifetimeSnapshot::TOTAL] = row[AdviceLifetimeSnapshot::TOTAL]
            .checked_add(bytes)
            .ok_or("accepted advice total overflow")?;
        row[AdviceLifetimeSnapshot::NOW] = row[AdviceLifetimeSnapshot::NOW]
            .checked_add(bytes)
            .ok_or("accepted advice outstanding overflow")?;
        row[AdviceLifetimeSnapshot::PEAK] =
            row[AdviceLifetimeSnapshot::PEAK].max(row[AdviceLifetimeSnapshot::NOW]);
        Ok(row)
    }

    /// Record accepted prefix bytes inside a usable span and its original reservation.
    /// The address/length integers are checked geometrically, not dereferenced.
    pub fn accepted(
        &mut self,
        reservation: usize,
        reservation_len: usize,
        usable: usize,
        usable_len: usize,
        advised: usize,
        group: usize,
    ) -> EvidenceResult<()> {
        self.supported()?;
        let prepared = (|| {
            let end = reservation
                .checked_add(reservation_len)
                .ok_or("reservation geometry overflow")?;
            let usable_end = usable
                .checked_add(usable_len)
                .ok_or("usable geometry overflow")?;
            if reservation == 0
                || reservation_len == 0
                || usable_len == 0
                || advised == 0
                || group >= 6
                || usable < reservation
                || usable_end > end
                || advised > usable_len
            {
                return Err("invalid accepted advice geometry or group");
            }
            if self
                .entries
                .iter()
                .any(|entry| entry.reservation == reservation)
            {
                return Err("duplicate live advice reservation");
            }
            if self.entries.iter().any(|entry| {
                entry.reservation != 0
                    && entry.reservation < end
                    && entry
                        .reservation
                        .checked_add(entry.reservation_len)
                        .is_some_and(|entry_end| reservation < entry_end)
            }) {
                return Err("overlapping live advice reservations");
            }
            let index = self
                .entries
                .iter()
                .position(|entry| entry.reservation == 0)
                .ok_or("advice lifetime capacity exhausted")?;
            let advised = u64::try_from(advised).map_err(|_| "advice byte conversion overflow")?;
            let mut totals = self.totals;
            totals.groups[group] = Self::accept_row(totals.groups[group], advised)?;
            totals.process = Self::accept_row(totals.process, advised)?;
            Ok((
                index,
                Entry {
                    reservation,
                    reservation_len,
                    advised,
                    group,
                },
                totals,
            ))
        })();
        match prepared {
            Ok((index, entry, totals)) => {
                self.entries[index] = entry;
                self.totals = totals;
                Ok(())
            }
            Err(message) => self.reject(message),
        }
    }

    fn release_row(mut row: [u64; 4], bytes: u64) -> EvidenceResult<[u64; 4]> {
        row[AdviceLifetimeSnapshot::NOW] = row[AdviceLifetimeSnapshot::NOW]
            .checked_sub(bytes)
            .ok_or("accepted advice outstanding underflow")?;
        row[AdviceLifetimeSnapshot::RELEASED] = row[AdviceLifetimeSnapshot::RELEASED]
            .checked_add(bytes)
            .ok_or("released advice total overflow")?;
        Ok(row)
    }

    /// Only confirmed terminal release removes tracked bytes. Untracked mappings
    /// are ignored: this core records accepted Large advice, not every VM mapping.
    /// On an uncertain tracked release retain the entry and reject all later evidence;
    /// no later success or reused numeric address can authorize a retry or repair.
    pub fn released(
        &mut self,
        reservation: usize,
        reservation_len: usize,
        confirmed: bool,
    ) -> EvidenceResult<bool> {
        self.supported()?;
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.reservation == reservation && reservation != 0)
        else {
            return Ok(false);
        };
        let entry = self.entries[index];
        if reservation_len != entry.reservation_len {
            return self.reject("release geometry differs from original reservation");
        }
        if !confirmed {
            return self.reject("tracked native release outcome unproven");
        }
        let prepared = (|| {
            let mut totals = self.totals;
            totals.groups[entry.group] =
                Self::release_row(totals.groups[entry.group], entry.advised)?;
            totals.process = Self::release_row(totals.process, entry.advised)?;
            Ok(totals)
        })();
        match prepared {
            Ok(totals) => {
                self.entries[index] = Entry::EMPTY;
                self.totals = totals;
                Ok(true)
            }
            Err(message) => self.reject(message),
        }
    }

    /// A partial-effect advice error cannot be labelled as accepted zero bytes.
    pub fn unknown_advice(&mut self) -> EvidenceResult<()> {
        self.reject("native advice effect unproven")
    }
}
