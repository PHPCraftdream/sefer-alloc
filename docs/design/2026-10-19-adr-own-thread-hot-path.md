# ADR: однопоточный горячий путь (own-thread alloc/free) — исполняемая спецификация D3, этап 1: per-class страницы (раунд 4)

Статус: **ПРОЕКТ, раунд 4 — исполнимая спецификация ТОЛЬКО этапа 1 (per-class страницы, фича
`small-per-class-pages`) для ИЗМЕРИТЕЛЬНОГО прототипа; production не меняется до прохождения всех гейтов**.
Дата: 2026-10-19. База: ветка `b4-hot-path`, R3 @ `74156f33`.

**Решение оркестратора (раунд 4, обязывающее для этого документа) — СУЖЕНИЕ:**

- Этап 2 (новый протокол межпоточного free: per-page pending, dirty-слова, формы A-B) **УБИРАЕТСЯ**
  из рекомендаций. Межпоточный free, credit, терминальная публикация, strict trim, sidecar-bitmap
  остаются **ровно как сегодня**. Ошибки этапа 2 из ревью 3 фиксируются в §7 «Отклонено/открыто»
  с объяснением — **без попыток спасти эти конструкции** (§7).
- ADR = исполнимая спецификация этапа 1: per-class геометрия страниц/span'ов для ИЗМЕРИТЕЛЬНОГО
  прототипа за opt-in фичей `small-per-class-pages` (§3, §5).
- **Все ценовые утверждения — ОЦЕНКИ до прототипа.** «Неослабление гарантий» — *цель спецификации*,
  проверяемая proof-набором §6, не факт. Формулировки «единственный путь» и «статус-кво»
  не используются; допустимо только «среди рассмотренных вариантов».

Числовая база — построчная разметка `D:/dev/rust/sefer-alloc/.rush/art/b4/hot_path_report.md`
(далее — отчёт B4-hot; детерминированный callgrind, larson T=1, production, маргинальный N/2N).
Связанные документы: `docs/design/2026-10-01-adr-physical-boundary-and-progress.md`,
`docs/REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md` (далее — контракт; в раунде 4 не пересматривается).

## 0. Статус раунда 4

- **База = R3: larson T=1 323 Ir/op, mstress T=1 499** (отчёт B4-hot §0:7, §4:53–56). Шаг 1
  (317/495) владельцем **отклонён**; число 317 нигде не используется — ни как база, ни как выигрыш,
  ни в арифметике. Собственный код: 323 − ≈134 (харнесс) ≈ **189–190 Ir/op**; mimalloc larson
  T=1 = 210.5 (отчёт B4-hot §0:7).
- **Этап 2 удалён из рекомендаций** (решение оркестратора); межпоточный протокол не трогается —
  ошибки его прежних конструкций задокументированы в §7 без «спасения».
- **Сводка исправлений по findings ревью 3** (`D:/dev/rust/sefer-alloc/.rush/art/b4/adr_review3.md`):
  | Finding (ревью 3) | Где устранён |
  |---|---|
  | **P1 pending-бит ≠ persistent lifetime admission** (строки 40–55) | §7(1): конструкция отклонена целиком; точный разбор — `publish` не проверяет `fetch_or` (`sidecar_bitmap.rs:118–126`), `BitmapScan::next_cut` очищает слово до обработки (`bitmap_scan.rs:49–85`), `BitmapCut::pop` не читает used/ledger (`bitmap_cut.rs:21–35`), free после reclaim ловится owner-side state-check (`alloc_core_small_reclaim.rs:48–54`), а не pop-абортом. Прежние обещания §3.4(2–3) ревью 3 (prev=1 ровно одному, `loom_page_incarnation_aba`) отозваны вместе с этапом 2 |
  | **P1 форма B нарушает inclusive strict trim** (строки 57–71) | §7(2): конструкция отклонена, контрпример с остановленным соседним producer'ом в том же слове зафиксирован против контракта §1/§7 |
  | **P1 owner-only dirty-слово не обнаруживает новые remote frees** (строки 73–81) | §7(3): конструкция отклонена; discovery либо полный, либо требует producer-side записи — обе альтернативы вне этапа 1 |
  | **P1 этап 1 — план, а не исполняемая спецификация** (строки 83–96) | §3: дескриптор страницы/span (§3.1), O(1)-поиск span'а и класса (§3.2), формула origin/stride с однозначным началом отсчёта (§3.3), multi-page span (§3.3), таблица проверок free по коду `small_free_guard` (§3.4), таблица переходов состояний с тремя независимыми структурами — **без единого used-бита** (§3.5), правила назначения/переназначения страниц (§3.6–3.7) |
  | **P2 арифметика бюджета** (строки 98–110) | §4: сумма статей сверена к **189–190** (не 189.7; в R3 сумма была 184.7); refill 14.7 — inclusive и НЕ разлагается повторно; issue-цепочка отдельно не выделяется (после R3 guards раз на run — `issue_transaction.rs:14–43`); ложные конверсии байт (32 KiB vs 2 KiB) и диапазонов отозваны вместе с прогнозом 125–138 |
  | **P2 proof-набор: невыполнимые/вакуозные клетки** (строки 112–124) | §6: каждая клетка = код + оракул + красный мутант + имя теста; Miri-позитив — **под фичей** `small-per-class-pages`; оракул «второй producer обязан увидеть prev=1» и UAF на paused-Box отозваны вместе с этапом 2 |
  | **P3 правила не перенесены дословно** (строки 126–132) | процессное: полный блок правил среды переносится в любое задание под-агента (как в §0 раунда 3) |
