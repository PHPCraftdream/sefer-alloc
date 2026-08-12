# aligned-vmem `VirtualAlloc2` VA-space optimization opportunity: DESIGN-ONLY (reasoned from spec)

**Task:** aligned-vmem round 4 code-quality review finding — document the
Windows `VirtualAlloc2` opportunity and its trade-offs. The Windows
`align > 64 KiB` two-call reservation path
(`crates/vmem/src/lib.rs:1497-1589`) — the crate's own flagship
allocator-segment use case (e.g. `align=4MiB, size=4MiB` is
`src/alloc_core/os.rs`'s only reservation shape) — currently costs 2
syscalls and holds `size + align` bytes of virtual address space to
deliver `size` bytes of usable span (e.g. 8 MiB VA held for 4 MiB
usable). `VirtualAlloc2` (Windows 10 1803+ / Server 2016+) accepts a
`MEM_EXTENDED_PARAMETER` of type
`MemExtendedParameterAddressRequirements` with arbitrary alignment,
allowing a single syscall with NO over-reserve.

**Outcome: DESIGN-ONLY.** No `crates/vmem/src/`, `Cargo.toml`, or `tests/`
file is modified. The deliverable is this doc, which documents the
opportunity, its trade-offs, and the open decision it is gated on
(Windows-version-floor policy + GetProcAddress-vs-link-time choice).

**Date:** 2026-08-13
**Base revision:** `vmem-r4h` branch @ `8804fc9` (current HEAD in this
worktree; aligned-vmem crate unchanged since task #848 landed the
align <= 64 KiB fast path).
**Platform:** Analysis is reasoned from Win32/BSD API specifications,
not measured on any specific host. The VirtualAlloc2 availability floor
and MEM_ADDRESS_REQUIREMENTS structure are documented in Microsoft's
official API documentation; the BSD MAP_ALIGNED flag is documented in
the respective BSD `mmap(2)` man pages.

---

## 0. TL;DR — open opportunity gated on policy, with a separate BSD spec-read-only note

**Windows `VirtualAlloc2` + `MEM_EXTENDED_PARAMETER_ADDRESS_REQUIREMENTS`** would
eliminate both inefficiencies of the current `align > 64 KiB` path:
* **Syscalls:** 2 → 1 (no separate `VirtualAlloc(MEM_RESERVE)` + alignment-finding
  `VirtualFree` + `VirtualAlloc(MEM_COMMIT)` dance)
* **Virtual address space:** `size + align` → `size` (no over-reserve to find an
  aligned window)

For the crate's own flagship use case (`align=4MiB, size=4MiB`), this means
reserving exactly 4 MiB VA instead of 8 MiB — a 2× VA-space win, and a 2×
syscall reduction (though R32-13's measured data shows the reserve+commit
pair is only ~4.3–4.8 % of the Windows segment lifecycle overall, so this
would be a VA-space optimization, not a latency optimization).

**The honest blocker:** VirtualAlloc2 is exported from `kernelbase.dll`,
NOT `kernel32.dll`. Using it would require either:
* (a) A hard link-time dependency on `kernelbase.dll` — silently dropping
  pre-1803 Windows support with no fallback, **OR**
* (b) Proper `GetProcAddress`-based dynamic resolution behind something like
  `OnceLock<Option<fn(...)>>` with fallback to the existing two-call path
  on older Windows — real NEW unsafe surface (function-pointer FFI resolved
  at runtime) in a crate whose stated premise is minimal, auditable unsafe.

The crate does NOT currently state a minimum Windows version anywhere
(`Cargo.toml` has `rust-version` but no platform floor). That policy
question must be settled FIRST — before any implementation can be
designed — because the entire trade-off calculus (GetProcAddress
complexity vs. dropping old Windows) depends on it.

**Separate BSD spec-read-only note (NOT the main focus):** FreeBSD/DragonFly/NetBSD
provide `MAP_ALIGNED(n)` (n = log2 alignment, shifted into the flags word) which
would turn `try_reserve_aligned_exact` (the Unix exact-size fast path) into a
GUARANTEED hit on those targets, eliminating the over-reserve fallback entirely
(3 syscalls on a miss → 1 always). All four BSDs are already in the crate's
`MAP_ANON` cfg list (`lib.rs:2080-2094`) but NONE is in CI, so this really is
spec-reading only, with zero measurement possible in this repo's current CI
matrix. This is a different mechanism, different platform — do not conflate it
with the Windows opportunity above.

**This is a design-note documenting an open opportunity, NOT an implementation
proposal.** R32-13 already rejected the "step 3" of this on a latency basis
(4.6 % materiality). The address-space axis was never evaluated separately,
and this note exists so a future decision can be made with the full trade-off
on the table (VA-space win vs. GetProcAddress unsafe surface vs. Windows-version
floor policy). No measured numbers are cited anywhere — this is entirely
reasoned from the Win32/BSD API specifications.

---

## 1. Current Windows `align > 64 KiB` implementation — the inefficiency to fix

### 1.1 The two-call path (crates/vmem/src/lib.rs:1497-1589)

For `align > 64 KiB`, the current code takes a deliberately over-reserved
region and finds an aligned window inside it:

```text
let over = size.checked_add(align)?;  // over-reserve by alignment
let region = VirtualAlloc(NULL, over, MEM_RESERVE, ...);  // syscall 1
let aligned_offset = align_up(region_addr, align) - region_addr;
let base = region + aligned_offset;
VirtualAlloc(base, commit_len, MEM_COMMIT, ...);  // syscall 2
// base .. base+size is the aligned, committed span
```

Key properties:
* **2 syscalls** per reserve+commit lifecycle (one `MEM_RESERVE`, one `MEM_COMMIT`)
* **`size + align` VA held** for `size` bytes of usable span (over-reserve needed to
  find an aligned window)
* For `align=4MiB, size=4MiB`: reserves 8 MiB VA to deliver 4 MiB aligned

### 1.2 Why this path exists at all

Task #848 added the align <= 64 KiB fast path: `MEM_RESERVE | MEM_COMMIT` on a
`size`-rounded-up region satisfies the alignment contract by construction when
`align <= 64 KiB` (the Windows `VirtualAlloc` alignment guarantee). For larger
alignments, no such guarantee exists, so the over-reserve + alignment-finding
dance is required — this is the best `VirtualAlloc` alone can do.

The over-reserve is honest: you cannot know in advance whether a randomly-
chosen base will be aligned, so you must reserve a span that is guaranteed to
contain at least one aligned window.

### 1.3 Measured context (R32-13, already-existing data)

`docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE.md` measured the
Windows reservation path cost:
* Median `MEM_RESERVE` ≈ **4 580 ns**
* Median `MEM_COMMIT` ≈ **9 133 ns**
* Reserve+commit pair ≈ **13.7 µs**
* This is **4.3–4.8 %** of the Windows segment lifecycle overall

R32-13's rejection of "step 3" (VirtualAlloc2) was on a latency basis: removing
the separate reserve call would cut ~4.6 µs from ~13.7 µs — a ~33 % cut of the
reserve+commit cost, but the reserve+commit cost itself is only ~4.3–4.8 % of
the overall lifecycle, so the wall-clock win would be minimal at the allocator
level.

**This note's focus:** the VA-space axis (2× over-reserve for 4 MiB aligned
segments), which R32-13 did NOT evaluate. On 64-bit, VA is not scarce enough to
justify a Win10-1803+ API floor purely on this axis — but the trade-off must be
stated explicitly so a future decision can be made with both axes (latency and
VA-space) on the table.

