#!/bin/bash
# mt_rss_pin.sh — WSL side of the R26-4 subprocess RSS probe (blocker 2).
# Expects .rush/wsl/<LP>-a.tar (base 2730d02a) and <LP>-b.tar (current tree),
# made by mt_rss_pin_win.sh. Builds the probe with internals∧bench-internals
# (measurement-only build; Ir is NEVER taken from this build) and runs ONE
# PROCESS per arm. Prints arm RSS + System bytes and the delta verdict
# (threshold: delta idle RSS <= +1 MiB).
set -e
LP=${1:-mt-xp3-rss}
D=~/ab/$LP; rm -rf $D; mkdir -p $D
for arm in a b; do
  rm -rf $D/$arm; mkdir -p $D/$arm
  tar -xf $REPO_WSL/.rush/wsl/$LP-$arm.tar -C $D/$arm
  cp $REPO_WSL/.rush/wsl/mt_rss_pin_probe.rs $D/$arm/examples/zz_mt_rss_pin.rs
  printf '\n[[example]]\nname = "zz_mt_rss_pin"\nrequired-features = ["alloc-global"]\n' >> $D/$arm/Cargo.toml
  (cd $D/$arm && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_TARGET_DIR=~/ab/tgtrss-$LP-$arm \
    cargo build -q --release --example zz_mt_rss_pin --features production,internals,bench-internals)
done
echo "===== arm A (base 2730d02a) ====="
~/ab/tgtrss-$LP-a/release/examples/zz_mt_rss_pin 2>&1 | tee $D/rss-a.txt
echo "===== arm B (current tree) ====="
~/ab/tgtrss-$LP-b/release/examples/zz_mt_rss_pin 2>&1 | tee $D/rss-b.txt
awk -v fa=$D/rss-a.txt -v fb=$D/rss-b.txt 'BEGIN{
  while ((getline line < fa) > 0) { if (line ~ /^RSS_KIB/) split(line,x," "); }
  ra=x[2]+0;
  while ((getline line < fb) > 0) { if (line ~ /^RSS_KIB/) split(line,y," "); }
  rb=y[2]+0;
  d=rb-ra;
  printf "DELTA_RSS_KIB %d (%.2f MiB)  threshold <= +1024 KiB : %s\n", d, d/1024, (d<=1024)?"PASS":"FAIL";
}'
echo "DONE mt_rss_pin $LP"
