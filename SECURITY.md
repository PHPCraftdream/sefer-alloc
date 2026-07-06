# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| Latest `0.x` release (see [crates.io](https://crates.io/crates/sefer-alloc)) | Yes |
| Earlier releases | No |

Only the latest published `0.x` release on [crates.io](https://crates.io)
receives security patches. Users on older releases are encouraged to upgrade.


## Reporting a vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Public disclosure before a fix is available gives potential attackers advance
notice and puts users at risk.

Use the private
[Security Advisory](../../security/advisories/new) form on this repository
(GitHub → Security tab → "Report a vulnerability"). This channel is
confidential; only the maintainers and invited collaborators can see the report.

Include in the report:

- A concise description of the vulnerability.
- The `sefer-alloc` version(s) affected.
- Minimal reproduction steps or proof-of-concept code.
- The `rustc` / OS / target triple of the environment where you reproduced it.
- Whether the bug is triggered by the default feature set or requires
  `experimental`/`byte` features.
- (If known) which invariant from [`docs/INVARIANTS.md`](docs/INVARIANTS.md)
  is violated.


## Response timeline

| Milestone | Target |
|---|---|
| Initial acknowledgement | within 72 hours |
| Triage and severity assessment | within 1 week |
| Coordinated patch + advisory | within 90 days (critical: sooner) |

We follow a coordinated disclosure model. If you have a hard deadline (e.g.,
conference deadline, vendor disclosure policy), please mention it in your
report and we will do our best to accommodate.


## Scope

### In scope

The following are considered security vulnerabilities in this project:

- **Memory safety** — use-after-free, out-of-bounds reads/writes, uninitialized
  reads, dangling references, pointer provenance violations.
- **Use-after-free via stale handles** — a `Handle` that should return `None`
  after the backing slot is freed instead returns a reference to freed or
  reused memory.
- **Double-free** — calling `remove` (or `dealloc` in the byte-allocator tier)
  on the same handle/pointer twice without triggering a well-defined error.
- **Data races** — unsound concurrent access in `SyncRegion`,
  `LockFreeRegion`, or the epoch reclamation path that results in undefined
  behaviour (not merely a logic error).
- **`unsafe` contract violations** — a caller that upholds all documented
  preconditions for a `safe` or `unsafe` public API still experiences UB.
- **Soundness holes in safe abstractions** — triggering UB through safe Rust
  code alone (no `unsafe` blocks in the calling code).

### Out of scope

The following are **not** considered security vulnerabilities:

- Denial-of-service via excess allocation requests — by design, callers control
  memory consumption and the allocator does not impose quotas.
- Panics reachable through documented panic conditions (e.g., capacity overflow
  with `expect`).
- Performance degradation without a safety impact.
- Findings that require arbitrary code execution on the host already (e.g.,
  exploiting a separate vulnerability) to trigger.
- Reports about crates in `[dev-dependencies]` that are not reachable from
  library code.


## Hall of fame

Researchers who responsibly disclose valid vulnerabilities will be credited here
(with permission).

| Researcher | Issue | Version fixed |
|---|---|---|
| (TBD) | — | — |
