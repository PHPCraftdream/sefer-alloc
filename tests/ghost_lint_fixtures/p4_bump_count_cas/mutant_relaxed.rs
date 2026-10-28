// expect: rule requires ["AcqRel", "Acquire"]
pub(super) fn bump_count(reg: &Registry) -> Option<usize> {
    let mut count = reg.count.load(Ordering::Acquire);
    loop {
        match reg
            .count
            .compare_exchange_weak(count, count + 1, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => return Some(count as usize),
            Err(actual) => count = actual,
        }
    }
}

pub fn pick_with_hint_spill(hint: &AtomicU32, count: &AtomicU32, max: usize) -> Option<usize> {
    let candidate = hint.swap(max as u32, Ordering::AcqRel) as usize;
    let before = (count.load(Ordering::Acquire) as usize).min(max);
    if candidate < before {
        return Some(candidate);
    }
    None
}
