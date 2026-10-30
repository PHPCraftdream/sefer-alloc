//! Audited System-backed route-directory seam: no lookup-derived pointer is
//! dereferenced, and allocator-origin roots stay in entries.
#![allow(unsafe_code)]

use core::cell::Cell;
use core::marker::PhantomData;
use core::mem;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::alloc::{GlobalAlloc, Layout, System};

use crate::alloc_core::fatal::fatal;
use crate::alloc_core::os::SEGMENT;

use super::blocks::{Block, BlockIndex, SpareBlock, BLOCK_CAP, BLOCK_HALF, INITIAL_INDEX_CAP};
use super::ingress::{HeapIngress, SegmentIngress};
use super::{shard_lock::ShardLock, AtomicBool, AtomicUsize};
use super::{LargeState, RouteError, RouteKind, RoutePin, RouteRegistration, SmallSidecar};

const SHARDS: usize = 64;

#[cfg(feature = "internals")]
std::thread_local! {
    static FAIL_NEXT_REGISTRATION: Cell<usize> = const { Cell::new(0) };
}

pub(in super::super) struct Entry {
    pub(in super::super) key: usize,
    pub(in super::super) end: usize,
    pub(in super::super) payload: usize,
    pub(in super::super) root: AtomicPtr<u8>,
    pub(in super::super) owner: usize,
    pub(in super::super) kind: RouteKind,
    pub(in super::super) incarnation: u64,
    pub(in super::super) refs: AtomicUsize,
    pub(in super::super) sidecar: AtomicPtr<u8>,
    pub(in super::super) linked: AtomicBool,
    /// SC-6 A-exact remote-free ingress state of this route.
    pub(in super::super) ingress: SegmentIngress,
}

#[cfg(not(loom))]
const _: () =
    assert!(crate::alloc_core::segment_table::MAX_SEGMENTS == 64 * super::ingress::INGRESS_WORDS);

/// SC-6: the ingress cell of registry slot `owner`; the fallback heap and
/// unbound owners (ids >= `MAX_HEAPS`) have none.
#[cfg(not(loom))]
pub(crate) fn heap_ingress(owner: usize) -> Option<&'static HeapIngress> {
    static CELLS: [HeapIngress; crate::alloc_core::segment_header::MAX_HEAPS] =
        [const { HeapIngress::new() }; crate::alloc_core::segment_header::MAX_HEAPS];
    CELLS.get(owner)
}

/// Loom harnesses register heapless routes (born NOTIFIED).
#[cfg(loom)]
pub(crate) fn heap_ingress(_owner: usize) -> Option<&'static HeapIngress> {
    None
}

/// One counted reference to a stable System allocation; there is no borrowed
/// `&mut Entry` after publication — all shared mutation is atomic.
pub(in super::super) struct EntryHandle {
    ptr: AtomicPtr<Entry>,
}

