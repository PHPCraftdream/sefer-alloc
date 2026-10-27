// expect: 0 statics named SLOT
thread_local! {
    static OTHER: Cell<usize> = const { Cell::new(0) };
}

struct Holder {
    value: Cell<u32>,
}
