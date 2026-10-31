# G1 R123: the AS-1 free lane without frame, bound checks and kind compare — `perf(runtime)`

Runtime change to plain `--features production`: the own-thread small free
lane (`HeapCore::dealloc_fast`, reached from `SeferAlloc`'s
`#[global_allocator]` `dealloc`) and the class lookup of `alloc`
(`SizeClasses::class_for_small_align`). No check is removed; three
reformulations remove the instructions that were not checks. Verdict: **GO**
by the magnitude rule in both paired sets, no losing cell, MT controls up in
every cell.

Entry point under test: the real `#[global_allocator]` path
(`SeferAlloc::dealloc` → `HeapCore::dealloc` → `dealloc_fast`), through
`malloc-bench-rs`'s `inline(never)` shims, the same boundary as
`__rust_dealloc` (G1-MT1).

## 1. Change

| Part | Before | After | Removed on the accept path |
|---|---|---|---|
| R1 frame | chain tail `dealloc_chain(&mut self, ptr, layout)`; `TORN = usize::MAX` | `lane_miss(ptr, layout, heap)` (heap last); exits after the memo hit pass `block` (same address, root provenance); `TORN = 1`, resolvers test `p.addr() > 1`; `free_magazine_full_fast` takes 4 arguments | SysV: `push`/`pop rbx`, 3 argument moves, the `lea -1` of the TLS sentinel test. Win64: 2 of 4 saves, `sub`/`add rsp`, 3 moves |
| R2 tag | `SIZE2CLASS: [u8]`, then `c >= SMALL_CLASS_COUNT`, then `encode(c, LIVE)` | `LiveTag` (`#[repr(u8)]`, variants `1..=SMALL_CLASS_COUNT` per feature set, pinned by const asserts), table `SIZE2TAG`; `c = tag - 1`, LIVE byte = tag, MAG byte = `tag \| 0x40` | `cmp 0x30; ja`, `lea c+1` (free); the bound panic check (alloc) |
| R3′ bounds | `kind > Small`, `off < PAYLOAD_START[kind]`, `off >= bump`, `off & 15 != 0` | `off < LANE_PS[kind]` (256-entry table, MAX for every kind but 0/1), `g = off.rotate_right(4)`, `g >= bump >> 4` | `cmp kind, 1; ja`, `test off, 15; jne` (`+1 shr` for the bump) |

Equivalence, per part:

- **R1.** The chain re-derives everything from the address; `block` equals
  `ptr`. A `HeapCore` is aligned to more than one byte
  (`const` assert) and never sits at address 1. So `TORN = 1` cannot collide
  with a heap pointer. The null → bind and TORN → Fallback / ForeignNoBind
  mapping is unchanged; it is covered by `tests/dealloc_only_no_bind_torn.rs`
  and `tests/regression_resolver_branch_collapse.rs`. `i = cnt & 15` equals
  `cnt`, because `FREE_PARK_CAP <= TCACHE_CAP` (clamped in
  `refill_n_for_class`).
- **R2.** `encode(c, s) == (c + 1) | (s << 6)`, so the tag is the class field
  of the state byte. A `const` block asserts `class()`, LIVE and MAG for
  every class. `LiveTag::COUNT == SMALL_CLASS_COUNT` rules out a missing or
  extra variant. The build for every feature set fails otherwise
  (`medium-classes`, `medium-classes-wide` add variants under `cfg`).
  `SIZE2TAG[i] == from_class(SIZE2CLASS[i])` by construction. The public
  `SegmentLayout::SIZE2CLASS` is unchanged.
- **R3′.** For a granule index `k`: `16k < bump ⇐ k < ⌊bump/16⌋`. The floor
  is at least as strict as the old bound, and a stricter reject only hands
  the free to the chain. A misaligned `off` rotates to `g >= 2^60` and is
  rejected. A kind other than 0/1 maps to `MAX`. `off == MAX` passes that
  compare and fails the granule bound. `SegmentHeader` is unchanged (@ox
  proposed a new `bump_granules` field; the floored compare needs none).

The lane keeps its debug refinement assertions: the lane class equals
`class_for`, and `small_free_guard` accepts. Debug test runs therefore check
every accepted free against the chain.

