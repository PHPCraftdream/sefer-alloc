//! One measured window, not an admitted runtime row or campaign result.
use super::control_snapshot::ControlSnapshot;
use super::os_sample::OsSample;
use super::window_execution::WindowExecution;
use super::workload::Cell;

#[derive(Debug)]
pub(crate) struct MeasuredWindow {
    pub cell: Cell,
    /// False for any geometry different from the complete registered scenario.
    pub registered_geometry: bool,
    pub os: OsSample,
    /// Batch-only quiescent diagnostics immediately before the live OS readings.
    pub live: Option<ControlSnapshot>,
    /// Includes OS begin/end probes and execution; not full child/process time.
    pub full_window_ns: u64,
    pub execution: WindowExecution,
}
