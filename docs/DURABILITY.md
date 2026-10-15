# DURABILITY — ultra-long-run counter inventory

The two failure modes that only appear on ultra-long (weeks/months, thread-per-
request, hot-segment) runs are:

- **(a) Unbounded growth / leaks** — caught by the soak tests
  (`examples/soak_xthread.rs`, `examples/rss_probe.rs`) and by `AllocStats`:
  watch `segments_reserved_total − segments_released_total` (net live
  segments) and `foreign_or_unroutable_frees`. Not the subject of this doc.
- **(b) Counter wrap** — a monotonic/wrapping/saturating counter or cursor
  reaching its width boundary and either corrupting order, re-colliding a
  generation/ABA tag, or silently truncating. **This doc is the authoritative
  inventory of every such counter.**

Current state (re-inventoried against `src/` on 2026-10-15): **no
correctness-load-bearing counter can wrap.** Each one is either structurally
bounded below its width, `checked_*` with a terminal outcome (an error, a
permanently disabled hint, or a process abort), or a diagnostic whose wrap
loses only a count. The segment/heap rings, the abandoned-segments stack, hash
tombstones and the hardened X7 per-granule generation were removed with the
mechanisms that owned them; their rows and wrap tests are gone with them (see
git history before 2026-10-15 for the old table).

## Master table

