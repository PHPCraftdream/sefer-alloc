// expect: prod rule requires ["Release"]
struct Route;
impl Route {
    fn unpin(&self) {
        // ghost-bind: route-pin/drop-acqrel
        if self.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.freed.store(true, Ordering::Release);
        }
    }
    fn unlink(&self) {
        let _g = self.lock.lock().unwrap();
        // ghost-bind: route-pin/unlink-release
        self.linked.store(false, Ordering::Relaxed);
    }
}
