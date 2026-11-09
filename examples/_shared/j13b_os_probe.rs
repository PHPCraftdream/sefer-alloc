//! Linux own-process window telemetry; no foreign sampling or THP policy writes.
use super::os_sample::OsSample;
use std::fs::{self, File};
use std::io::{self, Read};
use std::sync::{Mutex, MutexGuard, TryLockError};

static EPOCH: Mutex<()> = Mutex::new(());
const INPUT_LIMIT: u64 = 64 * 1024;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn decimal(text: &str) -> io::Result<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid("invalid unsigned OS counter"));
    }
    text.parse().map_err(|_| invalid("OS counter overflow"))
}

fn read_bounded(path: &str) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(INPUT_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_err(|_| invalid("OS input length overflow"))? > INPUT_LIMIT {
        return Err(invalid("OS input exceeds 64 KiB"));
    }
    Ok(bytes)
}

/// One process-wide RSS peak epoch. Only the controller owns this non-Send guard.
///
/// clear_refs=5 resets VmHWM to current RSS, not RSS itself or page referenced
/// bits (Linux >=4.0, CONFIG_PROC_PAGE_MONITOR). These calls run only in the
/// disposable measurement/CI child, outside allocator operations. The caller
/// quiesces workload workers at both boundaries and permits no external reset.
/// Peak scope includes the boundary probes themselves. Separate procfs reads
/// are not an atomic snapshot; faults are whole-process deltas, not child faults.
/// Linux documents VmRSS/VmHWM as inaccurate. A live hiwater read can include
/// current approximate RSS without persisting that maximum in stored hiwater;
/// the final reported counter need not bound every earlier reported reading.
pub(crate) struct OsProbe {
    _epoch: MutexGuard<'static, ()>,
    start_faults: (u64, u64),
    start_rss: u64,
    start_peak: u64,
}

