impl Core {
    fn flush(&mut self) {
        loop {
            if self.n != 0 {
                self.flush_one();
            }
            if !self.reload() {
                break;
            }
        }
    }
}