---

## 2. VirtualAlloc2 opportunity — single syscall, exact VA span

### 2.1 The API (Windows 10 1803+ / Server 2016+)

```text
LPVOID VirtualAlloc2(
    PVOID                                   Unused,
    PVOID                                   BaseAddress,
    SIZE_T                                  Size,
    ULONG                                   AllocationType,
    ULONG                                   PageProtection,
    MEM_EXTENDED_PARAMETER *ExtendedParameters,
    ULONG                                   ParameterCount
);
```

The key is the `ExtendedParameters` array. One parameter can have type
`MemExtendedParameterAddressRequirements`, pointing at:

```c
typedef struct MEM_ADDRESS_REQUIREMENTS {
    PVOID LowestStartingAddress;
    PVOID HighestEndingAddress;
    SIZE_T Alignment;
} MEM_ADDRESS_REQUIREMENTS;
```

`Alignment` can be **any power of two >= 64 KiB** (the spec does NOT restrict
it to 64 KiB). A single call returns a base aligned to `Alignment` with NO
over-reserve — the OS finds the aligned window internally.

### 2.2 The win (in principle, reasoned from spec)

For `align > 64 KiB`:
* **Syscalls:** 2 → 1 (no separate reserve+commit dance; `AllocationType` can
  combine `MEM_RESERVE | MEM_COMMIT`)
