use core::cell::Cell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::alloc_core::os::SEGMENT;

use super::{RouteError, RouteKind, RoutePin, RouteRegistration};

const SHARDS: usize = 64;
const SLOTS_PER_SHARD: usize = 64;

struct Entry {
    key: usize,
    end: usize,
    root: AtomicPtr<u8>,
    owner: usize,
    kind: RouteKind,
    incarnation: u64,
    pins: usize,
    admitting: bool,
}

struct Shard {
    slots: [Option<Entry>; SLOTS_PER_SHARD],
}

/// Fixed-capacity, process-stable route-identity directory. Retirement
/// closes admission immediately; an occupied slot is reusable after the
/// last pre-existing pin drops, without waiting in the owner path.
pub struct RouteDirectory {
    shards: [Mutex<Shard>; SHARDS],
    next_incarnation: AtomicU64,
}

impl RouteDirectory {
    pub const CAPACITY: usize = SHARDS * SLOTS_PER_SHARD;

    pub const fn new() -> Self {
        Self {
            shards: [const {
                Mutex::new(Shard {
                    slots: [const { None }; SLOTS_PER_SHARD],
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
    pub fn with_initial_incarnation_for_test(value: u64) -> Self {
        let directory = Self::new();
        directory.next_incarnation.store(value, Ordering::Relaxed);
        directory
    }

    /// Register before issue. `route_ptr` selects the numeric segment key
    /// common to all user pointers covered by this route. The caller must
    /// keep the separate reservation alive through all unpublished frees;
    /// pins are not reservation credits.
    pub fn register(
        &self,
        root: *mut u8,
        len: usize,
        route_ptr: *mut u8,
        owner: usize,
        kind: RouteKind,
    ) -> Result<RouteRegistration<'_>, RouteError> {
        let start = root.addr();
        let route_addr = route_ptr.addr();
        let end = start.checked_add(len).ok_or(RouteError::InvalidSpan)?;
        if start == 0
            || start & (SEGMENT - 1) != 0
            || len == 0
            || route_addr < start
            || route_addr >= end
        {
            return Err(RouteError::InvalidSpan);
        }
        if matches!(kind, RouteKind::Small | RouteKind::Primordial) && len != SEGMENT {
            return Err(RouteError::InvalidSpan);
        }
        let key = route_addr & !(SEGMENT - 1);
        let shard = &self.shards[Self::shard_index(key)];
        let mut guard = shard.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut vacant = None;
        for (index, slot) in guard.slots.iter().enumerate() {
            match slot {
                Some(entry) if entry.admitting && entry.key == key => {
                    return Err(RouteError::Duplicate);
                }
                None => {
                    vacant.get_or_insert(index);
                }
                _ => {}
            }
        }
        let index = vacant.ok_or(RouteError::Full)?;
        let incarnation = self
            .next_incarnation
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| RouteError::IncarnationExhausted)?
            + 1;
        guard.slots[index] = Some(Entry {
            key,
            end,
            root: AtomicPtr::new(root),
            owner,
            kind,
            incarnation,
            pins: 0,
            admitting: true,
        });
        drop(guard);
        Ok(RouteRegistration {
            directory: self,
            key,
            index,
            incarnation,
            root: AtomicPtr::new(root),
            owner_only: PhantomData::<Cell<()>>,
        })
    }

    /// Only `ptr.addr()` is used; the pointer is never dereferenced or
    /// converted into an allocator-origin root. Pin acquisition and removal
    /// are serialized by the same shard lock.
    pub fn lookup(&self, ptr: *mut u8) -> Option<RoutePin<'_>> {
        let addr = ptr.addr();
        let key = addr & !(SEGMENT - 1);
        let mut guard = self.shards[Self::shard_index(key)]
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        for (index, slot) in guard.slots.iter_mut().enumerate() {
            let Some(entry) = slot.as_mut() else {
                continue;
            };
            if !entry.admitting
                || entry.key != key
                || addr < entry.root.load(Ordering::Relaxed).addr()
                || addr >= entry.end
            {
                continue;
            }
            let incarnation = entry.incarnation;
            entry.pins = entry.pins.checked_add(1)?;
            if entry.incarnation != incarnation {
                std::process::abort();
            }
            return Some(RoutePin {
                directory: self,
                key,
                index,
                incarnation,
                owner: entry.owner,
                kind: entry.kind,
            });
        }
        None
    }

    pub(super) fn remove(&self, key: usize, index: usize, incarnation: u64) {
        let mut guard = self.shards[Self::shard_index(key)]
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(entry) = guard.slots[index].as_mut() else {
            std::process::abort();
        };
        if entry.key != key || entry.incarnation != incarnation || !entry.admitting {
            std::process::abort();
        }
        entry.admitting = false;
        if entry.pins == 0 {
            guard.slots[index] = None;
        }
    }

    pub(super) fn release_pin(&self, key: usize, index: usize, incarnation: u64) {
        let mut guard = self.shards[Self::shard_index(key)]
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let Some(entry) = guard.slots[index].as_mut() else {
            std::process::abort();
        };
        if entry.key != key || entry.incarnation != incarnation || entry.pins == 0 {
            std::process::abort();
        }
        entry.pins -= 1;
        if entry.pins == 0 && !entry.admitting {
            guard.slots[index] = None;
        }
    }

    const fn shard_index(key: usize) -> usize {
        (key / SEGMENT) % SHARDS
    }
}

impl Default for RouteDirectory {
    fn default() -> Self {
        Self::new()
    }
}
