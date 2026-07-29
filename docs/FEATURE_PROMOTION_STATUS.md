# Feature promotion status — non-`production` Cargo features

**Task:** R29-12 (task #443). **Docs/indexing only — no `src/` change, no
`Cargo.toml` feature change, no `production` re-composition, no new
measurement run.** This file is a SURVEY of a state that already exists
elsewhere in the tree; it makes no decision and cites no newly-measured
number.

**Why this file exists.** The independent R28 read-only review
(`docs/reviews/2026-07-29-oh-acceleration-code-project-review.md` §3.2)
found that at least three implemented, CI-tested features whose promotion
decision had been explicitly deferred were tracked in NEITHER open-items
index — the decision lived only inside a `Cargo.toml` comment. That is
exactly the failure mode `docs/CORRECTNESS_OPEN_ITEMS.md`'s own stated
purpose (lines 46–50) says its sibling indexes exist to prevent: *"a flag
that lives only inside a single commit message body or code comment is
exactly the failure mode this index exists to prevent."* This file is the
one-time survey that surfaces the full picture; the three sharpest cases
(`virgin-zero-skip`, `small-segment-lazy-commit`, `alloc-lazy-commit`) are
additionally carried as live open items in `docs/perf/OPEN_ITEMS.md`
(items 25–26) so the round-start ritual surfaces them.

**Why standalone, not a section in an index.** This survey is cross-cutting:
it spans perf-gated features (`virgin-zero-skip`, `exact-span-large`, …),
policy/correctness-flavoured features (`small-segment-lazy-commit`'s
decommit surface), and deliberately-opt-in features (`hardened`,
`numa-aware`). `docs/perf/OPEN_ITEMS.md`'s scope is deliberately narrow
("*`docs/perf/*.md` only … It is NOT a general issue tracker*") and
`docs/CORRECTNESS_OPEN_ITEMS.md`'s scope is "*correctness bugs, flaky
tests, and CI-coverage gaps*." A whole-feature promotion-status table fits
neither cleanly, so it lives here; only the genuinely-pending DECISIONS
land in the index whose scope matches their source.

**`production` composition (read from `Cargo.toml:399`, verified):**
`["alloc-global", "alloc-xthread", "alloc-decommit", "fastbin",
"alloc-segment-directory", "primordial-lazy-commit", "class-aware-dirty"]`.
Everything below is NOT in that list. The foundational `alloc-core`/`std`
and the already-in-`production` features are excluded by construction
(transitively shipped).

**Verdict key.**
- **GO-promoted** — already in `production` (not listed here).
- **CONDITIONAL-GO-not-promoted** — a complete design / gate report exists
  with a GO-or-CONDITIONAL-GO recommendation, but the feature was never
  promoted; the promotion decision is either deferred-with-reason or never
  formally closed.
- **DECIDED-opt-in** — an explicit on-record decision to keep it opt-in
  (not ship, not remove).
- **NO-GO** — an explicit on-record NO-GO for `production`.
- **NEVER-DECIDED** — the promotion question was raised but the gate that
  would answer it was never run; no verdict exists either way.
- **deliberately-opt-in** — by-design opt-in (diagnostic / unstable-API /
  host-specific / measurement-only); no promotion question applies.

---

## Survey table — one row per non-`production` feature

| feature | shipped-behind-flag | has-gate-report (doc) | promotion verdict | evidence citation |
|---|---|---|---|---|
| `virgin-zero-skip` | yes (`Cargo.toml:744`, `=["alloc-decommit"]`) | yes — R9-5 (design), R11-8 (re-verify), R13-3 (magazine-fix gate) | **NEVER-DECIDED** (two CONDITIONAL-GO designs; the design's own Stage-3 promotion gate was never run; R13-3 is explicitly NOT a promotion verdict) | `R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` §11 (Stage 3, lines 563–568) + §8; `R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md` §8; `R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md` (headline "No scenario shows a statistically significant difference"); `Cargo.toml:737–744`. See `OPEN_ITEMS.md` item 25. |
| `small-segment-lazy-commit` | yes (`Cargo.toml:700–704`) | yes — R12-9 (the split gate) | **CONDITIONAL-GO-not-promoted** (deliberately left opt-in; a reasoned decision EXISTS, unlike `virgin-zero-skip`) | `R12_9_PRIMORDIAL_LAZY_COMMIT.md` §6 (lines 231–238, explicit scope-out); `Cargo.toml:685–704`. See `OPEN_ITEMS.md` item 26. |
| `alloc-lazy-commit` | yes (`Cargo.toml:656`) | n/a — pure combinator alias | **reduces to `small-segment-lazy-commit`** — alias `=["primordial-lazy-commit","small-segment-lazy-commit"]`; `primordial-lazy-commit` is already in `production`, so this feature's promotion status IS `small-segment-lazy-commit`'s | `Cargo.toml:635–656` (the PURE-COMBINATOR note + the alias); `R12_9_PRIMORDIAL_LAZY_COMMIT.md` §1. See `OPEN_ITEMS.md` item 26. |
| `exact-span-large` | yes (`Cargo.toml:312`) | yes — R13-6 | **CONDITIONAL-GO-not-promoted** (R13-6 §7: "Promoting … unconditionally, is not recommended"; no unconditional GO or permanent NO-GO) | `R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md` §7 (lines 420–482). Mentioned in `OPEN_ITEMS.md` only as a passing reference inside other items (e.g. item 3 line 758), NOT as its own promotion item. |
| `large-reserved-capacity` | yes (`Cargo.toml:357`) | yes — R14-6 (GO rec), R20-2 (NULL on the C4 combo) | **CONDITIONAL-GO-not-promoted** (R14-6 §5 GO recommendation never acted on; R20-2 §6.1 NULL verdict on the `production+medium-classes+exact-span-large+large-reserved-capacity` combo); promotion is contingent on `exact-span-large` | `R14_6_ADAPTIVE_RESERVED_CAPACITY_GATE.md` §5 (lines 316–340); `R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` §6. Mentioned in `OPEN_ITEMS.md` item 8 only as a deferred growth-factor sub-finding. |
| `large-cache-extended` | yes (`Cargo.toml:371`, `=["alloc-decommit"]`) | yes — R14-5 | **CONDITIONAL-GO-not-promoted** (R14-5 §9: "promotion to `production` should be considered ONLY [under its GO condition]") | `R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` §9 (lines 414–446). `OPEN_ITEMS.md` item 7 carries one narrow deferred sub-finding, not the promotion question. |
| `medium-classes` | yes (`Cargo.toml:524`) | yes — R8-9, R9-3, R10-2, R14-4, R22-18 | **DECIDED-opt-in** (R22-18 §0 "Recommend (b) — formally document as a named opt-in workload profile"; well-indexed, 10 `OPEN_ITEMS.md` refs) | `R22_18_MEDIUM_CLASSES_FATE_DECISION.md` §0/§3. |
| `medium-classes-wide` | yes (`Cargo.toml:556`) | yes — R9-4 | **NO-GO** for `production` (large-realloc regression); indexed via `OPEN_ITEMS.md` items 5 & 11 | `R9_4_1_75MIB_CLASSES_PROTOTYPE.md` §4; `OPEN_ITEMS.md` item 5 (line 786) + item 11. |
| `batch-api` | yes (`Cargo.toml:214`) | yes — R23-7 (consumer status) | **deliberately-opt-in** — explicitly EXPERIMENTAL, nested under `experimental`, "no semver guarantees"; R23-7's open question is consumer adoption, not promotion | `Cargo.toml:172–214`; `R23_7_BATCH_API_CONSUMER_STATUS.md`. |
| `page-map-diag` | yes (`Cargo.toml:251`) | no (diagnostic-only by design) | **deliberately-opt-in** — promoting would ADD per-carve/per-page hot-path work (the feature gates PageMap WRITE sites ON); OFF is the perf-default | `Cargo.toml:216–251`. |
| `alloc-stats` | yes (`Cargo.toml:435`) | no (diagnostic counters) | **deliberately-opt-in** — trades a hot-path instruction for the two hit-rate diagnostics; the feature's own doc names `production alloc-stats` as the way to opt IN, not a candidate for unconditional promotion | `Cargo.toml:414–435`. |
| `bench-internals` | yes (`Cargo.toml:485`, `=[]`) | no (measurement-only hook gate) | **deliberately-opt-in by construction** — exists solely to keep measurement `unsafe fn dbg_*` hooks out of a plain `--features production` build (CLAUDE.md benchmark-hook rule 2) | `Cargo.toml:436–485`. |
| `hardened` | yes (`Cargo.toml:598`) | no | **deliberately-opt-in** — paranoid defence-in-depth that costs a non-power-of-two modulo on EVERY small free; explicitly "NOT free enough to enable unconditionally" | `Cargo.toml:581–598`. |
| `numa-aware` | yes (`Cargo.toml:603`) | no committed multi-node measurement | **deliberately-opt-in** — host-specific (no-op / macOS); promotion is a host-deployment question, not a gate question | `Cargo.toml:599–603`. (R28 review §2.4/§3.3 flag a separate NUMA-pool correctness gap + a per-PR compile-coverage gap — tracked in the review, not a promotion question.) |
| `numa-aware-mock` | yes (`Cargo.toml:616`) | no | **deliberately-opt-in** — TEST-ONLY (`numa-shim/mock` backend for scripted tests) | `Cargo.toml:604–616`. |
| `experimental` | yes (`Cargo.toml:117`) | no | **deliberately-opt-in** — research/RCU+epoch concurrent tier umbrella ("no semver guarantees, research-tier") | `Cargo.toml:109–117`. |
| `pinning` | yes (`Cargo.toml:133`) | no | **deliberately-opt-in** — thread-per-core runner over `experimental` | `Cargo.toml:118–133`. |

---

## The three flagged features — honest disposition

These are the three the R28 review (§3.2) flagged as the sharpest
dangling-promotion cases. For each, the constraint of R29-12 is honored:
**no new measurement was run to manufacture a verdict.**

### `virgin-zero-skip` — NEVER-DECIDED (needs a named missing measurement)

The feature is BUILT and CI-tested (`.github/workflows/ci.yml` runs a
`production virgin-zero-skip alloc-stats` step). Two independent design
docs (R9-5 primary, R11-8 independent re-verification) reach the same
**CONDITIONAL GO for staged implementation** verdict, and both define a
Stage 3 — the *promotion gate* — as the precondition for even *considering*
promotion ("Only on a green Stage 3 consider promoting `virgin-zero-skip`
into `production`," `R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:566`). **That Stage 3
was never run or closed.** The only later measurement, R13-3, is a was/now
gate for the R13-3 *magazine fix* and explicitly states it carries NO
promotion verdict and that its single-threaded loop does not capture the
cold-first-touch shape the feature targets.

So the existing evidence supports NEITHER a GO nor a NO-GO: it shows no
measured win AND no measured loss, on a workload the report itself says is
wrong for the question. The honest outcome is **NEVER-DECIDED**, with the
specific missing measurement named: the design's own Stage-0/Stage-3
`calloc`-shaped bench (`alloc_zeroed` on virgin pages vs recycled pages,
≥ 64 KiB where `memset` dominates) with paired-prefix subtraction, plus one
wall-clock arm at a memset-dominated size. This is cheap — the feature
already exists; only the judge is missing (independently requested by the
R28 review §1.3). **Recommendation (not actioned here): run that
measurement in a separate task; do NOT promote on the current evidence.**

### `small-segment-lazy-commit` — CONDITIONAL-keep-opt-in (a decision EXISTS)

Unlike `virgin-zero-skip`, this one WAS decided. R12-9 split the old
`alloc-lazy-commit` into `primordial-lazy-commit` (GO → promoted into
`production`) and `small-segment-lazy-commit` (explicitly NOT part of the
recommendation, `R12_9_PRIMORDIAL_LAZY_COMMIT.md:231–234`). The stated
reason is qualitative, not a missing number: the decommit/recommit
correctness surface this policy exercises on every pool eviction is
"materially larger" than the primordial's one-time bootstrap reservation,
and R8-10 (task #223, `852828e`) measured empty→pool→reuse→refill cycles
under this policy at 50–75× more commit/decommit syscalls before its
admission-side fix. The fix is permanent; the surface-size concern is the
recorded grounds for leaving it opt-in.

The defect here is purely procedural: that decision was recorded ONLY in
R12-9 §6 + the `Cargo.toml:685–704` comment, in NEITHER index. R29-12
fixes that by adding `OPEN_ITEMS.md` item 26. The one genuinely-open
thread (not actioned here): the *post-R8-10-fix net* steady-state
win/loss of this policy on a long-lived small-segment churn workload was
never measured — R8-10 measured the pre-fix regression, not the post-fix
net effect.

### `alloc-lazy-commit` — pure alias; reduces to `small-segment-lazy-commit`

`alloc-lazy-commit = ["primordial-lazy-commit", "small-segment-lazy-commit"]`
(`Cargo.toml:656`) is a PURE COMBINATOR — no `#[cfg]` in `src/` tests its
own name (`Cargo.toml:638–648`). Since `primordial-lazy-commit` is already
in `production`, "promoting `alloc-lazy-commit`" is exactly equivalent to
"promoting `small-segment-lazy-commit`." It therefore has no independent
promotion decision and no independent missing measurement; its status IS
item 26's status.

---

## Other features found in a SIMILAR (less sharp) shape

Beyond the three flagged, three more features share the
CONDITIONAL-GO-design-never-promoted shape: **`exact-span-large`** (R13-6
CONDITIONAL-GO), **`large-reserved-capacity`** (R14-6 GO rec, R20-2 NULL on
the combined config), and **`large-cache-extended`** (R14-5
CONDITIONAL-GO). They are less sharp than the flagged three because (a)
their gate reports are at least *referenced* in `OPEN_ITEMS.md` (as
sub-findings of other items, not as standalone promotion items), and (b)
their promotion is entangled — `large-reserved-capacity` exists to
counteract `exact-span-large`'s OPT-G headroom loss, and both are most
useful alongside `medium-classes`, whose own fate is already DECIDED-opt-in
(R22-18). R29-12 does NOT create index entries for these three (scope
discipline: the task names the three flagged features); they are recorded
here so a future round can decide whether to give them the same
index-entry treatment. Promoting any of them to `production` is a separate,
higher-stakes decision outside this task's scope.
