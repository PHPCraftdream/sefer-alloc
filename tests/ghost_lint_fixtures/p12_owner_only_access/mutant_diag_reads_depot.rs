// expect: only ["HeapCore::depot_park", "HeapCore::depot_reload", "HeapCore::depot_reload_tick", "HeapCore::park_overflow", "HeapCore::reload_overflow", "HeapCore::depot_tick", "HeapCore::depot_release_overflow_pages", "HeapCore::depot_decay"]
impl HeapCore {
    pub(crate) fn depot_park(&mut self, c: usize) -> bool {
        let Some(depot) = self.depot.as_deref_mut() else {
            return false;
        };
        depot.depot_park(c, &mut class.parked, &class.slots)
    }
    pub(crate) fn depot_reload(&mut self, c: usize) -> bool {
        let Some(depot) = self.depot.as_deref_mut() else {
            return false;
        };
        depot.depot_reload(c, &mut class.parked, &mut class.slots)
    }
    pub fn dbg_depot_peek(&mut self) -> bool {
        self.depot.as_deref_mut().is_some()
    }
    pub(crate) fn bind_depot(&mut self, depot: &'static mut Depot) {
        self.depot = Some(depot);
    }
}
