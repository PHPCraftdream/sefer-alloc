# Scalar prepared issue — экспериментальная доработка и сравнительные замеры

Дата: 2026-10-24. Verdict: **INCONCLUSIVE / DO NOT PROMOTE**.

Изменены два production-файла: удалены повторные prepared-проверки scalar commit; final checked issue, Release store и публичный auto-prepare owner API сохранены. D1′ не используется. B4-R3 batch path не менялся.

## Источники и метод

Base: `f80c681b799ef4ef7fa08d79c06896fdf1104fe9`. Точные patches, SHA-256 и toolchain — [source_identity.json](scalar_issue_prepared/source_identity.json), captured до сборки. Features: `production internals bench-internals`. win32/x64, 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz.

Пререгистрация: `docs/design/2026-10-24-scalar-issue-measurement-plan.md`. Raw данные: `scalar_issue_prepared/raw/`; canonical estimates восстановлены из Criterion samples и проверены генератором. Каноническая таблица — direct GlobalAlloc facade, НЕ installed allocator. Scalar microgate — отдельный AllocCore diagnostic, он не доказывает global speedup.

## Относительно mimalloc до и после

Четыре полных canonical sweep A/B/B/A; два sweep на arm. Before/after и mimalloc — арифметические средние двух Criterion point estimates. Win — медиана двух внутрипроцессных отношений mimalloc/Sefer. Win > 1 означает Sefer быстрее, Win < 1 — медленнее. B/A < 1 означает снижение времени Sefer. Различные единицы и composite teardown не смешиваются. Это описательные числа; два sweep на arm не дают убедительного paired CI изменения.

| Workload | Unit | Before | After | mimalloc A | mimalloc B | Win before | Win after | B/A | mi B/A |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| global_alloc/SeferAlloc/1024B | ns/pair | 25.1947 | 31.9881 | 18.6356 | 25.6784 | 0.7614 | 0.8018 | 1.2696 | 1.3779 |
| global_alloc/SeferAlloc/16B | ns/pair | 19.2801 | 22.9173 | 6.0226 | 7.4051 | 0.3132 | 0.3256 | 1.1887 | 1.2295 |
| global_alloc/SeferAlloc/256B | ns/pair | 22.5709 | 28.6411 | 9.3038 | 13.2033 | 0.4124 | 0.4680 | 1.2689 | 1.4191 |
| global_alloc/SeferAlloc/64B | ns/pair | 22.4338 | 23.4569 | 8.2650 | 10.1553 | 0.3684 | 0.4326 | 1.0456 | 1.2287 |
| global_alloc_churn/SeferAlloc/1024B | ns/pair | 13.3321 | 17.8055 | 85.3032 | 124.3288 | 6.3996 | 7.0342 | 1.3355 | 1.4575 |
| global_alloc_churn/SeferAlloc/16B | ns/pair | 9.8650 | 15.5747 | 8.0363 | 15.1019 | 0.8158 | 0.9899 | 1.5788 | 1.8792 |
| global_alloc_churn/SeferAlloc/256B | ns/pair | 10.5700 | 20.4861 | 16.3961 | 28.3746 | 1.5617 | 1.4137 | 1.9381 | 1.7306 |
| global_alloc_churn/SeferAlloc/64B | ns/pair | 9.6784 | 15.4529 | 9.2376 | 13.3465 | 0.9534 | 0.8739 | 1.5966 | 1.4448 |
| global_alloc_churn_with_teardown/SeferAlloc/1024B | ns/pair composite | 49.7638 | 47.8477 | 29.6805 | 39.2987 | 0.6199 | 0.8926 | 0.9615 | 1.3241 |
| global_alloc_churn_with_teardown/SeferAlloc/16B | ns/pair composite | 13.2543 | 15.9306 | 9.8044 | 12.4332 | 0.7433 | 0.7861 | 1.2019 | 1.2681 |
| global_alloc_churn_with_teardown/SeferAlloc/256B | ns/pair composite | 17.4783 | 22.5324 | 13.5147 | 19.0496 | 0.7723 | 0.8725 | 1.2892 | 1.4095 |
| global_alloc_churn_with_teardown/SeferAlloc/64B | ns/pair composite | 14.2567 | 22.6452 | 9.2497 | 17.1602 | 0.6472 | 0.7538 | 1.5884 | 1.8552 |
| global_alloc_churn_write/SeferAlloc/1024B | ns/pair | 14.5137 | 20.7155 | 85.8664 | 116.7285 | 5.9655 | 5.7397 | 1.4273 | 1.3594 |
| global_alloc_churn_write/SeferAlloc/16B | ns/pair | 10.3198 | 12.3369 | 8.9769 | 9.5846 | 0.8688 | 0.7766 | 1.1955 | 1.0677 |
| global_alloc_churn_write/SeferAlloc/256B | ns/pair | 11.2135 | 14.4729 | 18.2940 | 23.9905 | 1.6267 | 1.6777 | 1.2907 | 1.3114 |
| global_alloc_churn_write/SeferAlloc/64B | ns/pair | 11.1245 | 13.4045 | 10.7105 | 16.4151 | 0.9647 | 1.2053 | 1.2049 | 1.5326 |
| global_alloc/manual_realloc_sim/SeferAlloc | ns/closure | 509.0482 | 658.2268 | 443.7625 | 690.0305 | 0.8822 | 1.0470 | 1.2931 | 1.5550 |