impl EntryHandle {
    #[allow(clippy::too_many_arguments)]
    fn new(
        root: *mut u8,
        key: usize,
        end: usize,
        payload: usize,
        owner: usize,
        kind: RouteKind,
        incarnation: u64,
        slot: u32,
    ) -> Result<Self, RouteError> {
        let sidecar = match kind {
            RouteKind::Small | RouteKind::Primordial => {
                // SAFETY: System is independent of the installed allocator; the
                // span stays private until `initialize_at` builds every atomic.
                let ptr = unsafe { System.alloc(Layout::new::<SmallSidecar>()) };
                if !ptr.is_null() {
                    #[cfg(feature = "internals")]
                    crate::alloc_core::remote_bitmap::ClassLeaves::record_alloc(
                        Layout::new::<SmallSidecar>().size(),
                        false,
                    );
                    // SAFETY: fresh private storage has SmallSidecar's exact
                    // layout; in-place construction initializes every atomic.
                    unsafe { SmallSidecar::initialize_at(ptr.cast()) };
                }
                ptr
            }
            RouteKind::Large => {
                // SAFETY: System returns a properly aligned allocation for
                // this exact Layout, private until the write below.
                let ptr = unsafe { System.alloc(Layout::new::<LargeState>()) };
                if !ptr.is_null() {
                    // SAFETY: fresh exclusive storage is valid for LargeState.
                    unsafe { ptr.cast::<LargeState>().write(LargeState::new()) };
                }
                ptr
            }
        };
        if sidecar.is_null() {
            return Err(RouteError::OutOfMemory);
        }
        // SAFETY: System's returned storage has Descriptor's exact layout
        // and remains private until ptr::write constructs the full value.
        let ptr = unsafe { System.alloc(Layout::new::<Entry>()) }.cast::<Entry>();
        if ptr.is_null() {
            Self::free_sidecar(kind, sidecar);
            return Err(RouteError::OutOfMemory);
        }
        // SAFETY: fresh, aligned, exclusively owned System storage.
        unsafe {
            ptr.write(Entry {
                key,
                end,
                payload,
                root: AtomicPtr::new(root),
                owner,
                kind,
                incarnation,
                refs: AtomicUsize::new(1),
                sidecar: AtomicPtr::new(sidecar),
                linked: AtomicBool::new(true),
                ingress: SegmentIngress::new(
                    slot,
                    matches!(kind, RouteKind::Small | RouteKind::Primordial)
                        && slot != u32::MAX
                        && heap_ingress(owner).is_some(),
                ),
            });
        }
        Ok(Self {
            ptr: AtomicPtr::new(ptr),
        })
    }

    fn free_sidecar(kind: RouteKind, sidecar: *mut u8) {
        match kind {
            RouteKind::Small | RouteKind::Primordial => {
                // SAFETY: pointer is a fully initialized SmallSidecar from
                // System; last-pin release ensures no remaining reference.
                unsafe { ptr::drop_in_place(sidecar.cast::<SmallSidecar>()) };
                #[cfg(feature = "internals")]
                crate::alloc_core::remote_bitmap::ClassLeaves::record_free(
                    Layout::new::<SmallSidecar>().size(),
                );
                // SAFETY: pointer came from System.alloc with this Layout
                // and its mixed leaves were released by Drop above.
                unsafe { System.dealloc(sidecar, Layout::new::<SmallSidecar>()) };
            }
            RouteKind::Large => {
                // SAFETY: pointer came from System.alloc with this Layout and
                // LargeState has no destructor or remaining reference.
                unsafe { System.dealloc(sidecar, Layout::new::<LargeState>()) };
            }
        }
    }

    pub(in super::super) fn entry(&self) -> &Entry {
        // SAFETY: this handle owns one counted reference to a fully
        // initialized, non-moving Entry; final free occurs after its Drop.
        unsafe { &*self.ptr.load(Ordering::Relaxed) }
    }

    fn ptr(&self) -> *mut Entry {
        self.ptr.load(Ordering::Relaxed)
    }

    #[allow(deprecated)] // `fetch_update` is deprecated for `try_update`, which needs a newer MSRV.
    fn pin_from_array(ptr: *mut Entry) -> Self {
        // SAFETY: caller holds the shard lock, which excludes unlink and
        // release of the owner reference while the count is incremented.
        let entry = unsafe { &*ptr };
        match entry
            .refs
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
        {
            // Item 186: a pin is only ever taken while some reference is alive.
            Ok(_old) => crate::ghost::ghost_assert!(
                "route-pin/no-resurrection",
                _old != 0,
                "pin taken on an entry whose last reference is gone"
            ),
            // A valid free must never silently lose its route on exhaustion.
            Err(_) => fatal(b"directory: pin reference count overflow\n"),
        }
        Self {
            ptr: AtomicPtr::new(ptr),
        }
    }

