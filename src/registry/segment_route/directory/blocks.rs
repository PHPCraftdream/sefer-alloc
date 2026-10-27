//! Route-directory storage: fixed-capacity sorted blocks of entry pointers,
//! the per-shard block index and spare blocks (System-backed; mutated only
//! under the shard lock).
#![allow(unsafe_code)]

use core::mem;
use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};
use std::alloc::{GlobalAlloc, Layout, System};

use crate::alloc_core::fatal::fatal;

use super::route_directory::Entry;
use super::RouteError;

/// Block capacity: the most pointer cells one insert/remove shifts.
pub(super) const BLOCK_CAP: usize = 64;
/// Split point, and merge ceiling for two adjacent blocks.
pub(super) const BLOCK_HALF: usize = BLOCK_CAP / 2;
pub(super) const INITIAL_INDEX_CAP: usize = 8;

/// Fixed-capacity sorted run of non-owning entry pointers; a linked block is never empty.
pub(super) struct Block {
    pub(super) len: usize,
    pub(super) slots: [AtomicPtr<Entry>; BLOCK_CAP],
}

impl Block {
    pub(super) fn allocate() -> Result<*mut Block, RouteError> {
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
    pub(super) unsafe fn release(ptr: *mut Block) {
        // SAFETY: caller contract; the exact original Layout.
        unsafe { System.dealloc(ptr.cast(), Layout::new::<Block>()) };
    }

    pub(super) fn get(&self, index: usize) -> *mut Entry {
        if index >= self.len {
            fatal(b"directory: block get index out of range\n");
        }
        self.slots[index].load(Ordering::Relaxed)
    }

    pub(super) fn key_at(&self, index: usize) -> usize {
        // SAFETY: the shard lock keeps every indexed Entry linked, and its
        // owner registration retains its reference until unlink.
        unsafe { &*self.get(index) }.key
    }

    pub(super) fn find(&self, key: usize) -> Result<usize, usize> {
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
    pub(super) fn insert_at(&mut self, index: usize, value: *mut Entry) -> usize {
        if self.len == BLOCK_CAP || index > self.len {
            fatal(b"directory: block insert out of range\n");
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
    pub(super) fn remove_at(&mut self, index: usize) -> usize {
        if index >= self.len {
            fatal(b"directory: block remove index out of range\n");
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
    pub(super) fn split_into(&mut self, dst: &mut Block) -> usize {
        if dst.len != 0 || self.len != BLOCK_CAP {
            fatal(b"directory: block split precondition violated\n");
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
    pub(super) fn absorb(&mut self, other: &mut Block) -> usize {
        if self.len + other.len > BLOCK_CAP {
            fatal(b"directory: block absorb overflow\n");
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

/// Sorted index of owned `Block`s by first key; capacity is retained after removals.
pub(super) struct BlockIndex {
    pub(super) ptr: AtomicPtr<AtomicPtr<Block>>,
    pub(super) len: usize,
    pub(super) cap: usize,
}

impl BlockIndex {
    pub(super) const fn empty() -> Self {
        Self {
            ptr: AtomicPtr::new(ptr::null_mut()),
            len: 0,
            cap: 0,
        }
    }

    pub(super) fn with_capacity(cap: usize) -> Result<Self, RouteError> {
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

    pub(super) fn cell(&self, index: usize) -> &AtomicPtr<Block> {
        if index >= self.cap {
            fatal(b"directory: index cell out of range\n");
        }
        // SAFETY: index < cap in the live System array; every slot is an
        // initialized atomic, mutated only by the shard-lock holder.
        unsafe { &*self.ptr.load(Ordering::Relaxed).add(index) }
    }

    pub(super) fn get(&self, index: usize) -> *mut Block {
        if index >= self.len {
            fatal(b"directory: index get out of range\n");
        }
        self.cell(index).load(Ordering::Relaxed)
    }

    /// Returns the number of pointer cells moved.
    pub(super) fn copy_from(&mut self, old: &Self) -> usize {
        if self.cap < old.len {
            fatal(b"directory: index copy capacity too small\n");
        }
        for i in 0..old.len {
            self.cell(i).store(old.get(i), Ordering::Relaxed);
        }
        self.len = old.len;
        old.len
    }

    /// Returns the number of pointer cells moved.
    pub(super) fn insert(&mut self, index: usize, value: *mut Block) -> usize {
        if self.len == self.cap || index > self.len {
            fatal(b"directory: index insert out of range\n");
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
    pub(super) fn remove(&mut self, index: usize) -> usize {
        if index >= self.len {
            fatal(b"directory: index remove out of range\n");
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
        let layout = Layout::array::<AtomicPtr<Block>>(self.cap).unwrap_or_else(|_| {
            fatal(b"directory: index layout overflow\n");
        });
        // SAFETY: this is the allocation from with_capacity, with the exact
        // original Layout. The index does not own its blocks.
        unsafe { System.dealloc(self.ptr.load(Ordering::Relaxed).cast(), layout) };
    }
}

/// A System-allocated block not yet linked into a shard.
pub(super) struct SpareBlock(*mut Block);

impl SpareBlock {
    pub(super) fn allocate() -> Result<Self, RouteError> {
        Block::allocate().map(Self)
    }

    pub(super) fn into_raw(self) -> *mut Block {
        let ptr = self.0;
        mem::forget(self);
        ptr
    }

    pub(super) fn from_raw(block: *mut Block) -> Self {
        Self(block)
    }
}

impl Drop for SpareBlock {
    fn drop(&mut self) {
        // SAFETY: allocated by Block::allocate and never linked or shared.
        unsafe { Block::release(self.0) };
    }
}
