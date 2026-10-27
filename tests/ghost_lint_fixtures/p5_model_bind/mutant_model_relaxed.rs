// expect: prod rule requires ["AcqRel"]
struct Route;
impl Route {
    fn unpin(&self) {
        // ghost-bind: route-pin/drop-acqrel
        if self.refs.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.freed.store(true, Ordering::Release);
        }
    }
    fn unlink(&self) {
        let _g = self.lock.lock().unwrap();
        // ghost-bind: route-pin/unlink-release
        self.linked.store(false, Ordering::Release);
    }
}
