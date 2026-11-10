#[path = "../examples/_shared/j13b_workload.rs"]
#[allow(dead_code)]
mod workload;
use std::thread;
use workload::{
    migration_worker, plan, run_pair, touch_slots, touch_slots_stepped, AuditEvent, Bucket,
    JoinOutcome, MigrationFailure, MigrationReport, OwnedBytes, Phase, Role, Shape, TouchPlan,
    Training, TrainingState, WorkerError, WriteSlot, STRIDE_STEP_BYTES, TOUCH_BYTE,
};

#[test]
fn plans_preserve_registered_sizes_buckets_and_bounds() {
    assert_eq!(workload::MAX_SCENARIO_BYTES, 8_589_934_592_u64);
    for scenario in [
        "dense",
        "sparse",
        "stride",
        "tail",
        "phased",
        "mixed",
        "cycle",
        "grow",
        "many",
        "edge",
        "churn",
        "churn-small",
    ] {
        for cell in plan(scenario).unwrap_or_else(|error| panic!("{scenario}: {error}")) {
            assert!(cell.blocks <= 128, "{}", cell.id);
            assert!(
                cell.bytes * cell.blocks <= 2 * 1024 * 1024 * 1024,
                "{}",
                cell.id
            );
        }
    }
    assert_eq!(plan("many").unwrap()[0].bytes * 128, 2 * 1024 * 1024 * 1024);
    assert_eq!(
        plan("sparse").unwrap()[0].bytes * 128,
        2 * 1024 * 1024 * 1024
    );
    assert_eq!(
        plan("cycle")
            .unwrap()
            .iter()
            .map(|cell| cell.bytes)
            .collect::<Vec<_>>(),
        vec![1024 * 1024, 128 * 1024 * 1024, 1024 * 1024 * 1024]
    );
    let edge = plan("edge").unwrap()[0];
    assert_eq!(edge.bucket, Bucket::OneTwentyEightTo512);
    let grow = plan("grow").unwrap()[0];
    assert_eq!(grow.bytes, 1024 * 1024 * 1024);
    assert_eq!(grow.growth_start_bytes, Some(1024 * 1024));
    let mixed = plan("mixed").unwrap();
    assert_eq!(mixed[1].bytes, 64 * 1024 * 1024);
    assert_eq!(mixed[1].bucket, Bucket::ThirtyTwoTo128);
    assert_eq!(
        mixed[4].co_resident,
        Some((Shape::SparseTwoPercent, 64 * 1024 * 1024, 8))
    );
    let churn = plan("churn").unwrap()[0];
    assert_eq!(
        (churn.threads, churn.operations_per_thread, churn.shape),
        (4, 1024, Shape::Quarter)
    );
    let small = plan("churn-small").unwrap()[0];
    assert_eq!(
        (
            small.minimum_request,
            small.threads,
            small.operations_per_thread,
            small.shape
        ),
        (0, 4, 4096, Shape::Quarter)
    );
}

const SENTINEL: u8 = 0xA5;

/// Runs the shared kernel on a sentinel-filled safe buffer and returns (count, touched
/// offsets), asserting every touched byte holds TOUCH_BYTE. Anything not listed in the
/// touched offsets is, by construction, still the untouched sentinel.
fn observe(n: usize, shape: Shape, step: usize) -> (usize, Vec<usize>) {
    let mut v = vec![SENTINEL; n];
    let count = touch_slots_stepped(&mut v, shape, step);
    let touched: Vec<usize> = v
        .iter()
        .enumerate()
        .filter(|(_, b)| **b != SENTINEL)
        .map(|(i, _)| i)
        .collect();
    assert!(touched.iter().all(|&i| v[i] == TOUCH_BYTE));
    (count, touched)
}
fn check(n: usize, shape: Shape, expected: Vec<usize>) {
    let (count, touched) = observe(n, shape, 10);
    assert_eq!(touched, expected, "{shape:?} n={n}");
    assert_eq!(count, expected.len(), "{shape:?} n={n} count");
}

#[test]
fn kernel_writes_exactly_the_hand_derived_offsets_and_nothing_else() {
    assert_ne!(SENTINEL, TOUCH_BYTE);
    check(10, Shape::Dense, (0..10).collect());
    check(100, Shape::Sparse, vec![0]);
    // stride step 10: offsets are multiples of 10 strictly below n.
    check(35, Shape::Stride, vec![0, 10, 20, 30]);
    check(30, Shape::Stride, vec![0, 10, 20]);
    // tail starts at n - n/4.
    check(100, Shape::Tail, (75..100).collect());
    check(99, Shape::Tail, (75..99).collect());
    // 5% = n/20, 2% = n/50, quarter = n/4, all from offset 0.
    check(100, Shape::SparseFivePercent, (0..5).collect());
    check(99, Shape::SparseFivePercent, (0..4).collect());
    check(19, Shape::SparseFivePercent, vec![]);
    check(100, Shape::SparseTwoPercent, vec![0, 1]);
    check(99, Shape::SparseTwoPercent, vec![0]);
    check(49, Shape::SparseTwoPercent, vec![]);
    check(100, Shape::Quarter, (0..25).collect());
    check(7, Shape::Quarter, vec![0]);
    // edge: first byte and the byte one base page before the end.
    check(8192, Shape::EdgeTwoPages, vec![0, 4096]);
    check(10000, Shape::EdgeTwoPages, vec![0, 5904]);
    check(8191, Shape::EdgeTwoPages, vec![]);
    check(0, Shape::Dense, vec![]);
}

