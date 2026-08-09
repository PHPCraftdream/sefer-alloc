# Checkpoint — 2026-08-09 [numa-shim-round-complete]

## Session summary

This checkpoint closes numa-shim's crate-by-crate remediation round, the
fifth crate in the standing `/rust-intel` audit sweep sequence
(sefer-region → tagged-index-stack → racy-ptr-cell → aligned-vmem →
**numa-shim** → size-classes). The governing instruction for the whole
sweep (established earlier in this session, still standing): process one
crate at a time, close ALL of its tasks before advancing, commit between
tasks, and after each crate's fixes land run `/checkpoint`, update
`CHANGELOG.md`, commit all markdown, and run an `@oh` closing review — same
pattern repeated for every crate. A second standing instruction: act
independently on maintainer-level API/architecture decisions, documenting
the reasoning in commit messages rather than asking the user.

numa-shim's fix-task group is now fully closed: task #697 (mbind maxnode
off-by-one — the kernel's `get_nodes()` decrements `maxnode` before
building the addressable-bits mask, so `maxnode=64` for a 64-bit nodemask
silently drops node 63's bit; fixed to `maxnode=65`), #720 (a single
256-byte cpumap read was treated as complete, silently truncating and
misreading node topology on ~900+-CPU hosts; fixed with a loop-to-EOF into
a 4 KiB buffer), #721 (extracted five pure Linux-only cpumap-parsing
functions into a target-independent `#[doc(hidden)] pub mod cpumap` so
`tests/cpumap_parser.rs` — a genuinely new architectural move, not just a
bugfix — could exercise the crate's most intricate parsing logic on any
target including this session's own Windows host, closing a "zero
behavioral oracles" audit finding), #722 (4 doc/code semantics
divergences: an unreachable-`None`-case doc claim, undocumented `node >=
64` silent skip, an unhandled `MAXUSHORT` Windows sentinel, and a mock bug
where `set_current_node(NO_NODE)` produced `Some(NO_NODE)` instead of
`None`), #723 (hoisted the boot-static Linux cpu→node topology into a
`std::sync::OnceLock<Vec<Vec<u8>>>`, eliminating up to 64
open/read/close syscall triples per `current_node()` call on a path this
crate's own R11-5 comment establishes is re-entered from sefer-alloc's
allocation path — reasoned-from-spec + cross-compile verified only, since
this Windows session cannot execute the Linux-only code path directly),
#724 (the Windows `reserve_aligned_numa` helper committed the FULL
`size + align` over-reservation in one `VirtualAllocExNuma` call instead
of the caller-requested `size` alone, doubling commit charge in the worst
case; fixed to the same reserve-then-commit two-call shape aligned-vmem's
own `win_reserve_commit` already uses — this one WAS empirically verified
on real Windows hardware, since the Windows platform module is native on
this host, unlike the Linux-only fixes), #725 (`bind_range`'s `# Safety`
doc stated its precondition unconditionally even though the function body
short-circuits before touching the pointer when `node == NO_NODE` or
`len == 0`, making five green test call sites technically UB-by-contract;
fixed the doc to scope the precondition to when it actually applies),
#726 (5 publish-surface API decisions in the `mock` module, all decided
now before the crate's first crates.io publish: narrowed two `pub`
thread-locals to `pub(crate)`; capped the previously-unbounded `CALLS`
recording Vec at 4096 entries with a genuine regression test, counterfactual-
verified by temporarily removing the cap and confirming the test catches
it; added `#[repr(C)]` layout const-assertions to `ProcessorNumber`; added
variant-level `#[non_exhaustive]` to `MockCall`'s two struct-like variants,
which broke and required fixing two existing test call sites — confirming
the enforcement is real; and applied the SAME documentation-only policy
already decided for aligned-vmem's identical `mock`-feature-unification
finding in task #715, per that commit's own explicit note that the policy
should carry over here), and #727 (3 parser/test hygiene residuals:
`format_sysfs_path`'s `[u8; 4]` digit buffer would panic for `node >=
10000` — resized to `[u8; 10]`; `parse_hex_u32` silently wrapped instead
of rejecting hex tokens longer than 8 digits — added a length guard; two
smoke tests with no real postcondition on non-mock builds got clarifying
doc comments rather than deletion, since they retain narrower real value
as cross-platform "doesn't crash the real dispatch path" probes). Every
one of these nine tasks was individually committed with its own
`fix(perf)`/`perf(runtime)`/`docs(numa-shim)` prefix (per this repo's
R30-12 commit-taxonomy convention), and every fix that could be
counterfactually tested on this Windows host was — reverted, confirmed
the new regression test fails for the right reason with the expected
failure message, then cleanly restored with a verified zero-net-diff.
Where a fix touches Linux-only code this session cannot execute (the
majority of them), commit messages explicitly say so and rely on
cross-compilation (`--target x86_64-unknown-linux-gnu`) plus careful
reasoning rather than claiming empirical verification that didn't happen
— the established verification-honesty distinction from earlier in this
session.

Immediately before this checkpoint, task #748 (this checkpoint) was
marked in_progress. The remaining post-work chain for numa-shim is #749
(update CHANGELOG.md with the round), #750 (commit all markdown from this
round), and #751 (run the `@oh` closing review) — none of these have
started yet. After #751 closes (including any follow-up fix tasks the
review generates, per the established pattern from every prior crate in
this sweep — aligned-vmem's own round needed two follow-up tasks, #775
HIGH and #776 a 14-item bundle, before it was genuinely done), the sweep
advances to size-classes: task #701 plus #728-731 are its fix-task group,
already filed and blocked behind #751 in a `blockedBy` chain, followed by
its own #752-755 post-work chain (#755 being the LAST task of the entire
six-crate sweep).

All native verification on this session (Windows) used: `cargo test -p
numa-shim --all-features`, native `cargo clippy -p numa-shim
--all-features --all-targets -- -D warnings`, `cargo fmt -p numa-shim --
--check`, and `cargo doc -p numa-shim --all-features --no-deps`. Every
fix additionally got a Linux cross-compile check (`cargo check`/`cargo
clippy --target x86_64-unknown-linux-gnu`) since most of this crate's
logic is Linux-only. `scripts/verify-commit-prefixes.mjs` was run after
every commit and PASSed each time, with only pre-existing warning noise
from commits outside this session's own work.

## Active goal

None — no `/goal` Stop hook is currently armed in this session. Progress
is tracked via the TaskList per the standing `/babygoal`-established
pattern (a `# babysit tick` cron job, id confirmed earlier in the
session, resumes work on stalls).

