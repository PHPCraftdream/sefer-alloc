//! R16 item83 per-process RSS probe: ONE process = one arm, one scenario, one
//! sample. Linux-only (`/proc/self/status` VmRSS/VmHWM); it is driven by
//! `scripts/r16_perf83_iai.mjs --rss-run`, never by a global allocator.
//!
//! Allocator layer: `HeapRegistry` lease -> `(*heap).alloc/realloc/dealloc`
//! (`HeapCore`, the user-selected decision layer). `sefer_alloc` is NOT
//! registered as `#[global_allocator]`: the process allocator stays `System`,
//! so the observer's own bookkeeping never goes through the allocator under
//! test. The `/proc` reads use a fixed stack buffer (no heap growth); the final
//! output lines are formatted after the last RSS reading.
//!
//! Environment (all mandatory, strictly validated): `ARM` (`A`|`B`),
//! `SCENARIO` (the five Ir work fixtures or `cycles8to6`), `SAMPLE` (1..=9),
//! `SOURCE_INPUT_SHA256` and `BINARY_SHA256` (64 lowercase hex, supplied by
//! the driver). Work fixtures use the bench helper's touch schedule: alloc old
//! (align 16), full touch of old payload, one `realloc`, full touch of the new
//! payload; RSS is read with that single allocation still live. `cycles8to6`:
//! alloc+touch 8 MiB, then 20 x (shrink to 6 MiB + full touch, grow to 8 MiB +
//! full touch); `peak_bytes` is the Linux `VmHWM` LIFETIME peak (it includes
//! startup/setup; no resettable window exists) read after cycle 20 and before
//! the final free.
//!
//! Output: exactly one `config:{...}` line (resolved allocator configuration
//! for the driver's receipt, NOT part of the judge schema) and exactly one
//! NDJSON line with the judge schema keys. Absolute per-process bytes only.
//!
//! No Cargo entry is needed: without Linux and the features
//! `alloc-global alloc-xthread alloc-decommit fastbin bench-internals
//! internals` (i.e. `production bench-internals internals`) the fallback
//! `main` panics loudly instead of silently doing nothing.

#[cfg(not(all(
    target_os = "linux",
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "fastbin",
    feature = "bench-internals",
    feature = "internals"
)))]
fn main() {
    panic!(
        "r16_perf83_large_shrink_rss requires Linux and features \
         `production bench-internals internals`; refusing to emit data"
    );
}

#[cfg(all(
    target_os = "linux",
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "fastbin",
    feature = "bench-internals",
    feature = "internals"
))]
fn main() {
    probe::run();
}

