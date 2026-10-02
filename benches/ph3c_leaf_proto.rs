//! Ph3c step-1 — one-shot iai-callgrind micro-prototype for design candidate B
//! ("per-leaf bit leaves"), see `docs/design/2026-10-02-ph3c-offbody-geometry-design.md`
//! §2.2 (candidate B structure) and §3.3 (falsification plan). Falsification
//! thresholds to apply to the numbers below:
//! - dense leaf:   `Ir(LeafTable) / Ir(intrusive) > 1.10`  => B fails,
//! - sparse leaf:  `Ir(LeafTable) / Ir(intrusive) > 1.25`  (W_eff > 8 words) => B fails.
//!
//! The prototype is deliberately isolated from sefer-alloc internals: nothing
//! here imports the crate, so the five arms are pure free-set-geometry models
//! whose instruction counts are directly comparable to each other:
//! - `Intrusive` — base emulation: `next` lives in the block body (16 B-stride
//!   arena touches), flat segment bitmap with is_free + mark_free.
//! - `NextTable` — spike emulation: `next` in a 1 MiB off-body table, bitmap
//!   is_free / mark_free as separate accesses.
//! - `LeafTable` — candidate B: 64 leaves x 64 bitmap words (bit = slot FREE,
//!   same polarity as `AllocBitmap::is_free`/`mark_free`), per-leaf header
//!   {class, free_count, first_hint}, per-class word of non-empty leaves
//!   (bit l of `class_words[c]` = leaf l has free slots for class c).
//! - `LeafTableFast` — candidate B2 modification: same 32 KiB occupancy, but
//!   the hot path never touches a counter — the leaf header carries only
//!   {class, words_mask} with bit w = `occ[leaf][w] != 0`, so pop is two tzcnt
//!   with no word-scan loop; `free_count` is recomputed by popcount only
//!   inside merge (off the hot path).
//! - `LeafTable3` — candidate B3 (ADR addendum
//!   `docs/design/2026-10-02-adr-addendum-ph3c-decisions.md` §3): the B2 shape
//!   over a 4 KiB SUB-LEAF grain instead of a 64 KiB leaf (256 slots = 4
//!   occupancy words), so the class-bound unit wastes ≤ 4 KiB instead of ≤
//!   64 KiB — 49 classes cost ≤ 196 KiB = 4.8% of a segment, against ~3 MiB
//!   (75%) for the 64 KiB grain. Occupancy stays 32 KiB per segment; the
//!   per-class "non-empty sub-leaves" index is a 16-word mask (1024 sub-leaves)
//!   plus a cursor word, so the first-non-empty lookup is a single tzcnt (no
//!   linear scan of masks on the hot path). The B3 batch shape is REAL here:
//!   `drain_into`/`flush_run` work one occupancy WORD at a time (load,
//!   tzcnt/blsr loop, a single store) instead of the trait's per-slot default,
//!   and a class whose block is larger than a sub-leaf spans several of them —
//!   that geometry is marked and counted (`span_subleaves`, `span_retires`),
//!   not implemented, which is all the shoulder decision needs.
//!
//! Every scenario replays the SAME offset sequence through all five models;
//! `measure_identity` asserts the issued offsets match pairwise (intrusive vs
//! next_table, intrusive vs leaf_table, intrusive vs leaf_table_fast,
//! intrusive vs leaf_table_3), so any Ir delta is geometry, never
//! semantics. That identity check is NOT run inside the measured scenario arms
//! (its ~85 M instruction constant would swamp the 1-4% shoulder deltas); it
//! has its own dedicated `identity_check` bench arm instead, listed FIRST in the
//! group so a violated assert fails the run before any scenario is measured.
//!
//! Two more Linux-only entry points: `remote_merge_setup` measures the remote
//! merge with its donor/target built in iai's UNMEASURED setup prologue
//! (`#[bench::arm(setup_call())]`), which is the shape the addendum asks for
//! (the plain `remote_merge` arms keep the construction INSIDE the measured
//! body, so they are not comparable per block and are not gated); and the
//! `mod tests` at the bottom runs under `cargo test --bench ph3c_leaf_proto`
//! without valgrind or iai-callgrind-runner (see the note at the `main!`
//! invocation).
//!
//! Measured caveat: model construction (arena/table zeroing) sits inside the
//! timed function body and differs slightly per arm — compare per-op deltas
//! between arms, not raw totals.

// One-shot model code: explicit index loops / div-ceil arithmetic mirror the
// allocator shape it emulates; rewriting them would change the measured code.
#![allow(
    clippy::manual_div_ceil,
    clippy::needless_range_loop,
    clippy::type_complexity
)]

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
use std::hint::black_box;

#[cfg(target_os = "linux")]
use iai_callgrind::{library_benchmark, library_benchmark_group, main, LibraryBenchmarkConfig};

// ---------------------------------------------------------------------------
// Geometry constants — one 4 MiB segment of 16 B slots, re-sliced into 64 KiB
// leaves (4096 slots), each leaf described by 64 u64 bitmap words.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
const SLOT_BYTES: usize = 16;
#[cfg(target_os = "linux")]
const SEGMENT_BYTES: usize = 4 * 1024 * 1024;
#[cfg(target_os = "linux")]
const SEGMENT_SLOTS: usize = SEGMENT_BYTES / SLOT_BYTES; // 262144
#[cfg(target_os = "linux")]
const BITMAP_WORDS: usize = SEGMENT_SLOTS / 64; // 4096 words = 32 KiB, 1 bit/slot
#[cfg(target_os = "linux")]
const LEAF_BYTES: usize = 64 * 1024;
#[cfg(target_os = "linux")]
const LEAF_SLOTS: usize = LEAF_BYTES / SLOT_BYTES; // 4096
#[cfg(target_os = "linux")]
const LEAVES: usize = SEGMENT_BYTES / LEAF_BYTES; // 64
#[cfg(target_os = "linux")]
const WORDS_PER_LEAF: usize = LEAF_SLOTS / 64; // 64
#[cfg(target_os = "linux")]
const NUM_CLASSES: usize = 8;

// Size class of a block: index = log2(size / 16). 16 B -> 0, 64 B -> 2.
#[cfg(target_os = "linux")]
fn class_of(size: usize) -> usize {
    (size / SLOT_BYTES).trailing_zeros() as usize
}

// ---------------------------------------------------------------------------
// B3 geometry — a 4 KiB SUB-LEAF of 16 B slots (256 slots = 4 bitmap words).
// A 64 KiB prototype leaf is 16 sub-leaves; a 4 MiB segment is 1024 of them.
// Shrinking the class-bound unit 16x is the whole point of B3: the
// fragmentation tail per class is ≤ 4 KiB instead of ≤ 64 KiB (49 classes =>
// ≤ 196 KiB ≈ 4.8% of a segment, against ~3 MiB ≈ 75% for the leaf grain).
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
const SUBLEAF_BYTES: usize = 4 * 1024;
#[cfg(target_os = "linux")]
const SUBLEAF_SLOTS: usize = SUBLEAF_BYTES / SLOT_BYTES; // 256
#[cfg(target_os = "linux")]
const SUBLEAF_WORDS: usize = SUBLEAF_SLOTS / 64; // 4
#[cfg(target_os = "linux")]
const SUBLEAVES: usize = SEGMENT_BYTES / SUBLEAF_BYTES; // 1024
#[cfg(target_os = "linux")]
const SUBLEAVES_PER_LEAF: usize = LEAF_SLOTS / SUBLEAF_SLOTS; // 16

// One class's "non-empty sub-leaves" index needs 1024 bits = 16 u64 words
// (B/B2 fit 64 leaves into a single word). `class_cursor[c]` is the lowest
// mask word that can hold a set bit, so the first-non-empty lookup is one
// tzcnt and the hot path never scans the mask (ADR §3 "курсор на класс").
#[cfg(target_os = "linux")]
const CLASS_MASK_WORDS: usize = SUBLEAVES / 64; // 16
                                                // Cursor value meaning "this class has no non-empty sub-leaf at all".
#[cfg(target_os = "linux")]
const CLASS_MASK_EMPTY: u8 = CLASS_MASK_WORDS as u8;

// The real allocator's small table has 49 classes (`SIZE_CLASS_TABLE`); the
// prototype's scenarios only use classes 0..8, but B3's per-class index is
// sized for the production table — 49 x 16 x 8 B = 6.25 KiB per segment, which
// is what the candidate actually pays.
#[cfg(target_os = "linux")]
const PROD_CLASSES: usize = 49;

// Parametric stand-in for the real `SIZE_CLASS_TABLE` (49 classes): 40
// geometric classes (16 B, 1.25x growth rounded up to the 16 B block stride,
// the real table's `GEO_COUNT`) merged with the 9 exact device/page-friendly
// classes of its `EXTRAS` (256 … 16384), strictly increasing — the same
// construction `src/alloc_core/platform/size_classes.rs` uses, so the top of
// the table sits at the real `SMALL_MAX` (~253 KiB) instead of running off to
// segment-sized blocks. The prototype stays isolated from the crate, so the
// table is rebuilt from the constants above.
#[cfg(target_os = "linux")]
const CLASS_SIZES: [usize; PROD_CLASSES] = build_class_sizes();

#[cfg(target_os = "linux")]
const fn build_class_sizes() -> [usize; PROD_CLASSES] {
    // `EXTRAS` of the real scheme: task #145's exact 256 B class plus the
    // page-multiple classes of task B1.
    const EXTRAS: [usize; 9] = [256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];
    let mut table = [0usize; PROD_CLASSES];
    let mut size = SLOT_BYTES; // next geometric class
    let mut geo_left = 40usize; // geometric classes still to emit
    let mut e = 0usize; // extras consumed
    let mut i = 0usize; // entries emitted
    while i < PROD_CLASSES {
        let next = if e < EXTRAS.len() && (geo_left == 0 || EXTRAS[e] <= size) {
            let v = EXTRAS[e];
            e += 1;
            v
        } else {
            let v = size;
            // ceil(1.25 x), rounded up to the 16 B stride
            let grown = (v * 5 + 3) / 4;
            size = (grown + SLOT_BYTES - 1) / SLOT_BYTES * SLOT_BYTES;
            geo_left -= 1;
            v
        };
        // strictly increasing, as the real merged table is
        if i == 0 || next > table[i - 1] {
            table[i] = next;
            i += 1;
        }
    }
    table
}

