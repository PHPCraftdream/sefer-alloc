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

Only permitted compile/lint/format and cargo-free static verifiers were run locally. Tests, native error paths, Miri interpretation and mutants have not been executed locally. Exact-source CI evidence is pending; no passing-test claim is made.

## Impact and artifacts

Classification: `feat(vmem)`, not a measured performance improvement. Production feature composition and defaults are unchanged. No wall-clock, Ir or RSS measurements exist. New measurement raw logs: **0 files, 0 bytes**.

The strict-budget root funnel still belongs to the dependent #47 change: it must return charge only after confirmed checked release and retain charge on error or unknown outcome. This VM layer alone does not implement or accept the full THP policy. No merge into main or default promotion is included.

Current verdict: **INFRASTRUCTURE / CI PENDING**.
