use std::fs;
use std::path::Path;

const IN_SCOPE: &[&str] = &[
    "crates/core/sefer-registry/src/registry/heap_core/free/dealloc.rs",
    "crates/core/sefer-registry/src/registry/heap_core/free/dealloc_batch.rs",
    "crates/core/sefer-registry/src/registry/heap_core/alloc/batch.rs",
    "crates/core/sefer-registry/src/registry/heap_core/alloc/hot.rs",
    "crates/core/sefer-registry/src/registry/heap_core/state/tcache.rs",
    "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small/directory.rs",
    "crates/core/sefer-registry/src/registry/heap_core/diag/mod.rs",
    "crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs",
    "crates/core/sefer-segment/src/segment/segment_header/segment_header_impl/segment_header_impl.rs",
    "crates/core/sefer-registry/src/registry/heap_core/core.rs",
    "crates/core/sefer-policy/src/large_cache_config.rs",
    "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/mod.rs",
    "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/alloc_core_impl.rs",
    "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/lifecycle_parts/lifecycle.rs",
    "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/alloc_core_core_diag/mod.rs",
    "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs",
    "crates/core/sefer-registry/src/registry/bootstrap/chunk.rs",
    "src/global/tls_heap.rs",
    "crates/core/sefer-segment/src/segment/segment_header/segment_header_impl/layout_asserts.rs",
    "crates/core/sefer-registry/src/registry/bootstrap/ensure.rs",
    "crates/core/sefer-segment/src/segment/segment_header/segment_header_layout.rs",
    "crates/core/sefer-alloc-core/src/alloc_core/small/reserved_small_segment.rs",
    "crates/core/sefer-registry/src/registry/heap_core/free/realloc.rs",
    "crates/core/sefer-alloc-core/src/alloc_core/large/alloc_core_large.rs",
];

const STALE: &[&str] = &[
    "HeapOverflow",
    "ring push",
    "ring entry",
    "drain_heap_overflow",
    "owner_slot_is_live",
    "entry_class_idx",
    "dealloc_foreign_routing",
    "large_layout_consistent",
    "AtomicPtr<RegistryChunk>",
    "reserve_aligned(CHUNK_SIZE",
    "mem::forget(reservation)",
    "Phase 8 is single-threaded",
    "three resolvers",
    "u32::MAX` is reserved",
    "#138 cross-thread consistency",
    "does not release bytes below",
    "ring-simulation",
    "alloc_core_small.rs:",
    "heap_core_free.rs:",
    "via the TFS",
    "drain the TFS",
];

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn comments(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let line = line.trim_start();
            line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn r6_t17_scoped_doc_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for &file in IN_SCOPE {
        let prose = comments(&read(root, file));
        for phrase in STALE {
            assert!(!prose.contains(phrase), "stale prose in {file}: {phrase}");
        }
    }

    let global = comments(&read(root, "src/global/sefer_alloc/global_alloc.rs"));
    for phrase in [
        "CurrentHeap::Own",
        "CurrentHeap::Fallback",
        "current_for_dealloc",
        "publish_foreign",
        "exact-object",
        "exit_escape_alloc",
        "M4",
        "docs/INVARIANTS.md",
        "abort",
        "HeapCore",
        "raw pointer",
        "checked geometry",
        "MAX_SEGMENTS",
        "route registration",
        "reservation",
        "commit",
        "System",
        "class-leaf/spill metadata",
        "exit_escape_realloc",
        "min(old, new)",
        "fatal abort policy",
        "sefer_alloc/mod.rs",
    ] {
        assert!(
            global.contains(phrase),
            "global allocator contract lacks {phrase}"
        );
    }
    for phrase in ["never panics", "only null return is true OOM"] {
        assert!(
            !global.contains(phrase),
            "global allocator docs must not claim {phrase}"
        );
    }
    let bootstrap = comments(&read(
        root,
        "crates/core/sefer-registry/src/registry/bootstrap/mod.rs",
    ));
    for phrase in [
        "OncePtrCell",
        "leak_zeroed_pages",
        "FREE_SLOTS",
        "slot-state CAS",
    ] {
        assert!(
            bootstrap.contains(phrase),
            "bootstrap contract lacks {phrase}"
        );
    }
    assert!(
        comments(&read(
            root,
            "crates/core/sefer-registry/src/registry/heap_registry/claim.rs"
        ))
        .contains("get_unchecked"),
        "claim safety docs must explain get_unchecked"
    );
}
