# aligned-vmem — независимый предрелизный аудит R4

Дата: 2026-08-16

Проверенный snapshot: HEAD c0c52d1b217d81e80b85a3a8d876f6ebcda26506

Объект: crates/vmem, пакет aligned-vmem 0.2.0.

## Область и ограничения

Это новый статический проход по исходникам, публичному API, feature/cfg-веткам,
тестовым оракулам, CI и документации. В этой работе не запускались тесты,
сборка, clippy, rustdoc, benchmark или cross-compilation. Исходники, CI и
конфигурация crate не изменялись; добавлен только этот отчёт.

Платформенные выводы, для которых в checkout нет исполняемого runner-а,
помечены как reasoned-from-spec. Полнота проверки ограничена одним
статическим агентским проходом без исполнения всех target/backend-веток.

## Резюме

На обычных Linux/Windows путях текущего RAII-владения не обнаружен новый
очевидный double-release в нормальном сценарии. Но до релиза остаются:

- buildable, но фактически нерабочий MIPS/Linux backend из-за
  архитектурно-зависимых mmap-констант;
- повторная exact huge-page попытка на 32-битном Linux/Android и
  противоречащий ей тестовый oracle;
- задокументированная, но существенная семантическая несовместимость
  eager decommit на Darwin/BSD;
- несколько API-контрактов, которые плохо отражают runtime page size и
  невозможность decommit для huge pages;
- диагностические и test-only race/reentrancy риски;
- непокрытые исполнением 32-битная huge-ветка и успешные huge-page ветки
  с реальными OS permissions/configuration.

Рекомендация: релиз возможен только после явного решения по Darwin/BSD
семантике и target policy, а также после исправления 32-битной
huge-page ветки. Остальные пункты можно выпускать как последующие
улучшения только если они явно занесены в release notes и backlog.

## Findings

### R4-1 — MEDIUM — MIPS/Linux собирается, но reservation всегда падает

Доказательства:

- crates/vmem/src/lib.rs:3059-3082 задаёт MAP_ANON = 0x20 и прямо
  признаёт, что на MIPS нужно 0x0800;
- crates/vmem/src/lib.rs:3145-3151 аналогично задаёт MAP_HUGETLB = 0x40000
  вместо MIPS-значения 0x80000;
- crates/vmem/README.md:201-211 говорит, что MIPS buildable, но
  reserve_aligned на нём фактически всегда fail-closed.

На MIPS вызов без MAP_ANONYMOUS с fd = -1 доходит до EBADF. Это не
unsafety, но опубликованный buildable target выглядит поддержанным и
не даёт диагностического объяснения, почему любая reservation невозможна.
При включённом huge-pages та же проблема затрагивает huge path.

Рекомендация:

1. Если MIPS официально не поддерживается, добавить явный compile_error
   для MIPS и скорректировать target matrix.
2. Если поддержка нужна, завести target_arch-ветки с константами из
   соответствующих headers и добавить хотя бы compile/runtime smoke
   coverage на MIPS.

### R4-2 — MEDIUM — 32-битная huge exact-попытка выполняется дважды

В crates/vmem/src/lib.rs:2707-2713 при huge = true и align = 2 MiB
счётчик увеличивается и выполняется exact MAP_HUGETLB mmap. Если он
отказал, на 32-битном target код в lib.rs:2743-2745 всё ещё вызывает
общий try_reserve_aligned_exact. Он снова вызывает libc_mmap(size, huge)
и снова увеличивает тот же UNIX_EXACT_RESERVE_ATTEMPTS в
lib.rs:2858-2862. После второй неудачи только затем используется
over-reserve fallback.

Последствия:

- лишний syscall и лишняя попытка из scarce hugetlb pool;
- диагностический счётчик равен 2, хотя пользователь сделал одну
  reservation;
- crates/vmem/tests/huge_pages.rs:225-228 требует attempts == 1;
  этот тест противоречит собственному комментарию source о 32-битном
  повторном пути;
