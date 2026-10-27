// expect: is not preceded
impl Core {
    fn flush(&mut self) {
        loop {
            if !self.reload() {
                break;
            }
            if self.n != 0 {
                self.flush_one();
            }
        }
    }
}
