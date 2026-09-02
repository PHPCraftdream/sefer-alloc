//! Regression pin for `#[track_caller]` on `push` and its cold panic
//! helper: the guard's panic `Location` must name THIS file, not lib.rs.
//!
//! This lives in its own one-`#[test]` binary because reading the panic
//! `Location` requires mutating the process-global panic hook; with a
//! one-test binary there are no concurrent sibling tests to race against,
//! so no serialization machinery is needed (same reasoning as
//! `tests/threaded_conservation.rs`'s one-#[test]-per-binary note).
//!
//! These do NOT run under `--cfg loom` (matching `tests/stack_unit.rs`,
//! whose ordinary conformance tests this complements).

#![cfg(not(loom))]

use tagged_index_stack::{ArrayIndexStack, TaggedIndex, TAIL};

/// push's `index < INDEX_MASK` guard at a NON-degenerate width, where the two
/// things the guard exists to reject are DIFFERENT values: at
/// `INDEX_BITS = 16`, `INDEX_MASK` is `0xFFFF` (the reserved empty sentinel)
/// while `TAIL` is `u32::MAX`. (At the old legal maximum `INDEX_BITS = 32`
/// the two coincided and the guard's purposes collapsed into one; the
/// `1..=16` cap has made that coincidence impossible — see
/// `max_legal_width_index_mask_never_equals_tail` in `tests/stack_unit.rs` —
/// so this pins the guard's ordinary, out-of-range purpose in its own right.)
#[test]
fn width_16_push_rejects_index_mask_itself() {
    type T = TaggedIndex<16>;
    assert_ne!(
        T::INDEX_MASK,
        TAIL as u64,
        "at width 16 INDEX_MASK (0xFFFF) and TAIL (u32::MAX) must differ — \
         this test covers the guard's ordinary out-of-range case (no legal \
         width has an INDEX_MASK/TAIL coincidence any more)"
    );

    let stack = ArrayIndexStack::<16, 4>::new();
    // 0xFFFF == INDEX_MASK at this width: an in-range-looking u32 that the
    // guard must reject because it is the reserved empty sentinel. The full
    // panic assertion (not a bare is_err()) means the message must name the
    // guard's own contract, so an unrelated out-of-bounds panic (e.g. from
    // `ArrayLinks`) cannot satisfy this test.
    //
    // Also pins #[track_caller]'s effect: without it on both `push` and its
    // `#[cold]` helper,
    // this panic's Location would name lib.rs instead of this call site, and
    // that regression would leave every OTHER assertion here green. The panic
    // Location is observable ONLY through a panic hook (the caught payload
    // carries the message, never the location), so the hook is swapped below.
    // This is the binary's only #[test], so no serialization, chaining, or
    // thread-id filtering is needed — a plain capturing hook suffices. The
    // original hook is restored BEFORE the post-assertions so a failing
    // assertion reports through the normal hook and the test can never leave
    // a swapped hook behind if anything is ever added after it.
    // The hook fires on THIS thread (during `catch_unwind` below), so the
    // capture site is a thread-local plain local — no `Arc`, no cross-thread
    // sharing, nothing else in this process can touch it.
    thread_local! {
        static CAPTURED_FILE: std::cell::RefCell<Option<String>> =
            const { std::cell::RefCell::new(None) };
    }

    let result = {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(|info| {
            if let Some(loc) = info.location() {
                CAPTURED_FILE.with(|f| *f.borrow_mut() = Some(loc.file().to_string()));
            }
        }));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: DELIBERATE contract violation under test — INDEX_MASK is the reserved empty
            // sentinel, never a legal index; the guard panic this triggers is the test's subject.
            // Result discarded: the index-range guard panics before
            // push_index_impl would ever return a value here.
            let _ = unsafe { stack.push(T::INDEX_MASK as u32) };
        }));
        std::panic::set_hook(original);
        result
    };

    let captured_file = CAPTURED_FILE.with(|f| f.borrow_mut().take());
    let err = result.expect_err("pushing index == INDEX_MASK must panic");
    let message = err
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| err.downcast_ref::<String>().cloned())
        .expect("panic payload should be a string message");
    assert!(
        message.contains("index must be < INDEX_MASK"),
        "panic message did not name the push guard's own contract (got: {message:?})"
    );
    assert_eq!(
        captured_file.as_deref(),
        Some(file!()),
        "push's #[track_caller] should report THIS file as the panic \
         location, not lib.rs -- #[track_caller] regressed"
    );
}