- текущие i686 CI-строки в .github/workflows/ci.yml:199-204 делают
  cargo check, а не runtime test, поэтому противоречие не ловится.
  Проверка hits <= 1 в tests/huge_pages.rs:229-232 сама по себе
  почти tautological и не усиливает oracle.

Рекомендация: после одной huge exact-попытки передавать в общий helper
признак already_attempted либо не входить в 32-битный generic exact path.
Отдельно исправить oracle так, чтобы он проверял семантику, а не
архитектурно случайное число внутренних попыток. Для отказа huge
reservation нужно покрытие именно исполняемого 32-битного пути.

### R4-3 — MEDIUM / release decision — eager decommit не является общей
гарантией на Darwin/BSD

Публичная документация в crates/vmem/src/lib.rs:1508-1515 и
1554-1573, а также README:163-186, предупреждает: MADV_DONTNEED на
Darwin и четырёх BSD не гарантирует reclaim или zero-fill, а Unix
recommit_pages_impl в lib.rs:2953-2971 фактически no-op.

Следовательно, после decommit + recommit обычной reservation пользователь
может получить старые данные и прежний RSS. Это не скрытый defect —
ограничение честно задокументировано и уже оформлено как открытый
correctness item, — но оно расходится с интуитивным названием и общей
моделью decommit/recommit API. Код, безопасный на Linux, не получает
ту же гарантию на macOS/BSD.

Рекомендация до публикации:

- либо сузить public contract и supported target matrix до платформ,
  где заявленная семантика действительно выполняется;
- либо выделить платформенную capability/API и не обещать
  zero-fill/reclaim для общего decommit;
- либо отдельно спроектировать remap/MAP_FIXED-исправление с review
  конкуренции и lifetime, не подменяя его простым изменением комментария.

### R4-4 — LOW — huge-page decommit silently succeeds from the caller's
point of view

Для huge reservations текущие docs в lib.rs:1543-1552 и 1978-1986
говорят о несовместимости: Windows VirtualFree(MEM_DECOMMIT) отказывает,
а Linux madvise для MAP_HUGETLB принимает только huge-page granularity.
Публичный decommit возвращает (), отбрасывает результат syscall и не
сообщает, что RSS не уменьшился и старые данные остались.

Это особенно легко пропустить, если caller проверяет только
Reservation::is_huge() после reservation и затем использует единый
decommit path.

Рекомендация: добавить fallible/capability API (например,
try_decommit или can_decommit), либо сделать невозможность операции
частью type/API contract. До major API change минимум нужен отдельный
явный release-note warning и тест/diagnostic hook, который отличает
unsupported huge decommit от обычного no-op.

### R4-5 — LOW — Windows huge fast path может дорого промахиваться, а
счётчики не показывают syscall cost

В lib.rs:2176-2183 для huge request threshold расширен до
GetLargePageMinimum(). При отказе large-page VirtualAlloc код в
lib.rs:2202-2224 сначала повторяет обычную VirtualAlloc, а при
неподходящем адресе освобождает её и переходит к двухвызовному пути
в lib.rs:2244-2249. Собственная документация оценивает worst case как
до двух дополнительных VirtualAlloc и одного VirtualFree.

При этом счётчик WINDOWS_RESERVE_COMMIT_SINGLE_CALLS считается только
по успешному возврату single-call path, а TWO_CALL_PAIRS — по
двухвызовному завершению. Эти counters не показывают уже сделанные
неудачные huge/retry вызовы и потому не подходят для оценки syscall
стоимости или регрессии без дополнительной телеметрии.

Рекомендация: измерить реальные распределения align/privilege/size на
целевых Windows workloads; добавить отдельные retry/failure counters
или не расширять speculative window без доказанной выгоды. Не делать
оптимизацию только по текущему неподтверждённому benchmark claim.

### R4-6 — LOW — from_raw_parts недостаточно явно описывает runtime
page-alignment contract