    pub(in super::super) fn key(&self) -> usize {
        self.entry().key
    }
    pub(in super::super) fn incarnation(&self) -> u64 {
        self.entry().incarnation
    }
    pub(in super::super) fn owner(&self) -> usize {
        self.entry().owner
    }
    pub(in super::super) fn kind(&self) -> RouteKind {
        self.entry().kind
    }
    pub(in super::super) fn root(&self) -> *mut u8 {
        self.entry().root.load(Ordering::Relaxed)
    }
    pub(in super::super) fn contains_payload(&self, addr: usize, size: usize) -> bool {
        let entry = self.entry();
        addr >= entry.payload
            && (!matches!(entry.kind, RouteKind::Large)
                || crate::alloc_core::AllocCore::large_payload_address_matches(
                    addr,
                    entry.payload,
                    0,
                ))
            && addr.checked_add(size).is_some_and(|end| end <= entry.end)
    }

    pub(in super::super) fn small_sidecar(&self) -> Option<&SmallSidecar> {
        if matches!(self.kind(), RouteKind::Large) {
            return None;
        }
        let ptr = self
            .entry()
            .sidecar
            .load(Ordering::Relaxed)
            .cast::<SmallSidecar>();
        // SAFETY: this counted handle retains the System allocation; only
        // atomic fields are accessed through the shared reference.
        Some(unsafe { &*ptr })
    }

    /// SC-6 producer half: after a successful small publication (SeqCst
    /// RMW), notify the owner once per arming.
    #[inline(always)]
    pub(in super::super) fn after_publish(&self) {
        let entry = self.entry();
        entry.ingress.after_publish(|| heap_ingress(entry.owner));
    }

    /// SC-6 owner half: re-arm before a full drain of this route's sidecar.
    #[cfg_attr(feature = "numa-aware", allow(dead_code))]
    pub(in super::super) fn arm_ingress(&self) {
        self.entry().ingress.arm();
    }

    pub(in super::super) fn large_state(&self) -> Option<&LargeState> {
        if !matches!(self.kind(), RouteKind::Large) {
            return None;
        }
        let ptr = self
            .entry()
            .sidecar
            .load(Ordering::Relaxed)
            .cast::<LargeState>();
        // SAFETY: this counted handle retains the initialized System value;
        // shared access is only through LargeState's atomic word.
        Some(unsafe { &*ptr })
    }
}

impl Drop for EntryHandle {
    fn drop(&mut self) {
        let ptr = self.ptr();
        // SAFETY: this handle owns one count. Owner unlink happens before its
        // count is dropped; pins can only be acquired while linked under lock.
        let entry = unsafe { &*ptr };
        let prev = entry.refs.fetch_sub(1, Ordering::AcqRel);
        crate::ghost::ghost_assert!(
            "route-pin/refs-no-underflow",
            prev != 0,
            "route entry refs underflow"
        );
        if prev != 1 {
            return;
        }
        // Freed exactly once per entry (the Loom harness checks the count).
        crate::ghost::ghost_event!("route-pin/sidecar-freed", ptr.addr(), 1);
        let sidecar = entry.sidecar.load(Ordering::Relaxed);
        Self::free_sidecar(entry.kind, sidecar);
        // SAFETY: only the final ref reaches here. Exact original System
        // Layout; no reservation pointer is released or dereferenced.
        unsafe { System.dealloc(ptr.cast(), Layout::new::<Entry>()) };
    }
}

/// What an insert needs allocated outside the lock before it can proceed.
enum Insert {
    Done,
    Duplicate,
    Need { block: bool, index_cap: usize },
}

/// Allocations to release after the shard lock is dropped.
struct Surplus {
    _block: Option<SpareBlock>,
    _index: Option<BlockIndex>,
}

/// Blocks displaced by removal, released after the lock; `System.dealloc` never runs under it.
struct Retired {
    blocks: [Option<SpareBlock>; 2],
}

impl Retired {
    fn push(&mut self, block: SpareBlock) {
        if self.blocks[0].is_none() {
            self.blocks[0] = Some(block);
        } else {
            self.blocks[1] = Some(block);
        }
    }
}

