// Общий driver: полный installed workload, без per-op counters.
fn main() {
    let repeats: usize = std::env::args()
        .nth(1)
        .expect("нужно число повторений")
        .parse()
        .expect("целое число повторений");
    assert!((1..=4096).contains(&repeats));
    run_workload();
    let start = std::time::Instant::now();
    for _ in 0..repeats {
        run_workload();
    }
    let workload_ns = start.elapsed().as_nanos();
    let segments = installed_segments();
    assert_eq!(segments > 0, ARM == "sefer");
    let rss = rss_kib();
    let commit = commit_kib();
    println!(
        "{{\"arm\":\"{ARM}\",\"repeats\":{repeats},\"warmup_rounds\":1,\"workload_ns\":{workload_ns},\"segments_reserved_total\":{segments},\"rss_kib\":{rss},\"commit_kib\":{commit},\"completed\":true}}"
    );
}
