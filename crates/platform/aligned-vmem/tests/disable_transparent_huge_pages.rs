//! Contract tests for the owning `MADV_NOHUGEPAGE` operation, in two
//! evidence layers. Layer 1 is the kernel's own VMA policy record
//! (`/proc/self/smaps` VmFlags `nh`) — REQUIRED evidence on native Linux /
//! Android: an unreadable smaps is a loud test failure, never a silent
//! pass. Layer 2 is the mincore granularity oracle: it proves the observed
//! fault granularity is base-page (kernel truth under any huge/THP
//! backing) but CANNOT by itself distinguish the no-THP operation from a
//! no-op on a host that already uses base pages — layer 1 carries the
//! activation evidence. Unsupported / miri / mock backends must return
//! typed errors: explicit failure evidence, never fabricated success.

use std::sync::{Mutex, MutexGuard};

use aligned_vmem::reserve_aligned;

#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
use aligned_vmem::{Reservation, ResidencyRegion};

const MIB: usize = 1024 * 1024;
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
type VmaFlags = (usize, usize, String);

#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
fn overlapping_vma_flags(r: &Reservation) -> Option<Vec<VmaFlags>> {
    let text = std::fs::read_to_string("/proc/self/smaps").ok()?;
    let start = r.as_ptr().addr();
    let end = start.checked_add(r.len())?;
    let mut current = None;
    let mut found = Vec::new();
    for line in text.lines() {
        let first = line.split_whitespace().next().unwrap_or("");
        if let Some((lo, hi)) = first.split_once('-').and_then(|(lo, hi)| {
            Some((
                usize::from_str_radix(lo, 16).ok()?,
                usize::from_str_radix(hi, 16).ok()?,
            ))
        }) {
            current = Some((lo, hi));
        } else if let (Some((lo, hi)), Some(flags)) = (current, line.strip_prefix("VmFlags:")) {
            if lo < end && hi > start {
                found.push((
                    lo,
                    hi,
                    flags
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                        .join(" "),
                ));
            }
        }
    }
    if found.is_empty() {
        None
    } else {
        Some(found)
    }
}

/// A fresh untouched mapping must gain `nh` throughout its usable span
/// (layer 1, REQUIRED: an unreadable smaps is a loud failure). The
/// mincore granularity oracle (layer 2) is kernel truth about fault
/// granularity — after touching one byte at the start of each 2 MiB
/// region, exactly one base page per region may be resident; under any
/// huge/THP-backed region a single touch faults the whole 2 MiB and
/// mincore would report 512 resident pages — but it cannot by itself
/// distinguish the no-THP operation from a no-op on a host that already
/// uses base pages, so it is not activation proof. mincore also proves
/// the operation did not fault payload pages; a second application after
/// known writes proves it preserves them.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn fresh_mapping_gets_whole_span_nh_without_touching_or_changing_payload() {
    let _serial = serial();
    let len = 4 * MIB;
    let r = reserve_aligned(len, 2 * MIB).expect("reserve fresh mapping");
    r.try_disable_transparent_huge_pages()
        .expect("native no-THP policy must be accepted");

    let vmas = overlapping_vma_flags(&r)
        .expect("/proc/self/smaps must be readable to evidence the no-THP VMA policy record");
    let start = r.as_ptr().addr();
    let end = start + r.len();
    let mut covered_until = start;
    for (lo, hi, flags) in vmas {
        let overlap_start = lo.max(start);
        let overlap_end = hi.min(end);
        assert!(
            overlap_start <= covered_until,
            "unexpected VMA gap in mapping"
        );
        assert!(overlap_end > overlap_start, "empty VMA overlap");
        assert!(
            flags.split_whitespace().any(|flag| flag == "nh"),
            "every covered VMA must carry `nh`, got VmFlags: {flags}"
        );
        covered_until = covered_until.max(overlap_end);
    }
    assert_eq!(
        covered_until, end,
        "smaps VMAs must cover the whole usable span"
    );

    let ps = aligned_vmem::page_size();
    let fresh = r
        .try_sample_residency(&[
            ResidencyRegion::new(0, 2 * MIB),
            ResidencyRegion::new(2 * MIB, 2 * MIB),
        ])
        .expect("sample untouched mapping");
    assert_eq!(
        (fresh.resident_pages, fresh.sampled_pages),
        (0, len / ps),
        "policy application must leave fresh payload pages untouched"
    );

    // Granularity oracle (layer 2, kernel truth): touch ONE byte at the
    // start of each 2 MiB region, then demand exactly one resident base
    // page per region. Under any huge/THP-backed region one touch faults
    // the whole 2 MiB and mincore would report 512 resident pages per
    // region. This proves the observed fault granularity is base-page
    // under any backing, but it cannot distinguish the no-THP operation
    // from a no-op on a host that already uses base pages — activation
    // evidence lives in layer 1 above.
    let base = r.as_ptr();
    for region_start in [0usize, 2 * MIB] {
        // SAFETY: the reservation owns the committed span and `region_start`
        // is a live, in-bounds offset of the owned payload.
        unsafe { base.add(region_start).write_volatile(1u8) };
    }
    let granularity = r
        .try_sample_residency(&[
            ResidencyRegion::new(0, 2 * MIB),
            ResidencyRegion::new(2 * MIB, 2 * MIB),
        ])
        .expect("sample granularity oracle regions");
    assert_eq!(
        granularity.resident_pages, 2,
        "exactly one base page per 2 MiB region must be resident; a huge \
         backing would report 512 per region"
    );
    assert_eq!(
        granularity.sampled_pages,
        len / ps,
        "the sampler must have covered every base page of the span"
    );

    for offset in (0..len).step_by(ps) {
        // SAFETY: the reservation owns the committed span and `offset` is a
        // runtime-page multiple strictly inside its `len()` bytes.
        unsafe {
            base.add(offset)
                .write(((offset / ps) as u8).wrapping_add(1))
        };
    }
    r.try_disable_transparent_huge_pages()
        .expect("reapplying the policy is valid");
    for offset in (0..len).step_by(ps) {
        // SAFETY: same live, owned, in-bounds page starts as above.
        unsafe {
            assert_eq!(
                base.add(offset).read(),
                ((offset / ps) as u8).wrapping_add(1),
                "no-THP advice changed payload at {offset}"
            );
        }
    }
}