/// Two-level sorted shard: an index of blocks, each at most `BLOCK_CAP` pointers. Updates
/// shift within one block (plus the short block index on split/merge), all under the shard lock.
struct Shard {
    index: BlockIndex,
    /// One retained empty block, so splits after removals never allocate.
    spare: AtomicPtr<Block>,
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    moved: u64,
}

impl Shard {
    const fn empty() -> Self {
        Self {
            index: BlockIndex::empty(),
            spare: AtomicPtr::new(ptr::null_mut()),
            #[cfg(all(feature = "internals", feature = "bench-internals"))]
            moved: 0,
        }
    }

    fn note_moved(&mut self, cells: usize) {
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        {
            self.moved += cells as u64;
        }
        #[cfg(not(all(feature = "internals", feature = "bench-internals")))]
        let _ = cells;
    }

    fn block(&self, index: usize) -> &Block {
        // SAFETY: linked blocks are live System allocations owned by this
        // shard; the shard lock excludes concurrent mutation.
        unsafe { &*self.index.get(index) }
    }

    /// Block that owns or would own `key`, and the position inside it.
    fn locate(&self, key: usize) -> Option<(usize, Result<usize, usize>)> {
        let (mut lo, mut hi) = (0, self.index.len);
        if hi == 0 {
            return None;
        }
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.block(mid).key_at(0) <= key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let block = lo.saturating_sub(1);
        Some((block, self.block(block).find(key)))
    }

    fn find(&self, key: usize) -> Option<*mut Entry> {
        let (block, found) = self.locate(key)?;
        Some(self.block(block).get(found.ok()?))
    }

    fn insert(&mut self, key: usize, entry: *mut Entry) -> Insert {
        let (block, pos) = match self.locate(key) {
            None => (0, 0),
            Some((_, Ok(_))) => return Insert::Duplicate,
            Some((block, Err(pos))) => (block, pos),
        };
        let needs_block = self.index.len == 0 || self.block(block).len == BLOCK_CAP;
        if needs_block {
            let block = self.spare.load(Ordering::Relaxed).is_null();
            let index_cap = if self.index.len < self.index.cap {
                0
            } else if self.index.cap == 0 {
                INITIAL_INDEX_CAP
            } else {
                self.index.cap.saturating_mul(2)
            };
            if block || index_cap != 0 {
                return Insert::Need { block, index_cap };
            }
        }
        let mut moved = 0;
        let (target, pos) = if !needs_block {
            (self.index.get(block), pos)
        } else if self.index.len == 0 {
            let fresh = self.spare.swap(ptr::null_mut(), Ordering::Relaxed);
            moved += self.index.insert(0, fresh);
            (fresh, 0)
        } else {
            let fresh = self.spare.swap(ptr::null_mut(), Ordering::Relaxed);
            let old = self.index.get(block);
            // SAFETY: two distinct live blocks (the spare is unlinked and
            // empty), both exclusively reachable under the shard lock.
            moved += unsafe { &mut *old }.split_into(unsafe { &mut *fresh });
            moved += self.index.insert(block + 1, fresh);
            if pos <= BLOCK_HALF {
                (old, pos)
            } else {
                (fresh, pos - BLOCK_HALF)
            }
        };
        // SAFETY: `target` is a linked live block with room for one entry.
        moved += unsafe { &mut *target }.insert_at(pos, entry);
        self.note_moved(moved);
        Insert::Done
    }