- Прогноз собственного кода 125–138 Ir/op (R3 §4.3) **отозван**: он опирался на удалённый этап 2
  и на неверную арифметику. Этап 1 сам по себе даёт лишь −0…−4 Ir/op на free-плечо (§4).

## 1. База

ИЗМЕРЕНО (отчёт B4-hot, callgrind, larson T=1, production, маргинальный N/2N):

| Величина | Значение | Источник |
|---|---|---|
| SeferAlloc, larson T=1 | **323 Ir/op** | отчёт B4-hot §0:7, §4:55 |
| SeferAlloc, mstress T=1 | **499 Ir/op** | отчёт B4-hot §4:55 |
| mimalloc, larson T=1 | **210.5** | отчёт B4-hot §0:7 |
| харнесс (mpmc) | ≈134 Ir/op | отчёт B4-hot §0:7 |
| собственный код | ≈189–190 Ir/op | 323 − 134 |
| шаг 1 (317/495) | **отклонён владельцем, не используется** | отчёт B4-hot §4:53–56 |

Пофайловая разметка (отчёт B4-hot §1; все числа ИЗМЕРЕНО, если не помечено иначе):
alloc-hit `hot.rs` 13.7 Ir/op, из них `clear_magazine_on_issue` ≈7 (`hot.rs:35–46`);
refill-хвост `finish_magazine_refill` ~14.7 — **inclusive** (`hot.rs:66–89`, отчёт §1.1:20);
free-цепочка `dealloc_own_base.rs` 32.5 Ir/op: `kind_at` ~3, `payload_start` 3 (`:309`),
`is_in_magazine` 5 (`:312`), `bump_of` 2 (`:315`), `alloc_bitmap.is_free` 2 (`:318`),
`divides()` 6 (сайт `:422`), tcache push ~12 (вход `:377`), overflow-flush ~8.2 (`:504`);
`class_for` ~20 (10 alloc + 10 free); root-lookup free-пути ~12; reclaim-скан ~5;
`try_drain_freelist_batch` ~8.6. Атомиков на own-пути в production нет (отчёт §1.3).

## 2. Переквалификация точечных вариантов (сжато, выводы раунда 3)

Полные разборы — раунды 2–3; здесь выводы с кодовыми привязками.

| Вариант | Вывод | Обоснование (код/тесты) |
|---|---|---|
| **A** (штамп magazine-резиденции в word1) | **Отклонён по трём контрпримерам**: ложный fatal на легальной программе (против `t_false_positive_handled`, `tests/heap_core_tcache_m2.rs:286–305`); обход оракула при перезаписи штампа — причина удаления pre-Э6-фильтра (`dealloc_own_base.rs:165–169`); «bitmap-арбитр» несовместим с вычетом его обновлений. Hint-форма даёт 0 Ir/op | раунд 2 §2.1 |
| **B** (per-batch issue/credit) | **Поглощён R3, выигрыш ~0**: `IssueRun` с guards раз на run (`issue_transaction.rs:14–43`), пакетные carve/drain (`alloc_core_small_impl.rs`: carve_block/try_carve_batch), один `sub_live(accepted)` на flush (`alloc_core_small_magazine.rs:671–673`) | раунд 2 §2.2 |
| **C** (ленивые оракулы на flush) | **Ослабляющий**: переносит момент детекции двойного free на flush; в `hardened` недопустим | раунд 2 §2.3 |
| **D1** (page map как авторитет класса) | **Ослабляющий в текущей mixed-class геометрии**: PageMap хранит только первый класс («first class wins», `segment_header_impl.rs:179–193`), diagnostic-only (`descriptors.rs:27–48`), контрпример — `tests/phase13_3_dealloc_layout_class.rs:96–113` (проверяет **невакуозность** mixed-class предпосылки). Чинится именно этапом 1 (§3) | раунд 2 §2.4 |
| **D2** (плоский per-class каталог) | До ~10 Ir/op, но write-set пересекается с PR #76/R1; отдельный ADR после их закрытия | раунд 2 §2.5 |
| **Этап 2** (per-page pending / формы A-B / dirty-слова) | **Удалён решением оркестратора (раунд 4)**; ошибки конструкции — §7. Межпоточный free остаётся ровно как сегодня (sidecar-bitmap, `sidecar_bitmap.rs:101–126`) | §7 |

