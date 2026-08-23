# Новое исследование `aligned-vmem` перед публикацией

- Автор: Сол-кодекс
- Время: 2026-08-23 13:12:01 (Europe/Berlin)
- Ревизия: `db50f1acbeae459904b02f9699979c3b966786e2`
- Режим: только чтение, один агент, без под-агентов.
- Не запускались: тесты, сборка, `cargo check`, Clippy, Miri, бенчмарки, документация и `cargo publish`.

## Итоговый вердикт

**NO-GO для публикации текущего состояния.** Прямой блокер — в `crates/aligned-vmem/CHANGELOG.md:7` всё ещё стоит `0.2.0 - Unreleased`, а workflow релиза в `.github/workflows/release.yml` проверяет это и завершает релиз ошибкой.

В просмотренных текущих unsafe/FFI-, RAII- и арифметических путях нового очевидного критического UB не найдено. Предыдущее опасное покрытие с диапазоном за пределами reservation удалено, а контракты `decommit`/`recommit` теперь явно требуют `end <= reservation.len()`. Основные остаточные замечания — релизная гигиена и рассинхрон документации с текущим API; дополнительно есть несколько возможностей оптимизации, которые требуют измерений.

## Findings

### F1 — P0: unreleased changelog блокирует релиз

`crates/aligned-vmem/CHANGELOG.md:7` содержит `## 0.2.0 - Unreleased`. Workflow публикации (`.github/workflows/release.yml`, проверка changelog около строк 301–308) намеренно отклоняет такой раздел.

Перед публикацией нужно зафиксировать дату/статус релиза и убедиться, что release notes соответствуют фактическому набору изменений текущей ревизии.

### F2 — P2: устаревшее описание возможности decommit для huge reservations

`crates/aligned-vmem/src/reservation.rs` в документации `Reservation::decommit_reclaims_and_zeroes` всё ещё говорит, что decommit для huge-page reservations «silently fails». Это больше не является общей истиной: текущий backend допускает один документированный Linux/Android-сценарий для huge-aligned eager ranges на системах с соответствующей поддержкой (`crates/aligned-vmem/src/api/decommit.rs`). Другие документы crate уже описывают это точнее и подчёркивают, что capability-флаг консервативен и может недооценивать результат.

Нужно привести текст associated const к той же формулировке: const намеренно описывает только гарантированный ordinary-backend случай, а huge-range должен быть обозначен как зависящий от диапазона и kernel/backend, а не как безусловный silent failure.

### F3 — P2: документация `fault-injection` не охватывает добавленный decommit hook

В `Cargo.toml`, README и changelog feature описывается главным образом как инъекция отказа commit path. После последних изменений появился публичный `arm_fail_next_decommit` и отдельный decommit hook (`src/fault_injection.rs`, вызывается из `dispatch_try_decommit`). Следовательно, текущая документация неполна.

Нужно описать оба пути — commit и fallible decommit — и явно указать, что feature публичная, process-global, opt-in, предназначена для контролируемой fault injection, а срабатывание decommit hook симулирует отказ без syscall. Также следует обновить предупреждение о feature unification: включение feature в workspace/dependency graph может менять поведение всех использующих её экземпляров.

Связанная формулировка в `src/decommit_outcome.rs` называет decommit hook «test-only second source». Для публичной Cargo feature точнее сказать «optional fault-injection source» или «source enabled when the feature is explicitly armed».

### F4 — P3: хрупкий census конструкторов `VmemError` устарел

В текущем unreleased changelog сохранён подсчёт прямых конструкторов `VmemError` и список из трёх test-only источников. После добавления decommit fault-injection появился ещё один test/fault-injection constructor. Поэтому прежний census (10 всего, 7 production, 3 test-only) больше не соответствует исходному коду: статический пересчёт даёт 11 прямых мест, из них 7 production и 4 test/fault-injection.

