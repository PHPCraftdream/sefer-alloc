impl Core {
    fn trim_for_recycle(&mut self) {
        self.depot = Depot::EMPTY;
    }
    fn owner_flush(&mut self) {
        drain(&mut self.depot);
    }
    fn remote_free(&self, p: *mut u8) {
        self.ring.push(p);
    }
}
