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
    static FAIL_NEXT_REGISTRATION: Cell<bool> = const { Cell::new(false) };
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
                // SAFETY: System is independent of the installed global allocator.
                // AtomicU64/AtomicU8 have their integer representations; zero
                // is valid for every field of SmallSidecar.
                unsafe { System.alloc_zeroed(Layout::new::<SmallSidecar>()) }
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
                // SAFETY: pointer came from System.alloc_zeroed with this Layout
                // and no reference remains at this call site.
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

/// Sorted, non-owning pointers. Capacity is retained after removals.
struct PointerArray {
    ptr: AtomicPtr<AtomicPtr<Entry>>,
    len: usize,
    cap: usize,
}

impl PointerArray {
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
        let layout = Layout::array::<AtomicPtr<Entry>>(cap).map_err(|_| RouteError::OutOfMemory)?;
        // SAFETY: System uses the exact requested Layout; the span stays
        // private until all cap atomic pointer values are constructed.
        let ptr = unsafe { System.alloc(layout) }.cast::<AtomicPtr<Entry>>();
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

    fn get(&self, index: usize) -> *mut Entry {
        if index >= self.len {
            std::process::abort();
        }
        // SAFETY: index < len <= cap and the System array is initialized;
        // all mutations of its slots are serialized by the shard lock.
        unsafe { &*self.ptr.load(Ordering::Relaxed).add(index) }.load(Ordering::Relaxed)
    }

    fn set(&mut self, index: usize, value: *mut Entry) {
        if index >= self.cap {
            std::process::abort();
        }
        // SAFETY: index < cap in the live System array; only the shard-lock
        // holder calls this and pointer slots are initialized atomics.
        unsafe { &*self.ptr.load(Ordering::Relaxed).add(index) }.store(value, Ordering::Relaxed);
    }

    fn find(&self, key: usize) -> Result<usize, usize> {
        let (mut lo, mut hi) = (0, self.len);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            // SAFETY: the shard lock keeps this indexed Entry linked, and
            // its owner registration retains its reference until unlink.
            let mid_key = unsafe { &*self.get(mid) }.key;
            if mid_key < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo < self.len {
            // SAFETY: same shard-lock/owner-reference invariant as above.
            if unsafe { &*self.get(lo) }.key == key {
                return Ok(lo);
            }
        }
        Err(lo)
    }

    fn copy_from(&mut self, old: &Self) {
        if self.cap < old.len {
            std::process::abort();
        }
        for i in 0..old.len {
            self.set(i, old.get(i));
        }
        self.len = old.len;
    }

    fn insert(&mut self, index: usize, value: *mut Entry) {
        if self.len == self.cap || index > self.len {
            std::process::abort();
        }
        for i in (index..self.len).rev() {
            self.set(i + 1, self.get(i));
        }
        self.set(index, value);
        self.len += 1;
    }

    fn remove(&mut self, index: usize) {
        if index >= self.len {
            std::process::abort();
        }
        for i in index..self.len - 1 {
            self.set(i, self.get(i + 1));
        }
        self.len -= 1;
        self.set(self.len, ptr::null_mut());
    }
}

impl Drop for PointerArray {
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }
        let layout =
            Layout::array::<AtomicPtr<Entry>>(self.cap).unwrap_or_else(|_| std::process::abort());
        // SAFETY: this is the allocation from with_capacity, with the exact
        // original Layout. The array does not own its referenced entries.
        unsafe { System.dealloc(self.ptr.load(Ordering::Relaxed).cast(), layout) };
    }
}

struct Shard {
    array: PointerArray,
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
            shards: [const {
                Mutex::new(Shard {
                    array: PointerArray::empty(),
                })
            }; SHARDS],
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
        let _ = FAIL_NEXT_REGISTRATION.try_with(|flag| flag.set(true));
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn with_initial_incarnation_for_test(value: u64) -> Self {
        let directory = Self::new();
        directory.next_incarnation.store(value, Ordering::Relaxed);
        directory
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn retained_pointer_capacity_for_test(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| shard.lock().unwrap_or_else(|e| e.into_inner()).array.cap)
            .sum()
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
            .try_with(|flag| flag.replace(false))
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
            let index = match guard.array.find(key) {
                Ok(_) => {
                    drop(guard);
                    return Err(RouteError::Duplicate);
                }
                Err(index) => index,
            };
            if guard.array.len < guard.array.cap {
                guard.array.insert(index, entry.ptr());
                drop(guard);
                return Ok(RouteRegistration {
                    directory: self,
                    entry,
                    owner_only: PhantomData::<Cell<()>>,
                });
            }
            let target = if guard.array.cap == 0 {
                8
            } else {
                guard
                    .array
                    .cap
                    .checked_mul(2)
                    .ok_or(RouteError::OutOfMemory)?
            };
            drop(guard);
            let mut enlarged = PointerArray::with_capacity(target)?;
            let mut guard = shard.lock().unwrap_or_else(|e| e.into_inner());
            if guard.array.len == guard.array.cap && guard.array.cap < target {
                enlarged.copy_from(&guard.array);
                let old = mem::replace(&mut guard.array, enlarged);
                drop(guard);
                drop(old);
            } else {
                drop(guard);
                drop(enlarged);
            }
        }
    }

    /// Only `ptr.addr()` is used as a key; no reservation byte is read.
    pub fn lookup(&self, ptr: *mut u8) -> Option<RoutePin> {
        let addr = ptr.addr();
        let key = addr & !(SEGMENT - 1);
        let guard = self.shards[Self::shard_index(key)]
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let index = guard.array.find(key).ok()?;
        let raw = guard.array.get(index);
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
        if guard.array.get(index) != pin.ptr() || pin.incarnation() != incarnation {
            std::process::abort();
        }
        drop(guard);
        Some(RoutePin { entry: pin })
    }

    pub(super) fn remove(&self, entry: &EntryHandle) {
        let key = entry.key();
        let shard = &self.shards[Self::shard_index(key)];
        let mut guard = shard.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(index) = guard.array.find(key) else {
            std::process::abort();
        };
        if guard.array.get(index) != entry.ptr() {
            std::process::abort();
        }
        guard.array.remove(index);
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
