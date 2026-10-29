# Один MIR-exposure эксперимент — prereg до B

Исходная композиция: scalar flush + validated metadata slot, но baseline сравнения остаётся accepted f80c681b799ef4ef7fa08d79c06896fdf1104fe9. Никаких сумм прошлых выигрышей.

Evidence механизма: в измеренном scalar binary entry имеет216bytes и физический call675-byte flush_run с literal len=1 (.rush/own-free-single/symbols.txt и scalar-asm.txt). Константа не устранила общий callee boundary. Один вариант: изменить только scalar entry inline(never)→inline(always), чтобы MIR и длина1 были видимы в validated-overflow context. Generic batch flush_run/guards/policy не трогаются. Это не перебор атрибутов: если boundary/стоимость не улучшатся или появится regression, этот вариант останавливается.

Все safety/liveness contracts композиции unchanged: slot изstored-root StateByte послеguard, currentLIVEcredit удерживаетmapping черезresidentflush, currentpayload не трогаем, pointerнеescapes; caller/counter/flush/compact/state/mask порядок прежний. Новых unsafe операций/Surface API/настроек/версий нет.

Gates: pureproductionsameprobes, rtfree≤base−4Ir,const≤base,bulk64≤base,bulk16k≤base−30,repeat/Mi≤0.01Ir. Reference baseline reused immutable prior snapshot+toolchain, overflow A свежий. Два проходаN50000/100000 и bulk100/200rounds. Сначала cheap controls; native/broad matrix только лучшей композиции. Source identity доbuild; raw→checkedgenerator→CSV/report. SourceMIR/noop не считать speedup.

Отдельный semantic test full16/cap4 state/identity/credit/reuse компилируется; local tests/Miri/Loom/doc/agents/commit/push/merge не разрешены, comparative build/profile runs разрешены broad request пользователя. Если по завершении этого эксперимента новых механизмов с доказательством нет — закончить селекцию, проверить лучшую композицию шире и честно сохранить acceptance limits.
