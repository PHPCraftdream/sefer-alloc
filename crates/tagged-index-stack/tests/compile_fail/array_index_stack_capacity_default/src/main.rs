//! `Default::default()` must reject a capacity larger than the index mask.
//! The fixture deliberately contains no constructor call.
use tagged_index_stack::ArrayIndexStack;

fn main() {
    let _: ArrayIndexStack<4, 16> = Default::default();
}