/// Sub-leaves one block of `block_size` spans. 1 for every block that fits a
/// 4 KiB sub-leaf; more for the large classes of the real table (ADR §3:
/// "классы крупнее под-листа — спан из нескольких под-листов"). That geometry
/// is MARKED and COUNTED by the model — the split carve itself is out of the
/// prototype's scope, exactly as the addendum allows.
#[cfg(target_os = "linux")]
const fn span_of(block_size: usize) -> usize {
    (block_size + SUBLEAF_BYTES - 1) / SUBLEAF_BYTES
}

/// Sub-leaves one block of class `c` spans.
#[cfg(target_os = "linux")]
const fn span_subleaves(c: usize) -> usize {
    span_of(CLASS_SIZES[c])
}

/// Largest class index whose block still fits a single sub-leaf (26 of the 49
/// in the parametric table: classes 0..=25, the 4096 B class, span 1; the next
/// one, 4640 B, already spans two). A retire of a class above it spans several
/// sub-leaves, so the hot path marks it with one compare.
#[cfg(target_os = "linux")]
const LAST_SINGLE_SUBLEAF_CLASS: usize = last_single_subleaf_class();

#[cfg(target_os = "linux")]
const fn last_single_subleaf_class() -> usize {
    let mut last = 0;
    let mut c = 0;
    while c < PROD_CLASSES {
        if span_subleaves(c) == 1 {
            last = c;
        }
        c += 1;
    }
    last
}

// ---------------------------------------------------------------------------
// Common model interface — enum-dispatched (no dyn), so each arm's Ir is a
// static code shape.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum Model {
    Intrusive,
    NextTable,
    LeafTable,
    LeafTableFast,
    LeafTable3,
}

#[cfg(target_os = "linux")]
trait ModelOps: Sized {
    fn new() -> Self;
    fn retire(&mut self, class: usize, off: u32);
    fn pop(&mut self, class: usize) -> Option<u32>;
    /// Free slots inside one leaf (stored counter or popcount over the leaf words).
    fn free_slots_in_leaf(&self, leaf: usize) -> usize;
    /// The leaf's free-slot words (identity oracle: bitmap / occupancy bits).
    fn leaf_free_mask(&self, leaf: usize) -> [u64; WORDS_PER_LEAF];
    /// Class bound to a leaf. Only candidate B carries it in the leaf header —
    /// that is the D-3 property; the linked emulations keep the class outside
    /// the free-set geometry (segment/magazine metadata), so the scenario must
    /// supply it per call. In the real allocator I/N store the class
    /// differently — this asymmetry IS D-3. B3 carries the same property at
    /// sub-leaf granularity, so its `leaf_class` is the class of the leaf's
    /// first non-empty sub-leaf.
    fn leaf_class(&self, leaf: usize) -> Option<usize>;

    fn drain_into(&mut self, class: usize, out: &mut Vec<u32>, n: usize) {
        for _ in 0..n {
            match self.pop(class) {
                Some(off) => out.push(off),
                None => break,
            }
        }
    }

    // Accepts arrive run-sequential, so for LeafTable the bits land in the same
    // occupancy word; for I/N it is one retire per block.
    fn flush_run(&mut self, class: usize, offs: &[u32]) {
        for &off in offs {
            self.retire(class, off);
        }
    }
}

// ---------------------------------------------------------------------------
// Base emulation — intrusive `next` pointer stored in the block body.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
struct Intrusive {
    arena: Vec<u8>,      // 4 MiB segment image, zeroed
    free_bits: Vec<u64>, // flat bitmap, 4096 words, bit = 1 => slot free
    head: [u32; NUM_CLASSES],
}

#[cfg(target_os = "linux")]
impl Intrusive {
    #[inline]
    fn is_free(&self, off: u32) -> bool {
        self.free_bits[(off >> 6) as usize] & (1u64 << (off & 63)) != 0
    }

    #[inline]
    fn mark_free(&mut self, off: u32) {
        self.free_bits[(off >> 6) as usize] |= 1u64 << (off & 63);
    }

    #[inline]
    fn mark_alloc(&mut self, off: u32) {
        self.free_bits[(off >> 6) as usize] &= !(1u64 << (off & 63));
    }
}

#[cfg(target_os = "linux")]
impl ModelOps for Intrusive {
    fn new() -> Self {
        Self {
            arena: vec![0u8; SEGMENT_BYTES],
            free_bits: vec![0u64; BITMAP_WORDS],
            head: [u32::MAX; NUM_CLASSES],
        }
    }

    fn retire(&mut self, class: usize, off: u32) {
        // is_free + mark_free on the bitmap word, then the intrusive next-store
        // into the block body (emulates `Node::deref` + store).
        // Polarity: `is_free` is true for a slot that is ALREADY free, so the
        // precondition of a retire is `!is_free` — a double retire (re-retiring
        // an already free slot) is exactly the case this assert must catch. The
        // inverted form asserted `is_free`, i.e. it fired on every LEGITIMATE
        // retire of an occupied slot and never on a double one.
        debug_assert!(!self.is_free(off), "double retire of slot {off}");
        self.mark_free(off);
        let dst = off as usize * SLOT_BYTES;
        let next = self.head[class];
        self.arena[dst..dst + 4].copy_from_slice(&next.to_le_bytes());
        self.head[class] = off;
    }

    fn pop(&mut self, class: usize) -> Option<u32> {
        let off = self.head[class];
        if off == u32::MAX {
            return None;
        }
        let src = off as usize * SLOT_BYTES;
        let next = u32::from_le_bytes(self.arena[src..src + 4].try_into().unwrap());
        self.head[class] = next;
        self.mark_alloc(off);
        Some(off)
    }

    fn free_slots_in_leaf(&self, leaf: usize) -> usize {
        let base = leaf * WORDS_PER_LEAF;
        self.free_bits[base..base + WORDS_PER_LEAF]
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum()
    }

    fn leaf_free_mask(&self, leaf: usize) -> [u64; WORDS_PER_LEAF] {
        let base = leaf * WORDS_PER_LEAF;
        let mut out = [0u64; WORDS_PER_LEAF];
        out.copy_from_slice(&self.free_bits[base..base + WORDS_PER_LEAF]);
        out
    }

    fn leaf_class(&self, _leaf: usize) -> Option<usize> {
        None
    }
}

// ---------------------------------------------------------------------------
// Spike emulation — off-body `next` table (1 MiB), bitmap untouched in shape.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
struct NextTable {
    // Segment image: the spike never writes `next` into the body, so this field
    // is never read — it models the same segment geometry (and construction
    // cost) as the base arm.
    #[allow(dead_code)]
    arena: Vec<u8>,
    free_bits: Vec<u64>,  // same flat 4096-word bitmap
    next_table: Vec<u32>, // 262144 entries = 1 MiB, next lives here
    head: [u32; NUM_CLASSES],
}

#[cfg(target_os = "linux")]
impl NextTable {
    #[inline]
    fn is_free(&self, off: u32) -> bool {
        self.free_bits[(off >> 6) as usize] & (1u64 << (off & 63)) != 0
    }

    #[inline]
    fn mark_free(&mut self, off: u32) {
        self.free_bits[(off >> 6) as usize] |= 1u64 << (off & 63);
    }

    #[inline]
    fn mark_alloc(&mut self, off: u32) {
        self.free_bits[(off >> 6) as usize] &= !(1u64 << (off & 63));
    }
}

#[cfg(target_os = "linux")]
impl ModelOps for NextTable {
    fn new() -> Self {
        Self {
            arena: vec![0u8; SEGMENT_BYTES],
            free_bits: vec![0u64; BITMAP_WORDS],
            next_table: vec![0u32; SEGMENT_SLOTS],
            head: [u32::MAX; NUM_CLASSES],
        }
    }

    fn retire(&mut self, class: usize, off: u32) {
        // is_free + mark_free as separate bitmap accesses (two metadata touches
        // as in the spike), then the next-store into the off-body table.
        // Same polarity fix as the intrusive arm: the retire precondition is
        // "slot is occupied", so the check asserts `!is_free` — a double
        // retire (slot already free) is what must be caught.
        debug_assert!(!self.is_free(off), "double retire of slot {off}");
        self.mark_free(off);
        let next = self.head[class];
        self.next_table[off as usize] = next;
        self.head[class] = off;
    }

    fn pop(&mut self, class: usize) -> Option<u32> {
        let off = self.head[class];
        if off == u32::MAX {
            return None;
        }
        self.head[class] = self.next_table[off as usize];
        self.mark_alloc(off);
        Some(off)
    }

    fn free_slots_in_leaf(&self, leaf: usize) -> usize {
        let base = leaf * WORDS_PER_LEAF;
        self.free_bits[base..base + WORDS_PER_LEAF]
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum()
    }

    fn leaf_free_mask(&self, leaf: usize) -> [u64; WORDS_PER_LEAF] {
        let base = leaf * WORDS_PER_LEAF;
        let mut out = [0u64; WORDS_PER_LEAF];
        out.copy_from_slice(&self.free_bits[base..base + WORDS_PER_LEAF]);
        out
    }

    fn leaf_class(&self, _leaf: usize) -> Option<usize> {
        None
    }
}

// ---------------------------------------------------------------------------
// Candidate B — per-leaf bit leaves. bit = 1 => slot FREE (compatible with
// `AllocBitmap::is_free`/`mark_free` semantics).
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
struct LeafHdr {
    class: u8,
    free_count: u16,
    first_hint: u8,
}

#[cfg(target_os = "linux")]
struct LeafTable {
    occ: Box<[[u64; WORDS_PER_LEAF]; LEAVES]>, // 64 x 512 B = 32 KiB
    leaf_hdr: Box<[LeafHdr; LEAVES]>,
    class_words: [u64; NUM_CLASSES], // bit l = leaf l non-empty for class c
}

#[cfg(target_os = "linux")]
impl LeafTable {
    // The real merge carries the region's class with it; model that by priming
    // the target leaf header (first-carve bookkeeping is the same write).
    #[inline]
    fn set_leaf_class(&mut self, leaf: usize, class: usize) {
        self.leaf_hdr[leaf].class = class as u8;
    }

