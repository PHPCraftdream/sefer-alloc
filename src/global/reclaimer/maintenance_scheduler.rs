//! One bounded visit, leaving cadence and real sweeping to integration.

use crate::registry::heap_registry::MaintenanceLease;
use crate::registry::{HeapCore, HeapRegistry};

use super::super::fallback;
use super::visit::Visit;

/// Drives one registry lease attempt and one fallback try-lock per pass.
///
/// `HeapRegistry::try_maintenance` starts each scan at its rotating cursor,
/// skips unmaterialised/uninitialised slots, and grants access only after a
/// successful FREE-to-MAINTENANCE CAS. In a quiescent registry, repeated calls
/// visit persistent FREE slots without allocating chunks. Its cursor is shared
/// with claim scans, so fair service scheduling under concurrent claims still
/// needs an indexed lease API owned by the registry stage. The fallback
/// attempt is independent: a busy lock never blocks later passes.
///
/// This is not a service: it neither spawns a worker nor performs a sweep.
/// Callbacks must not be mistaken for production `HeapSweep`; integration must
/// supply a proven, non-panicking, non-reentrant sweep before startup can
/// truthfully report RUNNING.
#[doc(hidden)]
pub struct MaintenanceScheduler;

impl MaintenanceScheduler {
    /// Attempt at most one slot lease and one fallback lock acquisition.
    /// No callback is invoked without its corresponding exclusive authority.
    /// The fallback may be uninitialised; this step never initialises it.
    pub fn visit_once(
        on_slot: impl FnOnce(&mut MaintenanceLease),
        on_fallback: impl FnOnce(&mut HeapCore),
    ) -> Visit {
        let slot_leased = if let Some(mut lease) = HeapRegistry::try_maintenance() {
            on_slot(&mut lease);
            true
        } else {
            false
        };
        let fallback_locked = fallback::try_with_heap(on_fallback).is_some();
        Visit {
            slot_leased,
            fallback_locked,
        }
    }
}
