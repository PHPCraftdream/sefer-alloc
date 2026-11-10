use super::MIB;
#[cfg(all(
    feature = "production",
    feature = "alloc-stats",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]
use super::RSS_LIMIT;
use std::ffi::OsString;

const MIN: usize = 8 * MIB;
#[cfg(feature = "bench-internals")]
const GRANULE: usize = 2 * 1024 * 1024;
const MAX_LIVE: usize = 2 * 1024 * 1024 * 1024;
const CACHE_BUDGET: usize = 32 * MIB;

#[derive(Clone, Copy)]
pub struct Request {
    pub mode: sefer_alloc::LargeThpMode,
    pub cache: bool,
    pub bytes: usize,
    pub reps: usize,
    pub k: usize,
    pub scenario: &'static str,
}

fn error(message: impl std::fmt::Display) -> String {
    message.to_string()
}

pub fn expected_config(request: &Request) -> sefer_alloc::LargeCacheConfig {
    sefer_alloc::LargeCacheConfig::new()
        .budget_bytes(if request.cache { CACHE_BUDGET } else { 0 })
        .huge_page_mode(request.mode)
        .huge_page_block_min_bytes(MIN)
        .huge_page_budget_bytes(0)
        .huge_page_gradient(sefer_alloc::LargeThpGradient::CAUTIOUS)
}

pub fn parse_request(
    args: &[String],
    vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<Option<Request>, String> {
    let mut settings: [Option<String>; 3] = [None, None, None];
    for (key, value) in vars {
        if !key.to_string_lossy().starts_with("G1_BIG_") {
            continue;
        }
        let key = key
            .into_string()
            .map_err(|_| error("non-Unicode G1_BIG_* name"))?;
        let index = match key.as_str() {
            "G1_BIG_THP" => 0,
            "G1_BIG_CACHE" => 1,
            "G1_BIG_BYTES" => 2,
            _ => return Err(error(format!("unknown G1_BIG_* variable {key}"))),
        };
        let value = value
            .into_string()
            .map_err(|_| error(format!("non-Unicode value for {key}")))?;
        if settings[index].replace(value).is_some() {
            return Err(error(format!("ambiguous {key}")));
        }
    }
    if settings.iter().all(Option::is_none) {
        return Ok(None);
    }
    let mode = match settings[0].as_deref() {
        Some("off") => sefer_alloc::LargeThpMode::Off,
        Some("always") => sefer_alloc::LargeThpMode::Always,
        _ => {
            return Err(error(
                "G1_BIG_THP must be off or always; THP and cache are required",
            ))
        }
    };
    let cache = match settings[1].as_deref() {
        Some("off") => false,
        Some("on") => true,
        _ => {
            return Err(error(
                "G1_BIG_CACHE must be off or on; THP and cache are required",
            ))
        }
    };
    if args.len() < 5 || args.len() > 6 {
        return Err(error(
            "study expects scenario, sefer, size_mib, reps, and optional k",
        ));
    }
    let scenario = match args[1].as_str() {
        "cycle" => "cycle",
        "edge" => "edge",
        "grow" => "grow",
        "many" => "many",
        _ => return Err(error("study scenario must be cycle, edge, grow, or many")),
    };
    if args[2] != "sefer" {
        return Err(error("configured study supports SeferAlloc only"));
    }
    let mib = args[3].parse::<usize>().map_err(error)?;
    let reps = args[4].parse::<usize>().map_err(error)?;
    let k = args
        .get(5)
        .map_or(Ok(1), |value| value.parse::<usize>())
        .map_err(error)?;
    if mib == 0 || reps == 0 || k == 0 {
        return Err(error("size_mib, reps, and k must be positive"));
    }
    let ceiling = mib
        .checked_mul(MIB)
        .ok_or_else(|| error("size_mib overflow"))?;
    let bytes = match settings[2].as_deref() {
        None => ceiling,
        Some(value) if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
            let bytes = value.parse::<usize>().map_err(error)?;
            if bytes == 0 || bytes > ceiling || bytes <= ceiling - MIB {
                return Err(error("G1_BIG_BYTES does not ceil to CLI size_mib"));
            }
            bytes
        }
        _ => return Err(error("G1_BIG_BYTES must be exact positive decimal bytes")),
    };
    let live = bytes
        .checked_mul(if scenario == "many" { k } else { 1 })
        .ok_or_else(|| error("live-byte overflow"))?;
    if live > MAX_LIVE {
        return Err(error("study exceeds 2 GiB live payload"));
    }
    if cache {
        if (scenario, bytes, reps, k) != ("cycle", 16 * MIB, 2, 1) {
            return Err(error("cache-on requires cycle 16 MiB reps=2 k=1"));
        }
    } else if reps != 1 {
        return Err(error("cold study requires reps=1"));
    }
    Ok(Some(Request {
        mode,
        cache,
        bytes,
        reps,
        k,
        scenario,
    }))
}

