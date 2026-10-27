impl Drop for EntryHandle {
    fn drop(&mut self) {
        let ptr = self.ptr();
        let entry = unsafe { &*ptr };
        let prev = entry.refs.fetch_sub(1, Ordering::AcqRel);
        crate::ghost::ghost_assert!("route-pin/refs-no-underflow", prev != 0, "route entry refs underflow");
        if prev != 1 {
            return;
        }
        crate::ghost::ghost_event!("route-pin/sidecar-freed", ptr.addr(), 1);
        let sidecar = entry.sidecar.load(Ordering::Relaxed);
        Self::free_sidecar(entry.kind, sidecar);
        unsafe { System.dealloc(ptr.cast(), Layout::new::<Entry>()) };
    }
}
impl RouteDirectory {
    fn lookup(&self) {}
}
