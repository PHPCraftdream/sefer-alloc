// expect: catalog site
impl Sidecar {
    fn publish(&self, g: usize, _class: u8) -> bool {
        let prev = self.pending[g / 64].fetch_or(1 << (g % 64), Ordering::AcqRel);
        prev == 0
    }
}