#[test]
fn kernel_uses_the_registered_stride_step_and_writes_each_slot_once() {
    assert_eq!(STRIDE_STEP_BYTES, 33_554_432);
    let n = 2 * STRIDE_STEP_BYTES + 1;
    let mut v = vec![SENTINEL; n];
    assert_eq!(touch_slots(&mut v, Shape::Stride), 3);
    let touched: Vec<usize> = (0..n).filter(|&i| v[i] != SENTINEL).collect();
    assert_eq!(touched, vec![0, 33_554_432, 67_108_864]);

    #[derive(Clone, Copy, Default)]
    struct Probe {
        writes: u8,
        last: u8,
    }
    impl WriteSlot for Probe {
        fn put(&mut self, value: u8) {
            self.writes += 1;
            self.last = value;
        }
    }
    let mut probes = vec![Probe::default(); 64];
    assert_eq!(touch_slots(&mut probes, Shape::Dense), 64);
    assert!(probes.iter().all(|p| p.writes == 1 && p.last == TOUCH_BYTE));
}

#[test]
fn owned_bytes_touch_delegates_to_the_kernel_and_validates_allocation() {
    // OwnedBytes::touch runs the same kernel over MaybeUninit slots. Those bytes are
    // never read (no unsafe); only the returned write count is observable here.
    for (bytes, shape, count) in [
        (100, Shape::Dense, 100),
        (100, Shape::Sparse, 1),
        (100, Shape::Tail, 25),
        (100, Shape::SparseFivePercent, 5),
        (100, Shape::SparseTwoPercent, 2),
        (100, Shape::Quarter, 25),
        (8192, Shape::EdgeTwoPages, 2),
    ] {
        let mut data = OwnedBytes::allocate(TouchPlan { bytes, shape }).unwrap();
        assert_eq!(data.touch(), count, "{shape:?}");
        assert_eq!(observe(bytes, shape, 10).0, count, "{shape:?} vs kernel");
    }
    assert!(OwnedBytes::allocate(TouchPlan {
        bytes: 0,
        shape: Shape::Dense
    })
    .is_err());
    assert!(OwnedBytes::allocate(TouchPlan {
        bytes: 8191,
        shape: Shape::EdgeTwoPages
    })
    .is_err());
}

#[test]
fn training_reports_invalid_share_and_none_is_explicit() {
    let mut t = Training::new(false);
    for _ in 0..95 {
        t.observe(true, Some(0.95), None);
    }
    assert_eq!(
        t.observe(true, Some(0.95), Some(f64::NAN)),
        TrainingState::InvalidShare {
            eligible: 96,
            valid: 96
        }
    );
    let mut t = Training::new(true);
    for _ in 0..95 {
        t.observe(true, Some(0.0), None);
    }
    assert_eq!(
        t.observe(true, Some(0.0), Some(1.5)),
        TrainingState::InvalidShare {
            eligible: 96,
            valid: 96
        }
    );
    let mut t = Training::new(true);
    for _ in 0..95 {
        t.observe(true, Some(0.0), None);
    }
    assert_eq!(
        t.observe(true, Some(0.0), None),
        TrainingState::Complete {
            eligible: 96,
            valid: 96,
            attained_share: None
        }
    );
    let mut t = Training::new(false);
    for _ in 0..1535 {
        t.observe(true, Some(0.5), None);
    }
    assert_eq!(
        t.observe(true, Some(0.5), Some(-0.1)),
        TrainingState::InvalidShare {
            eligible: 1536,
            valid: 0
        }
    );
}

