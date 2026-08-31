# tagged-index-stack — prerelease review, Sol-codex, run 3

- Review time: 2026-08-31 16:21:15 +02:00 (Europe/Berlin)
- Reviewed commit: `45ca04de71ac72f92a4198c1635a30556b875dfa`
- Mode: read-only source review; no tests, builds, `cargo`, clippy, rustdoc, loom, Miri, benchmarks, examples, scripts, or network access were run.
- Reviewer: Sol-codex, working alone without sub-agents as requested.
- Scope: the complete `crates/tagged-index-stack` package (production source, manifest, README, changelog, tests, loom models, benchmark and example), crate-specific CI rows, recent crate commits since Sol-codex run 2, and the in-workspace production adapter in `src/registry/heap_registry.rs`.
- Audit lens: concurrency and memory ordering, finite-counter/ABA boundaries, safe-API contracts, numeric boundaries, test-oracle validity, feature/target matrices, packaging, performance and documentation-to-code conformance. Because sub-agents were explicitly forbidden, this is the rust-intel bounded single-context mode rather than its full module fan-out. Async, unsafe/FFI, crypto/security and async-drop modules were not separately audited; static search found no such production surface in this safe synchronous crate.

## Verdict

**NO-GO for publication.**

The implementation is substantially better than at Sol-codex run 2: the invalid-link guard is now release-active, lock-freedom is correctly conditional on `Links`, retry instrumentation is absent from default builds, real-thread conservation coverage exists, loom-oracle serialization is structurally centralized, and benchmark fairness/tail caveats are much more honest.

Two release-blocking design/claim defects remain:

1. The safe API still allows the head to be used with a different `Links` backing on every call. The crate documents that this deterministically double-issues indices, but neither the type system nor runtime prevents it.
2. A finite 48-bit minimum tag is presented as structurally defeating ABA and as unable to repeat in any physically plausible observation window, while the crate itself derives a full wrap in roughly 3.3–16 days at its assumed rates. That is an engineering-risk reduction, not a correctness proof or structural impossibility.

I would not publish the current API/claims and freeze them as 0.1.0. Since compatibility may be broken freely, this is the cheapest moment to make the backing relationship structural and make the ABA contract mathematically honest.

## Findings

### P1-1 — Safe calls can deterministically corrupt the stack by swapping `Links` backing

Evidence:

- `TaggedIndexStack` owns only `head` (`src/lib.rs:735-785`).
- Every `push` and `pop` independently accepts an arbitrary `&L` (`src/lib.rs:932`, `1129`). Nothing ties that value—or even its logical identity—to the head.
- The docs explicitly concede that a different zeroed backing can make `load_next` return the current index, make `compare_exchange(current, current)` succeed, and return the same index repeatedly (`src/lib.rs:849-912`).
- The release-active guard checks only `next == TAIL || next < INDEX_MASK` (`src/lib.rs:1158-1161`). A foreign backing returning `0` is numerically valid, so the guard cannot detect the documented failure.
- Tests exercise a correct second implementor and `&dyn Links`, but no negative backing-swap counterexample (`tests/custom_links_impl.rs:40-75`).

Minimal safe-Rust counterexample at width 16:

```rust
let a = ArrayLinks::<2>::new();
let b = ArrayLinks::<2>::new();
let stack = TaggedIndexStack::<16>::new();
stack.push(&a, 1);
stack.push(&a, 0);              // head 0, a[0] == 1
assert_eq!(stack.pop(&b), Some(0)); // b[0] == 0: CAS current -> current
assert_eq!(stack.pop(&b), Some(0)); // same index issued again
```

This is not merely a malicious trait implementation: both backings are the crate's own valid `ArrayLinks`. Documentation moves the trap into a caller contract but does not make the safe abstraction preserve its central conservation property. In the intended allocator consumer, duplicate slot issuance can become memory unsafety outside this crate.

Recommended release fix:

- Redesign the abstraction so one receiver supplies both head and links, or so the stack permanently owns/binds its backing. A storage trait with `head()` plus `load_next`/`store_next`, implemented by the allocator registry and by an owned-array wrapper, is preferable to passing backing identity into every operation.
- If a separate head type remains necessary, expose only a bound operation handle that cannot be reconstructed with another backing while non-empty. Merely adding `links_id()` shifts an unenforceable promise into another open-trait method and is not a structural fix.
- Add the deterministic two-`ArrayLinks` regression above while redesigning; the desired post-redesign result should be “cannot be expressed”, not a runtime panic.

