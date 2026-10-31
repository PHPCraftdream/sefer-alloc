//! Group module: the per-granule STATE BYTE (B4 variant (в)) — the single
//! authoritative small-segment state record. The former bitmap family
//! (`SegmentBitmap` mechanism + the `AllocBitmap`/`MagazineBitmap` wrappers)
//! was removed: no production reader/writer remained after the state-byte
//! rewrite, and the state byte now occupies their slot in the segment
//! metadata geometry (`Layout::state_byte_off()`).

/// B4 variant (в): the per-granule owner-private STATE BYTE.
pub(crate) mod state_byte;

/// R2: the LIVE state byte of a small class as a range-typed tag.
pub(crate) mod live_tag;
