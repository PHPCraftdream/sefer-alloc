//! Actual operation core with tiny buffers; no OS probes or campaign execution.
#[path = "../examples/_shared/j13b_owned_block.rs"]
mod owned;
#[path = "../examples/_shared/j13b_run_stats.rs"]
mod run_stats;
#[path = "../examples/_shared/j13b_window_kernel.rs"]
mod window_kernel;
#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use run_stats::RunStats;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::{Barrier, Mutex};
use window_kernel::WindowKernel;
use workload::{Shape, TouchPlan};

#[derive(Default)]
struct FailSecond(Mutex<(usize, usize)>);

// SAFETY: all admitted memory/layout operations delegate to System. The second
// allocation returns null without allocating; logging does not inspect payload.
// This bounded fixture is not installed globally, so its mutex cannot recurse.
unsafe impl GlobalAlloc for FailSecond {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut counts = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        counts.0 += 1;
        let fail = counts.0 == 2;
        drop(counts);
        if fail {
            return std::ptr::null_mut();
        }
        // SAFETY: the trait caller supplied a valid nonzero layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .1 += 1;
        // SAFETY: the pointer is uniquely owned System storage with its original layout.
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[test]
fn aggregate_bounds_are_rejected_before_allocator_or_worker_callbacks() {
    let allocator = FailSecond::default();
    for plans in [
        vec![],
        vec![TouchPlan {
            bytes: 0,
            shape: Shape::Dense,
        }],
        vec![
            TouchPlan {
                bytes: 1024 * 1024 * 1024 + 1,
                shape: Shape::Dense
            };
            2
        ],
        vec![TouchPlan {
            bytes: 64,
            shape: Shape::EdgeTwoPages,
        }],
    ] {
        let page = if plans
            .first()
            .is_some_and(|plan| plan.shape == Shape::EdgeTwoPages)
        {
            64
        } else {
            16
        };
        assert!(WindowKernel::batch(&allocator, &plans, page).is_err());
    }
    assert!(WindowKernel::churn(
        &allocator,
        TouchPlan {
            bytes: 64,
            shape: Shape::Dense
        },
        5,
        2,
        16,
        |_, _| panic!("invalid worker count must not start a worker")
    )
    .is_err());
    assert_eq!(*allocator.0.lock().unwrap(), (0, 0));
}

#[test]
fn batch_counts_real_requests_writes_and_frees() {
    let plans = [
        TouchPlan {
            bytes: 64,
            shape: Shape::Dense,
        },
        TouchPlan {
            bytes: 128,
            shape: Shape::Sparse,
        },
    ];
    let stats = WindowKernel::batch(&System, &plans, 16).expect("small actual batch");
    assert_eq!(stats.allocations, 2);
    assert_eq!(stats.frees, 2);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(stats.requested_bytes_issued, 192);
    assert_eq!(stats.bytes_written, 65);
}

#[test]
fn partial_allocation_failure_releases_every_admitted_buffer() {
    let allocator = FailSecond::default();
    let plans = [TouchPlan {
        bytes: 64,
        shape: Shape::Dense,
    }; 3];
    assert!(WindowKernel::batch(&allocator, &plans, 16).is_err());
    assert_eq!(*allocator.0.lock().unwrap(), (2, 1));
}

#[test]
fn growth_calls_real_realloc_and_touches_only_new_bytes() {
    let stats = WindowKernel::grow(&System, 8, 64, 16).expect("real bounded growth");
    assert_eq!(stats.allocations, 1);
    assert_eq!(stats.reallocations, 3);
    assert_eq!(stats.frees, 1);
    assert_eq!(stats.requested_bytes_issued, 8 + 16 + 32 + 64);
    assert_eq!(stats.bytes_written, 64);
}

#[test]
fn four_workers_execute_and_all_joins_precede_an_error_result() {
    let seen = Mutex::new(Vec::new());
    let barrier = Barrier::new(4);
    let parent = std::thread::current().id();
    let stats = WindowKernel::churn(
        &System,
        TouchPlan {
            bytes: 64,
            shape: Shape::Quarter,
        },
        4,
        2,
        16,
        |worker, _| {
            seen.lock()
                .unwrap()
                .push((worker, std::thread::current().id()));
            barrier.wait();
            Ok(())
        },
    )
    .expect("bounded four-worker run");
    assert_eq!(stats.allocations, 8);
    assert_eq!(stats.frees, 8);
    assert_eq!(stats.requested_bytes_issued, 512);
    assert_eq!(stats.bytes_written, 128);
    let entries = seen.lock().unwrap();
    assert_eq!(entries.len(), 4);
    assert!(entries.iter().all(|(_, thread)| *thread != parent));
    for worker in 0..4 {
        assert_eq!(entries.iter().filter(|(id, _)| *id == worker).count(), 1);
    }
    drop(entries);
    let barrier = Barrier::new(4);
    let result = WindowKernel::churn(
        &System,
        TouchPlan {
            bytes: 64,
            shape: Shape::Dense,
        },
        4,
        1,
        16,
        |worker, _| {
            barrier.wait();
            if worker == 2 {
                panic!("controlled preflight panic");
            }
            Ok(())
        },
    );
    assert!(
        result.is_err(),
        "worker panic must not become a successful sample"
    );
}

#[test]
fn statistic_merge_is_checked_transactional_and_does_not_sum_wall_time() {
    let mut stats = RunStats {
        alloc_ns: 3,
        bytes_written: u64::MAX,
        wall_ns: 11,
        ..RunStats::default()
    };
    let before = RunStats {
        alloc_ns: 3,
        bytes_written: u64::MAX,
        wall_ns: 11,
        ..RunStats::default()
    };
    assert!(stats
        .merge(RunStats {
            alloc_ns: 7,
            bytes_written: 1,
            ..RunStats::default()
        })
        .is_err());
    assert_eq!(stats, before);
    stats
        .merge(RunStats {
            allocations: 1,
            wall_ns: 99,
            ..RunStats::default()
        })
        .expect("nonoverflowing merge");
    assert_eq!(stats.allocations, 1);
    assert_eq!(stats.wall_ns, 11);
}
