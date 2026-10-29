//! G1-K7: depot overflow — full magazines parked above the per-class depot
//! floor, within a per-heap byte budget, in a lazily committed reservation.
//!
//! The control ([`DepotOverflow`]) lives in the slot's `Depot` and is valid
//! all-zero (empty, nothing reserved). The reservation holds only records:
//! per class a page-aligned region of `cap[c]` records forming a ring — new
//! records go to the tail, reload takes the tail (LIFO with the floor below),
//! decay takes the head (oldest). A class's ring length is
//! `PerClass::parked - DEPOT_K[c]`, also kept here (`len`) so a cold event
//! costs O(1).

use crate::alloc_core::size_classes::SIZE_CLASS_TABLE;
use crate::registry::heap_core::state::depot::DEPOT_CLASSES;
use crate::registry::heap_core::state::tcache::TCACHE_CAP;

type Mag = [*mut u8; TCACHE_CAP];
const REC: usize = core::mem::size_of::<Mag>();
/// Cold events per decay epoch at least (the epoch also spans twice the
/// occupied records, so a full park/reload cycle fits in one epoch).
pub(crate) const EPOCH_MIN: u32 = 256;
/// Decay flushes at most this many records per cold event.
pub(crate) const DECAY_WORK: u32 = 4;

/// TEST-ONLY fault hook: the next `n` overflow reservations fail as if the
/// OS refused them (the commit side uses `aligned-vmem`'s own hook).
#[cfg(feature = "lazy-commit-fault-injection")]
pub(crate) static RESERVE_FAIL_NEXT: core::sync::atomic::AtomicU32 =
    core::sync::atomic::AtomicU32::new(0);

#[cfg(feature = "lazy-commit-fault-injection")]
fn reserve_fault_fires() -> bool {
    use core::sync::atomic::Ordering::Relaxed;
    RESERVE_FAIL_NEXT
        .fetch_update(Relaxed, Relaxed, |n| n.checked_sub(1))
        .is_ok()
}

const _: () = assert!(REC == 128);
const _: () = assert!(DEPOT_CLASSES <= 32);

/// Owner-only counters (diagnostics; read through `bench-internals` hooks).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct OverflowStats {
    pub(crate) parks: u64,
    pub(crate) reloads: u64,
    pub(crate) budget_refusals: u64,
    pub(crate) os_refusals: u64,
    pub(crate) decay_flushed: u64,
    pub(crate) ingress_checks: u64,
    /// Decommit calls issued (not proof the OS dropped the pages).
    pub(crate) decommits: u64,
}

/// Overflow control; all-zero = empty and unreserved. The per-event fields
/// lead (`repr(C)`), so the common tick touches one line.
#[repr(C)]
pub(crate) struct DepotOverflow {
    base: *mut u8,
    bytes: usize,
    events: u32,
    epoch_len: u32,
    /// Classes with decay debt (bit `c`).
    debt_mask: u32,
    /// Classes that parked during the current epoch (bit `c`).
    parked_in_epoch: u32,
    /// Classes whose `PerClass::limit` is latched after a refusal (bit `c`).
    starved: u32,
    reloads_since_ingress: u32,
    reserve_failed: bool,
    len: [u16; DEPOT_CLASSES],
    head: [u16; DEPOT_CLASSES],
    cap: [u16; DEPOT_CLASSES],
    low: [u16; DEPOT_CLASSES],
    debt: [u16; DEPOT_CLASSES],
    off: [u32; DEPOT_CLASSES],
    committed: [u32; DEPOT_CLASSES],
    pub(crate) stats: OverflowStats,
}

const fn mag_bytes(c: usize) -> usize {
    SIZE_CLASS_TABLE[c] * TCACHE_CAP
}

