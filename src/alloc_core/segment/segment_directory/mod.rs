//! [`SegmentDirectory`] — per-class `class_nonempty` bitmap sidecar for
//! O(1) directory-driven segment lookup (task R7-A1).
//!
//! ## Design
//!
//! A flat 2-D bitmap: `class_nonempty[SMALL_CLASS_COUNT][WORDS_PER_CLASS]`
//! where each `u64` word covers 64 segment-table slot indices. Bit `j` of
//! word `w` in class `c` is set iff `SegmentTable::base_at(w * 64 + j)` is a
//! live Small/Primordial segment whose `BinTable::head(c) != FREE_LIST_NULL`.
//!
//! ## Owner-only
//!
//! The directory is written ONLY by the owning thread's alloc/dealloc path
//! (the same single-writer discipline `AllocCore` itself enforces). All
//! fields are plain `u64`, not `AtomicU64` — no cross-thread reader ever
//! touches this bitmap (the A4 dirty-routing mechanism is a SEPARATE
//! structure; this bitmap is the owner's private index into its own
//! `SegmentTable`). This is P-rule-correct and eliminates any atomic-RMW
//! overhead from what will become the inner loop of the A3 directory lookup.
//!
//! ## Layout
//!
//! ```text
//! class_nonempty_by_node: [[[u64; WORDS_PER_CLASS]; SMALL_CLASS_COUNT]; NODE_BITMAPS]
//!
//! WORDS_PER_CLASS = MAX_SEGMENTS / 64 = 64
//! SMALL_CLASS_COUNT = 49 (default) or 55 (medium-classes)
//! NODE_BITMAPS = 1 (non-NUMA) or MAX_NODES + 1 (numa-aware)
//!
//! Total (non-NUMA): 1 * 49 * 64 * 8 = 25,088 B = 24.5 KiB (default)
//!                    1 * 55 * 64 * 8 = 28,160 B = 27.5 KiB (medium-classes)
//! Total (numa-aware, MAX_NODES=8):
//!                    9 * 49 * 64 * 8 = 225,792 B = 220.5 KiB (default)
//! ```
//!
//! ### R11-6 NUMA node-indexed variant
//!
//! Under `numa-aware`, the outer `[NODE_BITMAPS]` dimension indexes per-node
//! bitmaps. Bucket `[0, MAX_NODES)` holds segments whose `node_id` maps to
//! that bucket via the dense `node_ids` registration table (R12-2 — see
//! below). Bucket `[MAX_NODES]` (the "unknown" bucket) holds segments with
//! `node_id == NO_NODE_RAW`, or whose node id was observed only after all
//! `MAX_NODES` bucket slots were already claimed by OTHER distinct nodes.
//! Under non-`numa-aware`, `NODE_BITMAPS == 1` and the structure is
//! byte-for-byte the pre-R11-6 flat 2-D bitmap (`[0]` is the only bucket) —
//! no memory tax on non-NUMA builds. See
//! `docs/perf/R10_6_NUMA_DIRECTORY_JUDGE.md` §3.2 (Approach A) for the
//! design. The node-indexed variant was wired in task R11-6 to close the
//! ~140× scan-cliff that existed because the directory lookup was compiled out
//! entirely under `numa-aware` (every free-list miss fell back to an O(S)
//! linear scan with two-pass NUMA preference).
//!
//! ### R12-2: dense node-id -> bucket mapping (was a direct clamp)
//!
//! R11-6 originally mapped `node_id` to a bucket by using it as a DIRECT
//! array index clamped at `MAX_NODES`: any `node_id >= MAX_NODES` landed in
//! the shared unknown bucket regardless of how many distinct nodes were
//! actually in play. `numa-shim` scans up to 64 real OS node ids
//! (`crates/numa-shim/src/lib.rs`), so on any host exposing node ids 8..63 this
//! silently defeated the R11-6 locality optimisation for every thread pinned
//! to one of those nodes: a thread on node 9 would prefer a node-10 segment
//! over its own node-9 segment, because both physically landed in the same
//! unknown bucket and the scan visits unknown before ascending foreign
//! buckets (design-defect R12-2, P0). R12-2 replaces the direct-index clamp
//! with a dense `node_ids: [u32; MAX_NODES]` registration table: a node id
//! claims the next free bucket slot the first time a segment on that node is
//! registered (`SegmentDirectory::node_bucket_mut`), so `MAX_NODES` now
//! bounds the number of DISTINCT nodes tracked simultaneously, not the raw
//! OS node id value. Only once MORE than `MAX_NODES` distinct node ids have
//! actually been observed does a node fall back to the unknown bucket. See
//! `node_bucket_mut`'s doc comment for the full design rationale (including
//! why `MAX_NODES` itself was NOT raised to 64: a 65-bucket sidecar costs
//! ~400 KiB per heap vs. ~56 KiB today, a 7x fixed tax paid by every process
//! even when only 2-3 buckets are ever populated).
//!
//! ## Lazy materialisation
//!
//! NOT placed inline in every `AllocCore` / `HeapSlot`. Instead, a plain
//! `*mut SegmentDirectory` in `AllocCore` starts null and is populated via
//! the same M5-clean direct-VM reservation pattern R6 established in
//! `registry::bootstrap` / `registry::heap_overflow`
//! (`aligned_vmem::reserve_aligned` + `mem::forget`). The directory is
//! owner-only (single-writer, single-reader — the owning thread), so no
//! `AtomicPtr` or CAS protocol is needed (unlike the `HeapOverflow` sidecar,
//! which is cross-thread and needs CAS-publish). The VM reservation and raw
//! pointer dereference live in the existing `alloc_core::os`
//! `#![allow(unsafe_code)]` seam (`reserve_directory_sidecar` /
//! `deref_directory_sidecar[_mut]`).
//!
//! The sidecar is materialised ONLY after `table.count() >=
//! DIRECTORY_MATERIALIZE_THRESHOLD` (= 32, chosen from A0 data — see
//! `docs/perf/R7_DIRECTORY_BASELINE.md` §3). Below the threshold the
//! current linear scan is used unchanged — this is A1 scope only (storage +
//! lazy materialisation + rebuild); the directory is NOT queried for lookups
//! yet (that is A3).
//!
//! Sidecar OOM is NOT allocator OOM: on reserve failure, the pointer stays
//! null and the mechanism is simply off (falls back to the linear scan).
//! Never abort.
//!
//! Pointer stable until heap death; `mem::forget`-leaked for the process
//! lifetime (same discipline as `RegistryChunk` / `HeapOverflowSidecar`).

#[path = "segment_directory_impl.rs"]
mod segment_directory_impl;
pub(crate) use segment_directory_impl::*;
