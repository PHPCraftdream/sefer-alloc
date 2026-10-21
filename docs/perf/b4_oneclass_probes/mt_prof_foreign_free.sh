#!/bin/bash
# Features: production (+bench-internals для oracle-прогона).
# Судья: foreign — Ir/free + oracle активации.
# Запуск ВЕЗДЕ с явным `timeout 600` и `CARGO_TARGET_DIR` вне репо (изолированный target); переменные: REPO (корень репо, Windows-путь), REPO_WSL (тот же корень под /mnt/<drive>/… в WSL), AB_DIR (scratch, по умолчанию $HOME/ab), SCRATCH (каталог логов вне репо).
# mt_prof_foreign_free.sh — Phase-1 baseline judge (b4-mt): REAL foreign-free
# path through GlobalAlloc::dealloc, measured by callgrind marginal Ir/op on
# the #[inline(never)] w_free window, plus a bench-internals oracle run for
# path activation (B4_FOREIGN_PUBLISH_OK, PIN_CACHE hits/misses >= 95%).
# Modeled on prof.sh / prof_ao.sh (NOT modified; this is a new mt_* script).
# Usage (WSL):
#   bash mt_prof_foreign_free.sh <worktree-name> <label> <foreign|own> <locality|capacity>
# Expects .rush/wsl/<label>.tar (working-tree snapshot of the worktree, made
# by mt_prof_foreign_free_win.sh). Read-only for the worktree.
AB_DIR=${AB_DIR:-$HOME/ab}
set -e
WT=$1; L=$2; MODE=${3:-foreign}; TRACE=${4:-locality}
D=$AB_DIR/$L; rm -rf $D; mkdir -p $D; tar -xf ${REPO_WSL}/.rush/wsl/$L.tar -C $D
cp ${REPO_WSL}/.rush/wsl/mt_foreign_free_probe.rs $D/examples/zz_mt_foreign_free.rs
printf '\n[[example]]\nname = "zz_mt_foreign_free"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
BIN=$AB_DIR/tgtp-$L/release/examples/zz_mt_foreign_free
# --- production build (NO counters) for the reported Ir -------------------
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$AB_DIR/tgtp-$L cargo build -q --release --example zz_mt_foreign_free --features production)
cd $AB_DIR
for k in 50000 100000; do
  valgrind --tool=callgrind --toggle-collect='*zz_mt_foreign_free::w_free*' \
    --callgrind-out-file=pmt.$L.$MODE.$TRACE.$k.out $BIN $MODE $TRACE $k >/dev/null 2>&1
done
t1=$(callgrind_annotate pmt.$L.$MODE.$TRACE.50000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
t2=$(callgrind_annotate pmt.$L.$MODE.$TRACE.100000.out | grep -m1 TOTALS | tr -d ', ' | sed 's/(.*//')
echo "MARGINAL Ir/free ($MODE $TRACE): $(( (t2 - t1) / 50000 ))"
# --- inclusive frames (NON-ADDITIVE: report each separately, never summed) -
for k in 50000 100000; do
  callgrind_annotate --tree=calling --inclusive=yes --threshold=100 pmt.$L.$MODE.$TRACE.$k.out 2>/dev/null \
    | grep -E 'zz_mt_foreign_free::w_free|GlobalAlloc|dealloc_routing|owned_block_of|hash_find|publish_foreign|publish_cached|publish_foreign_uncached|publish_small_borrowed|SidecarBitmap::publish|canonical_base|publish' \
    | tr -d ',' >> pmti.$L.$MODE.$TRACE.txt || true
done
echo "--- inclusive frames (each separately; DO NOT sum)"
sort -u pmti.$L.$MODE.$TRACE.txt | head -40
# --- bench-internals oracle build: counters, NO callgrind ------------------
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$AB_DIR/tgto-$L cargo build -q --release --example zz_mt_foreign_free --features production,internals,bench-internals)
OBIN=$AB_DIR/tgto-$L/release/examples/zz_mt_foreign_free
OK=10000; [ "$TRACE" = capacity ] && OK=100000
$OBIN $MODE $TRACE $OK oracle > pmtoracle.$L.$MODE.$TRACE.txt 2>&1
cat pmtoracle.$L.$MODE.$TRACE.txt
# Oracle verdict (foreign locality trace must be >=95% pin-cache hits;
# foreign mode must publish every free: delta publish_ok == frees).
ok=$(grep -o 'publish_ok=[0-9]*' pmtoracle.$L.$MODE.$TRACE.txt | tail -1 | cut -d= -f2)
pre=$(grep -o 'publish_ok=[0-9]*' pmtoracle.$L.$MODE.$TRACE.txt | head -1 | cut -d= -f2)
h1=$(grep -o 'pin_hits=[0-9]*' pmtoracle.$L.$MODE.$TRACE.txt | head -1 | cut -d= -f2)
h2=$(grep -o 'pin_hits=[0-9]*' pmtoracle.$L.$MODE.$TRACE.txt | tail -1 | cut -d= -f2)
m1=$(grep -o 'pin_misses=[0-9]*' pmtoracle.$L.$MODE.$TRACE.txt | head -1 | cut -d= -f2)
m2=$(grep -o 'pin_misses=[0-9]*' pmtoracle.$L.$MODE.$TRACE.txt | tail -1 | cut -d= -f2)
awk -v ok="$ok" -v pre="$pre" -v h1="$h1" -v h2="$h2" -v m1="$m1" -v m2="$m2" -v mode="$MODE" -v trace="$TRACE" -v n="$OK" 'BEGIN{
  dh=h2-h1; dm=m2-m1; dok=ok-pre; den=dh+dm;
  frac=(den>0)?dh/den:1;
  printf "ORACLE deltas: publish_ok=%d pin_hits=%d pin_misses=%d hit_frac=%.4f\n", dok, dh, dm, frac;
  if (mode=="foreign") {
    verdict=(dok==n && frac>=0.95) ? "PASS" : "FAIL";
    if (trace=="capacity") verdict=(dok==n) ? "PASS(capacity: misses expected)" : "FAIL";
  } else { verdict=(dok==0) ? "PASS(own control: no foreign publishes)" : "FAIL"; }
  print "ORACLE VERDICT: " verdict;
}'
echo "DONE mt_prof_foreign_free $WT $L $MODE $TRACE"