    // Remote merge: word-wise OR of the region plus a popcount recount — no
    // user-body page is touched (design §2.2 "remote merge").
    fn or_merge_region(&mut self, leaf: usize, src: &[u64; WORDS_PER_LEAF]) {
        let class = self.leaf_hdr[leaf].class as usize;
        let mut acc: u16 = 0;
        for (w, &s) in src.iter().enumerate() {
            self.occ[leaf][w] |= s;
            acc += s.count_ones() as u16;
        }
        self.leaf_hdr[leaf].free_count = acc;
        self.class_words[class] |= 1 << leaf;
    }
}

#[cfg(target_os = "linux")]
impl ModelOps for LeafTable {
    fn new() -> Self {
        Self {
            occ: Box::new([[0u64; WORDS_PER_LEAF]; LEAVES]),
            leaf_hdr: Box::new(
                [LeafHdr {
                    class: 0,
                    free_count: 0,
                    first_hint: 0,
                }; LEAVES],
            ),
            class_words: [0u64; NUM_CLASSES],
        }
    }

    fn retire(&mut self, class: usize, off: u32) {
        let leaf = (off / LEAF_SLOTS as u32) as usize;
        let slot = off & (LEAF_SLOTS as u32 - 1);
        let w = (slot >> 6) as usize;
        let bit = 1u64 << (slot & 63);
        // is_free + mark_free fused into ONE RMW of the same occupancy word.
        let word = &mut self.occ[leaf][w];
        *word |= bit;
        let hdr = &mut self.leaf_hdr[leaf];
        let was_empty = hdr.free_count == 0;
        hdr.free_count += 1;
        if was_empty {
            self.class_words[class] |= 1 << leaf;
            hdr.class = class as u8; // written at first retire, decides D-3
        }
    }

    fn pop(&mut self, class: usize) -> Option<u32> {
        let cw = self.class_words[class];
        if cw == 0 {
            return None;
        }
        let li = cw.trailing_zeros() as usize;
        let start = self.leaf_hdr[li].first_hint as usize;
        for i in 0..WORDS_PER_LEAF {
            let w = (start + i) & (WORDS_PER_LEAF - 1);
            let word = self.occ[li][w];
            if word != 0 {
                let slot = w * 64 + word.trailing_zeros() as usize;
                self.occ[li][w] = word & (word - 1); // clear lowest set bit
                let hdr = &mut self.leaf_hdr[li];
                hdr.free_count -= 1;
                hdr.first_hint = w as u8;
                if hdr.free_count == 0 {
                    self.class_words[class] &= !(1 << li);
                }
                return Some((li * LEAF_SLOTS + slot) as u32);
            }
        }
        None
    }

    fn free_slots_in_leaf(&self, leaf: usize) -> usize {
        self.leaf_hdr[leaf].free_count as usize
    }

    fn leaf_free_mask(&self, leaf: usize) -> [u64; WORDS_PER_LEAF] {
        self.occ[leaf]
    }

    fn leaf_class(&self, leaf: usize) -> Option<usize> {
        Some(self.leaf_hdr[leaf].class as usize) // D-3: class from the leaf
    }
}

// ---------------------------------------------------------------------------
// Candidate B2 — same occupancy as B, but the hot path keeps no counter:
// `words_mask` bit w = `occ[leaf][w] != 0`, so pop is O(1) (two trailing_zeros,
// no scan loop) and `free_count` exists only transiently inside merge.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
struct LeafHdrFast {
    class: u8,
    _pad: [u8; 7],
    words_mask: u64,
}

#[cfg(target_os = "linux")]
struct LeafTableFast {
    occ: Box<[[u64; WORDS_PER_LEAF]; LEAVES]>, // 64 x 512 B = 32 KiB, same as B
    leaf_hdr: Box<[LeafHdrFast; LEAVES]>,
    class_words: [u64; NUM_CLASSES], // bit l = leaf l non-empty for class c
}

#[cfg(target_os = "linux")]
impl LeafTableFast {
    #[inline]
    fn set_leaf_class(&mut self, leaf: usize, class: usize) {
        self.leaf_hdr[leaf].class = class as u8;
    }

    // Remote merge: word-wise OR of the region plus a popcount recount (off the
    // hot path). words_mask is recomputed from the OR-ed words: bit w = the
    // merged word is non-zero. No user-body page is touched.
    fn or_merge_region(&mut self, leaf: usize, src: &[u64; WORDS_PER_LEAF]) -> usize {
        let class = self.leaf_hdr[leaf].class as usize;
        let mut acc: usize = 0;
        let mut mask: u64 = 0;
        for (w, &s) in src.iter().enumerate() {
            let merged = self.occ[leaf][w] | s;
            self.occ[leaf][w] = merged;
            acc += s.count_ones() as usize;
            mask |= ((merged != 0) as u64) << w;
        }
        self.leaf_hdr[leaf].words_mask = mask;
        self.class_words[class] |= 1 << leaf;
        acc
    }
}

#[cfg(target_os = "linux")]
impl ModelOps for LeafTableFast {
    fn new() -> Self {
        Self {
            occ: Box::new([[0u64; WORDS_PER_LEAF]; LEAVES]),
            leaf_hdr: Box::new(
                [LeafHdrFast {
                    class: 0,
                    _pad: [0; 7],
                    words_mask: 0,
                }; LEAVES],
            ),
            class_words: [0u64; NUM_CLASSES],
        }
    }

    fn retire(&mut self, class: usize, off: u32) {
        let leaf = (off / LEAF_SLOTS as u32) as usize;
        let slot = off & (LEAF_SLOTS as u32 - 1);
        let w = (slot >> 6) as usize;
        let bit = 1u64 << (slot & 63);
        // is_free + mark_free fused into ONE RMW of the same occupancy word;
        // the mask update is a second, conditional RMW of the same cache line.
        let word = &mut self.occ[leaf][w];
        let old = *word;
        *word = old | bit;
        if old == 0 {
            let hdr = &mut self.leaf_hdr[leaf];
            let was_empty = hdr.words_mask == 0;
            hdr.words_mask |= 1 << w;
            if was_empty {
                self.class_words[class] |= 1 << leaf;
                hdr.class = class as u8; // written at first retire, decides D-3
            }
        }
    }

    fn pop(&mut self, class: usize) -> Option<u32> {
        let cw = self.class_words[class];
        if cw == 0 {
            return None;
        }
        let li = cw.trailing_zeros() as usize;
        let hdr = &mut self.leaf_hdr[li];
        let mask = hdr.words_mask;
        let w = mask.trailing_zeros() as usize;
        let word = self.occ[li][w];
        let slot = w * 64 + word.trailing_zeros() as usize;
        let word0 = word & (word - 1); // clear lowest set bit
        self.occ[li][w] = word0;
        if word0 == 0 {
            hdr.words_mask &= !(1 << w);
            if hdr.words_mask == 0 {
                self.class_words[class] &= !(1 << li);
            }
        }
        Some((li * LEAF_SLOTS + slot) as u32)
    }

    fn free_slots_in_leaf(&self, leaf: usize) -> usize {
        // Not stored in the hot path — recomputed by popcount (off the hot path).
        self.occ[leaf].iter().map(|w| w.count_ones() as usize).sum()
    }

    fn leaf_free_mask(&self, leaf: usize) -> [u64; WORDS_PER_LEAF] {
        self.occ[leaf]
    }

    fn leaf_class(&self, leaf: usize) -> Option<usize> {
        Some(self.leaf_hdr[leaf].class as usize) // D-3: class from the leaf
    }
}

// ---------------------------------------------------------------------------
// Candidate B3 — B2's header composition over a 4 KiB sub-leaf grain plus the
// batch-shaped hot path of ADR addendum §3.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
struct SubLeafHdr {
    class: u8, // class bound to this sub-leaf (D-3 at 4 KiB grain)
    _pad: [u8; 7],
    words_mask: u64, // bit w = occ[sl][w] != 0 — no counter on the hot path
}

#[cfg(target_os = "linux")]
struct LeafTable3 {
    occ: Box<[[u64; SUBLEAF_WORDS]; SUBLEAVES]>, // 1024 x 32 B = 32 KiB, same as B/B2
    sub_hdr: Box<[SubLeafHdr; SUBLEAVES]>,       // 1024 x 8 B
    class_mask: Box<[[u64; CLASS_MASK_WORDS]; PROD_CLASSES]>, // bit sl = sub-leaf non-empty for class
    // Lowest mask word of class `c` that can still hold a set bit; every word
    // below it is empty, so `first_subleaf` is a single tzcnt. Initialized to
    // CLASS_MASK_EMPTY ("class is empty") and lowered explicitly on link, so
    // the hot path never scans the mask.
    class_cursor: [u8; PROD_CLASSES],
    // Retires of classes whose block spans several sub-leaves. The split carve
    // is not implemented (ADR §3 allows marking + counting), but the events
    // must be countable so the shoulder decision sees the geometry.
    span_retires: u64,
}

#[cfg(target_os = "linux")]
impl LeafTable3 {
    // The real merge carries the region's class with it; priming the target
    // sub-leaf header is the same first-carve bookkeeping write as B/B2's
    // `set_leaf_class`.
    #[inline]
    fn set_subleaf_class(&mut self, sl: usize, class: usize) {
        self.sub_hdr[sl].class = class as u8;
    }

    /// Bind a non-empty sub-leaf into its class's mask, preserving the cursor
    /// invariant "every mask word below `class_cursor[class]` is empty" — that
    /// invariant is what keeps the first-non-empty lookup at one tzcnt.
    #[inline]
    fn link_subleaf(&mut self, class: usize, sl: usize) {
        let wi = sl >> 6;
        let word = &mut self.class_mask[class][wi];
        let bit = 1u64 << (sl & 63);
        if *word & bit == 0 {
            *word |= bit;
            if (wi as u8) < self.class_cursor[class] {
                self.class_cursor[class] = wi as u8;
            }
        }
    }

