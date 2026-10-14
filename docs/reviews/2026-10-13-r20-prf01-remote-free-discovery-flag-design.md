# R20 PRF-01: remote-free discovery as a user opt-in flag — design (ox, read-only)

**Disposition (2026-10-14):** not built. The C‴ default-path gate returned NO-GO (`docs/perf/R20_PRF01_C3_RUN01_REPORT.md`), and the owner rejected both the C‴ default and this flag fallback ("удаляем и флаг и функционал"). The design below is kept as a record only.

Owner decision (2026-10-13): make candidate C'' optional behind a flag the user sets when configuring the
allocator, default = the safest scenario; ask @ox for the best possible design. This file records the answer
of the `ox` agent (claude-opus-5-5, xhigh), run read-only in an isolated worktree at `origin/main`
`d5c8b970`; nothing was built, run or edited. It is a design proposal, not an accepted plan; the owner
decisions it needs are listed in §9. Notation: **[O]** observed in files/commits/reports, **[I]** inferred
from code reading, not measured.

---

## 0. Verdict

Ship PRF-01 as a per-heap opt-in mode, `RemoteFreeDiscovery::SummaryHint`, with default
`RemoteFreeDiscovery::FullScan` (exactly today's behaviour). Do **not** just flag C'': as written it
cannot be zero-cost when off, and it is not the best "on" mode. Ship a redesigned variant C‴ built on four
decisions.

1. **Surface.** A `#[non_exhaustive]` enum on the existing const config surface:
   `Profile::remote_free_discovery(..)` and `LargeCacheConfig::remote_free_discovery(..)`, used through
   `SeferAlloc::with_profile` / `with_config`. No Cargo feature and no type parameter.
2. **Mode carrier.** Each segment gets an immutable route tag, written at registration into the byte that
   `Entry.kind` already occupies. Producers and the owner branch on it at the same comparison they already
   make for Large vs Small. The off path therefore executes the same instruction count as today [I]; a
   zero-tax gate proves it.
3. **Summary storage.** The summary words live in the **dead metadata prefix of the existing `pending`
   array**, `pending[0..64)`. Those words cover owner metadata and can never carry a publication.
   `size_of::<SmallSidecar>()` stays 41 984 in both modes, so the +512 B and the +512 Ir memset are gone.
4. **"On" producer.** Keeps the terminal `pending.fetch_or(AcqRel)` with its result unused (x86 `lock or`,
   same as today). It then applies the contract's existing C′ form: a relaxed `load` of the summary word,
   and `fetch_or(Release)` only if the bit is clear. It does **not** use C''s `prev == 0` test, because
   using the `fetch_or` result turns the hot `lock or` into a `lock cmpxchg` retry loop on x86. Price:
   about +5 Ir per remote free [I], against +13.03 measured for C'' [O].

Supporting decisions:

- **Owner side.** A separate single-purpose summary scanner type, so fixed scans pay nothing per word. C''
  checked the summary mode on every word.
- **Scans that never read the summary.** Strict trim, bounded maintenance and the OOM rescue always use
  fixed scans that start at the first payload word.
- **Prerequisite.** Strict trim must start at the payload start (finishing Patch S) before the summary can
  live in the dead prefix.

Expected results:

| | off vs main | on vs main |
|---|---|---|
| Per remote Small free | 0 Ir | about +5 Ir [I] |
| Per discovery, per cut, strict trim, maintenance | 0 Ir | discovery about as cheap as C'' (B/A 0.07–0.10) [I] |
| Per heap construction | ≤ about 5 Ir to carry the mode byte [I] | about 0 [I] |
| Sidecar bytes | +0 | +0 |

## 1. Flag surface, default, documentation contract

### 1.1 Public API

New file `src/alloc_core/config/remote_free_discovery.rs` (directory goes from 5 to 6 entries):

```text
#[cfg(feature = "alloc-decommit")]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RemoteFreeDiscovery { #[default] FullScan, SummaryHint }
```

Plumbing:

- **`LargeCacheConfig`** (`large_cache_config.rs:176-215`):
  - new field `remote_free_discovery: Option<RemoteFreeDiscovery>`;
  - `pub const fn remote_free_discovery(self, m) -> Self`;
  - `pub(crate) const fn resolved_remote_free_discovery()`, with `None` resolving to `FullScan`;
  - same shape as the existing `pool(..)` (`:418`) and `resolved_pool` (`:504`) [O].
- **`Profile`** (`profile.rs:360-364`): a third axis `remote_free`, builder `remote_free_discovery(..)`,
  forwarded in `to_config` (`:458-466`). `Profile::DEFAULT` must stay byte-identical to
  `SeferAlloc::new()`.
- **Re-exports** from `alloc_core/mod.rs` and `lib.rs` (next to `lib.rs:510-518`).

Usage (non-doctest):

```text
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_profile(
    Profile::new().remote_free_discovery(RemoteFreeDiscovery::SummaryHint));
```

Config mismatches:

- `AllocCore::live_config_matches` (`lifecycle.rs:232-249`) must compare the resolved mode; its own "drift
  hazard" note requires this [O].
- A recycled slot claimed with a different mode then counts in `CONFIG_CONFLICTS` (`claim.rs:161-173`) [O].

Documented limits:

- Builds without `alloc-decommit` have no config at all, so they are always `FullScan`.
- Without routes (standalone `AllocCore`), the flag has no effect.

### 1.2 Why FullScan is the safest default

- **Fewest producer accesses.** The producer's last access is one terminal RMW (`sidecar_bitmap.rs:115-124`
  [O]). Nothing touches the sidecar after the linearization point, which is the narrowest
  lifetime/provenance surface in the contract (`REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md:41` [O]).
