// expect: only ["Core::trim_for_recycle", "Core::owner_flush"] may do this
impl Core {
    fn trim_for_recycle(&mut self) {
        self.depot = Depot::EMPTY;
    }
    fn owner_flush(&mut self) {
        drain(&mut self.depot);
    }
    fn remote_free(&mut self, p: *mut u8) {
        push_to(&mut self.depot, p);
    }
}
