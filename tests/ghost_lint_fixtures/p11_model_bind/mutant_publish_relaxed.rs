// expect: prod rule requires ["AcqRel"]
fn credit_lifetime() {
    let producer = thread::spawn(move || {
        // ghost-bind: sidecar/publish-acqrel
        producer_pending.fetch_or(1, Ordering::Relaxed);
    });
    // ghost-bind: sidecar/cut-acqrel
    let detached = pending.swap(0, Ordering::AcqRel);
    producer.join().unwrap();
    // ghost-bind: sidecar/cut-acqrel
    let later = pending.swap(0, Ordering::AcqRel);
}
