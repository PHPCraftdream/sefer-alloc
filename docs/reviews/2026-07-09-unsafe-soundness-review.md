# Ревью осторожности unsafe-решений (unsafe soundness review)

**Дата:** 2026-07-09. **Исполнитель:** независимый ревьюер №7 (угол: не «работает ли
код», а «насколько добросовестно обоснованы места, где обойдены гарантии
компилятора»). **Режим:** только чтение и трассировка; исполняемых PoC нет
(ограничение задания) — каждая находка снабжена конкретным сценарием отказа и
указанием, какой слой верификации его (не) видит.

**Вердикт (кратко):** дисциплина unsafe в проекте заметно выше средней по
индустрии — SAFETY-плотность фактически 1:1, verification-стек реален, честные
«здесь предел» комментарии встречаются чаще, чем приукрашивающие. Но найден один
High-класс системный пробел: inline-поле `HeapCore::thread_free` принимает
кросс-поточные атомарные ЗАПИСИ через exposed-provenance, в то время как владелец
держит protected `&mut HeapCore` над теми же байтами — ровно тот класс конфликта,
который проект сам объявил UB и вылечил в W3 для диагностических счётчиков
(причём там речь шла лишь о foreign-ЧТЕНИИ). Обоснование фикса #142 покрывает
только remote-vs-remote; remote-vs-owner-окно не аргументировано нигде и не
покрыто ни одним верификационным слоем. Плюс кластер устаревших/аспирационных
SAFETY-формулировок, переживших рефакторинги (#204, 12.5, A1).

---

## Scope

Все модули с `#![allow(unsafe_code)]` (проверено `grep -rln 'allow(unsafe_code)'
src/ crates/`; список совпал с инвентарём в `src/lib.rs:95-162`, включая
`crates/malloc-bench/src/lib.rs`, который в инвентаре есть, а в постановке задачи
не был назван):

- `src/alloc_core/{node,os,numa}.rs`
- `src/concurrent/hand.rs`
- `src/global/{sefer_alloc,fallback,tls_heap}.rs`
- `src/registry/{bootstrap,heap_registry,heap_slot}.rs`
- `crates/vmem/src/lib.rs`, `crates/numa/src/lib.rs`, `crates/malloc-bench/src/lib.rs`
- плюс верификационная обвязка: `src/kani_proofs.rs`, `.github/workflows/{ci,kani}.yml`,
  loom-набор (11 файлов `tests/loom_*.rs` — заявление «11 harness» сошлось).

## Методология

1. Для каждого seam-модуля SAFETY-комментарии не принимались на веру:
   нетривиальные обязательства трассировались до вызывающей стороны
   (`stamp_segment_owner` → `atomic_ptr_ref` → `push_large_deferred_free`;
   `claim` → `finish_bind` → `(*heap).alloc()`; `reclaim_large_segment` →
   `os::release_segment`; и т.д.).
2. Каждое `unsafe impl Send/Sync` проверено против фактического состава типа и
   против того, ЧТО доказывает его комментарий.
3. Верификационное покрытие сверено с CI: какие тесты бегут под miri-strict /
   miri-plain / loom / TSan / Kani, и какие интерливинги эти прогоны
   *структурно не могут* увидеть.
4. Отдельно искались: (a) SAFETY-заявления, не гарантируемые окружающим кодом;
   (b) unsafe-операции без верификационного покрытия; (c) «осторожная проза»
   поверх рискованного сокращения; (d) щели в Send/Sync-баундах; (e) unsafe без
   SAFETY вообще.

По пункту (e): **не найдено** — в seam-модулях каждый unsafe-БЛОК несёт SAFETY
(численные расхождения grep-счётчиков объясняются `unsafe fn`-сигнатурами
трейтов и extern-деклараций, а не голыми блоками; выборочно проверено).

---

## Находки

### H1 — inline `thread_free`: remote wildcard-CAS против protected `&mut HeapCore`; обоснование #142 не покрывает окно владельца — по собственному стандарту проекта (W3) это UB-класс

**Severity: High** (model-level UB на production-пути `alloc-xthread`;
верификационный стек структурно слеп к нужному интерливингу; противоречит
собственной планке проекта, зафиксированной в W3).

**Факты (проверено чтением):**

- `HeapCore::thread_free` — inline-поле структуры: `src/registry/heap_core.rs:213`.
- Владелец на КАЖДОМ alloc/dealloc материализует `&mut HeapCore` над всей
  структурой, включая байты `thread_free`: `src/global/sefer_alloc.rs:355,376,387,403`
  (`(*heap).alloc(...)`, метод `pub fn alloc(&mut self, ...)` —
  `heap_core.rs:565,802`), а для fallback-кучи — `src/global/fallback.rs:200`
  (`f(unsafe { &mut *heap })`). Как аргумент функции этот `&mut` **protected**
  на время вызова.
- Remote-поток при кросс-поточном free Large-сегмента CAS-ит ровно эти байты:
  `heap_core.rs:1372` → `Self::push_large_deferred_free(owner_tf, base)` →
  `Node::atomic_ptr_ref` (`src/alloc_core/node.rs:446-472`) →
  `with_exposed_provenance_mut` → `compare_exchange`
  (`src/alloc_core/deferred_large/push.rs:110-142`).
- Expose-сайт: `heap_core.rs:1607-1608` — `addr_of!(self.thread_free)` +
  `expose_provenance()`, выполняется **один раз на сегмент** (ветка
  `cur_head.is_null()`), изнутри текущего кадра `&mut self`.
- Проект сам сформулировал стандарт: `heap_core.rs:95-96` — «…a struct the
  OWNING thread concurrently holds a protected `&mut` into (the
  `alloc(&mut self, …)` protector) — a foreign-**read** of a protected `Unique`,
  UB under Stacked Borrows» — и в W3 ради этого счётчики были ВЫНЕСЕНЫ из
  `HeapCore` в `HeapSlot` (`heap_slot.rs:155-178`, `heap_registry.rs:503-541`,
  `alloc_core.rs:218-227`). `thread_free` остался inline (по уважительной
  M5-причине — `heap_core.rs:126-138`), но принимает foreign-**записи** — более
  сильный конфликт, чем тот, что чинили в W3.

**Почему обоснование #142 не закрывает дыру.** SAFETY-текст `atomic_ptr_ref`
(`node.rs:449-471`) доказывает одно: reconstruct через wildcard не делает
remote-доступ «ребёнком» стампованного reference-tag, поэтому remote A не
инвалидирует доступ remote B. Про сосуществование wildcard-ЗАПИСИ с живым
protected `&mut HeapCore` владельца — ни слова. Механика под Stacked Borrows
(как его гейтит miri в этом репо):

1. Fn-entry retag `&mut HeapCore` действует как write-подобный доступ по всему
   диапазону структуры (UnsafeCell-исключение применяется к shared-ретагам, не
   к `&mut`) и **выталкивает из стека байтов `thread_free` ранее exposed-тег**
   (он был потомком прошлого кадра `&mut`).
2. Далее два варианта, оба плохие:
   - remote CAS приходит, когда в стеке нет валидного exposed-тега (владелец с
     тех пор не стамповал новый сегмент — например, гоняет small-allocs через
     OPT-C fast path `heap_core.rs:1539-1551`, который **не** re-expose-ит) →
     «no exposed tag grants this access» → UB;
   - remote CAS находит старый exposed-тег НИЖЕ текущего protected Unique →
     запись выталкивает protected item → protector violation → UB.

**Конкретный сценарий отказа:** поток B в `(*heapB).alloc(layout)` (protected
`&mut`, small-alloc, стамп по кэш-хиту — re-expose не происходит); поток A
одновременно дропает Vec-буфер >SMALL_MAX, принадлежащий сегменту B →
`dealloc_routing` → wildcard-CAS в `heapB.thread_free`. Это **типовой** режим
async-runtime (ровно мотивация A1, `heap_core.rs:154-161`), а не экзотика.

**Почему это не ловится существующей верификацией (проверено по ci.yml):**

- `miri` (strict, `ci.yml:200`) — exposed-provenance пути исключены by design
  (задокументировано в `registry/bootstrap.rs:126-136` и `ci.yml:260-275`);
- `miri-plain` гоняет ОДИН xthread-тест
  (`tests/regression_xthread_large_free_no_leak.rs`), и он **фазово
  сериализован**: владелец аллоцирует → remote-поток освобождает (владелец
  пассивен) → владелец аллоцирует снова. Каждый Large-alloc владельца заново
  expose-ит свежий сегмент, поэтому «между кадрами» тег всегда валиден — тест
  зелёный именно благодаря каденции, а не благодаря общей корректности;
- loom проверяет модель атомиков, aliasing-модель не видит; TSan видит только
  data race уровня C++ (атомики легальны) — aliasing-модель не видит.

Т.е. класс расписаний «remote push пересекается с кадром владельца» не покрыт
**ничем**. На сегодняшнем codegen это не мискомпилируется (доступы атомарные),
но (i) `&mut self` → LLVM `noalias` — формально нарушенное предположение, (ii)
проект сам гейтится на miri/SB и чинил более слабые случаи (W3, #142), т.е. по
внутренней планке это дефект, а не педантизм.

**Рекомендация (в порядке предпочтения):**

1. Повторить W3-ход: вынести TFS/deferred-head из `HeapCore` в `HeapSlot`
   (адрес так же стабилен и `'static`, слот `Sync`, аллокаций не требуется —
   M5 сохраняется; для fallback — отдельный `static FALLBACK_TFS: AtomicPtr<u8>`
   рядом с `FALLBACK`, вне структуры). Тогда remote-записи вообще не попадают в
   диапазон чьего-либо `&mut`.
2. Минимум (если №1 откладывается): (а) дописать в `atomic_ptr_ref` и в
   `stamp_segment_owner` ЧЕСТНОЕ ограничение аргумента («покрыт только
   remote-vs-remote; remote-vs-owner-`&mut` окно открыто, риск SB-модельный»);
   (б) добавить в miri-plain MT-тест с реальным ПЕРЕСЕЧЕНИЕМ (remote-цикл
   push-ей параллельно owner-циклу small-allocs, повышенный
   `-Zmiri-preemption-rate`) — даже если он окажется зелёным, класс расписаний
   станет видимым и зафиксированным; красный — подтвердит H1 исполняемо.

---

### M1 — SAFETY-контракт `Node::atomic_ptr_ref` перечисляет удалённый тип и не перечисляет реальный второй случай

**Severity: Medium.** `src/alloc_core/node.rs:420-443`: контракт «pointee — это
ЛИБО (a) HeapCore в `'static` slot-array реестра, ЛИБО (b) leaked
`Box<AtomicPtr<u8>>` типа `Heap`» — но `crate::heap::Heap` /
`ThreadFreeStack` **удалены в task #204** (в `src/` их нет; ссылки битые), а
реальный живой второй случай — fallback-`HeapCore` в `static mut FALLBACK`
(`src/global/fallback.rs:90`), который НЕ лежит в slot-array. Вывод о `'static`
для fallback случайно верен (never dropped), но перечисление, на котором
строится доказательство, устарело с обеих сторон.

**Сценарий отказа:** редактор «подчищает мёртвый случай (b)», верит оставшемуся
«все pointee — слоты реестра» и вводит, например, инвалидацию по
slot-generation — fallback-pointee нарушит новое предположение молча.

**Рекомендация:** переписать контракт на «(реестровый слот | fallback-static)»,
убрать ссылки на `Heap`; это же место — единственный полный список того, КУДА
могут указывать стампы, его точность load-bearing.

### M2 — `'static`-обоснования `atomic_u8/u32/u64_at` опираются на ложное общее утверждение «segment remains mapped for the process lifetime»

**Severity: Medium.** `node.rs:331-335, 355-358, 383-392, 401-407` — все три
аксессора обосновывают `'static` фразами «freed only at `AllocCore::drop`,
after all cross-thread frees/adoption have quiesced». Фактически:

- Large-сегменты освобождаются **в середине жизни процесса**:
  `AllocCore::reclaim_large_segment` → `os::release_segment`
  (`src/alloc_core/alloc_core.rs:3580,3636`; плюс десяток release-сайтов
  крупного кэша: 3444, 3731, 3833, 4059, 4097…);
- клаузула «after cross-thread frees have quiesced» — аргумент эпохи удалённого
  `Heap`-фейса; сегодня её никто не обеспечивает и (к счастью) не требует:
  реестровые `HeapCore` не дропаются вовсе;
- реальная безопасность remote-доступов держится на тонких пер-путёвых
  liveness-аргументах, живущих В ДРУГИХ файлах (double-push guard в
  `deferred_large/push.rs:35-74`; честное «(a)/(b) неразличимы, dangling free →
  fault» в `heap_core.rs:1302-1323`) — а не на процитированной «карте мира».

**Сценарий отказа:** будущая decommit-when-empty политика (для которой
abandon/adopt-субстрат прямо «retained, loom-proven» —
`heap_registry.rs:247-257`) начинает освобождать small-сегменты mid-process;
автор доверяет blanket-`'static` из node.rs; remote, держащий
`&'static AtomicU64` на header сегмента, читает unmapped-страницу → UAF/SEGV.
REACTIVATION-HAZARD-нота (`heap_registry.rs:259-289`) уже показывает, что этот
класс «спящих» несостыковок в субстрате реален.

**Рекомендация:** переформулировать контракты аксессоров через фактический
инвариант («валидно, пока сегмент зарегистрирован в таблице владеющей кучи;
вызывающий обязан предъявить liveness-аргумент — см. push.rs/dealloc_routing»)
и связать их с REACTIVATION-HAZARD-нотами перекрёстными ссылками.

### M3 — SAFETY alloc-фейса overclaim-ит M2: «dealloc — safe no-op на foreign/dangling указателе»

**Severity: Medium.** `src/global/sefer_alloc.rs:371-375` заявляет: «`dealloc`
is a safe no-op on a foreign/dangling pointer (M2 defence-in-depth)». Сам
routing-код честнее: `heap_core.rs:1302-1316` — случай «(b) сегмент уже
released/unmapped» **неотличим** от живого чужого, чтение `magic_at(base)`
по dangling-указателю в released-сегмент словит fault; «double-free of a
released, unmapped segment is fundamentally UB». Т.е. читатель SAFETY-коммента
alloc-фейса получает более сильную гарантию, чем существует.

**Сценарий отказа:** потребитель, прочитав SAFETY на фейсе, решает, что
double-free больших буферов «безопасно поглощается», и не чинит его у себя →
в проде SEGV на чтении header (лучший случай) или residual-коррупция #138
(худший, `heap_core.rs:1360-1371`).

**Рекомендация:** квалифицировать фразу ровно так, как это делает
dealloc_routing: гарантия для live/mapped-случая; dangling-в-released-сегмент —
UB по контракту GlobalAlloc.

### M4 — Kani-покрытие (добавлено сегодня) переоценено собственным module-doc: harness-ы не проверяют ни одного контрактного обязательства

**Severity: Medium** (риск «верификационного театра» в отчётности).
`src/kani_proofs.rs:1-6`: «verify the round-trip correctness of the unsafe
pointer primitives in Node **and the publication/eviction protocol of
AtomicSlot**». Фактически (строки 141-152) hand-harness-ы проверяют
`vacant().generation() == 0` и no-op `drop_value()` на пустом слоте — к
publication/eviction-протоколу это отношения не имеет (что нижний комментарий
131-136 честно объясняет, но шапка — нет). Node-harness-ы (15-128) доказывают
round-trip на локально-валидных буферах — свойства, которые и так гарантирует
семантика `read`/`write`; ни одно из настоящих обязательств (bounds вызывающих,
эксклюзивность, `'static`-lifetime, expose-каденция из H1) Kani не моделирует —
и не может для конкурентной части (pthread_key_create).

**Сценарий отказа:** в CHANGELOG/README попадает «node.rs верифицирован Kani
(9 harness)»; следующий ревьюер снижает бдительность к ВЫЗЫВАЮЩИМ Node — а
только там и живёт весь риск этого модуля.

**Рекомендация:** (а) поправить шапку kani_proofs.rs («smoke round-trip;
caller-контракты НЕ моделируются»); (б) направить Kani туда, где он может
доказывать настоящие инварианты без конкурентности: pack/unpack
(`pack_abandoned_head`/`unpack_abandoned_head`, `pack_entry_hardened`),
`Layout`-offset-арифметика segment_header, `TaggedPtr` — это дешёвые и
содержательные bounded-доказательства.

### M5 — vmem: `recommit` игнорирует отказ `VirtualAlloc(MEM_COMMIT)` — на живом production-пути M6 это крэш вместо обещанного null-on-OOM

**Severity: Medium** (availability, не memory-safety: детерминированный AV на
Windows при исчерпании commit charge). `crates/vmem/src/lib.rs:386-399` —
возвращаемое значение `VirtualAlloc(addr, len, MEM_COMMIT, ...)` не
проверяется; recommit-путь ЖИВОЙ (`alloc_core.rs:3134, 3217` — M6-цикл
decommit→recommit под `alloc-decommit`, входит в `production`). При неудаче
commit последующая запись блока идёт в MEM_RESERVE-only страницу → access
violation → процесс падает, вопреки задокументированному «returns null on true
OOM, never panics» (`sefer_alloc.rs:44-58`). Попутно: `os.rs:226,236`
утверждает «`aligned_vmem::decommit`/`recommit` validates the range» — на деле
vmem валидирует только выравнивание и `start<end` (`vmem lib.rs:296,316`),
принадлежность диапазона — контракт вызывающего. И `#[allow(dead_code)]` на
`os::decommit_pages`/`recommit_pages` (`os.rs:221,232`) устарел — они
вызываются.

**Рекомендация:** `recommit` → `-> bool`, протащить отказ до alloc-пути
(вернуть null); минимум — задокументировать крэш-режим в SAFETY обоих слоёв и
убрать «validates the range».

### M6 — `unsafe impl Send for HeapSlot` доказывает не то утверждение

**Severity: Medium** (сейчас латентно; неверное доказательство на
компилятор-переопределяющем impl'е). `src/registry/heap_slot.rs:236-241`:
SAFETY-текст обосновывает отправку `&HeapSlot` между потоками — но это работа
`Sync` (impl которого выше и корректен). `Send` же разрешает переместить
`HeapSlot` **по значению** — вместе с возможно-живым `HeapCore` (сырые
указатели на сегменты, `AllocCore`) — случай, для которого никто доказательства
не писал; при этом `HeapSlot::new_uninit()` (`heap_slot.rs:192-205`) делает
by-value слоты конструируемыми внутри крейта.

**Сценарий отказа:** будущий код строит теневой `Vec<HeapSlot>` (например, для
snapshot-диагностики) и отправляет его в worker — компилятор молча согласен
из-за этого impl'а, хотя перенос LIVE-слота обходит всю claim-CAS-дисциплину,
на которой держатся соседние доказательства.

**Рекомендация:** либо доказать фактическое утверждение (перенос FREE/uninit
слота тривиален; перенос LIVE — сформулировать, почему `HeapCore` не имеет
thread-affinity), либо удалить `Send` (для `'static`-массива достаточно `Sync`).

### M7 — системное: compiler-enforced конфайнмент — это конфайнмент СИНТАКСИСА unsafe, а не границы соундности; safe-мембраны выносят UB-рычаги в весь крейт

**Severity: Medium** (архитектурная честность заявления «safe-by-construction»).
`src/lib.rs:117-166` продаёт «a stray `unsafe` outside these named modules is a
hard compile error» — верно, но легко перечитывается как «баг вне seam-ов не
может дать UB». Фактически seam-ы экспортируют **safe** `pub(crate)`-функции,
нарушение прозаических контрактов которых из безопасного кода — UB:
`Node::write_usize/write_struct/offset/...` (node.rs — весь файл построен как
«this fn is safe; the contract is the caller's invariant»),
`os::release_segment` (`os.rs:191-200` — double-release из safe-кода),
`os::decommit_pages`; плюс `HeapSlot`-поля `pub` (`heap_slot.rs:77-113`) —
safe-код может CAS-нуть `state` LIVE→FREE и сломать single-writer-инвариант,
на который ссылаются все соседние SAFETY. Итого trusted computing base для
соундности = seam-ы + **каждый вызывающий их мембран** (alloc_core.rs ~4к
строк, heap_core.rs ~1.7к, segment_header.rs ~1.2к safe-кода), а не 14 файлов
из инвентаря.

Это осознанный трейд-офф (мембранный паттерн концентрирует аудит unsafe-блоков)
и он в проекте работает — но нигде не проговорён предел.

**Рекомендация:** добавить в lib.rs-инвентарь один честный абзац: «конфайнмент
enforce-ит локализацию unsafe-синтаксиса; soundness-граница = seam-ы плюс все
вызывающие их safe-мембраны (список мембранных fn: …)». Сузить `pub`-поля
`HeapSlot` до `pub(crate)` + аксессоры, где тесты позволяют.

---

### L1 — избыточные `unsafe impl`, пиннящие auto-traits

**Severity: Low.** `os.rs:176` `unsafe impl Send for Segment` — `Segment`
оборачивает единственное поле `vmem::Reservation`, которое УЖЕ `Send`
(`vmem lib.rs:237`), т.е. auto-impl достаточно (комментарий «we simply forward
it» сам это признаёт). `bootstrap.rs:286` `unsafe impl Sync for Registry` —
все поля `Sync` (атомики + `[HeapSlot; N]` с собственным Sync-impl) → auto.
Оба impl'а не расширяют ничего сегодня, но **заморозят** Send/Sync, если
будущий редактор добавит !Send/!Sync-поле (например, `Cell<..>`-диагностику в
`Registry`) — компилятор промолчит там, где auto-impl честно бы отвалился.

**Рекомендация:** заменить на compile-time assert (`const _: () = { fn
assert_sync<T: Sync>() {} let _ = assert_sync::<Registry>; };`) — та же
документирующая сила без unsafe-переопределения.

### L2 — устаревшие «Phase 8 is single-threaded» в exclusivity-доказательствах node.rs

**Severity: Low.** `node.rs:76-84 (write_next), 129-135 (zero), 160-167
(write_struct)` — эксклюзивность обосновывается «Phase 8 is single-threaded»,
что в production-конфигурации давно неверно. Реальный (и валидный!) аргумент —
owner-only дисциплина плюс правило «remote free не трогает тело блока»
(variant-2 ring, `heap_core.rs:1376-1381`). Сценарий: редактор добавляет
экспериментальный remote-путь, пишущий в тело свободного блока; комментарии
node.rs не сигналят о конфликте, т.к. их посылка и так была ложной и все
привыкли её игнорировать. **Рекомендация:** переформулировать через
single-writer-инвариант со ссылкой на «block bytes untouched».

### L3 — `tls_heap::current()` возвращает указатель с условным (недокументированным) locking-обязательством

**Severity: Low** (мёртвый код, но публичный и «канонический» по собственному
doc-у). `tls_heap.rs:251-278`: на TORN/Err-ветках возвращается
fallback-указатель, mutable-доступ к которому корректен ТОЛЬКО под спинлоком
`fallback::with_heap` (`CurrentHeap::Fallback`-doc, `tls_heap.rs:292-296`,
говорит это про tagged-вариант; doc `current()` — нет). Будущий direct-API
потребитель сделает `&mut *current()` как для Own-случая → два потока в
teardown-окнах получат алиасящие `&mut` fallback-кучи. **Рекомендация:**
удалить untagged-вариант или задокументировать обязательство + вернуть
`CurrentHeap` и там.

### L4 — спинлок fallback не переживает панику в `f`

**Severity: Low.** `fallback.rs:195-202`: `acquire_lock(); f(...);
release_lock();` без RAII — паника внутри `f` навсегда оставляет `LOCK=true`,
все последующие pre-TLS/teardown-аллокации крутятся вечно (deadlock вместо
abort). Смягчение: no-panic — инвариант HeapCore, а паника в GlobalAlloc и так
abort-ит; но `with_heap` дёшево делается panic-устойчивым. **Рекомендация:**
guard-структура с `Drop`-release (ноль стоимости, минус одна посылка).

### L5 — malloc-bench: кросс-инстансный dealloc не назван в SAFETY; инвентарь lib.rs неточен

**Severity: Low** (bench-крейт, не рантайм). `crates/malloc-bench/src/lib.rs`:
`run()` создаёт **по экземпляру** `A` на поток (`make_alloc()` на строке ~441),
а блоки, пересланные по mpsc, освобождает ЧУЖИМ экземпляром — контракт
`GlobalAlloc` («allocated via this allocator») выполняется только для
stateless-фасадов над глобальным состоянием (System/SeferAlloc — целевой
случай), но SAFETY-комментарии (`~448, 469`) этого предположения не называют;
стейтфул-`A` даст UB при зелёных комментариях. Плюс инвентарь
`src/lib.rs:109-111` («confined to alloc_block/free_block/drain_mailbox helpers
only») не упоминает `unsafe impl Send for Block` (`~90`) и unsafe-блоки в
worker-ах/`run`. **Рекомендация:** дописать требование к `A` в doc `run()`;
поправить строку инвентаря.

---

## Отдельно: что проверено и оказалось добротным (чтобы находки не читались как общий приговор)

- **`concurrent/hand.rs`** — образцовый seam: seqlock-валидация g1==g2 с
  корректным анти-ABA (генерация монотонна, насыщение в MAX исключает wrap),
  четырёхчастное deref-доказательство в `read_with`, честный анализ MAX→MAX
  идемпотентного CAS, `drop_value` через `unprotected()` с настоящим
  `&mut`-обоснованием; Send/Sync-баунды `T: Send + Sync` консервативно
  совпадают с crossbeam'овскими (щели через дженерик нет), пост-7b re-audit
  прописан. Kani-ограничение задокументировано честно (131-136).
- **`registry/bootstrap.rs`** — аккуратный трёхфазный указатель-автомат:
  #131-rollback с обоснованием abort-vs-panic (unwinding аллоцирует →
  реентерабельность), #139 miri-зануление с точным SAFETY, #140 разделение
  провенанс-классов (сентинел strict-clean vs exposed-стеки) — редкая по
  качеству документация структурного предела miri-strict.
- **`deferred_large/push.rs`** — double-push guard с loom-найденным (#143)
  контрпримером прямо в комментарии; дисциплина «claim once, не в retry-loop»
  доказана и протестирована (`loom_deferred_large`).
- **`dealloc_routing`** (`heap_core.rs:1268-1348`) — порядок
  «membership-check ДО касания чужой памяти» (#135) и честная граница
  (a)/(b) для released-сегментов. (Именно на фоне этой честности выделяется
  overclaim M3 на фейсе.)
- **`crates/numa`** — sysfs-парсинг без единой аллокации (стековые буферы,
  raw open/read/close) — сознательная M5-осторожность в FFI-крейте;
  Windows-путь `from_raw_parts` с полным пятипунктовым контрактом.
- **W3/#133** — initialised-gate публикация (`heap_registry.rs:794-887`) —
  учебниковый Release/Acquire-publish с counterfactual-регрессией.
- **`fallback.rs`** OOM-rollback (UNINIT ← INITIALIZING) с анти-livelock
  обоснованием, покрыт `loom_fallback_init`.

## Итоговый вердикт

Проект НЕ страдает обычной болезнью «SAFETY-комментарий как заклинание»: в
большинстве мест обоснования настоящие, с трассируемыми инвариантами и
verification-привязкой. Систематическая слабость одна, но серьёзная: **судьба
байтов `HeapCore::thread_free` под пересечением «protected `&mut` владельца ×
exposed-wildcard запись remote'а» не обоснована нигде** — при том, что проект
сам установил (и в W3 оплатил) планку, по которой даже foreign-чтение такого
рода есть UB. Это надо закрыть ДО любых новых надстроек над xthread-путём
(вынос головы в `HeapSlot` — прямое повторение уже сработавшего W3-хода), и
одновременно закрыть кластер стале-комментариев (M1-M3, L2), которые сегодня
дезинформируют ровно тех будущих редакторов, на добросовестность которых
опирается мембранная архитектура (M7).

Приоритет фиксов: **H1 → M2 → M1/M3 (один проход по стале-SAFETY) → M5 → M4 →
M6/M7 → L1-L5.**
