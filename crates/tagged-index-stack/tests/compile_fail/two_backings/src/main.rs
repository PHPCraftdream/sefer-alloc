//! API-removal regression fixture, adapted to the
//! post-redesign type names. This file MUST FAIL TO COMPILE: `StackHead` is
//! the head word ONLY — it has no `push`/`pop`, and no public API accepts a
//! caller-supplied links backing per call. The operations live on
//! `StackOps` (crate-owned blanket impl over `StackStorage`), whose
//! implementor owns head AND links in ONE place — so the OLD per-call
//! spelling of "two backings against one head" no longer EXISTS. This pins
//! the API removal ONLY, not a safety invariant: the hazard class itself is
//! still expressible through a custom `unsafe impl StackStorage` that
//! asserts and then violates its `# Safety` contract (shape 2 of the trait
//! doc's hazard inventory; pinned by
//! `two_implementor_values_sharing_one_head_still_double_issue` in
//! `tests/custom_storage_impl.rs`). Pinned failing by
//! `tests/compile_fail_two_backings.rs`.
use tagged_index_stack::{ArrayLinks, StackHead};

fn main() {
    let a = ArrayLinks::<2>::new();
    let b = ArrayLinks::<2>::new();
    let stack = StackHead::<16>::new();
    stack.push(&a, 1); // ERROR: no method named `push` on StackHead<16>
    assert_eq!(stack.pop(&b), Some(0)); // ERROR: no method named `pop` on StackHead<16>
    assert_eq!(stack.pop(&b), Some(0)); // ERROR: no method named `pop` on StackHead<16>
}
