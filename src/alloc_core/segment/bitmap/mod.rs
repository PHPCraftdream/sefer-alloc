//! Group module: the per-segment bitmap family — the shared bit-test/set/
//! clear *mechanism* ([`segment_bitmap::SegmentBitmap`]) and the two
//! orthogonal per-segment bitmaps built on it ([`alloc_bitmap::AllocBitmap`]
//! free/resident, [`magazine_bitmap::MagazineBitmap`] magazine residency).

pub(crate) mod alloc_bitmap;
/// RAD-5 (plan Phase 5-E4), verdict GO — the second orthogonal per-segment
/// bitmap (magazine residency), wired into the production hot path. See the
/// module doc for the design and `docs/perf/IAI_BASELINE.md` §RAD-5 for the
/// measurement.
pub(crate) mod magazine_bitmap;
/// The shared per-segment bitmap *mechanism* (the bit-test/set/clear
/// arithmetic + `FOOTPRINT`) common to [`alloc_bitmap::AllocBitmap`] and
/// [`magazine_bitmap::MagazineBitmap`]; task #98 / R4-6 dedup of
/// `code_quality_review.md` finding #7. `pub(crate)` only because
/// `alloc_core` itself is `#[doc(hidden)]`; the type is `pub(super)` so neither
/// wrapper nor any other crate code can confuse the two bitmap KINDS at a call
/// site. Nothing here is stable public API.
pub(crate) mod segment_bitmap;