impl OsProbe {
    pub(crate) fn begin() -> io::Result<Self> {
        if !cfg!(all(target_os = "linux", not(miri))) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native Linux window peak probe required",
            ));
        }
        let epoch = match EPOCH.try_lock() {
            Ok(epoch) => epoch,
            Err(TryLockError::WouldBlock) => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "overlapping OS window epoch",
                ))
            }
            Err(TryLockError::Poisoned(_)) => {
                return Err(io::Error::other("poisoned OS window epoch"))
            }
        };
        // The fixed self path cannot target another process. Value 5 changes
        // only the accounting high-water mark; values 1..3 are never used.
        fs::write("/proc/self/clear_refs", b"5")?;
        let (rss, peak) = Self::memory()?;
        let start_faults = Self::parse_faults(&read_bounded("/proc/self/stat")?)?;
        Ok(Self {
            _epoch: epoch,
            start_faults,
            start_rss: rss,
            start_peak: peak,
        })
    }

    pub(crate) fn start_peak_bytes(&self) -> u64 {
        self.start_peak
    }

    /// Read whole-process RSS and AnonHugePages while actors hold live payloads.
    /// Workers must be quiescent; these separate reads are not an atomic snapshot.
    pub(crate) fn live_memory(&self) -> io::Result<(u64, u64)> {
        let huge = read_bounded("/proc/self/smaps_rollup")?;
        let huge = std::str::from_utf8(&huge).map_err(|_| invalid("invalid smaps UTF-8"))?;
        let huge = Self::parse_anon_huge(huge)?;
        let (rss, _) = Self::memory()?;
        Ok((rss, huge))
    }

    /// Consumes the epoch once. Any error yields no sample and releases its lease.
    pub(crate) fn finish(self) -> io::Result<OsSample> {
        let huge = read_bounded("/proc/self/smaps_rollup")?;
        let huge = std::str::from_utf8(&huge).map_err(|_| invalid("invalid smaps UTF-8"))?;
        let anon_huge_pages_bytes = Self::parse_anon_huge(huge)?;
        let end_faults = Self::parse_faults(&read_bounded("/proc/self/stat")?)?;
        let (rss_end_bytes, rss_peak_bytes) = Self::memory()?;
        let (minor_faults, major_faults) = Self::fault_delta(self.start_faults, end_faults)?;
        Ok(OsSample {
            rss_start_bytes: self.start_rss,
            rss_end_bytes,
            rss_peak_bytes,
            anon_huge_pages_bytes,
            minor_faults,
            major_faults,
            peak_scope: "OS-reported approximate VmHWM since clear_refs=5, including probes",
            fault_scope: "own-process stat 10/12 delta before final RSS read; excludes children",
        })
    }

    fn memory() -> io::Result<(u64, u64)> {
        let memory =
            proc_memstat::try_snapshot().map_err(|error| io::Error::other(error.to_string()))?;
        let peak = memory
            .peak_rss
            .ok_or_else(|| invalid("OS peak RSS unavailable"))?;
        if peak < memory.rss {
            return Err(invalid("OS RSS/peak relation inconsistent"));
        }
        Ok((memory.rss, peak))
    }

    /// Kernel ABI: fields 10/12 are process minflt/majflt; 11/13 are children.
    /// The comm bytes may contain spaces, parentheses and non-UTF-8 bytes.
    pub(crate) fn parse_faults(bytes: &[u8]) -> io::Result<(u64, u64)> {
        let open = bytes
            .iter()
            .position(|byte| *byte == b'(')
            .ok_or_else(|| invalid("missing stat comm"))?;
        let close = bytes
            .iter()
            .rposition(|byte| *byte == b')')
            .ok_or_else(|| invalid("missing stat comm terminator"))?;
        if close <= open {
            return Err(invalid("invalid stat comm bounds"));
        }
        let pid = std::str::from_utf8(&bytes[..open])
            .map_err(|_| invalid("invalid stat PID"))?
            .trim();
        if decimal(pid)? == 0 {
            return Err(invalid("zero stat PID"));
        }
        let tail = std::str::from_utf8(&bytes[close + 1..])
            .map_err(|_| invalid("invalid stat counter text"))?;
        let mut fields = tail.split_ascii_whitespace();
        let state = fields.next().ok_or_else(|| invalid("missing stat state"))?;
        if state.len() != 1 || !state.as_bytes()[0].is_ascii_alphabetic() {
            return Err(invalid("invalid stat state"));
        }
        for _ in 0..6 {
            fields
                .next()
                .ok_or_else(|| invalid("truncated stat prefix"))?;
        }
        let minor = decimal(fields.next().ok_or_else(|| invalid("missing minflt"))?)?;
        fields.next().ok_or_else(|| invalid("missing cminflt"))?;
        let major = decimal(fields.next().ok_or_else(|| invalid("missing majflt"))?)?;
        Ok((minor, major))
    }

    pub(crate) fn parse_anon_huge(text: &str) -> io::Result<u64> {
        let mut value = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("AnonHugePages:") {
                if value.is_some() {
                    return Err(invalid("duplicate AnonHugePages"));
                }
                let mut fields = rest.split_ascii_whitespace();
                let kib = decimal(
                    fields
                        .next()
                        .ok_or_else(|| invalid("missing huge-page counter"))?,
                )?;
                if fields.next() != Some("kB") || fields.next().is_some() {
                    return Err(invalid("invalid AnonHugePages unit/fields"));
                }
                value = Some(
                    kib.checked_mul(1024)
                        .ok_or_else(|| invalid("huge-page byte conversion overflow"))?,
                );
            }
        }
        value.ok_or_else(|| invalid("AnonHugePages unavailable"))
    }

    pub(crate) fn fault_delta(before: (u64, u64), after: (u64, u64)) -> io::Result<(u64, u64)> {
        let minor = after
            .0
            .checked_sub(before.0)
            .ok_or_else(|| invalid("minor fault counter decreased/wrapped"))?;
        let major = after
            .1
            .checked_sub(before.1)
            .ok_or_else(|| invalid("major fault counter decreased/wrapped"))?;
        Ok((minor, major))
    }
}
