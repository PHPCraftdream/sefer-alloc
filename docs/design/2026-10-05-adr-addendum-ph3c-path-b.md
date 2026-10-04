# ADR addendum — Ph3c путь (б): off-body учёт отменён, P1 → P1-box (принятый известный дефект)

Дата: 2026-10-05. Исполнение конечной точки (б) из `docs/design/2026-10-02-adr-addendum-ph3c-escalation.md` §5 после
результата шага 1′ (`docs/perf/PH3C_B3P_STEP1PRIME_IAI.md`, коммит `49057e12`): исправленный B3 провалил
пред-регистрированные гейты (hot Ir +3.639% / EstCycles +2.517% при ≤ 1.02; refill/flush Ir +20.902% /
EstCycles +13.038% при ≤ 1.20 / ≤ 1.10), A/A чист, детерминизм побитовый. Решающих замеров по правилу
step1prime §2 использовано два (шаг 0 PG-3r — PASS; шаг 1′ — FAIL); новых геометрий в рамках этого ADR не будет.

## Текст поправок к ADR (`docs/design/2026-10-01-adr-physical-boundary-and-progress.md`)

- **Решение 1′ (заменяет п.1):** production-backend — slab с интрузивными связями free-list в телах свободных
  Small-блоков; off-body учёт свободных блоков отменён по результатам замеров (PG-3, PG-3r, шаг 1′). Пути retire
  и reuse (own-thread free, flush, refill, owner cut/trim) по-прежнему пишут/читают слово `next` в теле свободного
  блока (`Node::write_next`/`Node::read_next`).
- **G1′ (заменяет G1):** каждый `dealloc` — терминальная публикация без аллокации; free не теряется и не
  удваивается. Retire/reuse интрузивного free-list пишет слово `next` в тело СВОБОДНОГО Small-блока; это известный
  дефект P1-box (ниже).
- **P1-box (бывший P1):** принятый известный дефект, не MODEL-LIMIT. Если блок освобождён через by-value `Box`,
  кадр которого ещё исполняется, когда аллокатор вписывает связь free-list в тело блока (owner reclaim
  cross-thread free или более поздний flush магазина), Miri в Stacked Borrows и Tree Borrows сообщает UB
  (нарушение protector). Нативных крашей и miscompilation не известно; aliasing-модели экспериментальны, это не
  гарантия их отсутствия. Повторная выдача этих байтов при живом освобождающем кадре — отдельный MODEL-LIMIT,
  общий для любого аллокатора на общей reservation.
- **Стоп-линии (замена):** вместо «Не возвращать красный путь интрузивного slab как быстрый fallback» и «откат ≠ GO»
  (план §3): «release GO не заявляет SB/TB-чистоту установленного `Box` при паузе producer'а; формулировка
  "Miri-clean" без этой оговорки запрещена». Не переводить P1-box в MODEL-LIMIT.
- **Переоткрытие P1-box** только по внешней причине: (1) нормативное решение Rust о protector'ах `Box`;
  (2) нативное воспроизведение (крах/miscompilation); (3) новое решение владельца с новой ценой.

## Фазы после (б)

Ph3c — CANCELLED. Ph4a — стартует сразу, база B_4a = SHA `main` при создании worktree, Loom-модели на текущем API
drain. Ph4b, Ph4c — без изменений. Ph5a — CANCELLED. Ph5b/Ph5c — упрочнение slab (C2/C3/C5/C6/C7 обязаны быть PASS,
C1 = KNOWN-DEFECT P1-box, C4 = MODEL-LIMIT). Ph6 — без A/B смены backend (остаётся A/B накопительно к B0 `7d80d7e1`
и патчу S). Ph7 — статус KNOWN-DEFECT с сигнатурой причины.

## CI

Четыре ожидаемо-красных miri-шага {production, alloc-global} × {SB strict, TB} уже усилены (`fd79861c`): exit ≠ 0,
текст UB, место (`Node::write_next` в `node.rs` с вызывающим `reclaim_sidecar_record`/`flush_run`/`dealloc_small`);
зелёный witness или красный в другом месте роняет шаг.

## Артефакты

Спайк B3 сохранён: теги `archive/ph3c-b3s-093c9d23`, `archive/ph3c-b3p-fd3cdb9e`, патч
`docs/perf/PH3C_B3P_SRC.patch`; Miri-логи закрытия P1 на спайке — `docs/perf/_raw_ph3c_b3s_miri_*.log`,
`docs/perf/_raw_ph3c_b3p_miri_*.log`. Патч S (скан с первого слова payload) — независимое улучшение
(`docs/perf/PG3R_SCAN_PATCH_IAI.md`, perf item 80), вносится отдельным `perf(runtime)`.