fn sparse_plans(n: usize) -> Vec<TouchPlan> {
    vec![
        TouchPlan {
            bytes: 4096,
            shape: Shape::Sparse
        };
        n
    ]
}
fn position(events: &[AuditEvent], wanted: AuditEvent) -> usize {
    events
        .iter()
        .position(|e| *e == wanted)
        .unwrap_or_else(|| panic!("missing {wanted:?} in {events:?}"))
}
fn failure(r: Result<MigrationReport, WorkerError>) -> MigrationFailure {
    match r {
        Err(WorkerError::Failed(f)) => f,
        other => panic!("expected Failed, got {other:?}"),
    }
}
/// Both workers exited (exit events from the threads' own guards), both joins returned
/// a value produced by the finished thread (or Panicked), and no buffer leaked.
fn assert_both_exited_and_joined(r: &MigrationReport, a_panics: bool, b_panics: bool) {
    assert_ne!(r.a_thread, r.b_thread);
    assert_ne!(r.a_thread, thread::current().id());
    assert_ne!(r.b_thread, thread::current().id());
    let a_exit = AuditEvent::WorkerExited {
        role: Role::A,
        thread: r.a_thread,
    };
    let b_exit = AuditEvent::WorkerExited {
        role: Role::B,
        thread: r.b_thread,
    };
    let (ai, bi) = (position(&r.events, a_exit), position(&r.events, b_exit));
    // The exit event is the last event each thread emits.
    let last = |t| {
        r.events
            .iter()
            .rposition(|e| match *e {
                AuditEvent::Allocated { thread, .. }
                | AuditEvent::Touched { thread, .. }
                | AuditEvent::Dropped { thread, .. }
                | AuditEvent::WorkerExited { thread, .. } => thread == t,
            })
            .unwrap()
    };
    assert_eq!(last(r.a_thread), ai);
    assert_eq!(last(r.b_thread), bi);
    assert_eq!(
        r.a_join,
        if a_panics {
            JoinOutcome::Panicked
        } else {
            JoinOutcome::Returned { thread: r.a_thread }
        }
    );
    assert_eq!(
        r.b_join,
        if b_panics {
            JoinOutcome::Panicked
        } else {
            JoinOutcome::Returned { thread: r.b_thread }
        }
    );
    assert_eq!(r.outstanding_buffers, 0);
}

#[test]
fn success_path_audit_shows_actual_threads_order_drops_and_exits() {
    let plans = [
        TouchPlan {
            bytes: 4096,
            shape: Shape::Sparse,
        },
        TouchPlan {
            bytes: 8192,
            shape: Shape::Quarter,
        },
        TouchPlan {
            bytes: 8192,
            shape: Shape::EdgeTwoPages,
        },
    ];
    let r = migration_worker(&plans).unwrap();
    assert_eq!(r.counts, vec![1, 2048, 2]);
    assert_eq!(r.events.len(), 3 * 3 + 2);
    assert_both_exited_and_joined(&r, false, false);
    for (id, count) in [1usize, 2048, 2].into_iter().enumerate() {
        let alloc = position(
            &r.events,
            AuditEvent::Allocated {
                id,
                thread: r.a_thread,
            },
        );
        let touch = position(
            &r.events,
            AuditEvent::Touched {
                id,
                thread: r.b_thread,
                count,
            },
        );
        let drop = position(
            &r.events,
            AuditEvent::Dropped {
                id,
                thread: r.b_thread,
            },
        );
        assert!(alloc < touch && touch < drop, "id {id}: {:?}", r.events);
        if id + 1 < 3 {
            let next = position(
                &r.events,
                AuditEvent::Allocated {
                    id: id + 1,
                    thread: r.a_thread,
                },
            );
            assert!(
                drop < next,
                "buffer {id} dropped before the next allocation"
            );
        }
    }
}

#[test]
fn bounds_are_rejected_before_any_thread_exists() {
    for bad in [
        vec![TouchPlan {
            bytes: workload::MAX_WORKER_LIVE_BYTES + 1,
            shape: Shape::Sparse,
        }],
        sparse_plans(workload::MAX_WORKER_PLANS + 1),
    ] {
        assert!(matches!(
            migration_worker(&bad),
            Err(WorkerError::Rejected(_))
        ));
    }
}

#[test]
fn allocation_failure_in_a_still_exits_and_joins_both_workers() {
    let f = failure(run_pair(
        &sparse_plans(3),
        |id, p| {
            if id == 1 {
                Err("injected alloc failure")
            } else {
                OwnedBytes::allocate(p)
            }
        },
        |_, b| Ok(b.touch()),
    ));
    assert_eq!(f.error, "injected alloc failure");
    let r = &f.report;
    assert_eq!(r.counts, vec![1]);
    assert_both_exited_and_joined(r, false, false);
    assert!(r.events.contains(&AuditEvent::Dropped {
        id: 0,
        thread: r.b_thread
    }));
    assert!(!r
        .events
        .iter()
        .any(|e| matches!(e, AuditEvent::Allocated { id: 1, .. })));
}