Рекомендуется либо обновить число и список, либо убрать из release notes хрупкую точную статистику и оставить проверяемое описание инварианта: все публично наблюдаемые ошибки создаются через единый тип и должны сохранять диагностическую причину.

### F5 — P2: реальный OS refusal decommit остаётся непроверенным статически

Новый hook позволяет детерминированно проверить преобразование отказа в `DecommitOutcome::Refused`, но не доказывает отказ ядра/ОС. Текущий тестовый комментарий это честно отмечает; в этом исследовании тесты не запускались.

Это не повод возвращать прежний опасный тест с недопустимым диапазоном. Если потребуется дополнительное покрытие, оно должно использовать безопасный, явно поддерживаемый механизм отказа или отдельный backend seam, не нарушающий safety contract. До этого limitation следует оставить явно зафиксированным в release/validation документации.

## Безопасность и корректность

- Проверены текущие контракты публичных `decommit` и `recommit`: граница `end` должна находиться внутри reservation до вычислений адресов.
- Проверены checked arithmetic, page-size/range validation, mock/native dispatch, Unix/Windows ownership и release paths на уровне чтения исходников.
- В текущей версии не обнаружен новый очевидный дефект владения mapping или UB в просмотренных путях. Это статический вывод, а не результат выполнения.
- Ограничения для adopted HugeTLB и 2 MiB alignment теперь документированы; решение не принимать произвольные non-2MiB HugeTLB reservations также зафиксировано владельцем.

## Возможности ускорения и уменьшения затрат

1. **Linux/Android HugeTLB.** Для alignment больше 2 MiB текущий over-reserve сохраняет целое `size + align` отображение. При bounded HugeTLB pool slack также оплачивается страницами пула; для некоторых малых запросов это может дать заметное кратное перерасходование. Предпочтение alignment 2 MiB уже является практической рекомендацией. Более сложный trim/remap следует рассматривать только после измерений: прежнее изменение lifetime mapping уже устранило риск утечки, и возвращать trim вслепую нельзя.
2. **Windows huge-page fallback.** В некоторых непривилегированных диапазонах сначала пробуется combined reserve+commit с large pages, после чего выполняется retry/fallback. Это может добавлять syscalls на неудачный путь. Возможные улучшения — preflight/caching capability или более узкая fast path, но только после профилирования на целевых системах.
3. **`fault-injection` в production graph.** При включённой feature настоящий commit path платит за atomic check и uncontended mutex на каждый commit; feature выключена по умолчанию. Для production рекомендуется не включать её без необходимости. Если feature предполагается только для тестовой инфраструктуры, можно отдельно рассмотреть изменение API/сборочной модели в будущем, но это уже semver/design решение.
4. **Обычный 64-bit Unix over-reserve.** Дополнительный virtual-address slack удерживается lifetime mapping. Это не RSS-расход и выглядит осознанным компромиссом ради выравнивания; оптимизировать его стоит только при подтверждённой потребности.

Измерения не выполнялись по прямому требованию пользователя, поэтому ни один из пунктов выше не заявляется как подтверждённое benchmark-ускорение.

## Что исправить перед публикацией

1. Убрать `Unreleased`, выставить дату релиза и проверить release notes.
2. Исправить описание `decommit_reclaims_and_zeroes` для huge reservations.
3. Обновить документацию `fault-injection` для commit и decommit, включая feature-unification и process-global semantics.
4. Исправить либо удалить устаревший точный census `VmemError`.
5. Зафиксировать residual limitation реального OS refusal без небезопасного синтетического диапазона.
6. Оптимизации HugeTLB и Windows рассматривать отдельной задачей после измерений на поддерживаемых ОС; они не являются текущими доказанными блокерами корректности.

После выполнения первых четырёх пунктов и проверки релизного workflow статический review можно считать достаточным основанием для повторного go/no-go решения. В текущей ревизии публикацию рекомендовать нельзя прежде всего из-за F1 и сопутствующих несогласованных release docs.
