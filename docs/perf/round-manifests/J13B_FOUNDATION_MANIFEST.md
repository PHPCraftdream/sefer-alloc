# J13b foundation — delivery manifest

Status: implementation foundation, CI pending; not a performance GO.

## Commit identity

Derived with `git log --reverse --format="%h%x09%s" origin/main..HEAD` before this manifest commit (base `fe7bb0ab`). This manifest-only commit is excluded to avoid a self-reference.

| Commit | Subject / taxonomy |
|---|---|
| 07013559 | feat(thp): add gradient math and bounded VM primitives for J13b |
| a89ec795 | build: reconcile J13b foundation branch with current main |

## Scope and verdict

- New pure gradient math and aligned-vmem range-advice/residency primitives: implemented in the candidate, runtime tests pending GitHub CI in PR #225.
- Runtime policy, global configuration/publication, budget lifecycle and judge integration: not delivered by this foundation.
- Production feature composition/defaults: unchanged by the J13b commits.
- No perf(runtime) or perf(opt-in) commit, no measured speedup, RSS reduction or mimalloc-parity claim.
- New raw performance logs committed by J13b: 0 files, 0 bytes. Earlier upstream artifacts arrived through the base merge; they are not new J13b measurement evidence.
- Local verification: cargo check, Clippy for touched feature rows, per-file rustfmt (cargo fmt hit Windows os error 206), structure and platform wording guards. These checks do not execute tests.
- Permission: push and PR for CI only; no merge to main or default promotion authorized.