- **No dependence on an unproven hint protocol.** Every discovery cuts every word up to the high-water mark
  (`bitmap_scan.rs:38-73` [O]), so reuse latency for remotely freed blocks needs no proof. Item 82's
  actual-type load/skip-race proof is still open (`OPEN_ITEMS.md:3048` [O]).
- **Failure mode.** The worst case of FullScan is bounded CPU, O(high-water) per discovery. A hint-protocol
  bug would instead strand frees until a full pass, which shows up as RSS growth.
- **Contention.** A producer touches only its own pending line. Hint mode also reads a summary line shared
  by 512 KiB of payload, and native contention there is unmeasured (item 70; Mode 2 (ii) deferred [O]).
- **Coverage.** FullScan is the mode every existing Loom, Miri and oracle suite and the shipped history
  cover.

### 1.3 Documentation contract (rustdoc on the enum and the `Profile` method)

1. **Effect.** Changes only how the allocation path finds Small blocks freed by other threads.
2. **Unchanged.**
   - Memory safety and the ledger.
   - The inclusion guarantees of `trim_current_thread`, thread-exit trim, the maintenance worker and the
     OOM rescue: all keep full fixed scans.
3. **Cost model.**
   - Each remote Small free into a `SummaryHint` segment does one extra relaxed load after publishing.
   - At most once per pending word per discovery epoch, it also does one extra atomic RMW.
   - Discovery visits at most 64 summary words per segment plus the flagged words, instead of every word up
     to the high-water mark.
4. **Scope.**
   - Resolved per heap slot: the first bind wins, and conflicts are counted in `config_conflicts`.
   - Applies to the segments registered by that heap.
   - A free into another heap's segment follows that segment's mode, not the freeing thread's.
5. **Evidence.** Only deterministic instruction counts exist (cite the gate report). Native latency and
   contention are unmeasured.
6. **When not to use it.** Many producers freeing blocks of 1 KiB or more into one heap's hot region: every
   such free starts a new pending word, so the extra RMW lands on a shared summary line.

## 2. Zero cost when off

Three mechanisms make the off path free:

- the mode byte is the comparison the code already makes;
- the summary storage already exists;
- each scanner is a single-purpose type.

### 2.1 Route tag (`src/registry/segment_route/kind.rs`)

`RouteKind` stays the public 3-variant enum, with discriminants pinned by `#[repr(u8)]`. A crate-private
`RouteTag` replaces `Entry.kind` (`directory.rs:24-34` [O]): u8 for u8, so the size is unchanged.

| Tag | Value | Test |
|---|---|---|
| Small | 0 | `t < 2` (fixed small family) |
| Primordial | 1 | `t < 2` (fixed small family) |
| Large | 2 | `t == 2` (same as today) |
| SmallHint | 3 | `t > 2` (hint family) |
| PrimordialHint | 4 | `t > 2` (hint family) |

