#!/bin/bash
# MT1 refresh: callgrind Ir, g1_mt_one T=1, base (c0ee7dfb) vs x (WILD-2) vs mimalloc, mstress and larson
set -e
source ~/.cargo/env
G=<scratch dir>
W=$G/wild2
O=$W/mt1c; mkdir -p $O
S=$HOME/w2m; mkdir -p $S
for L in base x; do
  rm -rf $S/$L; mkdir -p $S/$L; tar -xf $W/$L.tar -C $S/$L
  cp $G/g1_mt_one.rs $S/$L/examples/
  python3 - $S/$L/Cargo.toml <<'PY'
import sys
p=sys.argv[1]; s=open(p).read()
a='[[example]]\nname = "malloc_macro"'
if 'name = "g1_mt_one"' not in s:
    s=s.replace(a,'[[example]]\nname = "g1_mt_one"\nrequired-features = ["alloc-global", "alloc-xthread"]\n\n'+a,1)
open(p,'w').write(s)
PY
  (cd $S/$L && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=$S/tgt-$L cargo build -q --release --example g1_mt_one --features production)
done
sha256sum $S/tgt-base/release/examples/g1_mt_one $S/tgt-x/release/examples/g1_mt_one | sed "s#$S/##" > $O/ident.txt
cd /tmp
: > $O/totals.txt
for w in mstress larson; do
  for a in base x; do
    valgrind --tool=callgrind --callgrind-out-file=/tmp/mt1c.$w.$a.out $S/tgt-$a/release/examples/g1_mt_one $w sefer 1 300000 > /dev/null 2>&1
    echo "$w $a $(callgrind_annotate /tmp/mt1c.$w.$a.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}')" >> $O/totals.txt
    callgrind_annotate --threshold=100 /tmp/mt1c.$w.$a.out 2>/dev/null | grep -E '^ *[0-9,]+ ' > $O/cgfn.$w.$a.txt
  done
  valgrind --tool=callgrind --callgrind-out-file=/tmp/mt1c.$w.mi.out $S/tgt-base/release/examples/g1_mt_one $w mi 1 300000 > /dev/null 2>&1
  echo "$w mi $(callgrind_annotate /tmp/mt1c.$w.mi.out 2>/dev/null | grep 'PROGRAM TOTALS' | awk '{print $1}')" >> $O/totals.txt
  callgrind_annotate --threshold=100 /tmp/mt1c.$w.mi.out 2>/dev/null | grep -E '^ *[0-9,]+ ' > $O/cgfn.$w.mi.txt
done
echo mt1c-done