В lib.rs:1031-1052 и 1088-1148 from_raw_parts проверяет кратность
фиксированному PAGE = 4096, но не требует явно кратность runtime
page_size() для reservation, base и reservation_len. При этом
decommit/madvise paths используют runtime page size; на 16 KiB OS
формально принятые по текущему тексту 4 KiB-значения могут привести к
silent skip/EINVAL, а ошибка munmap при несовместимом внешнем mapping
теряется.

Функция unsafe и уже требует live/exclusive OS reservation, поэтому это
не превращает безопасный вызов в автоматически unsafe. Проблема в том,
что cross-crate adopter получает недостаточно точный контракт для
платформенно-критичных размеров и адресов.

Рекомендация: явно потребовать OS-page alignment для reservation,
reservation_len, base и usable span, либо явно описать, какие из них
могут быть только logical PAGE multiples. По возможности проверить
дешёвые адресные инварианты в конструкторе и добавить API для передачи
runtime page size/capability внешнего allocator-а.

### R4-7 — LOW — release/decommit failures скрываются, а release failure
не имеет полноценной диагностики

Unix libc_munmap в lib.rs:3444-3470 и Windows
winapi_virtual_decommit в lib.rs:2563-2590 отбрасывают OS return value,
частично считая ошибки только при bench-internals. Windows
winapi_virtual_release в lib.rs:2592-2602 отбрасывает MEM_RELEASE result
и вообще не имеет отдельного failure counter.

Для корректно созданных внутренних reservation это обычно означает
дефект bookkeeping. Но для unsafe from_raw_parts и внешнего
allocator handoff отказ превращается в тихий leak при Drop; public
release также не даёт caller-у различить успех и ошибку.

Рекомендация: добавить хотя бы release-failure diagnostic hook/counter
в internal builds и продумать fallible try_release для следующего
совместимого API-расширения. Для Drop отдельно сохранить правило
«не panic во время unwinding», но не терять наблюдаемость.

### R4-8 — LOW — fault-injection arm может проиграть concurrent re-arm

Модуль crates/vmem/src/fault_injection.rs:39-48 сам документирует
оставшийся race. После срабатывания should_fail_commit в строках
147-149 сначала сбрасывает FAIL_AT_TARGET, затем FAIL_AT_COUNTER.
Concurrent arm_fail_at может записать новый target между этими двумя
операциями, после чего старый self-disarm обнулит свежую настройку.

Это test/diagnostic feature, не production path, но его public functions
не требуют, чтобы arming выполнялся одним потоком. Ошибка даёт
недетерминированно пропущенный искусственный отказ и может маскировать
ошибки в consumer tests.

Рекомендация: заменить три atomics на CAS/epoch state machine либо
сериализовать arm/fire одним lock; если race сознательно оставляется,
сделать single-armer restriction частью public documentation и
добавить regression scenario для concurrent arm.

### R4-9 — LOW — mock::drain не защищён от reentrant allocator callback

В crates/vmem/src/mock.rs:225-227 drain удерживает RefMut на CALLS
весь период выражения borrow_mut().drain(..).collect(). Создание
возвращаемого Vec может вызвать global allocator. Если этот allocator
записывает новую mock Call, record снова делает borrow_mut() при уже
активном borrow и получает RefCell borrow panic. RECORDING-guard
защищает record от рекурсии внутри самого record, но не охватывает
drain.

Путь редкий, однако mock прямо предназначен для тестирования
allocator/error paths, поэтому это реальная feature-specific
reentrancy boundary.

Рекомендация: сначала atomically/logically извлекать сам Vec через
mem::take, отпустить RefMut, и только затем возвращать/преобразовывать
его; либо установить общий guard на drain. Добавить сценарий, где
выделение возвращаемого журнала проходит через тот же global allocator.

## Документация и API hygiene

### R4-10 — INFO — from_raw_parts всё ещё описан как пятизначный API

В lib.rs:1003-1027 текст говорит о «3 of the 5 fields» и «All five
values», хотя сигнатура в lib.rs:1073-1080 принимает шесть аргументов,
включая granted_huge. Историческая заметка около lib.rs:1447-1450
также описывает старый пятиэлементный tuple.

