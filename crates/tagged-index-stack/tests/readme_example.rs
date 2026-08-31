//! Mirrors README.md's `## Example` section verbatim so a future API change
//! that breaks it fails CI instead of silently rotting the published docs.
//!
//! This repo bans doctests (`#[doc = include_str!(...)]` is not used to pull
//! README.md into rustdoc either, so `cargo test --doc` never compiles the
//! README's fenced ```rust``` block) -- see CLAUDE.md's "No doctests"
//! section. A separate `tests/` file mirroring the example is the
//! established alternative: `crates/size-classes/tests/builder.rs`'s
//! `readme_example_compiles_and_derives_its_generics` does the identical
//! thing for a sibling crate. This crate's own test suite is organized
//! one-file-per-concern (`stack_unit.rs`, `proptest_pack_unpack.rs`,
//! `regression_counter_wrap.rs`, `loom_aba.rs`), so the README mirror gets
//! its own dedicated file rather than folding into an existing one.

#![cfg(not(loom))]

use tagged_index_stack::{ArrayLinks, TaggedIndexStack};

#[test]
fn readme_example_compiles_and_runs() {
    let links = ArrayLinks::<1024>::new();
    let stack = TaggedIndexStack::<16>::new(); // 16-bit index, 48-bit ABA tag

    stack.push(&links, 7); // recycle index 7
    assert_eq!(stack.pop(&links), Some(7)); // recycled index comes back out

    assert_eq!(
        stack.pop(&links),
        None,
        "the stack held only the one pushed index -- draining it leaves the stack empty"
    );
}
