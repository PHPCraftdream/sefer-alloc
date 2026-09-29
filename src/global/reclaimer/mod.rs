//! Stage 3 scheduling substrate, not an active reclamation service.
//! No production full sweep exists yet, so there is deliberately no worker,
//! startup success, or ownerless-progress claim here.

mod maintenance_scheduler;
mod visit;

pub use maintenance_scheduler::MaintenanceScheduler;
pub use visit::Visit;
