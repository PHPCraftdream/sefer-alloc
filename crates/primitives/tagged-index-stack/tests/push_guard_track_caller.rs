//! Regression coverage for `#[track_caller]` on `push`'s guard and the
//! `ArrayLinks` load/store bounds panic helper: all three panic locations must
//! name THIS file, not lib.rs, and all three messages must stay exact.
//!
//! This lives in its own one-`#[test]` binary because reading panic locations
//! requires mutating the process-global panic hook. With one test in the
//! binary there are no concurrent sibling tests to race against, so no
//! serialization machinery is needed (same reasoning as
//! `tests/threaded_conservation.rs`'s one-#[test]-per-binary note).
//!
//! These do NOT run under `--cfg loom` (matching `tests/stack_unit.rs`,
//! whose ordinary conformance tests this complements).

#![cfg(not(loom))]

use tagged_index_stack::{ArrayIndexStack, ArrayLinks, TaggedIndex};

/// `push`'s `index < INDEX_MASK` guard and `ArrayLinks`' load/store bounds
/// checks, including each exact panic message and caller location.
#[test]
fn push_guard_and_array_links_panics_report_exact_messages_and_callers() {
    type T = TaggedIndex<16>;

    let stack = ArrayIndexStack::<16, 4>::new();
    // 0xFFFF == INDEX_MASK at this width: an in-range-looking u32 that the
    // guard must reject because it is the reserved empty sentinel. The full
    // panic assertion means the message must name the guard's own contract,
    // so an unrelated out-of-bounds panic cannot satisfy this test.
    //
    // Also pins #[track_caller]'s effect: without it on both `push` and its
    // #[cold] helper, this panic's Location would name lib.rs instead of this
    // call site, and that regression would leave the message assertions
    // green. The panic Location is observable only through a panic hook; the
    // caught payload carries the message, never the location.
    //
    // This is the binary's only #[test], so no serialization, chaining, or
    // thread-id filtering is needed. The original hook is restored BEFORE
    // the post-assertions so a failing assertion reports through the normal
    // hook and the test can never leave a swapped hook behind.
    // The hook fires on THIS thread, so one thread-local Vec captures all
    // three locations without cross-thread sharing.
    thread_local! {
        static CAPTURED_FILES: std::cell::RefCell<Vec<String>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
        payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .expect("panic payload should be a string message")
    }

    let links = ArrayLinks::<4>::new();
    let (push_result, load_result, store_result) = {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(|info| {
            if let Some(location) = info.location() {
                CAPTURED_FILES.with(|files| files.borrow_mut().push(location.file().to_owned()));
            }
        }));

        let push_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: DELIBERATE contract violation under test: INDEX_MASK is
            // the reserved empty sentinel, never a legal index; the guard
            // panic this triggers is the test's subject. Result discarded:
            // the index-range guard panics before push_index_impl returns.
            let _ = unsafe { stack.push(T::INDEX_MASK as u32) };
        }));
        let load_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = links.load_next(4);
        }));
        let store_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            links.store_next(4, 0);
        }));

        std::panic::set_hook(original);
        (push_result, load_result, store_result)
    };

    let messages = vec![
        panic_message(push_result.expect_err("pushing index == INDEX_MASK must panic")),
        panic_message(load_result.expect_err("out-of-range load must panic")),
        panic_message(store_result.expect_err("out-of-range store must panic")),
    ];
    assert_eq!(
        messages,
        vec![
            "index must be < INDEX_MASK (the empty sentinel is reserved), got 65535 (INDEX_MASK = 0xffff)",
            "ArrayLinks index out of bounds: index 4 >= capacity 4",
            "ArrayLinks index out of bounds: index 4 >= capacity 4",
        ]
    );

    let captured_files =
        CAPTURED_FILES.with(|files| files.borrow_mut().drain(..).collect::<Vec<_>>());
    assert_eq!(captured_files, vec![file!().to_owned(); 3]);
}
