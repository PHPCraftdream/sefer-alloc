//! D1′ route registration/chunk-activation probe, measurement-only.
//!
//! Timing uses `SeferAlloc::alloc`/`dealloc` and thread teardown with
//! `production` features only. The separate oracle mode enables counters and
//! chunk observers; its timings are never used. This probe measures the direct
//! `GlobalAlloc` entrypoint, not an installed `#[global_allocator]` workload.

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::time::Instant;

use sefer_alloc::{SeferAlloc, SegmentLayout};

#[cfg(all(feature = "internals", feature = "bench-internals"))]
use sefer_alloc::registry::segment_route::RouteKind;
#[cfg(feature = "internals")]
use sefer_alloc::registry::segment_route::{RouteDirectory, SmallSidecar};

static GLOBAL: SeferAlloc = SeferAlloc::new();

const DENSE_16B_BLOCKS: usize = SegmentLayout::SEGMENT / SegmentLayout::MIN_BLOCK;
const REGISTRATION_BLOCKS: usize = SegmentLayout::SEGMENT / SegmentLayout::SMALL_MAX + 8;
const SPARSE_MAX_SMALL_BLOCKS: usize = 8;
#[cfg(all(feature = "internals", feature = "bench-internals"))]
const CHUNK_SPAN: usize = SegmentLayout::MIN_BLOCK * 64 * 8;
#[cfg(all(feature = "internals", feature = "bench-internals"))]
const CHUNKS_PER_SEGMENT: usize = SegmentLayout::SEGMENT / CHUNK_SPAN;

#[derive(Clone, Copy)]
struct Shape {
    name: &'static str,
    block_size: usize,
    blocks_per_cycle: usize,
}

impl Shape {
    fn parse(value: &str) -> Self {
        match value {
            "registration" => Self {
                name: "registration",
                block_size: SegmentLayout::SMALL_MAX,
                blocks_per_cycle: REGISTRATION_BLOCKS,
            },
            "dense16" => Self {
                name: "dense16",
                block_size: SegmentLayout::MIN_BLOCK,
                blocks_per_cycle: DENSE_16B_BLOCKS,
            },
            "sparse_max_small" => Self {
                name: "sparse_max_small",
                block_size: SegmentLayout::SMALL_MAX,
                blocks_per_cycle: SPARSE_MAX_SMALL_BLOCKS,
            },
            _ => panic!("unknown scenario {value}"),
        }
    }
}