    fn remove(&mut self, key: usize, entry: *mut Entry) -> Retired {
        let Some((block, Ok(pos))) = self.locate(key) else {
            fatal(b"directory: remove of unindexed key\n");
        };
        let raw = self.index.get(block);
        // SAFETY: linked live block under the shard lock.
        let blk = unsafe { &mut *raw };
        if blk.get(pos) != entry {
            fatal(b"directory: removed entry does not match key\n");
        }
        let mut retired = Retired {
            blocks: [None, None],
        };
        let mut moved = blk.remove_at(pos);
        if blk.len == 0 {
            moved += self.index.remove(block);
            self.retire(raw, &mut retired);
            self.note_moved(moved);
            return retired;
        }
        if block + 1 < self.index.len {
            let next = self.index.get(block + 1);
            // SAFETY: distinct linked live blocks under the shard lock.
            if blk.len + unsafe { &*next }.len <= BLOCK_HALF {
                moved += blk.absorb(unsafe { &mut *next });
                moved += self.index.remove(block + 1);
                self.retire(next, &mut retired);
            }
        }
        if block > 0 {
            let prev = self.index.get(block - 1);
            // SAFETY: distinct linked live blocks under the shard lock.
            if unsafe { &*prev }.len + blk.len <= BLOCK_HALF {
                moved += unsafe { &mut *prev }.absorb(blk);
                moved += self.index.remove(block);
                self.retire(raw, &mut retired);
            }
        }
        self.note_moved(moved);
        retired
    }

    /// Keeps one empty block for the next split; other displaced blocks are
    /// retired and released after the shard lock is dropped (the `Surplus`
    /// pattern: no `System.dealloc` under the lock).
    fn retire(&mut self, block: *mut Block, retired: &mut Retired) {
        if self.spare.load(Ordering::Relaxed).is_null() {
            self.spare.store(block, Ordering::Relaxed);
        } else {
            retired.push(SpareBlock::from_raw(block));
        }
    }

    /// Installs outside-lock allocations; surplus ones are returned to drop
    /// after the lock is released.
    fn install(&mut self, block: Option<SpareBlock>, index: Option<BlockIndex>) -> Surplus {
        let mut surplus = Surplus {
            _block: None,
            _index: None,
        };
        if let Some(block) = block {
            if self.spare.load(Ordering::Relaxed).is_null() {
                self.spare.store(block.into_raw(), Ordering::Relaxed);
            } else {
                surplus._block = Some(block);
            }
        }
        if let Some(mut enlarged) = index {
            if self.index.len == self.index.cap && self.index.cap < enlarged.cap {
                let moved = enlarged.copy_from(&self.index);
                self.note_moved(moved);
                surplus._index = Some(mem::replace(&mut self.index, enlarged));
            } else {
                surplus._index = Some(enlarged);
            }
        }
        surplus
    }
}

impl Drop for Shard {
    fn drop(&mut self) {
        for i in 0..self.index.len {
            // SAFETY: linked blocks are owned by the shard and unreachable
            // once it drops; entries are not owned and are not touched.
            unsafe { Block::release(self.index.get(i)) };
        }
        let spare = self.spare.load(Ordering::Relaxed);
        if !spare.is_null() {
            // SAFETY: the retained spare is an unlinked, owned block.
            unsafe { Block::release(spare) };
        }
    }
}

/// Process-stable directory; route count limited by System memory, O(log n) lookup
/// under one shard mutex.
pub struct RouteDirectory {
    shards: [ShardLock<Shard>; SHARDS],
    next_incarnation: AtomicU64,
}

impl RouteDirectory {
    pub const fn new() -> Self {
        Self {
            shards: [const { ShardLock::new(Shard::empty()) }; SHARDS],
            next_incarnation: AtomicU64::new(0),
        }
    }

