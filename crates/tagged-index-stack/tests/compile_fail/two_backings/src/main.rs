//! Sol-codex run-3 P1-1 minimal safe-Rust double-issue repro, adapted to the
//! post-redesign type names. This file MUST FAIL TO COMPILE: `StackHead` is the
//! head word ONLY — it has no `push`/`pop` — and `ArrayLinks` is a bare links
//! building block that no longer pairs with any head. The operations live on
//! `StackOps` (crate-owned blanket impl over `StackStorage`), whose implementor
//! owns head AND links in one place, so "two backings against one head" cannot
//! be expressed. Pinned failing by `tests/compile_fail_two_backings.rs`.
use tagged_index_stack::{ArrayLinks, StackHead};

fn main() {
    let a = ArrayLinks::<2>::new();
    let b = ArrayLinks::<2>::new();
    let stack = StackHead::<16>::new();
    stack.push(&a, 1); // ERROR: no method named `push` on StackHead<16>
    assert_eq!(stack.pop(&b), Some(0)); // ERROR: no method named `pop` on StackHead<16>
    assert_eq!(stack.pop(&b), Some(0)); // ERROR: no method named `pop` on StackHead<16>
}
