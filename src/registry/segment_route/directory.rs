//! Audited System-backed route-directory seam. No pointer derived from a
//! lookup address is dereferenced; allocator-origin roots stay in entries.
#![allow(unsafe_code)]

use core::cell::Cell;
use core::marker::PhantomData;
use core::mem;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;

use crate::alloc_core::os::SEGMENT;

use super::{LargeState, RouteError, RouteKind, RoutePin, RouteRegistration, SmallSidecar};

const SHARDS: usize = 64;

#[cfg(feature = "internals")]
std::thread_local! {
    static FAIL_NEXT_REGISTRATION: Cell<usize> = const { Cell::new(0) };
}

struct Entry {
    key: usize,
    end: usize,
    payload: usize,
    root: AtomicPtr<u8>,
    owner: usize,
    kind: RouteKind,
    incarnation: u64,
    refs: AtomicUsize,
    sidecar: AtomicPtr<u8>,
}

/// One counted reference to a stable System allocation. There is no borrowed
/// `&mut Entry` after publication; all shared mutation is atomic.
pub(super) struct EntryHandle {
    ptr: AtomicPtr<Entry>,
}

impl EntryHandle {
    fn new(
        root: *mut u8,
        key: usize,
        end: usize,
        payload: usize,
        owner: usize,
        kind: RouteKind,
        incarnation: u64,
    ) -> Result<Self, RouteError> {
        let sidecar = match kind {
            RouteKind::Small | RouteKind::Primordial => {
                // SAFETY: System is independent of the installed allocator;
                // the aligned span stays private until every atomic is built.
                let ptr = unsafe { System.alloc_zeroed(Layout::new::<SmallSidecar>()) };
                if !ptr.is_null() {
                    #[cfg(feature = "internals")]
                    crate::alloc_core::remote_bitmap::ClassLeaves::record_alloc(
                        Layout::new::<SmallSidecar>().size(),
                        true,
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
                // SAFETY: pointer came from System.alloc_zeroed with this Layout
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

    fn entry(&self) -> &Entry {
        // SAFETY: this handle owns one counted reference to a fully
        // initialized, non-moving Entry; final free occurs after its Drop.
        unsafe { &*self.ptr.load(Ordering::Relaxed) }
    }

    fn ptr(&self) -> *mut Entry {
        self.ptr.load(Ordering::Relaxed)
    }

    fn pin_from_array(ptr: *mut Entry) -> Self {
        // SAFETY: caller holds the shard lock, which excludes unlink and
        // release of the owner reference while the count is incremented.
        let entry = unsafe { &*ptr };
        if entry
            .refs
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .is_err()
        {
            // A valid free must never silently lose its route on exhaustion.
            std::process::abort();
        }
        Self {
            ptr: AtomicPtr::new(ptr),
        }
    }

    pub(super) fn key(&self) -> usize {
        self.entry().key
    }
    pub(super) fn incarnation(&self) -> u64 {
        self.entry().incarnation
    }
    pub(super) fn owner(&self) -> usize {
        self.entry().owner
    }
    pub(super) fn kind(&self) -> RouteKind {
        self.entry().kind
    }
    pub(super) fn root(&self) -> *mut u8 {
        self.entry().root.load(Ordering::Relaxed)
    }
    pub(super) fn contains_payload(&self, addr: usize, size: usize) -> bool {
        let entry = self.entry();
        addr >= entry.payload
            && (!matches!(entry.kind, RouteKind::Large) || addr == entry.payload)
            && addr.checked_add(size).is_some_and(|end| end <= entry.end)
    }

    pub(super) fn small_sidecar(&self) -> Option<&SmallSidecar> {
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

    pub(super) fn large_state(&self) -> Option<&LargeState> {
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
        if entry.refs.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }
        let sidecar = entry.sidecar.load(Ordering::Relaxed);
        Self::free_sidecar(entry.kind, sidecar);
        // SAFETY: only the final ref reaches here. Exact original System
        // Layout; no reservation pointer is released or dereferenced.
        unsafe { System.dealloc(ptr.cast(), Layout::new::<Entry>()) };
    }
}

/// Block capacity: the most pointer cells one insert/remove shifts.
const BLOCK_CAP: usize = 64;
/// Split point, and merge ceiling for two adjacent blocks.
const BLOCK_HALF: usize = BLOCK_CAP / 2;
const INITIAL_INDEX_CAP: usize = 8;

/// Fixed-capacity sorted run of non-owning entry pointers. A block linked in
/// a shard index is never empty.
struct Block {
    len: usize,
    slots: [AtomicPtr<Entry>; BLOCK_CAP],
}

impl Block {
    fn allocate() -> Result<*mut Block, RouteError> {
        // SAFETY: System uses the exact Layout of Block; the span stays
        // private until the write below constructs the whole value.
        let ptr = unsafe { System.alloc(Layout::new::<Block>()) }.cast::<Block>();
        if ptr.is_null() {
            return Err(RouteError::OutOfMemory);
        }
        // SAFETY: fresh, aligned, exclusively owned System storage.
        unsafe {
            ptr.write(Block {
                len: 0,
                slots: [const { AtomicPtr::new(ptr::null_mut()) }; BLOCK_CAP],
            });
        }
        Ok(ptr)
    }

    /// # Safety
    /// `ptr` came from `Block::allocate`, is unlinked from every shard
    /// structure, and is not used afterwards. Blocks own no entries.
    unsafe fn release(ptr: *mut Block) {
        // SAFETY: caller contract; the exact original Layout.
        unsafe { System.dealloc(ptr.cast(), Layout::new::<Block>()) };
    }

    fn get(&self, index: usize) -> *mut Entry {
        if index >= self.len {
            std::process::abort();
        }
        self.slots[index].load(Ordering::Relaxed)
    }

    fn key_at(&self, index: usize) -> usize {
        // SAFETY: the shard lock keeps every indexed Entry linked, and its
        // owner registration retains its reference until unlink.
        unsafe { &*self.get(index) }.key
    }

    fn find(&self, key: usize) -> Result<usize, usize> {
        let (mut lo, mut hi) = (0, self.len);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.key_at(mid) < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo < self.len && self.key_at(lo) == key {
            return Ok(lo);
        }
        Err(lo)
    }

    /// Returns the number of pointer cells moved.
    fn insert_at(&mut self, index: usize, value: *mut Entry) -> usize {
        if self.len == BLOCK_CAP || index > self.len {
            std::process::abort();
        }
        for i in (index..self.len).rev() {
            let moved = self.slots[i].load(Ordering::Relaxed);
            self.slots[i + 1].store(moved, Ordering::Relaxed);
        }
        self.slots[index].store(value, Ordering::Relaxed);
        self.len += 1;
        self.len - 1 - index
    }

    /// Returns the number of pointer cells moved.
    fn remove_at(&mut self, index: usize) -> usize {
        if index >= self.len {
            std::process::abort();
        }
        for i in index..self.len - 1 {
            let moved = self.slots[i + 1].load(Ordering::Relaxed);
            self.slots[i].store(moved, Ordering::Relaxed);
        }
        self.len -= 1;
        self.slots[self.len].store(ptr::null_mut(), Ordering::Relaxed);
        self.len - index
    }

    /// Moves the upper half into the empty `dst`; returns cells moved.
    fn split_into(&mut self, dst: &mut Block) -> usize {
        if dst.len != 0 || self.len != BLOCK_CAP {
            std::process::abort();
        }
        for i in BLOCK_HALF..BLOCK_CAP {
            let moved = self.slots[i].swap(ptr::null_mut(), Ordering::Relaxed);
            dst.slots[i - BLOCK_HALF].store(moved, Ordering::Relaxed);
        }
        self.len = BLOCK_HALF;
        dst.len = BLOCK_CAP - BLOCK_HALF;
        BLOCK_CAP - BLOCK_HALF
    }

    /// Appends every cell of the right neighbour `other` (all its keys are
    /// larger) and empties it; returns cells moved.
    fn absorb(&mut self, other: &mut Block) -> usize {
        if self.len + other.len > BLOCK_CAP {
            std::process::abort();
        }
        let moved = other.len;
        for i in 0..moved {
            let value = other.slots[i].swap(ptr::null_mut(), Ordering::Relaxed);
            self.slots[self.len + i].store(value, Ordering::Relaxed);
        }
        self.len += moved;
        other.len = 0;
        moved
    }
}

/// Sorted index of owned `Block`s by their first key. Capacity is retained
/// after removals; the blocks themselves are released by `Shard`.
struct BlockIndex {
    ptr: AtomicPtr<AtomicPtr<Block>>,
    len: usize,
    cap: usize,
}

impl BlockIndex {
    const fn empty() -> Self {
        Self {
            ptr: AtomicPtr::new(ptr::null_mut()),
            len: 0,
            cap: 0,
        }
    }

    fn with_capacity(cap: usize) -> Result<Self, RouteError> {
        if cap == 0 {
            return Err(RouteError::OutOfMemory);
        }
        let layout = Layout::array::<AtomicPtr<Block>>(cap).map_err(|_| RouteError::OutOfMemory)?;
        // SAFETY: System uses the exact requested Layout; the span stays
        // private until all cap atomic pointer values are constructed.
        let ptr = unsafe { System.alloc(layout) }.cast::<AtomicPtr<Block>>();
        if ptr.is_null() {
            return Err(RouteError::OutOfMemory);
        }
        for index in 0..cap {
            // SAFETY: index < cap within the fresh aligned System span;
            // each slot is initialized exactly once before publication.
            unsafe { ptr.add(index).write(AtomicPtr::new(ptr::null_mut())) };
        }
        Ok(Self {
            ptr: AtomicPtr::new(ptr),
            len: 0,
            cap,
        })
    }

    fn cell(&self, index: usize) -> &AtomicPtr<Block> {
        if index >= self.cap {
            std::process::abort();
        }
        // SAFETY: index < cap in the live System array; every slot is an
        // initialized atomic, mutated only by the shard-lock holder.
        unsafe { &*self.ptr.load(Ordering::Relaxed).add(index) }
    }

    fn get(&self, index: usize) -> *mut Block {
        if index >= self.len {
            std::process::abort();
        }
        self.cell(index).load(Ordering::Relaxed)
    }

    /// Returns the number of pointer cells moved.
    fn copy_from(&mut self, old: &Self) -> usize {
        if self.cap < old.len {
            std::process::abort();
        }
        for i in 0..old.len {
            self.cell(i).store(old.get(i), Ordering::Relaxed);
        }
        self.len = old.len;
        old.len
    }

    /// Returns the number of pointer cells moved.
    fn insert(&mut self, index: usize, value: *mut Block) -> usize {
        if self.len == self.cap || index > self.len {
            std::process::abort();
        }
        for i in (index..self.len).rev() {
            self.cell(i + 1)
                .store(self.cell(i).load(Ordering::Relaxed), Ordering::Relaxed);
        }
        self.cell(index).store(value, Ordering::Relaxed);
        self.len += 1;
        self.len - 1 - index
    }

    /// Returns the number of pointer cells moved.
    fn remove(&mut self, index: usize) -> usize {
        if index >= self.len {
            std::process::abort();
        }
        for i in index..self.len - 1 {
            self.cell(i)
                .store(self.cell(i + 1).load(Ordering::Relaxed), Ordering::Relaxed);
        }
        self.len -= 1;
        self.cell(self.len)
            .store(ptr::null_mut(), Ordering::Relaxed);
        self.len - index
    }
}

impl Drop for BlockIndex {
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }
        let layout =
            Layout::array::<AtomicPtr<Block>>(self.cap).unwrap_or_else(|_| std::process::abort());
        // SAFETY: this is the allocation from with_capacity, with the exact
        // original Layout. The index does not own its blocks.
        unsafe { System.dealloc(self.ptr.load(Ordering::Relaxed).cast(), layout) };
    }
}

/// A System-allocated block not yet linked into a shard.
struct SpareBlock(*mut Block);

impl SpareBlock {
    fn allocate() -> Result<Self, RouteError> {
        Block::allocate().map(Self)
    }

    fn into_raw(self) -> *mut Block {
        let ptr = self.0;
        mem::forget(self);
        ptr
    }
}

impl Drop for SpareBlock {
    fn drop(&mut self) {
        // SAFETY: allocated by Block::allocate and never linked or shared.
        unsafe { Block::release(self.0) };
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

/// Two-level sorted shard: an index of blocks, each at most `BLOCK_CAP`
/// pointers. Updates shift within one block (plus the short block index on
/// split/merge), never the whole shard. All access is under the shard lock.
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

    fn remove(&mut self, key: usize, entry: *mut Entry) {
        let Some((block, Ok(pos))) = self.locate(key) else {
            std::process::abort();
        };
        let raw = self.index.get(block);
        // SAFETY: linked live block under the shard lock.
        let blk = unsafe { &mut *raw };
        if blk.get(pos) != entry {
            std::process::abort();
        }
        let mut moved = blk.remove_at(pos);
        if blk.len == 0 {
            moved += self.index.remove(block);
            self.recycle(raw);
            self.note_moved(moved);
            return;
        }
        if block + 1 < self.index.len {
            let next = self.index.get(block + 1);
            // SAFETY: distinct linked live blocks under the shard lock.
            if blk.len + unsafe { &*next }.len <= BLOCK_HALF {
                moved += blk.absorb(unsafe { &mut *next });
                moved += self.index.remove(block + 1);
                self.recycle(next);
            }
        }
        if block > 0 {
            let prev = self.index.get(block - 1);
            // SAFETY: distinct linked live blocks under the shard lock.
            if unsafe { &*prev }.len + blk.len <= BLOCK_HALF {
                moved += unsafe { &mut *prev }.absorb(blk);
                moved += self.index.remove(block);
                self.recycle(raw);
            }
        }
        self.note_moved(moved);
    }

    /// Keeps one empty block for the next split, releases any other.
    fn recycle(&mut self, block: *mut Block) {
        if self.spare.load(Ordering::Relaxed).is_null() {
            self.spare.store(block, Ordering::Relaxed);
        } else {
            // SAFETY: the block was just unlinked and is empty; no other
            // reference exists under the shard lock.
            unsafe { Block::release(block) };
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

/// Process-stable directory; route count is limited by System memory, not
/// a fixed slot table. Lookup is O(log routes-in-shard) under one mutex.
pub struct RouteDirectory {
    shards: [Mutex<Shard>; SHARDS],
    next_incarnation: AtomicU64,
}

impl RouteDirectory {
    pub const fn new() -> Self {
        Self {
            shards: [const { Mutex::new(Shard::empty()) }; SHARDS],
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
                let guard = shard.lock().unwrap_or_else(|e| e.into_inner());
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
        self.shards
            .iter()
            .map(|shard| shard.lock().unwrap_or_else(|e| e.into_inner()).moved)
            .sum()
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
            let guard = shard.lock().unwrap_or_else(|e| e.into_inner());
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

    /// Registration and all System allocations precede user-visible issue.
    /// `root` must be the allocator-origin usable pointer. Every pointer
    /// issued under this registration must have `route_ptr`'s segment key.
    pub fn register(
        &self,
        root: *mut u8,
        len: usize,
        route_ptr: *mut u8,
        owner: usize,
        kind: RouteKind,
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
        let entry = EntryHandle::new(root, key, end, route_addr, owner, kind, incarnation)?;
        let shard = &self.shards[Self::shard_index(key)];
        loop {
            let mut guard = shard.lock().unwrap_or_else(|e| e.into_inner());
            let (need_block, index_cap) = match guard.insert(key, entry.ptr()) {
                Insert::Done => {
                    drop(guard);
                    return Ok(RouteRegistration {
                        directory: self,
                        entry,
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
            let mut guard = shard.lock().unwrap_or_else(|e| e.into_inner());
            let surplus = guard.install(block, enlarged);
            drop(guard);
            drop(surplus);
        }
    }

    /// Only `ptr.addr()` is used as a key; no reservation byte is read.
    pub fn lookup(&self, ptr: *mut u8) -> Option<RoutePin> {
        let addr = ptr.addr();
        let key = addr & !(SEGMENT - 1);
        let guard = self.shards[Self::shard_index(key)]
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let raw = guard.find(key)?;
        // SAFETY: the shard lock excludes unlink and final release; the
        // indexed entry is fully initialized and still owner-referenced.
        let entry = unsafe { &*raw };
        if addr < entry.payload
            || addr >= entry.end
            || (matches!(entry.kind, RouteKind::Large) && addr != entry.payload)
        {
            return None;
        }
        let incarnation = entry.incarnation;
        let pin = EntryHandle::pin_from_array(raw);
        if guard.find(key) != Some(pin.ptr()) || pin.incarnation() != incarnation {
            std::process::abort();
        }
        drop(guard);
        Some(RoutePin { entry: pin })
    }

    pub(super) fn remove(&self, entry: &EntryHandle) {
        let key = entry.key();
        let shard = &self.shards[Self::shard_index(key)];
        let mut guard = shard.lock().unwrap_or_else(|e| e.into_inner());
        guard.remove(key, entry.ptr());
        // The caller drops the owner reference after this lock is released.
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