| counter | file:line | width | class | boundary reachable? (arithmetic) | verdict | covered by |
|---|---|---|---|---|---|---|
| `RouteDirectory::next_incarnation` | `src/registry/segment_route/directory.rs:705` | `AtomicU64` | monotonic, `checked_add` | 2^64 route registrations — unreachable | **no wrap**: exhaustion returns `RouteError::IncarnationExhausted`; an old incarnation is never reissued | `tests/r6_route_directory.rs::incarnation_exhaustion_never_wraps` |
| Route entry `refs` (pin count) | `src/registry/segment_route/directory.rs:32` | `AtomicUsize` | counted, `checked_add` | bounded by concurrent pins (threads × in-flight frees) ≪ `usize::MAX` | **no wrap**: overflow aborts (a free must never lose its route) | `tests/r6_route_directory.rs` |
| Large generation (packed in `LargeState::word`) | `src/alloc_core/segment/segment_header/terminal_words.rs:32` (`MAX_LARGE_GENERATION = u64::MAX >> PHASE_BITS`, `PHASE_BITS = 3`) | 61-bit generation above a 3-bit phase in a `u64` | monotonic, bounded | 2^61 cache reuses of one Large reservation — unreachable | **no wrap**: `next_large_generation` returns `None` at the limit and the reservation retires | `tests/r6_terminal_large_state.rs::reduced_width_exhaustion_retires_without_wrap` |
| `SaturationHint` version | `src/registry/bootstrap/saturation.rs:12` (`LAST_VERSION = u64::MAX − 3`) | `AtomicU64` (bit 0 = saturated, rest = version) | monotonic, sealed | 2^63 availability publications — unreachable | **no wrap**: at `LAST_VERSION` the hint becomes `DISABLED` forever and the claim path falls back to the scan | `tests/r7_p3_claim_scan.rs::version_exhaustion_disables_hint_and_keeps_scan_backstop`, `near_max_publication_during_scan_cannot_restore_saturation` |
| `Registry::count` | `src/registry/bootstrap/registry.rs:74` | `AtomicU32` | monotonic (high-water) | capped at `MAX_HEAPS = 4096` | bounded | `tests/r11_p3_registry_claim_selector.rs` |
| `Registry::scan_cursor` | `src/registry/bootstrap/registry.rs:76` | `AtomicU32` | wrapping (`fetch_add`) | 2^32 full-registry scans — reachable in principle | **wrap-harmless**: used only as `cursor % count` for a scan start; a wrap shifts the start, the scan still visits every slot, ownership comes only from the slot-state CAS | claim-scan tests (`tests/r7_p3_claim_scan.rs`) |
| Maintenance pass cursor | `src/registry/heap_registry/maintenance.rs:25` | `usize` (worker-local) | reduced `% count` every pass | never exceeds `count` | bounded | `tests/r8_maintenance_registry.rs` |
| `HeapSlot::generation` | `src/registry/heap_slot.rs:198` | `AtomicU64` (widened from `u32`, W7a) | monotonic | 2^64 claims of one slot — unreachable | **production-dead**: written on claim, read only by test accessors; the stale-TLS hazard is guarded by the `TORN` sentinel (`global::tls_heap`) | `tests/regression_counter_wrap.rs::generation_crosses_u32_boundary_as_u64` |
| `SegmentHeader` `owner_gen` (packed in `owner_state`) | `src/alloc_core/segment/segment_header/segment_header_impl.rs:63` (`OWNER_GEN_SHIFT = 32`) | 32-bit field in bits [32..63] of `owner_state` | constant | always 0 since the adoption substrate was removed (task #97 / R4-5) | not a counter; retained for layout stability | — |
| `SegmentHeader::live_count` | `src/alloc_core/segment/segment_header/segment_header_impl.rs:313` (`add_live`/`sub_live` in `segment_header_meta_fields.rs:49`,`71`) | `u32` | counted, `checked_*` | bounded by `SEGMENT / MIN_BLOCK` = 262144 ≪ 2^32 | **no wrap**: exceeding the block limit or underflow aborts the process | `tests/regression_batch_flush.rs` |
| `SegmentHeader::bump` | `src/alloc_core/segment/segment_header/segment_header_impl.rs:272` | `usize` | monotonic | bounded by `SEGMENT` (4 MiB) | bounded | `tests/regression_bump_direct_refill.rs` |
| `SegmentTable::count` | `src/alloc_core/segment/segment_table/segment_table_impl.rs:208` | `u32` | monotonic (high-water) | capped at `MAX_SEGMENTS = 4096` | bounded | `tests/segment_table_o1.rs` |
| `AllocCore::large_cache_seq` / `CachedLarge::seq` | `src/alloc_core/alloc_core/alloc_core_impl.rs:402` | `u64` | monotonic wrapping (`wrapping_add`) | 2^64 large-cache deposits — unreachable | bounded-by-width (FIFO-oldest picked by `seq`) | `tests/regression_large_cache_multi_size_cycle.rs` |
| `os::SEGMENTS_RESERVED_TOTAL` / `RELEASED_TOTAL` / `RESERVE_FAILED_TOTAL` | `src/alloc_core/platform/os.rs:52`,`57`,`84` | `AtomicU64` | monotonic (`fetch_add`) | 2^64 — unreachable | diagnostic (net = reserved − released) | soak tests, `tests/regression_*_no_leak.rs` |
| Per-slot `tcache_hits` / `large_cache_hits` | `src/registry/heap_slot.rs:124`,`132` | `AtomicU64` | wrapping (diagnostic) | 2^64 — unreachable | diagnostic; aggregation into `AllocStats` is `saturating_add` | `tests/regression_percounter_perheap_aggregation.rs` |
| Registry diagnostics (`CONFIG_CONFLICTS`, `FALLBACK_TCACHE_HITS`, `FALLBACK_LARGE_CACHE_HITS`, `CLAIM_SOURCES[5]`, `DEFAULT_CONFIG_RESULTS[2]`) | `src/registry/heap_registry/counters.rs:27`,`46`,`54`,`465`,`467` | `AtomicU64` | monotonic (diagnostic) | 2^64 — unreachable | not correctness-load-bearing | `tests/r1_10_fallback_hits_counted_in_stats.rs` |
| Maintenance worker `PASSES` / `FALLBACK_VISITS` | `src/global/maintenance_service.rs:36`,`38` | `AtomicU64` | monotonic (diagnostic) | 2^64 — unreachable | not correctness-load-bearing | `tests/r8_autonomous_maintenance.rs`, `tests/r8_os_release_oracle.rs` |
| `HARDENED_LARGE_NOOP_COUNT` | `src/registry/heap_core/free/dealloc.rs:179` | `AtomicU64` | monotonic (diagnostic, `hardened`) | 2^64 — unreachable | not correctness-load-bearing | `tests/regression_hardened_large_kind_own_free.rs` |
| `AllocStats` fields (`tcache_hits`, `large_cache_hits`, `decommit_calls`, `large_xthread_reclaimed`, `segments_reserved/released_total`, `heaps_claimed_high_water`, `foreign_or_unroutable_frees`, `config_conflicts`) | `src/global/alloc_stats.rs:69`–173 | `u64` snapshot | monotonic/saturating (diagnostic) | 2^64 — unreachable | not correctness-load-bearing | `tests/regression_percounter_perheap_aggregation.rs`, `tests/regression_w3_stats_aliasing_miri.rs` |

## Reachability arithmetic (the genuinely-reachable one)

- **`Registry::scan_cursor` (`u32`).** One increment per full-registry scan, so
  a wrap needs 2^32 scans — plausible only on a very long run with constant
  saturation. The wrap is harmless by construction: the cursor only picks a
  scan start (`cursor % count`), the scan visits all `count` slots from there,
  and only the slot-state CAS grants ownership. No order or identity depends
  on the cursor's absolute value.

Every other reachable boundary from the old table belonged to a removed
mechanism (ring cursors, abandon tag, X7 granule generation) or was widened
(`HeapSlot::generation`, W7a).

## THE RULE — adding a new monotonic/wrapping counter

A new monotonic / wrapping / saturating counter or cursor is added **only** with
BOTH:

1. **A row in the master table above** — width, class, the reachability
   arithmetic, the verdict.
2. **A boundary-crossing test OR a compile-time bound assert:**
   - *Boundary test* (for reachable, widened or sealed counters): preset the
     counter near its limit and drive it across. Templates:
     - `tests/r6_route_directory.rs::incarnation_exhaustion_never_wraps` —
       construct the directory at `u64::MAX − 1`, issue the last incarnation,
       and assert the next registration is an error, not a wrap.
     - `tests/r6_terminal_large_state.rs::reduced_width_exhaustion_retires_without_wrap`
       — drive a reduced-width generation to its limit and assert retirement.
     - `tests/r7_p3_claim_scan.rs::version_exhaustion_disables_hint_and_keeps_scan_backstop`
       — preset a sealed version word and assert the hint disables itself
       while the scan still finds slots.
     - `tests/regression_counter_wrap.rs` (W7a) — preset `HeapSlot::generation`
       near `2^32`, cross the boundary, assert no truncation. Fails if the
       widening is reverted.
   - *Compile-time bound assert* (for structurally-bounded counters): pin the
     bound so a future config bump fails to compile rather than silently
     wrapping. Template: `pack_large_state`'s
     `assert!(generation <= MAX_LARGE_GENERATION)` in a `const fn`
     (`src/alloc_core/segment/segment_header/terminal_words.rs:48`).

## Widening must be Ir-neutral (W1 judge)

Before widening any counter, **prove zero hot-path cost**. Precedent: the W7a
widenings (`generation` → `AtomicU64`, the then-registry `TaggedIndex` tag
repack) were judged Ir-neutral by the W1 instruction-retired judge — **−4 Ir on
the cold path, byte-identical hot path**. A widening that moved the hot-path
Ir is not accepted; narrow the change or move the counter off the hot path
first.

---

*Cross-refs: [ARCHITECTURE.md](ARCHITECTURE.md) §10 (docs index),
`tests/regression_counter_wrap.rs`, `tests/r6_route_directory.rs`,
`tests/r7_p3_claim_scan.rs`.*
