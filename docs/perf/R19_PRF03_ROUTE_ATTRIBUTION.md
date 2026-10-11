# PRF-03 step 1 — Large route attribution (measurement-only; INCOMPLETE)

Source SHA: 7ce23bfc76bcee24fe23671b42c9468c66caae68; tree: d0389d743717eb6680c3827786e44ed94b129328. Empty src/benches patch SHA-256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855. Source identity captured before measurement; binary/environment metadata observed afterward.

Features: production bench-internals internals. OS: Linux 6.18.33.2-microsoft-standard-WSL2 (Ubuntu-24.04); architecture: x86_64; CPU: 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz. Toolchain: rustc 1.98.1 (48a229cea 2026-09-01); cargo 1.98.1 (797e8a9bc 2026-08-05). Valgrind: 3.22.0; annotate: 3.22.0 (version probe exit 255); iai runner: 0.14.2. Binary SHA-256: ffd5c88dc621ed8683bbe2da19ec05ef62d308d9b536a247c3deb29cd05aa537. Samples per arm: 1.

## Entry point

HeapCore::alloc/dealloc through claim_leaked_heap (bootstrap::ensure plus registry lease), existing large_cache_prefill_only_4mib / large_cache_hit_only_4mib pair. This layer exercises the heap allocation/free implementation behind SeferAlloc's production dispatch without replacing it with standalone AllocCore. It does not measure the outer GlobalAlloc dispatch.

| Metric | Prefill | Hit | Hit minus prefill |
|---|---:|---:|---:|
| Instructions | 58589 | 59848 | 1259 |
| route_register_inclusive | 2955 | 3233 | 278 |
| route_remove_and_nested_drop_inclusive | 2696 | 3033 | 337 |
| Large_register_calls | 8 | 9 | 1 |
| Large_remove_calls | 8 | 9 | 1 |
| Large_System_alloc_calls | 16 | 18 | 2 |
| Large_System_dealloc_calls | 16 | 18 | 2 |
| route_disjoint_sum | 5651 | 6266 | 615 |
| next_incarnation_CAS_Ir | 9 | 10 | 1 |
| register_initial_lock_CAS_Ir | 11 | 12 | 1 |
| remove_initial_lock_CAS_Ir | 8 | 9 | 1 |
| refs_decrement_Ir | 8 | 9 | 1 |

Route share: 615/1259 = 48.848%. Arithmetic asserted: 615 <= 1259; remainder 644 Ir. Numeric threshold: **615 >= 200 Ir = GE_THRESHOLD**. Acceptance: **INCOMPLETE**, required oracle=NOT_OBTAINED_EXACT_HEAPCORE_DBG, actual iai consistency=NOT_OBTAINED. **Conditional next gate after required evidence is completed; not implementation B. Item 86 remains open; this report does not close it or authorize B.**

## Inclusive accounting and atomic mapping

Register is measured on SegmentTable::register_payload -> RouteDirectory::register edges, excluding primordial bootstrap. RouteDirectory::remove and EntryHandle::drop are inlined into RouteRegistration::drop: count its inclusive edge once, including nested System/glibc deallocation, never add nested drop again. System.alloc/dealloc are inlined as malloc/free. Counts cover Large route objects only, not primordial or directory block allocation.

Atomic values are computed from raw instruction records; objdump address-to-instruction mapping is retained in _raw_r19_prf03_objdump.log. next_incarnation is 0x7318e; initial register shard-lock CAS is 0x732a6; remove shard-lock CAS is 0x75446; executed refs decrement is 0x758aa (alternate 0x758df has no executed record). Initial register-lock count includes two common cold-path reacquisitions represented as a second record at the same address. Each locked RMW counts one Ir, not native latency or contention.

## Required evidence still missing

Required exact-layer dbg_large_cache_hits oracle: **NOT OBTAINED**. The existing R32-7 example is only supplementary public stats().large_cache_hits evidence with alloc-stats added, through SeferAlloc: 0 -> 7 -> 8; tail delta 1. It is not the required HeapCore dbg oracle and does not satisfy it. No existing-file edits were made to manufacture that oracle; routed-scan observation also remains absent.

Actual iai Instructions: **NOT OBTAINED**. Existing runner --help and --version were examined; narrow binary CLI probes for each function exited zero with zero Instructions rows. Runner 0.14.2 filters meta.bench_name (benchmark file), not individual library functions; passing the file would execute the forbidden full group and was not attempted. See _raw_r19_prf03_iai_cli.log. Callgrind summary equals totals; iai SummaryParser would read the Ir event from totals, but that source-level compatibility is not independent actual iai consistency. No tolerance can replace missing data; acceptance remains incomplete.

## Reproduction (Ubuntu-24.04, this worktree root only)

Build: RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=$PWD/target cargo bench --locked --bench perf_gate_iai --features "production bench-internals internals" --no-run

Binary: target/release/deps/perf_gate_iai-6a994302539a9d7d

Prefill single-process dispatch: --iai-run perf_gate 53 0; hit: --iai-run perf_gate 54 0. Exact bounded valgrind commands are in _raw_r19_prf03_prefill.log / _raw_r19_prf03_hit.log; both use --dump-instr=yes. Inclusive annotate each resulting profile separately with callgrind_annotate --inclusive=yes --show=Ir --threshold=100 --auto=no. An initial wildcard matching outer and inner wrappers yielded 4 Ir and was rejected; only inner-wrapper collection is used.

Generate without writes: node scripts/r19_prf03_attribution.mjs

Check byte-exact artifacts without writes: node scripts/r19_prf03_attribution.mjs --check

Synthetic negative fixtures without writes: node scripts/r19_prf03_attribution.mjs --selftest

Evidence: _raw_r19_prf03_identity.log, _raw_r19_prf03_prefill.log, _raw_r19_prf03_hit.log, _raw_r19_prf03_oracle.log, _raw_r19_prf03_objdump.log, _raw_r19_prf03_iai_cli.log. All tables and status rows are emitted by one checked script from these logs.

## Risks and scope

One sample per arm; no repeated-profile stability claim. Normal optimized bench has no inline DWARF line tables: mapping uses binary disassembly and dynamic records. Attribution is route cost, not proven achievable B savings; no native MT result or runtime speedup. Before any future B gate preserve SEC-03 address check first and owner's counted-no-op policy. Existing item 86 is stale relative to owner Question4 and deliberately unchanged in this scope. New tests/examples/benches: 0; mutants: none; existing source/bench/Cargo/docs edits: none; git writes: none.

Proposed subject: bench: attribute the Large cache route share (PRF-03 step 1)

Proposed body: Attribute isolated Large-cache arms with disjoint inclusive System/glibc costs and reproducible metadata. Record the >=200 Ir numeric result while retaining INCOMPLETE acceptance for missing exact-layer oracle and actual iai consistency; no runtime implementation or item closure.

Questions: no new policy decision required; completion of missing evidence requires an authorized measurement surface or a supported narrow runner path.
