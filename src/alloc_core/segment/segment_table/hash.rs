use super::{SegmentTable, HASH_CAPACITY, SEGMENT_SHIFT};
// The fetch_max sites that consume this static are `alloc-stats`-gated.
#[cfg(feature = "alloc-stats")]
use super::HASH_REMOVE_MAX_SCAN_STEPS;

impl SegmentTable {
    // -------------------------------------------------------------------
    // OPT-B — open-addressing hash table helpers
    //
    // The hash table lives in the primordial segment immediately after the
    // slots array. Capacity = HASH_CAPACITY (a power of two). Two-state
    // encoding (backward-shift deletion — R4-8/N3 — never leaves tombstones):
    //   - null_mut()  → empty  (stops a probe chain)
    //   - other       → live segment base (SEGMENT-aligned, never null)
    //
    // All reads/writes go through the `node` seam, keeping this file
    // safe while meeting the crate's unsafe-confinement requirement.
    // -------------------------------------------------------------------

    /// Hash a segment base to an initial slot index.
    ///
    /// `base` is SEGMENT-aligned, so its low `SEGMENT_SHIFT` bits are zero.
    /// Right-shifting by `SEGMENT_SHIFT` gives a dense integer key (one key
    /// per segment in the virtual address space). We mask to `HASH_CAPACITY - 1`
    /// for a fast modulo (power-of-two capacity).
    #[inline(always)]
    fn hash_index(base: *mut u8) -> usize {
        (base as usize >> SEGMENT_SHIFT) & (HASH_CAPACITY - 1)
    }

    /// Address of hash slot `i`. Pure pointer arithmetic through the `node` seam.
    #[inline(always)]
    fn hash_slot_ptr(&self, i: usize) -> *mut *mut u8 {
        crate::alloc_core::node::Node::offset(
            self.hash_slots as *mut u8,
            i * core::mem::size_of::<*mut u8>(),
        ) as *mut *mut u8
    }

    /// Read the value stored at hash slot `i`.
    #[inline(always)]
    fn hash_slot_read(&self, i: usize) -> *mut u8 {
        crate::alloc_core::node::Node::read_struct::<*mut u8>(self.hash_slot_ptr(i))
    }

    /// Write `value` into hash slot `i`.
    #[inline]
    fn hash_slot_write(&mut self, i: usize, value: *mut u8) {
        crate::alloc_core::node::Node::write_struct::<*mut u8>(self.hash_slot_ptr(i), value);
    }

    /// Insert `base` into the hash table using linear probing.
    ///
    /// Scans forward from `hash_index(base)` (with wrap-around) until an empty
    /// slot is found, then writes `base` there.
    ///
    /// **Precondition:** the caller guarantees `base` is not already in the
    /// table AND at least one empty slot exists (load factor ≤ 50%).
    pub(super) fn hash_insert(&mut self, base: *mut u8) {
        let start = Self::hash_index(base);
        let mut i = start;
        loop {
            let entry = self.hash_slot_read(i);
            if entry.is_null() {
                // Empty slot: this slot is available.
                self.hash_slot_write(i, base);
                return;
            }
            i = (i + 1) & (HASH_CAPACITY - 1);
            // Under the load-factor ≤ 50% guarantee we will always find a
            // free slot before wrapping all the way around. The loop must
            // terminate: at least HASH_CAPACITY/2 slots are empty.
            debug_assert!(i != start, "hash table full — load factor exceeded");
        }
    }

