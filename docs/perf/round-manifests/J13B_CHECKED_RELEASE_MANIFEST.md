# J13b checked release — VM prerequisite

## Scope and source

Base: `39f228d8`. Derive the delivered commit list with:

```sh
git log --reverse --format='%H %s' 39f228d8..HEAD
```

This layer adds checked release outcomes in `aligned-vmem` and private root OS forwarding methods. The dependent Large-THP runtime policy, header charge, observation core, models and root error-retention test are not part of this layer. Forwarding methods are intentionally unused until that dependent change and carry a staged-bridge lint reason.

Existing infallible release, raw-parts release and Reservation Drop behavior are preserved. Checked success reflects native `munmap`/`VirtualFree` status, or terminal deallocation within Miri's model. Checked failure consumes the release decision without returning usable ownership; callers must not retry and must retain unresolved accounting conservatively. Null remains an explicit no-op.

## Verification boundary

Separate tests cover release geometry, actual native success activation, pre-dispatch fault injection and backend-result surrogate propagation. The surrogate is not actual kernel-refusal evidence. Counter movers and delta readers share a binary-local serialization mutex.

Parent review corrected release-vs-reserve validation, ownership forwarding safety, mock-test serialization, misleading documentation and obsolete hook atomics. Release hook state uses the established mutex pattern rather than a new explicit atomic protocol. The unsupported-target compile error is relocated verbatim from `os/unsupported.rs` into the crate root to preserve source-directory limits. Frozen oversized files do not grow.

Only permitted compile/lint/format and cargo-free static verifiers were run locally. Tests, native error paths, Miri interpretation and mutants have not been executed locally. CI `42426350786` completed successfully on `0a6059c65e2b04df744f353abf61afe00d372bd5`. Personally inspected native VM logs contain 5 checked-release, 3 backend-result surrogate and 3 fault-injection passes; the actual Miri interpreter also executed those groups with hooks enabled. The repaired TLS teardown test executed successfully twice in the aarch64 job. Zero-test feature rows are not counted as execution. Correctness item 269 records that narrow closure; this does not accept the unpublished THP runtime.

## Impact and artifacts

Classification: `feat(vmem)`, not a measured performance improvement. Production feature composition and defaults are unchanged. No wall-clock, Ir or RSS measurements exist. New measurement raw logs: **0 files, 0 bytes**.

The strict-budget root funnel still belongs to the dependent #47 change: it must return charge only after confirmed checked release and retain charge on error or unknown outcome. This VM layer alone does not implement or accept the full THP policy. No merge into main or default promotion is included.

Current VM-layer verdict: **INFRASTRUCTURE / CI VERIFIED** (run `42426350786`).

## Cross/QEMU verification repair

The next test-only delivery repairs a separately discovered false-positive abort oracle in `r6_sidecar_bitmap`. It is classified as `test`, not runtime performance work. Its changed paths are `Cross.toml`, the existing test and its focused child-process helper; no allocator source, production features, defaults, dependencies or workflow are changed by that delivery. Correctness item 270 remains OPEN until exact-source native and aarch64/QEMU execution confirms the repaired oracle and negative controls.

The repaired oracle requires exclusive fresh activation, the platform abort signature and an exact production diagnostic line; configured runner argv differs correctly from native argv. Supervision uses file-backed stderr and an owned direct child rather than inherited-pipe EOF or detached reader threads. Its portable fixed gate intentionally consumes 60 seconds per abort mode and 10 seconds per subprocess counterfactual, even for early completion. This is verification overhead, not an allocator latency measurement. Absolute OS syscall deadlines and arbitrary descendant-tree containment are not claimed. Cleanup-error injection and the stderr-size boundary remain source-review-only coverage limitations.

Local check/clippy/format and structure checks passed after transfer; runtime tests have not run locally. Cross-repair verdict: **INFRASTRUCTURE / CI PENDING**. New performance measurement raw logs remain **0 files, 0 bytes**. The command above derives the full delivered commit list, including VM `feat`/`fix`/`docs`, test-only repairs and the local checkpoint; no commit in this layer claims measured wall-clock, Ir or RSS improvement.
