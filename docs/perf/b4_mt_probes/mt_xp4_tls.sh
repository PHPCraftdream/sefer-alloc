#!/bin/bash
# mt_xp4_tls.sh — R30-8 decisive measurement on the BASE A snapshot
# (.rush/wsl/mt-xp3-rss-a.tar = git archive 2730d02a, X-pin absent):
# R1 TLS pin-cache hit-rate at T=2 and T=4 under larson and mstress.
# Counters build only (production,internals,bench-internals); no Ir here.
set -e
LP=mt-xp4-tls
D=~/ab/$LP; rm -rf $D; mkdir -p $D
tar -xf $REPO_WSL/.rush/wsl/mt-xp3-rss-a.tar -C $D
cp $REPO_WSL/.rush/wsl/mt_xp4_tls_probe.rs $D/examples/zz_mt_xp4_tls.rs
printf '\n[[example]]\nname = "zz_mt_xp4_tls"\nrequired-features = ["alloc-global"]\n' >> $D/Cargo.toml
(cd $D && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgtxp4-$LP \
  cargo build -q --release --example zz_mt_xp4_tls --features production,internals,bench-internals)
BIN=~/ab/tgtxp4-$LP/release/examples/zz_mt_xp4_tls
for wl in larson mstress; do
  for t in 2 4; do
    echo "== base A $wl T=$t =="
    $BIN sefer $wl 100000 $t 2>&1 | grep -E 'PINS|error'
  done
done
echo "DONE mt_xp4_tls"