* **Virtual address space:** `size + align` → `size` (no over-reserve; OS
  reserves exactly what you ask for, aligned)

For `align=4MiB, size=4MiB`:
* Current: 8 MiB VA held, 2 syscalls
* With VirtualAlloc2: 4 MiB VA held, 1 syscall

**This is reasoned from the API specification, NOT measured.** No prototype
exists in this repo; mimalloc resolves and uses exactly this API for the same
reason (VA-space efficiency for large alignments), which is a useful precedent
for both the shape and the resolution strategy — but mimalloc's code is not a
substitute for an in-repo measurement.

### 2.3 Availability floor (the honest blocker)

VirtualAlloc2 is exported from `kernelbase.dll`, NOT `kernel32.dll`. The crate's
current three Win32 declarations (`lib.rs:1679-1688`) all resolve at link time
against `kernel32.dll`:

```text
#[link(name = "kernel32")]
extern "system" {
    fn winapi_virtual_reserve(...)
    fn winapi_virtual_commit(...)
    fn winapi_virtual_release(...)
}
```

`kernelbase.dll` exists on Windows 10 1803+ (April 2018) and Server 2016+.
On Windows 10 1709 and earlier, VirtualAlloc2 is **not available** — attempting
to link against it would fail at load time, with no opportunity for fallback.

The crate does NOT currently state a minimum Windows version anywhere.
`Cargo.toml` has `rust-version = "1.83"` but no platform floor. This policy
question must be settled BEFORE any implementation: does the crate drop
pre-1803 Windows support, or does it maintain backward compatibility?

### 2.4 Two implementation paths, both with real trade-offs

#### Path (a): Link-time dependency on kernelbase.dll

Add:

```text
#[link(name = "kernelbase")]
extern "system" {
    fn VirtualAlloc2(...);
}
```

**Pros:**
* Zero runtime overhead — same cost as the existing kernel32.dll calls
* Minimal unsafe surface (function pointer resolved at link time, not runtime)

**Cons:**
* **Silently drops pre-1803 Windows support** — the binary simply won't load on
  older Windows, with no graceful degradation
* No fallback path — the two-call reserve is discarded entirely
* This is a **breaking change in platform support** that must be stated in the
  crate's README and rustdoc, and it permanently narrows the platform matrix

#### Path (b): GetProcAddress-based dynamic resolution

Add:

```text
use std::sync::OnceLock;

static VIRTUAL_ALLOC2: OnceLock<Option<unsafe fn(...) -> *mut c_void>> = OnceLock::new();

fn get_virtualalloc2() -> Option<unsafe fn(...) -> *mut c_void> {
    VIRTUAL_ALLOC2.get_or_init(|| {
        let kernelbase = unsafe { LoadLibraryA(b"kernelbase.dll\0".as_ptr() as *const i8) };
        if kernelbase.is_null() {
            return None;
        }
        let fnptr = unsafe { GetProcAddress(kernelbase, b"VirtualAlloc2\0".as_ptr() as *const i8) };
        if fnptr.is_null() {
            return None;
        }
        Some(unsafe { std::mem::transmute(fnptr) })
    }).copied()
}
```