### P1-2 — The advertised ABA guarantee is absolute, but the tag is finite and demonstrably wraps

Evidence:

- Manifest description: “ABA-defeating tag” (`Cargo.toml:7`).
- README and crate docs say the tag “structurally defeats” ABA for every permitted width and “cannot repeat within any physically plausible observation window” (`README.md:3-13`; `src/lib.rs:1-17`).
- The public type repeats the structural claim (`src/lib.rs:735-738`), and `pop` says the residual wrap is outside any physically plausible window (`src/lib.rs:1063-1070`).
- Yet the same docs calculate a 48-bit wrap in about 16 days at `2e8` pushes/s and 3.3 days at `1e9` pushes/s (`README.md:128-145`; `src/lib.rs:159-176`; `CHANGELOG.md:63-83`).
- Tests explicitly prove that the tag returns to zero (`tests/stack_unit.rs:162-188`; `tests/regression_counter_wrap.rs:68-96`). Loom's tiny models cannot explore `2^48` pushes and therefore cannot prove the absolute claim.

A finite tagged Treiber head prevents ordinary short-window ABA but does not structurally eliminate ABA. A thread paused after reading the head while other threads continue operating can eventually see the exact packed word recur. Whether that schedule is operationally likely is a deployment assumption, not a safety theorem. “Physically plausible” is especially unsuitable as a library contract: debuggers, thread-specific suspension, extreme starvation, instrumentation and unusual runtimes are outside the crate's control.

Recommended release fix—choose one contract deliberately:

- Honest bounded contract: call it ABA mitigation, state the exact recurrence condition, remove “structurally defeats”, “cannot repeat”, and “exhaustive proof” implications from the manifest, README, rustdoc and changelog, and require consumers to accept the finite-wrap risk.
- Strong contract: prevent reuse while an observer may retain a stale head, e.g. caller-provided hazard/epoch/quiescence state, or a wider/native tagged word on supported targets with a clearly documented fallback. A wider finite counter only extends the bound; it still does not justify a mathematical “never”.

The current 48-bit budget can be a reasonable engineering choice, but it cannot support the current absolute API promise.

### P2-1 — Public `pack` silently converts invalid input into valid-looking state

`TaggedIndex::pack` masks an oversized index and shifts away an oversized tag (`src/lib.rs:436-474`). The docs devote dozens of lines to the resulting wrong-index and false-empty cases, and `try_pack` was added beside it (`src/lib.rs:476-501`). Keeping the sharp operation public under the simple name `pack` still invites exactly the silent corruption the checked twin is meant to prevent.

For a first release with no compatibility constraint:

- make public `pack` checked (`Option`/`Result`),
- keep a private `pack_truncating`/`pack_unchecked_prevalidated` helper for the hot internal path, or give any intentionally truncating public function an explicit name,
- keep tag wrapping explicit at the call site rather than relying on overflow bits disappearing during a shift.

This also removes a large amount of defensive prose and makes invalid external input fail closed.

### P2-2 — `test-internals` is not exercised anywhere in CI, so its new oracles compile out

The feature is declared at `Cargo.toml:55-69`. `threaded_conservation` gates every retry/backoff activation assertion on it (`tests/threaded_conservation.rs:100-180`) and explicitly says the test is weaker without it.

No workflow command enables `test-internals`; the ordinary debug/release tests run default features only (`.github/workflows/ci.yml:1743`, `1754`), and default clippy/docs do the same (`1985-1986`). Consequently CI executes conservation but never executes the assertions introduced specifically to prove retry and full-depth backoff activation.

The surrounding CI comments are now stale and misleading: they repeatedly state that the crate has “exactly ONE feature, `loom`” (`.github/workflows/ci.yml:1937-1945`, `1964-1972`). The manifest now has `loom` and `test-internals`.

Fix: add an explicit test row with `--features test-internals` (preferably the release threaded test), and lint that feature combination. Update the feature-count comments so CI documents the manifest it actually checks.

### P2-3 — The panic-location test mutates the process-global hook and does not restore the previous hook

