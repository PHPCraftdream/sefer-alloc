#!/bin/bash
# U31 v2: all wall-clock / memory measurements, strictly sequential.
set -e
R=${REPO:?set REPO to the primary checkout}; K=$R/.rush/raw_keep/u31/v2
export RUSTFLAGS="-C llvm-args=-align-all-functions=6 -C llvm-args=-align-all-nofallthru-blocks=5"
echo "[1] build var probes $(date)"
cp $R/.rush/u31probe/u31_growth_wall.rs $R/worktrees/u31/examples/
(cd $R/worktrees/u31 && CARGO_TARGET_DIR=$R/.rush/u31tgt/var2 cargo build -q --release --example u31_growth_wall --example ph6b_mt_ab --features "production internals")
rm $R/worktrees/u31/examples/u31_growth_wall.rs
sha256sum $R/.rush/u31tgt/var2/release/examples/{u31_growth_wall,ph6b_mt_ab}.exe $R/.rush/u31tgt/base/release/examples/{u31_growth_wall,ph6b_mt_ab}.exe > $K/binaries.sha256
echo "[2] growth quads $(date)"
node $R/.rush/u31probe/growth_quads.mjs $R/.rush/u31tgt/base/release/examples/u31_growth_wall.exe $R/.rush/u31tgt/var2/release/examples/u31_growth_wall.exe 131072 100 50 $K/growth_quads_131072.jsonl
echo "[3] ablation 11 kernels $(date)"
(cd $R/worktrees/u31-abl && CARGO_TARGET_DIR=$R/.rush/u31tgt/abl node scripts/b4_ablate_run.mjs --out-dir $K/abl --blocks 2000 --reps 20 --kernels pair16,pair64,pair_rt,bulk64,bulk16k,churn16,churn64,churn256,churn1024,vec512,larson --variants u31_scan_backoff --no-null-gate > $K/abl_run.log 2>&1; git checkout docs/perf/B4_ABLATE_CHURN_summary.csv)
(cd $R/worktrees/u31-abl && node scripts/b4_ablate_vs_null.mjs --dir $K/abl --variant u31_scan_backoff --reps 20 --kernels pair16,pair64,pair_rt,bulk64,bulk16k,churn16,churn64,churn256,churn1024,vec512,larson --csv $K/abl/vsnull_u31.csv > $K/abl_vsnull.txt 2>&1)
echo "[4] MT quads $(date)"
(cd $R/worktrees/u31 && node scripts/mt_regression_quads_run.mjs $R/.rush/u31tgt/base/release/examples/ph6b_mt_ab.exe $R/.rush/u31tgt/var2/release/examples/ph6b_mt_ab.exe 100 50 $K/mt_quads.jsonl > /dev/null 2>&1 && node scripts/mt_regression_quads_table.mjs $K/mt_quads.jsonl $K/mt_quads_summary.csv > $K/mt_table.txt 2>&1)
echo "[5] RSS $(date)"
MSYS_NO_PATHCONV=1 wsl -e bash ${REPO_WSL:?set REPO_WSL}/.rush/u31probe/rss_v3.sh ${REPO_WSL:?set REPO_WSL}/.rush/raw_keep/u31/v2/rss_quads.jsonl
MSYS_NO_PATHCONV=1 wsl -e bash ${REPO_WSL:?set REPO_WSL}/.rush/u31probe/rss_mt100_v3.sh ${REPO_WSL:?set REPO_WSL}/.rush/raw_keep/u31/v2/rss_mt100.jsonl
echo "[done] $(date)"
