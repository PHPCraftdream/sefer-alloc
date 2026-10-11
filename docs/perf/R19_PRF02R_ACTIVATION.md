# R19-PRF02-R Stage C activation — perf(runtime)-кандидат B

Activation PASS 4/4, strictly sequential A1, A2, B1, B2. No performance GO declared; IAI not authorized or launched. All commands background with completion awaits; no polling.

Identity SHA256: 76b4f9d0ed144b1803409f6ecb4f01c52bb3346b51a5a336cd5a91f4052b849c
Preregistration SHA256: d1e951abb5a65b97e332dba38a5db124177979a8a18e2314e2f0cefde04f67ba
Judge SHA256: d5b316991010c275be83697b3618957be96860ef8b8f8fdbfb5dd6b20580d604

Before activation: 428 input hashes verified; original B hashes and external A/B snapshots verified; read-only git-show A verified; reconstructed B patch and both harness hashes verified; new real raw receipts absent. Judge body diagnostic corrected to (body-baseprobe)/64, not body-prealloc-baseprobe. Synthetic selftest uses target/r19-prf02r and asserts numerator2000. Driver/judge selftests PASS; pins replaced before first activation. Runtime logic and predictions unchanged; no git writes or other worktrees used.

Actual counts below; each small window frees16 after prealloc64. Each targeted test: 1 passed, 0 failed; command exit0.

| Run | hits | misses | tcache | routed scans | Large free | inplace | alloc | move | promotion | tail | Raw receipt |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| A1 | 32 | 0 | 16 | 0 | 3 | 1 | 0 | 2 | 3 | 1 | _raw_r19_prf02r_A_activation_1.log |
| A2 | 32 | 0 | 16 | 0 | 3 | 1 | 0 | 2 | 3 | 1 | _raw_r19_prf02r_A_activation_2.log |
| B1 | 16 | 0 | 16 | 0 | 2 | 1 | 0 | 1 | 2 | 0 | _raw_r19_prf02r_B_activation_1.log |
| B2 | 16 | 0 | 16 | 0 | 2 | 1 | 0 | 1 | 2 | 0 | _raw_r19_prf02r_B_activation_2.log |

Restoration: driver finally cp/cmp/hash restores B after every run. Independent cmp and SHA256 verification passed for all 10 original B source files after A1, A2, B1 and B2. Final 428/428 input hashes and preregistration pin match; exclusive lock absent. Raw receipts 4 activation, 0 IAI.