    /// First non-empty sub-leaf of `class`, O(1): the cursor names the lowest
    /// mask word that can hold a set bit, so the first tzcnt decides it. The
    /// loop only ever skips words that are empty at this moment, and each such
    /// word is skipped once for good — no linear scan on the hot path.
    #[inline]
    fn first_subleaf(&mut self, class: usize) -> Option<usize> {
        let mut wi = self.class_cursor[class] as usize;
        while wi < CLASS_MASK_WORDS {
            let word = self.class_mask[class][wi];
            if word != 0 {
                self.class_cursor[class] = wi as u8;
                return Some((wi << 6) + word.trailing_zeros() as usize);
            }
            wi += 1;
        }
        self.class_cursor[class] = CLASS_MASK_EMPTY;
        None
    }

    /// Drop an emptied sub-leaf from its class's mask. The cursor is NOT
    /// advanced here — `first_subleaf` skips empty words lazily, so a class
    /// that refills a low word is found again without rework.
    #[inline]
    fn unlink_subleaf(&mut self, class: usize, sl: usize) {
        let wi = sl >> 6;
        let bit = 1u64 << (sl & 63);
        self.class_mask[class][wi] &= !bit;
    }

    /// ONE RMW of an occupancy word plus the conditional mask/class bookkeeping
    /// — the shared core of `retire` and `flush_run`'s per-word commit.
    #[inline]
    fn commit_word(&mut self, class: usize, sl: usize, w: usize, bits: u64) {
        let old = self.occ[sl][w];
        self.occ[sl][w] = old | bits;
        if old == 0 {
            let hdr = &mut self.sub_hdr[sl];
            let was_empty = hdr.words_mask == 0;
            hdr.words_mask |= 1 << w;
            if was_empty {
                // first retire into this sub-leaf binds the class (D-3)
                hdr.class = class as u8;
            }
        }
        self.link_subleaf(class, sl);
    }

    // Remote merge over ONE sub-leaf: a 4-word OR plus a words_mask
    // recomputation. No user-body page is touched (design §2.2 "remote merge").
    fn or_merge_region(&mut self, sl: usize, src: &[u64; SUBLEAF_WORDS]) -> usize {
        let class = self.sub_hdr[sl].class as usize;
        let mut acc: usize = 0;
        let mut mask: u64 = 0;
        for w in 0..SUBLEAF_WORDS {
            let merged = self.occ[sl][w] | src[w];
            self.occ[sl][w] = merged;
            acc += src[w].count_ones() as usize;
            mask |= ((merged != 0) as u64) << w;
        }
        self.sub_hdr[sl].words_mask = mask;
        self.link_subleaf(class, sl);
        acc
    }
}

#[cfg(target_os = "linux")]
impl ModelOps for LeafTable3 {
    fn new() -> Self {
        Self {
            occ: Box::new([[0u64; SUBLEAF_WORDS]; SUBLEAVES]),
            sub_hdr: Box::new(
                [SubLeafHdr {
                    class: 0,
                    _pad: [0; 7],
                    words_mask: 0,
                }; SUBLEAVES],
            ),
            class_mask: Box::new([[0u64; CLASS_MASK_WORDS]; PROD_CLASSES]),
            class_cursor: [CLASS_MASK_EMPTY; PROD_CLASSES],
            span_retires: 0,
        }
    }

    fn retire(&mut self, class: usize, off: u32) {
        debug_assert!(
            class < PROD_CLASSES,
            "class {class} outside the 49-class table"
        );
        if class > LAST_SINGLE_SUBLEAF_CLASS {
            // A block of this class spans several sub-leaves (§3 B3). The
            // split carve is out of the prototype's scope, so the event is
            // counted instead of modelled.
            self.span_retires += 1;
        }
        let sl = (off / SUBLEAF_SLOTS as u32) as usize;
        let slot = off & (SUBLEAF_SLOTS as u32 - 1);
        let w = (slot >> 6) as usize;
        // is_free + mark_free fused into ONE RMW of the sub-leaf's occupancy
        // word; the mask update is a conditional second touch of the same line.
        self.commit_word(class, sl, w, 1u64 << (slot & 63));
    }

    fn pop(&mut self, class: usize) -> Option<u32> {
        // class cursor -> sub-leaf -> tzcnt(words_mask) -> tzcnt(word) -> blsr.
        let sl = self.first_subleaf(class)?;
        let mask = self.sub_hdr[sl].words_mask;
        // A linked sub-leaf can have an empty words_mask if a drain ended
        // exactly at a word boundary; drop it rather than reading occ[sl][64].
        if mask == 0 {
            self.unlink_subleaf(class, sl);
            return None;
        }
        let w = mask.trailing_zeros() as usize;
        let word = self.occ[sl][w];
        // words_mask bit w guarantees `word != 0`, so no word scan is possible.
        let bit = word.trailing_zeros();
        let rest = word & (word - 1);
        self.occ[sl][w] = rest;
        if rest == 0 {
            self.sub_hdr[sl].words_mask &= !(1 << w);
            if self.sub_hdr[sl].words_mask == 0 {
                self.unlink_subleaf(class, sl);
            }
        }
        Some((sl * SUBLEAF_SLOTS + w * 64 + bit as usize) as u32)
    }

    fn drain_into(&mut self, class: usize, out: &mut Vec<u32>, n: usize) {
        // ADR §3 batch shape: one word at a time — load, tzcnt/blsr loop, ONE
        // store of the remainder; the masks are touched once per WORD, never
        // once per slot. The trait's per-slot default is deliberately unused.
        while out.len() < n {
            let Some(sl) = self.first_subleaf(class) else {
                return;
            };
            let mut mask = self.sub_hdr[sl].words_mask;
            if mask == 0 {
                // stale link from a drain that stopped exactly at a boundary
                self.unlink_subleaf(class, sl);
                continue;
            }
            while out.len() < n {
                let w = mask.trailing_zeros() as usize;
                let budget = n - out.len();
                let base = (sl * SUBLEAF_SLOTS + w * 64) as u32;
                let mut word = self.occ[sl][w];
                let mut emitted = 0usize;
                while word != 0 && emitted < budget {
                    out.push(base + word.trailing_zeros());
                    word &= word - 1;
                    emitted += 1;
                }
                if word != 0 {
                    // budget ran out inside this word: store the remainder, stop
                    self.occ[sl][w] = word;
                    return;
                }
                self.occ[sl][w] = 0;
                mask &= !(1 << w);
                self.sub_hdr[sl].words_mask = mask;
                if mask == 0 {
                    // sub-leaf exhausted: unlink it from the class mask now, so
                    // "linked <=> words_mask != 0" stays exact, and move on to
                    // the next non-empty sub-leaf of this class.
                    self.unlink_subleaf(class, sl);
                    break;
                }
            }
        }
    }

    fn flush_run(&mut self, class: usize, offs: &[u32]) {
        // Run-sequential accepts land in the same occupancy word, so the bits
        // are accumulated in a register and committed once per word (ADR §3):
        // one RMW per word, not one per slot.
        let mut cur_sl = usize::MAX;
        let mut cur_w = usize::MAX;
        let mut acc: u64 = 0;
        for &off in offs {
            let sl = (off / SUBLEAF_SLOTS as u32) as usize;
            let slot = off & (SUBLEAF_SLOTS as u32 - 1);
            let w = (slot >> 6) as usize;
            if sl != cur_sl || w != cur_w {
                if acc != 0 {
                    self.commit_word(class, cur_sl, cur_w, acc);
                    acc = 0;
                }
                cur_sl = sl;
                cur_w = w;
            }
            acc |= 1u64 << (slot & 63);
        }
        if acc != 0 {
            self.commit_word(class, cur_sl, cur_w, acc);
        }
    }

    fn free_slots_in_leaf(&self, leaf: usize) -> usize {
        // A 64 KiB leaf is 16 4 KiB sub-leaves; no counter is kept on the hot
        // path, so this is a popcount over the leaf's 64 words.
        let base = leaf * SUBLEAVES_PER_LEAF;
        self.occ[base..base + SUBLEAVES_PER_LEAF]
            .iter()
            .flatten()
            .map(|w| w.count_ones() as usize)
            .sum()
    }

    fn leaf_free_mask(&self, leaf: usize) -> [u64; WORDS_PER_LEAF] {
        // Concatenation of the 16 sub-leaves' 4 words each: a leaf-relative
        // slot's bit index is identical to B/B2's, so identity holds.
        let base = leaf * SUBLEAVES_PER_LEAF;
        let mut out = [0u64; WORDS_PER_LEAF];
        for (i, sl) in (base..base + SUBLEAVES_PER_LEAF).enumerate() {
            out[i * SUBLEAF_WORDS..(i + 1) * SUBLEAF_WORDS].copy_from_slice(&self.occ[sl]);
        }
        out
    }

    fn leaf_class(&self, leaf: usize) -> Option<usize> {
        // D-3 at 4 KiB grain: the leaf's class is the class of its first
        // non-empty sub-leaf (a 64 KiB leaf is 16 class-bound sub-leaves).
        let base = leaf * SUBLEAVES_PER_LEAF;
        (base..base + SUBLEAVES_PER_LEAF)
            .find(|&sl| self.sub_hdr[sl].words_mask != 0)
            .map(|sl| self.sub_hdr[sl].class as usize)
    }
}

// ---------------------------------------------------------------------------
// Scenario drivers — generic over the model, monomorphized per arm (static
// shapes). All offset sequences are deterministic constants, so identity and
// every arm replay byte-identical inputs.
// ---------------------------------------------------------------------------

// (a) hot churn in one class: drain the whole working set, re-retire it.
#[cfg(target_os = "linux")]
const CHURN_BLOCKS: usize = 256; // slots 0..256 of leaf 0, class 0 (16 B)
#[cfg(target_os = "linux")]
const CHURN_ROUNDS: usize = 8;

#[cfg(target_os = "linux")]
fn drive_churn_one_class<M: ModelOps>() -> Vec<Vec<u32>> {
    let offs: Vec<u32> = (0..CHURN_BLOCKS as u32).collect();
    let mut m = M::new();
    for &off in &offs {
        m.retire(0, off);
    }
    let mut rounds = Vec::with_capacity(CHURN_ROUNDS);
    let mut issued = Vec::with_capacity(CHURN_BLOCKS);
    for _ in 0..CHURN_ROUNDS {
        issued.clear();
        m.drain_into(0, &mut issued, CHURN_BLOCKS);
        black_box(&issued);
        m.flush_run(0, &issued);
        rounds.push(issued.clone());
    }
    rounds
}

