//! Ordered replay outside allocator operations; addresses remain inert metadata.
#![forbid(unsafe_code)]
use super::ledger::AdviceLifetimeLedger;
use super::replay_snapshot::AdviceReplaySnapshot;
use super::stream::AdviceEvent;

type ReplayResult<T> = Result<T, &'static str>;

/// One replay from event0; preserve this state across window boundaries and cache
/// reuse. This validates stream closure, not source authenticity or OS outcomes.
pub struct AdviceReplay {
    ledger: AdviceLifetimeLedger,
    fresh: [Option<AdviceEvent>; AdviceLifetimeLedger::CAPACITY],
    fresh_positions: [u64; AdviceLifetimeLedger::CAPACITY],
    accepted_positions: [Option<u64>; AdviceLifetimeLedger::CAPACITY],
    totals: AdviceReplaySnapshot,
    failure: Option<&'static str>,
}

impl Default for AdviceReplay {
    fn default() -> Self {
        Self::new()
    }
}

impl AdviceReplay {
    pub const fn new() -> Self {
        Self {
            ledger: AdviceLifetimeLedger::new(),
            fresh: [None; AdviceLifetimeLedger::CAPACITY],
            fresh_positions: [0; AdviceLifetimeLedger::CAPACITY],
            accepted_positions: [None; AdviceLifetimeLedger::CAPACITY],
            totals: AdviceReplaySnapshot {
                advice: super::snapshot::AdviceLifetimeSnapshot::EMPTY,
                requests: [[0; 3]; 6],
                requested_bytes: [0; 6],
                usable_span_bytes: [0; 6],
                reservation_bytes: [0; 6],
                outstanding_fresh: 0,
                events: 0,
                canonical_releases: 0,
            },
            failure: None,
        }
    }

    fn supported(&self) -> ReplayResult<()> {
        if let Some(message) = self.failure {
            return Err(message);
        }
        self.ledger.snapshot().map(|_| ())
    }

    fn reject<T>(&mut self, message: &'static str) -> ReplayResult<T> {
        if self.failure.is_none() {
            self.failure = Some(message);
        }
        Err(self.failure.unwrap_or(message))
    }

    pub fn snapshot(&self) -> ReplayResult<AdviceReplaySnapshot> {
        self.supported()?;
        let mut result = self.totals;
        result.advice = self.ledger.snapshot()?;
        result.outstanding_fresh = self.fresh.iter().flatten().count();
        Ok(result)
    }

    fn find(&self, reservation: u64) -> Option<usize> {
        self.fresh.iter().position(|entry| {
            entry.is_some_and(|event| event.words()[AdviceEvent::RESERVATION] == reservation)
        })
    }

    fn add(left: u64, right: u64) -> ReplayResult<u64> {
        left.checked_add(right)
            .ok_or("advice replay counter overflow")
    }

    fn fresh(&mut self, event: AdviceEvent) -> ReplayResult<()> {
        let words = event.words();
        if self.find(words[AdviceEvent::RESERVATION]).is_some() {
            return Err("duplicate fresh mapping event");
        }
        let end = words[AdviceEvent::RESERVATION]
            .checked_add(words[AdviceEvent::RESERVATION_LEN])
            .ok_or("fresh mapping end overflow")?;
        if self.fresh.iter().flatten().any(|existing| {
            let old = existing.words();
            old[AdviceEvent::RESERVATION] < end
                && old[AdviceEvent::RESERVATION]
                    .checked_add(old[AdviceEvent::RESERVATION_LEN])
                    .is_some_and(|old_end| words[AdviceEvent::RESERVATION] < old_end)
        }) {
            return Err("overlapping fresh mapping events");
        }
        let index = self
            .fresh
            .iter()
            .position(Option::is_none)
            .ok_or("fresh mapping capture capacity exhausted")?;
        let group = event.group().ok_or("fresh event group missing")?;
        let mode = event.mode().ok_or("fresh event mode missing")?;
        let mut totals = self.totals;
        totals.requests[group][0] = Self::add(totals.requests[group][0], 1)?;
        if event.threshold_eligible() {
            totals.requests[group][1] = Self::add(totals.requests[group][1], 1)?;
            if mode != 0 {
                totals.requests[group][2] = Self::add(totals.requests[group][2], 1)?;
            }
        }
        totals.requested_bytes[group] =
            Self::add(totals.requested_bytes[group], words[AdviceEvent::REQUESTED])?;
        totals.usable_span_bytes[group] = Self::add(
            totals.usable_span_bytes[group],
            words[AdviceEvent::USABLE_LEN],
        )?;
        totals.reservation_bytes[group] = Self::add(
            totals.reservation_bytes[group],
            words[AdviceEvent::RESERVATION_LEN],
        )?;
        self.fresh[index] = Some(event);
        self.fresh_positions[index] = self.totals.events;
        self.accepted_positions[index] = None;
        self.totals = totals;
        Ok(())
    }