## TaskList

### in_progress
- #748 Post-work (numa-shim): /checkpoint after #697,720-727 land — this task, being closed by this very checkpoint write

### pending
- #749 Post-work (numa-shim): update CHANGELOG.md with the round (blockedBy: none, next up)
- #750 Post-work (numa-shim): commit all markdown docs from this round
- #751 Post-work (numa-shim): run @oh final review of all round work
- #701 size-classes: geometric-advance overflow is silently MASKED by the min-step fallback into a wrong-but-valid-looking table (blockedBy: #751)
- #728 size-classes: decide Params' #[non_exhaustive]/constructor before first publish (blockedBy: #751)
- #729 size-classes: class_for's fast path silently violates its own documented fit predicate for non-pow2 aligns (blockedBy: #751)
- #730 size-classes: 3 test defects (blockedBy: #751)
- #731 size-classes: 4 doc/validation residuals (blockedBy: #751)
- #752-755 Post-work (size-classes): checkpoint / CHANGELOG / commit-markdown / @oh review (blockedBy: #728-731 transitively)
- #656-661 publish-readiness tasks for all six crates (independently gated, not part of the active sweep order)
- #662-663, #756-768 bench-scale-tool / captrack assessment tasks (independently gated behind each crate's own closing review)
- #673 sefer-region contended-SyncRegion measurement — perpetually deferred, unverified-no-defect item, not part of the active sweep

### recently completed
- #727 numa-shim: 3 parser/test hygiene residuals
- #726 numa-shim: 5 publish-surface decisions in mock module
- #725 numa-shim: bind_range's # Safety contract scoping
- #724 numa-shim: Windows over-reservation commit-charge fix
- #723 numa-shim: boot-static topology OnceLock caching
- #722 numa-shim: 4 doc/code semantics divergences
- #721 numa-shim: cpumap parser behavioral oracles (architectural: doc-hidden test-forwarder extraction)
- #720 numa-shim: cpumap truncation fix
- #697 numa-shim: mbind maxnode off-by-one fix

## Decisions

- Task #724's Windows commit-charge fix: chose the two-call
  reserve-then-commit shape (mirroring `aligned-vmem`'s own
  `win_reserve_commit`) over keeping the single-call
  `MEM_RESERVE | MEM_COMMIT` approach, to close the "commits `size+align`
  instead of `size`" defect while staying consistent with this crate's own
  doc claim of mirroring aligned-vmem's strategy.
- Task #726's §C10 mock-feature-unification finding: applied the SAME
  documentation-only policy already decided for aligned-vmem's identical
  finding (task #715) rather than the stronger `--cfg`-flag conversion —
  explicitly per that commit's own note that this decision should carry
  over to numa-shim's round, avoiding two different answers to the same
  architectural question across sibling crates.
- Task #727's two no-op smoke tests: kept both (with clarifying doc
  comments) rather than deleting them, since they retain real, narrower
  value as cross-platform "doesn't crash the real dispatch path" probes
  distinct from the short-circuit postcondition (which IS genuinely
  covered elsewhere, in `tests/mock_dispatch.rs`).
- Task #723's topology-caching design: chose caching raw per-node cpumap
  file BYTES in a `OnceLock<Vec<Vec<u8>>>` and re-running the existing
  `parse_contains_cpu` parser, over a fixed `[u64; 64]` bitmask cache —
  preserves correctness for hosts with >64 CPUs per node, which the
  existing arbitrary-width parser already handles.

## Open questions

None outstanding — no user-input-blocking questions arose this round; all
maintainer-level decisions above were made independently per this
session's standing "act independently" instruction and documented in
each task's own commit message.

## Repo state

```
(clean — nothing to commit, working tree clean)
```
```
94c4a74 fix(perf): 3 parser/test hygiene residuals in numa-shim (task #727)
53b3ca2 fix(perf): 5 publish-surface decisions in numa-shim's mock module (task #726)
f989bed docs(numa-shim): scope bind_range's # Safety contract to when it actually applies (task #725)
2efa70f fix(perf): numa-shim Windows path committed size+align instead of size, doubling commit charge (task #724)
2cdb765 perf(runtime): cache numa-shim's boot-static cpu->node topology in a OnceLock (task #723)
```