// (b) dense drain: one full leaf (4096 free slots) emptied in 4 batches.
#[cfg(target_os = "linux")]
const DENSE_BATCHES: usize = 4;
#[cfg(target_os = "linux")]
const DENSE_BATCH: usize = 1024;

#[cfg(target_os = "linux")]
fn drive_drain_dense<M: ModelOps>() -> Vec<Vec<u32>> {
    let mut m = M::new();
    for off in 0..LEAF_SLOTS as u32 {
        m.retire(0, off);
    }
    let mut rounds = Vec::with_capacity(DENSE_BATCHES);
    let mut issued = Vec::with_capacity(DENSE_BATCH);
    for _ in 0..DENSE_BATCHES {
        issued.clear();
        m.drain_into(0, &mut issued, DENSE_BATCH);
        black_box(&issued);
        rounds.push(issued.clone());
    }
    rounds
}

// (c) sparse drain: k free slots scattered over one leaf — this is the W_eff
// measurement (pop scans up to 64 words from first_hint).
#[cfg(target_os = "linux")]
fn sparse_offsets(k: usize) -> Vec<u32> {
    match k {
        1 => vec![(LEAF_SLOTS / 2) as u32],
        8 => (0..8).map(|i| (i * LEAF_SLOTS / 8) as u32).collect(),
        64 => (0..64).map(|i| (i * LEAF_SLOTS / 64) as u32).collect(),
        _ => unreachable!("sparse offsets only defined for k in 1, 8, 64"),
    }
}

#[cfg(target_os = "linux")]
fn drive_drain_sparse<M: ModelOps>(k: usize) -> Vec<Vec<u32>> {
    let offs = sparse_offsets(k);
    let mut m = M::new();
    for &off in &offs {
        m.retire(0, off);
    }
    let mut issued = Vec::with_capacity(k);
    m.drain_into(0, &mut issued, k);
    black_box(&issued);
    vec![issued]
}

// (d) refill pattern: flush_run of 256 blocks per leaf over 4 leaves, then a
// per-leaf drain; repeated for 2 rounds (round 2's flush_run is the re-retire).
#[cfg(target_os = "linux")]
const REFILL_LEAVES: usize = 4;
#[cfg(target_os = "linux")]
const REFILL_BLOCKS: usize = 256;
#[cfg(target_os = "linux")]
const REFILL_ROUNDS: usize = 2;

#[cfg(target_os = "linux")]
fn drive_refill_cold<M: ModelOps>(class: usize) -> Vec<Vec<u32>> {
    let mut m = M::new();
    let mut rounds = Vec::with_capacity(REFILL_ROUNDS * REFILL_LEAVES);
    let mut issued = Vec::with_capacity(REFILL_BLOCKS);
    for _ in 0..REFILL_ROUNDS {
        for leaf in 0..REFILL_LEAVES {
            let base = (leaf * LEAF_SLOTS) as u32;
            let run: Vec<u32> = (0..REFILL_BLOCKS as u32).map(|i| base + i).collect();
            m.flush_run(class, &run);
            issued.clear();
            m.drain_into(class, &mut issued, REFILL_BLOCKS);
            black_box(&issued);
            rounds.push(issued.clone());
        }
    }
    rounds
}

// (e) mixed-class churn across 48 class-bound leaves (leaves 48..64 stay
// free), plus the fragmentation-tail counter of design §4.10 / D-3.
#[cfg(target_os = "linux")]
const MIXED_LEAVES: usize = 48; // leaf l -> class l % NUM_CLASSES
#[cfg(target_os = "linux")]
const MIXED_BLOCKS_PER_LEAF: usize = 64;
#[cfg(target_os = "linux")]
const MIXED_ROUNDS: usize = 4;
#[cfg(target_os = "linux")]
const MIXED_DRAIN: usize = 32;

#[cfg(target_os = "linux")]
fn drive_churn_mixed<M: ModelOps>() -> (Vec<Vec<u32>>, Vec<[u64; WORDS_PER_LEAF]>, u64) {
    let mut m = M::new();
    for leaf in 0..MIXED_LEAVES {
        let class = leaf % NUM_CLASSES;
        let base = (leaf * LEAF_SLOTS) as u32;
        for i in 0..MIXED_BLOCKS_PER_LEAF as u32 {
            m.retire(class, base + i);
        }
    }
    let mut rounds = Vec::with_capacity(MIXED_ROUNDS * MIXED_LEAVES);
    let mut issued = Vec::with_capacity(MIXED_DRAIN);
    for _ in 0..MIXED_ROUNDS {
        for leaf in 0..MIXED_LEAVES {
            let class = leaf % NUM_CLASSES;
            issued.clear();
            m.drain_into(class, &mut issued, MIXED_DRAIN);
            black_box(&issued);
            m.flush_run(class, &issued);
            rounds.push(issued.clone());
        }
    }
    // Fragmentation tail: a class-bound leaf's whole 64 KiB is unavailable to
    // other classes. live_bytes = carved blocks currently allocated; at this
    // point every block is re-retired, so the tail is the leaf's full capacity.
    let mut tail: u64 = 0;
    let mut free_sets = Vec::with_capacity(MIXED_LEAVES);
    for leaf in 0..MIXED_LEAVES {
        black_box(m.leaf_class(leaf));
        let live_bytes = (MIXED_BLOCKS_PER_LEAF - m.free_slots_in_leaf(leaf)) * SLOT_BYTES;
        tail += (LEAF_SLOTS * SLOT_BYTES - live_bytes) as u64;
        free_sets.push(m.leaf_free_mask(leaf));
    }
    (rounds, free_sets, tail)
}

// (f) remote merge: linked schemes pay one re-push per block; LeafTable pays
// one word-wise OR + popcount recount per leaf.
#[cfg(target_os = "linux")]
const MERGE_BLOCKS: usize = 64;
#[cfg(target_os = "linux")]
const MERGE_LEAF: usize = 0;

#[cfg(target_os = "linux")]
fn remote_merge_offs() -> Vec<u32> {
    (0..MERGE_BLOCKS as u32).collect() // contiguous run inside leaf 0
}

#[cfg(target_os = "linux")]
fn remote_merge_intrusive() -> Vec<u32> {
    let offs = remote_merge_offs();
    let mut m = Intrusive::new();
    for &off in &offs {
        m.retire(0, off); // price of merging in a linked scheme
    }
    black_box(&m);
    offs
}

#[cfg(target_os = "linux")]
fn remote_merge_next_table() -> Vec<u32> {
    let offs = remote_merge_offs();
    let mut m = NextTable::new();
    for &off in &offs {
        m.retire(0, off);
    }
    black_box(&m);
    offs
}

#[cfg(target_os = "linux")]
fn remote_merge_leaf() -> Vec<u32> {
    // Donor: the region is carved somewhere else, then its 64 occupancy words
    // travel as plain metadata (no user-body page touched).
    let mut donor = LeafTable::new();
    for off in remote_merge_offs() {
        donor.retire(0, off);
    }
    let mut src = [0u64; WORDS_PER_LEAF];
    src.copy_from_slice(&donor.occ[MERGE_LEAF]);
    let mut m = LeafTable::new();
    m.set_leaf_class(MERGE_LEAF, 0);
    m.or_merge_region(MERGE_LEAF, &src);
    // Enumerate the merged free set — must equal the linked arms' re-push set.
    let mut out = Vec::with_capacity(MERGE_BLOCKS);
    m.drain_into(0, &mut out, MERGE_BLOCKS);
    out.sort_unstable();
    out
}

#[cfg(target_os = "linux")]
fn remote_merge_leaf_fast() -> Vec<u32> {
    // Same as remote_merge_leaf; the merged popcount is the merge's return value.
    let mut donor = LeafTableFast::new();
    for off in remote_merge_offs() {
        donor.retire(0, off);
    }
    let mut src = [0u64; WORDS_PER_LEAF];
    src.copy_from_slice(&donor.occ[MERGE_LEAF]);
    let mut m = LeafTableFast::new();
    m.set_leaf_class(MERGE_LEAF, 0);
    assert_eq!(m.or_merge_region(MERGE_LEAF, &src), MERGE_BLOCKS);
    let mut out = Vec::with_capacity(MERGE_BLOCKS);
    m.drain_into(0, &mut out, MERGE_BLOCKS);
    out.sort_unstable();
    out
}

#[cfg(target_os = "linux")]
fn remote_merge_leaf3() -> Vec<u32> {
    // Same donor/target flow; the donor's 64 blocks live in sub-leaf 0, so the
    // merged metadata is that sub-leaf's 4 occupancy words.
    let mut donor = LeafTable3::new();
    for off in remote_merge_offs() {
        donor.retire(0, off);
    }
    let src = donor.occ[MERGE_SUBLEAF];
    let mut m = LeafTable3::new();
    m.set_subleaf_class(MERGE_SUBLEAF, 0);
    assert_eq!(m.or_merge_region(MERGE_SUBLEAF, &src), MERGE_BLOCKS);
    let mut out = Vec::with_capacity(MERGE_BLOCKS);
    m.drain_into(0, &mut out, MERGE_BLOCKS);
    out.sort_unstable();
    out
}

// ---------------------------------------------------------------------------
// Remote merge with the donor/target built OUTSIDE the measured body — ADR §3
// ("Remote merge мерить отдельным плечом, построив donor/target в setup вне
// замеряемого тела"). iai's `#[bench::arm(setup_call())]` evaluates the setup
// expression in an unmeasured prologue and passes its value to the bench
// function, so each arm's Ir is its own merge + drain, not two model
// constructions.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
const MERGE_SUBLEAF: usize = 0; // donor offsets 0..63 all fall inside sub-leaf 0

// Prepared state of one `remote_merge_setup` arm. I/N carry the already-built
// linked chain (the body only walks it out); L/L2/L3 carry the donor's
// occupancy words plus the class-primed target.
#[cfg(target_os = "linux")]
enum MergeSetup {
    Intrusive(Intrusive),
    NextTable(NextTable),
    LeafTable {
        src: [u64; WORDS_PER_LEAF],
        target: LeafTable,
    },
    LeafTableFast {
        src: [u64; WORDS_PER_LEAF],
        target: LeafTableFast,
    },
    LeafTable3 {
        src: [u64; SUBLEAF_WORDS],
        target: LeafTable3,
    },
}

