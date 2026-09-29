# Remote-free contract revision: provenance-safe sidecar ingress

Status: design decision, **not an implemented release guarantee**. Supersedes the
payload-intrusive ingress proposed in `REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`;
its ledger, lease, bounded-trim and ownerless-progress obligations remain in force.

## Reason for the revision

`GlobalAlloc::dealloc` supplies a pointer to the user's allocation, not a
capability over the entire segment. Masking its address and applying
`with_addr` does not restore allocator-owned provenance. The current foreign
route also reads header fields through that masked pointer. A correct redesign
must not use the caller's pointer to access header, block slack or other
reservation bytes. `SegmentTable` must cache only a root read from its own
stored entry, never the lookup pointer supplied by the caller.

## Chosen boundary

- A process-stable route directory accepts the numeric address as a **key**.
  Lookup pins an already-published sidecar before reading its fields. A
  load-then-increment of a removable pointer is insufficient. Registration
  and descriptor allocation happen before the first user-visible issue;
  `dealloc` neither allocates nor constructs TLS state.
- The descriptor retains the original allocator-owned usable root separately
  from the OS reservation/release token. It also identifies the reservation
  incarnation, owner and Small/Primordial/Large mode. It is not created by
  casting the numeric lookup key back to a pointer.
- The sidecar and reservation have distinct lifetimes. Closing directory
  admission prevents new pins; a descriptor with existing pins is retired
  and reclaimed later without waiting in strict trim. Outstanding allocation
  credits, not descriptor pins, prevent premature reservation release.
- The remote Small publisher touches only the sidecar: its class is recorded
  by the owner at issue, and `pending[word].fetch_or(mask, AcqRel)` is its
  terminal publication. After this operation it may release its *sidecar*
  pin, but may not access the reservation or its ownership ledger.
- An exclusive owner visits each bitmap word once with `swap(0, AcqRel)` and
  reclaims the returned bits exactly once. A per-word cut replaces the former
  single per-segment exchange; a free published before trim entry is covered
  because its word has not yet been visited. New publications after a word's
  exchange wait for a later pass. Budgeted maintenance persists the remainder
  outside releasable reservation memory.
- Large uses a per-reservation phase/generation word in the protected
  descriptor. `LIVE`, `PENDING`, `CONSUMING` hold one instance credit;
  `CACHED`, `INITIALIZING`, `RELEASED` hold none. No predecessor link is
  written to a Large reservation by a foreign publisher.

The first prototype uses a byte-per-granule class table for mixed-class
Small segments. At 4 MiB / 16 bytes this is 32 KiB pending bitmap plus
256 KiB class metadata per fully materialized segment, before allocator and
VM rounding overhead. These are logical sizes, **not RSS measurements**.
Class-homogeneous spans are the target representation if they reduce real
cost without weakening lifetime or locality guarantees. A denser packed
class table needs a separate atomicity/aliasing proof.

## Directory proof obligations

The directory implementation may choose its internal synchronization, but
must demonstrate all of the following before use by `GlobalAlloc`:

1. Lookup cannot dereference a descriptor that removal has freed. Pin
   acquisition and removal/admission closure form one protocol; incarnation
   is rechecked after pinning. Counters and generations never wrap into a
   valid old state.
2. Registration publishes a fully initialized descriptor before returning
   any allocation it covers. OOM/failure occurs before issue and preserves
   ownership for rollback. Every correct free has a pre-existing route and
   metadata slot; no queue-full or allocation-failure return is permitted.
3. A producer paused before terminal publication keeps its reservation live
   through the owner-only outstanding credit. A producer paused after it
   holds at most a sidecar pin: the owner may reclaim and unmap the reservation
   before that producer returns.
4. Owner free-list/cache paths never use the returned user pointer as an
   allocator root. Their pointers derive from the stored usable root and
   numeric block index. Wide `&mut` borrows cannot overlap remote-accessible
   atomic or sidecar bytes.
5. Directory lookup, pin release, fallback and bootstrap are non-recursive
   under `#[global_allocator]`. The producer progress class and contention
   cost are measured and documented, not inferred from the bitmap RMW alone.

## Acceptance gates

Run actual `GlobalAlloc`-path tests with requested sizes 1–7, narrow
reborrows, magazine reuse, owner exit followed by the last remote free and
no new allocator call, a producer paused on both sides of publication,
Large cache reuse/exhaustion, sidecar incarnation reuse, and strict trim under
ongoing post-cut frees. Exercise minimal `alloc-xthread`, `production`,
`hardened`, fastbin, decommit, directory/NUMA and supported OS profiles.
Miri must inspect both aliasing models where supported; Loom/model negative
controls must distinguish wrong ordering and premature reclamation. Native
tests cover OS decommit/release. Measure latency and RSS against the previous
route before claiming an optimization.

Until the directory, issue/free conversion, strict sweep, fallible autonomous
service and these gates are complete, **R6-01/R6-03 remain open and release is
NO-GO**. The existing intrusive inbox remains an unconnected experimental
primitive, not an alternative production ingress.