`RoutePin::kind()` decodes the tag back to `RouteKind` for the public API (tests, census).

### 2.2 Every touch point on the off path

| Off-path site | Today [O] | Design | Δ |
|---|---|---|---|
| Producer Large/Small dispatch | `match pin.kind()` (`routing.rs:49-61`), compiles to `== Large` | `tag.is_large()`, the same single compare | 0 [I] |
| Producer sidecar check in `publish_small` | `small_sidecar()` re-checks `kind == Large` (`directory.rs:186-198`, called from `pin.rs:46-50`) | `small_publication()`: `t < 2` takes the fixed `publish` (unchanged code); everything else goes to a `#[cold]` arm (Hint, or NotSmall → false). One compare replaces another. | 0 [I] |
| Producer RMW | `fetch_or`, result unused | unchanged `SidecarBitmap::publish` | 0 |
| Owner discovery | `scan_small_from` → `small_sidecar()?` (`route_slots.rs:200-218`) | `discover_small_from(.., sink)`: the same index and root checks, then the tag test in the Large check's slot. The fixed arm runs today's loop (`find_segment.rs:155-172`) through an inlined closure; the Hint arm is out of line. | 0 per discovery [I] |
| Per word cut | `BitmapScan::next_cut` | untouched. C'' added an `Option` check per cut (`edf90675:bitmap_scan.rs:21,50` [O]); a separate `SummaryScan` avoids it. | 0 |
| Strict trim and bounded maintenance | fixed scans | unchanged after PR-3 | 0 |
| Registration | `SegmentKind` → `RouteKind` constants (`route_slots.rs:114-119`) | the tag comes from a pair stored in `RouteSlots`: a load replaces an immediate | 0±1 per segment [I] |
| Heap construction | `new_inner(owner)` → `attach_owner` (`lifecycle.rs:255,276`) | the mode byte is passed into `RouteSlots::new`, which picks the tags once | ≤ about 5 Ir per heap [I]; the only non-zero item by design |
| Memory | `SmallSidecar` 41 984 B [O] | identical; `Entry` identical; `RouteSlots` +2 B per heap | 0 per segment |

### 2.3 How "zero" is proven

- Gate Z (§6): every budget is 0, measured against the program noise floor `T`:
  - per-operation paths: slope with `T_s`;
  - producer cell: ΔIr / 4096;
  - construction: `budget_K = 0 + T_s`;
  - sizes: exactly 0.
- The runner keeps a disassembly of `publish_foreign` and `drain_segment_sidecar` in both arms as a
  descriptive receipt.
- The Gate Z cells then join `IAI_BASELINE` permanently, so a future rustc that "un-folds" the compare
  shows up as a deterministic Ir delta.

## 3. Scope and mixed mode

**How the mode flows.** The user sets it per `SeferAlloc` instance. It takes effect per heap slot (first
bind wins, `core.rs:230-275` [O]), and each segment carries an immutable route tag stamped at
registration.

**How the producer learns the mode without a new load.** It already loads `Entry.kind` and compares it,
once at `routing.rs:49` and again at `directory.rs:187` [O]; the tag *is* that byte. All producer paths
funnel through `publish_foreign` → `RoutePin::publish_small`: `global_alloc.rs:80`, `batch.rs:133`,
`routing.rs:19`, and the dbg hook `sidecar_test_hooks.rs:36-42` [O].

**Owner side.** The owner dispatches on the same tag, not on its heap config. So producers and owner can
never run different protocols on one sidecar, which makes mixed mode safe by construction.

| Situation | Behaviour |
|---|---|
| Segment recycling across heaps | Does not happen. Pooled segments stay registered in their heap (`alloc_core_small_pool_impl.rs:221` [O]). A released segment is unregistered; a later reservation at the same address is a fresh registration with a fresh zeroed sidecar (`directory.rs:56`, freed at `:109-121` [O]) and that owner's tag. Heap slots recycle whole (`core.rs` docs [O]). |
| Fallback heap | Uses the first fallback config. The idle-fallback synchronous path (`routing.rs:35-44`) publishes nothing. |
| Cross-thread frees between heaps of different modes | Follow the target segment's tag; the freeing thread's mode is irrelevant. |
| Conflicting instances racing for one slot | The slot keeps the first mode; `config_conflicts` increments; the debug readback reveals it. |
| Large segments | Always the Large tag; no change. |