#[cfg(target_os = "linux")]
fn remote_merge_setup_intrusive() -> MergeSetup {
    let mut m = Intrusive::new();
    for off in remote_merge_offs() {
        m.retire(0, off); // the linked scheme's price for merging in a region
    }
    MergeSetup::Intrusive(m)
}

#[cfg(target_os = "linux")]
fn remote_merge_setup_next_table() -> MergeSetup {
    let mut m = NextTable::new();
    for off in remote_merge_offs() {
        m.retire(0, off);
    }
    MergeSetup::NextTable(m)
}

#[cfg(target_os = "linux")]
fn remote_merge_setup_leaf() -> MergeSetup {
    let mut donor = LeafTable::new();
    for off in remote_merge_offs() {
        donor.retire(0, off);
    }
    let mut src = [0u64; WORDS_PER_LEAF];
    src.copy_from_slice(&donor.occ[MERGE_LEAF]);
    let mut target = LeafTable::new();
    target.set_leaf_class(MERGE_LEAF, 0);
    MergeSetup::LeafTable { src, target }
}

#[cfg(target_os = "linux")]
fn remote_merge_setup_leaf_fast() -> MergeSetup {
    let mut donor = LeafTableFast::new();
    for off in remote_merge_offs() {
        donor.retire(0, off);
    }
    let mut src = [0u64; WORDS_PER_LEAF];
    src.copy_from_slice(&donor.occ[MERGE_LEAF]);
    let mut target = LeafTableFast::new();
    target.set_leaf_class(MERGE_LEAF, 0);
    MergeSetup::LeafTableFast { src, target }
}

#[cfg(target_os = "linux")]
fn remote_merge_setup_leaf3() -> MergeSetup {
    let mut donor = LeafTable3::new();
    for off in remote_merge_offs() {
        donor.retire(0, off);
    }
    let src = donor.occ[MERGE_SUBLEAF];
    let mut target = LeafTable3::new();
    target.set_subleaf_class(MERGE_SUBLEAF, 0);
    MergeSetup::LeafTable3 { src, target }
}

#[cfg(target_os = "linux")]
fn merged_drain_len<M: ModelOps>(m: &mut M) -> usize {
    let mut out = Vec::with_capacity(MERGE_BLOCKS);
    m.drain_into(0, &mut out, MERGE_BLOCKS);
    black_box(&out);
    out.len()
}

// ---------------------------------------------------------------------------
// Model identity check — replayed by the dedicated `identity_check` bench arm.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn normalize(rounds: &[Vec<u32>]) -> Vec<Vec<u32>> {
    rounds
        .iter()
        .map(|r| {
            let mut v = r.clone();
            v.sort_unstable();
            v
        })
        .collect()
}

// All issued offsets, sorted. Valid whenever each drain empties the free set
// it draws from (scenarios a, c, d) or the drain batches partition it without
// re-retire (scenario b) — then every model issues the same multiset, just in
// a different order (LIFO stack vs address-ordered bitmap).
#[cfg(target_os = "linux")]
fn check_issued(
    intrusive: &[Vec<u32>],
    next: &[Vec<u32>],
    leaf: &[Vec<u32>],
    leaf_fast: &[Vec<u32>],
    leaf3: &[Vec<u32>],
) {
    let flat = |rounds: &[Vec<u32>]| {
        let mut v: Vec<u32> = rounds.iter().flatten().copied().collect();
        v.sort_unstable();
        v
    };
    let (i, n, l, l2, l3) = (
        flat(intrusive),
        flat(next),
        flat(leaf),
        flat(leaf_fast),
        flat(leaf3),
    );
    assert_eq!(i, n, "identity violated: intrusive vs next_table");
    assert_eq!(i, l, "identity violated: intrusive vs leaf_table");
    assert_eq!(i, l2, "identity violated: intrusive vs leaf_table_fast");
    assert_eq!(i, l3, "identity violated: intrusive vs leaf_table_3");
}

#[cfg(target_os = "linux")]
fn check_free_sets(
    intrusive: &[[u64; WORDS_PER_LEAF]],
    next: &[[u64; WORDS_PER_LEAF]],
    leaf: &[[u64; WORDS_PER_LEAF]],
    leaf_fast: &[[u64; WORDS_PER_LEAF]],
    leaf3: &[[u64; WORDS_PER_LEAF]],
) {
    assert_eq!(
        intrusive, next,
        "free-set identity violated: intrusive vs next_table"
    );
    assert_eq!(
        intrusive, leaf,
        "free-set identity violated: intrusive vs leaf_table"
    );
    assert_eq!(
        intrusive, leaf_fast,
        "free-set identity violated: intrusive vs leaf_table_fast"
    );
    assert_eq!(
        intrusive, leaf3,
        "free-set identity violated: intrusive vs leaf_table_3"
    );
}

#[cfg(target_os = "linux")]
fn measure_identity() {
    // (a) every drain empties the 256-slot working set.
    check_issued(
        &drive_churn_one_class::<Intrusive>(),
        &drive_churn_one_class::<NextTable>(),
        &drive_churn_one_class::<LeafTable>(),
        &drive_churn_one_class::<LeafTableFast>(),
        &drive_churn_one_class::<LeafTable3>(),
    );
    // (b) the 4 batches partition the full 4096-slot leaf.
    check_issued(
        &drive_drain_dense::<Intrusive>(),
        &drive_drain_dense::<NextTable>(),
        &drive_drain_dense::<LeafTable>(),
        &drive_drain_dense::<LeafTableFast>(),
        &drive_drain_dense::<LeafTable3>(),
    );
    // (c) single drain of the whole k-slot free set.
    for &k in &[1usize, 8, 64] {
        check_issued(
            &drive_drain_sparse::<Intrusive>(k),
            &drive_drain_sparse::<NextTable>(k),
            &drive_drain_sparse::<LeafTable>(k),
            &drive_drain_sparse::<LeafTableFast>(k),
            &drive_drain_sparse::<LeafTable3>(k),
        );
    }
    // (d) each per-leaf drain empties that leaf's 256-slot run.
    for &class in &[class_of(16), class_of(64)] {
        check_issued(
            &drive_refill_cold::<Intrusive>(class),
            &drive_refill_cold::<NextTable>(class),
            &drive_refill_cold::<LeafTable>(class),
            &drive_refill_cold::<LeafTableFast>(class),
            &drive_refill_cold::<LeafTable3>(class),
        );
    }
    // (e) partial 32-of-64 drains: LIFO (I/N) and address-ordered (L/L2/L3)
    // issuance legitimately picks DIFFERENT slots per round (design §4
    // unknown #4: the LIFO -> address-ordered reissue effect is exactly
    // what is unmeasured). I/N must still agree with each other, and the
    // bookkeeping invariant that must hold for all five is the resulting
    // per-leaf free set — the drain/re-retire cycle restores it fully.
    let (mi, ni, li, l2i, l3i) = (
        drive_churn_mixed::<Intrusive>(),
        drive_churn_mixed::<NextTable>(),
        drive_churn_mixed::<LeafTable>(),
        drive_churn_mixed::<LeafTableFast>(),
        drive_churn_mixed::<LeafTable3>(),
    );
    assert_eq!(
        normalize(&mi.0),
        normalize(&ni.0),
        "identity violated: intrusive vs next_table"
    );
    check_free_sets(&mi.1, &ni.1, &li.1, &l2i.1, &l3i.1);
    // (f) one merge yields the same 64 free slots in every model.
    let (ri, rn, rl, rl2, rl3) = (
        remote_merge_intrusive(),
        remote_merge_next_table(),
        remote_merge_leaf(),
        remote_merge_leaf_fast(),
        remote_merge_leaf3(),
    );
    assert_eq!(
        ri, rn,
        "identity violated: remote merge intrusive vs next_table"
    );
    assert_eq!(
        ri, rl,
        "identity violated: remote merge intrusive vs leaf_table"
    );
    assert_eq!(
        ri, rl2,
        "identity violated: remote merge intrusive vs leaf_table_fast"
    );
    assert_eq!(
        ri, rl3,
        "identity violated: remote merge intrusive vs leaf_table_3"
    );
}

// ---------------------------------------------------------------------------
// Identity arm — runs FIRST, so a violated assert aborts the run before any
// scenario number is recorded.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::identity_only(0u8)]
fn identity_check(_tag: u8) {
    measure_identity();
}