impl DepotOverflow {
    /// Kani-only: the all-zero empty overflow an OS-zeroed slot holds.
    #[cfg(kani)]
    pub(crate) const fn empty() -> Self {
        Self {
            base: core::ptr::null_mut(),
            bytes: 0,
            events: 0,
            epoch_len: 0,
            debt_mask: 0,
            parked_in_epoch: 0,
            starved: 0,
            reloads_since_ingress: 0,
            reserve_failed: false,
            len: [0; DEPOT_CLASSES],
            head: [0; DEPOT_CLASSES],
            cap: [0; DEPOT_CLASSES],
            low: [0; DEPOT_CLASSES],
            debt: [0; DEPOT_CLASSES],
            off: [0; DEPOT_CLASSES],
            committed: [0; DEPOT_CLASSES],
            stats: OverflowStats {
                parks: 0,
                reloads: 0,
                budget_refusals: 0,
                os_refusals: 0,
                decay_flushed: 0,
                ingress_checks: 0,
                decommits: 0,
            },
        }
    }

    /// Kani-only: an overflow whose class `c` ring of `cap` records lives,
    /// fully committed, at `base` (a local buffer of `cap` records).
    #[cfg(kani)]
    pub(crate) fn kani_on_buffer(base: *mut u8, c: usize, cap: u16) -> Self {
        let mut o = Self::empty();
        o.base = base;
        o.cap[c] = cap;
        o.committed[c] = cap as u32 * REC as u32;
        o.epoch_len = EPOCH_MIN;
        o
    }

    /// Bytes of blocks currently parked in the overflow.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// Record bytes currently committed (all classes; the reservation's
    /// first page counts once it exists).
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub(crate) fn committed_bytes(&self) -> usize {
        self.committed.iter().map(|&b| b as usize).sum()
    }

    /// Whether a reservation exists (decay and teardown work only then).
    pub(crate) fn active(&self) -> bool {
        !self.base.is_null()
    }

    /// Ring capacity of class `c` for budget `budget` (records).
    pub(crate) const fn cap_for(budget: usize, c: usize) -> usize {
        budget / mag_bytes(c)
    }

    /// Reserve the per-class regions for `budget` on first use. `false` when
    /// reservation failed now or before (the overflow stays off until trim).
    fn ensure_reserved(&mut self, budget: usize) -> bool {
        if self.active() {
            return true;
        }
        if self.reserve_failed {
            return false;
        }
        let page = aligned_vmem::page_size();
        let mut total = 0usize;
        let mut c = 0;
        while c < DEPOT_CLASSES {
            let cap = Self::cap_for(budget, c);
            self.cap[c] = cap as u16;
            self.off[c] = total as u32;
            total += (cap * REC).next_multiple_of(page);
            c += 1;
        }
        if total == 0 {
            return false;
        }
        #[cfg(feature = "lazy-commit-fault-injection")]
        let reserved = if reserve_fault_fires() {
            None
        } else {
            crate::alloc_core::os::reserve_depot_overflow(total.next_multiple_of(page))
        };
        #[cfg(not(feature = "lazy-commit-fault-injection"))]
        let reserved = crate::alloc_core::os::reserve_depot_overflow(total.next_multiple_of(page));
        match reserved {
            Some(base) => {
                self.base = base;
                // The first page is committed by the reservation.
                self.committed[0] = (page.min(total)) as u32;
                self.epoch_len = EPOCH_MIN;
                true
            }
            None => {
                self.reserve_failed = true;
                false
            }
        }
    }

    /// Commit class `c`'s region through record `idx`; `false` on failure
    /// (nothing changes).
    fn ensure_committed(&mut self, c: usize, idx: usize) -> bool {
        let need = (idx + 1) * REC;
        let have = self.committed[c] as usize;
        if need <= have {
            return true;
        }
        let page = aligned_vmem::page_size();
        let end = need
            .next_multiple_of(page)
            .min((self.cap[c] as usize * REC).next_multiple_of(page));
        let off = self.off[c] as usize;
        if !crate::alloc_core::os::commit_depot_overflow(self.base, off + have, off + end) {
            return false;
        }
        self.committed[c] = end as u32;
        true
    }

