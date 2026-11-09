//! Real own-process VmHWM reset witness; explicitly run only on native Linux CI.
#![cfg(all(target_os = "linux", not(miri), feature = "alloc-core"))]
#[path = "../examples/_shared/j13b_os_probe.rs"]
mod os_probe;
#[path = "../examples/_shared/j13b_os_sample.rs"]
mod os_sample;
use os_probe::OsProbe;

fn fault_mapping(bytes: usize) -> aligned_vmem::Reservation {
    let page = aligned_vmem::page_size();
    assert!(page.is_power_of_two() && bytes.is_multiple_of(page));
    let reservation =
        aligned_vmem::try_reserve_aligned(bytes, page).expect("private committed mapping");
    let base = reservation.as_ptr();
    for offset in (0..reservation.len()).step_by(page) {
        // SAFETY: this sole owner retains the fresh committed writable mapping;
        // offset is inside its byte extent and no payload references/aliases exist.
        unsafe { base.add(offset).write_volatile(0x5a) };
    }
    reservation
}

#[test]
#[ignore = "requires native Linux procfs and exclusive control of this test process's peak epoch"]
fn reset_peak_is_real_window_scoped_and_faults_are_not_fabricated() {
    const MIB: usize = 1024 * 1024;
    // A checked VM release avoids assumptions about a malloc retention policy.
    let warm = fault_mapping(32 * MIB);
    let old_peak = proc_memstat::try_snapshot()
        .expect("real memory probe")
        .peak_rss
        .expect("VmHWM");
    warm.try_release().expect("checked native release");
    let now = proc_memstat::try_snapshot().expect("post-release RSS").rss;
    assert!(
        old_peak
            .checked_sub(now)
            .is_some_and(|gap| gap > 16 * MIB as u64),
        "native environment must provide a clear reset signal"
    );

    let probe = OsProbe::begin().expect("own clear_refs=5 epoch supported");
    let probe_start_peak = probe.start_peak_bytes();
    assert!(
        probe_start_peak < old_peak,
        "reset must really lower the previous peak"
    );
    let overlap = match OsProbe::begin() {
        Ok(_) => panic!("overlapping epochs must not reset an active window"),
        Err(error) => error,
    };
    assert_eq!(overlap.kind(), std::io::ErrorKind::WouldBlock);
    let active = fault_mapping(16 * MIB);
    let peak_inside = proc_memstat::try_snapshot()
        .expect("in-window RSS")
        .peak_rss
        .expect("in-window VmHWM");
    active.try_release().expect("checked in-window release");
    let sample = probe
        .finish()
        .expect("complete non-fabricated window sample");
    eprintln!(
        "[j13b_os_probe_native] old_peak={old_peak} post_release_rss={now} start_rss={} start_peak={} live_peak={peak_inside} end_rss={} retained_peak={} minor={} major={}",
        sample.rss_start_bytes, probe_start_peak, sample.rss_end_bytes,
        sample.rss_peak_bytes, sample.minor_faults, sample.major_faults,
    );
    assert!(
        sample.rss_peak_bytes >= peak_inside,
        "a freed in-window peak must survive: retained={} live={peak_inside}",
        sample.rss_peak_bytes,
    );
    assert!(
        sample.rss_peak_bytes < old_peak,
        "cumulative process peak must not leak into this window"
    );
    assert!(sample
        .rss_peak_bytes
        .checked_sub(sample.rss_end_bytes)
        .is_some_and(|gap| gap > 8 * MIB as u64));
    assert!(sample.rss_peak_bytes >= sample.rss_start_bytes);
    assert!(sample.minor_faults > 0 || sample.major_faults > 0);
    assert_eq!(
        sample.peak_scope,
        "own-process VmHWM since clear_refs=5, including probes"
    );
    assert!(sample.fault_scope.contains("excludes children"));
    // Zero is valid; missing telemetry is an error. Do not assert a
    // same-instant relation between separately read smaps and RSS counters.
    std::hint::black_box(sample.anon_huge_pages_bytes);
    let next = OsProbe::begin().expect("finished epoch released its lease");
    let _next_sample = next.finish().expect("second isolated peak epoch");
}