// ---------------------------------------------------------------------------
// Scenario arms — measured WITHOUT the identity constant, so per-arm deltas are
// the pure geometry. 9 scenarios x 4 model arms = 36 benches.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
#[bench::leaf_table_3(Model::LeafTable3)]
fn churn_one_class(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_churn_one_class::<Intrusive>(),
        Model::NextTable => drive_churn_one_class::<NextTable>(),
        Model::LeafTable => drive_churn_one_class::<LeafTable>(),
        Model::LeafTableFast => drive_churn_one_class::<LeafTableFast>(),
        Model::LeafTable3 => drive_churn_one_class::<LeafTable3>(),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
#[bench::leaf_table_3(Model::LeafTable3)]
fn drain_dense(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_drain_dense::<Intrusive>(),
        Model::NextTable => drive_drain_dense::<NextTable>(),
        Model::LeafTable => drive_drain_dense::<LeafTable>(),
        Model::LeafTableFast => drive_drain_dense::<LeafTableFast>(),
        Model::LeafTable3 => drive_drain_dense::<LeafTable3>(),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
#[bench::leaf_table_3(Model::LeafTable3)]
fn drain_sparse_1(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_drain_sparse::<Intrusive>(1),
        Model::NextTable => drive_drain_sparse::<NextTable>(1),
        Model::LeafTable => drive_drain_sparse::<LeafTable>(1),
        Model::LeafTableFast => drive_drain_sparse::<LeafTableFast>(1),
        Model::LeafTable3 => drive_drain_sparse::<LeafTable3>(1),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
#[bench::leaf_table_3(Model::LeafTable3)]
fn drain_sparse_8(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_drain_sparse::<Intrusive>(8),
        Model::NextTable => drive_drain_sparse::<NextTable>(8),
        Model::LeafTable => drive_drain_sparse::<LeafTable>(8),
        Model::LeafTableFast => drive_drain_sparse::<LeafTableFast>(8),
        Model::LeafTable3 => drive_drain_sparse::<LeafTable3>(8),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
#[bench::leaf_table_3(Model::LeafTable3)]
fn drain_sparse_64(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_drain_sparse::<Intrusive>(64),
        Model::NextTable => drive_drain_sparse::<NextTable>(64),
        Model::LeafTable => drive_drain_sparse::<LeafTable>(64),
        Model::LeafTableFast => drive_drain_sparse::<LeafTableFast>(64),
        Model::LeafTable3 => drive_drain_sparse::<LeafTable3>(64),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
#[bench::leaf_table_3(Model::LeafTable3)]
fn refill_cold_16(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_refill_cold::<Intrusive>(class_of(16)),
        Model::NextTable => drive_refill_cold::<NextTable>(class_of(16)),
        Model::LeafTable => drive_refill_cold::<LeafTable>(class_of(16)),
        Model::LeafTableFast => drive_refill_cold::<LeafTableFast>(class_of(16)),
        Model::LeafTable3 => drive_refill_cold::<LeafTable3>(class_of(16)),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
#[bench::leaf_table_3(Model::LeafTable3)]
fn refill_cold_64(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_refill_cold::<Intrusive>(class_of(64)),
        Model::NextTable => drive_refill_cold::<NextTable>(class_of(64)),
        Model::LeafTable => drive_refill_cold::<LeafTable>(class_of(64)),
        Model::LeafTableFast => drive_refill_cold::<LeafTableFast>(class_of(64)),
        Model::LeafTable3 => drive_refill_cold::<LeafTable3>(class_of(64)),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
#[bench::leaf_table_3(Model::LeafTable3)]
fn churn_mixed(m: Model) {
    let (rounds, _free_sets, tail) = match m {
        Model::Intrusive => drive_churn_mixed::<Intrusive>(),
        Model::NextTable => drive_churn_mixed::<NextTable>(),
        Model::LeafTable => drive_churn_mixed::<LeafTable>(),
        Model::LeafTableFast => drive_churn_mixed::<LeafTableFast>(),
        Model::LeafTable3 => drive_churn_mixed::<LeafTable3>(),
    };
    black_box(&rounds);
    // Constant marker (same value in every arm), not part of the geometry.
    println!("FRAGMENTATION_TAIL_BYTES {tail}");
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
#[bench::leaf_table_3(Model::LeafTable3)]
fn remote_merge(m: Model) {
    let blocks = match m {
        Model::Intrusive => remote_merge_intrusive().len(),
        Model::NextTable => remote_merge_next_table().len(),
        Model::LeafTable => {
            let mut donor = LeafTable::new();
            for off in remote_merge_offs() {
                donor.retire(0, off);
            }
            let mut src = [0u64; WORDS_PER_LEAF];
            src.copy_from_slice(&donor.occ[MERGE_LEAF]);
            let mut target = LeafTable::new();
            target.set_leaf_class(MERGE_LEAF, 0);
            target.or_merge_region(MERGE_LEAF, &src);
            assert_eq!(
                target.leaf_hdr[MERGE_LEAF].free_count as usize,
                MERGE_BLOCKS
            );
            black_box(&target);
            MERGE_BLOCKS
        }
        Model::LeafTableFast => {
            // Same donor/target flow; the merge returns the popped popcount.
            let mut donor = LeafTableFast::new();
            for off in remote_merge_offs() {
                donor.retire(0, off);
            }
            let mut src = [0u64; WORDS_PER_LEAF];
            src.copy_from_slice(&donor.occ[MERGE_LEAF]);
            let mut target = LeafTableFast::new();
            target.set_leaf_class(MERGE_LEAF, 0);
            assert_eq!(target.or_merge_region(MERGE_LEAF, &src), MERGE_BLOCKS);
            black_box(&target);
            MERGE_BLOCKS
        }
        Model::LeafTable3 => {
            // Same donor/target flow; the merge is a 4-word OR of sub-leaf 0.
            let mut donor = LeafTable3::new();
            for off in remote_merge_offs() {
                donor.retire(0, off);
            }
            let src = donor.occ[MERGE_SUBLEAF];
            let mut target = LeafTable3::new();
            target.set_subleaf_class(MERGE_SUBLEAF, 0);
            assert_eq!(target.or_merge_region(MERGE_SUBLEAF, &src), MERGE_BLOCKS);
            black_box(&target);
            MERGE_BLOCKS
        }
    };
    black_box(blocks);
    // Constant marker (same value in every arm), not part of the geometry.
    println!("REMOTE_MERGE_BLOCKS {blocks}");
}

// ---------------------------------------------------------------------------
// Remote merge, setup outside the measurement — the ADR §3 shape. The donor and
// the target are built in iai's unmeasured setup prologue, so the arm's body is
// ONLY the merge + drain: no model construction, no zeroing of a second model.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(remote_merge_setup_intrusive())]
#[bench::next_table(remote_merge_setup_next_table())]
#[bench::leaf_table(remote_merge_setup_leaf())]
#[bench::leaf_table_fast(remote_merge_setup_leaf_fast())]
#[bench::leaf_table_3(remote_merge_setup_leaf3())]
fn remote_merge_setup(setup: MergeSetup) {
    let blocks = match setup {
        // I/N: the chain is already pushed; the body walks it out (64 `next`
        // hops — the linked scheme's per-block merge price).
        MergeSetup::Intrusive(mut m) => merged_drain_len(&mut m),
        MergeSetup::NextTable(mut m) => merged_drain_len(&mut m),
        // L/L2/L3: one word-wise OR of the donor's metadata into the target,
        // then the merged free set is drained.
        MergeSetup::LeafTable { src, mut target } => {
            target.or_merge_region(MERGE_LEAF, &src);
            assert_eq!(
                target.leaf_hdr[MERGE_LEAF].free_count as usize,
                MERGE_BLOCKS
            );
            merged_drain_len(&mut target)
        }
        MergeSetup::LeafTableFast { src, mut target } => {
            assert_eq!(target.or_merge_region(MERGE_LEAF, &src), MERGE_BLOCKS);
            merged_drain_len(&mut target)
        }
        MergeSetup::LeafTable3 { src, mut target } => {
            assert_eq!(target.or_merge_region(MERGE_SUBLEAF, &src), MERGE_BLOCKS);
            merged_drain_len(&mut target)
        }
    };
    black_box(blocks);
    // Constant marker (same value in every arm), not part of the geometry.
    println!("REMOTE_MERGE_SETUP_BLOCKS {blocks}");
}

// ---------------------------------------------------------------------------
// Unit tests — the fragmentation tail of the 49-class production table and the
// LeafTable3 bookkeeping invariants. They run under
// `cargo test --bench ph3c_leaf_proto`.
//
// Why the cases are plain `fn`s and not `#[test]` items: this target is
// `harness = false`, so cargo compiles it with `--cfg test` but WITHOUT
// `--test` — libtest is never linked and `#[test]` items are stripped from the
// crate entirely, i.e. they would never run. `run_all()` below is the runner
// (libtest-shaped: catch_unwind per case, exit code = failure count) and it is
// called from `main!`'s config expression on the direct-execution path, so a
// test run needs neither valgrind nor iai-callgrind-runner.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[cfg(test)]
mod tests {
    use super::*;

    /// One carved block of the tail pattern: the block's first slot, its
    /// slot capacity, and the class-bound sub-leaf region it lives in (a whole
    /// number of sub-leaves, aligned — that is what "привязка класса к
    /// под-листу" means for the tail). The layout is the real allocator's
    /// `carve_block` shape: every block starts inside its own region.
    struct Carve {
        off: u32,
        slots: usize,
        region_slots: usize,
    }

    /// One block per production class (49 classes) laid out inside one segment,
    /// each in its own sub-leaf-aligned region of `span_subleaves(c)` sub-leaves.
    fn layout_49_classes() -> Vec<Carve> {
        let mut out = Vec::with_capacity(PROD_CLASSES);
        let mut slot = 0usize;
        for c in 0..PROD_CLASSES {
            let region = span_subleaves(c) * SUBLEAF_SLOTS;
            slot = slot.div_ceil(region) * region;
            out.push(Carve {
                off: slot as u32,
                slots: CLASS_SIZES[c] / SLOT_BYTES,
                region_slots: region,
            });
            slot += region;
        }
        assert!(
            slot <= SEGMENT_SLOTS,
            "the 49-class carve does not fit one segment"
        );
        out
    }

    /// Free slots inside `[from, from + slots)`, read out of the model itself.
    fn free_slots_in_range(m: &LeafTable3, from: usize, slots: usize) -> usize {
        (from..from + slots)
            .filter(|&s| m.occ[s / SUBLEAF_SLOTS][(s % SUBLEAF_SLOTS) >> 6] >> (s & 63) & 1 == 1)
            .count()
    }

    /// The measured fragmentation tail: every sub-leaf a class's carve region
    /// binds is unavailable to any other class, so its unused bytes are the
    /// class's tail. Live bytes are read out of the model, not recomputed.
    fn measured_tail(m: &LeafTable3, layout: &[Carve]) -> u64 {
        let mut tail = 0u64;
        for carve in layout.iter() {
            let free = free_slots_in_range(m, carve.off as usize, carve.slots);
            let live = (carve.slots - free) * SLOT_BYTES;
            let bound = carve.region_slots * SLOT_BYTES;
            assert!(
                live <= bound,
                "class at offset {}: live {live} exceeds its span",
                carve.off
            );
            tail += (bound - live) as u64;
        }
        tail
    }

    /// The same tail from the model's CONSTANTS only. The churn pattern's
    /// measurement point has exactly one free slot per class (the carved
    /// block's first slot, re-retired after a pop).
    fn analytic_tail(layout: &[Carve]) -> u64 {
        layout
            .iter()
            .map(|carve| {
                let live = (carve.slots - 1) * SLOT_BYTES;
                (carve.region_slots * SLOT_BYTES - live) as u64
            })
            .sum()
    }

    /// The LeafTable3 bookkeeping invariants, checked after a churn pattern.
    fn check_invariants(m: &LeafTable3) {
        for c in 0..PROD_CLASSES {
            let non_empty: usize = m.class_mask[c]
                .iter()
                .map(|w| w.count_ones() as usize)
                .sum();
            assert!(
                non_empty <= SUBLEAVES,
                "class {c}: {non_empty} non-empty sub-leaves"
            );
            let cursor = m.class_cursor[c] as usize;
            if non_empty != 0 {
                // the cursor must never sit above the lowest non-empty word
                let lowest = m.class_mask[c].iter().position(|w| *w != 0).unwrap();
                assert!(
                    cursor <= lowest,
                    "class {c}: cursor {cursor} above the lowest set word {lowest}"
                );
            }
            for w in 0..cursor {
                assert_eq!(
                    m.class_mask[c][w], 0,
                    "class {c}: mask word {w} below the cursor is set"
                );
            }
        }
        for sl in 0..SUBLEAVES {
            let hdr = m.sub_hdr[sl];
            let mut expect = 0u64;
            for w in 0..SUBLEAF_WORDS {
                expect |= ((m.occ[sl][w] != 0) as u64) << w;
            }
            assert_eq!(
                hdr.words_mask, expect,
                "sub-leaf {sl}: words_mask disagrees with occupancy"
            );
            // a sub-leaf is in its class's mask exactly while it is non-empty
            let linked = m.class_mask[hdr.class as usize][sl >> 6] >> (sl & 63) & 1 == 1;
            assert_eq!(
                linked,
                expect != 0,
                "sub-leaf {sl}: class mask disagrees with words_mask"
            );
        }
    }

    /// Retires the first slot of every class's block, pops it back and
    /// re-retires it — the churn shape the scenario drivers replay.
    fn churn_49_classes(m: &mut LeafTable3, layout: &[Carve], rounds: usize) {
        for _ in 0..rounds {
            for (c, carve) in layout.iter().enumerate() {
                m.retire(c, carve.off);
            }
            for (c, carve) in layout.iter().enumerate() {
                assert_eq!(m.pop(c), Some(carve.off), "class {c}: pop lost a slot");
            }
        }
    }

    fn fragmentation_tail_49_classes_is_subleaf_grain() {
        let layout = layout_49_classes();
        let mut m = LeafTable3::new();
        churn_49_classes(&mut m, &layout, 1);
        // the last op of the pattern is a pop, so re-retire the working set
        for (c, carve) in layout.iter().enumerate() {
            m.retire(c, carve.off);
        }
        check_invariants(&m);
        let measured = measured_tail(&m, &layout);
        assert_eq!(
            measured,
            analytic_tail(&layout),
            "the tail counter disagrees with the class constants"
        );
        // ADR §3 bound: at most one partially-used sub-leaf per class.
        let worst = (PROD_CLASSES * SUBLEAF_BYTES) as u64;
        assert!(
            measured < worst,
            "tail {measured} exceeds the 49 x 4 KiB bound {worst}"
        );
        let pct = measured as f64 / SEGMENT_BYTES as f64 * 100.0;
        assert!(pct <= 5.0, "tail is {pct}% of the segment");
        // the same table on the 64 KiB leaf grain that made B/B2 NO-GO
        let leaf_pct = (PROD_CLASSES * LEAF_BYTES) as f64 / SEGMENT_BYTES as f64 * 100.0;
        assert!(
            leaf_pct > 50.0,
            "the 64 KiB grain would cost {leaf_pct}% — the reason B3 exists"
        );
        assert_eq!(LEAF_BYTES / SUBLEAF_BYTES, SUBLEAVES_PER_LEAF);
    }

    fn pop_and_drain_restore_the_free_set() {
        let layout = layout_49_classes();
        let mut m = LeafTable3::new();
        churn_49_classes(&mut m, &layout, 1);
        for (c, carve) in layout.iter().enumerate() {
            m.retire(c, carve.off);
        }
        // every free slot must come back out exactly once, in any order
        let mut issued = Vec::with_capacity(PROD_CLASSES);
        for (c, carve) in layout.iter().enumerate() {
            m.drain_into(c, &mut issued, carve.slots);
        }
        let mut expected: Vec<u32> = layout.iter().map(|carve| carve.off).collect();
        issued.sort_unstable();
        expected.sort_unstable();
        assert_eq!(
            issued.len(),
            PROD_CLASSES,
            "drain did not empty every class"
        );
        assert_eq!(issued, expected, "drain_into did not restore the free set");
        assert_eq!(
            free_slots_in_range(&m, 0, SEGMENT_SLOTS),
            0,
            "the model is not empty after the full drain"
        );
        check_invariants(&m);
    }

    fn span_geometry_is_marked_and_counted() {
        // both grains must exist in the parametric table
        let spanning = (0..PROD_CLASSES).filter(|&c| span_subleaves(c) > 1).count();
        assert!(
            spanning > 0 && spanning < PROD_CLASSES,
            "the parametric table has {spanning} spanning classes"
        );
        for c in 0..=LAST_SINGLE_SUBLEAF_CLASS {
            assert_eq!(span_subleaves(c), 1, "class {c} should fit one sub-leaf");
        }
        assert!(
            span_subleaves(LAST_SINGLE_SUBLEAF_CLASS + 1) > 1,
            "class {} should span sub-leaves",
            LAST_SINGLE_SUBLEAF_CLASS + 1
        );
        let layout = layout_49_classes();
        let mut m = LeafTable3::new();
        churn_49_classes(&mut m, &layout, 3);
        // every retire of a spanning class is counted (the split carve itself
        // is not implemented, as the addendum allows)
        assert_eq!(
            m.span_retires,
            3 * spanning as u64,
            "spanning-class retires were not counted"
        );
    }

    fn identity_matches_leaf_table_fast() {
        // The bench's `identity_check` arm asserts the same multisets, but it
        // only runs under valgrind; here a plain `cargo test --bench` does.
        let flat = |rounds: &[Vec<u32>]| {
            let mut v: Vec<u32> = rounds.iter().flatten().copied().collect();
            v.sort_unstable();
            v
        };
        let pairs: [(Vec<Vec<u32>>, Vec<Vec<u32>>); 6] = [
            (
                drive_churn_one_class::<LeafTable3>(),
                drive_churn_one_class::<LeafTableFast>(),
            ),
            (
                drive_drain_dense::<LeafTable3>(),
                drive_drain_dense::<LeafTableFast>(),
            ),
            (
                drive_drain_sparse::<LeafTable3>(1),
                drive_drain_sparse::<LeafTableFast>(1),
            ),
            (
                drive_drain_sparse::<LeafTable3>(8),
                drive_drain_sparse::<LeafTableFast>(8),
            ),
            (
                drive_drain_sparse::<LeafTable3>(64),
                drive_drain_sparse::<LeafTableFast>(64),
            ),
            (
                drive_refill_cold::<LeafTable3>(class_of(16)),
                drive_refill_cold::<LeafTableFast>(class_of(16)),
            ),
        ];
        for (l3, l2) in pairs.iter() {
            assert_eq!(flat(l3), flat(l2), "issued multiset differs from B2");
        }
        assert_eq!(
            flat(&drive_refill_cold::<LeafTable3>(class_of(64))),
            flat(&drive_refill_cold::<LeafTableFast>(class_of(64)))
        );
        // (e) compares free sets, not issuance orders
        let (_, l3_free, _) = drive_churn_mixed::<LeafTable3>();
        let (_, l2_free, _) = drive_churn_mixed::<LeafTableFast>();
        assert_eq!(l3_free, l2_free, "churn_mixed free sets differ from B2");
        assert_eq!(
            remote_merge_leaf3(),
            remote_merge_leaf_fast(),
            "remote merge free sets differ from B2"
        );
    }

    /// Executes the cases the way libtest would — `cargo test --bench` has no
    /// libtest main for a `harness = false` target, and this is what
    /// `main!`'s config expression calls on the direct-execution path.
    pub(super) fn run_all() {
        let cases: [(&str, fn()); 5] = [
            (
                "fragmentation_tail_49_classes",
                fragmentation_tail_49_classes_is_subleaf_grain,
            ),
            (
                "pop_and_drain_restore_the_free_set",
                pop_and_drain_restore_the_free_set,
            ),
            (
                "span_geometry_is_marked_and_counted",
                span_geometry_is_marked_and_counted,
            ),
            (
                "identity_matches_leaf_table_fast",
                identity_matches_leaf_table_fast,
            ),
            // the bench's own `identity_check` oracle, run here so a broken
            // L3 identity is caught by `cargo test` and not only under valgrind
            ("measure_identity", measure_identity),
        ];
        let mut failed = 0usize;
        for (name, case) in cases {
            match std::panic::catch_unwind(case) {
                Ok(()) => println!("test {name} ... ok"),
                Err(_) => {
                    failed += 1;
                    println!("test {name} ... FAILED");
                }
            }
        }
        println!(
            "test result: {} for LeafTable3 ({} passed, {} failed)",
            if failed == 0 { "ok" } else { "FAILED" },
            cases.len() - failed,
            failed
        );
        std::process::exit(if failed == 0 { 0 } else { 1 });
    }
}

// ---------------------------------------------------------------------------
// Test build dispatch. `cargo test --bench ph3c_leaf_proto` builds this target
// in the DEV profile (`--cfg test`, debug-assertions ON) and executes the
// binary directly, while `cargo bench` builds the BENCH profile (debug-assertions
// OFF) and hands the binary to iai-callgrind-runner. The `config` expression is
// evaluated only on the direct-execution path, so the dev-profile build runs the
// unit tests above and exits BEFORE any runner is spawned — a test run needs
// neither valgrind nor iai-callgrind-runner. Use exactly that command: a
// release-profile `cargo test --bench` would take the iai path instead.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn run_unit_tests_if_test_build() {
    if cfg!(debug_assertions) {
        #[cfg(test)]
        tests::run_all();
    }
}

#[cfg(target_os = "linux")]
library_benchmark_group!(
    name = ph3c_leaf_proto;
    benchmarks =
        identity_check,
        churn_one_class,
        drain_dense,
        drain_sparse_1,
        drain_sparse_8,
        drain_sparse_64,
        refill_cold_16,
        refill_cold_64,
        churn_mixed,
        remote_merge,
        remote_merge_setup,
);

#[cfg(target_os = "linux")]
main!(
    config = {
        run_unit_tests_if_test_build();
        LibraryBenchmarkConfig::default()
    };
    library_benchmark_groups = ph3c_leaf_proto
);
