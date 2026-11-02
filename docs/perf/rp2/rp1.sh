#!/bin/bash
# RP-1: owner allocation latency after a wave of foreign frees, sefer (c0ee7dfb) vs mimalloc, interleaved processes
set -e
source ~/.cargo/env
W=<scratch dir>
S=$HOME/w2m
O=$W/rp1/out; mkdir -p $O
cp $W/rp1/g1_rp1_probe.rs $S/base/examples/
python3 - $S/base/Cargo.toml <<'PY'
import sys
p=sys.argv[1]; s=open(p).read()
a='[[example]]\nname = "malloc_macro"'
if 'name = "g1_rp1_probe"' not in s:
    s=s.replace(a,'[[example]]\nname = "g1_rp1_probe"\nrequired-features = ["alloc-global", "alloc-xthread"]\n\n'+a,1)
open(p,'w').write(s)
PY
(cd $S/base && CARGO_TARGET_DIR=$S/tgt-base cargo build -q --release --example g1_rp1_probe --features production)
BIN=$S/tgt-base/release/examples/g1_rp1_probe
sha256sum $BIN | sed "s#$S/##" > $O/ident.txt
: > $O/raw.txt
for cfg in "4 64 3" "16 64 3" "64 64 3" "16 1024 3"; do
  set -- $cfg
  for i in 1 2 3 4 5 6 7; do
    for who in sefer mi; do
      n=$(( $1 * (4194304 / $2) * 9 / 10 ))
      $BIN $who $1 $2 $3 $n >> $O/raw.txt
    done
  done
done
echo rp1-done