## 3. Исполнимая спецификация этапа 1: per-class страницы

### 3.1. Дескриптор страницы/span

Сегодня: `PageMap` — 1 байт на страницу, `FOOTPRINT = PAGES_PER_SEGMENT * 1` байт
(`descriptors.rs:59`); `PAGES_PER_SEGMENT = SEGMENT / PAGE = 4 MiB / 4096 = 1024`
(`segment_header_impl.rs:144`); таблица diagnostic-only, «NOT load-bearing for class routing»
(`descriptors.rs:27–48`); её место в layout фиксировано (`segment_header_layout.rs:13–18`,
`small_meta_end()` — page-aligned past метаданных, `segment_header_layout.rs:60`).

Под фичей `small-per-class-pages` таблица расширяется до **авторитетной per-page записи 4 байта**:

```
u32: class:8 | span_start_page:12 | span_len_pages:12
классы: 0 = Free, 1 = Meta (sentinel), иначе class index (2.., кодируется class+2)
span_len_pages: длины >4095 не выражаются — гейт compile-time assert (см. ниже)
```

- Объём: `1024 × 4 = 4096` байт = **4 КиБ на сегмент** (против 1 КиБ сегодня, +3 КиБ).
- Запас метаданных: `small_meta_end()` выравнивается к `PAGE` (`segment_header_layout.rs:38–60`),
  поэтому +3 КиБ в худшем случае сдвигают payload_start на одну страницу (4 КиБ) —
  ОЦЕНКА 0.1% payload сегмента 4 MiB; точный layout (укладывается ли прирост в текущую
  выравненную дыру или добавляется страница) — **задача реализации**; гейт — **compile-time
  assert**: `const _: () = assert!(Layout::small_meta_end() <= SMALL_META_CAP_UNDER_FEATURE);`
  с константой, зафиксированной в этом ADR как «payload не теряет более одной страницы».
