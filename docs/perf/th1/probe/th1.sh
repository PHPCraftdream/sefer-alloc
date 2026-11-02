#!/bin/bash
# TH-1: MADV_HUGEPAGE on sefer spans (Linux probe): sefer (TH1_OFF) vs sefer+THP vs mi, ABBA x2.
# Inputs in $WORK: base/x source tarballs (git archive of the two SHAs), the judge source and g1_mt_one.rs; $SCRATCH is a Linux-local build directory.
source ~/.cargo/env
P=${PERF:-perf}
G=${WORK:?set WORK to the inputs directory}; O=$G/th1d; mkdir -p $O; : > $O/raw.txt
rm -rf ${SCRATCH:-$HOME/ab}/th1; mkdir -p ${SCRATCH:-$HOME/ab}/th1; tar -xf $G/th1.tar -C ${SCRATCH:-$HOME/ab}/th1
(cd ${SCRATCH:-$HOME/ab}/th1 && CARGO_TARGET_DIR=${SCRATCH:-$HOME/ab}/tgt-th1 cargo build -q --release --example g1_wild_judge --features production)
B=${SCRATCH:-$HOME/ab}/tgt-th1/release/examples/g1_wild_judge
one() { # tag arm scen env
  if [ "$1" = off ]; then E="TH1_OFF=1"; else E=""; fi
  out=$(env $E $P stat -e page-faults -x, -o /tmp/th1.stat $B $3 $2 1 0.5 2>/dev/null)
  echo "$1 $3 $(grep -o '^[0-9]*' /tmp/th1.stat | head -1) $out" >> $O/raw.txt
}
for s in grow64 grow1k growmix regrow large wild phase; do
  for r in 1 2; do one off sefer $s; one on sefer $s; one mi mi $s; one on sefer $s; one off sefer $s; done
done
echo done
