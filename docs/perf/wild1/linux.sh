#!/bin/bash
source ~/.cargo/env
W=<dir with src.tar, linux_schedule.txt>; S=$HOME/w1
rm -rf $S/src; mkdir -p $S/src; tar -xf $W/src.tar -C $S/src
(cd $S/src && CARGO_TARGET_DIR=$S/tgt cargo build -q --release --example g1_wild_judge --features "production internals") || exit 1
E=$S/tgt/release/examples/g1_wild_judge
{ echo "linux build of git archive HEAD (sha256 of tar: $(sha256sum $W/src.tar | cut -c1-64))"; echo "$(rustc -V)"; echo "$(uname -sr)"; sha256sum $E | sed "s#$S/##"; } > $W/linux_ident.txt
: > $W/linux_prof_raw.jsonl
tr -d '\r' < $W/linux_schedule.txt | while read b sc arm k; do
  r=$(WJ_PROF=1 $E $sc $arm 1 1) || exit 1
  echo "{\"block\":$b,\"rep\":$k,\"arm\":\"$arm\",\"r\":$r}" >> $W/linux_prof_raw.jsonl
done
echo linux-done
