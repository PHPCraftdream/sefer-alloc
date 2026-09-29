/// Admission failures occur before any allocation covered by a route issues.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteError {
    InvalidSpan,
    Duplicate,
    OutOfMemory,
    IncarnationExhausted,
}
