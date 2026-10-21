#!/bin/bash
# Features: production (FEAT, 4-й аргумент).
# Судья: aoc — rt/c16/c64 alloc/free Ir/op (+cache).
# Запуск ВЕЗДЕ с явным `timeout 600` и `CARGO_TARGET_DIR` вне репо (изолированный target); переменные: REPO (корень репо, Windows-путь), REPO_WSL (тот же корень под /mnt/<drive>/… в WSL), AB_DIR (scratch, по умолчанию $HOME/ab), SCRATCH (каталог логов вне репо).
# Allocator-only deterministic profile, CONSTANT-Layout mode: same churn as
# prof_ao_f.sh but via examples/zz_alloc_const built from
# alloc_only_const_probe.rs. Measures runtime-Layout control (w_alloc/w_free)
# plus const wrappers w_alloc16/w_free16 and w_alloc64/w_free64, whose Layout
# is a compile-time constant inside the #[inline(never)] wrapper.
# Usage (WSL): bash prof_ao_c.sh <label> [sefer|mimalloc] [features]
AB_DIR=${AB_DIR:-$HOME/ab}
set -e
L=$1; A=${2:-sefer}; FEAT="${3:-production}"
D=$AB_DIR/$L; rm -rf $D; mkdir -p $D; tar -xf ${REPO_WSL}/.rush/wsl/$L.tar -C $D
cp ${REPO_WSL}/.rush/wsl/alloc_only_const_probe.rs $D/examples/zz_alloc_const.rs
printf '\n[[example]]\nname = "zz_alloc_const"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$AB_DIR/tgtao-$L cargo build -q --release --example zz_alloc_const --features "$FEAT")
cd $AB_DIR
profile() { # wrapper-name mode
  local w=$1 mode=$2
  for k in 50000 100000; do valgrind --tool=callgrind --toggle-collect="*zz_alloc_const::$w*" --callgrind-out-file=pa.$L.$A.$w.$k.out $AB_DIR/tgtao-$L/release/examples/zz_alloc_const $A $k $mode >/dev/null 2>&1; done
  t1=$(callgrind_annotate pa.$L.$A.$w.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//'); t2=$(callgrind_annotate pa.$L.$A.$w.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
  echo "== $A $w (mode=$mode) marginal Ir/op: $(( (t2 - t1) / 50000 ))"
  for k in 50000 100000; do callgrind_annotate --auto=yes --show-percs=no --threshold=100 pa.$L.$A.$w.$k.out 2>/dev/null | awk '/-- Auto-annotated source: /{src=$NF; next} /^ *[0-9,]+ /{ if(src !~ /^\?\?\?/ && $0 !~ /=>/){ v=$1; gsub(",","",v); line=$0; sub(/^ *[0-9,]+ +/,"",line); s[src" | "line]+=v } } END{for(k in s) print s[k]"\t"k}' > pax.$k.txt; done
  awk -F'\t' 'NR==FNR{a[$2]=$1;next}{d=$1-a[$2]; if(d>25000) printf "%6.1f\t%s\n", d/50000, $2}' pax.50000.txt pax.100000.txt | sort -rn | sed "s|$L/||; s|$AB_DIR/||" | grep -v "events annotated\|PROGRAM TOTALS" | head -30
}
profile w_alloc rt
profile w_free rt
profile w_alloc16 c16
profile w_free16 c16
profile w_alloc64 c64
profile w_free64 c64
