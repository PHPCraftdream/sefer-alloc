//! Contract tripwire for the `fork()` safety documentation (R1-02).
//!
//! There is no `pthread_atfork` handling in this crate (unlike glibc /
//! jemalloc / mimalloc); a full handler is deliberately out of scope for
//! this minimum-fix task. This test only pins the documented contract
//! (README.md's "Fork safety" section and `SeferAlloc`'s own rustdoc): fail
//! if either loses its key sentences, so a future edit cannot silently drop
//! the warning.

const README: &str = include_str!("../README.md");
const SEFER_ALLOC_CORE: &str = include_str!("../src/global/sefer_alloc/core.rs");

#[test]
fn readme_states_the_fork_contract() {
    assert!(README.contains("## Fork safety"));
    assert!(README.contains("no `pthread_atfork` handling anywhere in this crate"));
    assert!(README.contains("`fork()` from a single-threaded process is fine"));
    assert!(
        README.contains("followed immediately by `exec()` (e.g. `std::process::Command`, or any")
    );
    assert!(README.contains(
        "allocating or freeing through `SeferAlloc` in the child of\na multi-threaded `fork()`, before `exec()`"
    ));
    assert!(README.contains("Unbounded spin, including on free."));
    assert!(README.contains("Permanently undrained memory."));
    assert!(README.contains("Bounded but real stall."));
    assert!(README.contains("`fork`+`exec` is safe"));
    assert!(README.contains("tracked in `docs/CORRECTNESS_OPEN_ITEMS.md`"));
}

#[test]
fn readme_cites_the_confirmed_hazard_sites() {
    for needle in [
        "src/global/fallback.rs`, `LockGuard::acquire`",
        "src/registry/bootstrap/overflow_sidecar.rs`",
        "`HeapOverflow::push_impl`",
        "src/registry/bootstrap/registry.rs`,\n  `ensure_chunk`/`try_ensure_chunk`",
        "src/registry/heap_overflow/spill.rs`",
        "src/alloc_core/large/deferred_large/drain.rs`",
        "src/registry/heap_core_xthread/ring.rs`,\n  `owner_slot_is_live`",
        "src/registry/heap_core_xthread/overflow.rs`",
    ] {
        assert!(
            README.contains(needle),
            "README missing hazard site: {needle:?}"
        );
    }
}

#[test]
fn sefer_alloc_rustdoc_states_the_fork_contract() {
    assert!(SEFER_ALLOC_CORE.contains("# Fork safety"));
    assert!(SEFER_ALLOC_CORE.contains("no `pthread_atfork` handling in this crate"));
    assert!(SEFER_ALLOC_CORE.contains("Single-threaded `fork()` is fine"));
    assert!(SEFER_ALLOC_CORE
        .contains("multi-threaded process followed immediately by `exec()` is also fine"));
    assert!(SEFER_ALLOC_CORE.contains(
        "allocating or freeing through `SeferAlloc` in the child of a\n/// multi-threaded `fork()` before `exec()`"
    ));
    assert!(SEFER_ALLOC_CORE.contains("README.md's \"Fork safety\" section"));
    assert!(SEFER_ALLOC_CORE.contains("tracked in `docs/CORRECTNESS_OPEN_ITEMS.md`"));
    // No doctests allowed in src/**/*.rs (project convention) — the example
    // fence must stay non-executed.
    assert!(SEFER_ALLOC_CORE.contains("```text\n/// // Safe: fork + exec"));
}
