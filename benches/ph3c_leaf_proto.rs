//! Ph3c step-1 — one-shot iai-callgrind micro-prototype for design candidate B
//! ("per-leaf bit leaves"), see `docs/design/2026-10-02-ph3c-offbody-geometry-design.md`
//! §2.2 (candidate B structure) and §3.3 (falsification plan). Falsification
//! thresholds to apply to the numbers below:
//! - dense leaf:   `Ir(LeafTable) / Ir(intrusive) > 1.10`  => B fails,
//! - sparse leaf:  `Ir(LeafTable) / Ir(intrusive) > 1.25`  (W_eff > 8 words) => B fails.
//!
//! The prototype is deliberately isolated from sefer-alloc internals: nothing
//! here imports the crate, so the four arms are pure free-set-geometry models
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
//!
//! Every scenario replays the SAME offset sequence through all four models;
//! `measure_identity` asserts the issued offsets match pairwise (intrusive vs
//! next_table, intrusive vs leaf_table, intrusive vs leaf_table_fast), so any
//! Ir delta is geometry, never
//! semantics. That identity check is NOT run inside the measured scenario arms
//! (its ~85 M instruction constant would swamp the 1-4% shoulder deltas); it
//! has its own dedicated `identity_check` bench arm instead, listed FIRST in the
//! group so a violated assert fails the run before any scenario is measured.
//!
//! Measured caveat: model construction (arena/table zeroing) sits inside the
//! timed function body and differs slightly per arm — compare per-op deltas
//! between arms, not raw totals.

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
use std::hint::black_box;

#[cfg(target_os = "linux")]
use iai_callgrind::{library_benchmark, library_benchmark_group, main};

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
    /// differently — this asymmetry IS D-3.
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
        debug_assert!(self.is_free(off), "double retire of slot {off}");
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
        debug_assert!(self.is_free(off), "double retire of slot {off}");
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
) {
    let flat = |rounds: &[Vec<u32>]| {
        let mut v: Vec<u32> = rounds.iter().flatten().copied().collect();
        v.sort_unstable();
        v
    };
    let (i, n, l, l2) = (flat(intrusive), flat(next), flat(leaf), flat(leaf_fast));
    assert_eq!(i, n, "identity violated: intrusive vs next_table");
    assert_eq!(i, l, "identity violated: intrusive vs leaf_table");
    assert_eq!(i, l2, "identity violated: intrusive vs leaf_table_fast");
}

#[cfg(target_os = "linux")]
fn check_free_sets(
    intrusive: &[[u64; WORDS_PER_LEAF]],
    next: &[[u64; WORDS_PER_LEAF]],
    leaf: &[[u64; WORDS_PER_LEAF]],
    leaf_fast: &[[u64; WORDS_PER_LEAF]],
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
}