Then, at reserve time:

```text
if let Some(virtualalloc2) = get_virtualalloc2() {
    // Use VirtualAlloc2 with MEM_EXTENDED_PARAMETER_ADDRESS_REQUIREMENTS
} else {
    // Fall back to existing two-call path
}
```

**Pros:**
* **Maintains backward compatibility** — pre-1803 Windows gets the two-call path,
  newer Windows gets the optimized path
* Graceful degradation — no breaking change in platform support

**Cons:**
* **Real NEW unsafe surface** — function-pointer FFI resolved at runtime, with
  type-safety only as good as the transmute correctness
* Runtime cost (one-time `LoadLibraryA` + `GetProcAddress` via `OnceLock`)
* The `OnceLock<Option<fn(...)>>` pattern is well-tested in the ecosystem, but it
  is still additional complexity and an additional auditing surface in a crate
  whose stated premise is minimal, auditable unsafe

### 2.5 The open decision — Windows-version-floor policy

The fundamental blocker is **not technical** — VirtualAlloc2's API is clear and
the implementation paths above are both technically feasible. The blocker is
**policy**: the crate does not currently state a minimum Windows version, so
it is unclear whether dropping pre-1803 Windows support is acceptable.

This decision cannot be made in isolation. Questions a maintainer must answer
before any implementation:

1. **What is the crate's target platform floor?** Is "Windows 10 1803+" an
   acceptable minimum, or must pre-1803 Windows (10 1709 and earlier, Windows 8.1,
   Windows 7) remain supported?
2. **If backward compatibility is required, is the GetProcAddress complexity
   acceptable given the crate's "minimal, auditable unsafe" premise?** The
   unsafe surface grows from "static FFI declarations in extern blocks" to
   "runtime-resolved function pointers via transmute" — a real new category of
   unsafety.
3. **Is the VA-space win (2× for 4 MiB aligned segments) worth the trade-off on
   either axis?** On 64-bit, VA is not scarce — the win is principled, not urgent.
   On 32-bit, the win is more material, but the crate's own bench harness
   (`benches/vmem_bench.rs`) has no 32-bit CI arm, so this cannot be measured
   in-repo today.

**This note exists so a future decision can be made with the full trade-off on
the table.** R32-13's latency rejection (4.6 % materiality) is one axis; the
VA-space win (2× for flagship use case) is the other; the policy question
(Windows-version-floor) is the gate. No implementation should proceed until
the policy question is settled — this note records the opportunity and the
trade-offs so the policy decision can be made with all information available.

---

## 3. Separate BSD spec-read-only note — MAP_ALIGNED(n)

### 3.1 The flag (FreeBSD/DragonFly/NetBSD)

FreeBSD, DragonFly, and NetBSD provide `MAP_ALIGNED(n)` in the `mmap` flags
word. The parameter `n` is the log2 of the desired alignment, shifted into
the flags:

```c
#define MAP_ALIGNED(n)  ((n) << 24)  // n = log2(alignment)
```

Example: `MAP_ALIGNED(22)` requests 2^22 = 4 MiB alignment. The OS guarantees
the returned base is aligned to the requested power-of-two boundary, with NO
over-reserve.

### 3.2 The win for `try_reserve_aligned_exact` (in principle, reasoned from spec)

The Unix path already has `try_reserve_aligned_exact` (exact-size fast path):
call `mmap` with `MAP_ANON` and check whether the base happens to be aligned.
If aligned: **1 syscall**, `size` VA. If not aligned: fall back to the
over-reserve path (reserve `size + align`, find aligned window, release the
unused tail) — **3 syscalls**, `size + align` VA.

With `MAP_ALIGNED(n)` available, `try_reserve_aligned_exact` becomes a
GUARANTEED hit on FreeBSD/DragonFly/NetBSD:
* **1 syscall always** (no fallback, no retry)
* **`size` VA always** (no over-reserve)

