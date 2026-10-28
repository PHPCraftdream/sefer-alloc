// expect: no ghost_assert!/ghost_event! with id pin-cache/held
struct PinCache {
    slots: [PinCacheSlot; PIN_CACHE_SLOTS],
    next_victim: usize,
}

impl PinCache {
    fn sweep_dead_slots(&mut self) {
        for slot in &mut self.slots {
            if slot.pin.as_ref().is_some_and(|pin| !pin.linked()) {
                slot.pin = None;
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
            }
        }
    }
}

impl Drop for PinCache {
    fn drop(&mut self) {
        for slot in &mut self.slots {
            crate::ghost::ghost_event!("pin-cache/held", slot.base, -i64::from(slot.pin.is_some()));
            slot.pin = None;
        }
    }
}

#[cfg(all(loom, test))]
loom::thread_local! {
    static CACHE: RefCell<PinCache> = RefCell::new(PinCache::EMPTY);
}

#[cfg(not(all(loom, test)))]
thread_local! {
    static CACHE: RefCell<PinCache> = const { RefCell::new(PinCache::EMPTY) };
}

pub(crate) unsafe fn publish_cached(ptr: *mut u8) -> CachedPublication {
    CACHE
        .try_with(|cell| {
            let Ok(mut cache) = cell.try_borrow_mut() else {
                return CachedPublication::Miss;
            };
            let Some(slot) = cache.slots.iter_mut().find(|slot| slot.base == base) else {
                cache.sweep_dead_slots();
                return CachedPublication::Miss;
            };
            if let Some(pin) = slot.pin.as_ref() {
                if !pin.linked() {
                    slot.pin = None;
                    crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                    return CachedPublication::Miss;
                }
                if pin.incarnation() != slot.incarnation {
                    slot.pin = None;
                    crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
                    return CachedPublication::Miss;
                }
                return CachedPublication::Published(unsafe { pin.publish_small_borrowed(offset) });
            }
            CachedPublication::Miss
        })
        .unwrap_or(CachedPublication::Miss)
}

pub(crate) fn cache_insert(pin: RoutePin, base: usize, incarnation: u64) {
    let inserted = CACHE.try_with(|cell| {
        let Ok(mut cache) = cell.try_borrow_mut() else {
            return Err(pin);
        };
        if let Some(slot) = cache.slots.iter_mut().find(|slot| slot.base == base) {
            if slot.pin.take().is_some() {
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
            }
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        } else {
            let slot = &mut cache.slots[victim];
            if slot.pin.take().is_some() {
                crate::ghost::ghost_event!("pin-cache/held", slot.base, -1);
            }
            slot.base = base;
            slot.pin = Some(pin);
            crate::ghost::ghost_event!("pin-cache/held", slot.base, 1);
        }
        Ok(())
    });
    drop(inserted);
}

pub(crate) fn cache_evict(base: usize) {
    let _ = CACHE.try_with(|cell| {
        let Ok(mut cache) = cell.try_borrow_mut() else {
            return;
        };
        if let Some(slot) = cache.slots.iter_mut().find(|slot| slot.base == base) {
            slot.pin = None;
        }
    });
}
