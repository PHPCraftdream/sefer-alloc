// expect: is not preceded
impl RoutePin {
    pub unsafe fn publish_small_borrowed(&self, offset: u32) -> bool {
        let Some(sidecar) = self.entry.small_sidecar() else {
            return false;
        };
        self.entry.after_publish();
        if !sidecar.publish(offset) {
            return false;
        }
        true
    }
}