`tests/stack_unit.rs:270-282` takes the previous panic hook, moves it into a wrapper, installs that wrapper, then removes and drops the wrapper. `take_hook()` installs the default hook; it does not recover the previous hook captured inside the dropped closure. Thus the test permanently replaces any pre-existing harness/custom hook with the default for the rest of the test process.

The hook is process-global while libtest runs tests concurrently. Filtering by thread ID avoids capturing another thread's panic, but does not stop other tests from observing the temporary wrapper or the later loss of their previous hook. A panic before cleanup also leaks the wrapper.

Fix: avoid testing `track_caller` via a global hook if possible. Otherwise serialize hook mutation, retain the previous hook in recoverable shared state, and restore it with an RAII guard even during unwinding.

### P2-4 — The advertised fail-fast cfg diagnostics still produce secondary name-resolution errors

The crate emits explicit `compile_error!` for missing 64-bit atomics and for `--cfg loom` without the feature (`src/lib.rs:245-266`), but the imports immediately below are only gated on `loom` (`273-276`). The rest of the file remains active too.

Therefore:

- on a target without `AtomicU64`, normal code still tries to import/use `AtomicU64`;
- with `--cfg loom` but no `loom` feature, code still tries to resolve `loom::...`.

That contradicts README/changelog claims that the build fails with the named error *rather than* the cryptic unresolved import (`README.md:181-190`; `CHANGELOG.md:124-129`). `compile_error!` does not stop parsing/name resolution of sibling items.

Fix: place the implementation in a module gated by the valid configuration and re-export it, leaving only the intended `compile_error!` active in invalid configurations. Add compile-fail/UI coverage for both invalid configurations.

### P3-1 — The hot path pays stronger link ordering than its own proof requires

The trait requires `Release` link stores and `Acquire` link loads, while its own documentation correctly explains that the head's Release/Acquire publication already orders even Relaxed link accesses (`src/lib.rs:564-590`). `ArrayLinks` nevertheless emits Acquire/Release operations (`720-731`). On x86 these often compile identically, but on weakly ordered targets they may add real acquire/release instructions on every operation.

The stated “correct on its own terms” defence is not a semantic property: `load_next`/`store_next` are protocol methods whose meaning exists only through this stack. For minimum overhead, make shipped link accesses Relaxed and keep the ordering proof on the head. Model-check that exact implementation. If stronger ordering is intentionally retained as change-resilience, describe it as a deliberate portability cost rather than a correctness requirement.

### P3-2 — `push` performs an unnecessary initial Acquire head load

`push` starts with `self.head.load(Ordering::Acquire)` (`src/lib.rs:937`) but its failed CAS is Relaxed and the accompanying proof says push uses the observed word only as `(index, tag)` values and never follows a link (`src/lib.rs:965-983`). The same reasoning makes the initial load eligible for Relaxed. This can reduce barriers on weakly ordered architectures.

`pop`'s initial and failed-CAS observations must remain Acquire because it subsequently follows the selected link.

### P3-3 — Strong CAS and a fixed x86-derived backoff policy deserve target-specific measurement

Both loops use `compare_exchange`, despite already being retry loops (`src/lib.rs:984-1039`, `1174-1223`). `compare_exchange_weak` is equivalent on x86 but can avoid hidden retry work on LL/SC targets and let the crate's own measured backoff govern retries. This is an optimization candidate, not a correctness finding; benchmark on AArch64/other supported weakly ordered targets before changing it.

Likewise, the cap-6 decision is backed by one x86 laptop and short samples. The documentation is now honest about fairness and rare 41–60 ms outliers, but the fixed policy may not be optimal under oversubscription, single-core systems, SMT differences or non-x86 spin hints. Consider a policy/configuration seam only if multi-target measurements justify the API cost.

### P3-4 — The contention benchmark's denominator is not one shared timed window

Each worker computes its own deadline after returning from the barrier (`benches/tagged_index_stack_bench.rs:213-215`, `331-333`), while the coordinator separately records `start` after its own barrier return (`243-250`, `358-361`). Barrier participants are released together but resume at different times; operations are summed over worker-local one-second windows and divided by coordinator-to-last-join elapsed time. Scheduler skew therefore changes numerator exposure independently of the denominator and also affects per-thread fairness.

