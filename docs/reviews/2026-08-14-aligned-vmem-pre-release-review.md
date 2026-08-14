# `aligned-vmem` — independent pre-release review (2026-08-14)

**Scope:** `crates/vmem/` (crate name `aligned-vmem`), read from scratch as
first-look due diligence ahead of a 0.2.0 release. No prior review of this
crate was consulted while forming the findings below; `docs/CORRECTNESS_OPEN_ITEMS.md`
and `docs/perf/OPEN_ITEMS.md` were consulted only at the end, to mark which
findings are already tracked.

**Method:** full read of `src/lib.rs` (2793 lines), `src/error.rs`,
`src/mock.rs`, `src/fault_injection.rs`, `Cargo.toml`, all seven integration
test files, the bench, the example, and `README.md`; plus `cargo check` /
`cargo clippy` / `cargo test` runs across several feature combinations on the
host (Windows x86_64) to ground specific findings.

---

## Executive summary

**Verdict: ship 0.2.0 — after four cheap fixes and one conscious decision.**
Nothing found here blocks a release on safety grounds.

This is a genuinely well-built crate. The soundness bar is met: I walked every
`unsafe` site individually (Area 9) and found **no UB, no memory-unsafety, no
double-free, no provenance violation, and no wrong FFI signature**. The
strict-provenance discipline is real, not aspirational. Failure paths release
what they allocated. `errno`/`GetLastError` capture timing is correct at every
site. The documentation is unusually honest — non-portable constants carry
explicit REASONED-FROM-SPEC vs. VERIFIED annotations, known-unfixed hazards are
named rather than hidden, and at least two tests carry doc comments explaining
that an *earlier version of themselves* was vacuous and how that was fixed and
verified. That is a level of self-scrutiny I rarely see.

What I would fix before publishing:

1. **V-1 (HIGH, performance/doc).** The Unix exact-size `mmap` fast path is now
   a **net syscall loss at every hit rate below 100 %**. Its justifying comment
   cites a 50 % break-even that task #842 eliminated when it removed the
   over-reserve path's trim `munmap`s — the comparison is now 3 syscalls on a
   miss versus 1 for going straight to over-reserve. At the crate's own measured
   hit rates this is 87 %–131 % more syscall traffic on the hottest entry point.
   The *code* change is a judgement call (the fast path still buys address-space
   economy, which matters on 32-bit). The *comment* fix is mandatory: it is the
   only recorded justification for the path and it is arithmetically wrong. The
   same stale reasoning has propagated into `docs/perf/OPEN_ITEMS.md` item 46's
   "still a net win" verdict.
2. **V-7 (MEDIUM, doc drift).** Four public doc sites — including the README and
   `Reservation::is_huge`'s rustdoc — state that the Windows two-call path "does
   not support `MEM_LARGE_PAGES` at all". The code passes `MEM_LARGE_PAGES` on
   that path and would report `is_huge() == true` if Windows ever accepted it.
   One-line fix that removes a wasted syscall and an unenforced invariant at the
   same time.
3. **V-6 + V-25 (MEDIUM, defensive gap + the coverage hole that hides it).** The
   `MEM_LARGE_PAGES` retry inside the Windows single-call fast path returns
   without the alignment check the surrounding code goes out of its way to make
   unconditional and release-compiled. And **no test anywhere calls
   `reserve_aligned_huge` with `align <= 64 KiB`**, so that entire branch is
   unexercised on every platform. One test would cover both.
4. **V-11 + V-20 (MEDIUM, small but semver-relevant).** `try_reserve_aligned`
   classifies the same top-of-range `size` differently on Unix and Windows
   (`invalid_argument` vs. an OS code), violating both its own doc and the
   principle an existing test exists to defend; and a target that is neither
   `unix` nor `windows` fails with a bare "cannot find function
   `reserve_aligned_raw`" rather than the attributable `compile_error!` the
   crate already added one level down for unenumerated Unix targets.

And one decision rather than a defect: **V-22** — `mock` is a non-additive,
backend-replacing Cargo feature whose own manifest says the window to convert it
to a `--cfg` flag closes the moment 0.2.0 publishes. It is documented
impeccably, but documentation cannot defuse a footgun whose failure mode is
silent and whose trigger lives in someone else's manifest. Settle it
deliberately, either way, before the button is pressed.

