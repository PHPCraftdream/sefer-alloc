use std::fs;
use std::path::Path;

const IN_SCOPE: &[&str] = &[
    "src/registry/heap_core/free/dealloc.rs",
    "src/registry/heap_core/free/dealloc_batch.rs",
    "src/registry/heap_core/alloc/batch.rs",
    "src/registry/heap_core/alloc/hot.rs",
    "src/registry/heap_core/state/tcache.rs",
    "src/alloc_core/small/alloc_core_small/directory.rs",
    "src/registry/heap_core/diag/mod.rs",
    "src/registry/heap_core/diag/diag_probes.rs",
    "src/alloc_core/segment/segment_header/segment_header_impl.rs",
    "src/registry/heap_core/core.rs",
    "src/alloc_core/config/large_cache_config.rs",
    "src/alloc_core/alloc_core/mod.rs",
    "src/alloc_core/alloc_core/alloc_core_impl.rs",
    "src/alloc_core/alloc_core/lifecycle.rs",
    "src/alloc_core/alloc_core/alloc_core_core_diag/mod.rs",
    "src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs",
    "src/registry/bootstrap/chunk.rs",
    "src/global/tls_heap.rs",
    "src/alloc_core/segment/segment_header/layout_asserts.rs",
    "src/registry/bootstrap/ensure.rs",
    "src/alloc_core/segment/segment_header/segment_header_layout.rs",
    "src/alloc_core/small/reserved_small_segment.rs",
    "src/registry/heap_core/free/realloc.rs",
    "src/alloc_core/large/alloc_core_large.rs",
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
    let bootstrap = comments(&read(root, "src/registry/bootstrap/mod.rs"));
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
        comments(&read(root, "src/registry/heap_registry/claim.rs")).contains("get_unchecked"),
        "claim safety docs must explain get_unchecked"
    );
}