#[test]
fn touch_error_in_b_drops_the_buffer_on_b_and_joins_both_workers() {
    let f = failure(run_pair(
        &sparse_plans(3),
        |_, p| OwnedBytes::allocate(p),
        |id, b| {
            if id == 1 {
                Err("injected touch failure")
            } else {
                Ok(b.touch())
            }
        },
    ));
    assert_eq!(f.error, "injected touch failure");
    let r = &f.report;
    assert_eq!(r.counts, vec![1]);
    assert_both_exited_and_joined(r, false, false);
    assert!(r.events.contains(&AuditEvent::Allocated {
        id: 1,
        thread: r.a_thread
    }));
    assert!(r.events.contains(&AuditEvent::Dropped {
        id: 1,
        thread: r.b_thread
    }));
    assert!(!r
        .events
        .iter()
        .any(|e| matches!(e, AuditEvent::Touched { id: 1, .. })));
}

#[test]
fn touch_panic_in_b_unwinds_drops_exits_and_both_joins_are_observed() {
    let f = failure(run_pair(
        &sparse_plans(3),
        |_, p| OwnedBytes::allocate(p),
        |id, b| {
            if id == 1 {
                panic!("injected touch panic");
            }
            Ok(b.touch())
        },
    ));
    assert_eq!(f.error, "worker B panicked");
    let r = &f.report;
    assert_both_exited_and_joined(r, false, true);
    assert!(r.events.contains(&AuditEvent::Dropped {
        id: 1,
        thread: r.b_thread
    }));
}

#[test]
fn allocation_panic_in_a_still_lets_b_exit_and_both_joins_are_observed() {
    let f = failure(run_pair(
        &sparse_plans(3),
        |id, p| {
            if id == 1 {
                panic!("injected alloc panic");
            }
            OwnedBytes::allocate(p)
        },
        |_, b| Ok(b.touch()),
    ));
    assert_eq!(f.error, "worker A panicked");
    let r = &f.report;
    assert_both_exited_and_joined(r, true, false);
    assert!(r.events.contains(&AuditEvent::Dropped {
        id: 0,
        thread: r.b_thread
    }));
}

#[test]
fn training_counts_only_real_class_observations_and_reports_insufficient_cap() {
    let mut sparse = Training::new(true);
    for _ in 0..95 {
        assert!(matches!(
            sparse.observe(true, Some(0.05), None),
            TrainingState::Collecting { .. }
        ));
    }
    assert_eq!(
        sparse.observe(true, Some(0.05), Some(0.2)),
        TrainingState::Complete {
            eligible: 96,
            valid: 96,
            attained_share: Some(0.2)
        }
    );
    let mut dense = Training::new(false);
    for _ in 0..1535 {
        dense.observe(true, Some(0.5), Some(0.8));
    }
    assert_eq!(
        dense.observe(true, None, None),
        TrainingState::Insufficient {
            eligible: 1536,
            valid: 0,
            attained_share: None
        }
    );
    assert_eq!(plan("phased").unwrap()[1].phase, Phase::TrainSparse);
}

/// Hand-written canonical (cell id -> windows) table, copied independently from the
/// `CELL_MATRIX` in `scripts/j13b_schema.mjs`. A drifting Rust window name fails here.
#[test]
fn plan_cell_ids_and_windows_match_the_canonical_js_matrix() {
    let canonical: &[(&str, &[&str])] = &[
        ("cycle-1m", &["initial"]),
        ("cycle-128m", &["initial"]),
        ("cycle-1g", &["initial"]),
        ("grow-1g", &["initial"]),
        ("many-128x16m", &["initial"]),
        ("sparse-128x16m", &["initial"]),
        ("stride-64x16m", &["initial"]),
        ("tail-32x16m", &["initial"]),
        ("edge-128m", &["initial"]),
        (
            "phased",
            &[
                "initial",
                "sparse-training",
                "sparse-trained",
                "dense-training",
                "dense-trained",
            ],
        ),
        (
            "mixed",
            &[
                "dense-training",
                "sparse-training",
                "dense-only",
                "sparse-only",
                "co-resident",
            ],
        ),
        ("churn-4x1024", &["initial"]),
        ("churn-small-4x4096", &["initial"]),
    ];
    // The `dense` scenario alias is a Rust-only helper, not a registered cell, so it is
    // deliberately excluded from this JS-consistency check.
    let scenarios = [
        "cycle",
        "grow",
        "many",
        "sparse",
        "stride",
        "tail",
        "edge",
        "phased",
        "mixed",
        "churn",
        "churn-small",
    ];
    let mut actual: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for s in scenarios {
        for c in plan(s).unwrap() {
            actual.entry(c.cell_id).or_default().push(c.window);
        }
    }
    assert_eq!(actual.len(), canonical.len(), "{actual:?}");
    for (id, windows) in canonical {
        let mut want: Vec<&str> = windows.to_vec();
        let mut got = actual
            .get(id)
            .unwrap_or_else(|| panic!("missing {id}"))
            .clone();
        want.sort_unstable();
        got.sort_unstable();
        assert_eq!(got, want, "windows drifted for {id}");
    }
}
