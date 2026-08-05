# Release-readiness readonly review — R34 / remediation wave

Диапазон: `40241b0810b42c672f3f7c507f21b2de762b782b..HEAD`  
HEAD при ревью: `4623dc3a87742d4dc416398d64cf673cb1986e33`  
Режим: строго readonly по коду; `cargo`/tests/builds/benches/Miri/Kani/loom/clippy/fmt/scripts не запускались. Единственная запись — этот отчёт.

## Вердикт

**CONDITIONAL-GO.** Блокеров уровня UB/UAF/double-free/OOB/data-race в новой волне по прочитанному diff/code не нашёл. Для релиза условие одно: перед публикацией нужна фактически зелёная release/CI-проверка на чистом дереве, потому что в рамках этого задания проверки запускать было запрещено.

Ключевой вывод по скорости: **эта волна не является production-speedup wave.** `CHANGELOG.md:12` честно фиксирует `Runtime improvements this round: 0`; `Cargo.toml:399` показывает, что `production` не включает новых perf-фич и не включает `internals`. Изменения в `src/` в диапазоне — в основном correctness/proof/diagnostic hardening: ordering fix, bounded decay catch-up, large-cache-hit field reset, free-path OOM policy, panic-safety guards, stack-size pin, API-boundary gating.

## Findings

### P0

Нет подтверждённых P0.

Проверенные поверхности: `RemoteFreeRing` push/drain orderings, large-cache hit reuse publication, registry chunk OOM path, fallback init lock/state guards, owner/deferred-field reset, `internals` cfg boundary. Подтверждённого UB/UAF/double-free/OOB/race/ABA/provenance hole в новых landing commits не нашёл.

### P1

Нет подтверждённых P1.

### P2 — release condition, not code defect: текущий аудит не заменяет чистый CI/release gate

Подтверждение: задание прямо запрещало `cargo`/tests/builds/scripts, поэтому этот отчёт не может подтвердить фактическую сборку/линковку/тесты. При этом релизовый workflow теперь делает правильную пару для root crate: `cargo test -p "$NAME" --lib --features production` и затем `cargo test -p "$NAME" --features "production internals"` (`.github/workflows/release.yml:201-202`). Это правильная форма после R34-3, но её надо увидеть зелёной на чистом checkout перед publish.

Риск: без этого остаётся только статическое чтение diff. Оно достаточно для NO-GO при найденном дефекте, но не достаточно для GO на публикацию allocator crate.

Рекомендация: **CONDITIONAL-GO** до зелёного release workflow / эквивалентного чистого pre-publish run.

### P3 — evidence self-containment: committed docs cite untracked review artifacts

Подтверждение: `CHANGELOG.md:44`, `docs/perf/round-manifests/R34_MANIFEST.md:4` и `docs/perf/round-manifests/R34_MANIFEST.md:224` ссылаются на `docs/reviews/2026-08-05-round34-readonly-review.md`; `git status --short` показывает этот файл untracked, а `git ls-files docs/reviews` не включает его. Это не runtime-дефект и соответствует локальной convention "readonly review reports stay uncommitted", но clean clone / crates.io source package не сможет открыть источник, на который committed release docs ссылаются как на доказательство.

Риск: доказательность release notes хуже, чем у gate reports с tracked raw/CSV. Особенно заметно, потому что R34 позиционирует себя как evidence-driven remediation wave.

Рекомендация: перед финальным релизом либо commit конкретные review artifacts, на которые уже ссылаются committed docs, либо заменить ссылки на tracked checkpoint/manifest excerpts. Не блокирует кодовый релиз, но блокирует идеальную самодостаточность доказательств.

### P3 — CI prose drift: clippy row count comment stale after adding `production internals`

Подтверждение: `.github/workflows/ci.yml:159-160` всё ещё говорит о "5 clippy rows", но фактически job содержит 6 clippy steps (`default`, `experimental`, `--all-features`, `hardened medium-classes`, `production`, `production internals`; см. `.github/workflows/ci.yml:106-151`). `scripts/check-matrix.mjs:71-171` тоже содержит 6 `kind: 'clippy'` rows plus non-clippy rows.

