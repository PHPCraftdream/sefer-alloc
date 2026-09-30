//! Explicit activation of the ownerless maintenance guarantee.

use super::SeferAlloc;
use crate::global::{MaintenanceService, MaintenanceStartError};

impl SeferAlloc {
    /// Start the process-global autonomous maintenance worker, confirming it is
    /// RUNNING before returning `Ok(())`. Repeated calls after activation are
    /// idempotent; simultaneous startup returns an explicit `InProgress` error.
    ///
    /// Call this at application initialization, outside allocator callbacks,
    /// heap leases, fallback-lock closures and TLS teardown. Startup may use
    /// the selected global allocator to create std thread metadata; it holds
    /// no allocator locks and the allocator does not recursively start workers.
    /// Allocation OOM follows std's existing abort policy; an OS thread-spawn
    /// failure is returned as `MaintenanceStartError::Spawn` and may be retried.
    ///
    /// **Only after success:** under fair scheduling and available exclusive
    /// leases, correctly published frees in heaps remaining FREE are eventually
    /// logically reclaimed without another allocator call or claimant. Every
    /// finite pass advances round-robin through initialized, materialized slots
    /// and tries the fallback lock once. Concurrently owned/paused heaps are
    /// never taken over by timeouts. Physical OS return additionally depends on
    /// remaining live allocations and retention policy; no latency SLA is made.
    ///
    /// Before successful startup (including `Err`), ordinary allocator behavior
    /// remains, but no autonomous ownerless guarantee is active. Applications
    /// requiring that guarantee must handle `Err`, not assume best-effort is
    /// equivalent. A worker unwind or unexpected return is terminal process
    /// abort, never silent degradation or automatic restart.
    ///
    /// The detached worker and its process-static state live until process
    /// exit. There is deliberately no shutdown API; process exit is not a drain
    /// barrier. Forking an activated multithreaded process or unloading this
    /// allocator's code while its worker exists is unsupported.
    pub fn start_maintenance() -> Result<(), MaintenanceStartError> {
        MaintenanceService::start()
    }

    /// Whether explicit startup has confirmed RUNNING. False means there is no
    /// autonomous ownerless guarantee. Worker failure after activation aborts
    /// the process rather than changing this to a silent failed-service state.
    pub fn maintenance_running() -> bool {
        MaintenanceService::running()
    }
}
