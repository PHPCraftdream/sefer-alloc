//! Mirrors README.md's `## Example` section, and additionally asserts the
//! drained-empty case the README's example does not show, so a future API
//! change that breaks either fails CI instead of silently rotting the
//! published docs.
//!
//! This repo bans doctests (`#[doc = include_str!(...)]` is not used to pull
//! README.md into rustdoc either, so `cargo test --doc` never compiles the
//! README's fenced ```rust``` block) -- see CLAUDE.md's "No doctests"
//! section. A separate `tests/` file mirroring the example is the
//! established alternative: `crates/size-classes/tests/builder.rs`'s
//! `readme_example_compiles_and_derives_its_generics` does the identical
//! thing for a sibling crate. This crate's own test suite is organized
//! one-file-per-concern (`stack_unit.rs`, `proptest_pack_unpack.rs`,
//! `custom_storage_impl.rs`, `loom_aba.rs`,
//! `threaded_conservation.rs`), so the README mirror gets its own dedicated
//! file rather than folding into an existing one.

#![cfg(not(loom))]

use tagged_index_stack::ArrayIndexStack;

#[test]
fn readme_example_compiles_and_runs() {
    let stack = ArrayIndexStack::<16, 1024>::new(); // 16-bit index, 48-bit ABA tag

    stack.push(7); // recycle index 7
    assert_eq!(stack.pop(), Some(7)); // recycled index comes back out

    assert_eq!(
        stack.pop(),
        None,
        "the stack held only the one pushed index -- draining it leaves the stack empty"
    );
}
