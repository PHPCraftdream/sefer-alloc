# Checkpoint — 2026-07-03 4-agent review done, improvement plan queued

## Session summary

Продолжение sefer-alloc 0.3.x. За сессию: (1) реализованы ДВЕ перф-арки P0–P6
(эврики Э1–Э6) под-агентами @oh с личным построчным zero-trust + counterfactual
на каждой фазе — 256 B churn обогнан, M2 усилен (Э6); (2) всё запушено
(origin/main = HEAD = 3aaa4b1, 12 коммитов от 1e62cc3); (3) обновлены доки
свежими полными бенч-таблицами. Затем пользователь заказал **полное ревью
проекта 4 агентами @fxx** (read-only, параллельно) по областям: A=adversarial
memory-safety перф-слоя, B=конкурентность/xthread+покрытие, C=перф-фронтир
(cold tiny), D=Tier-F конформность/релиз. Все 4 отчёта получены и изучены.

**Главный результат ревью:** агенты A и B **НЕЗАВИСИМО** нашли одну дыру —
cross-thread double-free (магазин и RemoteFreeRing взаимно слепы: remote-free
в ring не ставит бит битмапа → own-free того же блока проходит оба Э6-оракула
→ блок в магазин; позже drain пишет write_next в ЖИВУЮ переизданную память +
double-issue + под decommit unmap живого сегмента). HIGH, pre-existing с
fastbin (0.3.0), пережила ревью #128–#143, наши арки её НЕ вносили (Э6 сузил).
Нет теста/loom. A также нашёл: magazine-push потерял `off>=bump` guard
(MEDIUM, фикс 1 строка). Всё НОВОЕ (Э1–Э6, 256-класс, P7-retire) атаки
выдержало — A задокументировал провалившиеся атаки. C: «cold 1.60×» — НЕ
page-faults (доки врут), а freelist-round-trip на steady-state; кандидаты
Э7–Э11 → прогноз 16B→1.1-1.2×, 64B→паритет. D: код публикуем как 0.3.0, но
перед тегом нужны доки (unsafe-инвентарь пропускает registry::bootstrap;
мёртвые env-vars SEFER_LARGE_CACHE_*; секция «do not deploy»; README ставит
version=0.3 при live 0.2.1).

**В конце сессии** из 4 отчётов составлен поэтапный план — **12 задач
#153–#164** (арки R/D/P7/F, цепочка blockedBy). Ничего из плана ещё НЕ начато —
ждёт команды «делай». babysit НЕ активен.

## Active goal

none (стоп-хук не активен).

## TaskList

Все pending, цепочка blockedBy. Порядок: R1→R2→{R3→R4→P7…, D1→D2}, F независимо.

### pending
- #153 R1: off>=bump guard в magazine-push + counterfactual (СТАРТ отсюда, не blocked)
- #154 R2: cross-thread DF честная граница M2 — доки + pinning-тест(ignore) + loom (blockedBy #153)
- #155 R3: санитайзеры — TSan production job + miri fastbin + loom-варианты (blockedBy #154)
- #156 R4: гигиена кодовых доков 40→49 и пр. (blockedBy #155)
- #157 D1: релизные доки — seams+bootstrap, M2-scope, env-purge, do-not-deploy… (blockedBy #154)
- #158 D2: CHANGELOG fold [Unreleased]→[0.3.0] + yank-notes (blockedBy #157)
- #159 P7.0: iai recycle_alloc_free two-round + фикс page-fault нарратива (blockedBy #156)
- #160 P7.1: Э9 classify/base-once (риск 0) (blockedBy #159)
- #161 P7.2: Э7 batch freelist drain + Э11 stamp-dedupe (главный рычаг) (blockedBy #160)
- #162 P7.3: Э8 batch flush same-segment (M2-критично) (blockedBy #161)
- #163 P7.4/5: Э10 branchless scan + финал/вердикт (blockedBy #162)
- #164 F: дизайн настоящего фикса ring↔magazine (blockedBy #154)

### recently completed (эта сессия, до ревью)
- #144–#149 арка P0–P5 (Э1–Э5), #150–#152 арка P6 (Э6) — всё запушено

## Decisions

- **R1 перед R2**: сначала дешёвая заплата реального кодового пробела
  (off>=bump), потом честная фиксация ДОКУМЕНТИРУЕМОГО лимита (ring↔magazine).
- **Настоящий фикс ring↔magazine — отдельная дизайн-задача F, НЕ блокер
  релиза.** Дыра pre-existing (есть и в live 0.2.1 fastbin), честно
  задокументировать остаточный лимит (как released-Large note) достаточно для
  0.3.0. Реальный фикс — 4 кандидата (drain-с-magazine-видимостью / per-heap
  bloom / conflict-list гибрид; ring-peek отклонён A как 256 loads/free).
- **Версия 0.3.0** (D-вердикт): 0.3.0 не опубликован, 0.2.x→0.3.0 верная
  pre-1.0 breaking-полоса; SMALL_CLASS_COUNT 48→49 pub(crate), type-invisible.
  Ничто не форсит 0.4.0.
- **Cold-перф: батч — рычаг.** steady-state cold = freelist round-trip (не
  bump, не page-faults); Э7 batch-drain + Э9 classify-once + Э8 batch-flush +
  Э10 branchless — тавтологии в batch-контексте, guards (is_free, off>=bump)
  остаются per-block. Нужен two-round iai (существующий cold-iai слеп).
- **P4 остаётся отклонённым** — но Э8 (batch flush) переоткрыт C с
  run-detection (снимает P4(a)-возражение о разбросе): cold-flush ~100%
  same-segment.

## Open questions

- **С чего начать реализацию** — план ждёт «делай». Дефолт: R1 (#153), не
  blocked. Или пользователь выберет подмножество (только R-арка? только доки
  перед тегом? перф?).
- **Тег + релиз 0.3.0** — после R+D арок (или раньше, если пользователь решит
  релизить с задокументированным лимитом). Затем yank 0.2.1 (A2-unsound
  fastbin). Отдельное явное решение.
- **F (настоящий фикс)** — до или после релиза 0.3.0? Не блокирует.
- shamir-db перепрогон — давний хвост, после публикации.
- Untracked чекпойнты — в коммит по решению пользователя.

## Repo state

```
## main...origin/main   (синхронизировано, дерево чистое)
?? docs/checkpoints/2026-07-03-4agent-review-improvement-plan.md   (этот файл)
```

```
3aaa4b1 docs(checkpoint): perf arc P0–P6 complete, unpushed
6b9ddd0 docs: refresh benchmark tables with a clean full run + System column
381a649 docs(#152): P6.2 — Э6 verdict, the 256 B churn loss is eliminated
828e94b perf(#151): P6.1 Э6 — M2 double-free oracle in hot metadata, not block body
06d30ac perf(#150): P6.0 writing-churn bench — measurement foundation for arc P6
```

origin/main = HEAD = 3aaa4b1 (запушено, синхронно). crates.io = 0.2.1 live
(0.1.0/0.2.0 yanked). Cargo.toml = 0.3.0, тег НЕ создан, publish НЕ выполнен.
Перф-арки под CHANGELOG [Unreleased]. CI последнего пуша был queued/зелёный.