    fn record(&self, c: usize, idx: usize) -> *mut Mag {
        self.base
            .wrapping_add(self.off[c] as usize + idx * REC)
            .cast::<Mag>()
    }

    /// Ring slot `head + i` of class `c` (no division: `i < cap`).
    fn slot(&self, c: usize, i: usize) -> usize {
        let idx = self.head[c] as usize + i;
        let cap = self.cap[c] as usize;
        if idx >= cap {
            idx - cap
        } else {
            idx
        }
    }

    /// Class `c`'s overflow length (records).
    pub(crate) fn len(&self, c: usize) -> usize {
        self.len[c] as usize
    }

    /// Park `mag` as class `c`'s newest overflow record; `n` is the class's
    /// current overflow length. `false` (no change to records) when over
    /// budget, full or the storage cannot be reserved / committed.
    pub(crate) fn park(&mut self, c: usize, n: usize, mag: &Mag, budget: usize) -> bool {
        debug_assert_eq!(n, self.len[c] as usize);
        if self.bytes + mag_bytes(c) > budget {
            self.stats.budget_refusals += 1;
            return false;
        }
        if !self.ensure_reserved(budget) {
            self.stats.os_refusals += 1;
            return false;
        }
        if n >= self.cap[c] as usize {
            self.stats.budget_refusals += 1;
            return false;
        }
        let idx = self.slot(c, n);
        if !self.ensure_committed(c, idx) {
            self.stats.os_refusals += 1;
            return false;
        }
        // SAFETY: `idx < cap[c]`, so the record lies inside class `c`'s region
        // of the live reservation, committed through `idx` just above; the
        // region is page-aligned and records are 128 B, so the pointer is
        // aligned for `Mag`. Only the owner reaches this control.
        #[allow(unsafe_code)]
        unsafe {
            self.record(c, idx).write(*mag);
        }
        self.bytes += mag_bytes(c);
        self.len[c] += 1;
        self.parked_in_epoch |= 1 << c;
        self.stats.parks += 1;
        true
    }

    /// Move class `c`'s newest overflow record into `out`; `n` is the class's
    /// current overflow length (`> 0`).
    pub(crate) fn reload(&mut self, c: usize, n: usize, out: &mut Mag) {
        debug_assert!(n > 0 && self.active() && n == self.len[c] as usize);
        let idx = self.slot(c, n - 1);
        // SAFETY: `n > 0` records are live in class `c`'s ring, so `idx` was
        // written by `park` (committed, aligned, inside the reservation).
        #[allow(unsafe_code)]
        unsafe {
            *out = self.record(c, idx).read();
        }
        self.bytes -= mag_bytes(c);
        self.len[c] -= 1;
        self.low[c] = self.low[c].min(self.len[c]);
        // Debt never exceeds the ring: a reload consumes what decay would.
        self.set_debt(c, self.debt[c].min(self.len[c]));
        self.stats.reloads += 1;
    }

    /// Move class `c`'s oldest overflow record into `out` (decay); `n` is the
    /// class's current overflow length (`> 0`).
    pub(crate) fn pop_oldest(&mut self, c: usize, n: usize, out: &mut Mag) {
        debug_assert!(n > 0 && self.active() && n == self.len[c] as usize);
        let idx = self.head[c] as usize;
        // SAFETY: as in `reload` — the head record is live while `n > 0`.
        #[allow(unsafe_code)]
        unsafe {
            *out = self.record(c, idx).read();
        }
        self.head[c] = self.slot(c, 1) as u16;
        self.bytes -= mag_bytes(c);
        self.len[c] -= 1;
        self.low[c] = self.low[c].min(self.len[c]);
        self.set_debt(c, self.debt[c].saturating_sub(1));
        self.stats.decay_flushed += 1;
    }

    fn set_debt(&mut self, c: usize, d: u16) {
        self.debt[c] = d;
        if d == 0 {
            self.debt_mask &= !(1 << c);
        } else {
            self.debt_mask |= 1 << c;
        }
    }

