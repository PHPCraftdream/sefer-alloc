# Native alloc leaf entry — пререгистрация эксперимента

База: f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Новый независимый кандидат, не композиция B. Цель — проверить причинную гипотезу Windows alloc-frame, а не ещё одну перестановку attributes.

## Evidence и механизм

Чтение прежних frozen production native binaries: `docs/perf/NATIVE_OWNER_CODEGEN_READ.md` в основном исследовательском worktree. Const magazine hit не содержит call, но общий alloc entry сохраняет rsi и выделяет 32 B shadow space; та же unwind-функция содержит non-tail bind/refill и null/process-exit обработку. TLS — native loads, не lazy-init helper.

Гипотеза: законченный bound magazine hit можно оставить leaf, если все call-bearing продолжения завершать отдельным tail entry, сохранив их прежнее поведение. Это не обещание native benefit.

## Изменение

- Пассивный `current_for_trim` используется только как existing-bound lookup; null/TORN/Err уходят в прежний полный allocation path.
- Single classification передаётся через hot magazine-pop и bound-miss continuation. Сам pop/virgin-mask/hit-stat/state-clear переносится в один shared helper, а не копируется.
- Bound miss может повторно прочитать count внутри прежнего classified body: это явная возможная цена control, не скрытая оптимизация.
- Unbound continuation использует прежний current_heap/config/fallback путь. Bound helper не читает config, как и прежний bound resolver.
- Null/process-exit escape остаётся тем же; alloc_zeroed/realloc, exact-object gate, producer protocol, free и magazine policy не меняются.
- Нового persistent cache, поля, feature или публичного pointer API нет.

## Proof boundary

Пассивный TLS lookup даёт тот же живой exclusive owner HeapCore. Между lookup/pop и miss continuation нет callback/синхронизации/передачи управления другому owner. Поп сохраняет прежний порядок мутаций, class получен прежним classifier. Null/TORN/first-bind/config-conflict и process-exit branches не обобщаются из hit. Non-fastbin build сохраняет прежний entry. Отдельные semantic regressions пишутся в tests/; локального test/Miri/Loom execution разрешения нет.

## Gates до просмотра новых чисел

1. Native codegen: const16 и const64 bound magazine-hit wrappers не должны иметь entry push/sub-rsp frame или call на hit; runtime wrapper не должен получить дополнительный frame tax. Если leaf не получен — эта форма закрывается без attribute-перебора.
2. WSL production-only marginal alloc/free/full pair: const16/64 и runtime; снижение alloc Ir не покупается ростом free. Противоположные controls — fresh refill/bulk64, cap-eviction bulk16KiB. Любой наблюдаемый рост цены указывается; строгий zero-regression GO при положительном delta не заявляется.
3. Native: свежие before/after binaries, одинаковые source harness, production features, baseline/candidate/mimalloc; 20 balanced process blocks и 10 A/A blocks. Все samples остаются. Win определяется как mean(mi)/mean(Sefer), paired CI B−A по block means. Значимый target benefit требует CI верхней границей ниже нуля; controls с неопределённостью не объявляются доказанной нерегрессией.
4. Только survivor получает broad IAI/canonical/installed/MT по области изменения. Canonical churn16B — обязательный ранее неблагоприятный control, не заменяется новой удачной ячейкой.

Source patch и hashes фиксируются до build/measure; binary hashes до и после запуска. Builds/runs только benchmark/profile; no local tests/Miri/Loom/doc, no agents/install/version/commit/push/merge. Каждый долгий запуск — run_in_background и notification, без polling. Итог может быть NO-GO/INCONCLUSIVE; меньший Ir сам по себе не является продвижением.