    fn advice(&mut self, event: AdviceEvent) -> ReplayResult<()> {
        let words = event.words();
        let index = self
            .find(words[AdviceEvent::RESERVATION])
            .ok_or("advice has no fresh mapping event")?;
        let original = self.fresh[index].ok_or("fresh mapping event missing")?;
        let fresh = original.words();
        if words[1..6] != fresh[1..6]
            || original.mode() != event.mode()
            || !original.threshold_eligible()
        {
            return Err("advice differs from fresh mapping disposition");
        }
        if event.kind() == AdviceEvent::UNKNOWN_ADVICE {
            return self.ledger.unknown_advice();
        }
        let number =
            |value| usize::try_from(value).map_err(|_| "replay geometry exceeds native width");
        self.ledger.accepted(
            number(words[AdviceEvent::RESERVATION])?,
            number(words[AdviceEvent::RESERVATION_LEN])?,
            number(words[AdviceEvent::USABLE])?,
            number(words[AdviceEvent::USABLE_LEN])?,
            number(words[AdviceEvent::VALUE])?,
            event.group().ok_or("advice group missing")?,
        )?;
        self.accepted_positions[index] = Some(self.totals.events);
        Ok(())
    }

    fn release(&mut self, event: AdviceEvent) -> ReplayResult<()> {
        let words = event.words();
        let Some(index) = self.find(words[AdviceEvent::RESERVATION]) else {
            return Ok(());
        };
        if let Some(begin) = event.release_begin() {
            if !event.large_release() {
                return Ok(());
            }
            if begin > self.totals.events {
                return Err("release marker is after its observed position");
            }
            // A late result for an older, untracked VM generation must not retire
            // a newer Large mapping that subsequently obtained the same address.
            if self.fresh_positions[index] >= begin {
                return Ok(());
            }
            if self.accepted_positions[index].is_some_and(|position| position >= begin) {
                return Err("release began before original advice publication");
            }
        }
        let original = self.fresh[index]
            .ok_or("release fresh mapping missing")?
            .words();
        if original[AdviceEvent::RESERVATION_LEN] != words[AdviceEvent::RESERVATION_LEN] {
            return Err("terminal release geometry differs from fresh mapping");
        }
        let reservation = usize::try_from(words[AdviceEvent::RESERVATION])
            .map_err(|_| "replay release address overflow")?;
        let len = usize::try_from(words[AdviceEvent::RESERVATION_LEN])
            .map_err(|_| "replay release length overflow")?;
        self.ledger
            .released(reservation, len, event.confirmed_release())?;
        if !event.confirmed_release() {
            return Err("fresh mapping terminal outcome unproven");
        }
        self.fresh[index] = None;
        Ok(())
    }

    fn overlap(&self, event: AdviceEvent) -> Option<usize> {
        let words = event.words();
        let end =
            words[AdviceEvent::RESERVATION].checked_add(words[AdviceEvent::RESERVATION_LEN])?;
        self.fresh.iter().position(|entry| {
            entry.is_some_and(|old| {
                let old = old.words();
                old[AdviceEvent::RESERVATION] < end
                    && old[AdviceEvent::RESERVATION]
                        .checked_add(old[AdviceEvent::RESERVATION_LEN])
                        .is_some_and(|old_end| words[AdviceEvent::RESERVATION] < old_end)
            })
        })
    }