struct WorkerMetrics {
    alloc_ns: u128,
    cleanup_ns: u128,
    work_ns: u128,
    blocks: usize,
    #[cfg(feature = "internals")]
    live_routes_after_alloc: Option<(usize, usize, usize)>,
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    chunk_oracle: Option<ChunkOracle>,
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
struct ChunkOracle {
    active_routes: usize,
    observed_chunks: usize,
    logical_payload_bytes: usize,
    sidecar_backing_bytes: usize,
    expected_chunks_by_route: Vec<usize>,
    observed_chunks_by_route: Vec<usize>,
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn observe_chunks(pointers: &[*mut u8]) -> ChunkOracle {
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::AtomicU64;

    let mut expected = HashMap::<usize, (*mut u8, HashSet<usize>)>::new();
    for &pointer in pointers {
        let address = pointer.addr();
        let root = address & !(SegmentLayout::SEGMENT - 1);
        let chunk = (address - root) / CHUNK_SPAN;
        expected
            .entry(root)
            .or_insert_with(|| (pointer, HashSet::new()))
            .1
            .insert(chunk);
    }

    let directory = RouteDirectory::global();
    let route_count = expected.len();
    let mut expected_counts = Vec::with_capacity(route_count);
    let mut observed_counts = Vec::with_capacity(route_count);
    let mut observed_chunks = 0;
    let mut logical_payload_bytes = 0;
    let mut sidecar_backing_bytes = 0;
    for (_, (pointer, chunks)) in expected {
        let pin = directory
            .lookup(pointer)
            .expect("live allocation must have a published route");
        assert!(matches!(
            pin.kind(),
            RouteKind::Small | RouteKind::Primordial
        ));
        let sidecar = pin
            .dbg_small_sidecar_for_test()
            .expect("small allocation route must own a sidecar");
        let actual = sidecar.initialized_chunks_for_test();
        assert!(
            actual >= chunks.len(),
            "every touched chunk must be constructed"
        );
        observed_chunks += actual;
        logical_payload_bytes += actual * 16 * core::mem::size_of::<AtomicU64>();
        sidecar_backing_bytes += core::mem::size_of_val(sidecar);
        expected_counts.push(chunks.len());
        observed_counts.push(actual);
    }
    expected_counts.sort_unstable();
    observed_counts.sort_unstable();
    assert!(
        observed_chunks > 0,
        "scenario did not materialize any chunk"
    );
    assert!(observed_chunks <= route_count.max(1) * CHUNKS_PER_SEGMENT);
    ChunkOracle {
        active_routes: expected_counts.len(),
        observed_chunks,
        logical_payload_bytes,
        sidecar_backing_bytes,
        expected_chunks_by_route: expected_counts,
        observed_chunks_by_route: observed_counts,
    }
}

fn run_worker(shape: Shape, oracle: bool) -> WorkerMetrics {
    let layout = Layout::from_size_align(shape.block_size, 8).expect("valid small layout");
    let mut pointers = Vec::with_capacity(shape.blocks_per_cycle);
    let work_start = Instant::now();
    let alloc_start = Instant::now();
    for _ in 0..shape.blocks_per_cycle {
        // SAFETY: layout is non-zero and valid; every returned pointer is kept
        // in `pointers` and deallocated once with the identical layout below.
        let pointer = unsafe { GLOBAL.alloc(layout) };
        assert!(!pointer.is_null(), "SeferAlloc allocation failed");
        pointers.push(pointer);
        black_box(pointer);
    }
    let alloc_ns = alloc_start.elapsed().as_nanos();

    #[cfg(feature = "internals")]
    let live_routes_after_alloc =
        oracle.then(|| RouteDirectory::global().live_route_census_for_test());
    #[cfg(not(feature = "internals"))]
    let _ = oracle;

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    let chunk_oracle = oracle.then(|| observe_chunks(&pointers));
    #[cfg(not(all(feature = "internals", feature = "bench-internals")))]
    let _ = oracle;

    let cleanup_start = Instant::now();
    for pointer in pointers.drain(..) {
        // SAFETY: pointer is live from the allocation loop and is freed once
        // with the original layout on the same owner thread.
        unsafe { GLOBAL.dealloc(pointer, layout) };
    }
    GLOBAL.trim_current_thread();
    let cleanup_ns = cleanup_start.elapsed().as_nanos();
    let work_ns = work_start.elapsed().as_nanos();
    WorkerMetrics {
        alloc_ns,
        cleanup_ns,
        work_ns,
        blocks: shape.blocks_per_cycle,
        #[cfg(feature = "internals")]
        live_routes_after_alloc,
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        chunk_oracle,
    }
}

#[cfg(feature = "internals")]
fn route_count(routes: (usize, usize, usize)) -> usize {
    routes.0 + routes.1 + routes.2
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 2,
        "usage: d1_sidecar_registration_gate <registration|dense16|sparse_max_small> <cycles> [oracle]"
    );
    let shape = Shape::parse(&args[0]);
    let cycles: usize = args[1].parse().expect("cycles must be an integer");
    let oracle = args.get(2).is_some_and(|value| value == "oracle");
    assert!(cycles > 0);
    #[cfg(not(feature = "internals"))]
    assert!(!oracle, "oracle mode requires internals");
    #[cfg(feature = "internals")]
    let directory = RouteDirectory::global();

