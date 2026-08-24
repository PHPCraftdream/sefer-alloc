# `bind_range` Linux contract — decision brief (written 2026-08-24 12:01, for owner review)

Not yet committed to git. This is a working document collecting the full
picture of an open owner decision (task #1303, correctness-open-items
item 105) so it can be reviewed at leisure, not a finalized record. Once
a decision is made, fold the outcome into item 105's card and task
#1303's implementation, following this campaign's established
decision-record convention (see `docs/NUMA_GATE_2026-08-23_0.2.0_phase24_waiver.md`
for the shape a finalized record takes).

## 1. The confirmed bug

The fourteenth independent review (Sol-codex,
`docs/reviews/2026-08-24-113155-numa-shim-publication-audit-Sol-codex.md`)
found, and this session independently verified live against the
authoritative man page, a real functional defect:

`crates/numa-shim/src/lib.rs`'s `pub unsafe fn bind_range(base: *mut u8,
len: usize, node: u32)` (~line 512) calls, on Linux,
`bind_range_impl_linux` (~line 1099), which passes `base` directly to the
`mbind(2)` syscall via a raw `syscall(SYS_MBIND, …)` wrapper
(`libc_mbind`, ~line 1120) and **discards the syscall's return value
entirely** — no check, by original design (the doc comment already says
"silently ignores OS errors... the allocation is always valid regardless
of whether binding succeeded").

**Verified fact** (fetched live from
`https://man7.org/linux/man-pages/man2/mbind.2.html`'s ERRORS section,
not assumed from the review's citation alone): `mbind(2)` returns
`EINVAL` when `addr` is not a multiple of the system page size. The
kernel does **not** round the address down — it rejects the call
outright.

**Consequence:** `README.md`'s own headline example (~line 48):

```rust
let mut buf = vec![0u8; 4096];
let node = current_node().unwrap_or(0);
unsafe { bind_range(buf.as_mut_ptr(), buf.len(), node) };
```

A plain heap `Vec<u8>` allocation is essentially never page-aligned
(typical allocator alignment for a request this size is 8–16 bytes, well
below any `mmap_threshold` that would guarantee page alignment). So this
exact, copy-pasteable example silently fails to bind on Linux in the
overwhelming majority of runs: `EINVAL` happens, gets discarded, the
caller observes ordinary success, and no NUMA policy is actually
applied. `tests/smoke.rs::bind_range_on_owned_memory_does_not_panic` only
asserts the call doesn't panic and the buffer stays readable — zero
coverage in either direction for whether binding actually took effect.

This is the first genuinely new P1 functional defect found in
already-reviewed code across this campaign's entire 14-audit history.

## 2. Context that constrains the choice

- **`reserve_on_node` (the `vmem-integration`-gated function) is already
  page-aligned by construction** — its contract requires `align >= PAGE`
  and `size` a page multiple (`lib.rs:543-545`, enforced Windows-side at
  `:1333-1343`). The crate's "safe path" never hits this bug.
- **The in-tree consumer is unaffected by any option**:
  `src/alloc_core/numa.rs:61-71` (`bind_segment`) only ever passes 4
  MiB-segment-aligned reservations to `bind_range`.
- **Misaligned `base` is NOT UB** — it's a failed syscall, not memory
  unsafety. Tasks #725/#778 (earlier in this crate's history) deliberately
  narrowed `bind_range`'s `# Safety` section away from over-broad
  preconditions, specifically so the heap-`Vec` smoke test would not be
  UB-by-contract. Putting an alignment requirement into `# Safety` would
  reverse that direction.