    pub fn global() -> &'static Self {
        static ROUTES: RouteDirectory = RouteDirectory::new();
        &ROUTES
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn fail_next_registration_for_test() {
        let _ = FAIL_NEXT_REGISTRATION.try_with(|left| left.set(1));
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn fail_next_registrations_for_test(count: usize) {
        let _ = FAIL_NEXT_REGISTRATION.try_with(|left| left.set(count));
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn with_initial_incarnation_for_test(value: u64) -> Self {
        let directory = Self::new();
        directory.next_incarnation.store(value, Ordering::Relaxed);
        directory
    }

    /// Pointer cells retained: block indexes plus whole blocks (spares too).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn retained_pointer_capacity_for_test(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| {
                let guard = shard.lock();
                let spare = usize::from(!guard.spare.load(Ordering::Relaxed).is_null());
                guard.index.cap + (guard.index.len + spare) * BLOCK_CAP
            })
            .sum()
    }

    /// Pointer cells moved by updates (shifts, splits, merges, index growth
    /// copies) since construction.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn moved_pointer_cells_for_test(&self) -> u64 {
        self.shards.iter().map(|shard| shard.lock().moved).sum()
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn shard_index_for_test(key: usize) -> usize {
        Self::shard_index(key)
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn live_route_census_for_test(&self) -> (usize, usize, usize) {
        let mut counts = (0, 0, 0);
        for shard in &self.shards {
            let guard = shard.lock();
            for b in 0..guard.index.len {
                let block = guard.block(b);
                for index in 0..block.len {
                    // SAFETY: the shard lock retains the linked owner reference.
                    match unsafe { &*block.get(index) }.kind {
                        RouteKind::Small => counts.0 += 1,
                        RouteKind::Primordial => counts.1 += 1,
                        RouteKind::Large => counts.2 += 1,
                    }
                }
            }
        }
        counts
    }

    /// Test surface of [`Self::register_route`] (R6-T38), `internals` only.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn register(
        &self,
        root: *mut u8,
        len: usize,
        route_ptr: *mut u8,
        owner: usize,
        kind: RouteKind,
    ) -> Result<RouteRegistration<'_>, RouteError> {
        self.register_route(root, len, route_ptr, owner, kind, u32::MAX)
    }

    /// Registration and all System allocations precede user-visible issue.
    /// `root` must be the allocator-origin usable pointer. Every pointer
    /// issued under this registration must have `route_ptr`'s segment key.
    #[allow(deprecated)] // `fetch_update` is deprecated for `try_update`, which needs a newer MSRV.
    pub(crate) fn register_route(
        &self,
        root: *mut u8,
        len: usize,
        route_ptr: *mut u8,
        owner: usize,
        kind: RouteKind,
        slot: u32,
    ) -> Result<RouteRegistration<'_>, RouteError> {
        #[cfg(feature = "internals")]
        if FAIL_NEXT_REGISTRATION
            .try_with(|left| {
                let remaining = left.get();
                left.set(remaining.saturating_sub(1));
                remaining != 0
            })
            .unwrap_or(false)
        {
            return Err(RouteError::OutOfMemory);
        }
        let start = root.addr();
        let route_addr = route_ptr.addr();
        let end = start.checked_add(len).ok_or(RouteError::InvalidSpan)?;
        if start == 0
            || start & (aligned_vmem::page_size() - 1) != 0
            || len == 0
            || route_addr < start
            || route_addr >= end
        {
            return Err(RouteError::InvalidSpan);
        }
        if matches!(kind, RouteKind::Small | RouteKind::Primordial)
            && (len != SEGMENT || start & (SEGMENT - 1) != 0)
        {
            return Err(RouteError::InvalidSpan);
        }
        let key = route_addr & !(SEGMENT - 1);
        let incarnation = self
            .next_incarnation
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| RouteError::IncarnationExhausted)?
            + 1;
        let entry = EntryHandle::new(root, key, end, route_addr, owner, kind, incarnation, slot)?;
        let shard = &self.shards[Self::shard_index(key)];
        loop {
            let mut guard = shard
                .lock_unless_exiting(
                    crate::global::process_exit_in_progress,
                    crate::global::fallback::record_exit_lock_giveup,
                )
                .ok_or(RouteError::ProcessExiting)?;
            let (need_block, index_cap) = match guard.insert(key, entry.ptr()) {
                Insert::Done => {
                    drop(guard);
                    return Ok(RouteRegistration {
                        directory: self,
                        entry: core::mem::ManuallyDrop::new(entry),
                        owner_only: PhantomData::<Cell<()>>,
                    });
                }
                Insert::Duplicate => {
                    drop(guard);
                    return Err(RouteError::Duplicate);
                }
                Insert::Need { block, index_cap } => (block, index_cap),
            };
            drop(guard);
            let block = if need_block {
                Some(SpareBlock::allocate()?)
            } else {
                None
            };
            let enlarged = if index_cap != 0 {
                Some(BlockIndex::with_capacity(index_cap)?)
            } else {
                None
            };
            let mut guard = shard
                .lock_unless_exiting(
                    crate::global::process_exit_in_progress,
                    crate::global::fallback::record_exit_lock_giveup,
                )
                .ok_or(RouteError::ProcessExiting)?;
            let surplus = guard.install(block, enlarged);
            drop(guard);
            drop(surplus);
        }
    }

    /// Runs a test closure while holding the shard selected by the address.
    /// The pointer is only a numeric shard key and is never dereferenced.
    /// Reentering this shard from `f` can deadlock: a liveness risk, not a
    /// memory-safety precondition. Unwinding releases the ordinary guard.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn dbg_with_shard_of_locked_for_test(&self, ptr: *mut u8, f: impl FnOnce()) {
        let key = ptr.addr() & !(SEGMENT - 1);
        let guard = self.shards[Self::shard_index(key)].lock();
        f();
        drop(guard);
    }

    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn dbg_route_pin_refs(pin: &RoutePin) -> usize {
        pin.entry.entry().refs.load(Ordering::Relaxed)
    }

    /// Only `ptr.addr()` is used as a key; no reservation byte is read.
    pub fn lookup(&self, ptr: *mut u8) -> Option<RoutePin> {
        #[cfg(feature = "bench-internals")]
        crate::alloc_core::B4_DIRECTORY_LOOKUPS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        let (addr, key) = (ptr.addr(), ptr.addr() & !(SEGMENT - 1));
        let guard = self.shards[Self::shard_index(key)].lock_unless_exiting(
            crate::global::process_exit_in_progress,
            crate::global::fallback::record_exit_lock_giveup,
        )?;
        let raw = guard.find(key)?;
        // SAFETY: the shard lock excludes unlink and final release; the entry
        // is fully initialized and still owner-referenced.
        let entry = unsafe { &*raw };
        if addr < entry.payload
            || addr >= entry.end
            || (matches!(entry.kind, RouteKind::Large)
                && !crate::alloc_core::AllocCore::large_payload_address_matches(
                    addr,
                    entry.payload,
                    0,
                ))
        {
            return None;
        }
        let incarnation = entry.incarnation;
        let pin = EntryHandle::pin_from_array(raw);
        if guard.find(key) != Some(pin.ptr()) || pin.incarnation() != incarnation {
            fatal(b"directory: pin key or incarnation mismatch\n");
        }
        drop(guard);
        Some(RoutePin { entry: pin })
    }

    pub(in super::super) fn remove(&self, entry: &EntryHandle) -> bool {
        let key = entry.key();
        let shard = &self.shards[Self::shard_index(key)];
        let Some(mut guard) = shard.lock_unless_exiting(
            crate::global::process_exit_in_progress,
            crate::global::fallback::record_exit_lock_giveup,
        ) else {
            return false;
        };
        // `_retired` (plain System blocks, Drop never re-enters) and the owner
        // ref drop after the lock; Release `linked = false` publishes the unlink.
        let _retired = guard.remove(key, entry.ptr());
        entry.entry().linked.store(false, Ordering::Release);
        drop(guard);
        true
    }

    const fn shard_index(key: usize) -> usize {
        let value = key / SEGMENT;
        (value ^ (value >> 13) ^ (value >> 26)) % SHARDS
    }
}

impl Default for RouteDirectory {
    fn default() -> Self {
        Self::new()
    }
}