/// No backend on miri, mock, or non-Linux/Android may report the premise as
/// established. This is deliberately an error, never a fabricated success.
#[cfg(any(
    miri,
    aligned_vmem_mock,
    not(any(target_os = "linux", target_os = "android"))
))]
#[test]
fn unsupported_backend_is_typed_error() {
    let _serial = serial();
    let r = reserve_aligned(2 * MIB, 2 * MIB).expect("reserve");
    let err = r
        .try_disable_transparent_huge_pages()
        .expect_err("unsupported backends must not claim no-THP policy");
    assert!(
        !err.is_invalid_argument(),
        "unsupported, not malformed: {err:?}"
    );
    assert_eq!(err.os_code(), None);
}

/// An explicit HugeTLB grant is outside the transparent-policy contract.
/// This proves rejection only when a real HugeTLB grant is actually
/// obtained; on hosts without a grant the coverage is environmental,
/// never a fabricated skip.
#[cfg(all(
    feature = "huge-pages",
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn explicit_hugetlb_mapping_is_rejected_when_the_kernel_grants_it() {
    let _serial = serial();
    let Some(r) = aligned_vmem::reserve_aligned_huge(2 * MIB, 2 * MIB) else {
        return;
    };
    if r.is_huge() {
        let err = r
            .try_disable_transparent_huge_pages()
            .expect_err("explicit HugeTLB mapping is not a THP control mapping");
        assert!(err.is_invalid_argument(), "got {err:?}");
    }
}

/// The one-shot pre-syscall fault must fail closed; the following invocation
/// must reach the real native backend and establish the policy.
#[cfg(all(
    feature = "fault-injection",
    any(target_os = "linux", target_os = "android"),
    not(miri),
    not(aligned_vmem_mock)
))]
#[test]
fn injected_pre_syscall_failure_then_native_recovery() {
    use aligned_vmem::fault_injection::arm_fail_next_advise_range;

    let _serial = serial();
    arm_fail_next_advise_range(0);
    let r = reserve_aligned(4 * MIB, 2 * MIB).expect("reserve");
    arm_fail_next_advise_range(1);
    let err = r
        .try_disable_transparent_huge_pages()
        .expect_err("armed failure must not claim the policy was applied");
    assert!(
        !err.is_invalid_argument(),
        "injected pre-syscall error: {err:?}"
    );
    assert_eq!(err.os_code(), None);
    r.try_disable_transparent_huge_pages()
        .expect("one-shot fault is consumed; native operation must recover");
    arm_fail_next_advise_range(0);
}
