//! Failure to activate autonomous ownerless maintenance.

/// Maintenance is not activated by a failed call. Retry is permitted.
/// Existing allocation semantics remain available, but callers requiring the
/// ownerless guarantee must handle this error rather than assume activation.
#[derive(Debug)]
#[non_exhaustive]
pub enum MaintenanceStartError {
    /// Another caller is starting the service. No wait occurs, including on
    /// a reentrant call during startup's ordinary allocator bootstrap.
    InProgress,
    /// The OS refused to create the worker. No worker or lease remains held.
    Spawn(std::io::Error),
}

impl core::fmt::Display for MaintenanceStartError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InProgress => f.write_str("allocator maintenance startup is in progress"),
            Self::Spawn(error) => {
                write!(f, "allocator maintenance worker could not start: {error}")
            }
        }
    }
}

impl std::error::Error for MaintenanceStartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InProgress => None,
            Self::Spawn(error) => Some(error),
        }
    }
}