The Unix path would match the Windows VirtualAlloc2 ideal on those targets.

### 3.3 Availability and CI gap

All four BSDs are already in the crate's `MAP_ANON` cfg list
(`lib.rs:2080-2094`):
* FreeBSD (via `MAP_ANON` detection)
* DragonFlyBSD
* NetBSD
* OpenBSD (does NOT have `MAP_ALIGNED` in the same form; OpenBSD's alignment
  guarantee is via `MAP_ALIGNED_SUPER`, a different mechanism with different
  semantics)

**However, NONE of the BSDs are in this repo's CI matrix.** The crate's CI
(`ci.yml`) tests on Windows and Linux only. There is zero in-repo measurement
possible for the BSD path, so this really is spec-reading only — no measured
hit-rate, no measured wall-clock win, no measured RSS/commit delta.

### 3.4 Why this is separate from the Windows opportunity

Different mechanism, different platform:
* **Windows:** `VirtualAlloc2` + `MEM_EXTENDED_PARAMETER_ADDRESS_REQUIREMENTS`
  — a newer API (Win10 1803+) replacing an older two-call path.
* **BSD:** `MAP_ALIGNED(n)` — a flag in the existing `mmap` API, available on
  all three BSDs that carry it (no version-floor question, just platform
  availability).

Do not conflate the two opportunities. This section is a spec-read-only note
("this flag exists and would help if we ever add BSD CI"), NOT an implementation
proposal. The Windows opportunity is the one with a real policy question and a
real trade-off surface; the BSD note is contextual information only.

---

## 4. Summary — what this note delivers, and what it does NOT deliver

### 4.1 What this note delivers

1. **Documentation of the Windows VirtualAlloc2 opportunity** — single syscall,
   exact VA span, with the API specification and the two implementation paths
   (link-time vs GetProcAddress).
2. **Statement of the open policy decision** — Windows-version-floor must be
   settled before any implementation, because the trade-off calculus depends
   entirely on whether pre-1803 Windows support is maintained.
3. **BSD MAP_ALIGNED spec-read-only note** — contextual information only, not
   an implementation proposal, documenting a different mechanism on a different
   platform.
4. **No measured numbers** — this is entirely reasoned from the Win32/BSD API
   specifications, with explicit disclaimers where "in principle" or "expected"
   is used instead of "measured."

### 4.2 What this note does NOT deliver

1. **No implementation** — no `crates/vmem/src/`, `Cargo.toml`, or `tests/`
   changes. This is a design-note documenting an open opportunity, not a code
   change.
2. **No measured speedup claim** — there are no measured numbers to cite. R32-13
   measured the reserve+commit cost (~4.3–4.8 % of the Windows segment lifecycle),
   but the VA-space win (2× for 4 MiB aligned segments) has never been measured
   in-repo because there is no VA-pressure measurement harness.
3. **No recommendation on the policy question** — this note does NOT say
   "drop pre-1803 Windows" or "maintain backward compatibility at all costs."
   That decision belongs to the crate maintainers, based on their target
   platform matrix and their risk tolerance for new unsafe surface.
4. **No BSD implementation proposal** — the BSD note is spec-reading only,
   because there is no BSD CI to measure against. This is contextual information,
   not a "add BSD CI and implement MAP_ALIGNED" proposal.

### 4.3 When to revisit this

A future round should revisit this note when:
* The crate's maintainers settle the Windows-version-floor policy (either by
  documenting a minimum Windows version in the README/rustdoc, or by explicitly
  deciding that backward compatibility is required), OR
* The crate adds BSD CI to its matrix, at which point the BSD MAP_ALIGNED
  opportunity can be measured and evaluated with real data (not just
  spec-reading).

Until then, this note stands as the complete record of the opportunity and the
trade-offs — so a future decision can be made with all information available,
not just the latency axis R32-13 evaluated.

---

**Status:** OPEN — gated on Windows-version-floor policy. No action until
that policy question is settled by the crate maintainers.