- Альтернатива (одной строкой): отдельная span-таблица (дескрипторы только по span'ам) +
  `u16` page→span индекс — отвергается для прототипа как два cache-line вместо одного
  (ОЦЕНКА), рассматриваться не будет, если прототип покажет приемлемую цену формата u32.
- **`PAGE = 4096` — минимальная OS-гранула** (`crates/aligned-vmem/src/page.rs:18`), а
  `page_size()` может быть больше (компилятор-время недоступно; `MAX_REALISTIC_PAGE_SIZE`
  используется для decommit-границ, `segment_header_layout.rs:50–58`). Геометрия страниц
  этапа 1 — **константа аллокатора** (`PAGE`), и **корректность от OS page_size НЕ зависит**:
  все структуры (bitmap'ы, дескрипторы, bump) индексируются по allocator-PAGE. От OS page_size
  зависит только decommit/recommit — как сегодня, округление до реальной `page_size()`
  (`small_decommit_start`, `segment_header_layout.rs:50–58`).

### 3.2. Поиск начала span и класса за O(1)

```
p              = off / PAGE                 // off = ptr − segment_base; PAGE — степень двойки ⇒ сдвиг
d              = desc[p]                    // 1 load u32 из дескрипторной области
span_base_page = d.span_start_page
span_base      = span_base_page * PAGE
class          = d.class
```

Стоимость: 1 load u32 + арифметика — **ОЦЕНКА ~2 Ir**. Дескриптор каждой страницы multi-page
span'а ссылается на `span_start_page`, поэтому нормализация не требует обхода.

### 3.3. Формула block origin/stride; multi-page span

Для freed-указателя с offset `off` в span'е класса `c`:

```
bs              = block_size(c)
off_from_span   = off − span_base
valid           ⟺ (off_from_span % bs == 0) И (off_from_span / bs < capacity)
capacity        = span_len_pages * PAGE / bs
```

- **Кратность СТРОГО от `span_base`** — начало отсчёта однозначно (одна страница-владелец span'а),
  чем устраняется неоднозначность `off % bs == 0` из ревью 3 (`adr_review3.md:91`).
- **Oracle 1.5-аналог (per-span bump):** span хранит bump (высоту carve) — проверка
  `off < span_base + span.bump` (вместо сегментного `bump_of`, `dealloc_own_base.rs:315`).
  Свободная хвостовая область span'а ⇒ reject, как сегодня.
- **Multi-page span:** классы с `bs > PAGE` получают span из `k = ceil(bs / PAGE)` смежных
  страниц (`k` ≤ 63 для `SMALL_MAX` ≈ 253 KiB при default 49 классах; `size_classes.rs:22–44,246`).
  Дескриптор **каждой** страницы span'а содержит один и тот же `span_start_page` и `class`;
  span не меняет класс, пока не освобождены **все** его блоки (§3.6).
- **Предел охвата прототипа:** этап 1 покрывает все default small-классы (49, `bs ≤ ~253 KiB`,
  span ≤ 62 страницы). **Medium-классы** (расширение до 55/58 классов, `SMALL_MAX` 1 MiB /
  1.75 MiB, `size_classes.rs:35–39,22–44`) — **вне прототипа этапа 1**: под `medium-classes`
  фича не включается и путь остаётся сегодняшним. Причина: span до 448 страниц истощает
  12-битное поле и качественно меняет дефрагментацию; решение по §8(б). Large — вне скоупа
  (свой путь, `BlockKind::Large`).

### 3.4. Проверки free (по коду `small_free_guard`, `dealloc_own_base.rs:284–323`)

Порядок guard'ов M2 («in-magazine, THEN stale-free, THEN flushed-bitmap», коммент
`:298–301`) сохраняется. Таблица: | шаг | сегодня | под фичей | Ir (ОЦЕНКА, кроме ИЗМЕРЕНО) |

| # | Проверка | Сегодня | Под фичей | Цена |
|---|---|---|---|---|
| 1 | `kind_at` + decode | `:284–286` (отчёт §1.2: ~3) | **без изменений** | ~3 (ИЗМЕРЕНО) |
| 2 | `payload_start` cmp | `:304–311` (3) | без изменений / продлён до span-meta границ (заголовки span'ов лежат в метаданных до `payload_start`) | ~3 (ИЗМЕРЕНО; +0…+1) |
| 3 | **span lookup + class** | — | новый: `desc[p]` → `span_base`, `page_class` (§3.2) | ~2 (ОЦЕНКА) |
| 4 | **stride от span_base** | `divides()`, сайт `:421–427` (6) | заменяется: `off_from_span % bs == 0` с однозначным началом (§3.3) | ~2–3 (ОЦЕНКА, замена 6) |
| 5 | **соответствие Layout-класса классу страницы** | `class_for(size,align)` уже выполняется на free-side (~10 Ir, НЕ удаляется, отчёт §1.3) → затем `divides` как слабый фильтр; aligned wrong-class **проходит сегодня** (коммент `:413–416`) | + проверка `c == page_class` (~1–2). Aligned wrong-class free под фичей **отвергается** | +1–2 (ОЦЕНКА) |
| 6 | `is_in_magazine` | `:312–314` (5) | **без изменений** | 5 (ИЗМЕРЕНО) |
| 7 | `bump_of` → **span-bump** | `:315–317` (2) | per-span bump вместо сегментного; сила та же (oracle 1.5) | ~2 (ИЗМЕРЕНО сегментный; span-аналог ОЦЕНКА без изменения силы) |
| 8 | `alloc_bitmap.is_free` | `:318–320` (2) | **без изменений** | 2 (ИЗМЕРЕНО) |

Пункт 5 — усиление: сегодня aligned wrong-class free проходит (`:413–416` — «aligned wrong-class
pointers can pass»); под фичей page-class известен точно, mismatch ⇒ reject. Это **не меняет
valid-use поведение** — только invalid-use reject; классифицируется как «сильнее, не ослабление»
(изменение наблюдаемого reject-поведения — вопрос владельцу §8(а)).

**Явно не трогается:** M2-оракулы и их порядок (§3.4 №1,2,6,8), H1-кратность в `hardened`
(`:289–296`), `small_class_matches` в `hardened` (`:417–420`), residual M2 limit
(`:180–186`: guards распознают текущее состояние, не прошлый instance — не расширяется),
ledger (`add_live`/`sub_live`, terminal fatal — `segment_header_meta_fields.rs:50–58,72–79`),
sidecar-протокол межпоточного free (§7), SEC-1/M2 сила сохранена/усилена (проверка — §6).

### 3.5. Таблица переходов состояний блока (per-class геометрия)

**Три независимые структуры — как сегодня; единый «used»-бит НЕ вводится** (это прямое
исправление ревью 3, `adr_review3.md:94–96`):

1. **alloc bitmap** (`AllocBitmap`, сегментный, `is_free/mark_free`) — занятость блока substrate'ом;
2. **magazine bitmap** (`MagazineBitmap`, «Owner-only, no atomics», `magazine_bitmap.rs`) —
   резидентность в журнале;
3. **ledger** (`live_count`, `add_live(k)` на carve-партии / `sub_live(accepted)` на flush,
   `alloc_core_small_magazine.rs:671–673`) — exactly-once credit.

Sidecar pending — четвёртая, только для alloc-xthread; **вне прототипа этапа 1, не трогается**.

| Состояние | alloc bitmap | magazine bitmap | ledger | где виден |
|---|---|---|---|---|
| Свободен в бине substrate | free (1) | 0 | учтён в live | free-list бина |
| В журнале (tcache slot / magazine) | alloc (0) | 1 (+ tcache slot) | учтён | tcache/magazine |
| Выдан пользователю | alloc (0) | 0 | учтён | — |
| Pending sidecar (foreign free) | alloc (0) | 0 | учтён (credit у producer'а) | **не меняется в этапе 1** |
| Reclaimed (owner вернул) | free (1) | 0 | вычтен (`sub_live`) | free-list |

| Переход | Операция над структурами | Код |
|---|---|---|
| refill (выдача партии в журнал) | `mark_magazine(n−1)` + заполнение tcache slots | `hot.rs:66–89` (`finish_magazine_refill`), `clear_magazine_on_issue` `hot.rs:35–46` |
| magazine hit | `clear_magazine_on_issue` (clear RMW magazine-бита) | `hot.rs:35–46` |
| free → push в журнал | M2-guard'ы (§3.4) + `mark_magazine` + slot store | `dealloc_own_base.rs:377+` |
| overflow-flush | clear magazine + `mark_free` + `sub_live(accepted)` | `dealloc_own_base.rs:504`, `alloc_core_small_magazine.rs:671–673` |
| remote reclaim (вне этапа 1, не трогается) | `reclaim_sidecar_record`: read payload/bump/кратность, is_free, is_in_magazine, `write_next`, `dec_live` | `alloc_core_small_reclaim.rs:23–72` |
| reset / decommit / pool / recycle | обнуление **всех трёх** структур страницы/сегмента + дескрипторы → `Free` (§3.6) | `dec_live_and_maybe_decommit` (`alloc_core_small_pool_impl.rs:60`), `release_or_pool_empty_segment` (`alloc_core_small_pool_impl.rs:191`) |

Что упрощается при per-class геометрии: **только** определение класса (span lookup §3.2) и
oracle 1.5 (per-span bump вместо сегментного). Всё остальное — без изменений.

### 3.6. Назначение/переназначение страниц

- Страница получает класс при **первом carve в неё** (per-class bump-курсоры вместо сегментного;
  свободные страницы — из free-page списка сегмента, §3.7).
- Span держит класс до полного освобождения всех его блоков: условие возврата в free-page
  список — `alloc_bitmap` бит-в-бит свободен по всем страницам span'а ∧ magazine-биты нулевые ∧
  нет pending sidecar-публикаций (pending — вне этапа 1, поэтому фактически: аллок-бит +
  magazine-бит + live_count span'а == 0).
- **reset/decommit/pool/recycle сегмента**: все страницы `Free`, дескрипторы обнулены.
  Точки интеграции: `dec_live_and_maybe_decommit` → `release_or_pool_empty_segment`
  (`alloc_core_small_pool_impl.rs:60,191`, вызовы `alloc_core_small_magazine.rs:680`,
  `sidecar_drain.rs:231–232`, `alloc_core_small/dealloc.rs:141–142`) — каждая из этих точек
  под фичей обязана чистить дескрипторы (proof §6, мутант stale-class).
- **Primordial** — вне геометрии этапа 1 (legacy-модель, текущие оракулы; `Primordial`-ветка
  `payload_start`, `dealloc_own_base.rs:304–306`). Фиксируется; подтверждение владельцу — §8(в).

### 3.7. Переназначение страницы классу A→B

Допустимо **только в полностью свободном состоянии** (критерий §3.6). Свободные страницы
разных классов не смешиваются: страница несёт **один** класс (дескриптор). Смешанное
использование одной страницы двумя классами (сегодняшняя mixed-class carve, контрпример
`tests/phase13_3_dealloc_layout_class.rs:96–113`) под фичей **невыразимо** — это и есть
устранение класса ошибок D1 (§2).

## 4. Что ускоряется (честная сверка к базе 189–190)

Все статьи ИЗМЕРЕНО по разметке (§1); refill 14.7 — **inclusive**, его подпункты повторно
не добавляются; issue-цепочка после R3 выполняет guards раз на run (`issue_transaction.rs:14–43`)
и **отдельно не выделяется** (нечего ускорять этапом 1 без непересечения — не доказано ⇒
не выделяем).

Разложение базы (larson T=1, Ir/op):

| Статья | Ir/op |
|---|---|
| alloc-hit `hot.rs` (вкл. refill-хвост 14.7, inclusive) | 13.7 + 14.7 = 28.4 |
| free-цепочка `dealloc_own_base.rs` | 32.5 |
| `class_for` (10 alloc + 10 free) | ~20 |
| root-lookup free-пути | ~12 |
| reclaim-скан | ~5 |
| `try_drain_freelist_batch` | ~8.6 |
| overflow-flush | ~8.2 |
| directory/dispatch/прочее (остаток, ОЦЕНКА) | ~74 |
| **Сумма** | **≈188.7–190** ✓ сходится к 189–190 |

Эффект этапа 1 (только free-плечо; ОЦЕНКА ±30%):

- **уходит:** `divides()` 6 → stride-check от span_base ~2–3 ⇒ **−3…−4**;
- **добавляется:** span lookup ~2 + class-equality ~1–2 ⇒ **+3…+4**;
- всё остальное — **без изменений** (`kind_at`, `payload_start`, `is_in_magazine`,
  `bump_of`→span-bump той же силы и цены, `is_free`, tcache push, flush, `class_for` остаётся).

**Итог: собственный код ≈ 189–190 − (−2…+1), т.е. этап 1 сам по себе НЕ даёт существенного
выигрыша** — в лучшем случае −2…−4 Ir/op чистыми на free-плечо (если stride-замена
перевешивает lookup; может выйти и ±0). Паритет с mimalloc **не ожидается — прямо**.
Ценность этапа 1 — не в Ir: (1) устранение mixed-class как класса ошибок (чинит D1-контрпример,
§2/§3.7); (2) усиление reject aligned wrong-class (§3.4 №5); (3) подготовка почвы (авторитетный
page-class — необходимая предпосылка любых будущих сокращений, но их выигрыш вне этапа 1).
**Честно:** если замер покажет выигрыш < 10 Ir/op — по критерию остановки раунда 3
прототип закрывается как «нецелесообразный»; это **ПРИЕМЛЕМЫЙ исход** (измерение ради данных
для решения), а не неудача.

## 5. План прототипа (измерительного)

- **Фича:** `small-per-class-pages` — opt-in; `production` её **не включает**; production-поведение
  не меняется. Исключение наблюдаемого reject-поведения (§3.4 №5) — только под фичей (§8(а)).
- **Модули (новые/под фичей):**
  - per-class bump-курсоры + free-page список в `src/alloc_core/small/alloc_core_small/`
    (carve_block/try_carve_batch → per-class вариант, IssueRun-семантика сохраняется,
    prepare-до-мутаций сохраняется, `add_live(k/n)` сохраняется);
  - дескрипторы страниц в `SegmentLayout`/`descriptors.rs` под фичей (формат §3.1; под фичей
    PageMap перестаёт быть diagnostic-only **в пределах фичи**);
  - free-guard ветка в `dealloc_own_base.rs` под фичей (span lookup + stride + class-equality,
    §3.4);
  - bootstrap init (инициализация дескрипторов, compile-time assert §3.1);
  - reset/decommit/pool пути: `dec_live_and_maybe_decommit`, `release_or_pool_empty_segment`,
    `alloc_core_small_pool/decomp_hooks.rs` — очистка дескрипторов (§3.6).
- **Переиспользуется без изменений:** tcache, M2-оракулы и их порядок, H1/hardened-набор,
  битмапы (`SegmentBitmap` locate со сдвигом, `segment_bitmap.rs:109–117`; ортогональная
  резидентность `magazine_bitmap.rs`), ledger (`segment_header_meta_fields.rs:50–79`), sidecar
  (`small_sidecar.rs`, §7), IssueRun, `class_for`.
- **Обязательные тесты ДО замера** (замер валиден только при их зелёности/ожидаемой красноте):
  весь существующий набор под фичей: `heap_core_tcache_m2` (вкл. `t_false_positive_handled`),
  `regression_magazine_oracles`, `regression_hardened_interior_ptr`, `r11_ph3a_issue_transaction_batch`,
  `r11_ph3a_issue_transaction_scalar`, `regression_dealloc_metadata_region_guard` (существует,
  проверено), no_stale_doc_references / unsafe_allow_placement / src_file_size_cap.
  НОВЫЕ минимальные:
  - `span_stride_free_rejected` — interior от span base отвергается;
  - `span_wrong_class_free_rejected` — aligned wrong-class теперь отвергается; **это изменение
    поведения относительно базы — задокументировано здесь (§3.4 №5) и в §8(а)**;
  - `span_class_lifecycle` — назначение класса странице, полное освобождение span'а,
    переиспользование под другой класс;
  - `span_decommit_reset` — дескрипторы обнуляются на reset/decommit/pool;
  - `phase13_3_dealloc_layout_class` под фичей: mixed-class страница **невыразима** — тест
    обязан проверять **отрицание**: после churn-сценария не существует ни одной страницы,
    чей дескриптор contradicts фактическому использованию (дескриптор == фактический класс
    всех блоков страницы); skip **не допускается** — инвариант выражаем, поэтому проверяем
    именно его (не вакуозная mixed-class предпосылка `:96–113`, а её отрицание).
- **Метрика:** `bash D:/dev/rust/sefer-alloc/.rush/wsl/prof_win.sh r6-u26 <метка> larson|mstress
  sefer|mimalloc` (T=1, детерминированный callgrind) + iai-ячейки `benches/perf_gate_iai.rs`:
  `small_churn_16b` (`:166`), `dealloc_free_only_16b_n8/n9/n16` (`:875–1008`),
  `alloc_magazine_hit_only_16b` (`:495`), cold/recycle `256x{16,64}b`.
- **Критерий ПРОДОЛЖЕНИЯ (все):** все обязательные тесты зелёные/ожидаемо-красные **И**
  larson T=1 ≤ **318** (323 − 5) **И** iai hot ≤ 1.02.
- **Критерий ОСТАНОВКИ (любой):** любой корректный гейт красный не по ожиданию; **или**
  larson T=1 хуже 323 (per-class геометрия дороже базы).
- **Исход «выигрыш мал»** (318 < результат < 323... точнее: продолжение-гейт не достигнут,
  корректность зелёная) — валидное закрытие этапа с данными для решения; по честной оценке §4
  это **наиболее вероятный** исход.

## 6. Proof-набор этапа 1

Правило: каждая клетка = код + оракул + красный мутант + имя теста (существующий или НОВЫЙ).
Все мутанты — expected-red по образцу пин-формата `ci.yml:3048–3155`.

| Клетка | Код | Оракул | Красный мутант / тест |
|---|---|---|---|
| Miri SB позитив **под фичей** `small-per-class-pages` | `--features "production internals bench-internals small-per-class-pages"` | существующий набор зелёный | — (зелёная клетка; прогон буквально с фичей — исправление ревью 3:119) |
| Miri TB позитив **под фичей** | тот же feature-набор, `-Zmiri-tree-borrows` | зелёный | — |
| Miri SB/TB: alloc-global internals bench-internals **с фичей** | `--features "alloc-global internals bench-internals small-per-class-pages"` | зелёный | — |
| Мутант: stride-проверка пропущена | free-guard этапа 1 (`dealloc_own_base.rs` под фичей) | interior-указатель от span base обязан reject/no-op | по образцу `tests/regression_hardened_interior_ptr.rs` — НОВЫЙ `span_stride_free_rejected` (expected-red при мутанте) |
| Мутант: class-equality пропущена | free-guard этапа 1 | aligned wrong-class обязан reject | по образцу `tests/phase13_3_dealloc_layout_class.rs` (адаптация: инвариант отрицания §5) — НОВЫЙ `span_wrong_class_free_rejected` |
| Мутант: span reset не чистит дескрипторы | reset/decommit/pool путь (§3.6) | после reset дескриптор страницы = Free; stale-class ловится lifecycle-инвариантом | НОВЫЙ `span_decommit_reset` (+ `span_class_lifecycle` для переиспользования) |
| C6 OOM-инъекция на carve-границах страницы | per-class carve/IssueRun под фичей | OOM на границе ⇒ null/частичная выдача, credit не расходится, ретраибельно | по образцу `tests/r11_ph3a_issue_transaction_batch.rs` (мутант add_live красный) и `..._scalar.rs` — НОВЫЙ `span_carve_oom_injection` |
| Мутант: add_live ≠ выданным | carve-партия под фичей | ledger under/overflow — terminal fatal (`segment_header_meta_fields.rs:50–58,72–79`) | существующий мутационный шаблон `r11_ph3a_issue_transaction_batch` красный |
| 4 expected-red path-b сигнатуры | `ci.yml:3048–3155` | **не меняются**: этап 1 не переносит места body-write `Node::write_next` (`reclaim_sidecar_record`/`flush_run`/`dealloc_small`), P1-box не расширяется | сигнатуры остаются красными в прежних местах |
| Windows MT A/B | larson/mstress T=1..8 | — | **не требуется на этапе измерения** (T=1 callgrind); **обязателен до любого production-продвижения** |

## 7. Отклонено/открыто (этап 2 и прочее) — фиксация ошибок ревью 3 без «спасения»

Межпоточный free остаётся **ровно как сегодня**: sidecar-bitmap (`sidecar_bitmap.rs:101–126`),
credit, терминальная публикация, strict trim (`sidecar_drain.rs:431–487`). Любые изменения
протокола — **отдельный ADR после закрытия BLOCKER-вопросов контракта**
(`REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md`). Зафиксированные ошибки:

1. **Pending-бит ≠ persistent lifetime admission.** Утверждение R3 «два frees одного instance ⇒
   ровно один prev=0» и «free после reclaim ловится pop-абортом» — **неверны**: (а)
   `BitmapScan::next_cut` очищает pending-слово **до** обработки записей (`bitmap_scan.rs:49–85`) ⇒
   валидна последовательность publish₁ → owner swap(0) → publish₂, оба RMW получают prev=0;
   (б) `publish` не проверяет результат `fetch_or` (`sidecar_bitmap.rs:118–126`); (в)
   `BitmapCut::pop` не читает used-бит/ledger (`bitmap_cut.rs:21–35`); (г) free после reclaim
   ловится owner-side state-check (блок уже свободен ⇒ `return false`, без второго
   credit-decrement), `alloc_core_small_reclaim.rs:48–54,67–70`, а не pop-абортом. Pending
   объединяет публикации **между cut'ами**, не более. Оракул `loom_page_incarnation_aba`
   отозван (ограничение ABA честно сформулировано в контракте `:53`).
2. **Форма B (P/R-биты) нарушает inclusive strict trim при остановленном соседнем producer'е.**
   Контрпример: блок A в слове завершил P и R до входа trim; producer блока B того же слова
   установил P и остановился до R; форма B предписывает пропустить слово целиком ⇒ A не
   reclaim-ится. Это нарушает контракт §1/§7: завершённая **до входа** публикация обязана быть
   обработана без ожидания другого producer'а (`REMOTE_FREE_TERMINAL_PUBLICATION_CONTRACT.md:7–9,65–67`).
   Дополнительно: размещение P/R и атомарность их очистки не заданы; заявка «B строже базы»
   необоснована (форма A к reservation на reject-пути вообще не обращается).
3. **Owner-only dirty-слово не обнаруживает новые remote frees.** Dirty-слово обновляет только
   владелец при вычитке cut'ов; producer его не пишет ⇒ новая публикация в ранее пустую страницу
   невидима до полного обхода. Заявленное «discovery O(затронутых страниц)» из конструкции
   **не следует**: discovery либо полный (контракт `:41,112` — hints лишь подсказки), либо требует
   producer-side записи (новый producer write — изменение P1-box, отдельное решение).
4. **Per-page терминальное слово** (перенос `remote_head` в заголовок страницы) — против
   требования независимого lifetime sidecar (контракт §3/§4); не рассматривается.

Вывод: none из конструкций этапа 2 не «чинится» в этом ADR. Вопросы протокола — после BLOCKER.

## 8. Вопросы владельцу (только меняющие форму гарантий)

- **(а) Усиление invalid-use:** под фичей aligned wrong-class free **отвергается** — сегодня
  проходит (`dealloc_own_base.rs:413–416`). Это не ослабление, но **изменение наблюдаемого
  reject-поведения** относительно базы (в пределах opt-in фичи). Подтвердить допустимость.
- **(б) Охват medium-классов:** span'ы >1 MiB (под `medium-classes`) в этапе 1 **не покрываются**
  (§3.3); подтвердить отложенный охват или потребовать включения (тогда — отдельная итерация
  спецификации: 12-битное поле и free-page политика не покрывают 448-страничные span'ы).
- **(в) Primordial вне per-class геометрии** (legacy-модель, текущие оракулы, §3.6) — подтвердить.

Вопросы, не меняющие форму гарантий (числа, порядок работ), не выносятся.

## 9. ИЗМЕРЕНО vs ОЦЕНКА + итог раунда 4

**ИЗМЕРЕНО** (принято по отчёту B4-hot; независимое воспроизведение не выполнялось —
документальный режим): larson T=1 = 323; mstress T=1 = 499 (база R3, без 317); mimalloc 210.5;
харнесс ≈134; пофайловая разметка §1 (hot.rs 13.7, dealloc_own_base 32.5, class_for ~20,
root ~12, reclaim ~5, drain ~8.6, refill 14.7 inclusive, flush 8.2).

**ОЦЕНКА (±30%, до прототипа):** цены §3.4 (span lookup ~2, stride ~2–3, class-equality ~1–2);
итог §4 (−2…−4 Ir/op чистыми, возможно ±0); +3 КиБ метаданных на сегмент и сдвиг payload ≤1
страницы (§3.1); кэш-поведение дескрипторного load.

**Не утверждается:** эквивалентность/неослабление любой строки без пройденного proof-набора §6;
паритет; «единственный путь»; прогноз 125–138 (отозван, §0).

**Итог раунда 4 по findings ревью 3** (каждый пункт: где устранён):

- **P1 pending ≠ lifetime admission** (строки 40–55) → устранён в §7(1): конструкция отозвана
  вместе с этапом 2, неверные обещания §3.4-Р3 сняты, точный код-разбор приведён.
- **P1 форма B / strict trim** (строки 57–71) → устранён в §7(2): контрпример зафиксирован,
  конструкция не «спасается».
- **P1 owner-only dirty** (строки 73–81) → устранён в §7(3): фиксация, альтернативы названы
  и не предпринимаются.
- **P1 этап 1 не исполняемая спецификация** (строки 83–96) → устранён в §3: дескриптор (§3.1),
  O(1)-поиск (§3.2), формула origin/stride с однозначным отсчётом и multi-page span (§3.3),
  таблица free-проверок с ценами (§3.4), таблица переходов без единого used-бита (§3.5),
  жизненный цикл страниц (§3.6–3.7).
- **P2 арифметика бюджета** (строки 98–110) → устранён в §4: сумма сходится к 189–190,
  refill не считается дважды, issue-цепочка не выделяется без непересечения, прогноз
  125–138 и ложные конверсии (32 KiB/2 KiB, 105–165) отозваны.
- **P2 proof-набор** (строки 112–124) → устранён в §6: каждая клетка полная; Miri-позитив
  буквально под фичей; вакуозные оракулы этапа 2 отозваны с ним же.
- **P3 правила делегациям** (строки 126–132) → процессное обязательство: полный дословный блок
  правил среды в каждое задание под-агента.
