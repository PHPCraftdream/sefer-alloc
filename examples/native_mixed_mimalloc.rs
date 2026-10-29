//! Контрольный installed mimalloc для того же workload.
const ARM: &str = "mimalloc";

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn installed_segments() -> u64 {
    sefer_alloc::SeferAlloc::new()
        .stats()
        .segments_reserved_total
}

include!("_shared/paired_ab_workload.rs");
include!("_shared/native_mixed_driver.rs");