Это не runtime bug, но перед первым publish может привести adopter-а к
неверному raw handoff и скрывает важность is_huge/decommit capability.
Нужно переписать count/former tuple wording и явно связать шестой
аргумент с ReservationParts/huge-page semantics.

### R4-11 — INFO — ReservationParts теряет usable и huge metadata

into_reservation_parts в lib.rs:823-849 возвращает только underlying
ptr/len/align. Документация предупреждает, что base и usable len нужно
сохранить отдельно, но не подчёркивает, что при реконструкции полного
Reservation также требуется granted_huge.

Для обычного ручного release это корректно. Для межкрейтового handoff
такая структура не является полноценным ownership snapshot и легко
приводит к восстановлению is_huge = false или к потере usable span.
Улучшение: отдельная AdoptionParts со всеми шестью полями либо
явное поле/метод, сохраняющий huge capability.

### R4-12 — INFO/PERF — 64-битный Unix всегда удерживает size + align

В lib.rs:1244-1264 и unix_reserve в lib.rs:2749-2830 exact path
выключен на 64-bit, поэтому mapping over-reserves на size + align и
держит лишнюю virtual address space весь lifetime reservation. Это
осознанный trade-off: exact miss требует дополнительных syscall/trim,
а не ошибка. Но для больших align и большого числа живых сегментов
адресное пространство может стать существенным расходом.

Возможное улучшение: измерить workload, затем исследовать отдельный
64-bit strategy (hint/exact/trim) с проверкой page alignment и failure
cleanup. До измерения текущая одна mmap-модель выглядит разумнее
непроверенной смены на более сложную.

### R4-13 — INFO — старая Linux kernel меняет смысл MAP_HUGE_2MB

Комментарий около lib.rs:3153-3179 отмечает, что size encoding
MAP_HUGE_* появился только в Linux 3.8. На более старом kernel bits
игнорируются и может использоваться default huge-page size, тогда как
crate validation и документация исходят из 2 MiB. Это условный
portability risk, а не доказанный дефект на поддерживаемых runner-ах.

Рекомендация: задекларировать минимальную kernel version для
huge-pages или иметь явный fail-closed/detection path, который не
помечает reservation как ожидаемую 2 MiB huge mapping при неизвестной
семантике kernel.

## Coverage gaps, которые влияют на release confidence

- i686-gnu и i686-musl сейчас проверяются compile-only; runtime
  32-bit exact/huge path не исполняется в CI.
- Успешная Linux MAP_HUGETLB ветка зависит от configured hugetlb pool,
  а успешная Windows MEM_LARGE_PAGES ветка — от privilege и размера
  large page; обычный CI их не представляет.
- BSD, Android, tvOS и watchOS ветки в основном reasoned-from-spec;
  macOS покрывает только часть Darwin family.
- Miri в workflow используется как cargo check с cfg, не как
  interpreter test. Это не проверяет runtime semantics native mmap,
  munmap или madvise.

## Приоритет действий

1. Зафиксировать release policy для Darwin/BSD decommit и MIPS:
   поддерживаемые targets должны либо работать, либо fail clearly at
   compile/configuration time.
2. Устранить повторную 32-битную huge exact attempt и заменить
   attempts == 1 на oracle семантики.
3. Исправить шестипараметричный from_raw_parts contract и raw handoff
   documentation; отдельно уточнить runtime page-size requirements.
4. Добавить наблюдаемость release/decommit failures и решить, нужен ли
   fallible/capability decommit API.
5. Перед performance changes измерить Windows speculative retries и
   64-bit Unix address-space cost; параллельно закрыть fault-injection
   и mock drain reentrancy contracts.

Итоговый verdict статического прохода: codebase заметно подготовлен к
релизу, но target semantics и 32-битная huge ветка ещё недостаточно
строги для безусловного publish без решений из пунктов 1–2.

