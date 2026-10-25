# Scalar issue: preregistration сравнительных замеров

2026-10-24. До первого B-run. Baseline: `f80c681b799ef4ef7fa08d79c06896fdf1104fe9`. Кандидат отличается в production только `RouteSlots::issue_small` и новым crate-private `SmallSidecar::issue_prepared`. D1′ не входит ни в один arm.

## Последовательность и критерии

1. `scalar_issue_gate` на N=32768 и 65536: по 20 fresh-process ABBA blocks на размер; затем 10 AA blocks на N=65536. Оба arms имеют идентичный example и Cargo declaration. Прямой вызов существующего scalar-carve hook гарантирует scalar activation; class/start oracle выполняется за пределами внутреннего окна. Это diagnostic `AllocCore`, не global allocator и не конкурентное сравнение с mimalloc.
2. Для каждого N вычислить per-block среднее двух A и двух B, paired delta B−A, mean и Student-t 95% CI. Условие предварительной полезности: upper CI внутреннего delta < 0 и отношение mean B / mean A <= 0.99. N/2N slope приводится отдельно от totals; сравнение полных process wall — отдельная ось. AA обязан не показывать значимого направленного сдвига. Невыполнение — INCONCLUSIVE/NO-GO для promotion; нельзя менять пороги после данных.
3. По явному запросу пользователя получить описательное сравнение относительно mimalloc даже если scalar gate не проходит: четыре полных canonical `npm run bench:table` в порядке A/B/B/A, с отдельным CARGO_TARGET_DIR для каждого arm. Features как у штатного скрипта: `production internals bench-internals`. Сохранить все Criterion samples/estimates, не только хорошие строки. Для каждой строки вывести ns/pair (manual growth chain — ns/closure), mimalloc/Sefer before и after, B/A. Это direct GlobalAlloc facade, не installed allocator. Два прогона на arm недостаточны для самостоятельного paired process CI: таблица описательная, её колебания не называют доказанным speedup.
4. Полная promotion-матрица (IAI marginal, dense16/registration/LarsonT4/T8, memory и exact-head CI) не запускается автоматически после слабого/отрицательного scalar результата. При успешном раннем gate эти проверки всё равно обязательны; canonical таблица не подменяет их. Удаление сравнительного преимущества относительно mimalloc не считается улучшением.

## Воспроизводимость и ограничения

До сборки runner сохраняет patch против baseline, SHA-256 patch и нового example, точный toolchain, OS/CPU, features, source hashes. В конце проверяет неизменность source inputs. Raw samples первичны; отчёт и CSV генерируются одним проверяющим скриптом. Compiler/build stdout с локальными абсолютными путями остаётся scratch; переносимые artifacts не должны содержать частные пути.

Новые hooks/counters, refill changes, production features, зависимости и версии не добавляются. Публичный owner API остаётся прежним. Новая regression suite проходит cargo check, но execution/Miri/Loom и полный CI ещё не проверены: push и локальные тесты в текущем запросе не разрешены. Замеры экспериментального кандидата не означают correctness acceptance или разрешение merge.
