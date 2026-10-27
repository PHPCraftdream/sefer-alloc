impl Sidecar {
    fn publish(&self, g: usize, class: u8) -> bool {
        self.classes[g].store(class, Ordering::Release);
        let prev = self.pending[g / 64].fetch_or(1 << (g % 64), Ordering::AcqRel);
        prev == 0
    }
}