    /// Remove `base` from the hash table using **backward-shift deletion**
    /// (R4-8/N3). This is the classic technique for open-addressing linear-
    /// probing tables: deleting an entry by writing a tombstone would leave a
    /// hole that future `hash_contains` probes must skip, and without periodic
    /// rebuild those holes accumulate forever (the original W2 perf-metastable
    /// collapse). Backward-shift deletion instead REPAIRS the probe chain at
    /// delete time, leaving a clean empty slot — so no tombstones ever exist,
    /// and no rebuild is ever needed.
    ///
    /// The cost is bounded by the CURRENT cluster length (the run of live
    /// entries that probed past `i`), NOT by `HASH_CAPACITY` — and it is paid
    /// as a normal part of every delete, not as a periodic O(`HASH_CAPACITY`)
    /// spike concentrated on one unlucky call (the N3 tail-latency regression).
    ///
    /// ## The shift-eligibility condition (the correctness crux)
    ///
    /// After nulling the slot at index `i` (the removed entry), we walk forward
    /// from `i+1`. For each subsequent live entry `e` at index `j`, we must
    /// decide: can `e` be moved back to fill the hole at `hole`, or must it
    /// stay where it is? The rule is:
    ///
    /// **`e` is eligible to move back to `hole` iff `e` does NOT probe over
    /// `hole` on its way to `j`** — i.e., moving `e` to `hole` does not skip
    /// its own ideal bucket. Measure both distances FORWARD, mod `HASH_CAPACITY`
    /// (call it `C`): `dist_to_hole = (hole - home_e) mod C` and
    /// `dist_to_j = (j - home_e) mod C`. `e` is eligible iff
    /// `dist_to_hole <= dist_to_j` — i.e. `hole` is reached from `home_e` no
    /// later than `j` is. (Equality would imply `hole == j`, which never occurs
    /// — `hole` always trails `j` by ≥ 1 along the walk.) When eligible, we copy
    /// `e` to `hole`, and `j` becomes the new `hole`; we then continue scanning
    /// forward from `j+1` to fill the new hole. When NOT eligible, `e` stays and
    /// we scan forward from `j+1` (the hole at the OLD position is still open).
    ///
    /// Why this preserves `hash_contains` for EVERY key: a future probe for `e`
    /// starts at `home_e` and walks forward. Pre-shift it would pass through
    /// `hole` (then `hole+1..j`) to reach `e` at `j`. Post-shift `e` is at
    /// `hole`, which is strictly EARLIER in probe order than `j` and still at
    /// or after `home_e` (that is exactly the eligibility check), so the probe
    /// still finds `e` — and finds it sooner. A key `e'` that probed PAST `j`
    /// also probed past `hole` (since `hole` is before `j`), so moving the live
    /// entry at `j` to `hole` does not insert a gap into `e'`'s probe chain —
    /// the chain is contiguous live entries from `home_e'` to `e'`, and we only
    /// ever relocate an entry to an earlier-or-equal probe position within that
    /// same contiguous run. The final empty slot (where the shift ends) is the
    /// gap that was ALWAYS there at the end of the cluster — it just moved left.
    ///
    /// Only called under `alloc-decommit` or `alloc-xthread` (from `recycle`
    /// and `unregister`); the lint is suppressed for builds with neither, to
    /// keep the code uniform.
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    pub(super) fn hash_remove(&mut self, base: *mut u8) {
        let start = Self::hash_index(base);
        let mask = HASH_CAPACITY - 1;
        let mut i = start;
        // R23-6 (task #375): count of probe/scan steps THIS call takes, both
        // phases (find + backward-shift) combined — the exact quantity the
        // "no single delete does O(HASH_CAPACITY) work" claim is about. Local
        // to this call; folded into the process-wide max at the end.
        #[cfg(feature = "alloc-stats")]
        let mut steps: u64 = 0;
        // 1. Find the slot currently holding `base`.
        loop {
            let entry = self.hash_slot_read(i);
            if entry.is_null() {
                // Empty slot: probe chain terminates; base is not present.
                // Defensive no-op (caller bug) — do not corrupt the table.
                return;
            }
            if entry == base {
                break;
            }
            // A different live entry: skip and continue probing.
            i = (i + 1) & mask;
            #[cfg(feature = "alloc-stats")]
            {
                steps += 1;
            }
            debug_assert!(i != start, "hash_remove looped without finding base");
        }
        // 2. Backward-shift deletion: fill the hole at `i` by pulling later
        //    entries in the cluster back, preserving every probe chain. `hole`
        //    is the gap in the probe chain we are filling; physically slot
        //    `hole` may still hold a STALE value (the removed `base`, or a
        //    just-moved entry's old copy) until it is overwritten or nulled —
        //    it is never re-read as a candidate, because `j` only walks forward
        //    past it. `j` scans forward for a candidate to fill `hole`.
        let mut hole = i;
        let mut j = (i + 1) & mask;
        loop {
            let entry = self.hash_slot_read(j);
            if entry.is_null() {
                // End of the cluster — nothing more to shift. The slot at
                // `hole` stays empty, which is the correct terminal state.
                break;
            }
            // Eligibility: can `entry` (whose ideal slot is `home`) legally
            // occupy `hole`? `entry` is eligible iff `hole` lies in the closed
            // probe interval [home, j] (mod C) — i.e. moving it to `hole` does
            // not place it before its own ideal bucket. Measure both distances
            // FORWARD (mod C): `entry` is eligible iff the distance from `home`
            // to `hole` is <= the distance from `home` to `j`. (Equal only when
            // `hole == j`, which never occurs since `hole < j` along the walk.)
            let home = Self::hash_index(entry);
            let dist_hole = (hole.wrapping_sub(home)) & mask;
            let dist_j = (j.wrapping_sub(home)) & mask;
            let eligible = dist_hole <= dist_j;

            if eligible {
                // Move `entry` back to `hole`; the vacated slot `j` becomes
                // the new hole for the next iteration.
                self.hash_slot_write(hole, entry);
                hole = j;
            }
            j = (j + 1) & mask;
            #[cfg(feature = "alloc-stats")]
            {
                steps += 1;
            }
            debug_assert!(
                j != i,
                "hash_remove backward-shift looped — no empty slot found in cluster"
            );
        }
        // 3. The final hole is genuinely empty now (nothing shifted into it).
        self.hash_slot_write(hole, core::ptr::null_mut());
        // R23-6: fold this call's step count into the process-wide max (a
        // relaxed compare-exchange loop — `fetch_max` is not available pre-
        // 1.45-independent of MSRV concerns here, but the crate already uses
        // plain AtomicU64; use `fetch_max`, stable since Rust 1.45).
        #[cfg(feature = "alloc-stats")]
        {
            HASH_REMOVE_MAX_SCAN_STEPS.fetch_max(steps, core::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Check whether `base` is present in the hash table (O(1) average).
    ///
    /// Scans forward from `hash_index(base)` until:
    /// - `base` is found → returns `true`
    /// - an empty slot (`null_mut()`) is reached → returns `false`
    ///
    /// Backward-shift deletion (R4-8/N3) guarantees every non-live slot is
    /// genuinely empty (no tombstones), so an empty slot unambiguously
    /// terminates the probe — a `false` result is always correct.
    #[inline(always)]
    pub(super) fn hash_contains(&self, base: *mut u8) -> bool {
        let start = Self::hash_index(base);
        let mut i = start;
        loop {
            let entry = self.hash_slot_read(i);
            if entry.is_null() {
                // Empty slot: the probe chain ends here; base is not present.
                return false;
            }
            if entry == base {
                return true;
            }
            // A different live entry: skip and continue.
            i = (i + 1) & (HASH_CAPACITY - 1);
            if i == start {
                // Wrapped all the way around without finding base or an empty
                // slot. This can only happen if the table is completely full of
                // live entries. Under the guaranteed ≤ 50% load factor this
                // cannot occur, but handle it defensively.
                return false;
            }
        }
    }
}