fn runtime_admission() -> Result<(), String> {
    if cfg!(feature = "numa-aware") {
        return Err(error(
            "G1 big study runtime admission does not support numa-aware",
        ));
    }
    if !cfg!(all(
        feature = "production",
        feature = "alloc-stats",
        feature = "internals",
        feature = "bench-internals"
    )) {
        return Err(error(
            "study requires production, alloc-stats, internals, bench-internals, and no numa-aware",
        ));
    }
    Ok(())
}

#[cfg(all(
    feature = "production",
    feature = "alloc-stats",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]
pub fn configuration_readback_matches(
    request: &Request,
    allocator: &sefer_alloc::SeferAlloc,
) -> bool {
    let cache_budget = allocator.dbg_current_large_cache_budget();
    let tuple = expected_config(request).dbg_resolved_thp_tuple();
    let stats = allocator.stats();
    tuple.mode == request.mode
        && tuple.min_bytes == MIN
        && tuple.budget_bytes == 0
        && tuple.gradient == sefer_alloc::LargeThpGradient::CAUTIOUS
        && sefer_alloc::AllocCore::dbg_thp_tuple() == tuple
        && sefer_alloc::AllocCore::dbg_thp_tuple_conflicts() == 0
        && stats.thp_tuple_conflicts == 0
        && stats.config_conflicts == 0
        && cache_budget == Some(if request.cache { CACHE_BUDGET } else { 0 })
}

#[cfg(feature = "bench-internals")]
pub fn validate_capture(
    request: &Request,
    capture: &aligned_vmem::AdviceCaptureSnapshot,
) -> Result<(Vec<usize>, usize, Vec<serde_json::Value>), String> {
    use aligned_vmem::{AdviceEvent as E, AdviceLifetimeSnapshot as L, AdviceReplay};
    let replay = AdviceReplay::from_capture(capture).map_err(error)?;
    if capture.boundary != u64::try_from(capture.records.len()).map_err(error)? {
        return Err(error("capture boundary mismatch"));
    }
    let mut grow_steps = Vec::new();
    if request.scenario == "grow" {
        let mut size = MIB.min(request.bytes);
        grow_steps.push(size);
        while size < request.bytes {
            size = size.saturating_mul(2).min(request.bytes);
            grow_steps.push(size);
        }
    }
    let mut fresh = Vec::new();
    let (mut eligible, mut accepted) = (0, 0);
    let mut advice = Vec::new();
    for event in &capture.records {
        let words = event.words();
        match event.kind() {
            E::FRESH => {
                let bytes = usize::try_from(words[E::REQUESTED]).map_err(error)?;
                let expected = match request.scenario {
                    "grow" => grow_steps.contains(&bytes),
                    _ => bytes == request.bytes,
                };
                if !expected
                    || words[E::VALUE] != MIN as u64
                    || event.mode()
                        != Some(u8::from(request.mode == sefer_alloc::LargeThpMode::Always))
                    || event.threshold_eligible() != (bytes >= MIN)
                {
                    return Err(error("fresh request/config mismatch"));
                }
                fresh.push(bytes);
                eligible += usize::from(event.threshold_eligible());
            }
            E::ACCEPTED => {
                let usable = words[E::USABLE_LEN];
                let bytes = words[E::VALUE];
                if bytes == 0 || bytes != usable / GRANULE as u64 * GRANULE as u64 {
                    return Err(error(
                        "accepted prefix differs from 2 MiB usable-span floor",
                    ));
                }
                accepted += 1;
                advice.push(serde_json::json!({
                    "requested_bytes": words[E::REQUESTED],
                    "usable_span_bytes": usable,
                    "accepted_bytes": bytes,
                }));
            }
            E::UNKNOWN_ADVICE => return Err(error("native advice outcome unknown")),
            E::RELEASED => {}
            _ => return Err(error("unsupported advice event")),
        }
    }
    let expected_count = match request.scenario {
        "cycle" if request.cache => 1,
        "cycle" => request.reps,
        "edge" => 1,
        "many" => request.k,
        "grow" if fresh.len() <= grow_steps.len() => fresh.len(),
        "grow" => return Err(error("fresh mapping count exceeds grow workload")),
        _ => return Err(error("invalid scenario")),
    };
    if fresh.is_empty()
        || fresh.len() != expected_count
        || (request.mode == sefer_alloc::LargeThpMode::Always && accepted != eligible)
        || (request.mode == sefer_alloc::LargeThpMode::Off && accepted != 0)
        || (request.scenario == "grow" && !ordered_subset(&grow_steps, &fresh))
    {
        return Err(error("fresh mapping count/order or advice mismatch"));
    }
    let accepted_bytes = capture
        .records
        .iter()
        .try_fold(0u64, |sum, event| {
            if event.kind() == E::ACCEPTED {
                sum.checked_add(event.words()[E::VALUE])
            } else {
                Some(sum)
            }
        })
        .ok_or_else(|| error("accepted byte overflow"))?;
    if replay.snapshot().map_err(error)?.advice.process[L::TOTAL] != accepted_bytes {
        return Err(error("advice replay total mismatch"));
    }
    Ok((fresh, accepted, advice))
}