    /// Count one reload from the overflow; `true` every 8th, when the caller
    /// runs the bounded remote-ingress check it otherwise skips.
    pub(crate) fn ingress_due(&mut self) -> bool {
        self.reloads_since_ingress += 1;
        if self.reloads_since_ingress >= 8 {
            self.reloads_since_ingress = 0;
            self.stats.ingress_checks += 1;
            return true;
        }
        false
    }

    /// Count one cold event; `true` when decay work is due (an epoch ended
    /// or debt is outstanding) and the caller must run [`Self::decay_step`].
    #[inline(always)]
    pub(crate) fn tick(&mut self) -> bool {
        if self.bytes == 0 {
            return false;
        }
        self.events += 1;
        self.events >= self.epoch_len || self.debt_mask != 0
    }

    /// Epoch end, if due: each class that parked nothing in the epoch owes
    /// half its low-water mark (rounded up); the next epoch starts from the
    /// current lengths. Returns the class to decay next, if any.
    pub(crate) fn decay_step(&mut self) -> Option<usize> {
        if self.events >= self.epoch_len.max(EPOCH_MIN) {
            let mut total = 0usize;
            let mut c = 0;
            while c < DEPOT_CLASSES {
                let d = if self.parked_in_epoch & (1 << c) == 0 {
                    self.low[c].div_ceil(2).min(self.len[c])
                } else {
                    0
                };
                self.set_debt(c, d);
                self.low[c] = self.len[c];
                total += self.len[c] as usize;
                c += 1;
            }
            self.parked_in_epoch = 0;
            self.events = 0;
            self.epoch_len = (2 * total as u32).max(EPOCH_MIN);
        }
        if self.debt_mask == 0 {
            None
        } else {
            Some(self.debt_mask.trailing_zeros() as usize)
        }
    }

    /// Mark class `c` as latched (its `PerClass::limit` was lowered).
    pub(crate) fn starve(&mut self, c: usize) {
        self.starved |= 1 << c;
    }

    /// Latched classes that may park again under `budget` (room for one more
    /// magazine of theirs, ring not full); their bits are cleared.
    pub(crate) fn take_unstarved(&mut self, budget: usize) -> u32 {
        if self.starved == 0 || self.reserve_failed {
            return 0;
        }
        let mut ready = 0;
        let mut m = self.starved;
        while m != 0 {
            let c = m.trailing_zeros() as usize;
            m &= m - 1;
            let ring_room = !self.active() || self.len[c] < self.cap[c];
            if self.bytes + mag_bytes(c) <= budget && ring_room {
                ready |= 1 << c;
            }
        }
        self.starved &= !ready;
        ready
    }

    /// After the depot is fully drained: return the committed record pages to
    /// the OS, then reset rings, watermarks and decay state (the next park
    /// re-commits). The reservation and its layout stay for reuse.
    pub(crate) fn release_pages(&mut self) {
        debug_assert_eq!(self.bytes, 0, "overflow must be drained first");
        if !self.active() {
            self.reserve_failed = false;
            return;
        }
        let mut c = 0;
        while c < DEPOT_CLASSES {
            let have = self.committed[c] as usize;
            if have != 0 {
                let off = self.off[c] as usize;
                crate::alloc_core::os::decommit_depot_overflow(self.base, off, off + have);
                self.stats.decommits += 1;
            }
            c += 1;
        }
        self.reset_rings();
    }

    /// Empty every ring and drop every commit watermark to 0.
    fn reset_rings(&mut self) {
        self.committed = [0; DEPOT_CLASSES];
        self.len = [0; DEPOT_CLASSES];
        self.debt_mask = 0;
        self.head = [0; DEPOT_CLASSES];
        self.low = [0; DEPOT_CLASSES];
        self.debt = [0; DEPOT_CLASSES];
        self.parked_in_epoch = 0;
        self.events = 0;
        self.epoch_len = EPOCH_MIN;
        self.reloads_since_ingress = 0;
    }
}