    /// Replay a complete quiescent native prefix. A callback can arrive after a
    /// subsequent mmap reused its range. Only one confirmed, begin-marked release
    /// can resolve that overlap; unknown/ambiguous outcomes never repair evidence.
    /// Scratch indexing allocates outside allocator operations, on caller System.
    pub fn from_capture(capture: &super::stream::AdviceCaptureSnapshot) -> ReplayResult<Self> {
        let count = capture.records.len();
        if capture.encoding_version != AdviceEvent::ENCODING_VERSION
            || count > 65536
            || u64::try_from(count).ok() != Some(capture.boundary)
        {
            return Err("unsupported native capture envelope");
        }
        let mut releases = Vec::new();
        releases
            .try_reserve_exact(count)
            .map_err(|_| "release index allocation failed")?;
        for (index, event) in capture.records.iter().enumerate() {
            if event.kind() == AdviceEvent::RELEASED {
                let begin = event
                    .release_begin()
                    .ok_or("native release has no begin marker")?;
                if begin > u64::try_from(index).map_err(|_| "native event position overflow")? {
                    return Err("release begin marker is after its result");
                }
                if event.large_release() {
                    releases.push(index);
                }
            }
        }
        releases.sort_unstable_by_key(|&index| {
            let event = capture.records[index];
            let words = event.words();
            (
                words[AdviceEvent::RESERVATION],
                words[AdviceEvent::RESERVATION_LEN],
                event.release_begin(),
                index,
            )
        });
        let mut consumed = Vec::new();
        consumed
            .try_reserve_exact(count)
            .map_err(|_| "release closure allocation failed")?;
        consumed.resize(count, false);
        let mut replay = Self::new();
        for (index, &event) in capture.records.iter().enumerate() {
            let position = u64::try_from(index).map_err(|_| "native event position overflow")?;
            if consumed[index] {
                replay.totals.events = Self::add(replay.totals.events, 1)?;
                continue;
            }
            if event.kind() == AdviceEvent::FRESH {
                for _ in 0..AdviceLifetimeLedger::CAPACITY {
                    let Some(slot) = replay.overlap(event) else {
                        break;
                    };
                    let original = replay.fresh[slot]
                        .ok_or("overlap original event missing")?
                        .words();
                    let minimum = replay.accepted_positions[slot]
                        .unwrap_or(replay.fresh_positions[slot])
                        .checked_add(1)
                        .ok_or("mapping generation position overflow")?;
                    let key = (
                        original[AdviceEvent::RESERVATION],
                        original[AdviceEvent::RESERVATION_LEN],
                    );
                    let lower = releases.partition_point(|&release| {
                        let result = capture.records[release];
                        let words = result.words();
                        (
                            words[AdviceEvent::RESERVATION],
                            words[AdviceEvent::RESERVATION_LEN],
                            result.release_begin(),
                        ) < (key.0, key.1, Some(minimum))
                    });
                    let mut candidate = None;
                    for &release in &releases[lower..] {
                        let result = capture.records[release];
                        let words = result.words();
                        if (
                            words[AdviceEvent::RESERVATION],
                            words[AdviceEvent::RESERVATION_LEN],
                        ) != key
                            || result.release_begin().is_none_or(|begin| begin > position)
                        {
                            break;
                        }
                        if release <= index || consumed[release] {
                            continue;
                        }
                        if !result.confirmed_release() {
                            return Err("unproven release cannot resolve address reuse");
                        }
                        if candidate.replace(release).is_some() {
                            return Err("ambiguous release markers for reused range");
                        }
                    }
                    let release = candidate
                        .ok_or("fresh range overlaps without a confirmed prior release")?;
                    replay.release(capture.records[release])?;
                    consumed[release] = true;
                    replay.totals.canonical_releases =
                        Self::add(replay.totals.canonical_releases, 1)?;
                }
                if replay.overlap(event).is_some() {
                    return Err("native address reuse closure exceeded bound");
                }
            }
            replay.push(position, event)?;
        }
        replay.supported()?;
        Ok(replay)
    }

    /// Every actual capture position is consumed once, including unrelated VM
    /// releases; gaps/duplicates and unsupported outcomes never become zero rows.
    pub fn push(&mut self, sequence: u64, event: AdviceEvent) -> ReplayResult<()> {
        self.supported()?;
        if sequence != self.totals.events {
            return self.reject("advice replay sequence gap or duplicate");
        }
        let Some(next) = sequence.checked_add(1) else {
            return self.reject("advice replay position overflow");
        };
        let result = match event.kind() {
            AdviceEvent::FRESH => self.fresh(event),
            AdviceEvent::ACCEPTED | AdviceEvent::UNKNOWN_ADVICE => self.advice(event),
            AdviceEvent::RELEASED => self.release(event),
            _ => Err("unsupported advice replay event"),
        };
        match result {
            Ok(()) => {
                self.totals.events = next;
                Ok(())
            }
            Err(message) => self.reject(message),
        }
    }
}
