// expect: only ["HeapCore::bind_depot"]
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
        let ok = depot.depot_reload(c, &mut class.parked, &mut class.slots);
        self.depot = None;
        ok
    }
    pub(crate) fn bind_depot(&mut self, depot: &'static mut Depot) {
        self.depot = Some(depot);
    }
}
