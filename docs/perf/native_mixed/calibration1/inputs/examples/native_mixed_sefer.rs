//! Калибровка полного installed workload на принятой базе.
const ARM: &str = "sefer";

#[global_allocator]
static GLOBAL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();

fn installed_segments() -> u64 {
    GLOBAL.stats().segments_reserved_total
}

include!("_shared/paired_ab_workload.rs");
include!("_shared/native_mixed_driver.rs");
