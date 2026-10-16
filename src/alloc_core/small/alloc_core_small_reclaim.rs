//! Owner-only logical retirement of terminal Small sidecar records.

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::alloc_bitmap::AllocBitmap;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::fatal::fatal;
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin"
))]
use crate::alloc_core::magazine_bitmap::MagazineBitmap;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::node::Node;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment_header::{
    BinTable, Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::size_classes::{SizeClasses, SMALL_CLASS_COUNT};
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use core::ptr::NonNull;

/// One owner-side scan's retirement context (R6-T12): the segment invariants
/// the drain hoists once — `kind`, `payload_start`, `bump` and the metadata
/// views — plus the per-record retirement primitive and the accepted-credit
/// accumulator. The owner retains the scan while retiring records and drops
/// it before reservation finalization; a scan never carves and the owner
/// lease excludes issue, so `bump` and `kind` cannot change between records.
/// A duplicate or magazine-resident record never mutates the free list or
/// retires a second credit.
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
pub(crate) struct SidecarReclaim {
    base: *mut u8,
    payload_start: usize,
    bump: usize,
    bins: BinTable,
    bitmap: AllocBitmap,
    #[cfg(feature = "fastbin")]
    magazine: MagazineBitmap,
    accepted: usize,
}

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
impl SidecarReclaim {
    /// Snapshot segment context. Validate the first class before reading kind,
    /// preserving the singleton seam's class-before-kind failure order.
    pub(crate) fn new(base: *mut u8, class: u8) -> Self {
        Self::validate_class(usize::from(class));
        let kind = SegmentHeader::kind_at(base);
        if !matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
            fatal(b"alloc_core_small_reclaim: sidecar record kind is not small\n");
        }
        let meta = SegmentMeta::new(base);
        Self {
            base,
            payload_start: if kind == SegmentKind::Primordial {
                SegLayout::primordial_meta_end()
            } else {
                SegLayout::small_meta_end()
            },
            bump: meta.bump_of(),
            bins: meta.bin_table(),
            bitmap: meta.alloc_bitmap(),
            #[cfg(feature = "fastbin")]
            magazine: meta.magazine_bitmap(),
            accepted: 0,
        }
    }

    fn validate_class(class_idx: usize) {
        if class_idx >= SMALL_CLASS_COUNT {
            fatal(b"alloc_core_small_reclaim: small class index out of range\n");
        }
    }

    fn geometry_invalid() -> ! {
        fatal(b"alloc_core_small_reclaim: sidecar record offset geometry invalid\n");
    }

    /// Retire one terminal record, or reject it if already free or
    /// magazine-resident. Accepted credits retire in [`finish`](Self::finish).
    pub(crate) fn reclaim_record(&mut self, offset: u32, class: u8) -> bool {
        let off = offset as usize;
        let class_idx = usize::from(class);
        Self::validate_class(class_idx);
        let block_size = SizeClasses::block_size(class_idx);
        let end = off.checked_add(block_size);
        if off < self.payload_start || end.is_none_or(|end| end > self.bump) {
            Self::geometry_invalid();
        }
        if !Self::offset_divisible(class_idx, block_size, off) {
            Self::geometry_invalid();
        }
        if self.bitmap.is_free(offset) {
            return false;
        }
        #[cfg(feature = "fastbin")]
        if self.magazine.is_in_magazine(offset) {
            return false;
        }
        let ptr = Node::deref(self.base, off);
        let old_head = self.bins.head(class_idx);
        let next = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(self.base, old_head as usize)
        };
        let block = NonNull::new(ptr).unwrap_or_else(|| {
            fatal(b"alloc_core_small_reclaim: reclaimed record resolves to null\n")
        });
        Node::write_next(block, next);
        self.bins.set_head(class_idx, offset);
        self.bitmap.mark_free(offset);
        self.accepted += 1;
        true
    }

    /// Retire accumulated accepted credits once; consuming the context makes
    /// double retirement impossible.
    pub(crate) fn finish(self) {
        if self.accepted != 0 {
            SegmentMeta::new(self.base).sub_live(self.accepted as u32);
        }
    }

    /// Round-up reciprocal divisibility is exact for in-segment offsets:
    /// for `off = k*d`, the quotient is `k + floor(k*e/2^S)` where
    /// `0 <= e < d`; `k*e < SEGMENT < 2^S`. Non-multiples cannot equal `q*d`.
    #[inline]
    fn offset_divisible(class_idx: usize, block_size: usize, off: usize) -> bool {
        if off > crate::alloc_core::os::SEGMENT {
            return false;
        }
        let q = ((off as u64 * DIVISIBILITY_MAGIC[class_idx]) >> DIVISIBILITY_SHIFT) as usize;
        q * block_size == off
    }
}

/// Round-up reciprocal shift (R17-OPT-08). `2^45` leaves a wide exactness
/// margin — the product `off * magic` stays under `2^63` for every
/// in-segment offset while `k * e < 2^S` holds with room to spare (see
/// `SidecarReclaim::offset_divisible`).
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
const DIVISIBILITY_SHIFT: u32 = 45;

/// Per-class round-up reciprocals `ceil(2^DIVISIBILITY_SHIFT /
/// block_size(c))`, computed at compile time from the size-class table.
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
const DIVISIBILITY_MAGIC: [u64; SMALL_CLASS_COUNT] = {
    let mut table = [0u64; SMALL_CLASS_COUNT];
    let mut c = 0;
    while c < SMALL_CLASS_COUNT {
        let d = SizeClasses::block_size(c) as u64;
        table[c] = (1u64 << DIVISIBILITY_SHIFT).div_ceil(d);
        c += 1;
    }
    table
};

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
impl AllocCore {
    /// Single-record sidecar retirement — the `dbg_*` probe seam's primitive
    /// (`HeapCore::dbg_reclaim_sidecar_record_for_test`) and the per-record
    /// semantic reference for the scan-batched [`SidecarReclaim`]. Compiled
    /// only beside its one caller: the production drain retires records
    /// through the batched context.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(crate) fn reclaim_sidecar_record(base: *mut u8, offset: u32, class: u8) -> bool {
        let mut reclaim = SidecarReclaim::new(base, class);
        let accepted = reclaim.reclaim_record(offset, class);
        reclaim.finish();
        accepted
    }

    /// TEST-ONLY (R6-T12 / R17-OPT-08): the div-free divisibility predicate
    /// the sidecar-record geometry check uses, exposed so
    /// `tests/r6_t12_sidecar_offset_divisibility.rs` can sweep it against
    /// `%` for every class and offset through SEGMENT. Out-of-domain inputs
    /// return false. Pure arithmetic; not stable public API.
    #[doc(hidden)]
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_sidecar_offset_divisible_for_test(class_idx: usize, offset: usize) -> bool {
        if class_idx >= SMALL_CLASS_COUNT || offset > crate::alloc_core::os::SEGMENT {
            return false;
        }
        SidecarReclaim::offset_divisible(class_idx, SizeClasses::block_size(class_idx), offset)
    }
}