#[cfg(target_os = "linux")]
fn measure_identity() {
    // (a) every drain empties the 256-slot working set.
    check_issued(
        &drive_churn_one_class::<Intrusive>(),
        &drive_churn_one_class::<NextTable>(),
        &drive_churn_one_class::<LeafTable>(),
        &drive_churn_one_class::<LeafTableFast>(),
    );
    // (b) the 4 batches partition the full 4096-slot leaf.
    check_issued(
        &drive_drain_dense::<Intrusive>(),
        &drive_drain_dense::<NextTable>(),
        &drive_drain_dense::<LeafTable>(),
        &drive_drain_dense::<LeafTableFast>(),
    );
    // (c) single drain of the whole k-slot free set.
    for &k in &[1usize, 8, 64] {
        check_issued(
            &drive_drain_sparse::<Intrusive>(k),
            &drive_drain_sparse::<NextTable>(k),
            &drive_drain_sparse::<LeafTable>(k),
            &drive_drain_sparse::<LeafTableFast>(k),
        );
    }
    // (d) each per-leaf drain empties that leaf's 256-slot run.
    for &class in &[class_of(16), class_of(64)] {
        check_issued(
            &drive_refill_cold::<Intrusive>(class),
            &drive_refill_cold::<NextTable>(class),
            &drive_refill_cold::<LeafTable>(class),
            &drive_refill_cold::<LeafTableFast>(class),
        );
    }
    // (e) partial 32-of-64 drains: LIFO (I/N) and address-ordered (L/L2)
    // issuance legitimately picks DIFFERENT slots per round (design §4
    // unknown #4: the LIFO -> address-ordered reissue effect is exactly
    // what is unmeasured). I/N must still agree with each other, and the
    // bookkeeping invariant that must hold for all four is the resulting
    // per-leaf free set — the drain/re-retire cycle restores it fully.
    let (mi, ni, li, l2i) = (
        drive_churn_mixed::<Intrusive>(),
        drive_churn_mixed::<NextTable>(),
        drive_churn_mixed::<LeafTable>(),
        drive_churn_mixed::<LeafTableFast>(),
    );
    assert_eq!(
        normalize(&mi.0),
        normalize(&ni.0),
        "identity violated: intrusive vs next_table"
    );
    check_free_sets(&mi.1, &ni.1, &li.1, &l2i.1);
    // (f) one merge yields the same 64 free slots in every model.
    let (ri, rn, rl, rl2) = (
        remote_merge_intrusive(),
        remote_merge_next_table(),
        remote_merge_leaf(),
        remote_merge_leaf_fast(),
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
fn churn_one_class(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_churn_one_class::<Intrusive>(),
        Model::NextTable => drive_churn_one_class::<NextTable>(),
        Model::LeafTable => drive_churn_one_class::<LeafTable>(),
        Model::LeafTableFast => drive_churn_one_class::<LeafTableFast>(),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
fn drain_dense(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_drain_dense::<Intrusive>(),
        Model::NextTable => drive_drain_dense::<NextTable>(),
        Model::LeafTable => drive_drain_dense::<LeafTable>(),
        Model::LeafTableFast => drive_drain_dense::<LeafTableFast>(),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
fn drain_sparse_1(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_drain_sparse::<Intrusive>(1),
        Model::NextTable => drive_drain_sparse::<NextTable>(1),
        Model::LeafTable => drive_drain_sparse::<LeafTable>(1),
        Model::LeafTableFast => drive_drain_sparse::<LeafTableFast>(1),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
fn drain_sparse_8(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_drain_sparse::<Intrusive>(8),
        Model::NextTable => drive_drain_sparse::<NextTable>(8),
        Model::LeafTable => drive_drain_sparse::<LeafTable>(8),
        Model::LeafTableFast => drive_drain_sparse::<LeafTableFast>(8),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
fn drain_sparse_64(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_drain_sparse::<Intrusive>(64),
        Model::NextTable => drive_drain_sparse::<NextTable>(64),
        Model::LeafTable => drive_drain_sparse::<LeafTable>(64),
        Model::LeafTableFast => drive_drain_sparse::<LeafTableFast>(64),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
fn refill_cold_16(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_refill_cold::<Intrusive>(class_of(16)),
        Model::NextTable => drive_refill_cold::<NextTable>(class_of(16)),
        Model::LeafTable => drive_refill_cold::<LeafTable>(class_of(16)),
        Model::LeafTableFast => drive_refill_cold::<LeafTableFast>(class_of(16)),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
fn refill_cold_64(m: Model) {
    let rounds = match m {
        Model::Intrusive => drive_refill_cold::<Intrusive>(class_of(64)),
        Model::NextTable => drive_refill_cold::<NextTable>(class_of(64)),
        Model::LeafTable => drive_refill_cold::<LeafTable>(class_of(64)),
        Model::LeafTableFast => drive_refill_cold::<LeafTableFast>(class_of(64)),
    };
    black_box(&rounds);
}

#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::intrusive(Model::Intrusive)]
#[bench::next_table(Model::NextTable)]
#[bench::leaf_table(Model::LeafTable)]
#[bench::leaf_table_fast(Model::LeafTableFast)]
fn churn_mixed(m: Model) {
    let (rounds, _free_sets, tail) = match m {
        Model::Intrusive => drive_churn_mixed::<Intrusive>(),
        Model::NextTable => drive_churn_mixed::<NextTable>(),
        Model::LeafTable => drive_churn_mixed::<LeafTable>(),
        Model::LeafTableFast => drive_churn_mixed::<LeafTableFast>(),
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
    };
    black_box(blocks);
    // Constant marker (same value in every arm), not part of the geometry.
    println!("REMOTE_MERGE_BLOCKS {blocks}");
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
);

#[cfg(target_os = "linux")]
main!(library_benchmark_groups = ph3c_leaf_proto);