#[cfg(all(
    target_os = "linux",
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "fastbin",
    feature = "bench-internals",
    feature = "internals"
))]
mod probe {
    use sefer_alloc::registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry};
    use std::alloc::Layout;
    use std::hint::black_box;
    use std::io::Read;

    const ALIGN: usize = 16;
    const CYCLES: usize = 20;
    const BIG: usize = 8_388_608;
    const SMALL: usize = 6_291_456;
    const SCENARIOS: [&str; 6] = [
        "realloc_large_shrink_8_to_6mib",
        "realloc_large_shrink_8_to_4p5mib",
        "realloc_large_shrink_8_to_3mib",
        "realloc_large_grow_6_to_8mib",
        "realloc_large_equal_8mib",
        "cycles8to6",
    ];

    fn env(name: &str) -> String {
        match std::env::var(name) {
            Ok(value) => value,
            Err(error) => panic!("{name} must be set to a valid value: {error}"),
        }
    }

    fn sha256_hex(name: &str) -> String {
        let value = env(name);
        assert!(
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{name} must be 64 lowercase hex characters"
        );
        value
    }

    /// Read one `/proc/self/status` field (`VmRSS:`/`VmHWM:`) in bytes using a
    /// fixed stack buffer; the process-wide value (single-threaded probe).
    fn status_bytes(field: &str) -> u64 {
        let mut buf = [0u8; 8192];
        let mut file = std::fs::File::open("/proc/self/status").expect("open /proc/self/status");
        let mut len = 0;
        loop {
            assert!(
                len < buf.len(),
                "/proc/self/status larger than the stack buffer"
            );
            let n = file.read(&mut buf[len..]).expect("read /proc/self/status");
            if n == 0 {
                break;
            }
            len += n;
        }
        let text = core::str::from_utf8(&buf[..len]).expect("status is UTF-8");
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix(field) {
                let mut parts = rest.split_whitespace();
                let kib: u64 = parts
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| panic!("unparsable {field}"));
                assert_eq!(parts.next(), Some("kB"), "{field} unit");
                assert!(kib > 0, "{field} must be positive");
                return kib * 1024;
            }
        }
        panic!("{field} missing from /proc/self/status");
    }

    fn rss() -> u64 {
        status_bytes("VmRSS:")
    }

    fn peak() -> u64 {
        status_bytes("VmHWM:")
    }

    fn touch(ptr: *mut u8, len: usize) {
        // SAFETY: ptr is a live allocation of this heap with at least `len`
        // writable bytes; no other reference to it exists.
        unsafe { core::ptr::write_bytes(black_box(ptr), 0x5A, len) };
    }

    fn layout(size: usize) -> Layout {
        Layout::from_size_align(size, ALIGN).expect("valid layout")
    }

    fn resize(heap: &mut HeapCore, ptr: *mut u8, old: usize, new: usize) -> *mut u8 {
        // SAFETY: ptr is the single live allocation of this heap and `old`
        // is exactly the size/align it was allocated or resized with.
        let resized = unsafe { (*heap).realloc(ptr, layout(old), new) };
        if resized.is_null() {
            // SAFETY: a failed realloc leaves the original allocation live.
            unsafe { (*heap).dealloc(ptr, layout(old)) };
            panic!("realloc {old} -> {new} failed");
        }
        resized
    }

    pub(super) fn run() {
        let arm = env("ARM");
        assert!(arm == "A" || arm == "B", "ARM must be A or B");
        let scenario = env("SCENARIO");
        assert!(SCENARIOS.contains(&scenario.as_str()), "invalid SCENARIO");
        let sample: u32 = env("SAMPLE").parse().expect("SAMPLE integer");
        assert!((1..=9).contains(&sample), "SAMPLE must be 1..=9");
        let source = sha256_hex("SOURCE_INPUT_SHA256");
        let binary = sha256_hex("BINARY_SHA256");

        // Baseline at measurement start, before bootstrap/lease/workload.
        let baseline = rss();
        let _ = bootstrap::ensure();
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim lease");
        let conflicts_before = config_conflicts_total();
        let heap = lease.core();
        for size in [BIG, SMALL, 4_718_592, 3_145_728] {
            assert!(
                (*heap).dbg_class_for(layout(size)).is_none(),
                "{size} must classify Large"
            );
        }
        let (decay_rate_bp, decay_interval_ms, headroom) = (*heap).dbg_decay_config();
        let budget = (*heap).dbg_large_cache_budget();
        let slots = (*heap).dbg_large_cache_total_slots();
        let pool_cap = (*heap).dbg_pool_cap();

        let (old, new) = match scenario.as_str() {
            "realloc_large_shrink_8_to_6mib" => (BIG, SMALL),
            "realloc_large_shrink_8_to_4p5mib" => (BIG, 4_718_592),
            "realloc_large_shrink_8_to_3mib" => (BIG, 3_145_728),
            "realloc_large_grow_6_to_8mib" => (SMALL, BIG),
            "realloc_large_equal_8mib" => (BIG, BIG),
            _ => (BIG, BIG),
        };
        let mut ptr = (*heap).alloc(layout(old));
        assert!(!ptr.is_null(), "initial allocation");
        touch(ptr, old);
        let final_size = if scenario == "cycles8to6" {
            for _ in 0..CYCLES {
                ptr = resize(heap, ptr, BIG, SMALL);
                touch(ptr, SMALL);
                ptr = resize(heap, ptr, SMALL, BIG);
                touch(ptr, BIG);
            }
            BIG
        } else {
            ptr = resize(heap, ptr, old, new);
            touch(ptr, new);
            new
        };
        // One live allocation: primary RSS and lifetime peak (VmHWM).
        let rss_bytes = rss();
        let peak_bytes = peak();
        black_box(ptr);
        // SAFETY: ptr is the single live allocation, freed once with its
        // current layout.
        unsafe { (*heap).dealloc(ptr, layout(final_size)) };
        let final_free_rss_bytes = rss();
        let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);
        drop(lease);
        let teardown_rss_bytes = rss();

        assert_eq!(conflicts_delta, 0, "config conflict in a fresh process");
        assert!(
            peak_bytes >= rss_bytes && peak_bytes >= baseline,
            "VmHWM below resident bytes"
        );
        let budget_json = match budget {
            Some(bytes) => bytes.to_string(),
            None => "null".to_owned(),
        };
        println!(
            "config:{{\"requested\":\"default HeapRegistry::dbg_claim_lease config\",\
\"decay_rate_bp\":{decay_rate_bp},\"decay_interval_ms\":{decay_interval_ms},\
\"headroom_bytes\":{headroom},\"large_cache_budget_bytes\":{budget_json},\
\"large_cache_total_slots\":{slots},\"pool_cap\":{pool_cap},\
\"config_conflicts_delta\":{conflicts_delta}}}"
        );
        println!(
            "{{\"arm\":\"{arm}\",\"scenario\":\"{scenario}\",\"sample\":{sample},\
\"rss_bytes\":{rss_bytes},\"baseline_rss_bytes\":{baseline},\"peak_bytes\":{peak_bytes},\
\"final_free_rss_bytes\":{final_free_rss_bytes},\"teardown_rss_bytes\":{teardown_rss_bytes},\
\"pid\":{},\"statistic\":\"per-process absolute RSS\",\
\"source_input_sha256\":\"{source}\",\"binary_sha256\":\"{binary}\"}}",
            std::process::id()
        );
    }
}
