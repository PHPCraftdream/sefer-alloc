# R19 PRF-01 — A-only producer preflight (b03): Amendment B coverage is achievable

Verdict class: **bench / diagnostic only**. No shipping or opt-in allocator code changed, no sample
exists, nothing here is gate evidence and no GO/NO-GO is derived. The preflight answers one
question that Amendment B left open: can the frozen coverage schema be met by the real binaries?

## 1. What was run

Entry point under test: the iai bench binary `perf_gate_iai` (cell `r19_prf01_joined_producer_same_word`,
dispatcher `--iai-run perf_gate 89 0`), driven directly by Valgrind/Callgrind with
`--toggle-collect=<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window`,
`--collect-atstart=no`, `--separate-threads=yes` (the settings frozen in the runner's
`PRODUCER_SETTINGS`). The workload instantiates a `SeferAlloc` value, the same layer the gate cell uses;
it is not an installed `#[global_allocator]` run. Arm A only: base `7e7da281` plus the uncommitted
harness/runner tree captured below. Primary and removed-frees control are built from one source with the
same features (`production bench-internals internals`), the control adding only
`cfg(r19_prf01_producer_control)`.

| Item | Value |
|---|---|
| capture sha256 | `d153969eb8ebe53afe8acffba12c6f88d376dad5df50bd46e3cfb536681d860f` |
| source identity (b03 capture) | `30a3c158fe5e57ad2581b360ce7f47d0b8a1afc54b9483b2e307aea7ee84c6ac` |
| patch / untracked sha256 | `844fa0314f273b0965cb256285b1d039075ef317822328b5a03b48dcbf5c254d` / `afad56aa9d9d1b3f34b428640c506eeb64091c196ed10762917aa45809facba9` |
| primary binary sha256 | `3f261e176b3dfc8015c0524f97295bdd445787ba572f29f5bd4ce443a06229ea` |
| control binary sha256 | `dcf48f9ee571bd7e08d54c85548b81bc9779923260d9af92c7c7729cb4790552` |
| plan sha256 | `cbecf89bd814a52373004da2bb771db3acd15fc404db7f5d8511db91a74e3fc7` |
| toolchain / host | rustc 1.98.1, Valgrind callgrind-3.22.0, WSL2 Linux 6.18.33 x86_64 |

The harness sources are the ones committed afterwards as `2a0671d7`. The runner of b03 is the version at the
moment of the launch (hashed inside the capture); the committed runner additionally contains fixes 3 and 4 of
§4, made after the launch.

Independent review before the launch (orchestrator, not the runner author): both binary hashes recomputed in
WSL and equal to the capture receipts; `nm -C --defined-only` shows exactly one definition of the exact symbol
in each binary; `objdump -Cd` shows one direct call from the child closure in each, the control symbol being a
single `ret` (followed by `int3` alignment padding) and the primary symbol the full inlined remote-free loop;
the two argv vectors differ only in the binary and the output/log paths; at most two processes, no retries.

## 2. Observed numbers (derived by `scripts/r19_prf01_preflight_reparse.mjs`)

Raw per-thread profiles: `docs/perf/_raw_r19_prf01_producer_preflight_b03_*.log` (private arm root replaced by
`<arm-root>`; the valgrind logs and the zero-byte base files are cited by sha256 in
`R19_PRF01_PRODUCER_PREFLIGHT_B03_reparse.json`). Summary: `R19_PRF01_PRODUCER_PREFLIGHT_B03_summary.csv`.

| role | totalIr (sum of all thread profiles) | childIr (thread 2) | boundary executions | thread profiles |
|---|---|---|---|---|
| primary | 1,515,559 | 1,515,559 | 1 (one incoming edge) | 2 |
| control | 1 | 1 | 1 (one incoming edge) | 2 |

- `boundaryOverheadIr` = control `childIr` = **1**, inside the frozen `1..16`.
- `workloadIr` = primary `childIr` − `boundaryOverheadIr` = 1,515,559 − 1 = **1,515,558** (> 0); control
  `workloadIr` = 0. Both totals count only the toggled window, so the remainder also equals the total delta
  1,515,559 − 1; the script asserts both equalities.
- Primary call graph from the retained profile: `producer_free_window` → `HeapCore::publish_foreign` ×4096
  (exactly the declared `childFreeCount` 4096) → `RouteDirectory::lookup` ×4096 and `RoutePin::publish_small`
  ×4096. The anchor survived: the symbol and its single executed call exist in both variants.
- Thread 1 (parent) collects 0 Ir in both runs, as `collect-atstart=no` requires.

This shows that the schema can be met and that the control is empty and measured as 1 Ir; it is **not** by
itself proof of the primary workload — the separate oracle of the actual 4096 remote frees (and, for arm B,
the summary load/RMW checks) and the independent review of raw evidence remain the gate's job
(Amendment B, point 3).

## 3. Chronology and what was and was not consumed

| label | outcome | consumed the single Callgrind launch? |
|---|---|---|
| a01 | prepared before Amendment B; control binary lacked the exact symbol (the finding that motivated the amendment) | no |
| b01 | anchor present in both binaries; the runner's control verifier rejected the `int3` padding after `ret` → plan `launchable:false` | no |
| b02 | plan launchable; approval refused at the pre-launch check: the retained dispatcher source had been written through the path redaction and was not valid JSON | no (`preflight-launch.json` absent) |
| b03 | approved by the orchestrator, launched once (primary + control, exit 0 both); the runner then rejected the raw profiles for two parser defects (§4) → `INCOMPLETE` | **yes — no retry was made** |

The anchor, the 1..16 limit and every frozen criterion were not touched after any observation. After the
launch only parser/filter code changed, and the retained raw files were re-parsed offline (nothing launched).

## 4. Runner defects found (each fixed with a regression check in `--self-test`, 140 synthetic checks)

1. The control verifier included the `int3` alignment padding after `ret` (found at b01).
2. The dispatcher source was stored through `publicText`, which replaces quoted slash-leading strings, so the
   JSON no longer parsed (found at b02, before any launch).
3. The zero-byte base file `callgrind.out.#PID` that Valgrind writes next to the `-NN` thread files was parsed as a
   thread profile (found at b03). It stays in the raw originals; empty files are no longer treated as profiles.
   A missing child profile is still caught by the thread-gap and child-count checks.
4. Callgrind omits trailing zero columns of `totals:`; the parser demanded all nine events (found at b03). Extra
   columns remain rejected.

## 5. Next step

Full gate: capture arms A (`main` with the oracle) and B (`r19-prf01-impl` rebased over the committed harness)
as clean commits with equal-length paths, independent identity review, one fresh run in a quiet window,
raw-evidence review, judge. The preflight directories and the retained Callgrind output live under the arm's
`target/` (not committed); the committed excerpts above are the citable part.
