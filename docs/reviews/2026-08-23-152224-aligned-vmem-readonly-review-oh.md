# Eighth independent pre-publication review — `aligned-vmem` 0.2.0 @ `743b27a`

**Author:** `@oh` (Opus, effort=high). **Reported:** 2026-08-23 15:22:24 Europe/Berlin.
**Revision reviewed:** `743b27a7e42c0d9c395c85af31598a1baeaddbcd` (`main`).
**Mode:** READ-ONLY, STATIC. No sub-agents. No file edited, no git write command run.
**Nothing was executed** — no `cargo test`, `cargo check`, `clippy`, `cargo doc`, `cargo
package`, Miri, or benchmark run, per the brief. Every finding below is a source read, and
every claim in this report is therefore **unverified by execution** unless it names a
specific file+line I read directly (all of them do).

**Filename deliberately ASCII-only**, matching the fifth audit's own reasoning about
`scripts/verify-commit-prefixes.mjs` (task #1218).

---

## 0. Verdict

**Code-level: GO.** I found no new soundness defect, no UB, no leak, and no semantic
divergence in the paths I read (the full `src/` tree — 24 files — plus `Cargo.toml`,
`README.md`, and the `CHANGELOG.md` header). This is the second consecutive audit to return
a clean code-level result (the seventh, GLM's, was the first).

**Formally: still NO-GO, for exactly one already-owned reason** — `crates/aligned-vmem/CHANGELOG.md:7`'s
`## 0.2.0 - Unreleased` header, owned by index items 97/F1 and 98/G1 and explicitly deferred
by the owner. That is a release *act*, not a defect. I add nothing to it.

**What I do add: five findings none of the seven prior audits reports**, three of them on
publish-facing surfaces, plus four unfiled performance candidates. None is a blocker on its
own; F1–F3 are the kind of thing that should not ship on a crates.io front page or a docs.rs
page if it costs one commit to fix.

---

## 1. Findings

### F1 — P2, publish-facing factual error: `README.md:360` names a Win32 API the crate never calls

```
- **Windows**: the Win32 backend (`VirtualAlloc`/`VirtualFree`/`VirtualProtect`); see the
  CI-verified list for which targets are tested.
```

`VirtualProtect` **does not exist anywhere in this crate**. `grep -rn VirtualProtect
crates/aligned-vmem/` returns exactly one hit: this README line itself. The Windows
`extern "system"` block (`src/os/windows.rs:474-484`) declares four symbols and no more:
`VirtualAlloc`, `VirtualFree`, `GetSystemInfo`, `GetLargePageMinimum`. The crate never
changes page protection — every mapping is created `PAGE_READWRITE` and stays that way.

This also contradicts the README's own earlier line 118-120 ("Backends: … `VirtualAlloc`/
`VirtualFree(MEM_DECOMMIT/MEM_RELEASE)` on Windows") and the crate-level rustdoc
(`src/lib.rs:5-6`, same two-symbol list). Line 360 is inside the "**The exact support
matrix** … this enumeration, not a blanket family claim, is the contract" block — i.e. the
one paragraph the crate asks readers to treat as normative — which makes a wrong symbol
there worse than a passing mention.

**Why it matters beyond tidiness:** a reader auditing what this crate can do to their
address space reads "VirtualProtect" as "this crate may change page protections behind my
back". It cannot.

**Fix:** delete `/`VirtualProtect`` from `README.md:360`. One token.

---

### F2 — P2, publish-facing unresolvable citation: three sites still cite a bare `docs/…` path, one of them in docs.rs-rendered rustdoc and one inside a shipped `compile_error!` message

The class task #889 closed for seven sites and task #1227 closed for seven more —
publish-facing text citing a repository path that is **not in the published `.crate`
tarball**, so a crates.io / docs.rs reader cannot resolve it — is still live at three
places. I found these by grepping for `docs/…` citations that lack an `https://` prefix:

| Site | Surface | Text |
|---|---|---|
| `src/reservation.rs:1317` | **docs.rs-rendered rustdoc** — `Reservation::from_raw_parts`'s "Correctness contract" section; `from_raw_parts` is unconditionally compiled, so this renders under the docs.rs feature set (`lazy-commit, huge-pages, fault-injection`) | ``the decision is recorded in `docs/CORRECTNESS_OPEN_ITEMS.md` item 90's OPEN QUESTION block`` |
| `README.md:350` | **crates.io front page**, MIPS support-matrix entry | ``This is a release decision; see `docs/CORRECTNESS_OPEN_ITEMS.md` item 62 for the decision record.`` |
| `src/os/unix.rs:747` | **the MIPS `compile_error!` string itself** — a downstream MIPS user reads this in their compiler output | ``See docs/CORRECTNESS_OPEN_ITEMS.md item 62 for the release decision record.`` |

Stated precisely, so this does not overclaim: the path **does** still resolve *inside this
repository* — I verified `docs/CORRECTNESS_OPEN_ITEMS.md` exists (25,870 bytes) and that its
lookup table row `| 90 | TRACKED_publish_readiness.md |` (line 363) resolves item 90
correctly after the task-#1217/#1221/#1222 splits. The defect is not staleness; it is that
`docs/` is not shipped, so for the audience these three surfaces are written for the
citation is a dead end. The same README already does this right twice — lines 198 and 295
cite the *same file* as
`<https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/CORRECTNESS_OPEN_ITEMS.md>`.
Line 350 is simply the one that was missed.

`src/os/unix.rs:747` is arguably the sharpest of the three: it is not documentation a reader
chooses to open, it is a compiler diagnostic pushed at a user whose build just failed, and
it points them at a file they do not have.

**Ten further sites** carry the same shape in `src/bench_internals/*.rs` rustdoc (10
occurrences citing `docs/reviews/*.md` and `docs/perf/OPEN_ITEMS.md`) and one in
`src/page_size_override.rs:53`'s module doc. Both are materially lower severity —
`bench-internals` is excluded from the docs.rs feature set (`Cargo.toml:14-20`) and
`page_size_override` is behind a build-time cfg — so neither is rendered on the published
docs page. I list them for completeness, not as blockers.

**Fix:** the three table rows above → full GitHub URLs, exactly as README:198/295 already do.

---

### F3 — P2, contract gap on a **CI-verified** target: the eager reserve path validates `size` against compile-time `PAGE`, everything downstream validates against runtime `page_size()`

`validate_size_align` (`src/api/internal.rs:11`) requires `size.is_multiple_of(PAGE)` —
the *compile-time* 4 KiB constant. Every decommit/recommit validator requires multiples of
the *runtime* `page_size()`. On a host where those differ — **Apple Silicon macOS, 16 KiB
pages, which this crate's README lists under "CI-verified targets"** — the two contracts
disagree, and the disagreement is reachable through the most ordinary call sequence in the
crate:

```
let mut r = reserve_aligned(4096, 4096).unwrap();   // accepted: 4096 is a PAGE multiple
r.decommit(0, r.len());                             // r.len() == 4096, page_size() == 16384
```

Traced through the code: `Reservation::decommit` (`src/reservation.rs:704`) passes its
`end > self.len()` bounds check (4096 > 4096 is false), is not huge, and forwards to the
free `decommit` (`src/api/decommit.rs:215`), which reads `ps = 16384`, evaluates
`decommit_range_is_well_formed(0, 4096, 16384)` → `4096.is_multiple_of(16384)` → `false`,
and therefore **trips the `debug_assert!` at `src/api/decommit.rs:242`**. In a debug build
this is a panic from a safe `pub fn`, on the single most natural call a user makes
("hand this whole reservation's pages back"). In a release build it is a silent no-op: such
a reservation can *never* be decommitted at all, at any offset, because no non-empty
sub-range of a 4096-byte span is a 16384-multiple.

**This is not a new mechanism — it is a known one that never reached the public contract.**
The crate's own test file works around it in a comment
(`tests/bench_internals_counters.rs:42` and `:80`): *"Reserve `4 * page_size()` … important
on Apple Silicon macOS where `page_size() == 16 KiB`, so `decommit(0, PAGE)` would fail the
guard that checks `end.is_multiple_of(ps)`"*. So the project knows. What I could not find,
after grepping `README.md` and all of `src/`, is any publish-facing statement of it:
`reserve_aligned`'s rustdoc (`src/api/reserve.rs:12-13`) states the `PAGE`-multiple contract
with no caveat; `README.md:134` says `size` must be a non-zero multiple of `PAGE`, full stop;
`Reservation::decommit`'s `# Panics` documents the debug tripwire only in the abstract
("a contract-violating range"), never that a `size` the crate itself accepted can make the
whole span contract-violating.

**The lazy path already closed exactly this gap, with the reasoning written out.**
`validate_initial_commit` (`src/api/internal.rs:75`) requires `size.is_multiple_of(ps)` —
runtime page size — and its own doc comment gives the rationale verbatim: *"This is a
fail-closed check: reject requests that would create spans that cannot be fully committed via
the public API"* (`:45-47`), plus *"a caller developing on x86-64 Linux who passes a
`PAGE`-multiple that is not a `page_size()` multiple writes code that is silently broken the
moment it runs on Windows with a larger runtime page — and the crate would have accepted it
on every host they tested"* (`:58-62`). Substitute "decommitted" for "committed" and
"Apple Silicon" for "Windows" and that paragraph describes the eager path today.

**Two honest options, and I am not choosing for the owner:**

  (a) **Document it** — one sentence on `reserve_aligned`'s rustdoc and one README bullet:
      *"a `size` that is not also a multiple of `page_size()` produces a reservation whose
      span cannot be decommitted; on such a host `decommit(0, len())` is a debug-build
      panic."* Zero behavior change, zero semver impact. This is the minimum.

  (b) **Tighten the validator** to `size.is_multiple_of(page_size())`, matching the lazy
      path. This is behavior-breaking on 16/64 KiB-page hosts (calls that succeed today
      start returning `None`), so it is a 0.2.0-or-never change, not a patch-release one —
      which is precisely why it is worth deciding *before* the first publish rather than
      after. Note it also introduces a runtime `page_size()` read into `reserve_aligned`,
      which today reads no cache at all.

I lean (a) for 0.2.0 and (b) as a recorded 0.3.0 candidate, but the asymmetry with the lazy
path is the thing that should be resolved deliberately, not left to be rediscovered.

---

### F4 — P3, API ergonomics: the two ownership-token types are not `Send`, though the `Reservation` they come from is

`Reservation` carries `unsafe impl Send for Reservation {}` (`src/reservation.rs:1572`) with
a documented SAFETY argument ("owns its OS reservation exclusively; moving it to another
thread moves ownership of every byte"). `ReservationParts` (`src/reservation_parts.rs:739`)
and `ReservationFullParts` (`src/reservation_full_parts.rs:812`) hold `*mut u8` fields and
have no such impl — so they are **neither `Send` nor `Sync`**, and the auto-trait leak means
this is invisible until a consumer tries it.

That collides with the documented use case. `into_reservation_parts`'s own rustdoc says it
exists for *"an allocator [that] records the reservation in its own self-hosted metadata"*,
and `from_raw_parts`'s says the motivating pattern is *"the cross-crate handoff pattern: a
sibling crate (`numa-shim` on Windows) issues a platform-specific reservation call … then
adopts the result"*. Both shapes routinely want the token to live in a structure shared
across threads. Today `Reservation` can cross a thread boundary and its own parts cannot —
the round trip `Reservation → into_full_parts() → (move) → into_reservation()` is blocked
in the middle for no soundness reason the crate states.

The SAFETY argument transfers unchanged: the parts describe the same exclusively-owned
mapping, and every operation on them is already `unsafe`. `unsafe impl Send for
ReservationParts {}` / `… for ReservationFullParts {}` is purely additive and semver-safe.
`Sync` is a separate question and I am **not** proposing it (`&ReservationParts` hands out a
`*mut u8` a second reader could release — the same reason `Reservation` is `Send` but not
`Sync`).

Worth an explicit decision before publish: adding `Send` later is additive, but a consumer
who hits the gap in the first week and works around it with a newtype + their own
`unsafe impl` has already paid for it.

---

### F5 — P3, residual of the class task #1213/L1 closed: `LazyReservation`'s two mutators read the page-size cache twice per call

Task #1213/L1 removed a double `page_size_or_poison()` read from the `try_decommit` chain,
and `dispatch_try_decommit`'s doc comment (`src/api/decommit.rs:326-332`) now states the
property as achieved: *"the total atomic-load count for ANY call through either entry point
is now exactly one, not two or three."* That is true of the decommit chain. It is not true of
the lazy-commit chain, which has the identical shape and was not touched:

- `LazyReservation::ensure_committed` (`src/lazy_reservation.rs:144`) reads `page_size()`
  at `:153` for the rounding, then calls `self.inner.try_commit_range(...)` at `:155`,
  which forwards to the free `try_commit_range` (`src/api/commit_range.rs:76`) — whose
  first statement at `:77` is another `page_size_or_poison()`.
- `LazyReservation::shrink_committed` (`:171`) reads `page_size()` at `:175`, then calls
  `self.inner.decommit(...)` at `:179` → free `decommit` (`src/api/decommit.rs:215`) →
  `page_size_or_poison()` at `:216`.

Two relaxed loads of a process-lifetime-constant cache line per call. The cost is
negligible in isolation — I am **not** claiming a measurable speedup, and no measurement
exists — but `ensure_committed` is explicitly documented as the call you make *"before every
write without remembering what you already committed"*, i.e. the one entry point in this
crate designed to be called in a loop. If the class was worth closing on the decommit path
it is worth closing here, or the property statement at `src/api/decommit.rs:326-332` should
be scoped to say it describes the decommit dispatch only (it already says "through either
entry point", which a reader can fairly take as crate-wide).

Cheapest fix: a `pub(crate)` `try_commit_range_with_ps(base, start, end, ps)` /
`decommit_with_ps(...)` that the public forms wrap, with `LazyReservation` calling the
`_with_ps` forms using the snapshot it already took. Alternative: accept and document.

---

## 2. Performance candidates — unfiled, ranked by expected value

None of these is a measured result. Each is a reading-level observation with a named
mechanism; per this repository's own rules none may be presented as a proven speedup without
a gate report. I checked `docs/perf/OPEN_ITEMS.md` before listing each — P1, P2 and P3 have
zero prior mentions there; P4 is adjacent to item 48/53 but is a different axis.

### P1 — Windows: `VirtualAlloc2` + `MEM_ADDRESS_REQUIREMENTS` collapses the two-call path to one call with zero over-reserve

**This is the largest concrete unexploited win I found, and it targets the crate's own
headline use case.** For `align > WIN_ALLOCATION_GRANULARITY` (64 KiB) — i.e. every 2 MiB
or 4 MiB allocator segment, the shape the README's first example uses — `win_reserve_commit`
(`src/os/windows.rs:56`) takes the two-call path: `VirtualAlloc(NULL, size + align,
MEM_RESERVE)` then `VirtualAlloc(base, size, MEM_COMMIT)`, and keeps the whole `size + align`
region for the reservation's lifetime. A 4 MiB-aligned 4 MiB segment therefore costs **2
syscalls and 8 MiB of address space**.

`VirtualAlloc2` (Windows 10 1803+ / Server 2019, `api-ms-win-core-memory-l1-1-6.dll`) accepts
a `MEM_EXTENDED_PARAMETER` of type `MemExtendedParameterAddressRequirements` carrying
`MEM_ADDRESS_REQUIREMENTS { LowestStartingAddress: NULL, HighestEndingAddress: NULL,
Alignment: align }`. The kernel returns a reservation whose base is **exactly** `align`-aligned,
in one call, with **no over-reserve**. That is 1 syscall instead of 2 and `size` bytes of VA
instead of `size + align` — and, as a side effect, it makes `Reservation::reservation_len()`
honest for this path (its rustdoc currently spends three bullets explaining why the value
under-reports).

**Costs, stated so this is not sold as free:** the crate today declares all its imports
statically; using `VirtualAlloc2` while keeping pre-1803 support requires runtime
`GetModuleHandle`/`GetProcAddress` resolution cached in an atomic, plus keeping the existing
two-call path as the fallback. That is real new machinery in the one file where this crate's
`unsafe` lives, and it needs its own path-activation oracle (a `bench-internals` counter
splitting resolved-vs-fallback) before any number is quotable. Not a 0.2.0 change; a
well-shaped 0.3.0 one.

### P2 — Windows: `GetLargePageMinimum()` is a live FFI call on every huge reservation, for a process-constant

`win_reserve_commit:105-111` calls `GetLargePageMinimum()` on every invocation with
`extra_commit_flags != 0` — i.e. every `reserve_aligned_huge`. The value cannot change during
process lifetime. This crate already caches the analogous `GetSystemInfo` answer in
`PAGE_SIZE_CACHE` (`src/page_size.rs:21`) for exactly this reason, with a documented
three-state protocol. Caching this one the same way is a handful of lines and removes an FFI
call from the reservation path.

Note for whoever picks this up: `docs/perf/OPEN_ITEMS.md` item 47's evidence block mentions
`GetLargePageMinimum` — but only to record that a *different* proposal (a size-divisibility
pre-check) does not exist, and to note the symbol appeared after the task-#1082 split. The
caching question itself is not filed. Expected value is small and it is not on any hot path
(huge reservations are large upfront segments), so this is a while-you-are-in-there item, not
a round of its own.

### P3 — Linux: no `MAP_NORESERVE` on the ordinary over-reserve, so strict-overcommit hosts are charged for slack that is never touched

`libc_mmap` (`src/os/unix.rs:1033`) passes `MAP_PRIVATE | MAP_ANON` and `PROT_READ |
PROT_WRITE`, never `MAP_NORESERVE`. The hugetlb half of this is already documented at length
(`unix_reserve`'s task-#1069 comment, and `reserve_aligned_huge`'s rustdoc). The **ordinary**
half is not: under `vm.overcommit_memory=2` (strict accounting — common in hardened
containers and some distro defaults), every 64-bit Unix reservation charges the full
`size + align` against the system commit limit, including the exactly-`align` bytes of slack
that the crate's own design guarantees are never written to.

**I am flagging this as a question, not a recommendation.** `MAP_NORESERVE` moves the failure
point from `mmap` (a clean `None`/`Err` the caller handles) to first touch (a SIGSEGV or the
OOM killer), which is a materially worse failure mode for an allocator and would need its own
owner decision. A narrower variant — `MAP_NORESERVE` only on the slack, via a second mapping
— reintroduces the head/tail split that task #842 deliberately removed for soundness, so it
is not obviously better. Filing it so the strict-overcommit dimension is at least recorded.

### P4 — Unix: the over-reserve slack is mapped `PROT_READ | PROT_WRITE`, so an out-of-bounds write into it silently succeeds where Windows faults

Not a safety defect — the slack is inside the crate's own reservation and is released with it
— but a debuggability asymmetry worth knowing. On Windows the two-call path commits only
`[base, base + size)`, so a consumer's off-by-one write past `as_ptr() + len()` raises
`STATUS_ACCESS_VIOLATION` immediately. On Unix the same write lands in mapped, writable slack
and succeeds silently, up to `align` bytes past the usable span. A consumer developing on
Linux and shipping on Windows gets the crash on the *other* platform — the same
platform-divergence shape the README already documents for decommit (line 191). Closing it
would need `mprotect(PROT_NONE)` on the head and tail, which is extra syscalls on the reserve
path *and* partially reverses task #842's one-`munmap` design. Most likely disposition: a
sentence in `Reservation::as_ptr`'s validity section, not a code change.

---

## 3. Surfaces I re-read and found clean

Stated so this report's silence is informative rather than ambiguous. Read directly at
`743b27a`, not trusted from prior audits:

- **errno/`GetLastError` capture timing** (task #713's discipline): every `Err` in
  `src/os/unix.rs` and `src/os/windows.rs` is constructed at the failing syscall, before any
  cleanup FFI. The `libc_mmap` zero-address rejection path correctly uses the no-code
  sentinel rather than a post-`munmap` `last_os_error()` (`src/os/unix.rs:1086-1089`).
- **Strict provenance**: `.addr()` / `.with_addr()` throughout both backends; no
  address-to-pointer round trip in `src/`.
- **Checked arithmetic**: `size.checked_add(align)` on all four over-reserve sites, plus
  `validate_size_align`'s `sum > isize::MAX` rejection (`src/api/internal.rs:32`) that keeps
  a later `Layout::from_size_align` from turning a contract violation into a deferred panic.
- **Poison fail-closed**: `PAGE_SIZE_QUERY_FAILED == usize::MAX` is not a power of two and no
  non-zero offset is a multiple of it, so a validator that read the raw value *without* an
  explicit poison check still rejects every non-empty range. The "suspenders" argument in
  `decommit_range_is_well_formed`'s doc holds as written.
- **`from_raw_parts` asserts**: the unconditional block plus the `#[cfg(not(feature =
  "huge-pages"))] !granted_huge` assert plus the Linux/Android 2 MiB block. The task-#1196
  note that the fifth conjunct is implied by the two address conjuncts is correct and the
  comment says so.
- **Windows `SystemInfo` layout** — field order and types match `SYSTEM_INFO`; only
  `dw_page_size` and `dw_allocation_granularity` are read.
- **`Drop` / release exactly-once**: `into_parts` / `into_reservation_parts` /
  `into_full_parts` all `mem::forget(self)`; `release` early-returns on null *before* the
  assert and before `mock::record`.
- **`mock` reentrancy and TLS-teardown handling**: `RECORDING` guard + `CALLS.try_with`, and
  `drain`'s `mem::take` that does not hold the borrow across the returned `Vec`'s allocation.
- **`fault_injection` atomicity**: `fetch_update` (not load-then-store) on both `FAIL_NEXT`
  and `FAIL_NEXT_DECOMMIT`, with the `then` (lazy) vs `then_some` (eager) underflow trap
  correctly avoided and commented at both sites.
- **`bench_internals` completeness**: `reset_bench_internals_counters` stores 16 counters and
  its doc names 16; `windows_reserve_commit_calls` is a derived sum of two reset counters,
  which is why it is 17 accessors over 16 statics.
- **MSRV**: `rust-version = "1.88"` covers everything used — `is_multiple_of` on integers
  (1.87), `.addr()`/`.with_addr()` (1.84), `io::Error::other` (1.74), `[lints]` with
  `check-cfg` (1.80), `next_multiple_of` (1.73).
- **Packaging**: `LICENSE-MIT` + `LICENSE-APACHE` present, `readme`/`repository`/`homepage`/
  `documentation` set, 5 keywords / 2 categories (both within crates.io limits), crate
  directory 1.2 MB total with no oversized artifacts. No `include`/`exclude`, so `tests/`
  (512 K), `benches/`, and `examples/` ship — acceptable, and the two source-text guard tests
  that read `env!("CARGO_MANIFEST_DIR")/src` at runtime still resolve inside a vendored copy.

---

## 4. What this review did NOT do

- Executed nothing. No test, build, clippy, doc, Miri, benchmark, or `cargo publish
  --dry-run` was run. F1–F5 and P1–P4 are all source reads.
- Did not re-derive the prior seven audits' findings or re-check their closures beyond the
  surfaces listed in §3.
- Did not read `tests/` in full (18,211 lines across the crate; I read the test files only
  where a finding required it — `tests/bench_internals_counters.rs` for F3).
- Did not evaluate the CI workflow's correctness, only which rows exist.
- Produced no measurement, so nothing in §2 may be quoted as a speedup.