Beyond that: 17 LOW findings (mostly API-freeze ergonomics — `as_ptr` returning
`*mut`, a `ReservationParts` type nobody can construct, no accessor for a live
reservation's `align`, a README example nothing compiles) and 16 INFO entries,
of which several are explicit **null results** where I looked hard and found
nothing wrong. Six findings are already tracked in this repository's own
open-items indexes and are marked as such.

**Counts:** 0 CRITICAL · 1 HIGH · 5 MEDIUM · 17 LOW · 16 INFO.

---

## Severity scale used here

| Rating | Meaning |
|---|---|
| CRITICAL | Memory-unsafety / UB reachable from safe code, or silent data loss on a supported target. |
| HIGH | Wrong result, contract violation, or leak on a supported target; or a released-API decision that cannot be undone without a semver-major bump. |
| MEDIUM | Real defect with a bounded blast radius (one feature, one platform, one doc claim that misleads a consumer into a wrong design). |
| LOW | Inconsistency, defensive-check gap, or ergonomics problem with no currently reachable failure. |
| INFO | Observation, null result, or a note for the maintainer's judgement — no action strictly required. |

---

## Area 1 — Unix reservation path (`unix_reserve`, `try_reserve_aligned_exact`)

Files: `crates/vmem/src/lib.rs:1977-2211`, `:2586-2634`.

### V-1 — HIGH (performance) — the Unix "exact-size mmap fast path" is now a **net syscall loss at every hit rate below 100 %**, and the doc that justifies it still cites a break-even that no longer exists

`crates/vmem/src/lib.rs:2046-2050` (the fast-path attempt), `:2051-2082` (the
over-reserve fallback), `:2147-2211` (the fast path itself), and the design
rationale at `:176-182`.

The rationale comment at `:179-182` says:

> The survey (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` F11) computed this
> fast path is a net syscall **LOSS below a 50 % hit rate** …

That 50 % break-even was computed against the **old** over-reserve path, which
mapped `size + align` and then issued **two trimming `munmap` calls** (head and
tail). Task #842 (cited at `:2107-2112`: "Keep the entire over-reserve mapping
as the reservation … This removes the `munmap` trim calls") deleted those trims.
Recount the syscalls against the code as it exists today:

| Path taken | Syscalls issued |
|---|---|
| Fast path **hit** (`:2148-2210`) | 1 (`mmap`) |
| Fast path **miss** (`:2194-2198` unmaps, then `:2058-2082` maps again) | 3 (`mmap` + `munmap` + `mmap`) |
| **If the fast path did not exist** — go straight to `:2051-2082` | 1 (`mmap`), unconditionally |

Expected cost with the fast path is `p·1 + (1-p)·3 = 3 - 2p`, versus a flat `1`
without it. `3 - 2p > 1` for every `p < 1`. **The fast path can therefore never
win on syscall count; it only breaks even in the impossible case of a 100 % hit
rate.** Under the crate's own measured hit rates (`:882-885`: 34.4 % at 64 KiB
align, 46.7 % at 1 MiB, 56.7 % at 4 MiB) the reserve path issues **1.87–2.31
syscalls** where **1** would do — i.e. 87 % to 131 % more `mmap`/`munmap`
traffic than the alternative, on the hottest entry point in the crate.

Concrete scenario: on Linux/x86_64, `reserve_aligned(64*1024, 64*1024)` in a
loop. Measured hit rate 34.4 %, so ~65.6 % of calls do `mmap(64 KiB)` →
`munmap(64 KiB)` → `mmap(128 KiB)`, three syscalls plus two extra VMA
insert/split operations in the kernel, to produce a result the single
`mmap(128 KiB)` would have produced alone.

What the fast path *does* still buy is **address-space economy**: a hit holds
`size` bytes of VA instead of `size + align`. On 64-bit that is close to
worthless (the crate's own comment at `:2112` says "Cost: up to `align` bytes of
untouched VA held for the reservation's lifetime (no RSS)"), but on the 32-bit
Unix targets this crate explicitly supports (`i686-unknown-linux-gnu`,
verified to compile; see the `OffT` 32-bit arm at `:2553-2559`) doubling the VA
footprint of every 4 MiB segment is a real constraint.

Recommendation (pick one, before the API/perf story locks in at 0.2.0):
1. Delete the fast path on 64-bit targets and keep it only under
   `target_pointer_width = "32"`, where VA is scarce; or
2. keep it but stop paying for the miss — pass the *over-reserve* length to the
   first `mmap` and only `munmap` the excess when it happens to be aligned
   (i.e. merge the two paths); or
3. at minimum, **fix the stale rationale comment at `:179-182`** so the next
   reader is not told there is a 50 % break-even that the trim removal
   eliminated.

I regard (3) as mandatory regardless of which of (1)/(2) is chosen: the comment
is the only recorded justification for a code path that measurably costs
syscalls, and it is now arithmetically wrong.

### V-2 — LOW — `unix_reserve`'s huge-page retry loses the `MADV_HUGEPAGE` distinction, and applies the THP hint to a mapping that already failed `MAP_HUGETLB`

`crates/vmem/src/lib.rs:2062-2081` and `:2113-2118`.

When `huge == true` and `libc_mmap(over, true)` fails, the code retries with
`libc_mmap(over, false)` and sets `granted_huge = false` (`:2073`) — correct. But
`:2113-2118` then issues `libc_madvise_hugepage(base, size)` unconditionally
whenever `huge` is set, including on that ordinary-page fallback mapping. That
is harmless (`MADV_HUGEPAGE` is a hint whose errors are discarded at `:2675`),
but it is one wasted `madvise(2)` syscall on the exact path that has already
been told huge pages are unavailable. It is also arguably *wrong intent*: the
caller asked for `MAP_HUGETLB` explicitly-sized huge pages, and the fallback
silently substitutes transparent huge pages, which have materially different
latency and fragmentation characteristics — while `is_huge()` reports `false`,
so the caller cannot tell that THP was requested on its behalf.

### V-3 — LOW — `HUGE_SUPPORTED && huge` in `try_reserve_aligned_exact` reports a *request*, not an observed grant, on one narrow path

`crates/vmem/src/lib.rs:2208-2210`.

`try_reserve_aligned_exact` returns `granted_huge = HUGE_SUPPORTED && huge`. On
Linux with `huge-pages`, `HUGE_SUPPORTED` is the compile-time constant `true`
(`:2437`), so this is really "the caller asked for huge and the `mmap`
succeeded". Because `libc_mmap` sets `MAP_HUGETLB | MAP_HUGE_2MB` and the kernel
fails the whole call when 2 MiB hugetlb pages are unavailable, "the `mmap`
succeeded" does imply the grant — the reasoning is sound, but it is *implicit*.
A one-line comment tying `granted_huge` to `MAP_HUGETLB`'s all-or-nothing
failure mode would make this auditable; today the reader has to reconstruct it
from three separate sites (`:2079`, `:2210`, `:2593-2596`).

### V-4 — INFO — `decommit_lazy` leaves free BSD reclaim on the table

`crates/vmem/src/lib.rs:2304-2317` (`madv_free_advice`) and `:2467-2474`.

`MADV_FREE` is defined only for `target_os = "linux"` (`:2471`) and
`MADV_FREE_REUSABLE` only for macOS/iOS (`:2474`); every other Unix — including
FreeBSD (`MADV_FREE` = 5), NetBSD (6), OpenBSD (6), DragonFly (5), all four of
which are in the crate's own supported `MAP_ANON` list at `:2348-2362` — falls
back to `MADV_DONTNEED`. That is *correct* (the doc at `:2292-2298` says so
plainly), just not the cheap path the function's name advertises. Adding three
`#[cfg]` arms would make `decommit_lazy` actually lazy on the BSDs. Flagging as
INFO, not a defect, because the behaviour is accurately documented and the
constants would be REASONED-FROM-SPEC like the rest of the BSD table.

### V-5 — INFO (null result) — the Unix `unsafe` blocks and provenance discipline check out

I specifically looked for, and did **not** find, problems in:

- **Provenance.** `:2086` / `:2106` use `.addr()` + `region_ptr.with_addr(...)`,
  so `base` inherits `region_ptr`'s provenance over the whole `over`-byte
  mapping. `:2126` casts `region_ptr` directly (not via `base`), so the
  reservation pointer also carries full-mapping provenance. Both are what
  strict-provenance requires; the README's claim at `README.md:171-177` holds
  for the Unix half.
- **Leak on every early-return.** Each failure exit that occurs *after* a
  successful `mmap` unmaps first: `:2099` (fit-computation failure),
  `:2196` (fast-path alignment miss). The paths that return before any
  successful map (`:2071`, `:2076`, `:2161`) have nothing to clean up. I traced
  all six exits; none leaks and none double-unmaps.
- **`errno` capture timing.** Every `VmemError::last_os_error()` on this path
  (`:2071`, `:2076`, `:2161`) is issued before any cleanup FFI, matching the
  timing contract stated at `error.rs:98-101`. The two *non*-OS failures
  (`:2100`, `:2197`) correctly return `invalid_argument()` rather than reading a
  stale `errno`.
- **`MAP_FAILED` handling.** `:2612` compares `p.addr() == usize::MAX` rather
  than casting; correct for every supported target, where `MAP_FAILED` is
  `(void*)-1`.
- **FFI signatures.** `mmap`/`munmap`/`madvise`/`sysconf` at `:2572-2584` match
  the real ABI, including the conditional `OffT` width at `:2553-2569`. I
  compiled `i686-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`,
  `aarch64-unknown-linux-gnu`, `x86_64-unknown-freebsd` and
  `x86_64-unknown-netbsd` with `--features lazy-commit,huge-pages,fault-injection,bench-internals`;
  all five compile clean.

---

## Area 2 — Windows reservation path (`win_reserve_commit`)

File: `crates/vmem/src/lib.rs:1577-1970`.

### V-6 — MEDIUM — the huge-page retry inside the single-call fast path **skips the alignment check** that the same function deliberately makes unconditional 20 lines later

`crates/vmem/src/lib.rs:1652-1671` vs. `:1677-1701`.

`:1677-1688` carries an extended comment (task #917) explaining why the
alignment check at `:1689` is a *real runtime check and not a `debug_assert!`*:
`WIN_ALLOCATION_GRANULARITY = 65536` is REASONED-FROM-SPEC, never verified at
the point of use, and "release builds are exactly where an unverified constant
matters".

The `extra_commit_flags != 0` retry branch at `:1658-1668` bypasses that check
entirely:

```rust
let plain = VirtualAlloc(null_mut(), commit_len, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
match NonNull::new(plain as *mut u8) {
    Some(n) => {
        …
        return Ok((n, n, commit_len, false)); // <-- no is_multiple_of(align) check
    }
```

Concrete scenario: `huge-pages` enabled, `reserve_aligned_huge(2*1024*1024,
65536)` on a host without `SeLockMemoryPrivilege`. The `MEM_LARGE_PAGES` call at
`:1643` returns NULL, the retry at `:1658` succeeds, and the function returns a
base that has **never been checked against `align`**. If the premise the task
#917 comment distrusts were ever false, `Reservation::as_ptr()`'s documented
alignment guarantee (`:509-511`) would be silently violated on exactly this
path — no error, no diagnostic, the same failure the unconditional check was
added to prevent, just on the branch nobody looked at.

The fix is one line: route the retry's `n` through the same `if
!n.as_ptr().addr().is_multiple_of(align)` fall-through as the main path.

### V-7 — MEDIUM (doc drift) — the public docs say the Windows two-call path "does not support `MEM_LARGE_PAGES` at all"; the code passes `MEM_LARGE_PAGES` on that path and can set `is_huge() == true` from it

Public claims: `crates/vmem/src/lib.rs:490-491` (`Reservation::granted_huge`
field doc), `:598-600` (`Reservation::is_huge` rustdoc), `:1484-1485`
(`reserve_aligned_huge` rustdoc), and `README.md:62-68`. All four state, without
qualification:

> For `align > 64 KiB` the two-call path is used, which **does not support
> `MEM_LARGE_PAGES` at all**.

The code does the opposite. `reserve_aligned_huge_raw` (`:1881`) passes
`MEM_LARGE_PAGES` as `extra_commit_flags` regardless of `align`; the two-call
path then ORs it into the commit at `:1758` (`MEM_COMMIT | extra_commit_flags`)
and, on success, returns `granted_huge = extra_commit_flags != 0` at `:1794` —
i.e. **`true`**. The internal comment at `:1789-1793` is honest about this
("uses the requested flag, not the observed grant … a documented-but-not-enforced
invariant"), but that honesty lives in a private comment while four public
surfaces state the opposite as fact.

Two separate problems fall out:

1. **Correctness of `is_huge()`.** The claim that this cannot happen rests on
   Windows always rejecting `MEM_COMMIT | MEM_LARGE_PAGES` into a pre-reserved
   region. That is the documented behaviour today, but it is an *unverified
   assumption about a foreign OS*, and if it is ever wrong, `is_huge()` returns
   `true` for a reservation that got ordinary pages — and a consumer that keys
   its decommit policy off `is_huge()` (which the crate's own docs at
   `:1487-1495` tell it to do!) would then decommit on a region it was told not
   to.
2. **A wasted syscall on every call.** `reserve_aligned_huge(size, align)` with
   `align > 64 KiB` on Windows always issues a `MEM_LARGE_PAGES` commit that is
   expected to fail (`:1754-1761`), then a plain retry (`:1766-1768`). That is a
   guaranteed-wasted syscall per reservation on a path whose own docs say the
   flag "is not supported at all".

Cheapest fix: don't pass `extra_commit_flags` down the two-call path at all
(`:1758` → `MEM_COMMIT`), and set `granted_huge = false` there. That makes the
code match all four public doc sites, removes the wasted syscall, and removes
the unenforced invariant in one edit.

### V-8 — LOW — `winapi_virtual_release` discards `VirtualFree`'s `BOOL` with no explanatory comment, unlike its Unix twin

`crates/vmem/src/lib.rs:1965-1970` vs. `:2620-2634`.

`libc_munmap` carries a 12-line comment (task #719) explaining that the discard
is *deliberate*, why a failure would indicate a bug in this crate's own
bookkeeping rather than a recoverable condition, and that the failure mode is a
leak and never unsafety. `winapi_virtual_release` — which is the Windows
`Drop`-path release, i.e. the single most consequential FFI call in the crate —
discards `VirtualFree`'s `BOOL` return with no such note at all. Same for
`winapi_virtual_decommit` at `:1957-1963`, whose failure is *known* to be
reachable (the huge-page case documented at `:1093`). This is a documentation /
consistency gap between the two platform halves, not a behavioural defect.

### V-9 — LOW — `reservation_len` under-reports on Windows in a way that makes `from_raw_parts` round-tripping subtly wrong

`crates/vmem/src/lib.rs:551-578` (the doc), `:1700` (the value produced).

The single-call fast path returns `reservation_len = commit_len = size`. Windows
rounds the underlying VA reservation up to 64 KiB, so `reserve_aligned(4096,
4096)` reports `reservation_len() == 4096` while consuming 64 KiB. The doc at
`:553-573` is admirably explicit that this is not a portable measure — good.

But the crate also ships `Reservation::from_raw_parts`, whose `# Safety`
contract (`:705`) requires `reservation_len` to be "the full size of the OS
reservation". A consumer following the documented "cross-crate handoff"
pattern — take a reservation apart with `into_parts()`, store the triple, rebuild
with `from_raw_parts` — round-trips a `reservation_len` that the crate's *own*
docs say is not the full size. It happens to be harmless (Windows
`VirtualFree(ptr, 0, MEM_RELEASE)` ignores the length), but the two doc sites
contradict each other about what `reservation_len` means. Worth reconciling
before the API is frozen: either weaken `from_raw_parts`'s wording to "the value
this crate would report" or state that the field is advisory on Windows.

### V-10 — INFO (null result) — the Windows FFI surface is correct

Checked and found fine:

- **`SYSTEM_INFO` layout** (`:1897-1911`). Field order, types and `#[repr(C)]`
  match the Win32 `SYSTEM_INFO` exactly, including `DWORD_PTR
  dwActiveProcessorMask` → `usize` and the 4-byte `dwOemId` union head modelled
  as `w_processor_architecture: u16` + `w_reserved: u16`. On x86_64 this yields
  the correct 48-byte layout with `dwAllocationGranularity` at offset 40.
- **Calling convention.** `extern "system"` at `:1885` — correct for
  `VirtualAlloc`/`VirtualFree`/`GetSystemInfo` on both `x86_64-pc-windows-msvc`
  and `i686-pc-windows-msvc` (where it maps to `stdcall`). A plain `extern "C"`
  here would have been a real 32-bit bug; it is not made.
- **Constant values.** `MEM_COMMIT` 0x1000, `MEM_RESERVE` 0x2000, `MEM_DECOMMIT`
  0x4000, `MEM_RELEASE` 0x8000, `MEM_LARGE_PAGES` 0x2000_0000, `PAGE_READWRITE`
  0x04 (`:1934-1949`) — all correct.
- **Leak-freedom on the two-call path.** Every failure exit after the
  `MEM_RESERVE` succeeds releases the region first: `:1743` (fit failure),
  `:1778` (large-page retry also failed), `:1784` (plain commit failed). The
  fall-through at `:1693` also releases before retrying via the two-call path.
  No exit leaks; none double-frees.
- **`GetLastError` capture timing.** `:1776` and `:1782` both capture before the
  cleanup `VirtualFree`, exactly as `error.rs:98-101` requires; `:1744` correctly
  returns `invalid_argument()` for the non-OS fit failure instead of reading a
  stale code.


---

## Area 3 — Public API surface, `Reservation`, and contract validation

Files: `crates/vmem/src/lib.rs:449-1052`, `:1525-1570`.

### V-11 — MEDIUM — `try_reserve_aligned` returns **different error kinds on Unix and Windows** for the same well-formed input, and violates its own "without touching the OS" doc on Unix

`crates/vmem/src/lib.rs:895-902` + `:971-975` (the doc), `:2051-2053` (Unix),
`:1706-1708` / `:1673` (Windows).

`validate_size_align` (`:897-902`) accepts any `size` that is non-zero and a
`PAGE` multiple, so `size = 0xFFFF_FFFF_FFFF_F000` (the largest legal value)
with `align = PAGE` passes validation. Then:

- **Unix** (`:2046-2053`): `try_reserve_aligned_exact` runs first and issues a
  real `mmap` (which fails, `ENOMEM`); the code discards that error, falls
  through, and `size.checked_add(align)` overflows →
  `Err(VmemError::invalid_argument())`.
- **Windows** (`:1636`, `:1673`): `align <= 64 KiB && commit_len == size` takes
  the single-call path, `VirtualAlloc` fails, →
  `Err(VmemError::from_os_code(ERROR_NOT_ENOUGH_MEMORY | ERROR_COMMITMENT_LIMIT))`.

So `try_reserve_aligned(0xFFFF_FFFF_FFFF_F000, 4096).unwrap_err().is_invalid_argument()`
is **`true` on Linux and `false` on Windows** for byte-identical input. Two
consequences:

1. The doc at `:895-896` and `:974-975` promises "A contract violation (bad
   `size`/`align`) returns `VmemError::invalid_argument` **without touching the
   OS**". On Unix this path touches the OS (one failed `mmap`) *before*
   producing `invalid_argument`.
2. `crates/vmem/tests/smoke.rs:967-990`
   (`try_reserve_huge_size_is_a_genuine_os_refusal_not_invalid_argument`) asserts
   exactly the principle this breaks — "a well-formed (if absurd) size/align
   must never be classified as a caller contract violation" — but tests it only
   at `1 << 46`, which does not overflow. The invariant the test exists to
   defend is violated at the top of the legal `size` range, on one platform, and
   no test covers it.

Fix: hoist the `size.checked_add(align)` overflow check into
`validate_size_align` so it is rejected identically on every platform *before*
any syscall, or classify the overflow as `os_refusal_unknown_code()` instead of
`invalid_argument()`.

### V-12 — LOW — `from_raw_parts`'s `saturating_sub` silently accepts `base < reservation`, and the comment claiming exhaustive cheap checks is (again) an overclaim

`crates/vmem/src/lib.rs:767-791`, specifically `:769`.

```rust
let offset = base_addr.saturating_sub(res_addr);
```

The documented contract (`:701-705`) requires `reservation <= base` — that is
what makes `reservation_len >= len + (base - reservation)` meaningful. With
`saturating_sub`, a caller passing `base` *below* `reservation` gets
`offset == 0`, the sufficiency check degenerates to `reservation_len >= len`,
and the call is accepted. Concrete: on a `2 * PAGE` reservation at `res`,
`from_raw_parts(res.wrapping_sub(PAGE), PAGE, res, 2 * PAGE, PAGE)` constructs
successfully today; the resulting `Reservation` reports an `as_ptr()` that lies
entirely outside its own reservation.

This matters because the comment immediately above (`:756-766`, task #916) says:

> All three are now checked explicitly below, leaving only the genuinely
> uncheckable invariants (pointer validity, liveness, exclusivity) as unchecked
> caller responsibilities.

`base_addr >= res_addr` is neither uncheckable nor expensive — it is one
comparison, and the code already computes both addresses. This is the same class
of overclaiming comment that task #916 was created to fix, reproduced one
commit later. Add `base_addr >= res_addr` to the `assert!` and switch `:769` to
a plain subtraction (or `checked_sub`).

### V-13 — LOW — `ReservationParts` is a type nobody can construct, and its own rustdoc says the use case it exists for cannot be realised

`crates/vmem/src/lib.rs:825-859`.

The doc at `:832-837` states plainly:

> **Note:** there is no public constructor for this type… The motivating use
> case ("self-hosted metadata") **cannot currently be realized** because you
> need to *construct* a `ReservationParts` from already-saved fields, not just
> destructure an existing reservation.

That is an accurate self-assessment, and it means the typed alternative to
`into_parts`/`release` — the one the README (`README.md:43-45`) and three
rustdoc sites steer new code toward — **does not actually serve its stated
purpose**. A consumer that stores the reservation in its own metadata still has
to fall back to the raw tuple, which is exactly the footgun `ReservationParts`
was added to remove.

Because the type is `#[non_exhaustive]` with all-public fields, adding a
`ReservationParts::new(ptr, len, align)` later is a purely additive change — so
this is not a semver trap. But shipping 0.2.0 with a documented dead end in the
API is a poor first impression for a crate whose pitch is "a small focused
tool". Either add the constructor now (three lines) or drop the type until it
does something.

### V-14 — LOW — `leak_zeroed_pages` rounds with `PAGE`, not `page_size()`, and on Windows burns 64 KiB of VA per 4 KiB sidecar

`crates/vmem/src/lib.rs:1546-1570`.

- `:1550` rounds `size` up to a `PAGE` (4 KiB) multiple, then reserves with
  `align = PAGE`. On a 16 KiB-page host the OS rounds further; the returned
  pointer is valid for *more* than the promised span, so this is conservative
  and safe — but it means `leak_zeroed_pages(4096)` on Apple Silicon wastes
  12 KiB, and on Windows the single-call fast path reserves a full 64 KiB
  granule (see V-9) for the same 4 KiB request. For a pre-`main` bookkeeping
  sidecar this is likely irrelevant; for a caller that leaks many small
  sidecars it is a 4×–16× VA multiplier that the doc does not mention.
- No functional defect — flagged as a documentation/efficiency note.

### V-15 — INFO (null result) — RAII / double-free discipline is clean

I traced every path that can release a reservation and found no double-free or
leak:

- `Drop` (`:803-817`) is the only automatic release. Both `into_parts`
  (`:627-631`) and `into_reservation_parts` (`:646-660`) `mem::forget(self)`
  **after** copying the fields out — correct ordering, and
  `into_reservation_parts` carries an explicit comment (`:652-657`) explaining
  why the `forget` is load-bearing.
- `leak_zeroed_pages` (`:1566`) `mem::forget`s deliberately.
- `release` (`:1014-1026`) null-checks and forwards; `release_parts`
  (`:1044-1052`) destructures and delegates to `release`, so there is exactly
  one release implementation.
- `Reservation` is `Send` (`:823`) and, because `NonNull<u8>` is not `Sync`,
  automatically `!Sync` — matching the doc at `:461-463`. `tests/smoke.rs:37-38`
  pins the `Send` half with a `const _: () = assert_send::<Reservation>();`.

### V-16 — INFO (null result) — the `VmemError` design is sound

`crates/vmem/src/error.rs` in full. The three-state model
(`invalid_argument` / `from_os_code(Some)` / `os_refusal_unknown_code`) with
`code: Option<u32>` genuinely closes the "`Some(0)` means two different things"
ambiguity its own doc describes (`:22-27`), and
`tests/smoke.rs:908-954` pins all three states pairwise including the `Display`
output. The `From<VmemError> for std::io::Error` bridge (`:138-148`) maps each
state to a distinct `io::Error` shape and is covered by three tests in
`tests/vmemerror_io_bridge.rs`. Two cosmetic nits, neither worth a finding of
its own: `os_code(&self)` / `is_invalid_argument(&self)` take `&self` on a
`Copy` type, and `VmemError` implements neither `Hash` nor `PartialOrd`
(harmless, but `Hash` is cheap and occasionally wanted for error dedup).

---

## Area 4 — Decommit / recommit / commit_range

Files: `crates/vmem/src/lib.rs:1058-1368`, `:1805-1851`, `:2220-2270`.

### V-17 — INFO (null result, with one documented asymmetry) — the validation-base split is deliberate and correctly documented

`decommit`/`decommit_lazy` validate offsets against **`page_size()`**
(`:1118-1121`, `:1186-1189`) and silently return on a violation;
`recommit`/`try_recommit`/`commit_range`/`try_commit_range` validate against
**`PAGE`** (`:1245`, `:1335`) and *reject* with `false`/`Err`. I initially read
this as a bug, and it is not: `README.md:96-106` states both halves and the
rationale (a `()` return carries no write-permitting sentinel to misuse; a
`bool`/`Result` one does — and clamping a violation to the write-permitting
value already caused a real crash, per `:1222-1226`).

The residual asymmetry is real but benign: on a 16 KiB-page host,
`decommit(base, PAGE, 2*PAGE)` is silently skipped while
`recommit(base, PAGE, 2*PAGE)` returns `true`. That cannot corrupt anything
(the Unix `recommit_pages_impl` is a no-op, `:2242-2261`; on Windows
`page_size() == PAGE == 4096` always, so the two bases coincide). Two tests —
`tests/smoke.rs:559-632` and `tests/mock.rs:255-330` — even carry
`if page_size() > PAGE` arms specifically to discriminate the two bases on the
Apple-Silicon CI runner. This is unusually careful work; I found nothing wrong
with it.

### V-18 — INFO (already tracked) — `decommit` is a silent no-op on Darwin and on huge-page reservations

`crates/vmem/src/lib.rs:1090-1116` and `:2247-2259`. Both divergences are
documented at length in the rustdoc, in `README.md:145-164`, and the Darwin half
is tracked as `docs/CORRECTNESS_OPEN_ITEMS.md` item 48. No fix is implemented;
the honest interim state is well signposted. **Already tracked** — reported here
only for completeness, per the review brief.

One observation the existing item does not make: `decommit`'s rustdoc
(`:1090-1099`) tells the caller "Use `reserve_aligned` instead if you need
decommit functionality" for huge pages, and `is_huge()` is the documented way to
detect the case — but `Reservation::from_raw_parts` unconditionally sets
`granted_huge: false` (`:798`), so an adopted huge reservation reports
`is_huge() == false` and the caller's guard silently fails open. That is
documented at `:602-605`, but the interaction between the two docs is not, and
it is the one place where a caller following both pieces of advice still gets
the broken behaviour.

### V-19 — LOW — `try_recommit` / `try_commit_range` accept a *misaligned* empty range

`crates/vmem/src/lib.rs:1242-1247` and `:1332-1337`.

The `start == end` early return runs **before** the alignment check, so
`try_recommit(base, 5, 5)` returns `Ok(())` while `try_recommit(base, 5, 4096)`
returns `Err(invalid_argument())`. This is harmless (nothing is committed either
way) and arguably matches the doc's "a genuinely empty range (`start == end`) is
a no-op returning `true`" (`:1279-1280`). But it means the one input class the
crate treats as "well-formed but misaligned" silently passes, which slightly
weakens the task #712 story that misaligned offsets are always rejected.
Reordering the two checks would cost nothing.

---

## Area 5 — Features, cfg-gating, `mock`, `fault-injection`

Files: `crates/vmem/Cargo.toml`, `crates/vmem/src/lib.rs:91-139`,
`crates/vmem/src/mock.rs`, `crates/vmem/src/fault_injection.rs`.

### V-20 — MEDIUM — no target-support `compile_error!` for non-Unix, non-Windows targets; the crate fails with a bare `cannot find function reserve_aligned_raw`

`crates/vmem/src/lib.rs:1578` / `:1978` / `:2695` (the only three
`reserve_aligned_raw` definitions; verified by grep) and `:1798` / `:2214` /
`:2711` (`release_reservation`). Their cfg arms are exactly
`all(windows, not(miri))`, `all(unix, not(miri))`, and `miri`.

Task #918 added a `compile_error!` (`:2379-2400`) for `cfg(unix)` targets
outside the enumerated `MAP_ANON` list, precisely so an unsupported target
produces an attributable diagnostic instead of `error[E0425]: cannot find value
MAP_ANON`. The identical gap remains one level up: a target that is **neither**
`unix` nor `windows` and has `std` — e.g. `wasm32-wasip1` (`target_os = "wasi"`,
`target_family = "wasm"`), or `x86_64-fortanix-unknown-sgx` — matches none of
the three arms and fails with a bare `cannot find function
reserve_aligned_raw in this scope`, times six symbols.

This is the same "fails closed, but unattributably" shape task #918 judged worth
fixing, and the fix is the same three lines. Rated MEDIUM rather than LOW only
because a published crate's first contact with a new consumer is often exactly
this: someone adds it to a workspace that also builds a wasm target.

### V-21 — LOW — the crate-wide `allow(dead_code)` that the module doc says was eliminated still exists for one feature combination

`crates/vmem/src/lib.rs:96-125`.

The module comment is explicit that a crate-wide `allow(dead_code)` "made the
whole crate structurally unable to report ANY unused item under `--all-features`
(task #646/F8)" and was therefore narrowed to per-item
`#[cfg_attr(feature = "mock", allow(dead_code))]`. Then `:122-125` reintroduces
exactly that construct at crate level, conditioned on
`all(feature = "fault-injection", not(feature = "lazy-commit"))`.

Under `--features fault-injection` (no `lazy-commit`) — a legal combination the
weekly `cargo-hack` feature-powerset job exercises — the crate is again
structurally unable to report any unused item. The single item it exists for is
`fault_injection::should_fail_commit` (`src/fault_injection.rs:118`), which
already carries a per-item `#[cfg_attr(feature = "mock", allow(dead_code))]` at
`:117`; extending that attribute's condition is a one-line change that restores
the per-item discipline the doc claims. Verified locally: `cargo clippy
--features fault-injection` and `--features fault-injection,lazy-commit` are
both clean, so nothing is being hidden *today* — the objection is structural.

### V-22 — INFO (design decision worth settling before publish) — `mock` is a non-additive, backend-replacing Cargo feature, and its own docs say the window to fix that closes when 0.2.0 ships

`crates/vmem/Cargo.toml:62-87`, `crates/vmem/src/mock.rs:25-42`.

The hazard is documented thoroughly and honestly: Cargo unifies features across
the whole graph, so any downstream `dev-dependency` enabling `aligned-vmem/mock`
silently replaces the commit/decommit/recommit backend for *every* consumer in
that build, with no compile error. The manifest's own text names the stronger
fix (a `--cfg` flag, matching this repo's `cfg(loom)`/`cfg(kani)` precedent),
records the deferral, and states the deadline:

> Removing or converting `mock` is therefore still free today and stays free
> only until 0.2.0 ships (task #658)… **settle the `--cfg` question before 0.2.0
> publishes if it is going to be settled at all.**

This is the single largest *irreversible* decision in this release. It is not a
defect and I am not overriding the maintainer's judgement — but the brief asks
what would make this crate safer to depend on, and my honest answer is: a
backend-replacing Cargo feature on a crate whose whole job is "be the one place
that talks to the OS" is a footgun documentation cannot fully defuse, because
the failure mode is *silent* and the trigger is *not in the affected crate's own
manifest*. **Already tracked** as `docs/CORRECTNESS_OPEN_ITEMS.md` item 42
(recorded there as deferred/closed); flagged here because the deadline the
manifest itself sets is now.

### V-23 — LOW — `arm_fail_next` stores `Relaxed` while `arm_fail_at` was deliberately upgraded to `Release`

`crates/vmem/src/fault_injection.rs:86` vs. `:108`.

The module doc (`:32-45`) explains at length why `arm_fail_at`'s
counter-reset-then-target-store needs a `Release`/`Acquire` pair for cross-thread
arming. `arm_fail_next` (`:85-87`) uses a bare `Relaxed` store. That is
*defensible* — `FAIL_NEXT` has no payload to publish, so there is nothing for a
`Release` to order — but the module doc says "this module does NOT assume the
arming and committing thread are the same", and a reader comparing the two
functions will reasonably ask why one is `Release` and the other is not. One
sentence in `arm_fail_next`'s doc ("no payload, so `Relaxed` suffices") closes
it.

The third, larger hazard — a concurrent `arm_fail_at` racing
`should_fail_commit`'s self-disarm — is already named and scoped out at
`:47-57`. That is exactly the right way to record a known-unfixed race; no
complaint.

### V-24 — INFO (null result) — the `mock` recorder and the `docs.rs` feature selection are correct

- `Call` (`src/mock.rs:61-141`) carries `#[non_exhaustive]` at **both** the enum
  and every variant level, with the reasoning recorded at `:50-60`. This is the
  right call and is made before first publish, which is the only time it can be.
- Thread-local state (`:201-208`) with `const { … }` initialisers; `reset`
  (`:221-225`) clears the log *and* both fault counters, so tests can isolate.
- `[package.metadata.docs.rs] features = ["lazy-commit", "huge-pages",
  "fault-injection"]` (`Cargo.toml:26-28`) deliberately excludes `mock` and
  `bench-internals` rather than using `all-features`, with the reasoning at
  `:15-25`. Correct — `all-features` would render the backend-replacing mock as
  if it were ordinary reference API.
- `docsrs` cfg gating (`#![cfg_attr(docsrs, feature(doc_cfg))]` at `:93` plus
  `#[cfg_attr(docsrs, doc(cfg(...)))]` on every gated item) is applied
  consistently; I checked every `pub` item behind a feature and found none
  missing the attribute.
- `alloc-lazy-commit = ["lazy-commit"]` (`Cargo.toml:44`) is a pure alias with a
  documented one-release deprecation window. Correct handling of a rename.

---

## Area 6 — Test coverage: what is actually exercised vs. only claimed

Files: `crates/vmem/tests/*.rs` (2,181 lines across seven files), read in full.

Overall the suite is **well above average for a crate this size**: 8 negative
tests on `from_raw_parts` alone, a genuinely non-vacuous concurrency oracle
(`tests/fault_injection.rs:197-271`, whose doc comment documents that a *prior*
version of the same oracle was mathematically incapable of failing and how it
was fixed and verified by reverting the fix), and per-test `#[cfg]` scoping that
is explicit about which platform each real-OS assertion is a property of. The
gaps below are real but narrow.

### V-25 — MEDIUM — **no test anywhere calls `reserve_aligned_huge` with `align <= 64 KiB`**, so the entire Windows single-call large-page branch — including the V-6 defect — is unexercised

Verified by grep over `tests/`, `benches/`, `examples/`: every
`reserve_aligned_huge` / `try_reserve_aligned_huge` call site uses `align` of
2 MiB or 4 MiB (`tests/huge_pages.rs:50`, `:80`, `:97`, `:115`, `:136`;
`tests/mock.rs:172`, `:176`). On Windows every one of those takes the
**two-call** path (`align > WIN_ALLOCATION_GRANULARITY`).

Consequence: `win_reserve_commit`'s single-call branch with
`extra_commit_flags != 0` — `lib.rs:1636-1701`, which contains the
alignment-check gap reported as V-6 and the `MEM_LARGE_PAGES` retry at
`:1658-1668` — has **zero test coverage on any platform**. One test
(`reserve_aligned_huge(64 * 1024, 64 * 1024)`, asserting `!is_huge()` on a host
without `SeLockMemoryPrivilege`, plus base alignment and writability) would
cover it and would be green on the existing Windows CI runner.

### V-26 — LOW — `align > size` is never tested, on any platform

Every reservation in the suite uses `align == size` or `align < size`. But
`align > size` is legal under the documented contract (`align` a power of two
`>= PAGE`; `size` a non-zero `PAGE` multiple — nothing relates them), and it is
the case that most stresses the over-reserve arithmetic: `over = size + align`
is then dominated by `align`, and `align_up_addr(region_addr, align)` can land
far from `region_addr`. Concretely untested: `reserve_aligned(PAGE, 4 * MIB)`.
On Unix that is a guaranteed fast-path miss followed by a 4 MiB + 4 KiB
over-reserve; on Windows it is the two-call path with a large head offset. Both
should work by inspection — but nothing proves it.

### V-27 — LOW — the crate's central structural invariant (`reservation ⊇ usable span`) is asserted for *caller-supplied* values but never for the crate's *own* output

`Reservation::from_raw_parts` hard-asserts
`reservation_len >= len + (base - reservation)` (`lib.rs:778-780`) for values a
caller hands in. No test asserts the same relation for a `Reservation` the crate
itself produced. The closest is `tests/smoke.rs:129` (`parts.len >= 4 * MIB`),
which checks only the length, not the containment.

A three-line assertion — `reservation_ptr() <= as_ptr()` and
`as_ptr().addr() + len() <= reservation_ptr().addr() + reservation_len()` —
added to the existing `reserve_is_aligned_and_writable` test would pin the
invariant on every platform and every path (Unix fast-path hit, Unix
over-reserve, Windows single-call, Windows two-call), and would have caught any
regression in the `with_addr` arithmetic at `lib.rs:1750` / `:2106`.

### V-28 — LOW (already tracked) — the Windows `bench-internals` reserve-path counters are exercised by nothing

`WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` / `_TWO_CALL_PAIRS`
(`lib.rs:228-252`, accessors `:307-330`) have no test or example reading them —
confirmed by grep. They are the natural path-activation oracle for
`win_reserve_commit`'s `align <= 64 KiB && commit_len == size` dispatch, whose
`==` half was a real shipped bug (task #848), currently regression-tested only
*indirectly* via `reservation_len()` (`tests/lazy_commit.rs:71-117`,
`tests/smoke.rs:107-118`). **Already tracked** as
`docs/CORRECTNESS_OPEN_ITEMS.md` item 50 (U10 half). Reported here because it
compounds V-25: neither the direct oracle nor a large-page test covers the fast
path's huge-page branch.

### V-29 — INFO — two tests are tautological or self-declared non-guards

- `tests/min_page.rs:8-10` (`min_page_equals_page`) asserts `MIN_PAGE == PAGE`
  where `pub const MIN_PAGE: usize = PAGE;` (`lib.rs:164`) — the compiler
  guarantees it. It cannot fail. Its sibling `min_page_is_4kib` is fine.
- `tests/smoke.rs:88-95` (`ordinary_reservation_never_reports_huge`) — its own
  doc comment (`:71-87`) states it "is unconditionally true on every
  platform/feature combo and cannot fail against a regression on this path — it
  is NOT a W2 regression guard", and correctly names the test that *is*. Keeping
  a documented-vacuous test is defensible as a smoke check; I note it only
  because the suite's own honesty here is the reason I could tell.

### V-30 — INFO (already tracked) — `decommit_lazy_roundtrip` is effect-blind on every platform

`tests/smoke.rs:394-411` writes, calls `decommit_lazy`, `recommit`s, writes
again, and reads back. That sequence passes whether `madvise` succeeded, failed,
or was never issued. **Already tracked** as `docs/CORRECTNESS_OPEN_ITEMS.md`
item 48's "S4 remainder" bullet, which also names the fix (the `bench-internals`
`unix_madvise_*` counters already exist and are used exactly this way in the
macOS-gated test at `:472-531`; the blocker is that no CI row runs
`bench-internals` against the real Unix backend on Linux).

### V-31 — INFO — small untested corners, listed for completeness, none rated

`release(null, …)`'s early return (`lib.rs:1015-1018`);
`ReservationParts`'s derived `PartialEq`/`Eq`; the deprecated
`Reservation::is_empty` (`:538-540`); `leak_zeroed_pages` with an exact-multiple
size (only `3 * PAGE + 7` is tested, `tests/smoke.rs:637`); the
`try_reserve_aligned` `size + align` overflow case (see V-11 — this one is the
only member of the list with a real defect behind it).

---

## Area 7 — Performance in the real hot paths

Scope, as the brief requires: the actual reserve / commit / decommit / release
paths only — not the bench harness, not `mock`, not the `bench-internals`
counters.

### V-1 (restated) — the single material finding

The Unix exact-size fast path is the one place where the crate demonstrably does
more work than the platform requires — see Area 1. It is worth ~0.87–1.31 extra
syscalls per `reserve_aligned` call on Linux at the crate's own measured hit
rates. Everything else below is either already minimal or a marginal win.

**Interaction with an existing tracked item.** `docs/perf/OPEN_ITEMS.md` item 46
(`[L]` tier, "R-V20-849 — Unix exact-reserve hit rate") records the measured hit
rates and concludes:

> The ~57% hit rate means the exact-mmap fast path is **very likely still a net
> win** even for large aligns on this platform+kernel, so the V20/P17
> align-threshold guard's premise does not hold here.

That verdict is derived from the 50 % break-even in
`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md:1208-1213`, whose own
arithmetic is explicit: *"a miss costs 1 (`mmap`) + 1 (`munmap`) + the
fallback's 1 + up to 2 = 5, versus 3 for going straight to over-reserve."* The
"up to 2" are the head/tail trim `munmap`s that task #842 removed. With the
trims gone the comparison is **3 versus 1**, and the break-even moves from 50 %
to 100 %. So item 46's "still a net win" conclusion is stale in the same way
the source comment at `lib.rs:179-182` is — **already tracked, but with a
verdict that the code has since invalidated.** Interestingly item 46's own S12
sub-note *does* count the current cost correctly ("still costs 3 syscalls
(`mmap(size)` → `munmap` → `mmap(size + align)`)") — it just never compares that
3 against the 1-syscall no-fast-path baseline, only against a proposed
hint-retry alternative. Nobody has redone the baseline comparison since #842.

### V-32 — LOW (opportunity) — the Windows lazy-commit path over-reserves `size + align` even when `align <= 64 KiB`, where no over-reserve is needed

`crates/vmem/src/lib.rs:1636` (the fast-path guard) and `:1706-1708`.

`reserve_aligned_lazy(size, PAGE, initial)` on Windows has `commit_len != size`,
so it correctly bypasses the single-call fast path and lands on the two-call
path, which unconditionally reserves `over = size + align`. But
`VirtualAlloc(NULL, size, MEM_RESERVE, …)` *already* returns a base aligned to
the 64 KiB allocation granularity, so for `align <= 64 KiB` the over-reserve
buys nothing: `align_up_addr(region_addr, align) == region_addr` by
construction.

Available shape, structurally identical to the existing fast path (allocate →
check alignment → release and fall through on a miss):

```text
if align <= WIN_ALLOCATION_GRANULARITY {
    region = VirtualAlloc(NULL, size, MEM_RESERVE, PAGE_READWRITE)
    if region % align == 0 { over = size; ... }   // commit commit_len at region
    else { VirtualFree(region, 0, MEM_RELEASE); fall through to size+align }
}
```

Still two syscalls (no worse), but saves `align` bytes of VA per lazy
reservation and — more usefully — makes `reservation_len()` report the truth on
that path. The caveat that keeps this at LOW: the fall-through arm is required
(otherwise a wrong `WIN_ALLOCATION_GRANULARITY` turns a working reservation into
an `invalid_argument` error), which is extra code for a saving that is
negligible on 64-bit. Worth doing only if the lazy path matters to a real
consumer.

### V-33 — INFO (null result) — everything else in the hot paths is already minimal

I checked each of the following and found no avoidable work:

- **Windows eager reserve** (`:1636-1701`): 1 syscall
  (`VirtualAlloc(MEM_RESERVE|MEM_COMMIT)`) for `align <= 64 KiB`; 2 otherwise.
  That is the floor — Windows cannot reserve-and-partially-commit in one call
  (the `commit_len == size` guard at `:1636` and its `// confirmed concretely`
  note at `:1626-1635` get this exactly right), and cannot request >64 KiB
  alignment without over-reserving.
- **`decommit` / `decommit_lazy`**: exactly one `madvise` (`:2234-2235`) or one
  `VirtualFree(MEM_DECOMMIT)` (`:1816`). No pre/post syscalls.
- **`recommit`**: exactly one `VirtualAlloc(MEM_COMMIT)` on Windows
  (`:1828-1835`), a pure `Ok(())` on Unix (`:2242-2261`). Correct — there is
  genuinely nothing to do on Linux.
- **`release`**: exactly one `VirtualFree(0, MEM_RELEASE)` (`:1969`) or one
  `munmap` (`:2217`).
- **`page_size()`** (`:385-407`): the steady-state path is a single `Relaxed`
  load plus a branch. Called twice per `decommit` pair at most. Nothing to gain.
- **Inlining.** `Reservation`'s accessors, `ReservationParts::as_tuple`,
  `VmemError`'s constructors/accessors and `madv_free_advice` all carry
  `#[inline]`. The un-annotated private helpers (`validate_size_align`,
  `finish_reservation`, `align_up_addr`, `libc_mmap`) are crate-internal and
  trivially inlinable by LLVM within a codegen unit; adding `#[inline]` to them
  would change nothing. **I looked for an inlining-worthy miss and did not find
  one.**
- **Atomics on the hot path.** With `bench-internals` off (the default), the
  counters are not compiled at all — `#[cfg(feature = "bench-internals")]` gates
  the statics *and* every increment (`:203-204`, `:2154`, `:2200`, `:1667`,
  `:2660-2666`). Verified by reading each site; the zero-cost claim in
  `Cargo.toml:125-127` holds.

---

## Area 8 — Documentation accuracy, API ergonomics, and the 0.2.0 freeze

### V-34 — LOW — the README's only code example is never compiled by anything

`crates/vmem/README.md:19-34` is a ` ```rust ` fence. `src/lib.rs` does **not**
`#![doc = include_str!("../README.md")]` (verified by grep: no `include_str!`
anywhere in `crates/vmem/src/`), and no CI step runs a README doctest. So the
example that every crates.io visitor sees first is unverified and free to rot.

The crate's own module doc handles this correctly — `lib.rs:54-70` uses a
` ```text ` fence and adds "Runnable form: `tests/smoke.rs`" (`:72`), which is
the project's no-doctest convention applied properly. The README should get the
same treatment: either point it at the same runnable test, or add a
`tests/readme_example.rs` that is the example verbatim. (Note that
`#![doc = include_str!("../README.md")]` is *not* the right fix here — it would
turn the README into a doctest, which this repo's conventions forbid.)

I checked the example by hand and it is currently correct.

### V-35 — LOW — `as_ptr()` returns `*mut u8`, breaking the std naming convention right before the API freezes

`crates/vmem/src/lib.rs:513-515`.

Throughout `std`, `as_ptr` yields `*const T` and `as_mut_ptr` yields `*mut T`
(`Vec`, `slice`, `str`, `CStr`, …). `Reservation::as_ptr(&self) -> *mut u8`
inverts that: a shared `&self` hands out a mutable raw pointer under the
`as_ptr` name. It is not unsound (raw pointers carry no borrow obligation, and
the span is genuinely exclusively owned), and it is convenient for the
allocator use case — but it is exactly the kind of thing that is free to change
today and semver-major after publish. Options: rename to `as_mut_ptr`, or keep
`as_ptr` and document explicitly why it returns `*mut`.

### V-36 — LOW — there is no way to read a live `Reservation`'s `align`

`crates/vmem/src/lib.rs:508-610` — the accessor set is `as_ptr`, `len`,
`is_empty` (deprecated), `reservation_ptr`, `reservation_len`, `is_huge`. The
`align` field (`:472`) is readable only by *consuming* the handle
(`into_parts` / `into_reservation_parts`) or by parsing the `Debug` output
(`:503`). A consumer holding a `Reservation` and wanting to know its alignment —
e.g. to decide whether a sub-span is suitably aligned — has no non-destructive
route.

This is purely additive to fix (`pub const fn align(&self) -> usize`), so it is
not a semver trap, but it is a small hole in an otherwise complete accessor set
and costs three lines.

### V-37 — INFO — `release`'s null-pointer behaviour is implemented but undocumented

`crates/vmem/src/lib.rs:1014-1018` silently returns when `reservation` is null.
That is a sensible defensive choice, but `release`'s `# Safety` section
(`:1006-1013`) does not mention it, so a caller cannot rely on it — and the mock
recorder is also skipped in that case (`:1019-1023` runs after the early
return), which would silently desync a `mock`-based test's expected call log.
One sentence in the doc closes it.

### V-38 — INFO (null result) — documentation quality is, on the whole, unusually high

Things I actively tried to falsify and could not:

- Every `unsafe` block in `src/lib.rs` carries a `// SAFETY:` comment. I checked
  all of them; none is missing, and none of the ones I checked is wrong.
- `cargo doc --no-deps` with
  `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D warnings"`
  across `lazy-commit,huge-pages,fault-injection,mock,bench-internals` builds
  clean — no broken links, and `#![deny(missing_docs)]` (`:92`) is satisfied.
- Every non-portable magic constant carries a REASONED-FROM-SPEC vs.
  VERIFIED-ON-HARDWARE annotation, including the MIPS `MAP_ANON`/`MAP_HUGETLB`
  divergence (`:2325-2345`), the four-BSD `_SC_PAGESIZE` table
  (`:2478-2533`), the `MAP_HUGE_2MB` encoding (`:2411-2430`), and the 32-bit
  `off_t` classification (`:2535-2569`). I spot-checked the Linux, Darwin and
  FreeBSD values against my own knowledge of the headers and found no errors.
- `README.md` and the module doc agree with each other and with `Cargo.toml`'s
  `description` on the reservation strategy (exact-size Unix fast path /
  over-reserve on miss / Windows single-call under 64 KiB) — three copies of the
  same paragraph, all currently in sync. **The one place they do not agree with
  the code is V-7's `MEM_LARGE_PAGES` claim.**
- CI coverage is broad: `.github/workflows/ci.yml` runs `aligned-vmem` on Linux
  (`:148-171`, three clippy rows + `--all-features` test + a `--cfg miri`
  *check*), Windows (`:781-793`, both the real-feature set and `--all-features`),
  macOS (`:815-828`, same pair), a `fault-injection lazy-commit` row (`:920`),
  and a weekly `cargo hack check --feature-powerset --depth 2` (`:2039-2040`).
  All 48 tests pass locally on Windows with
  `--features lazy-commit,huge-pages,fault-injection,bench-internals`.
- Residual CI gap: no `cargo miri test -p aligned-vmem` (only a compile check).
  **Already tracked** as `docs/CORRECTNESS_OPEN_ITEMS.md` item 41.

### V-39 — INFO — `Cargo.toml`'s `description` is ~600 characters of implementation detail

`crates/vmem/Cargo.toml:7`. crates.io renders this as the one-line summary in
search results and on the crate page header. The current text —
"…(exact-size mmap fast path on Unix; on fast-path miss or Windows with align >
64 KiB, over-reserve size+align and keep the full mapping; Windows with align <=
64 KiB uses single-call fast path with no over-reserve)…" — is accurate but
unreadable at that position, and it also *encodes the reservation strategy in
the package metadata*, which means every future change to that strategy (see
V-1) is a manifest edit too. The README and rustdoc already carry the detail.
A one-sentence description would serve the reader better.

---

## Area 9 — Soundness: the explicit null result

This is the section the brief asks me to be honest about, so I will be
specific about what I checked rather than just asserting a conclusion.

**I found no memory-unsafety, no UB, and no soundness hole in this crate.**
Not "no obvious one" — I walked every `unsafe` site individually. What I
checked:

1. **Every `unsafe` block and `unsafe fn`.** All carry a `// SAFETY:` comment;
   I evaluated each against its callers rather than taking the comment's word
   for it. The Unix half (`:2033-2317`, `:2586-2682`), the Windows half
   (`:1577-1970`), and the miri fallback (`:2694-2764`).
2. **Every `NonNull::new_unchecked`** — `:1750`, `:2106`, `:2126`, `:2202`,
   `:1569`. Each is preceded either by an explicit null check on the source
   pointer or by address arithmetic that provably cannot produce 0 (`align_up`
   only increases a non-zero address). None can construct a null `NonNull`.
3. **Pointer arithmetic and underflow.** `decommit_pages_impl` (`:1810`,
   `:2225`) and `recommit_pages_impl` (`:1823`) compute `end - start`.
   `decommit`/`decommit_lazy` reject `start >= end` before dispatch
   (`:1119`, `:1187`); `try_recommit`/`try_commit_range` return early on
   `start == end` and reject `start > end` (`:1242-1246`, `:1332-1336`). No
   caller can reach a subtraction underflow.
4. **Provenance.** No exposed-address round-trip exists on any reservation
   path: `:1729-1750` and `:2086-2106` both use `.addr()` + `.with_addr()`, so
   the derived `base` carries the parent mapping's provenance over the whole
   reservation, and `:2126` casts the region pointer directly. The `mock`
   recorder stores `usize` addresses obtained via `.addr()` and never casts
   them back. The README's claim at `:171-177` is accurate.
5. **Double-free / leak.** Traced every early return that occurs after a
   successful map/reserve: Unix `:2099`, `:2196`; Windows `:1693`, `:1743`,
   `:1778`, `:1784`. Each releases exactly the region it created, before any
   handle escapes. `Drop` + `into_parts`/`into_reservation_parts`'s
   `mem::forget` ordering is correct (V-15).
6. **FFI signatures.** Unix `mmap`/`munmap`/`madvise`/`sysconf` with the
   conditional `OffT` width (`:2553-2584`); Windows `VirtualAlloc`/
   `VirtualFree`/`GetSystemInfo` as `extern "system"` with a byte-correct
   `#[repr(C)] SYSTEM_INFO` (`:1885-1911`). Verified by compiling five Unix
   targets, including a 32-bit one, plus the Windows host (V-5, V-10).
7. **`debug_assert!` guarding an invariant that matters in release.** Two
   sites. `align_up_addr`'s power-of-two assert (`:2790`) is backed by a real
   runtime check in `validate_size_align` (`:898`) on every public path, so the
   invariant is enforced in release by something else. `query_os_page_size`'s
   allocation-granularity assert (`:427-433`) *is* release-compiled-out, but
   the crate knows it: the note at `:434-439` says so explicitly, and the real
   protection is the unconditional runtime alignment check at `:1689` — which
   the code (`:1685-1688`) justifies in exactly these terms, citing this
   repository's own rule that `debug_assert!` compiles out of `--release`.
   **The one gap in that reasoning is V-6**, where the retry branch skips that
   unconditional check.
8. **Data races.** `PAGE_SIZE_CACHE` (`:168`) is a benign
   compute-twice-store-same-value race. `fault_injection`'s three atomics carry
   a documented ordering argument plus an explicitly scoped-out third hazard
   (`fault_injection.rs:47-57`). `mock`'s state is thread-local.
   `Reservation: Send + !Sync` is correct.

The one thing I could **not** verify by execution: this host is Windows, so
every Unix `unsafe` site was reviewed by reading and cross-compiling, not by
running. The repository's CI covers Linux and macOS execution; my
Unix conclusions above are code-reading conclusions.

---

## Findings index

| ID | Sev | Area | One line |
|---|---|---|---|
| V-1 | **HIGH** | Unix reserve | Exact-size fast path is a net syscall loss at any hit rate < 100 % since #842 removed the trim `munmap`s; the 50 % break-even comment is stale |
| V-6 | MEDIUM | Windows reserve | Huge-page retry inside the single-call fast path skips the deliberately-unconditional alignment check |
| V-7 | MEDIUM | Windows reserve | Four public doc sites say the two-call path "does not support `MEM_LARGE_PAGES` at all"; the code passes it and can set `is_huge() == true` from it |
| V-11 | MEDIUM | API | `try_reserve_aligned` returns `invalid_argument` on Unix and an OS code on Windows for the same top-of-range `size`; violates its own "without touching the OS" doc |
| V-20 | MEDIUM | cfg | No `compile_error!` for non-Unix/non-Windows targets — bare `cannot find function reserve_aligned_raw` |
| V-25 | MEDIUM | Tests | No test calls `reserve_aligned_huge` with `align <= 64 KiB`; the whole Windows single-call large-page branch (incl. V-6) is unexercised |
| V-2 | LOW | Unix reserve | THP hint applied to the ordinary-page fallback that already failed `MAP_HUGETLB` |
| V-3 | LOW | Unix reserve | `granted_huge` correctness is implicit in `MAP_HUGETLB`'s all-or-nothing failure; undocumented |
| V-8 | LOW | Windows | `VirtualFree`'s `BOOL` discarded with no comment, unlike `libc_munmap`'s 12-line justification |
| V-9 | LOW | API | `reservation_len` under-reports on Windows in a way that conflicts with `from_raw_parts`'s own contract wording |
| V-12 | LOW | API | `from_raw_parts`'s `saturating_sub` silently accepts `base < reservation`; the "all cheap checks done" comment overclaims |
| V-13 | LOW | API | `ReservationParts` has no public constructor; its own doc says the use case it exists for cannot be realised |
| V-14 | LOW | API | `leak_zeroed_pages` rounds with `PAGE`, not `page_size()`; 64 KiB VA per 4 KiB sidecar on Windows |
| V-19 | LOW | commit | `try_recommit(base, 5, 5)` returns `Ok` — the empty-range early return precedes the alignment check |
| V-21 | LOW | cfg | Crate-wide `allow(dead_code)` still present for `fault-injection` without `lazy-commit` |
| V-23 | LOW | fault-inj | `arm_fail_next` is `Relaxed` while `arm_fail_at` is `Release`; the asymmetry is correct but unexplained |
| V-26 | LOW | Tests | `align > size` never tested |
| V-27 | LOW | Tests | Reservation-containment invariant asserted for caller input, never for the crate's own output |
| V-28 | LOW | Tests | Windows `bench-internals` counters exercised by nothing (**already tracked**, item 50/U10) |
| V-32 | LOW | Perf | Windows lazy path over-reserves `size + align` where `align <= 64 KiB` needs no over-reserve |
| V-34 | LOW | Docs | README's only code example is compiled by nothing |
| V-35 | LOW | API | `as_ptr(&self) -> *mut u8` inverts the std `as_ptr`/`as_mut_ptr` convention, right before the freeze |
| V-36 | LOW | API | No non-destructive accessor for a live `Reservation`'s `align` |
| V-4, V-5, V-10, V-15, V-16, V-17, V-18, V-22, V-24, V-29, V-30, V-31, V-33, V-37, V-38, V-39 | INFO | — | Null results, already-tracked items, and notes — see the sections above |

### Cross-references to already-tracked items

| Finding | Tracked as |
|---|---|
| V-1 (verdict staleness) | `docs/perf/OPEN_ITEMS.md` item 46 — **tracked, but its "still a net win" verdict is invalidated by task #842's trim removal and has not been revisited** |
| V-18 (Darwin decommit) | `docs/CORRECTNESS_OPEN_ITEMS.md` item 48 |
| V-22 (`mock` feature unification) | `docs/CORRECTNESS_OPEN_ITEMS.md` item 42 |
| V-28 (Windows counters untested) | `docs/CORRECTNESS_OPEN_ITEMS.md` item 50, U10 half |
| V-30 (`decommit_lazy_roundtrip` effect-blind) | `docs/CORRECTNESS_OPEN_ITEMS.md` item 48, S4 remainder |
| V-38 (no `cargo miri test`) | `docs/CORRECTNESS_OPEN_ITEMS.md` item 41 |
| (BSD `_SC_PAGESIZE` unverified) | `docs/CORRECTNESS_OPEN_ITEMS.md` item 43 — noted while reading `:2478-2533`; no new finding, the code's own annotation is accurate |
| (edition-2024 `unsafe_op_in_unsafe_fn`) | `docs/CORRECTNESS_OPEN_ITEMS.md` item 49 — 9 sites remain; not re-derived here |

### Verification performed for this review

- `cargo clippy -p aligned-vmem --all-features --all-targets` — clean.
- `cargo clippy --features fault-injection` and `--features fault-injection,lazy-commit` — clean.
- `cargo check --target {x86_64,i686,aarch64}-unknown-linux-gnu`,
  `x86_64-unknown-freebsd`, `x86_64-unknown-netbsd`, each with
  `--features lazy-commit,huge-pages,fault-injection,bench-internals` — all clean.
- `cargo test --features lazy-commit,huge-pages,fault-injection,bench-internals`
  on `x86_64-pc-windows-msvc` — 48 tests, all pass.
- `cargo doc --no-deps` with `-D rustdoc::broken_intra_doc_links -D
  rustdoc::private_intra_doc_links -D warnings` across
  `lazy-commit,huge-pages,fault-injection,mock,bench-internals` — clean.
- No file in the repository was modified by this review other than this report.