## 4. Correctness with the mode on

### 4.1 Protocol

Notation: granule `g`, word `w = g/64`, summary word `s = w/64`, summary bit `w%64`.

**Producer:**

- **P1.** `e = classes.encoded(g)` (Acquire); reject if `e == 0`. Unchanged (`sidecar_bitmap.rs:119-121`).
- **P2.** `pending[w].fetch_or(1<<(g%64), AcqRel)`. This is the linearization point; the result is unused.
- **P3.** `if pending[s].load(Relaxed) & (1<<(w%64)) == 0 { pending[s].fetch_or(1<<(w%64), Release) }`.
  The setting branch is `#[cold]`, out of line.
- **P4.** Drop the pin.
- The P3 address is pure arithmetic on the `pending` base loaded **before** P2. Nothing is loaded after P2
  except the summary word itself.

**Owner discovery:**

- For each summary index `s` in `[start/64, ceil(end/64))`: `m = pending[s].swap(0, AcqRel) &
  range_mask(s)`.
- For each set bit of `m`: `pending[w].swap(0, AcqRel)`, then pop the records.
- The whole scan is always consumed, as it already is today (`find_segment.rs:155-172` [O]).

**Strict trim, bounded maintenance, OOM rescue:** fixed cuts over `[start, end)` only.

### 4.2 Invariants [I] (to be proven by the models in §4.3)

- **I1. No lost publication; eventual discovery.**
  - If producer X's P2 lands after a discovery's cut of `w`: the cut's release synchronizes with P2's
    acquire, so the discovery's earlier summary swap happens-before X's P3 load. By coherence that load
    sees either the cleared value or a bit set after the swap. Either X sets the bit, or another producer's
    bit for `w` was set after the swap and stays set until the next discovery.
  - If P2 lands before the cut, the cut consumes the bit.
  - Full-scan cuts do not clear the summary, so a bit seen as set after such a cut really is set.
  - `Release` on the P3 RMW is required: an owner whose swap reads the hint must also see P2 in its cut.
- **I2. Exactly-once consumption.** Cuts are swaps; unchanged.
- **I3. No premature finalization.** A stranded bit keeps its ledger credit; unchanged.
- **I4. Lifetime.** P3 touches only the sidecar pinned by the `RoutePin`, and the sidecar is freed at the
  last pin (`directory.rs:215-229` [O]).
- **I5. Summary words are never read as pending bits.**
  - Owner metadata is at least 64 KiB: two 32 KiB per-granule bitmaps (`segment_header_layout.rs:28-46`,
    `segment_bitmap.rs:50` [O]). So `payload_start_word ≥ 64 = SUMMARY_WORDS` [I].
  - Class leaves are 4 KiB (`leaf_classes.rs:16` [O]) and the metadata end is page-aligned
    (`segment_header_layout.rs:70-72` [O]), so every metadata granule has class 0 and P1 rejects it.
  - After PR-3 every scanner starts at or after `start`.
  - Enforcement:
    - a `const` assert in `small_sidecar.rs`: both meta-end values divided by 1 KiB are ≥ `SUMMARY_WORDS`;
    - a release-mode `start_word ≥ SUMMARY_WORDS` check in the summary scanner (Hint only).
- **I6. Isolation.** Fixed-tag sidecars never run P3 or a summary scan; tags are immutable.
- **I7. Dropped out-of-range bits are harmless.**
  - Words below `start` never carry content.
  - Words at or above the high-water mark cannot: bump only grows while any credit is outstanding.
- **Defence in depth.** The OOM rescue (`find_segment.rs:111-113`) passes `forced` into the drain and uses
  a **fixed** scan even for Hint routes, so a hint bug cannot cause a spurious OOM.

### 4.3 What must be proven, and how

