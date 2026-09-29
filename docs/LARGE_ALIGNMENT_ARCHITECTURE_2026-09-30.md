# Large alignment: explicit geometry and canonical identity

Decision after the XXA consultation on snapshot `da5ff7af`. This is an
implementation plan for R7 P3-2, not a claim that over-segment alignment is
supported. The current rejection guard remains until all lifecycle paths are
converted and verified.

## Representation

Keep one Large lifecycle, with four separate values:

| Value | Role |
|---|---|
| Reservation token | Original OS pointer, release length, release alignment/backend |
| Usable root | Allocator-origin pointer for metadata and its useful reserved window |
| Payload | Allocation-start returned to the caller |
| Address key | Numeric payload address masked to the segment granularity; lookup only |

A key or subtraction from the caller's payload pointer cannot recover
metadata provenance. The owner table stores the original root; remote code
receives only an independently pinned publication capability.

For over-aligned Large, prefer a page-aligned metadata root immediately
before an aligned payload. Let `M` cover all metadata, page-rounded, and let
`R` be useful reserved capacity. Reserve a raw span of checked size `R + A`,
select `P = align_up(raw_address + M, A)`, and derive `U = P - M` from the
original reservation pointer. Commit only the useful `[U, U + C)` window
where the backend supports it. Every bound and pointer offset is checked
before access. Raw address-space overhead remains proportional to alignment.

Store an explicit `payload_offset`; alignment constrains the starting
address, not a rounding of every requested byte to `A`. Small/Primordial
roots retain segment alignment. The release token retains its original
backend alignment, including under Miri; an interior usable root is not an
OS release pointer.

## Index and lifecycle conversion

Use one owner hash entry per live segment: `payload key -> segment id ->
canonical root and geometry`. Keep internal access by owned identity separate
from lookup by issued address. Unregister removes that identity and its cache
entry before release. Do not add a second alias without changing the hash
capacity and metadata footprint.

The route directory receives the actual payload key and allocation-start,
allows page-aligned Large roots, and retains a stable useful capacity end.
That bound is not permission to free arbitrary interior pointers. Mix high
address bits into owner hashes and route shard selection: high alignments
otherwise collapse the current low-bit indices.

Fresh, moving realloc, promotion, zeroed allocation and NUMA fallback share
the same geometry and registration rules. Moving realloc allocates and
registers the destination before copying and publishing the old free; failure
preserves the old allocation. In-place growth checks `payload_offset + size`
against committed and reserved frontiers and changes logical size only after
successful commit.

Initially bypass cache for the over-aligned backing kind. A later cache
implementation must account for retained raw VA slack, preserve the release
token, validate payload alignment and capacity, and create a fresh route
incarnation before reissue.

## Publication contract retained

The consultation also suggested a descriptor predecessor queue to avoid Large
table scans. That suggestion is not adopted: the previously accepted strict
trim contract requires completed publications to be visible without waiting
for a producer paused between queue writes. Keep terminal sidecar publication
and bounded owner cuts. A future pending-identity index must preserve those
properties; it cannot reinstate the old publishing-gap protocol.

Sidecar publication conveys one consume obligation; the owner-only physical
phase controls cache/release. Reclaim must consume the explicit obligation
once, rather than infer permission from two independent state machines.

## Implementation order and verification

First convert identity/geometry and own lookup while retaining the alignment
guard. Then convert route registration and all foreign header reads, introduce
the checked biased OS window, and connect allocation/realloc/NUMA/release.
Remove the guard last.

Required tests include successful alignments `SEGMENT`, `2 * SEGMENT` and
`16 * SEGMENT`; distinct reservation/root/payload values; narrow reborrow and
address-only diagnostics under Miri; allocation/commit/registration rollback;
own/foreign/batch/fallback frees; realloc prefix and OOM preservation; and
paused producer lifetime. Huge arithmetic boundaries use injected geometry
and failure models, not giant real allocations. Deterministic probe counters
check lookup complexity. No performance or RSS benefit is claimed yet.
