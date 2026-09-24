//! Contract tripwire for legal remote frees and unsafe-caller misuse.

const README: &str = include_str!("../README.md");
const OVERFLOW: &str = include_str!("../src/registry/heap_overflow.rs");
const NODE: &str = include_str!("../src/alloc_core/platform/node.rs");
const POOL: &str = include_str!("../src/alloc_core/small/alloc_core_small_pool/mod.rs");
const CORE: &str = include_str!("../src/registry/heap_core/core.rs");
const HOT: &str = include_str!("../src/registry/heap_core/alloc/hot.rs");

#[test]
fn unsafe_caller_double_free_is_not_promised_safe_when_mapped() {
    assert!(README.contains("double-free is outside its unsafe-caller contract"));
    assert!(README.contains("a concurrent duplicate can race"));
    assert!(README.contains("a sequential duplicate can self-link"));
    assert!(README.contains("generation check does not make such duplicate spill publication safe"));
    assert!(OVERFLOW.contains("A duplicate free is outside the unsafe caller's"));
    assert!(OVERFLOW.contains("there is no atomic per-block"));
}

#[test]
fn remote_body_write_requires_exclusive_transfer_and_spill_saturation() {
    assert!(README.contains("neither ring reads nor writes the block body"));
    assert!(README.contains("the intrusive spill writes a 16-byte note"));
    assert!(README.contains("transferred exclusive use of that block"));
    assert!(NODE.contains("this freshly allocated block has not yet"));
    assert!(NODE.contains("After handoff a remote spill may write"));
    assert!(POOL.contains("an intrusive"));
    assert!(POOL.contains("pending ring, sidecar, or spill note still counts as live"));
    assert!(CORE.contains("R2-09 subsequently added the lossless intrusive spill"));
    assert!(HOT.contains("the intrusive spill; saturation no longer discards"));
}

#[test]
fn unconditional_or_terminal_leak_descriptions_do_not_return() {
    for stale in [
        "non-intrusive cross-thread free through a",
        "Cross-thread free (opt-in `alloc-xthread`) does **not** dereference the",
        "the free path never touches the block body",
    ] {
        assert!(!README.contains(stale), "stale README claim: {stale}");
    }
    for stale in [
        "remote free NEVER writes the body",
        "a remote free never writes a block's body",
    ] {
        assert!(!NODE.contains(stale), "stale node safety proof: {stale}");
    }
    assert!(!HOT.contains("overflow → bounded leak"));
    assert!(!CORE.contains("sound bounded leak —"));
    assert!(!POOL.contains("cross-thread freer NEVER dereferences the block"));
}