1. **Actual-source Loom refinement** (answers item 82's "actual-type proof" requirement):
   - route `sidecar_bitmap.rs`, `bitmap_scan.rs` and the new `sidecar_bitmap/summary_scan.rs` through a
     re-exported atomic alias in `alloc_core/mod.rs` (a `use` line, allowed in `mod.rs`);
   - move the geometry const asserts to `small_sidecar.rs`;
   - reuse the existing source-include harness (`tests/r6_sidecar_bitmap.rs:5-20` [O]) with stubbed loom
     atomics and a reduced `SEGMENT`: 3 words = 1 summary/dead word + 2 payload words;
   - scenarios:
     - two producers into the same word and into different words, with two discoveries;
     - a producer paused after P2: discovery may miss it, strict trim must include it;
     - a full-scan cut racing a hint;
     - a stale bit set by another producer;
     - the final union equals the published set, with no duplicates;
   - negative controls that must fail:
     - hint before the linearization point;
     - summary load before P2;
     - owner clears the summary after the cut;
     - producer never sets a clear bit;
     - discovery drops in-range bits;
     - strict trim reads the prefix;
   - a `Relaxed` hint RMW may stay green under Loom and must not count as proof
     (`2026-10-11-solution-research-scan-prf01.md` §5.2 [O]);
   - keep the existing reduced shadow `tests/loom_sidecar_summary.rs` [O].
2. **Written argument.** Record the I1–I7 chain in `CROSS_THREAD_STATE_MACHINES.md` and in the contract
   amendment.
3. **Miri** (Stacked and Tree Borrows) on actual routed types:
   - a hook pauses the producer after P2;
   - while it is paused, the owner retires the segment with strict trim;
   - on resume, the producer writes into the still-pinned sidecar.
4. **Quiescent invariant oracle** in native tests: after all producers join and before discovery, every
   non-empty payload word of a Hint sidecar has its summary bit set.
5. **Both-mode coverage**, each mode in its own process (R26-4 lesson: first-to-bind-wins):
   - run in both modes: `r6_terminal_owner_drain`, `r6_terminal_owner_drain_model`,
     `loom_terminal_owner_drain`, `r8_owner_sidecar_miss`, `r9_bounded_background_maintenance`,
     `r11_ph4c_ingress_consume_exactly_once_oracle`, `r11_p3_small_sidecar_routes`,
     `r19_prf01_sidecar_oracle`;
   - `r11_ph5b_c5_sidecar_ordering_pinned`: extend to pin the new orderings;
   - `r6_sidecar_bitmap` and `sidecar_granule_index_exhaustive`: stay fixed-mode only (their fixtures use
     offsets below 64 KiB); add a Hint-mode fixture variant;
   - port C''s tests to C‴: `r19_sidecar_summary_discovery.rs`, `r20_prf01_summary_transition_miri.rs`,
     `loom_r20_prf01_summary_transition.rs` (all from `edf90675`) [O].
6. **Mixed-mode test.** Two `SeferAlloc` instances (FullScan and SummaryHint) on different threads, cross
   frees in both directions. Everything must be reclaimed, and the tag census must show both tag kinds.

## 5. Reducing the "on" price

### 5.1 Producer: C'' +13.03 Ir/free [O] → C‴ about +5 [I]

Where C'''s cost comes from (code [O], cost [I]):

| Source in C'' | Cost |
|---|---|
| `fetch_or` result used (`edf90675:sidecar_bitmap.rs:187`): x86 `lock cmpxchg` loop instead of `lock or`; natively it can also retry on a contended pending word | about +4 |
| `prev == 0` test (`:164`) | +2 |
| Summary address and bit computed *before* the linearization point on every free and held across the RMW (`:147`) | about +3–4 |
| `publish_sequence` builds and returns a discarded `Option<(&AtomicU64, u64, bool)>` (`:140`ff) | about +1–2 |

C‴ removes all four:

- the linearization point stays `lock or`;
- the summary address comes from the same base register;
- the bit test is written `(s >> (w & 63)) & 1`, which compiles to `bt`;
- the setting branch is out of line;
- the pause test hook shares the `publish` and hint halves instead of using a const-generic loop.

Estimate: `mov, shr, load, bt, jcc` ≈ +5, plus about 0.05 for summary transitions in the same-word cell
[I]. This is C′'s protocol (R19 +9.03 [O]) without C′'s overheads. Worst case: with blocks of 1 KiB or more,
every free starts a new word and pays the RMW; that case gets its own cell and budget (§6).

### 5.2 Construction: C'' +641 [O] → about 0 [I]

- **+512 is layout.** C'' appended 512 B (`edf90675:small_sidecar.rs:17,21`). R19 attributed the identical
  +512 to `__memset_avx2_unaligned_erms` [O]. The dead prefix adds zero bytes.
- **About +129 is scanner code** (not decomposed in v2, report §4 [O]):
  - the per-scan `Option` field and the per-cut check (`edf90675:bitmap_scan.rs:21,50`);
  - the extra `summarized` parameter and branch (`edf90675:segment_table_impl.rs:349-361`);
  - per-word mask arithmetic on every summary word (`edf90675:bitmap_scan.rs:75-79`).
  A single-purpose `SummaryScan` with precomputed first/last masks and dispatch folded into the tag compare
  removes them. On a fresh segment `start == end`, so the scan returns immediately.

Out of scope, recorded for later:

- **OPT-04 is probably a no-op.** C'' dropped the 4096-word zeroing loop and construction still rose by
  +641. That suggests LLVM already elided those stores after `alloc_zeroed` [I].
- **The real construction lever is the sidecar `calloc`.** At about 1 Ir per byte (R19 memset accounting
  [O]), the 41 984 B `calloc` is about 42 k of the 49 356 Ir in arm A of the construction cell [O, I].
  Recycling or lazily zeroing sidecars is the real lever; it should be its own future item.

## 6. Verification plan (CI only)

### 6.1 Common setup

- **Execution.** `r20-gates.yml` dispatch, run only on the owner's explicit command. Reuse the v2 runner,
  judge and verifier (`scripts/r20_prf01_sidecar_summary_{run,gate}.mjs`,
  `r20_prf01_verify_artifacts.mjs` [O]). Commit artifacts by size tier.
- **Entry point.** `SeferAlloc` via `GlobalAlloc` → routed `HeapCore`, the layer the flag ships at
  (`with_profile`). Producer cells free from a joined second thread through `GlobalAlloc::dealloc` →
  `publish_foreign`.
- **Preregistration.** Frozen with hashes before any candidate build exists; any change is a recorded
  pre-measurement re-freeze.
- **Identities.**
  - A1/A2: the `main` SHA after PR-3.
  - C0: A built under a different path label.
  - B1/B2: the candidate commit SHA.

### 6.2 Gate S (PR-3: strict trim starts at the payload start, `perf(runtime)`)

- New strict-trim cell (none exists in `benches/perf_gate_iai.rs` [O]): ΔIr = −(`start_word` × per-word
  cost) per Small/Primordial segment.
- Oracle: `strict_words_visited` equals Σ(end − start) in B and Σ end in A.
- Every other cell: Δ = 0, or a saving that the oracle explains.

### 6.3 Gate Z (zero tax: candidate with the default config vs main)

Cells:

- producer, same word (`r19_prf01_joined_producer_same_word`, 4096 frees);
- producer, distinct words (new: 4096 remote frees of 1 KiB blocks);
- producer, remote Large free (new);
- the direct construction cell `r20_prf01_start_one_alloc_free_16b`;
- discovery: `multiseg_cold_256k`, `seg_cycle_decommit_256k`, `growth16_{n,2n,4n}`,
  `r19_prf01_publication`;
- the 4 hot families at N/2N/4N;
- strict-trim and bounded-maintenance cells;
- the mimalloc cells.

Budgets, all 0:

| Family | Rule |
|---|---|
| per-operation families | slope `\|s_B − s_A\| ≤ T_s` |
| producer cells | `\|ΔIr\| ≤ T` (i.e. < T/4096 Ir per free) |
| construction | `≤ 0 + T_s` |
| `sizeof(SmallSidecar)`, `Entry` | Δ exactly 0 |

Oracle (oracle build only, under the dedicated cfg, per the R35 lesson [O]):

- all summary counters are 0;
- `pending_words_visited` equals A in every cell;
- the tag census is all fixed;
- the resolved-mode readback is `FullScan`;
- the `config_conflicts` delta is 0.

### 6.4 Gate P (mode on: candidate with `SummaryHint` vs main)

The "on" cells are built only in B, under a runner-set cfg. Their bodies are identical to the paired A
cells, and the runner hashes the shared body. The same B build also gives an on-vs-off consistency check
inside B.

| Check | Budget |
|---|---|
| `multiseg_cold_256k` B/A | ≤ 0.25 |
| `seg_cycle_decommit_256k` B/A | ≤ 0.25 |
| `growth16` slope of B | ≤ 2.5 |
| same-regime net, `r19_prf01_publication` | B ≤ A |
| producer, same word | ≤ +6 Ir/free (unchanged) |
| producer, distinct words | owner sets it before data. Proposal: ≤ +10 Ir/free. Principle: one test-and-set load plus exactly one RMW per newly non-empty word. |
| construction | `budget_K = 0 + T_s` |
| hot families | slope-neutral |
| `sizeof` | Δ 0 |

Oracle per arm:

- the tag census: every Small/Primordial route of the measured heap carries a Hint tag;
- the requested and resolved mode readback;
- the `config_conflicts` delta is 0, with one process per cell;
- `pending_words_visited` B ≪ A, and `summary_words_visited > 0`;
- `summary_hint_loads` = the number of remote frees;
- `summary_hint_rmw` = the expected number of transitions;
- `discovery_records_reclaimed` is equal in A and B for the publication cell;
- the quiescent invariant holds at the end of every producer cell.

Native axis: DEFERRED, as in v2 [O].

## 7. Implementation plan (separate PRs)

1. **PR-1 — `docs`.**
   - The design record.
   - Contract amendment at `CONTRACT.md:41`:
     - the summary lives in the dead prefix of the same pinned sidecar;
     - its address derives from the base loaded before the linearization point;
     - strict trim, maintenance and rescue never read it;
     - each segment's tag is immutable;
     - add the §12 negative controls.
   - Update `CROSS_THREAD_STATE_MACHINES.md`.
   - OPEN_ITEMS item 82 card: "redesign as opt-in (owner 2026-10-13)".
2. **PR-2 — `test(loom)` plus an import-only refactor.**
   - The atomic alias; the geometry asserts moved.
   - An actual-source Loom model of the *current* fixed protocol. It also closes the known gap where
     `loom_sidecar_bitmap.rs` models load-before-swap instead of production's swap [O].
   - Register the model in the Loom inventory (`scripts/loom.mjs`, `ci.yml`).
3. **PR-3 — `bench` + `perf(runtime)`.**
   - Add strict-trim and bounded-maintenance cells.
   - `sidecar_drain.rs:213` → `scan_small_route_from(index, base, hw, start.min(end))`.
   - Delete `SegmentTable::scan_small_route` (`segment_table_impl.rs:335-343`) and `RouteSlots::scan_small`
     (`route_slots.rs:181-198`); this frees lines in the 994-line file.
   - Add an oracle counter and `tests/r20_strict_trim_payload_start.rs`.
   - Run Gate S.
4. **PR-4 — `bench`.**
   - The Gate Z/P harness and cells.
   - Oracle counters.
   - Judge modes (zero-tax / opt-in) and self-test mutants.
   - A frozen preregistration.
5. **PR-5 — `perf(opt-in)` candidate branch**, merged only after PR-6:
   - **(a) Config surface** (§1.1), plus the mode passed through `new_inner` →
     `attach_owner(owner, primordial, mode)` → `RouteSlots::new`.
   - **(b) Route tag.**
     - `kind.rs`: `RouteTag` and its predicates.
     - `directory.rs`: `Entry.tag`, crate-private `register_with_discovery`, `small_publication()`, census
       by tag.
     - `registration.rs`.
     - `route_slots.rs`: the tag pair and `discover_small_from`.
     - `segment_table_impl.rs`: a thin wrapper.
   - **(c) Producer.**
     - `routing.rs:49` uses `is_large()`.
     - `pin.rs`: the dispatch, plus the test hook `unsafe fn dbg_publish_pending_only_for_test` →
       `PausedHint` (doc-hidden, safe `resume`, no `NonNull`).
     - `small_sidecar.rs`: `publish_hinted`, `summary_scan_from`, the dead-prefix const assert, a
       quiescent-invariant observer.
     - `sidecar_bitmap.rs`: `SUMMARY_WORDS` and `publish_hinted`.
   - **(d) Owner scanner.** New `sidecar_bitmap/summary_scan.rs`, included via `#[path]`, because
     `remote_bitmap/` already has 7 entries [O].
   - **(e) Drain and rescue.** The drain in `find_segment.rs` uses the visitor and passes `forced`.
   - **(f) Hooks.** Entries in `scripts/verify-dbg-hook-safety.mjs`.
   - **(g) Tests** from §4.3, plus the Miri and Loom inventories.
   - **(h) Docs.** README configuration section (the unsafe list is unchanged), `INTEGRATION.md`,
     `ARCHITECTURE.md`, `CHANGELOG`.
6. **PR-6 — `bench`.**
   - One CI dispatch for Gate Z and Gate P.
   - Reports, summary CSVs, the OPEN_ITEMS update, the round manifest.
   - Merge PR-5 only if Gate Z is GO and Gate P is GO (opt-in).

### Risks

| Risk | Consequence | Mitigation |
|---|---|---|
| The compiler stops folding the tag compare | Gate Z fails | Move the Hint arms out of line with `#[cold]` and check the disassembly receipt. Last resort: a compile-time gate for the Hint arms, losing runtime selectability. |
| "On" producer cost exceeds +6 Ir | NO-GO as specified | The owner records an explicit override that states the price (the arbiter's option [O]), or keeps the item closed. |
| Owner metadata shrinks below 64 KiB (e.g. RAD-5 `MagazineBitmap` removal) | The const assert stops the build | Use a coarser summary, e.g. 1 bit per 2 words. |
| A hint liveness bug | Stranded frees and RSS growth, "on" mode only | Bounded by strict trim, maintenance and rescue; caught by the actual-source Loom model and the quiescent-invariant oracle. |
| First bind wins | The user believes the mode is on, but the slot is FullScan | `config_conflicts`, the debug readback, the docs. |
| Native fan-in for blocks of 1 KiB or more | Unmeasured | Disclosed in the docs; the native axis is deferred. |
| File and directory limits: `segment_table_impl.rs` 994/1000, `directory.rs` 957/1000, `segment_route/` 13 entries (grandfathered) | A split becomes necessary | No new files there; split only by pure moves. |
| Per-mode tests share the global registry | The wrong mode is tested silently | Separate processes per mode. |
| `SmallSidecar::scan()` (starts at word 0) used on a Hint route | It would cut summary words; the pop aborts loudly | It is a test-only API; after PR-3 no production scanner starts at word 0. |

## 8. Rejected alternatives

| Alternative | Why rejected |
|---|---|
| C'' as-is behind a flag | Per-cut branch in fixed scans, a CAS loop, +512 B: neither zero-tax nor the best "on" mode. |
| Cargo feature | Unification lets any dependency switch it on; it is not a configuration flag; it grows the powerset. Kept only as the fallback for the folding risk. |
| `SeferAlloc<P>` type parameter | The registry, directory and fallback are process-global, non-generic statics, so mixing modes would still be possible. |
| A mode byte in the sidecar, or a new `Entry` field | Adds a load and a test on the off producer path. |
| A 512 B summary allocated only when on | "On" then pays +512 Ir of memset; the dead prefix strictly dominates. |
| `lock xadd` to detect the 0→non-0 transition | A remote double free would carry into a neighbouring bit and could reclaim a live block. |
| Load before the RMW on the pending word | Provably strands a word when an owner cut lands between the load and the RMW. |
| `bt [mem], reg` in inline asm | Unsafe, outside the Rust memory model, and microcoded on real hardware. |
| A periodic fixed-scan backstop in "on" mode | Unnecessary, given the proof and the rescue path. |

## 9. Owner decisions needed before any data

1. Approve the PR-1 contract amendment.
2. Approve PR-3 as a separate `perf(runtime)` change. If declined, strict trim can dispatch on the tag
   instead, still at zero cost [I].
3. Set the Gate P budget for the distinct-words producer cell. The others stay: same-word +6, `budget_K` 0,
   `sizeof` Δ 0.
4. Decide whether an opt-in may ship with "GO (deterministic axes), native DEFERRED", disclosed in the
   rustdoc. The v2 judge never issues GO while native is deferred [O].
5. Choose the Gate Z tolerance: program-standard `T` with its floor of 10, or no floor at all (exact
   identity).