mi B/A показывает drift неизменённого конкурента; нельзя приписать все колебания B/A патчу. Группы global_alloc — warm bulk bursts, не первый холодный alloc. Отсутствие scalar activation в отдельной canonical строке не трактуется как ошибка: основные fastbin refills идут через неизменённый batch path; её числа не служат атрибуцией scalar механизма.

## Ранний scalar gate

20 fresh-process ABBA blocks на N. В таблице — mean двух одноимённых процессов в block, затем mean по 20 blocks. CI относится к среднему paired B−A; внутреннее окно и полный process wall разделены. Oracle после timed region проверяет каждый start и отсутствие class у interior granule. Setup/teardown, oracle и запуск процесса входят в process wall, не internal_ns.

| N | Metric | Mean A ns | Mean B ns | B/A | Paired delta ns | 95% CI ns |
|---:|---|---:|---:|---:|---:|---|
| 32768 | internal_ns | 530177.5000 | 487585.0000 | 0.9197 | -42592.5000 | -85647.5651 … 462.5651 |
| 32768 | process_ns | 9125642.5000 | 9076550.0000 | 0.9946 | -49092.5000 | -564602.4227 … 466417.4227 |
| 65536 | internal_ns | 1096065.0000 | 981522.5000 | 0.8955 | -114542.5000 | -310654.1345 … 81569.1345 |
| 65536 | process_ns | 12705495.0000 | 12690085.0000 | 0.9988 | -15410.0000 | -665060.3732 … 634240.3732 |

### A/A controls

| Metric | Mean pseudo-B−A ns | 95% CI ns | Includes zero |
|---|---:|---|---|
| internal_ns | -128340.0000 | -355517.1259 … 98837.1259 | true |
| process_ns | -705945.0000 | -2066617.5419 … 654727.5419 | true |

### N/2N slope

- internal_ns: marginal mean A = 17.2695 ns/scalar call; B = 15.0738 ns/scalar call. Разность двух means включает изменение cold page/leaf стоимости; не отдельный чистый instruction counter.
- process_ns: marginal mean A = 109.2484 ns/scalar call; B = 110.2763 ns/scalar call. Разность двух means включает изменение cold page/leaf стоимости; не отдельный чистый instruction counter.

## Граница результата

Локально прошли rustfmt --check затронутых Rust-файлов, Clippy -D warnings для lib production и lib/test/example production internals bench-internals, cargo check default и core-only internals negative row нового теста. Correctness execution, Miri/Loom и exact-head CI ещё НЕ получены: локальные тесты не разрешены, push не запрошен. Добавлена отдельная regression suite scalar_issue_prepared; cargo check не означает test PASS. Нет разрешения merge. Все сравнительные процессы здесь — Windows, не native Linux и не WSL.

Контрфактуальный source review: пропуск final class store обнаруживается проверкой class/start; пропуск fallible preflight — проверкой отказа без изменения owner state; потеря owner auto-prepare — проверкой свежего route. Возврат лишних prepared-проверок сохраняет поведение и не обязан ломать correctness test: его цена проверяется performance gate. Существующий ClassLeaves::issue проверяет null leaf и bounds без promotion; root/live-slot/fatal и Acquire/Release сохранены. Новых unsafe-блоков в production нет; unsafe в новом test/probe ограничен освобождением уникальных живых блоков с исходным layout. Это узкий single-context review, не полный rust-intel аудит проекта.

Полная promotion-матрица IAI/dense16/registration/Larson/memory не объявляется закрытой. Общий выигрыш нельзя вывести из удаления двух source-level checks или из одной удачной строки. При непрохождении раннего gate кандидат не принимать; текущая main не меняется.

Воспроизведение: применить соответствующий patch к base, скопировать сохранённый scalar_issue_gate.rs в examples/, использовать отдельные target directories; запустить scalar probe N=32768/65536 по ABBA и A/A, затем npm run bench:table A/B/B/A. Полный runner сохранён в scalar_issue_prepared/reproduction/measure.mjs; его ожидаемый layout описан рядом. Проверка отчёта: `node scripts/scalar_issue_report.mjs --check`.
