// expect: outside a branch matching ^cacheable$
impl HeapCore {
    unsafe fn publish_foreign_uncached(ptr: *mut u8, layout: Layout) -> bool {
        let Some(pin) = RouteDirectory::global().lookup(ptr) else {
            route_pin_cache::cache_evict(ptr.addr() & !(os::SEGMENT - 1));
            return false;
        };
        match pin.kind() {
            RouteKind::Large => unsafe { pin.publish_large() },
            RouteKind::Small | RouteKind::Primordial => {
                let cacheable =
                    pin.owner() != crate::alloc_core::segment_header::OWNER_ID_FALLBACK as usize;
                let published = unsafe { pin.publish_small_borrowed(offset) };
                route_pin_cache::cache_insert(pin, key, incarnation);
                published
            }
        }
    }
}

impl RoutePin {
    pub fn dbg_cache_insert_for_test(self) {
        crate::registry::heap_core_xthread::route_pin_cache::cache_insert(self, base, incarnation);
    }
}