#[cfg(feature = "bench-internals")]
fn ordered_subset(steps: &[usize], observed: &[usize]) -> bool {
    let mut next = 0;
    observed.iter().all(|bytes| {
        if let Some(offset) = steps
            .get(next..)
            .and_then(|rest| rest.iter().position(|step| step == bytes))
        {
            next += offset + 1;
            true
        } else {
            false
        }
    })
}

#[cfg(all(
    feature = "production",
    feature = "alloc-stats",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]
mod configured {
    use super::*;
    use aligned_vmem::{AdviceCapture as Capture, AdviceEvent as E};

    fn os(huge: Option<u64>) -> serde_json::Value {
        let mem = proc_memstat::try_snapshot().ok();
        let faults = std::fs::read_to_string("/proc/self/stat")
            .ok()
            .and_then(|text| {
                text.rsplit_once(')')?
                    .1
                    .split_whitespace()
                    .nth(7)?
                    .parse::<u64>()
                    .ok()
            });
        serde_json::json!({
            "peak_rss": mem.as_ref().and_then(|m| m.peak_rss), "end_rss": mem.map(|m| m.rss),
            "user_ms": null, "kernel_ms": null, "faults": faults, "anon_huge_peak": huge,
        })
    }

    pub(super) fn execute(request: Request) -> Result<(), String> {
        if request.bytes > RSS_LIMIT as usize {
            return Err(error("request exceeds 7 GiB RSS bound"));
        }
        let config = expected_config(&request);
        Capture::begin_native().map_err(error)?;
        let allocator = sefer_alloc::SeferAlloc::with_config(config);
        if !configuration_readback_matches(&request, &allocator) {
            return Err(error(
                "allocator configuration/readback mismatch before work",
            ));
        }
        let hits_before = allocator.stats().large_cache_hits;
        let mut rows = Vec::with_capacity(request.reps);
        let (mut huge, mut huge_samples_valid) = (None, true);
        let mut cache_events = None;
        for rep in 0..request.reps {
            let start = if request.cache && rep == 1 {
                Some(Capture::boundary().map_err(error)?)
            } else {
                None
            };
            rows.push(super::super::once(
                super::super::Arm::Configured(&allocator),
                request.scenario,
                request.bytes,
                request.k,
                || match super::super::anon_huge_bytes_opt() {
                    Some(bytes) => huge = Some(huge.map_or(bytes, |peak: u64| peak.max(bytes))),
                    None => huge_samples_valid = false,
                },
            ));
            if let Some(start) = start {
                cache_events = Some((start, Capture::boundary().map_err(error)?));
            }
        }
        if !configuration_readback_matches(&request, &allocator) {
            return Err(error("allocator configuration/readback mismatch"));
        }
        let stats = allocator.stats();
        let hit_delta = stats
            .large_cache_hits
            .checked_sub(hits_before)
            .ok_or_else(|| error("cache hit counter decreased"))?;
        let cache_hit = if request.cache {
            if hit_delta != 1 || allocator.dbg_current_large_cache_used_bytes() == 0 {
                return Err(error("cache did not produce a real hit"));
            }
            let capture = Capture::snapshot().map_err(error)?;
            let (start, end) =
                cache_events.ok_or_else(|| error("cache phase boundaries missing"))?;
            let start = usize::try_from(start).map_err(error)?;
            let end = usize::try_from(end).map_err(error)?;
            if capture
                .records
                .get(start..end)
                .ok_or_else(|| error("cache phase boundary invalid"))?
                .iter()
                .any(|event| matches!(event.kind(), E::FRESH | E::ACCEPTED | E::UNKNOWN_ADVICE))
            {
                return Err(error("cache hit emitted fresh/advice evidence"));
            }
            true
        } else {
            false
        };
        allocator.trim_current_thread();
        if allocator.dbg_current_large_cache_used_bytes() != 0
            || !configuration_readback_matches(&request, &allocator)
        {
            return Err(error("trim or final allocator configuration check failed"));
        }
        let capture = Capture::snapshot().map_err(error)?;
        let (fresh, accepted, advice) = validate_capture(&request, &capture)?;
        let total = |row: &[u64; 3]| {
            row.iter()
                .try_fold(0u64, |sum, value| sum.checked_add(*value))
                .ok_or_else(|| error("phase time overflow"))
        };
        let cold = total(&rows[0])?;
        let cache_ns = if request.cache {
            Some(total(&rows[1])?)
        } else {
            None
        };
        let mode = if request.mode == sefer_alloc::LargeThpMode::Always {
            "Always"
        } else {
            "Off"
        };
        let resolved = sefer_alloc::AllocCore::dbg_thp_tuple();
        let json = serde_json::json!({
            "study": true, "diagnostics": "native-advice-capture", "scenario": request.scenario,
            "arm": "sefer", "requested_bytes": request.bytes, "reps": request.reps, "k": request.k,
            "cold_ns": cold, "warm_ns": null, "cache_ns": cache_ns,
            "timing_scope": "allocation+touch+free; runner full-child timing distinct", "rep": rows,
            "mode_requested": mode,
            "mode_resolved": if resolved.mode == sefer_alloc::LargeThpMode::Always { "Always" } else { "Off" },
            "thp_min_requested_bytes": MIN, "thp_min_resolved_bytes": resolved.min_bytes,
            "thp_budget_requested_bytes": 0, "thp_budget_resolved_bytes": resolved.budget_bytes,
            "gradient_requested": "CAUTIOUS", "gradient_resolved": format!("{:?}", resolved.gradient),
            "cache_requested": if request.cache { "on" } else { "off" },
            "cache_resolved_budget_bytes": allocator.dbg_current_large_cache_budget(),
            "cache_hit_confirmed": cache_hit, "large_cache_hits": hit_delta,
            "config_conflicts": stats.config_conflicts, "thp_tuple_conflicts": stats.thp_tuple_conflicts,
            "fresh_mappings": fresh.len(), "fresh_requested_bytes": fresh,
            "accepted_advice_count": accepted, "accepted_advice": advice,
            "advice_granule_bytes": GRANULE, "capture_events": capture.records.len(),
            "capture_boundary": capture.boundary, "capture_slot_storage_bytes": capture.slot_storage_bytes,
            "capture_total_storage_bytes": capture.total_storage_bytes,
            "capture_tls_context_bytes_per_thread": capture.tls_context_bytes_per_thread,
            "os": os(huge.filter(|_| huge_samples_valid)),
        });
        println!("{json}");
        if proc_memstat::try_snapshot()
            .ok()
            .and_then(|m| m.peak_rss)
            .is_some_and(|rss| rss > RSS_LIMIT)
        {
            return Err(error("peak RSS above 7 GiB"));
        }
        Ok(())
    }
}

pub(super) fn run_with(
    args: &[String],
    vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<bool, String> {
    let Some(request) = parse_request(args, vars)? else {
        return Ok(false);
    };
    runtime_admission()?;
    #[cfg(not(all(
        feature = "production",
        feature = "alloc-stats",
        feature = "internals",
        feature = "bench-internals",
        not(feature = "numa-aware")
    )))]
    let _ = request;
    #[cfg(all(
        feature = "production",
        feature = "alloc-stats",
        feature = "internals",
        feature = "bench-internals",
        not(feature = "numa-aware")
    ))]
    configured::execute(request)?;
    Ok(true)
}
