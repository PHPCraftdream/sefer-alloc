// expect: ghost field Entry.ghost_pins
struct Entry {
    refs: AtomicUsize,
    ghost_pins: usize,
}