    println!("RESULT scenario={}", shape.name);
    println!("RESULT cycles={cycles}");
    println!("RESULT blocks_per_cycle={}", shape.blocks_per_cycle);
    println!("RESULT block_size={}", shape.block_size);
    #[cfg(feature = "internals")]
    if oracle {
        println!(
            "RESULT sidecar_size_bytes={}",
            core::mem::size_of::<SmallSidecar>()
        );
    }
    let mut total_work_ns = 0u128;
    let mut total_cycle_wall_ns = 0u128;
    let mut total_alloc_ns = 0u128;
    let mut total_cleanup_ns = 0u128;
    let mut total_blocks = 0usize;

    for cycle in 0..cycles {
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        let (regs_before, unlinks_before) = (
            directory.dbg_registrations_total(),
            directory.dbg_unlinks_total(),
        );
        #[cfg(feature = "internals")]
        let routes_before = oracle.then(|| directory.live_route_census_for_test());
        let cycle_start = Instant::now();
        let shape_copy = shape;
        let oracle_copy = oracle;
        let worker = std::thread::spawn(move || run_worker(shape_copy, oracle_copy))
            .join()
            .expect("worker thread panicked");
        let cycle_wall_ns = cycle_start.elapsed().as_nanos();
        total_work_ns += worker.work_ns;
        total_cycle_wall_ns += cycle_wall_ns;
        total_alloc_ns += worker.alloc_ns;
        total_cleanup_ns += worker.cleanup_ns;
        total_blocks += worker.blocks;

        println!("RESULT work_ns_{cycle}={}", worker.work_ns);
        println!("RESULT cycle_wall_ns_{cycle}={cycle_wall_ns}");
        #[cfg(feature = "internals")]
        if let Some(routes_during) = worker.live_routes_after_alloc {
            let before = routes_before.expect("oracle snapshot missing");
            let routes_after = directory.live_route_census_for_test();
            let before_count = route_count(before);
            let during_count = route_count(routes_during);
            let after_count = route_count(routes_after);
            assert!(
                during_count >= before_count,
                "route census shrank during alloc-only window"
            );
            assert!(
                after_count <= during_count,
                "route census grew after teardown"
            );
            let registered_live = during_count - before_count;
            let unlinked_live = during_count - after_count;
            println!("RESULT registrations_visible_{cycle}={registered_live}");
            println!("RESULT unlinks_visible_{cycle}={unlinked_live}");
            println!("RESULT live_routes_after_{cycle}={after_count}");
            #[cfg(feature = "bench-internals")]
            {
                let registrations = directory.dbg_registrations_total() - regs_before;
                let unlinks = directory.dbg_unlinks_total() - unlinks_before;
                let census_delta = after_count as isize - before_count as isize;
                assert_eq!(
                    census_delta,
                    registrations as isize - unlinks as isize,
                    "route counter/census conservation"
                );
                println!("RESULT registrations_total_{cycle}={registrations}");
                println!("RESULT unlinks_total_{cycle}={unlinks}");
            }
        }
        #[cfg(all(feature = "internals", feature = "bench-internals"))]
        if let Some(chunks) = worker.chunk_oracle {
            println!(
                "RESULT active_sidecar_routes_{cycle}={}",
                chunks.active_routes
            );
            println!(
                "RESULT constructed_chunks_{cycle}={}",
                chunks.observed_chunks
            );
            println!(
                "RESULT logical_payload_bytes_{cycle}={}",
                chunks.logical_payload_bytes
            );
            println!(
                "RESULT sidecar_backing_bytes_{cycle}={}",
                chunks.sidecar_backing_bytes
            );
            println!(
                "RESULT expected_chunks_by_route_{cycle}={:?}",
                chunks.expected_chunks_by_route
            );
            println!(
                "RESULT observed_chunks_by_route_{cycle}={:?}",
                chunks.observed_chunks_by_route
            );
        }
    }
    println!("RESULT total_work_ns={total_work_ns}");
    println!("RESULT total_cycle_wall_ns={total_cycle_wall_ns}");
    println!("RESULT total_alloc_ns={total_alloc_ns}");
    println!("RESULT total_cleanup_ns={total_cleanup_ns}");
    println!("RESULT total_blocks={total_blocks}");
}
