# Checkpoint — 2026-07-03 R+S+P7 arcs done (pre-Shabbat), unpushed

## Session summary

Продолжение sefer-alloc 0.3.x после 4-агентного ревью. За эту сессию по плану
#153–#168 (арки R/S/P7) реализовано **17 фаз под-агентами @ox/@fxx с личным
построчным zero-trust + counterfactual на КАЖДОЙ и коммитом между фазами**:

- **Reliability R1–R4:** R1 off>=bump guard в magazine-push (реальный пробел
  защиты, найден ревью-агентом A); R2 честная граница M2 для ring↔magazine
  cross-thread DF (#164, НЕ фикс — доки + pinning-тест `#[ignore]` + loom-модель;
  loom попутно доказал, что НАИВНЫЙ фикс тоже дыряв); R3 санитайзеры для
  production (TSan + miri для fastbin — 0 гонок, 0 UB); R4 гигиена кодовых доков
  (40→49 классов и пр.).
- **Stress S1–S3:** S1 concurrent boundary-hammer (канарейки, 1с, heavy opt-in
  через SEFER_STRESS_HEAVY); S2 exhaustive single-thread sweep (2142 кейса,
  0.5с); S3 прогон S1 под TSan / S2 под miri (урезанные бюджеты через env/
  cfg(miri), нативное не тронуто). ВСЕ строго в safe-envelope (легальный
  GlobalAlloc; double-free/foreign — «сам себе Буратино», вне зоны).
  **Ни S1/S2, ни санитайзеры НОВЫХ багов не нашли.**
- **Perf P7 (P7.0–P7.4):** P7.0 two-round recycle iai-бенчи + фикс page-fault
  нарратива (открытие агента C: cold=freelist-round-trip, НЕ page-faults);
  Э9 classify/base-once; Э7 batch freelist drain (главный cold-рычаг) + Э11
  stamp-dedupe; Э8 batch flush (M2-критично, is_free/off>=bump остались
  per-block); Э10 branchless chunked M2-скан. P7.5 — честный вердикт.

Личные counterfactual'ы этой сессии (ломал через Edit, НЕ git checkout —
урок прошлой сессии усвоен): R1 bogus-uncarved→RED; R2 pinning `--ignored`→RED
(sentinel clobbered); S1 канарейка→CANARY CORRUPTION; Э7 лишний inc_live→D1 RED;
Э8 отключить is_free→ring-DF double-issue RED; Э10 round-up bound→stale-slot RED.

**Производительность (шумный Windows, ratios — сигнал):** реалистичный
(пишущий) churn — ЛИДИРУЕМ на всех размерах (16B 1.63×, 64B 1.69×, 256B 1.14×,
1024B 5.4× быстрее mi); large 13–34×; единственное отставание — cold tiny
16–64B (~1.15–1.5× медленнее, инструкционно на freelist-refill, НЕ page-faults).
P7 — инструкционная оптимизация; wall-clock на шумном хосте её НЕ разрешает,
детерминированное доказательство — iai Ir-гейт на Linux CI (сработает после
пуша). ~1.1–1.2× для 16B НЕ заявлены достигнутыми — ждём Ir. Ни одна гарантия
не ослаблена (M2 усилен в Э6).

**Состояние: 13 коммитов впереди origin/main, НЕ запушено, дерево чистое.**
Пользователь ушёл на Шаббат — продолжим после. babysit НЕ активен, фоновых
агентов НЕТ.

## Active goal

none (стоп-хук не активен).

## TaskList

