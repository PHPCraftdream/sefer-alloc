# J13 limited Always/Off study — measurement-only manifest

## Scope and current outcome

**No shipping runtime or opt-in algorithm changes in this delivery. Production feature composition and default Off are unchanged.** Current range Always is measured through explicit runtime configuration, not presented as an already-shipped default speedup.

- Tooling/preparation: INFRASTRUCTURE, accepted exact-source CI `42999985109` on `9c63c3a7`; subsequent fixture repair accepted CI `43008649237` on `573cb7c2` and successful smoke measurement `43013696396`.
- Registered measurement: [run43014496362](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/43014496362), source `573cb7c213eb72fe070776efb3a8e44548633ba6`; measurement job `130515411731` and upload SUCCESS. Exactly one complete 960-child campaign; no repeat or replacement.
- Evidence/report: `../j13_limited_20261111/J13_LIMITED_ALWAYS_OFF.md`, its generated summary CSV, original compressed raw and reducer outputs, and content-hash manifest. Smoke data remain separately marked discarded for performance inference.
- Large timing/RSS/fault/AnonHugePages results are the generated report, not hand-transcribed into this manifest. Sparse edge and CACHE costs require explicit owner review alongside dense benefit. No product GO or default decision is inferred.
- Small-Ir: no standalone measurement; #107 owns existing changed-default Small-path gates. Correctness item264 remains OPEN.
- Native fixture failure and independent fuzz-adapter false capability are tracked in `docs/CORRECTNESS_OPEN_ITEMS.md`. The former is verified resolved; fuzz regression replay/CI on `eef21ab2` is still awaiting acceptance at this snapshot. The old-source manual workflow is not relabelled overall SUCCESS.
- #107 requires the owner's decision after the report. #115 Auto/budget and full round closeout remain later; #113/#114 remain paused. Unknown contract worktree lock cleanup remains administrative and unresolved, not a performance veto.

## Derived commit inventory

The table below is derived from `git log --reverse --format='%H%x09%s' bad95849f5560571940f0e1b6311b4cd1bb97426..eef21ab201c8a16148a75a5defe0a966f8d0a847`. This is the implementation-prefix snapshot before the evidence-packaging commit, not a final round-close claim. Later landing/evidence/default commits must be included by the final closeout inventory.

| Commit | Category | Subject |
|---|---|---|
| `0f674078677acab68360c2c34d5d14a0fafb6f48` | docs(config) | docs(config): register narrow J13 Always study and reconcile runtime status |
| `c641c3630d37682937c1537fb7423b773eec5d4b` | bench | bench(j13): integrate bounded Always study harness and raw reducer |
| `c8eabb82d74efc086491f297bec147ed18ddb1a3` | fix | fix(ci): parenthesize build receipt jq conditionals |
| `25fcdfe08f44ae5e287e4abd2768cd8931b1dbf0` | docs(config) | docs(config): refresh verification inventory after study tests |
| `212362f40aa808a83b8e231be71d6446242a1714` | docs(config) | docs(config): amend J13 premeasurement protocol for manual CI |
| `9c63c3a74091f14f0346893b8cfe060876530f05` | bench | bench(j13): wire manual study with native CI timeout |
| `573cb7c213eb72fe070776efb3a8e44548633ba6` | test | test(j13b): publish fixture tuple before concurrent capture workers |
| `eef21ab201c8a16148a75a5defe0a966f8d0a847` | test | test(fuzz): stop promising unsupported AllocCore double-free semantics |

## Evidence storage snapshot

- Manifest-listed evidence files: 83; stored bytes: 557036 (manifest file itself excluded).
- Original JSON documents are losslessly compressed under the per-file storage policy; compressed and uncompressed hashes are retained. Discarded smoke is separately identified and never pooled with registered data.
- Combined first-study executable-source tally remains899/900. Native fixture/fuzz correctness repairs are not new measurement/controller implementation. Workflow remains981lines; license extraction preserved expanded commands.
- This manifest does not close the round. Actual report publication/landing, owner default decision and later full-CI closeout remain outstanding.
