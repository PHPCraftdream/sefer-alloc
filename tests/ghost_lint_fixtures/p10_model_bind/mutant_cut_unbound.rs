// expect: no `// ghost-bind
fn credit_lifetime() {
    let producer = thread::spawn(move || {
        // ghost-bind: sidecar/publish-acqrel
        producer_pending.fetch_or(1, Ordering::AcqRel);
    });
    let detached = pending.swap(0, Ordering::AcqRel);
    producer.join().unwrap();
    let later = pending.swap(0, Ordering::AcqRel);
}
