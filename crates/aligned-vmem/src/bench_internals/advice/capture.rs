//! Native diagnostic namespace; emitter wiring is separate from pure log mechanics.
#![forbid(unsafe_code)]
use super::stream::{
    AdviceCaptureSnapshot, AdviceEvent, AdviceEventLog, AdviceReleaseMarker, AdviceReleaseScope,
};

static EVENTS: AdviceEventLog<65536> = AdviceEventLog::new();

/// One append-only capture per disposable native child. The controller starts it
/// before any payload allocation and quiesces/joins producers before reading.
/// No API here proves those premises or authenticates caller-supplied events.
/// Collection uses the process allocator; the judge must keep it on System.
pub struct AdviceCapture;

impl AdviceCapture {
    pub const fn native_available() -> bool {
        cfg!(all(
            any(target_os = "linux", target_os = "android"),
            not(miri),
            not(aligned_vmem_mock)
        ))
    }

    fn native() -> Result<(), &'static str> {
        if Self::native_available() {
            Ok(())
        } else {
            Err("advice lifetime capture requires native Linux/Android")
        }
    }

    pub fn begin_native() -> Result<(), &'static str> {
        Self::native()?;
        EVENTS.begin()
    }

    /// Allocation-free inert event recording; never return a diagnostic failure
    /// into allocator policy, change a native outcome or retry a native operation.
    pub fn record(event: Result<AdviceEvent, &'static str>) {
        if Self::native_available() {
            EVENTS.record(event);
        }
    }

    pub(crate) fn context_lost() {
        Self::record(Err("release source context unavailable"));
    }

    /// Inert thread-affine classification at the real Large release boundary.
    /// The guard changes no mapping, policy or release outcome.
    pub fn large_release_scope(reservation: usize, len: usize) -> Option<AdviceReleaseScope> {
        if !Self::native_available() || !EVENTS.is_started() {
            return None;
        }
        match AdviceReleaseScope::enter(reservation, len) {
            Ok(scope) => Some(scope),
            Err(_) => {
                Self::context_lost();
                None
            }
        }
    }

    /// Capture before the actual release attempt, while its original key is live.
    pub fn release_begin(reservation: usize, len: usize) -> Option<AdviceReleaseMarker> {
        if Self::native().is_err() {
            return None;
        }
        let begin = EVENTS.release_begin_hint()?;
        match AdviceReleaseScope::matches(reservation, len) {
            Ok(large) => Some(AdviceReleaseMarker { begin, large }),
            Err(_) => {
                Self::context_lost();
                None
            }
        }
    }

    pub fn release_end(
        marker: Option<AdviceReleaseMarker>,
        reservation: usize,
        len: usize,
        confirmed: bool,
    ) {
        if let Some(marker) = marker {
            let event = if marker.large {
                AdviceEvent::large_released_at(reservation, len, confirmed, marker.begin)
            } else {
                AdviceEvent::released_at(reservation, len, confirmed, marker.begin)
            };
            Self::record(event);
        }
    }

    pub fn boundary() -> Result<u64, &'static str> {
        Self::native()?;
        EVENTS.boundary()
    }

    /// Caller keeps producers quiescent throughout collection. Scalar publication
    /// checks cannot prove the absence of future writers or external allocator work.
    pub fn snapshot() -> Result<AdviceCaptureSnapshot, &'static str> {
        let boundary = Self::boundary()?;
        let count = usize::try_from(boundary).map_err(|_| "capture count conversion overflow")?;
        let mut records = Vec::new();
        records
            .try_reserve_exact(count)
            .map_err(|_| "capture metadata allocation failed")?;
        for index in 0..boundary {
            records.push(EVENTS.read(index, boundary)?);
        }
        if Self::boundary()? != boundary {
            return EVENTS.changed_boundary();
        }
        Ok(AdviceCaptureSnapshot {
            encoding_version: AdviceEvent::ENCODING_VERSION,
            records,
            boundary,
            slot_storage_bytes: AdviceEventLog::<65536>::slot_storage_bytes(),
            total_storage_bytes: AdviceEventLog::<65536>::total_storage_bytes(),
            tls_context_bytes_per_thread: AdviceReleaseScope::scalar_storage_bytes_per_thread(),
        })
    }
}
