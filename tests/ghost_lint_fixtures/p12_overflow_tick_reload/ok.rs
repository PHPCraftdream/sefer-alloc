impl HeapCore {
    pub(crate) fn depot_reload(&mut self, c: usize) -> bool {
        if class.count != 0 || !depot.depot_reload(c, &mut class.parked, &mut class.slots) {
            if class.count == 0 {
                return self.reload_overflow(c);
            }
            return false;
        }
        true
    }
    pub(crate) fn depot_reload_tick(&mut self, c: usize) -> bool {
        if class.count != 0 || !depot.depot_reload(c, &mut class.parked, &mut class.slots) {
            if class.count == 0 {
                return self.reload_overflow_tick(c);
            }
            return false;
        }
        true
    }
}