### pending (осталось после R/S/P7)
- #157 D1: релизные доки — unsafe-seams инвентарь(+bootstrap), M2-scope,
  env-purge, do-not-deploy, production+fastbin, 1024-ceiling, счётчики
  (blockedBy #154 ✅ → РАЗБЛОКИРОВАНА)
- #158 D2: CHANGELOG fold [Unreleased]→[0.3.0] + yank-notes (blockedBy #157)
- #164 F: дизайн настоящего фикса ring↔magazine cross-thread DF
  (blockedBy #154 ✅ → РАЗБЛОКИРОВАНА; вход — R2 pinning-тест + loom-модель)
- #167 H1: защита от unsafe-злоупотреблений (mismatched-layout, interior-ptr…)
  ТОЛЬКО без перф-регрессий (иначе за фичу `hardened`) (blockedBy #163 ✅ →
  РАЗБЛОКИРОВАНА). НЕ начата (помечал in_progress, но агента не запускал —
  вернул в pending).

### recently completed (эта сессия)
- #153 R1, #154 R2, #155 R3, #156 R4 (reliability)
- #165 S1, #166 S2, #168 S3 (stress)
- #159 P7.0, #160 P7.1(Э9), #161 P7.2(Э7+Э11), #162 P7.3(Э8), #163 P7.4(Э10)+P7.5

## Decisions

- **Пробой-стресс S1/S2 — строго из safe-кода** (легальный GlobalAlloc-envelope);
  защита от unsafe (double-free/foreign) остаётся в ОТДЕЛЬНЫХ детерминированных
  M2-тестах (regression_magazine_oracles и др.), не смешивать. H1 — отдельная
  задача для РАСШИРЕНИЯ защиты, но БЕЗ перф-регрессий (замер iai+wall на каждый
  guard; платные — за `hardened` фичу default-off; honest-reject валиден).
- **Обход границ под санитайзерами** (S3): S1→TSan, S2→miri (loom неприменим —
  он для маленьких рукотворных моделей); урезать бюджеты под санитайзер, нативное
  не трогать.
- **Настоящий фикс ring↔magazine — задача F, НЕ блокер релиза** (дыра
  pre-existing в live 0.2.1 fastbin тоже; честно задокументирована). loom
  показал: наивный «own_free читает ring» — тоже дыряв (симметричная нога) →
  фикс требует, чтобы drain видел магазин.
- **P7 перф — не переклеймлять**: wall-clock шумный, реальное доказательство —
  Linux iai Ir; guard'ы (is_free/off>=bump/mark_alloc/dec_live) остаются
  per-block, батчатся только тавтологии (set_head/head-read/inc_live/bump-load).
- **Э8 batch-flush — IMPLEMENTED, не rejected** (splice доказан byte-identical;
  P4(a)-возражение про разброс снято run-detection).

## Open questions

- **Порядок оставшегося** (D/F/H1 независимы). После Шаббата: пользователь
  выбрал «всё по порядку сам» ранее — вероятно продолжить D1→D2 (релиз-доки →
  тег) ИЛИ F (снятие лимита) ИЛИ H1. Уточнить приоритет или продолжать по
  blockedBy.
- **Пуш 13 коммитов + тег 0.3.0** — после D1/D2 (или раньше по явной команде).
  Затем yank 0.2.1 (A2-unsound fastbin). Отдельное решение пользователя.
- **Linux CI после пуша** — подтвердит TSan production, miri fastbin, iai Ir
  (в т.ч. новые recycle_* и стресс-под-санитайзерами S3). Присмотреть.
- shamir-db перепрогон — давний хвост, после публикации.

## Repo state

```
## main...origin/main [ahead 13]
?? docs/checkpoints/2026-07-03-reliability-stress-perf-p7-done.md  (этот файл)
(рабочее дерево иначе чистое)
```

```
055061a docs(#163): P7.5 — honest cold-recycle verdict for the P7 arc (Э7–Э11)
e6a1eaf perf(#163): P7.4 — Э10 branchless chunked in-magazine M2 scan
ff4a1af perf(#162): P7.3 — Э8 batch flush (same-segment runs in flush_class)
ae7afe1 perf(#161): P7.2 — Э7 batch freelist drain + Э11 stamp-dedupe (main cold lever)
8e69bff perf(#160): P7.1 — Э9 classify-once + base-once on the HeapCore faces
```

origin/main = 3aaa4b1 (0.3.0 арка P0–P6, запушено ранее). crates.io = 0.2.1
live (0.1.0/0.2.0 yanked). Cargo.toml = 0.3.0, тег НЕ создан, publish НЕ
выполнен. Перф-арки P0–P7 под CHANGELOG [Unreleased]. Все локальные оси зелёные
(production suite ×2, clippy prod+all-features, fmt); Linux-only TSan/miri/iai
подтвердит CI после пуша.
