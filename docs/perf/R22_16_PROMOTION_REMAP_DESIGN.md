# R22-16 — Remap-instead-of-copy for the medium→Large promotion memcpy: design (NOT implementation)

> **CORRECTION (2026-07-27, task #373, R23-4).** This document's §2.4
> "promotion-time neighbor-liveness check" blocker for sub-region remap is
> **WRONG** — independently re-verified against the actual `carve_block`/
> `carve_batch`/`decommit_empty_segment_impl` source. A live carved block's
> byte range is provably exclusive for its entire live lifetime (bump-carve
> is monotonically forward-only; the only backward bump reset requires the
> WHOLE segment to already be empty), so no runtime "who else is on my
> pages" check is needed for Linux sub-region remap specifically. §3's
> whole-segment base-address-stability blocker is UNAFFECTED and remains a
> real NO-GO. The revised verdict — NO-GO for whole-segment remap,
> CONDITIONAL-GO for Linux sub-region remap pending a correctness
> prototype, Windows unaffected (still blocked by the section-object
> constraint, §1.2) — is in the new **§10. CORRECTION** at the end of this
> document. Original content below is preserved verbatim per this project's
> "append, don't rewrite" convention; do not cite §0/§2.4/§6's original
> framing without reading §10 first.

**Task:** R22-16 (task #367, P1). **DESIGN-ONLY.** No `src/` change, no
`Cargo.toml` change, no `crates/vmem/src/lib.rs` change, no `tests/` change, no
benchmark run. This document investigates whether an OS-level VA-remap
primitive (Windows placeholder VA / `MEM_REPLACE_PLACEHOLDER`, Linux `mremap`)
could replace the promotion `memcpy` itself — the one part of the R10-2 kill
gate that every prior lever (`large-reserved-capacity`, OPT-H) attacked
around, never at.

**Date:** 2026-07-26. **Base revision:** `main` @ `ff48029` (R22-15, task #366;
none of the immediately preceding R22-1..R22-15 commits touch the files this
design reads).

**Where this task comes from:** the task brief names it directly — the last
untried asymptotic lever after `docs/perf/R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md`
(NULL: destination headroom cannot retroactively cheapen an already-happened
copy) and `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md`'s R22-6 addendum
(OPT-H closed via closed-form LCM arithmetic: at most one cross-class hop per
segment, not enough to dodge the copy for a realistically-growing buffer).
Both prior levers moved the DESTINATION or the CARVE POSITION; this design
asks whether the COPY can be eliminated at the OS level instead.

---

## 0. Headline summary

**VERDICT: NO-GO**, on the current segment model, for the general case; **the
narrower MediumExtent redesign (§4a) is CONDITIONAL-GO as a SEPARATE future
design**, gated on a cheap Stage-1 measurement this document specifies but
does not run.

The crux (task brief point 2) is real and is a hard blocker, not a soft one:
`crates/vmem/src/lib.rs` — read in full (§1) — provides **zero** existing
wrapping of any region-relocation primitive (`mremap`, Windows placeholder VA
/ `MEM_REPLACE_PLACEHOLDER`, `MapViewOfFile3`). Every primitive it exposes is
**reserve / commit / decommit / release a region at its OWN fixed base** — none
of them move an existing mapping's *contents* to a *new address* while
preserving the *old* address's page-table entries as untouched. This would be
entirely new FFI surface on both platforms (§2).

Worse, and independent of the FFI gap: `src/alloc_core/segment_header.rs` and
`src/alloc_core/alloc_core_small.rs`'s carve model (read in full, §3) confirms
a medium/small **segment is a single 4 MiB OS mapping shared by many blocks of
possibly different classes**, carved by **one segment-wide bump cursor**
(`SegmentHeader::bump`) — not "one block, one mapping." A remap primitive
(`mremap`/placeholder-VA) operates on a REGION at OS page granularity: it
relocates *every byte* in the region it is given, and (per the OS contracts
cited in §2) that region must correspond to a mapping *the kernel itself
tracks as one unit* — it cannot selectively pull one live block's pages out of
a shared VMA/mapping while leaving sibling blocks' pages resident at their
original addresses, when the caller does not currently have a mapping
boundary that already separates them. **A promoted medium object almost
never has such a boundary**: it typically shares its segment's one 4 MiB
mapping with dozens of unrelated live/free small-class blocks, freely
interleaved by the bump cursor with no page-aligned start/end and no
carve-time reservation of "this object gets its own pages." §3.3 works the
actual numbers: a 256 KiB object is 64 pages at the default 4 KiB page size —
large relative to a single small-class block, but **still only 1/16th of one
segment**, carved back-to-back with whatever else the bump cursor placed
before and after it, with **no page-boundary guarantee at either edge**.

So under the segment model as it exists TODAY, remap-in-place (§4, direction
b) is a dead end — not because the OS primitive is weak, but because this
codebase's OWN carve discipline never produces the "one object, cleanly
page-isolated, nothing else sharing its pages" precondition the OS primitive
requires. This is the same shape of finding OPT-H's R20-3 design already
made for a different mechanism (§1.3 of that doc: "the only genuinely free
lunch is bump-tail adjacency, a single-slot-per-segment resource") — here the
resource that would need to be free (page-isolated ownership) is *rarer than*
tail-adjacency, not more common, because it additionally requires page
ALIGNMENT at both edges, which nothing in the carve path currently arranges
for medium objects specifically.

The one path that could admit the primitive is the MediumExtent redesign
named in the task brief's point 4a: give every candidate-for-promotion medium
object **its own dedicated, page-aligned VA region from carve time** (structurally
the same thing this crate already does for Large — see §4's comparison).
That removes the neighbor-sharing problem by construction, but it is a
**different mechanism with a different cost model** (every candidate object
pays an OS reservation at ALLOCATION time, not just at promotion time) — not
a small patch to the existing shared-segment carve path. This document does
not design MediumExtent in full (that is out of scope per the task brief's
framing — it names it as "direction (a)" for future comparison, not something
to build here); it establishes that direction (b) is closed and that (a) is
the only surviving direction, and specifies the cheap Stage-1 measurement
(§5) a future round would need before investing in designing (a) in full.

---

## 1. The exact per-OS primitive and its constraints

### 1.1 What `crates/vmem/src/lib.rs` already provides — read in full

`crates/vmem/src/lib.rs` (1394 lines) is confirmed, per the file's own module
doc (lines 1-24) and CLAUDE.md's own "single-file seam crate" exception, to be
the **entire** OS-reservation surface for this crate — every raw
`VirtualAlloc`/`VirtualFree`/`mmap`/`munmap`/`madvise`/`sysconf`/
`GetSystemInfo` FFI declaration used anywhere in `sefer-alloc` lives here (no
`windows-sys`, no `libc` dependency — raw `extern "system"`/`extern "C"`
blocks declared locally, lines 933-943 for Windows, 1247-1260 for Unix).

The complete inventory of what it exposes:

- **Reserve** (`reserve_aligned`/`try_reserve_aligned`, lines 353-386): the
  over-reserve + trim technique — reserve `size + align` bytes at a
  kernel-chosen address (`VirtualAlloc(NULL, .., MEM_RESERVE, ..)` /
  `mmap(NULL, .., MAP_PRIVATE|MAP_ANON, ..)`), find the aligned sub-range,
  trim the excess. **The base is always kernel-chosen**, never a caller-target
  address — there is no `VirtualAlloc(hint_address, ..)` / `mmap(hint_addr,
  MAP_FIXED, ..)` call anywhere in this file.
- **Release** (`release`/`Drop`, lines 388-410, 324-332): `VirtualFree(..,
  MEM_RELEASE)` / `munmap` — returns the WHOLE reservation to the OS.
- **Decommit/recommit** (`decommit`/`decommit_lazy`/`recommit`, lines 416-530):
  `MEM_DECOMMIT`/`madvise(MADV_DONTNEED|MADV_FREE)` and `VirtualAlloc(..,
  MEM_COMMIT, ..)` — return/restore PHYSICAL backing within an EXISTING,
  already-based reservation. Never changes any address.
- **Incremental commit** (`commit_range`/`try_commit_range`, lines 536-606,
  `lazy-commit` feature): commits a sub-range of an existing reservation that
  was reserved-but-uncommitted. Same base, no relocation.
- **Huge pages** (`reserve_aligned_huge`, `huge-pages` feature, lines
  689-731): `MAP_HUGETLB`/`MEM_LARGE_PAGES` — a page-size hint at RESERVE
  time, not a post-hoc operation on a live mapping.
- **`leak_zeroed_pages`** (lines 737-782): reserve-and-leak for
  process-lifetime sidecars. Not relevant here.
- **`Reservation::from_raw_parts`** (lines 266-321): adopts an
  externally-obtained reservation (used for the `numa-shim` cross-crate
  handoff) into the RAII lifecycle. Still assumes the reservation's base is
  whatever the external call produced — no relocation semantics.

**Confirmed: nothing in this file is close to placeholder VA /
`MEM_REPLACE_PLACEHOLDER` / `MapViewOfFile3`, nor to `mremap`.** A
process-wide grep for `mremap`, `MEM_REPLACE_PLACEHOLDER`, `VirtualAlloc2`,
`MapViewOfFile3`, `MEM_RESERVE_PLACEHOLDER` across the ENTIRE repository
(not just `crates/vmem`) returns **zero** hits in any `src/`, `crates/`, or
`benches/` file — the only 3 files mentioning any of those tokens at all are
this round's own review docs (`docs/reviews/2026-07-26-r22-plan.md`,
`docs/reviews/2026-07-26-oh-review-r19-r21.md`) and an unrelated historical
checkpoint (`docs/checkpoints/2026-07-05-perf-X-arc-planned.md`) that merely
name-check the idea in passing. This would be **entirely new FFI surface**
on both platforms — nothing to extend, only to invent.

### 1.2 Windows: placeholder VA + `MEM_REPLACE_PLACEHOLDER`

The real Windows 10 (1803+)/Server 2016+ mechanism this task names:

- `VirtualAlloc2(process, addr, size, MEM_RESERVE | MEM_RESERVE_PLACEHOLDER,
  PAGE_NOACCESS, ...)` reserves a VA range as a *placeholder* — address
  space with no backing, explicitly markable as splittable/replaceable.
- To move a payload: (1) `VirtualFreeEx(.., MEM_RELEASE |
  MEM_PRESERVE_PLACEHOLDER)` demotes the *source* range back to an
  unbacked placeholder (releasing its physical/pagefile-backed pages,
  keeping the VA reservation itself alive as a placeholder), OR the
  mapping is done via `MapViewOfFile3` against a **pagefile-backed section
  object** (`CreateFileMapping2`/`NtCreateSection`) — the section object,
  not the VA range, is what actually owns the physical pages; (2) at the
  DESTINATION placeholder, `MapViewOfFile3(section, process, dest_addr,
  ..., MEM_REPLACE_PLACEHOLDER, ...)` maps the SAME section object's pages
  at a new address.
- **The hard constraint this task brief itself names, confirmed against the
  real API contract**: this mechanism moves a **section-backed mapping**,
  not raw anonymous `VirtualAlloc` memory. `VirtualAlloc`-committed memory
  (which is exactly what every `crates/vmem` reservation is — see §1.1,
  there is no section object anywhere in this crate) has no section handle
  to re-map elsewhere; you cannot `MapViewOfFile3` a `VirtualAlloc` region.
  **Adopting this mechanism would require re-architecting every segment's
  underlying memory model from `VirtualAlloc`/anonymous-mmap to
  file-mapping-backed** (a pagefile-backed section on Windows, `memfd`/
  shared anonymous mapping analogue on Linux) — a foundational change to
  `crates/vmem`'s entire reservation strategy, not an additive function. This
  is a materially larger commitment than the task brief's framing ("does
  `crates/vmem` already wrap anything close") suggests when read at face
  value: the honest answer is not just "no, needs new FFI" but "no, and the
  new FFI needs a different backing-store model than every allocation this
  crate has ever made."
- Granularity: `VirtualAlloc2`/`MapViewOfFile3` operate at the OS
  **allocation granularity** (`SYSTEM_INFO::dwAllocationGranularity`, 64 KiB
  on all Windows targets sefer-alloc supports — distinct from
  `dwPageSize`, 4 KiB) for the placeholder's base address, and page
  granularity (`dwPageSize`) for split points. A placeholder can be SPLIT
  (`VirtualFreeEx(.., MEM_PRESERVE_PLACEHOLDER)` on a sub-range) so a
  sub-region of a larger reservation *can* become independently
  replaceable — but only along page boundaries, and only if the placeholder
  was created (or later split) with that sub-region as a distinct
  placeholder unit BEFORE any real mapping occupied it. Retrofitting a
  split onto an already-live, already-carved 4 MiB segment (today's
  reality — see §3) is not what this API is for; it is designed for
  reserve-time layout planning, not after-the-fact carving-out of one live
  object from a mixed-content region.

### 1.3 Linux: `mremap`

- `mremap(old_addr, old_size, new_size, MREMAP_MAYMOVE[, new_addr])` —
  POSIX/Linux-specific (glibc/musl wrap the raw syscall; this crate would
  need a new raw `extern "C"` declaration, following the exact pattern
  `libc_mmap`/`libc_munmap` already establish at `crates/vmem/src/lib.rs`
  lines 1247-1260, since the crate deliberately has no `libc` dependency —
  §1.1).
- **Constraint directly relevant to §3's finding**: `mremap`'s `old_addr`
  must be **page-aligned**, and — critically — the kernel operates on
  `old_addr`'s **VMA** (virtual memory area): the man page is explicit that
  `mremap` "expands (or shrinks) an existing memory mapping" and moves
  "all the pages" in the specified range. If `[old_addr, old_addr+old_size)`
  spans only PART of a VMA that also backs other live data (the common case
  here — see §3: one segment is one `mmap`-created VMA containing many
  carved blocks), `mremap` **still only touches the byte range you name,
  splitting the VMA if needed** — Linux CAN in principle remap an arbitrary
  page-aligned sub-range of a larger anonymous mapping, unlike Windows'
  placeholder model which needs the sub-region pre-declared. This is a real
  asymmetry between the two platforms' primitives that any future revisit
  must not paper over: **Linux's `mremap` is structurally more permissive
  than Windows' placeholder-VA mechanism for this specific ask** (it does
  not require a pre-existing section object or a pre-split placeholder), but
  it still requires the moved range to be an exact, page-aligned span whose
  bytes belong ENTIRELY to the object being moved — the neighbor-sharing
  problem (§3) is unchanged: if the 256 KiB object's first or last page
  also holds bytes of an ADJACENT carved block (no alignment guarantee at
  either edge, per §3.2), `mremap`-ing the page-aligned span containing it
  would move (and by Linux's copy-on-move-into-a-new-mapping semantics for
  the destination, effectively duplicate/orphan) the neighbor's bytes too —
  silent corruption of whichever block does NOT end up at the address the
  allocator's segment-lookup arithmetic (§3.4) expects it at.
- Granularity: page-size multiples (4 KiB on x86-64 Linux, matching this
  crate's `PAGE` constant, `crates/vmem/src/lib.rs:111`; up to 64 KiB on
  some `aarch64` configs per that same file's own `MAX_REALISTIC_PAGE_SIZE`
  doc, lines 75-93). No allocation-granularity distinction the way Windows
  has one.

### 1.4 Cross-platform disparity this design must flag

The two platforms' primitives are NOT equivalent in what they need as a
precondition: Linux's `mremap` can act on an arbitrary page-aligned
sub-range of any anonymous mapping today, with no prior architectural
change; Windows' placeholder-VA path requires switching the ENTIRE
reservation strategy to section-object-backed mappings first (§1.2). Any
future design that wanted to pursue this would either need to accept a
Linux-only implementation (with Windows falling back to the existing copy
path — a real, but real-COST, platform-parity gap for a crate whose stated
target from `crates/vmem`'s own doc comment is being cross-platform "the
one crate whose ENTIRE purpose is the unsafe OS calls" for BOTH `mmap`- and
`VirtualAlloc`-based hosts), or would need to fund the section-object
rearchitecture on Windows as a prerequisite. Neither option is free, and
this asymmetry is itself evidence the mechanism's true cost is higher than
"add one function to `crates/vmem`."

---

## 2. The crux: can one medium block be remapped independently of its neighbors?

### 2.1 First claim to verify — a medium/small segment is ONE shared mapping (TRUE, re-confirmed)

`src/alloc_core/segment_header.rs`'s own module doc (lines 19-32) and
`PageMap`'s doc (lines 177-191, quoted verbatim by R20-3 §1.1 and
re-verified here) state this explicitly: a small/medium segment carves from
**one segment-wide bump cursor** (`SegmentHeader::bump`, `SegmentMeta::
bump_of`/`set_bump`, `segment_header.rs:1138-1152`) shared across **every**
size class ever carved from that segment. `carve_block`
(`src/alloc_core/alloc_core_small.rs:1429-1557`, read in full for this
design) does exactly what R20-3 §1.1 already documented:

```text
let bump = meta.bump_of();
let aligned_bump = align_up(bump, block_size);   // round UP to THIS carve's block_size
if aligned_bump + block_size > SEGMENT { return None; }
... (lazy-commit grow-on-carve, unchanged by anything here) ...
meta.set_bump(aligned_bump + block_size);
... (page-map "first class wins" marking, diagnostic-only) ...
Node::deref(segment, aligned_bump)
```

`PageMap`'s doc is explicit that this is a *shipped design decision*, not an
edge case: "under this substrate's shared-bump-cursor model a page is
mixed-class... `PageMap` is therefore NOT a reliable class oracle." A 256
KiB medium block and a 64-byte small block routinely sit back-to-back in the
same segment, sharing the one `mmap`/`VirtualAlloc` mapping that IS the
segment's single OS reservation (`src/alloc_core/os.rs:65`, `SEGMENT = 1 <<
22` = 4 MiB — one `Segment::reserve` call, one `aligned_vmem::Reservation`,
per segment, confirmed by reading `os.rs` in full for this design).

### 2.2 Second claim to verify — does a promoted medium object (≥256 KiB = ≥64 pages) monopolize whole pages?

**Individually, yes — trivially, since 256 KiB is itself an exact multiple
of the 4 KiB page size (64 pages exactly). The question that actually
matters is whether its NEIGHBORS' bytes intrude on its first/last page**,
and the carve arithmetic answers this precisely:

`carve_block`'s `align_up(bump, block_size)` rounds the START of a new carve
UP to a multiple of the class's OWN `block_size` — **not** up to a page
boundary, and **not** relative to any OTHER class's block size. For the
medium ladder (256/320/384/512/768/1024 KiB — `src/alloc_core/size_classes.rs`
`EXTRAS`, confirmed present at exactly those values in this build), every
one of those sizes happens to be a multiple of 4 KiB (256 KiB = 65536×4KiB;
320 KiB = 81920×4KiB; etc. — trivially true since they are all whole
multiples of 64 KiB), so a medium block's OWN start and end, in isolation,
ARE always page-aligned when carved as a medium class. **But that is not
the question that matters**: the bump cursor is SHARED with every smaller
class too (§2.1) — a segment's carve ORDER interleaves small classes (as
small as 16 B, `size_classes.rs`'s base geometric ladder) with medium
classes, and small-class carves are page-**mis**aligned relative to a
following medium carve's own page-aligned start ONLY IF the small carve
that precedes it does not itself end on a page boundary.

Working the actual arithmetic: suppose the bump cursor sits at some
non-page-aligned offset `X` after a run of small-class carves (entirely
possible — a 96-byte class's blocks do not sum to a multiple of 4 KiB in
general), and the NEXT carve requested is a 256 KiB medium block.
`align_up(X, 262144)` rounds `X` UP to the next 256 KiB boundary — which is
ALSO a page boundary (256 KiB is a multiple of 4 KiB) — so **the medium
block's own start IS always page-aligned**, regardless of what came
before, because `align_up` to a page-aligned `block_size` always lands on
a page boundary. This is the one point in the medium ladder's favor: **the
medium block's START is guaranteed page-aligned by construction** (a
strictly stronger and more useful fact than R20-3's tail-adjacency
analysis needed, because that design only cared about END-of-segment
adjacency, not page alignment at both edges).

**The END is the actual problem.** After the medium block's `block_size`
bytes, `bump` becomes `aligned_bump + block_size` — itself ALSO a page
boundary (same reasoning: page-aligned start + page-aligned multiple length
= page-aligned end). So a medium block's `[start, end)` span, purely by
this arithmetic, is **exactly page-aligned at both edges, in isolation**.
This looks favorable at first glance — but it does NOT mean nothing else
lives on those pages: it means nothing FROM THE SAME CARVE STREAM's prior
carve straddles the boundary. It says nothing about the `mmap`/
`VirtualAlloc`-level VMA that surrounds it (the whole 4 MiB segment is ONE
VMA — §2.1), and more importantly it says nothing about whether the pages
immediately AFTER the medium block (where the bump cursor now sits) get
claimed by the medium object's later grow or by an entirely different
class's carve that happens next, in the exact scenario R10-2's own harness
exercises (16 simultaneously-live objects being carved and grown
interleaved). **The medium object's OWN [start,end) is page-clean at the
moment of its OWN carve** — but that says nothing about the SEGMENT-LEVEL
question a remap actually needs answered, which is the subject of §2.3.

### 2.3 The real blocker: the segment, not the block, is the OS-level mapping unit

Page-alignment of one block's own span is a NECESSARY but NOT SUFFICIENT
condition for remapping it independently. Both `mremap` and Windows'
placeholder mechanism (§1.2/§1.3) operate on ranges *carved out of an
existing OS-level mapping* — and the OS-level mapping here is the **whole 4
MiB segment**, one single `mmap`/`VirtualAlloc` call
(`src/alloc_core/os.rs`'s `Segment::reserve`, thin wrapper over
`aligned_vmem::reserve_aligned`). Neither primitive has any concept of "this
byte range within my VMA is objectA, that one is objectB" — that
distinction exists ONLY in this crate's own `SegmentHeader`/`BinTable`/bump
cursor bookkeeping, which the kernel knows nothing about.

So "is the medium block's own span page-aligned" (§2.2, answered: yes, by
construction) is a necessary but insufficient condition. The SUFFICIENT
condition `mremap`/placeholder-VA actually need is: **"is this span the
ONLY live content currently anywhere in the pages it occupies, AND does the
allocator have some way to tell the kernel to move exactly this span
without disturbing the REST of the segment's one VMA."** For `mremap`
specifically (Linux, §1.3), the man page confirms partial-VMA remap is
mechanically possible (the kernel will split the VMA as needed) — so the
TECHNICAL capability exists on Linux. The blocker is not "can the kernel do
a partial move" (it can, on Linux) but **"can this crate's segment-identity
model tolerate the result"** — which is §3's question, and where the real
NO-GO lives, independent of page alignment.

### 2.4 Answer to the task brief's central question

**Is a medium block's page-aligned span ever the CURRENT single occupant of
its pages, with no live sibling sharing them?** Given §2.2's finding (the
span itself is always page-clean at carve time), the practical answer
reduces to: **yes, in the instant right after that one carve, before
anything else is carved past it** — but this is NOT a stable, checkable
precondition an implementation could gate on cheaply, because unlike
OPT-H's tail-adjacency check (a single cheap comparison against the CURRENT
bump cursor, R20-3 §2.1 precondition 3), "is there STILL nothing else
sharing my pages" would need to hold not just at carve time but at
PROMOTION time (potentially much later, after arbitrary intervening
carves/frees of other objects in the same segment) — and nothing in
`SegmentHeader`/`BinTable`/`PageMap` tracks "which OTHER blocks, if any,
have since been carved into the page range `[my_start, my_end)`" (`PageMap`
is explicitly NOT a reliable class oracle for this, §2.1, and even if it
were, it tracks CLASS not LIVENESS). Determining this at promotion time
would require a new O(segment-size) scan or new bookkeeping this document
did not find any existing hook for — a real, additional design burden
direction (b) would need to solve even if the OS-primitive/FFI and
segment-identity problems (§1, §3) were both somehow resolved.

**Bottom line for point 2 of the task brief:** the neighbor-sharing problem
is real, but its precise shape differs from a naive "objects are randomly
misaligned" story — medium blocks ARE page-aligned by construction (§2.2),
but that alignment is a snapshot fact at carve time with no standing
invariant that it stays true (no sibling later claims the same pages)
through to promotion time, and no existing structure tracks whether it
does. This is a second, independent blocker on top of §3's segment-identity
blocker, not a restatement of it.

---

## 3. Interaction with `SegmentTable` identity — does the model assume a stable base address?

### 3.1 `segment_base_of` is a pure bitmask of the POINTER'S OWN address

`src/alloc_core/os.rs:95-123`, read in full: `segment_base_of(addr) = addr &
!(SEGMENT - 1)` and `segment_base_of_ptr` is the pointer-preserving
equivalent (`ptr.map_addr(|a| a & !(SEGMENT - 1))`). This is not a lookup —
it is arithmetic performed directly on whatever address the caller currently
holds. **There is no indirection layer that could be updated to redirect
"old address → new address"**: every live pointer a caller holds into a
segment, and every subsequent `dealloc`/`realloc` call using that pointer,
independently re-derives the segment base by masking the CALLER'S copy of
the address. If the segment's bytes moved to a new base, EVERY outstanding
pointer into that segment — not just the one object being promoted, but
EVERY sibling object sharing the segment (§2.1) — would still carry its OLD
address, and `segment_base_of_ptr` on that stale address would mask to the
OLD base, which no longer has a live segment there (or, worse, now has some
OTHER live segment there if the VA range got reused).

### 3.2 `SegmentTable` stores and hashes on that exact base pointer

`src/alloc_core/segment_table.rs` (read in full for this design): the
registry is "a fixed-capacity array of segment-base pointers" (module doc,
lines 1-30) plus an open-addressing hash table (`hash_slots`, OPT-B, lines
144-150) whose **key IS the segment base pointer itself** — `contains_base`/
`contains_base_ro` (lines 455, 475) probe the hash table by the exact base
address. There is no separate "segment ID" that decouples identity from
address the way, say, a generational-index scheme would — the base address
**is** the identity, doubly so: once as the bitmask target every `dealloc`/
`realloc` call computes from a live pointer (§3.1), and again as the literal
hash key `SegmentTable` uses for membership/ownership checks
(`dealloc_routing`'s cross-thread `contains_base` probe, per the task
brief's own citation).

### 3.3 Consequence: remapping breaks EVERY live pointer into the segment, not just the promoted one

Combining §3.1 and §3.2: **this crate's entire addressing model assumes a
segment's base address is stable for its entire lifetime.** This is not an
incidental implementation detail that a small patch could work around — it
is the SAME "membrane inversion" design (`segment_table.rs`'s own doc,
lines 1-9) that makes `segment_of(ptr)` an O(1) bitmask instead of a lookup
in the first place: the speed of that O(1) path is PURCHASED by the
assumption that the bitmask always yields the right answer, which requires
the base to never move.

Remapping the WHOLE segment (not a sub-region) to a new address would:
1. Invalidate every OTHER live pointer sharing that segment (§2.1) — their
   holders have no way to learn the new address; the very next `dealloc`/
   `realloc` on any of them masks to the OLD (now-stale or reused) base and
   either silently misbehaves or, if `hardened`'s magic/generation checks
   catch the mismatch, is treated as a foreign/corrupted pointer.
2. Require updating `SegmentTable`'s hash-table entry (remove old base,
   insert new base) — mechanically possible in isolation, but only solves
   the REGISTRY's bookkeeping, not the problem in point 1 (the registry was
   never the bottleneck; the bitmask arithmetic every caller independently
   performs is).
3. Require the OWN thread doing the remap to somehow also fix up its
   *thread-local* fast paths that cache a segment base directly (e.g.
   `AllocCore`'s `small_cur` field, `alloc_core_small.rs:1430`, which is
   exactly the field `carve_block` reads to find "the current small
   segment" — if the segment being remapped IS `small_cur`, every
   subsequent carve into "the same" segment needs `small_cur` updated too,
   in addition to the registry).

**Remapping a sub-region within a segment (moving just the promoted
object's bytes, leaving the rest of the segment's mapping and its base
address untouched) does not have this problem** — the segment's identity
never changes, only some of its internal bytes relocate to a NEW,
independent mapping (effectively: the promoted object silently becomes its
own segment elsewhere, and the OLD segment's now-vacated span becomes an
unusual "hole" other blocks in that segment must never be carved into
again). This is mechanically closer to viable than moving the whole
segment — but it still needs an answer to §2's "how do you know nothing
else currently lives in those exact pages" problem, AND it introduces a new
hazard neither of the OS primitives nor this crate's existing model has any
precedent for: a segment with a page-aligned HOLE in the middle of its
address range that must be tracked as permanently unusable (not simply
"freed," since nothing else was ever carved there and the bump cursor has
already advanced past it) — no existing `BinTable`/free-list/`PageMap`
mechanism represents "these bytes are gone, never carve here again, but
this is not a normal free."

### 3.4 Resolution of the task brief's central tension (point 3)

The task brief poses this as an explicit tension to work through, not
pre-answer: "remapping a WHOLE segment... might be more structurally
compatible... but only helps if the segment holds JUST that one promoted
object (which contradicts the shared-multi-block-segment model)." Having
now read both sides against the actual source (§2.1, §3.1-3.3): **this
tension does not resolve in favor of either whole-segment or sub-region
remap under the CURRENT model** — whole-segment remap is only clean when
the segment is single-occupant, which contradicts §2.1's confirmed shared
model for essentially every real segment; sub-region remap avoids that
contradiction but trades it for an unrepresented "permanent hole"
bookkeeping problem this crate's data structures have no slot for, on top
of still needing §2.4's promotion-time neighbor-liveness check that this
document found no cheap existing mechanism for. **Neither direction is
buildable as a modification of the EXISTING shared-segment model** — this is
the honest resolution: the tension is real, and it resolves as "both sides
of it are blocked," not "one side wins."

---

## 4. Which representation could actually admit this primitive

Per the task brief's explicit instruction to compare directions (a) and (b)
rather than parallel-design them:

### 4a. MediumExtent — a dedicated single-object-per-mapping segment kind

Give a promotable medium object its OWN page-aligned VA region from carve
time — structurally the SAME thing `SegmentKind::Large`
(`segment_header.rs:156-158`, "holds ONE allocation of arbitrary size/align.
No page map") already does, and the same thing `AllocCore::alloc_large`
(`src/alloc_core/alloc_core_large.rs:127`, read for this design) already
implements: **Large is already a "one object owns its whole reservation"
model.** A MediumExtent kind would be, structurally, "Large but for objects
in the 256 KiB–1 MiB range that MIGHT grow past the promotion threshold" —
NOT a new invention, but reuse of the pattern this crate already ships,
applied earlier (at first-alloc time for a candidate object) instead of at
promotion time.

This resolves BOTH of §2's and §3's blockers simultaneously, by
construction, for exactly the reason Large already sidesteps them:
- **No neighbor-sharing** (§2): the mapping holds exactly one object from
  the start — there is no "was anything else ever carved into these pages"
  question, because nothing else is EVER carved into a MediumExtent's
  pages.
- **No segment-identity break** (§3): the SAME identity argument that lets
  Large's whole-segment-is-one-allocation model work today (a Large
  segment's base IS the allocation's effective identity, and it never
  needs an in-place sub-region move because there is nothing else in the
  segment to preserve) applies verbatim. A remap of a MediumExtent, IF one
  were later added, would be a WHOLE-segment remap of a single-occupant
  segment — exactly the clean case §3.4 identified as the only
  structurally compatible one, now actually achieved (not contradicted) by
  construction.

**But this is explicitly a DIFFERENT mechanism with a different, real cost
trade-off**, exactly as the task brief frames it: every object that becomes
a MediumExtent candidate needs its OWN OS reservation at FIRST-ALLOC time
(not a cheap bump-carve sharing an existing segment's slack) — undermining
`medium-classes`' own stated value proposition (density: R10-2 §3.1's
alloc/free density win, the same trade-off R20-3 §5.4 already flagged for
the OTHER un-designed lever, over-allocation headroom). Unlike Large
(where one-object-per-segment is acceptable because Large objects are, by
definition, already big enough that per-object OS-reservation overhead is
proportionally small), a MediumExtent candidate starts as small as 256 KiB
— 1/16th of a segment — so paying a FULL segment's worth of OS reservation
overhead for every candidate, even those that never actually grow past the
promotion threshold, is a materially different economic trade than what
`medium-classes` was built to buy. **This is not analyzed to a verdict
here** — it is a genuinely separate, nontrivial design question (which
objects become MediumExtent candidates? All medium-class allocations, or
only ones showing a growth pattern? What is the reservation-overhead
break-even point?) that the task brief itself scopes as future work, not
this document's job to settle (§0, §6).

### 4b. Remap-in-place within the EXISTING shared-segment model

Per §2 and §3's findings: **this is a dead end, and this document says so
explicitly, per the task brief's own instruction not to force a design
around a blocked direction.** Both the neighbor-liveness check (§2.4, no
cheap mechanism exists to confirm "nothing else shares my exact pages" at
promotion time, only at carve time) and the segment-identity assumption
(§3.1-3.3, base-address stability is load-bearing throughout the addressing
model, not a soft convention) block this direction independently of each
other — either alone is sufficient to block it, and both are present. A
sub-region remap (§3.3's second paragraph) is the least-bad variant of this
direction, but it still needs §2.4's unsolved liveness check AND introduces
an unrepresented "permanent hole" concept with no existing data-structure
slot. This document does not recommend pursuing 4b further under the
current segment model; the LCM/tail-adjacency-style closed-form argument
that closed OPT-H (R22-6) has a real analogue here — the resource remap-
in-place would need (a page-isolated, promotion-time-verified, single-
occupant span within an otherwise-shared mapping) is structurally rarer
than tail-adjacency was, not more common, because it additionally demands
persistent isolation through time, which nothing in the shared-bump-cursor
model was ever built to guarantee or even track.

### 4c. Explicit comparison table

| | 4a: MediumExtent | 4b: remap-in-place, shared segment |
|---|---|---|
| Neighbor-sharing problem (§2) | Solved by construction (one object, one mapping) | Unsolved — no promotion-time liveness check exists |
| Segment-identity stability (§3) | Preserved (same pattern as Large) | Broken (whole-segment) or needs new "permanent hole" bookkeeping (sub-region) |
| New FFI needed (§1) | Same remap primitive question applies EQUALLY — MediumExtent only fixes the segment-model blocker, not the FFI gap | Same FFI gap |
| Cost model change | Yes — per-candidate-object OS reservation at first-alloc, not promotion time (density trade-off, undermines part of `medium-classes`' own value prop) | None to the existing model (if it worked) — but it doesn't (§2, §3) |
| Buildable as an incremental patch? | No — a new segment kind, new carve path, new promotion logic | No — blocked outright |

**This table is the honest output of the task brief's point 4 instruction**:
4a is not "the winner" in some unqualified sense — it is the ONLY direction
that is even STRUCTURALLY compatible with a remap primitive, at the cost of
being a materially larger, separately-scoped redesign with its own
unresolved economic question (density vs. speed), not a refinement of the
existing promotion mechanism.

---

## 5. A concrete Stage-1 measurement plan

Per this project's established cheap-counters-before-implementation
discipline (R17-10 §5.1, R20-3 §6, R21-2's own precedent — read as the
template): **the cheapest possible diagnostic does not touch the OS-remap
question at all** — it tests 4a's PRECONDITION for being worthwhile before
anyone designs 4a in full, and separately, independently, tests whether 4b
has ANY viable footing before more analysis is spent on it.

### 5.1 For 4a (MediumExtent) — the real gating question is workload shape, not mechanism

The MediumExtent redesign only pays for itself if a MATERIAL fraction of
medium-class allocations actually go on to cross
`MEDIUM_REALLOC_PROMOTION_THRESHOLD` (256 KiB, confirmed at
`src/registry/heap_core_free.rs:157`) — if most medium objects are
allocated once and never grow past it, giving them all their own OS
reservation up front is pure density loss for no realloc-speed benefit
(§4a). This is measurable **today, with zero new code**, using counters
this crate already has: `#[cfg(feature = "alloc-stats")]`'s existing
allocation/promotion counters (the same family R14-4/R18-2/R20-2 already
read for their own cache-hit-rate proxies) already distinguish "medium
allocations made" from "promotions triggered." The cheapest possible
Stage-1 probe is: **run the existing `paired_ab_medium_workload.rs` harness
(R10-2's own, zero modification) plus the R20-3-recommended
single-hot-buffer harness (not yet built — same artifact R20-3 §6.1 point 2
already named as the next round's most actionable deliverable, still
outstanding), and read off `promotions_triggered / medium_allocations_made`
for each** — a ratio close to 1 (most medium objects DO eventually promote)
would argue MediumExtent's up-front-reservation cost is well-spent; a ratio
close to 0 (most medium objects never promote) would argue it is not,
independent of whatever the OS-remap mechanism itself could achieve. This
measurement is ALREADY implied by existing counters — no new instrumentation
is needed, only a new invocation and a division.

### 5.2 For 4b (remap-in-place) — falsify the neighbor-sharing assumption directly, if anyone doubts §2's reading

Although §2/§3's source-grounded reading already concludes 4b is blocked,
the cheapest possible empirical check — should a future reader want direct
evidence rather than trusting this document's static-analysis argument —
is a **diagnostic-only, `page-map-diag`-gated counter** (mirroring the
existing `page-map-diag` feature's own diagnostic-only stance, `PageMap`'s
struct doc) added at the EXACT MOMENT a medium object crosses
`MEDIUM_REALLOC_PROMOTION_THRESHOLD` (the existing `try_promote_to_large`
call site, `src/registry/heap_core_free.rs:1276`): walk the promoted
object's own page range `[off, off+old_size)` against `PageMap`'s existing
per-page "first class wins" records (already carried, zero new storage) and
count how many of those pages show a DIFFERENT class than the promoted
object's own — a nonzero count is direct, cheap, existing-data evidence
that a neighbor's carve landed on a page this object also occupies, i.e. a
DIRECT falsification (or confirmation) of §2.4's "no persistent liveness
guarantee" argument, without writing any new counter machinery, reusing
`PageMap` exactly as it already exists. Given `PageMap`'s own documented
caveat ("NOT a reliable class oracle... first class wins" — a page could
show the PROMOTED object's own class even if a later, different-class
neighbor also touched it, if the promoted object carved first), this
would need to be read as a LOWER BOUND on neighbor-sharing (it can
undercount, never overcount) — still directionally decisive: even a lower
bound showing frequent neighbor-sharing would be sufficient to keep 4b
closed; it could never singlehandedly resurrect it, since `PageMap`'s known
undercounting bias only makes a "sharing is common" finding MORE credible,
never less.

**Decision gate:** given §2/§3's already-conclusive static reading, this
Stage-1 measurement for 4b is offered as an OPTIONAL falsification exercise
for a skeptical future reader, not a prerequisite this document itself
treats as unresolved — unlike 4a (§5.1), where the workload-shape question
is genuinely open and this document does not claim to have measured it.

---

## 6. Honest verdict

**NO-GO for 4b (remap-in-place within the existing shared-segment model).**
Both independent blockers this document found — §2.4's unsolved
promotion-time neighbor-liveness check, and §3.1-3.3's load-bearing
base-address-stability assumption threaded through `segment_base_of_ptr`,
`SegmentTable`'s hash keys, and `AllocCore::small_cur` — would each alone be
sufficient to block this direction; both being present makes this an
unambiguous, not a marginal, NO-GO. This is the honest, source-grounded
answer to the task brief's central open question (points 2-3): the
neighbor-sharing problem is real and the segment-identity model genuinely
does assume base-address stability for the segment's whole lifetime, exactly
as the task brief suspected it might.

**CONDITIONAL-GO for 4a (MediumExtent), as a SEPARATE future design task, not
as a continuation of THIS mechanism.** The precondition for proceeding is
squarely economic, not technical: §5.1's workload-shape measurement (what
fraction of medium allocations actually cross the promotion threshold) must
show a material fraction promoting before the up-front per-object
reservation cost is justified. This document does not run that measurement
(per its own DESIGN-ONLY scope) — it identifies that the measurement is
CHEAP (reuses existing `alloc-stats` counters, needs only the
already-separately-recommended single-hot-buffer harness, R20-3 §6.1 point
2, still not built) and names it as the correct Stage 1 for whoever designs
4a next, exactly as R20-3's own CONDITIONAL-GO was gated on a Stage-1
measurement rather than settled by reasoning alone.

**Why this is not a plain NO-GO overall:** unlike a design that finds no
surviving direction at all, this investigation found ONE (4a) that is
structurally sound (reuses an already-shipped pattern — Large's
one-object-per-segment model — rather than inventing a new one) even though
it was not this document's job to fully design or measure. That is the
same "the missing piece is narrowly empirical, not a doubt about soundness"
shape R20-3 §9 used to justify ITS OWN CONDITIONAL-GO over a NO-GO, applied
here to a narrower slice (4a only, not the mechanism this document was
actually asked to investigate, which is 4b/general remap — and THAT part is
unambiguously closed).

**Why 4b is not left CONDITIONAL either:** unlike 4a, where the open
question is a measurable economic trade-off that could in principle
resolve either way, 4b's blockers (§2.4, §3.1-3.3) are architectural
invariants of the CURRENT segment model, not workload-dependent facts a
measurement could move. No conceivable Stage-1 counter changes the answer
to "does `segment_base_of_ptr` assume base-address stability" (yes,
verified by reading the source) or "does anything track promotion-time
neighbor liveness" (no, verified by reading the source) — these are
source-code facts, not empirical ones, which is why §5.2 frames its own
measurement as an optional falsification exercise rather than a genuine
open question the way §5.1's is for 4a.

---

## 7. What does NOT work and why (explicit summary, per R20-3's template)

- **Whole-segment remap under the current shared-segment model** does not
  work: contradicts the confirmed shared-multi-block reality (§2.1) — a
  segment holding many live siblings cannot be relocated on behalf of just
  one of them without invalidating every other live pointer into it (§3.3).
- **Sub-region remap of just the promoted object's pages, leaving the rest
  of the segment in place**, does not work TODAY: needs (a) a
  promotion-time neighbor-liveness check this crate has no existing
  mechanism for (§2.4), and (b) new bookkeeping for a permanent
  "carved-then-vacated, never reusable" hole that no existing
  `BinTable`/`PageMap`/free-list structure represents (§3.3).
- **Retrofitting Windows placeholder-VA onto the existing `VirtualAlloc`-
  based reservation model** does not work at all, independent of the
  segment-sharing question: placeholder-VA/`MEM_REPLACE_PLACEHOLDER`
  fundamentally operates on section-object-backed mappings, and
  `crates/vmem` (confirmed by reading it in full, §1.1) uses plain
  anonymous `VirtualAlloc`, which has no section handle to remap (§1.2). This
  would require rearchitecting the ENTIRE Windows reservation strategy, not
  adding one function.
- **A cross-platform-uniform implementation** does not work even if
  everything else were solved: Linux's `mremap` can act on a sub-VMA range
  today (§1.3); Windows' placeholder mechanism structurally cannot without
  the section-object rearchitecture (§1.2) — any real implementation would
  either be Linux-only or need to fund that Windows-side prerequisite
  separately (§1.4).

## 8. What WOULD work, scoped honestly

- **MediumExtent (§4a)**: a new, Large-like one-object-per-segment kind for
  medium-range objects, applied at first-alloc (not promotion) time.
  Structurally sound (reuses Large's already-shipped pattern) but a
  SEPARATE, materially larger design question than "avoid this one memcpy" —
  its cost model trades per-object OS-reservation overhead for realloc
  speed, the OPPOSITE trade-off direction from what made `medium-classes`
  attractive in the first place (density). Gated on §5.1's cheap,
  already-instrumentable workload-shape measurement before anyone designs it
  in full.

---

## 9. Files/lines this document is grounded in

- `crates/vmem/src/lib.rs` — read in FULL (1394 lines). §1's complete
  inventory of what it does/doesn't provide: `reserve_aligned`/
  `try_reserve_aligned` (lines 353-386, over-reserve+trim, kernel-chosen
  base only), `release` (388-410), `decommit`/`decommit_lazy`/`recommit`
  (416-530), `commit_range` (536-606, `lazy-commit`), `reserve_aligned_huge`
  (689-731, `huge-pages`), `leak_zeroed_pages` (737-782), Windows raw FFI
  (789-1013, `VirtualAlloc`/`VirtualFree`/`GetSystemInfo` only — no
  `VirtualAlloc2`/`MapViewOfFile3`), Unix raw FFI (1019-1313, `mmap`/
  `munmap`/`madvise`/`sysconf` only — no `mremap`).
- `src/alloc_core/os.rs` — read in full. `SEGMENT = 1 << 22` (line 65,
  4 MiB), `segment_base_of`/`segment_base_of_ptr` (95-123, the pure
  address-bitmask identity function §3.1's argument rests on).
- `src/alloc_core/segment_header.rs` — read the module doc (1-37),
  `SegmentKind` (144-175, confirms `Large` is already one-object-per-
  segment — §4a's precedent), `PageMap`'s "mixed-class"/"NOT a reliable
  class oracle" doc (177-191), `BinTable` (992-1069), `SegmentMeta`/
  `bump_of`/`set_bump` (1084-1152, the owner-only single-writer bump
  cursor §2.1/§3.3 point 3 both depend on).
- `src/alloc_core/alloc_core_small.rs:1419-1557` — `carve_block`, read in
  full. The exact `align_up(bump, block_size)` arithmetic §2.2's
  page-alignment finding is derived from.
- `src/alloc_core/alloc_core_large.rs:127-` — `alloc_large`, read for §4a's
  "Large is already one-object-per-segment" precedent.
- `src/alloc_core/segment_table.rs` — read the module doc and struct
  definition (1-161) in full. Confirms the base-pointer-keyed hash table
  (`hash_slots`, `contains_base`/`contains_base_ro` at lines 455/475) §3.2's
  argument rests on.
- `src/alloc_core/size_classes.rs` — `EXTRAS` (grepped, confirms
  256/320/384/512/768/1024 KiB medium ladder, matching R20-3 §5.2's own
  citation).
- `src/registry/heap_core_free.rs` — `MEDIUM_REALLOC_PROMOTION_THRESHOLD`
  (line 157, 256 KiB) and `try_promote_to_large` (1276-, read its doc
  comment) — the exact call site §5.2's optional diagnostic would attach
  to.
- `docs/perf/R20_3_INPLACE_MEDIUM_GROW_DESIGN.md` — read in FULL (this
  document's explicit template per the task brief; §1.1-1.3's shared-bump-
  cursor/tail-adjacency findings are the direct precedent §2's analysis
  builds on and contrasts against).
- `docs/perf/R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` — read in FULL.
  The NULL destination-headroom result this design's §0 opens by
  contrasting against (both prior levers moved the destination/carve
  position; this one investigated moving the copy itself).
- `docs/reviews/2026-07-26-r22-plan.md` §5 item 2 — the synthesis note
  flagging this design's potential overlap with an independently-proposed
  "MediumExtent/PageRun" idea, which this document's §4a explicitly
  addresses per that note's own instruction.
- `docs/perf/OPEN_ITEMS.md` — Active item 1 (OPT-H, already resolved
  R22-6) is the item this design's own §0 contrasts against as "every
  lever attacked destination/carve, never the copy" — this document does
  NOT close or reopen that item; it is a new, separate investigation. No
  `OPEN_ITEMS.md` edit is made by this task (design-only, no new open item
  is created — the 4a follow-up is named here in prose, per the task
  brief's framing as "gated on this design's own verdict before any
  implementation task gets filed," i.e. filing that follow-up task is an
  action for the user/orchestrator, not this document).

---

## 10. CORRECTION (2026-07-27, task #373, R23-4)

### 10.1 What was wrong — §2.4's "promotion-time neighbor-liveness check"

§2.4 (and the §6/§7 verdict text that leans on it) claims the sub-region
remap direction needs an unsolved runtime check: "is there STILL nothing
else sharing my pages" at PROMOTION time, "not just at carve time," because
(§2.4's own words) "nothing in `SegmentHeader`/`BinTable`/`PageMap` tracks
which OTHER blocks, if any, have since been carved into the page range
`[my_start, my_end)`." **This premise is false.** It was re-derived
independently for this correction, reading the current source directly (not
re-trusting the original document's description, and not trusting the task
prompt's description either — both were checked against the file as it
stands today):

1. **`carve_block`** (`src/alloc_core/alloc_core_small.rs:1429-1557`): reads
   `bump = meta.bump_of()` (line 1438), computes
   `aligned_bump = align_up(bump, block_size)` (line 1439), bails with
   `None` if `aligned_bump + block_size > SEGMENT` (line 1440-1442),
   otherwise carves `[aligned_bump, aligned_bump+block_size)` and finishes
   with `meta.set_bump(aligned_bump + block_size)` (line 1535). Every
   carved range starts at or after the bump position that existed
   immediately before the carve, and the bump only ever moves forward by
   this call.
2. **`carve_batch`** (`src/alloc_core/alloc_core_small.rs:1608-` , the
   batched sibling): identical shape — `bump = meta.bump_of()` (line 1619),
   `aligned_start = align_up(bump, block_size)` (line 1620), same
   `> SEGMENT` bail (line 1621), and the batch's own close,
   `meta.set_bump(aligned_start + n * block_size)` (line 1688), is
   explicitly documented (line 1686-1687) as "byte-identical to the final
   `set_bump` of the n-th sequential carve." Also forward-only.
3. **Every call site of `set_bump` in the whole crate** was enumerated
   (`grep -rn "set_bump" src/`): the only two are the two above (forward,
   monotonic) and exactly one other pair, both inside
   **`decommit_empty_segment_impl`** (`src/alloc_core/alloc_core_small_pool.rs:751`
   and `:812`), both `meta.set_bump(payload_start)` — a backward reset to
   the segment's payload start. There is no fourth call site anywhere in
   `src/`.
4. **`decommit_empty_segment_impl`'s reset is gated on the whole segment
   already being empty.** Its two production call paths both require this:
   - `decommit_empty_segment_for_release` (line 730-732), whose only
     production caller is `release_empty_segment_now`
     (`alloc_core_small_pool.rs:429-431`) and the pool-eviction path
     (`release_or_pool_empty_segment`) — both reachable only from
     `dec_live_and_maybe_decommit` (`alloc_core_small_pool.rs:78-113`)
     having already returned `true`, which requires (line 85)
     `live != 0 || base == small_cur || meta.is_decommitted()` to be
     **false** — i.e. `live_count == 0` (the whole segment, every class,
     is empty), the segment is not the current carve target, and it is not
     already decommitted.
   - The other reachable path, `dbg_force_decommit_retain_for`
     (`alloc_core_small_pool.rs:694-707`), is a `#[doc(hidden)]` **test-only**
     hook (used by `tests/alloc_zeroed_virgin_small_skip.rs` to exercise the
     `release_follows == false` retain-and-recommit leg, which the function's
     own doc comment says "has ZERO production callers today"). Its own doc
     is explicit that it does **not** check `live_count` itself and instead
     trusts "the caller is responsible for having emptied the segment
     first." This is a real gap in the abstract ("some code path resets
     bump without checking liveness") worth flagging honestly per this
     project's zero-trust convention — but it is not a production code
     path: it is unreachable from any allocator entry point
     (`alloc`/`dealloc`/`realloc`), gated behind `#[doc(hidden)]` +
     `alloc-decommit`, and invoked only by one integration test that itself
     empties the segment first. It does not weaken the monotonicity
     argument for any real promotion.
5. `dec_live_and_maybe_decommit`'s own doc comment (lines 100-111) already
   states, in the code's own words, why the reset must wait for full
   emptiness: an in-place `set_bump(payload_start)` performed while blocks
   are still live "would push every freed block's offset `>= bump`, making
   a pooled segment's free-list blocks unreachable" — the author already
   understood the reset requires whole-segment emptiness; §2.4's claim that
   nothing enforces this was simply not checked against this function
   before being written.

**Conclusion:** a live (carved-but-not-yet-freed) medium block's byte range
`[start, end)` is exclusive for its entire live lifetime. No other carve can
land inside it (carve only extends the bump forward past everything already
carved, per points 1-2), and no bump reset can revisit it while it's live
either (the reset requires `live_count == 0` for the WHOLE segment, per
points 3-4, which is false while this object — one of that segment's live
blocks — is still live). This is a closed-form, source-derived guarantee,
not something that needs a promotion-time runtime scan. §2.4's blocker does
not exist for the production allocation paths; it was an unverified premise
in the original document, not a re-derivation from the actual carve/decommit
code (the original document cites `PageMap`/`BinTable`/`SegmentHeader`
generically at line 373-374 but never actually reads `carve_block`'s own
control flow or `dec_live_and_maybe_decommit`'s liveness gate — the two
functions this correction's argument rests on).

### 10.2 §3's whole-segment base-address-stability blocker — UNAFFECTED, still real

§3.1-3.3's argument does not depend on §2.4 at all, and this correction does
not touch it. `segment_base_of_ptr` (`src/alloc_core/os.rs:95-123`) is a pure
bitmask of the pointer's own address with no indirection layer; `SegmentTable`
(`src/alloc_core/segment_table.rs`) hashes on that exact base pointer as its
membership key; and `AllocCore::small_cur` caches a segment base directly for
the owner's fast carve path. Moving a segment's OWN base address would still
invalidate every other live pointer sharing it (§3.3, points 1-3) — none of
that reasoning used the neighbor-liveness claim this correction retracts. It
is re-confirmed here, independently, by re-reading `os.rs:95-123` and
`segment_table.rs`'s module doc + `contains_base`/`contains_base_ro` for this
correction: the finding stands exactly as §3 originally stated it.
**Whole-segment remap remains NO-GO under the current segment model.**

### 10.3 The "permanent hole" bookkeeping concern — only PARTIALLY resolved by monotonicity, not fully solved

§3.3's second paragraph flagged a "permanent hole" concern for sub-region
remap: after a promoted object's bytes move away, its vacated byte range
within the still-live segment needs some representation so "other blocks in
that segment must never be carved into again." The task brief that produced
this correction asked whether monotonicity dissolves this too. **It does
not, fully** — investigated here by tracing what TODAY's (unmodified,
memcpy-based) promotion actually does to the source block, since that is the
baseline any remap design must change relative to:

- `try_promote_to_large` (`src/registry/heap_core_free.rs:1276-1343`) does
  `alloc_large` for the new location, `Node::copy_nonoverlapping` to copy the
  bytes, then **`self.dealloc(ptr, old_layout)`** on the OLD block (line
  1338-1341) — an entirely ordinary free through the normal `dealloc_small`
  path. That path pushes the freed offset onto its size class's
  `BinTable` free list (`src/alloc_core/segment_header.rs:999-1069`,
  `BinTable::set_head`).
- Confirmed medium classes participate in this SAME `BinTable` indexing:
  `SMALL_CLASS_COUNT = SIZE_CLASS_TABLE.len() = GEO_COUNT + EXTRAS.len()`
  (`src/alloc_core/size_classes.rs:138,165`), and `EXTRAS` under
  `medium-classes` IS the six-class medium ladder (256 KiB-1 MiB,
  `size_classes.rs:95-111`) — there is no separate free-list mechanism for
  medium; it is the same per-class `BinTable` head/offset scheme small
  classes use.
- **This means today's memcpy-based promotion's freed source offset is NOT
  a permanent hole at all** — it is a completely ordinary free-list entry,
  reusable by a LATER same-class carve/free-list-pop in that same segment,
  exactly like freeing any other medium block. Monotonicity of `bump`
  (§10.1) is irrelevant to this reuse path, because free-list reuse never
  touches `bump` — it hands out an already-carved offset a second time via
  `BinTable`, not via a new bump-advance.
- **The permanent-hole problem is therefore a NEW hazard a remap design
  introduces, not an existing one monotonicity resolves.** If a future
  sub-region-remap promotion moves the object's physical pages away
  WITHOUT running the ordinary `dealloc`/free-list-push (which it must not
  do — pushing the offset onto `BinTable` and later handing it to a new
  allocation would hand out an offset whose physical pages no longer belong
  to this segment, a use-after-remap correctness bug), then that offset
  must instead be marked as permanently unusable — never pushed to
  `BinTable`, never reachable via a future `carve_block`/`carve_batch` (already
  true, by §10.1's monotonicity, for as long as bump does not re-carve it —
  but that is a NECESSARY, not SUFFICIENT, condition, since the free-list
  path is the other way an already-carved offset becomes live-again, and
  monotonicity says nothing about the free-list). **No existing data
  structure represents "carved once, vacated, must never be pushed to
  BinTable or reissued" today** — `BinTable`'s `FREE_LIST_NULL` sentinel
  means "empty list," not "this specific offset is forbidden," and nothing
  in `PageMap`/`SegmentHeader` has a per-offset "permanently retired" bit.
  This is a genuine, still-open bookkeeping gap a remap prototype would need
  to close (e.g. a per-offset "retired" bitmap, or simply never routing a
  remap-promoted object's freeing through `dealloc_small` at all and instead
  accounting for the vacated span only at segment-teardown/decommit time —
  both are viable but neither exists today and neither is designed in this
  correction).
- **Segment-teardown/decommit accounting is comparatively easier and IS
  helped by monotonicity**, though only partially: `dec_live_and_maybe_decommit`
  gates the WHOLE-segment decommit/release/pool-recycle path on
  `live_count == 0` for the ENTIRE segment (§10.1 point 4) — a remap-vacated
  span does not need its own special teardown handling AT THE WHOLE-SEGMENT
  level, because when the segment's `live_count` does reach zero, the entire
  4 MiB payload gets decommitted/released uniformly regardless of which
  offsets were "normal frees" vs. "remap-vacated holes" (`decommit_empty_segment_impl`
  does not distinguish — it resets the whole payload). The open question is
  narrower than "does teardown work at all" (yes, whole-segment teardown is
  agnostic to how any given offset became non-live) — it is specifically
  "does anything, before whole-segment teardown, accidentally hand the
  vacated span's offset back out as if it were a normal free" (yes, today,
  via `BinTable`, unless a remap design explicitly excludes the vacated
  offset from ever being pushed there). **Honest summary: monotonicity
  fully resolves the CARVE-side reuse risk (bump will never re-visit a
  vacated span), but does NOT resolve the FREE-LIST-side reuse risk (an
  ordinary `dealloc` on the vacated span, if a remap design mistakenly ran
  one, would make that span reachable again via `BinTable` — the mechanism
  monotonicity does not touch at all)** — so the "permanent hole" concern is
  real, but its resolution is a matter of "do not free the vacated span
  through the ordinary path," a design discipline any remap implementation
  must observe, not a hole in the allocator's existing data structures that
  blocks the idea outright.

### 10.4 Revised verdict

**NO-GO for WHOLE-segment remap under the current segment model** — the
base-address stability blocker (§3.1-3.3, re-confirmed unaffected at §10.2)
is real, structural, and unaffected by this correction.

**CONDITIONAL-GO for LINUX SUB-REGION remap specifically** — §2.4's
neighbor-liveness blocker is retracted (§10.1): a live medium block's page
range is provably exclusive for its whole lifetime, so no promotion-time "who
else is on my pages" scan is needed. What remains is: (a) the FFI gap (§1.3 —
`mremap` needs a new raw `extern "C"` declaration, not otherwise a blocker,
Linux's primitive is structurally permissive enough per §1.3/§1.4's own
finding), and (b) the free-list-side "permanent hole" discipline (§10.3) —
tractable (do not route a remap-promoted block's old offset through ordinary
`dealloc`), but genuinely unbuilt and unproven today.

**Windows remains NO-GO, on a SEPARATE blocker this correction does not
touch.** §1.2 is explicit and independent of both §2.4 and §3: Windows'
placeholder-VA / `MEM_REPLACE_PLACEHOLDER` mechanism only moves
section-object-backed mappings (`MapViewOfFile3` against a
`CreateFileMapping2`/`NtCreateSection` section handle), and every reservation
`crates/vmem` makes is plain anonymous `VirtualAlloc` with no section handle
to remap — "adopting this mechanism would require re-architecting every
segment's underlying memory model... a foundational change... not an
additive function" (§1.2's own words). This is a backing-store
architecture mismatch, has nothing to do with neighbor-sharing or
segment-identity, and is untouched by this correction. Windows sub-region
remap is therefore NO-GO for a different, orthogonal reason than either of
the two blockers §2/§3 discuss.

**Overall, revised:** NO-GO for whole-segment remap (base-address stability,
§10.2 — unaffected, still real) and NO-GO for Windows sub-region/whole-segment
remap (section-object backing mismatch, §1.2 — untouched by this
correction); **CONDITIONAL-GO specifically for Linux sub-region remap**,
pending a correctness prototype that (i) proves out the free-list-exclusion
discipline §10.3 identifies as the one remaining unbuilt piece, and (ii)
adds the new `mremap` FFI surface §1.3 already scoped. This is narrower than
a blanket "sub-region remap is viable" — it is Linux-only, and it trades one
retracted blocker (neighbor-liveness) for one still-open, but concretely
scoped and tractable, design discipline (never free-list-push a
remap-vacated offset).

### 10.5 Sketch of the minimal correctness-prototype gate (DESIGN ONLY — not implemented)

Following the same checklist-style structure this document's own §4/§5 used
for their design sketches (a bulleted precondition/scope list, not
prescriptive code):

- **Linux-only.** No Windows leg — Windows falls back to the existing
  `try_promote_to_large` memcpy path unconditionally (§1.2/§1.4's
  cross-platform disparity is not resolved by this correction and is not in
  scope to resolve).
- **Page-aligned medium block only.** Restrict the prototype to objects
  whose class is one of the six medium classes (256/320/384/512/768/1024
  KiB, all multiples of the page size, §2.2's finding — unaffected by this
  correction) so the object's own `[start, end)` span is guaranteed
  page-aligned at both edges by construction, exactly as §2.2 already
  derived.
- **Remap exactly the object's byte range**, i.e. `mremap(old_addr,
  old_layout.size(), new_size, MREMAP_MAYMOVE)` with `old_addr` = the
  promoted block's carved address and `old_layout.size()` its exact
  page-aligned span — never a rounded-up-to-VMA-boundary superset that
  could touch a neighbor (there is no neighbor risk during the object's live
  window per §10.1, but the remap call itself must still be scoped to
  exactly this span, not "the rest of the segment," since the segment's VMA
  extends further and other blocks legitimately live elsewhere in it).
- **Destination registration.** The `mremap` return address becomes a new
  Large/extent registration: run the same `stamp_segment_owner` +
  `SegmentTable` registration bookkeeping `alloc_large` already performs for
  a fresh Large segment (§4a's Large-model precedent is the template here,
  even though this is sub-region remap, not MediumExtent) — the moved
  object must become a first-class, independently-tracked Large allocation,
  not merely "some bytes that moved."
- **Vacated-range marking (per §10.3's finding).** The old `[start, end)`
  span must be excluded from `BinTable`'s free-list push that
  `try_promote_to_large`'s current `self.dealloc(ptr, old_layout)` call
  performs — a remap-based promotion must NOT call ordinary `dealloc` on the
  source span. Instead: either (a) a per-offset "retired" marker (new
  bookkeeping, not designed here) that `carve`/free-list-pop paths check and
  skip, or (b) account for the vacated span only passively, at
  whole-segment teardown time (§10.3's finding that whole-segment
  decommit/release is already agnostic to how an offset went non-live) —
  accepting that the vacated span's bytes are simply "not reachable by
  anything, and not tracked as free," until the whole segment eventually
  empties and gets torn down uniformly. Either option needs to be chosen and
  built; neither is built today.
- **The old address must never enter `BinTable` or any free-list.** This is
  the one correctness-critical invariant a prototype's test suite must
  directly assert: after a remap-promotion, no future `alloc` on that
  segment/class may ever return the vacated offset. A regression test
  should carve-promote-then-exhaust-the-class's-free-list and assert the
  vacated offset never reappears.
- **Any error path (remap failure, e.g. `mremap` returning `MAP_FAILED` under
  address-space pressure or a kernel that refuses the move) falls back to
  the existing memcpy move-leg unconditionally** — `try_promote_to_large`'s
  current `alloc_large` + `copy_nonoverlapping` + `dealloc` sequence remains
  the universal fallback; remap is purely an optional fast path attempted
  first, never a replacement that removes the memcpy code.

This sketch is NOT an implementation plan with file/line targets — it is the
minimal shape a future round's correctness-prototype task would need to
fill in, matching this document's own established practice of scoping a
Stage-1/prototype gate without writing its code (§5's own Stage-1 sketch is
the precedent this follows).

### 10.6 Verification performed for this correction

- Read this entire document in full before writing anything (per the
  project's zero-trust convention — not just the §2.4/§3 sections the task
  prompt named).
- Re-read `carve_block` (`alloc_core_small.rs:1429-1557`) and `carve_batch`
  (`alloc_core_small.rs:1608-1721`) in full against the CURRENT source (not
  the line numbers cited in the original document, which had drifted
  slightly) — confirmed both are forward-only bump advances.
- Grepped every `set_bump` call site in `src/` (not just the two the task
  prompt named) to confirm there is no other backward-reset path — found
  exactly the two forward call sites plus the two backward-reset call sites
  inside `decommit_empty_segment_impl`, no others.
- Traced every caller of `decommit_empty_segment_impl` /
  `decommit_empty_segment_for_release` / `unpool_if_present`'s sibling paths,
  and found a genuine caveat the task prompt's framing did not name:
  `dbg_force_decommit_retain_for`, a `#[doc(hidden)]` test-only hook, resets
  bump WITHOUT checking `live_count` itself (trusting its one test caller to
  have emptied the segment first). Reported honestly in §10.1 point 4 as a
  real gap in the abstract, assessed as not weakening the production-path
  argument (unreachable from any real allocation entry point).
- Traced `try_promote_to_large` (`heap_core_free.rs:1276-1343`) end to end to
  determine what today's promotion actually does to the source block —
  found it calls ordinary `dealloc`, which confirmed the "permanent hole"
  question needed to be answered relative to `BinTable`/free-list reuse, not
  relative to `bump`/carve reuse (§10.3's central finding).
- Confirmed medium classes share `BinTable`/`SMALL_CLASS_COUNT` indexing
  with small classes (`size_classes.rs:138,165,95-111`) — establishing that
  a promoted medium object's freed offset is reachable via the SAME
  free-list mechanism small blocks use, not a separate medium-only
  structure.
- Re-read §3.1-3.3 in full to confirm the base-address-stability argument
  does not depend on anything this correction retracts — confirmed
  independent (§10.2).
- Re-read §1.2 in full to confirm the Windows blocker (section-object
  backing) is a separate, third blocker untouched by this correction
  (§10.4's Windows paragraph).
- No `src/`, `benches/`, or `Cargo.toml` file was modified. This correction
  is documentation-only, per the task's own scope.