Риск: только process/doc drift. Механика выглядит корректной: hand-transcribed clippy rows и manifest row set совпадают по смыслу; сама stale-фраза не ломает CI.

Рекомендация: поправить комментарий с "5" на "manifest clippy rows" или "6" в следующей docs cleanup.

## Проверка опасных поверхностей

- `RemoteFreeRing` shadow-head: R34-6 поменял `cached_head` fast-path load на `Acquire` и slow refresh store на `Release` (`src/alloc_core/remote_free_ring.rs:1107`, `:1127`). Это закрывает proof gap без изменения value-domain invariant. Drain publishes `head` через RAII guard on drop (`src/alloc_core/remote_free_ring.rs:854-866`, `:1272-1307`). Подтверждённого race/ABA regression не нашёл; остаточный wrap/stale-high hazard остаётся документированным экстремальным risk, не новым blocker.

- Large-cache hit reuse: R34-14 явно сбрасывает `owner_state`, `deferred_next`, `owner_thread_free` до `register()` (`src/alloc_core/alloc_core_large.rs:383-424`). Это восстанавливает поведение старого full-header constructor path и закрывает реальный permanent-leak defect с stale `deferred_next`. Окно `register -> segment_id patch` не даёт remote path использовать stale `segment_id`, потому что `owner_thread_free` ещё null и cross-thread free returns before dirty-bit routing (`src/registry/heap_core_xthread.rs:921-947`).

- Registry free-path OOM: `slot_or_none`/`try_ensure_chunk` возвращают `None` вместо abort на free path (`src/registry/bootstrap.rs:595-601`, `:662-668`), а новые callers range-check owner id and bail (`src/registry/heap_core_xthread.rs:338-358`, `:1538-1550`, `:1591-1601`). Alloc path abort policy сохранён (`src/registry/bootstrap.rs:631-652`). Это conditional graceful leak/drop, не memory corruption.

- Fallback init/lock panic-safety: `InitStateGuard` rolls `INIT_STATE` back to `UNINIT` on unwind (`src/global/fallback.rs:373-403`); `LockGuard` releases fallback spinlock on unwind (`src/global/fallback.rs:327-350`). В production panic escaping `GlobalAlloc` aborts; guard mainly prevents test/invariant-panic livelock.

- Decay catch-up: R34-11 caps work at `DECAY_CATCHUP_MAX_STEPS = 8` (`src/alloc_core/alloc_core_large_cache.rs:49`) and advances timer by `due * interval` (`src/alloc_core/alloc_core_large_cache.rs:528-545`). No OOB/race surface; worst case is bounded extra eviction work per clock read.

- `internals` boundary: `production` does not imply `internals` (`Cargo.toml:399`, `:455`); module paths are `pub` only with `internals`, otherwise `pub(crate)` (`src/lib.rs:329-379`), while crate-root re-exports remain available (`src/lib.rs:395-411`). Shape is semver-positive.

## Future speedup/readiness plan

1. **Before release:** require green release workflow / clean CI over `production` and `production internals`; this report did not run it.
2. **Next perf wave:** do not count R34 as a speedup wave. Treat it as correctness/evidence stabilization.
3. **Best future acceleration candidates:** sub-16 KiB realloc ladder remains genuinely unmeasured after the ~40x realloc claim correction (`docs/perf/OPEN_ITEMS.md:1188-1190`); page-run layer and small-magazine provenance designs are currently research/NO-GO leaning and need real consumer/workload evidence before implementation.
4. **Evidence hygiene:** avoid committed docs depending on untracked review files unless the convention changes; prefer tracked manifests/gate reports for release-facing citations.

## Final

**CONDITIONAL-GO**: no code-level release blocker found in the read-only audit; release only after green clean release/CI because verification execution was intentionally out of scope here.
