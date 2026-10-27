// expect: used outside a ghost macro without a ghost cfg
fn harness_reset() {
    crate::ghost::reset();
}
