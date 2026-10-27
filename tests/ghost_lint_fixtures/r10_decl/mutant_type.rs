// expect: does not match
thread_local! {
    static SLOT: Cell<u8> = const { Cell::new(0) };
}

struct Holder {
    value: Cell<u32>,
}