For stronger receipts, publish a shared start/deadline established before release (or an atomic stop flag controlled by one timer), include warm-up, use multiple samples with dispersion, and avoid drawing broad throughput conclusions from one sample per matrix cell. The current harness is useful for local comparison but not strong enough to underwrite universal hardware ceilings or defaults by itself.

### P3-5 — No package-isolation gate exists for this standalone crate

CI has crate-specific build, test, clippy, rustdoc, MSRV and loom rows, but no `cargo publish --dry-run -p tagged-index-stack`/package verification row. Neighboring standalone crates have explicit package gates. This matters because the manifest inherits workspace lints/dependencies and the published README points to repository-only evidence files.

Add a package dry-run gate that verifies the generated archive and its isolated build. This is about package integrity, not version management.

### P3-6 — The const-generic rejection is acknowledged but not regression-tested

`tests/stack_unit.rs:370-386` explicitly says `INDEX_BITS > 16` has no automated compile-fail test. This bound carries the entire documented minimum-tag and sentinel argument, so a hand-verified diagnostic is insufficient as the only negative oracle.

Add a small UI/compile-fail test for widths `0` and `17`, and pin that the failure names the range requirement. Also cover invalid target/cfg combinations from P2-4 in the same mechanism.

### P4-1 — Documentation volume and review-history residue obscure the executable invariants

The production file is 1,508 lines for a tiny data structure; the loom test is 1,110 lines. Many comments contain review-round IDs, task archaeology, old alternatives, exact one-machine benchmark samples and repeated copies of the same claims. Examples include `src/lib.rs:290-350`, `386-405`, `990-1033`, `1323-1508` and extensive CHANGELOG first-release archaeology.

This is “neuroslop” in the maintainability sense: not necessarily false line-by-line, but too much generated rationale around too little mechanism. It increases contradiction risk—the stale CI feature count is already one example—and makes the load-bearing proof harder to see.

Before release:

- keep one concise memory-ordering proof adjacent to `head` and one concise caller contract;
- move benchmark receipts and review history to repository docs;
- avoid “canonical”, all-caps emphasis and claims of exhaustiveness where the model is intentionally tiny;
- make API shape eliminate contracts instead of documenting their failure modes for hundreds of lines.

## Recent changes reviewed

The review inspected all crate changes from Sol-codex run-2 commit `47c81e9` through `45ca04d`, including:

- release-active invalid-link guard and cold tracked panic path;
- conditional lock-freedom wording and payload-alias prohibition;
- bounded/skip-on-empty backoff changes and fairness output;
- real-thread conservation test and retry/full-depth instrumentation;
- instrumentation gating behind `test-internals`/loom;
- loom oracle serialization and expanded retry models;
- README/CHANGELOG/hidden-item updates.

The changes close several prior findings, but P2-2 shows the newest instrumentation feature was not integrated into CI, and P1-1/P1-2 remain architectural rather than documentary problems.

## Positive observations

- Production code is `no_std`, allocation-free on success paths and forbids unsafe code.
- The release-sequence invariant is explicitly identified; push Release and pop Acquire/failure-Acquire roles are reasoned about coherently.
- H-2 running-tag preservation is implemented correctly and has positive/counterfactual loom coverage.
- Invalid numeric links now fail loudly in release and the panic path is cold/out-of-line.
- Default builds no longer pay retry-instrumentation atomics.
- Real-thread conservation complements tiny exhaustive loom scenarios.
- `is_empty` is honestly advisory and uses Relaxed appropriately.
- Array/backing false-sharing and starvation/fairness limitations are documented rather than hidden.

## Recommended release sequence

1. Redesign the head/backing relationship so P1-1 is unrepresentable.
2. Decide whether the product promises bounded ABA mitigation or a strong no-reuse guarantee; rewrite or redesign for P1-2.
3. Make unchecked packing private/explicitly truncating.
4. Wire `test-internals` into CI and fix the panic-hook test.
5. Isolate invalid cfg configurations so they emit only their intended diagnostics; add compile-fail coverage.
6. Add package-isolation verification.
7. Measure Relaxed links, Relaxed push load and weak CAS on at least x86-64 and AArch64 before selecting the final hot path.
8. Compress source docs after the API is final, retaining only load-bearing invariants.

After steps 1–6, perform another static review and then run the project's normal dynamic release gates outside this read-only review.
