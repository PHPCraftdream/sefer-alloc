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

/// Windows CI checks these files out with CRLF line endings; the needles use
/// `\n`, so compare against an LF-normalized copy.
fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn readme_states_the_fork_contract() {
    let readme = lf(README);
    assert!(readme.contains("## Fork safety"));
    assert!(readme.contains("no `pthread_atfork` handling anywhere in this crate"));
    assert!(readme.contains("`fork()` from a single-threaded process is fine"));
    assert!(
        readme.contains("followed immediately by `exec()` (e.g. `std::process::Command`, or any")
    );
    assert!(readme.contains(
        "allocating or freeing through `SeferAlloc` in the child of\na multi-threaded `fork()`, before `exec()`"
    ));
    assert!(readme.contains("Unbounded spin, including on free."));
    assert!(readme.contains("Permanently undrained memory."));
    assert!(readme.contains("Bounded but real stall."));
    assert!(readme.contains("`fork`+`exec` is safe"));
    assert!(readme.contains("tracked in `docs/CORRECTNESS_OPEN_ITEMS.md`"));
}

#[test]
fn readme_cites_the_confirmed_hazard_sites() {
    let readme = lf(README);
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
            readme.contains(needle),
            "README missing hazard site: {needle:?}"
        );
    }
}

#[test]
fn sefer_alloc_rustdoc_states_the_fork_contract() {
    let sefer_alloc_core = lf(SEFER_ALLOC_CORE);
    assert!(sefer_alloc_core.contains("# Fork safety"));
    assert!(sefer_alloc_core.contains("no `pthread_atfork` handling in this crate"));
    assert!(sefer_alloc_core.contains("Single-threaded `fork()` is fine"));
    assert!(sefer_alloc_core
        .contains("multi-threaded process followed immediately by `exec()` is also fine"));
    assert!(sefer_alloc_core.contains(
        "allocating or freeing through `SeferAlloc` in the child of a\n/// multi-threaded `fork()` before `exec()`"
    ));
    assert!(sefer_alloc_core.contains("README.md's \"Fork safety\" section"));
    assert!(sefer_alloc_core.contains("tracked in `docs/CORRECTNESS_OPEN_ITEMS.md`"));
    // No doctests allowed in src/**/*.rs (project convention) — the example
    // fence must stay non-executed.
    assert!(sefer_alloc_core.contains("```text\n/// // Safe: fork + exec"));
}