## 2. Instruction counts

Accept path of the free shim, entry to `ret`, from objdump (Linux) and from
`--emit asm` (Win64). Raw: `r123/asm_*`.

| | Linux base | Linux R123 | Win64 base | Win64 R123 |
|---|---:|---:|---:|---:|
| free accept path | 62 | 49 | 72 | 55 |
| alloc hit path | 38 | 35 | — | — |

Callgrind, `g1_mt_one`, T=1, 300 000 operations, Linux (`r123/mt_callgrind_totals.txt`):

| Workload | base Ir | R1 only | R1+R2+R3′ | Δ total |
|---|---:|---:|---:|---:|
| mstress | 78,113,757 | 74,181,418 | 71,674,521 | −8.2 % |
| larson | 73,835,465 | 70,925,683 | 69,122,938 | −6.4 % |

## 3. Paired bench:table (Windows, judge of the magnitude rule)

Two sets, order base, x, x, base each, aligned code
(`-align-all-functions=6 -align-all-nofallthru-blocks=5`). Tables derived
by `node scripts/g1_magnitude_verdict.mjs docs/perf/r123 s{1,2}base s{1,2}r123 s{1,2}`;
the summary CSV `G1_R123_FREE_LANE_summary.csv` is written by the same
script (`MAG_CSV`).

| Set | sefer time gm | wins / losses (pp) | largest loss | ratio to mimalloc gm | new LOSS cells | Verdict |
|---|---:|---:|---:|---:|---:|---|
| s1 | −9.6 % | 95.4 / 0.0 | +0.0 % | −6.6 % | 0 | GO |
| s2 | −9.6 % | 69.3 / 0.0 | +0.0 % | −8.2 % | 0 | GO |

Cells that leave the LOSS class against mimalloc (sefer/mi, mean of the
pair, set 1 / set 2): churn_write 16 B 0.978 → 0.916 / 1.030 → 0.900,
churn_write 64 B 1.016 → 0.892 / 0.979 → 0.897. Still a loss: direct 16 B
(1.370 → 1.308 / 1.356 → 1.293).

## 4. MT control (malloc_macro, Windows)

ABBA × 6 (12 runs per arm), table by
`node scripts/as1_mt_table.mjs docs/perf/r123/mt_malloc_macro_raw.txt` (the
script labels the second arm `as1`; here it is R123). Higher is better.

| Workload | T | sefer before (M ops/s) | after | Δ | sefer/mi before | after |
|---|---:|---:|---:|---:|---:|---:|
| larson | 1 | 52.75 | 54.61 | +3.5 % | 0.936 | 0.995 |
| larson | 2 | 57.43 | 60.33 | +5.0 % | 1.231 | 1.318 |
| larson | 4 | 98.67 | 100.34 | +1.7 % | 1.167 | 1.222 |
| mstress | 1 | 58.96 | 59.80 | +1.4 % | 0.833 | 0.880 |
| mstress | 2 | 95.03 | 99.81 | +5.0 % | 1.004 | 1.010 |
| mstress | 4 | 150.69 | 160.56 | +6.5 % | 0.948 | 0.964 |

## 5. Identity

- Base: `main` at `5fd6b277f438c9486e9e7dd694d767adc94d8546`.
- Measured tree: commit `560d6ba6cd7813cf4fa05d8cfa5e9a8147aa5f28` (branch
  `g1-r123`, before squash; its tree equals the landing commit's `src/`).
- Linux lab binaries (sha256 prefix): base `0f827f2b47a2fbcc`, R1 `c47cfd498242a22d`, R123 `e0ae6395d727272b`.
- Host: Windows 10 x86_64 (bench:table, MT), WSL2 Ubuntu (callgrind, objdump).

## 6. Not done

- @ox's R2 copy of `FREE_PARK_CAP` into the heap (−1 Ir) is not taken: it
  grows `HeapCore` and its layout pins for one instruction.
- Win64 still saves `rsi`/`rdi` on the free accept path (2 pushes, 2 pops):
  nine live values against seven volatile registers. That is the next
  candidate.