- **CHANGELOG already records an open, related ask**: task #1274/N8 —
  "`bind_range` with `node >= 64` still silently no-ops with no
  caller-detectable signal (F4's binding-side ask remains open)." Any
  option that makes binding failures observable would close this
  long-standing item too.
- **No `PAGE_SIZE` constant currently exists in `numa-shim`** —
  `aligned_vmem::page_size()` is feature-gated behind
  `vmem-integration`, unavailable to the base crate. A page-envelope
  approach needs runtime discovery via `sysconf(_SC_PAGESIZE)` /
  `getpagesize()` (cannot hardcode 4096 — aarch64 can run 16K/64K
  pages), cached in the crate's existing `OnceLock` topology-cache
  pattern.

## 3. The three original options (as posed by the review)

- **(a) Require page-aligned `base`.** Document the requirement in
  `# Safety`, replace the README example with a page-aligned allocation.
  The review's own stated "safest small option."
- **(b) Accept a page envelope.** The crate itself computes
  `page_floor(base)` / `page_ceil(base+len)` and passes THAT range to
  `mbind`, instead of the caller's raw range — formalizing the
  page-granularity behavior the `# Safety` doc already discloses
  ("any other data sharing a page… is affected — harmless for a policy
  hint").
- **(c) Surface the syscall's error/status.** Make the silent `EINVAL`
  (and the already-known `node >= 64` no-op) observable instead of
  invisible.

## 4. `@fh`'s analysis (first consultation)

Read the actual source (not just the summary), including
`reserve_on_node`'s alignment contract, the in-tree consumer, and the
CHANGELOG.

**Key findings:**
- Confirmed (a) is semantically wrong — non-UB condition misfiled as a
  `# Safety` precondition, reverses #725/#778's direction, and a
  `debug_assert` for it would compile out in release (the crate's own
  R25-5 lesson about release-invisible checks).
- Confirmed (b) is safe: every page in the envelope intersects the
  caller's already-mapped range (no new EFAULT surface), and the
  `# Safety` doc already discloses page-granularity spillover as
  "harmless for a policy hint."
- Noted (c) alone doesn't fix the underlying mismatch, just makes the
  failure visible — "equivalent to (a) with extra steps."
- Noted 0.2.0 already carries other breaking changes (the `mock` feature
  removal), so timing may favor bundling one more.

**Recommendation: (b) + (c) combined.** Change `bind_range`'s own return
type from `()` to a `#[must_use]` `#[non_exhaustive]` status enum
(`Bound` / `Skipped{...}` / `Unsupported` / `Failed(errno)`). Reasoning:
(b) alone leaves real failures silent; (c) alone doesn't fix the bug;
together, the common case works AND rare failures are observable, the
open N8 item closes, and — critically — the combination makes the bug
class testable for the first time (`tests/smoke.rs` could assert a real
`Bound`/`Unsupported` outcome instead of just "doesn't panic").
Advised against (a) in any form.

## 5. `@ox`'s independent, adversarial second opinion

Explicitly asked to stress-test `@fh`'s analysis rather than rubber-stamp
it — re-read the same source independently.

**Agrees with `@fh` on the core technical point:** (b) is genuinely safe.
Mapping boundaries are page-granular by construction, so the envelope
can only ever land inside pages that already contain caller bytes — a
sub-page "neighbor" is another heap object in the SAME mapped page, never
unmapped memory. A genuine gap yields `EFAULT`, not UB (mbind never
touches payload bytes).

**Two costs `@fh`'s analysis missed:**
1. `mbind` on a sub-VMA range **splits the VMA** at the kernel level.
   Today's `EINVAL` makes this free (the call never reaches that kernel
   code path); after (b), a loop binding many small heap objects could
   walk toward `vm.max_map_count` (default 65530) — a new resource-cost
   dimension that doesn't exist today.
2. **The policy attaches to the address range, not the `Vec` object** —
   it outlives the `Vec`'s deallocation. If the allocator reuses those
   pages for something unrelated later, the OLD NUMA policy is still in
   effect on them, silently, with no connection to the new occupant.

**Disagrees sharply with `@fh` on (c)'s "breaking change is fine, timing
is favorable" framing.** Checked the actual CHANGELOG: **all four
existing 0.2.0 breaking changes are `mock::*`-surface only** (`MockCall`
`#[non_exhaustive]`, `CALLS`/`CURRENT_NODE_SLOT` narrowed, a variant
`#[non_exhaustive]`, the `mock` Cargo feature removed) — every one hits
only consumers of a TEST-ONLY backend. Changing `bind_range`'s own
signature would be **the first break to the crate's real, production
API surface**. "Four already, one more is free" does not hold — those
four cost real consumers nothing; this one would cost every current
caller of `bind_range` a source change.

**Alternative recommendation:** the crate already solved this EXACT
shape of problem one release ago, additively — `current_node()` stayed
unchanged, `current_node_resolution() -> NodeResolution` was ADDED
alongside it (task #1266). Do the same for the binding side: keep
`bind_range -> ()` completely unchanged (so it becomes `let _ =
bind_range_status(...)` internally, zero break for existing callers),
and add a NEW `#[must_use] fn bind_range_status(base, len, node) ->
BindStatus` alongside it. Zero breaking change, N8 still closes,
`sefer-alloc`'s own `numa.rs:70` call site untouched, and
`tests/smoke.rs` still gets its first real behavioral oracle (by calling
the new function in the test instead of the old one).

**Also flagged, not previously raised by anyone:**
- The README example is conceptually wrong, not just misaligned — (b)
  makes it "succeed" while still teaching users a pattern that splits
  their heap VMA and leaves a permanent policy behind on reused pages.
  Suggests leading the README with `reserve_on_node` instead, and
  demoting `bind_range` to "you already own a real OS mapping."
- Page-size discovery adds a new raw FFI call to a crate that markets
  itself on having a small, carefully-audited unsafe surface.
- Whatever `Unsupported`/status enum is chosen must also correctly cover
  Linux-but-not-x86_64/aarch64 (the architectures without a known
  `SYS_MBIND` number today already no-op silently — this is a THIRD
  silent-no-op path, separate from `node >= 64` and from the alignment
  bug, that a complete status enum needs to represent too).

## 6. The four choices as currently framed

1. **(b) + additive `bind_range_status()`** (`@ox`'s recommendation) —
   page envelope fixes the bug for everyone silently; a new, separate,
   `#[must_use]` function gives status visibility to callers who want it;
   zero breaking change to `bind_range` itself.
2. **(b) + (c) combined** (`@fh`'s original recommendation) — change
   `bind_range`'s own signature to return a status; one API break, but a
   simpler single-function surface (no `_status` twin to maintain).
3. **(b) alone, no status surfacing** — minimal scope, purely fixes the
   confirmed bug, defers the N8 observability ask to a later release.
4. **(a), page-alignment requirement** — the review's original
   suggestion; both `@fh` and `@ox` independently recommend AGAINST this
   one, for overlapping but not identical reasons (misfiled Safety
   precondition; guts the headline "bind a live allocation" use case;
   release-invisible `debug_assert`).

Neither `@fh` nor `@ox` flagged VMA-splitting cost or policy-outliving-
the-Vec as blocking — both treat them as documentable residual behavior
`@ox` — not as reasons to abandon (b). Worth deciding explicitly whether
either warrants an additional doc note ("binding many small short-lived
objects individually is not free; policy persists past deallocation
until the pages are reused under a different policy call") regardless of
which of the four numbered choices above is picked.

## 7. Not yet decided

The owner asked to have this written down for review rather than decide
in the moment. No implementation has started. Task #1303 (TaskList)
tracks the open decision; correctness-open-items item 105 tracks the
underlying finding. Once a choice is made, the natural next steps are:
implement the chosen option, add the real behavioral-oracle test both
consultants agree is now finally possible, update the README, and file
the decision (with reasoning) into item 105's card and task #1303's
closure — matching this campaign's established decision-record
convention.
