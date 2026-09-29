/// The reservation's route mode, independent of its numeric address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteKind {
    Small,
    Primordial,
    Large,
}
