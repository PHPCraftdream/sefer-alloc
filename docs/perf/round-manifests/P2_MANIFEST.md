# Phase 2 manifest — workspace split and verification tooling

## Scope and commit classification

The first P2.0 commit is `5565827191ffdad127806daf0a48f4ddb5801115` (back-edges a, c and d); its sole parent, `a966aa8f5e416f2f0f1902bcdb3e1cf7a9ead261`, is the exclusive lower bound. The inclusive upper bound is the observed `origin/main`, `bf6364788445fe54ccde9781590f8c682c015303` (PR #284, source-scanner coverage for `sefer-segment`). The P2.5 (#187) close-out commits and landing SHA are outside this pinned antecedent range.

The table uses Git's full reachability range, not `--first-parent`, so it includes all side commits: 111 rows, 38 merges and 73 non-merges. The category is the historical subject prefix (scope included), or `merge` when `%P` has multiple parents. The first-parent integration PR identifies how each row entered main; `adjacent` marks CI, verification and selection work related to the split but not a crate extraction; `incidental` marks other concurrent work. In particular, CI timing/Miri rebalance, the P0.2 verdict, and P1.5 selection were not P2 extractions.

```sh
git log --reverse --date-order --format='%H|%P|%s' a966aa8f5e416f2f0f1902bcdb3e1cf7a9ead261..bf6364788445fe54ccde9781590f8c682c015303
git log --first-parent --reverse --format='%H|%P|%s' a966aa8f5e416f2f0f1902bcdb3e1cf7a9ead261..bf6364788445fe54ccde9781590f8c682c015303
```

Each integration PR's member rows are the output of the same full-range log command with that merge's first parent and the merge itself as bounds.

| # | Commit | Category | Integration | Subject |
|---:|---|---|---|---|
| 1 | `5565827191ffdad127806daf0a48f4ddb5801115` | `refactor(core)` | #259 P2.0 | refactor(core): remove the P2.0 back-edges a, c and d inside the root crate |
| 2 | `d6f129d0940af3e9a48febfbf3694d16133e8089` | `build(workspace)` | #259 P2.0 | build(workspace): derived data and the EV-1 reverify request for the P2.0 back-edge removal |
| 3 | `e66744f9515da72f3181ded702ab115378ec6687` | `bench(ci)` | #260 incidental | bench(ci): measure where the long CI jobs spend their time |
| 4 | `b5e2976fba3962c1a791f689eea61d7a9ddfa960` | `merge` | #260 incidental | Merge remote-tracking branch 'origin/main' into ci-long-jobs-20261116 |
| 5 | `7c974f70b5ab4f695a505d42efba57a448b5d23f` | `build(workspace)` | #260 incidental | build(workspace): regenerate the test classification after merging main |
| 6 | `e3d22f13db0db620f904055b0947396ed4adfc59` | `build(ci)` | #260 incidental | build(ci): run the inventory and job-duration Node tests once in docs-consistency |
| 7 | `da4b7325488e6100c3c8e0c75ed698f2df94a5cf` | `build(ci)` | #259 P2.0 | build(ci): judge a declared intra-crate layer restructuring by probe Ir; fix the P2.0 doc inventories and the hook loom oracle |
| 8 | `6a1b4edbf1e5f1a917a77cc6bd045019fda6d78a` | `refactor(platform)` | #259 P2.0 | refactor(platform): move the process-exit giveup state to sefer-platform (P2.0b) |
| 9 | `f54db09fa6a09822bddc3fb64c80469809f5798d` | `build(workspace)` | #259 P2.0 | build(workspace): regenerate the test classification after P2.0b |
| 10 | `19cbc5f8f93f59de4aa63f519b6005ff5f37ee00` | `merge` | #260 incidental | Merge pull request #260 from PHPCraftdream/ci-long-jobs-20261116 |
| 11 | `131c65968b909c034fbd75246c79907cfb0de05a` | `build(ci)` | #261 incidental | build(ci): shard the three long Miri jobs by test target without changing their coverage |
| 12 | `da8126b7e8ea687ef4743b525eea68c1911b2e9c` | `build(workspace)` | #261 incidental | build(workspace): regenerate the test classification after the Miri sharding |
| 13 | `a06731cf56845ef8b441b7e00c8d820233f822c9` | `build(ci)` | #261 incidental | build(ci): keep the magazine-oracles Miri command a block scalar |
| 14 | `6b83a476ac95421c3fb902a90461b5e4358c597e` | `test(pin-cache)` | #259 P2.0 | test(pin-cache): the base helper rejects fallback-owned bases, so the thread-exit refs oracle sees a cached pin |
| 15 | `2a4e0550a10d3806cdd31cafb507615f8c275527` | `build(workspace)` | #259 P2.0 | build(workspace): regenerate the test classification after the pin-cache helper fix |
| 16 | `6f5ad05dedf8455efa90510ecfea2e52c37dbf8b` | `merge` | #259 P2.0 | Merge remote-tracking branch 'origin/main' into p20-backedges-20261116 |
| 17 | `6c07db472897054ed5525b1310aeada8ed1b198f` | `build(workspace)` | #259 P2.0 | build(workspace): regenerate the test classification after merging main |
| 18 | `cd316a3b751f8e39b54d49560f8d060daf595848` | `merge` | #261 incidental | Merge pull request #261 from PHPCraftdream/miri-shard-20261116 |
| 19 | `813748edf36a9674335d9ae309ae8fd8db428d73` | `build(ci)` | #262 incidental | build(ci): rebalance the fastbin and plain Miri shards by measured target times |
| 20 | `e3230ae77c1d44bc395319e3d37ad0b38358cc58` | `build(workspace)` | #262 incidental | build(workspace): regenerate the test classification after the Miri rebalance |
| 21 | `9e5fbe08de2c2fe1182aa35fcaabc01752eb22b3` | `ci(evidence)` | #259 P2.0 | ci(evidence): refresh the 13 rows whose sources P2.0 moved; the temporary reverify config is removed |
| 22 | `d7a16d32963585cd6116bbc086f1cd8ab9e1da4a` | `merge` | #259 P2.0 | Merge remote-tracking branch 'origin/main' into p20-backedges-20261116 |
| 23 | `bf3fe077f48d49040ddbffc1a515d7e0dfb53d42` | `build(workspace)` | #259 P2.0 | build(workspace): regenerate the test classification after merging main |
| 24 | `dcccdcf3c7e41ea524c5ab26298be132f27d3872` | `merge` | #262 incidental | Merge pull request #262 from PHPCraftdream/miri-rebal-20261117 |
| 25 | `5c648b2a5ccbe76c983ea84beeb8c58dfbd4f0b0` | `merge` | #259 P2.0 | Merge remote-tracking branch 'origin/main' into p20-backedges-20261116 |
| 26 | `03b6eac56a1bf58e87df17ed67ecbfe385503f10` | `build(workspace)` | #259 P2.0 | build(workspace): regenerate the test classification after merging main |
| 27 | `02997aec0e7a1dd79e48074215711176ee293230` | `docs(plan)` | #263 incidental | docs(plan): revision 4 of the restructure plan, owner-goal metric and three CI open items |
| 28 | `1a684c38b9a9557a25e59cec5e150770b9b6ba67` | `merge` | #259 P2.0 | Merge pull request #259 from PHPCraftdream/p20-backedges-20261116 |
| 29 | `aa0bce3f304062a686c12a624e3bfc2b63d07045` | `merge` | #263 incidental | Merge remote-tracking branch 'origin/main' into plan-rev4-20261117 |
| 30 | `6de31a0b152829130dda2b623a8326f528f1cc3c` | `docs(correctness)` | #263 incidental | docs(correctness): add the missing lookup row for item 280 |
| 31 | `3ab6f71ba8f2a3bce30af954ebf7123104f59204` | `merge` | #263 incidental | Merge pull request #263 from PHPCraftdream/plan-rev4-20261117 |
| 32 | `58cadda28892941e8b63b8c2c96881cf2d0350c2` | `build(ci)` | #264 adjacent | build(ci): conflict-free derived files and cargo-free doc inventories |
| 33 | `f73d431ad1e783397128286d0ca1c4361fc19f7a` | `merge` | #264 adjacent | Merge remote-tracking branch 'origin/main' into hygiene-p21-20261117 |
| 34 | `79e0e5d484ab4442252394a88fba57a3612956c3` | `build(workspace)` | #264 adjacent | build(workspace): regenerate the test classification after merging main |
| 35 | `e02aec5a77cd8f96d9239427886c08494e16c8bc` | `merge` | #264 adjacent | Merge pull request #264 from PHPCraftdream/hygiene-p21-20261117 |
| 36 | `077db9c95231b91ed9d19e7c774596c35aafaf4b` | `build(evidence)` | #265 adjacent | build(evidence): script that applies an r20-gates EV-1 reverify output to the registry |
| 37 | `6239cdde5903bfdbcbab66267ba7b4ebf3742b18` | `build(workspace)` | #265 adjacent | build(workspace): regenerate the test classification after merging main |
| 38 | `e3e49dfaadc90a97f271c7ed27c5a03ace58aa78` | `merge` | #265 adjacent | Merge pull request #265 from PHPCraftdream/ev1-apply-20261117 |
| 39 | `a0714866eb6535869749f1bdf89ef077f6c73da6` | `build(ci)` | #266 adjacent | build(ci): a Node test, bench or example change no longer selects every Rust test |
| 40 | `22c3dab3e573990e44f3747331e61d72a06853e5` | `merge` | #266 adjacent | Merge remote-tracking branch 'origin/main' into p15a-node-select-20261117 |
| 41 | `3115d89e3be9221493e2306b352de56f3081a692` | `build(workspace)` | #266 adjacent | build(workspace): regenerate the test classification after merging main |
| 42 | `c708ac9623bd66aa8f3f3f538942820505964556` | `merge` | #266 adjacent | Merge pull request #266 from PHPCraftdream/p15a-node-select-20261117 |
| 43 | `17494141477d58b7605c69aed3aa96e95937661b` | `build(workspace)` | #267 adjacent | build(workspace): per-file test classification report with no aggregate lines |
| 44 | `94a14d62dacc6a36da6c69ae4ae095f8af3b9c81` | `merge` | #267 adjacent | Merge remote-tracking branch 'origin/main' into classify-counts-20261117 |
| 45 | `136d50964cf847bcfded6eed150f3813ea0a97db` | `build(workspace)` | #267 adjacent | build(workspace): regenerate the test classification after merging main |
| 46 | `df5620503df91193d13a0c25a52dfc19839a335e` | `fix(ci)` | #267 adjacent | fix(ci): classify-tests --check compares line endings normalized |
| 47 | `0c1029d740c266caaf96a8dcad066e89a607bfcc` | `merge` | #267 adjacent | Merge pull request #267 from PHPCraftdream/classify-counts-20261117 |
| 48 | `68d5ba0f4d1246b0e77bb3eaa66dd331aa473be3` | `build(ci)` | #268 adjacent | build(ci): job and test selection by the workspace crate graph |
| 49 | `8db40b1bbb50d4f7794d1860bab4ae0cd04f1023` | `merge` | #268 adjacent | Merge remote-tracking branch 'origin/main' into p15b-crate-graph-20261117 |
| 50 | `449a85626f06e612f2bd4b40b22754cbea66fda1` | `build(workspace)` | #268 adjacent | build(workspace): regenerate the test classification after merging main |
| 51 | `a28be5dace7d9517f3767fa37ee15a99a06d0dde` | `merge` | #268 adjacent | Merge pull request #268 from PHPCraftdream/p15b-crate-graph-20261117 |
| 52 | `816a28f54c8d20bd2029510b362cbbf35dc37ab0` | `docs(perf)` | #269 incidental | docs(perf): P0.2 verdict is NO-GO - thread-sanitizer grew 27 % after the test merge |
| 53 | `66294fa74b5bf346844cc608d3b9a66c741ee0db` | `merge` | #269 incidental | Merge pull request #269 from PHPCraftdream/p02-verdict-20261117 |
| 54 | `e7d7647124b444984db7d58520366e6750a4b2f1` | `build(ci)` | #272 adjacent | build(ci): measurable safety criterion for TEST_SELECTION=enforce (P1.5c) |
| 55 | `4a84153eeb446e975df4371366a7164d14a3dd25` | `build(workspace)` | #272 adjacent | build(workspace): regenerate the test classification |
| 56 | `2282acac254862fd5361ac51c8fb5b3a3fc214a4` | `refactor(core)` | #274 P2.1 | refactor(core): extract segment and route into the sefer-segment crate (P2.1) |
| 57 | `f533581020e8f09e0b9581dd8bf5e13a578bfb44` | `build(evidence)` | #274 P2.1 | build(evidence): EV-1 request and recipes for the rows touched by the P2.1 move |
| 58 | `66911c4547e2e9eebfa0226d774f1dbef7de0626` | `build(workspace)` | #274 P2.1 | build(workspace): regenerate the test classification |
| 59 | `29fbbda1118c7842a1b15a6269c3d9246eed94c8` | `merge` | #272 adjacent | Merge pull request #272 from PHPCraftdream/p15c-enforce-criterion-20261118 |
| 60 | `dda602e29472fb54fb308822c55555e6e4693f89` | `merge` | #274 P2.1 | Merge remote-tracking branch 'origin/main' into p21-sefer-segment-c-20261118 |
| 61 | `be789c5a28dfaabe7de197bdce43b96a2a5b420b` | `fix(ci)` | #274 P2.1 | fix(ci): clear the first full CI findings of the P2.1 move |
| 62 | `b2cbf2c926339f2d81bb2ea3983f69c8f0288e2c` | `build(workspace)` | #274 P2.1 | build(workspace): regenerate the test classification |
| 63 | `7ef8ddbf04dd91652bca8af7f7f45fbee3e6199d` | `docs` | #274 P2.1 | docs: tier-2 unsafe allow counters after dropping the duplicate attribute |
| 64 | `a08afb30a99952c2c01bde9b0c0b1884a2da9c05` | `refactor(core)` | #274 P2.1 | refactor(core): keep the P2.1 carve-path calls inlinable across the crate boundary |
| 65 | `7f5f972d3ab8455e172e66e4498db279f24e78aa` | `ci(evidence)` | #274 P2.1 | ci(evidence): reverify proposals pin mutant_rejected only for a killed mutant on a PASS row |
| 66 | `77aaf32ce0dadb32edb648717742034384a4059f` | `ci(evidence)` | #274 P2.1 | ci(evidence): refresh the 33 rows whose sources P2.1 moved; the temporary reverify config is removed |
| 67 | `b99b57c9f633272551ea288550f81d41ba8198d4` | `merge` | #274 P2.1 | Merge pull request #274 from PHPCraftdream/p21-sefer-segment-d-20261118 |
| 68 | `a1be2fe2bb7d5cd881866c6f9909b8b983c1506e` | `ci(workflow)` | #275 adjacent | ci(workflow): extract miri guard and hugetlb steps into composites; gate sefer-segment by crate flag |
| 69 | `30d3a5d346f6fcc88468b6ea2f111c81a202d5ba` | `merge` | #275 adjacent | Merge pull request #275 from PHPCraftdream/p201-ci-room-20261118 |
| 70 | `55416030fa7e6dcdbba64f84f044f0038e729438` | `ci(workflow)` | #276 incidental | ci(workflow): shard check-matrix into 3 row groups with a coverage proof |
| 71 | `0ba25b9776e12dfd62f2a9e5662e2ab4e9b8c225` | `ci(evidence)` | #276 incidental | ci(evidence): drop tests and examples from the source scope of the six full-run rows |
| 72 | `8e7efc9fc82e18ea7afa5e20a133b73d36d6af05` | `test(miri)` | #277 incidental | test(miri): zero the segment state array with one memset under Miri |
| 73 | `ec691c64f77351b3727f23d53e3fe711436e8b2c` | `build(evidence)` | #277 incidental | build(evidence): EV-1 request for the seven rows whose scope the state-array change touches |
| 74 | `6084a833366a34b87bc860785199c65487d7b4a5` | `merge` | #276 incidental | Merge pull request #276 from PHPCraftdream/p196a-check-matrix-shards-20261118 |
| 75 | `35d64a3685af4c7ccc749cdc91140f4da33e3100` | `ci(evidence)` | #277 incidental | ci(evidence): refresh the seven rows whose scope the Miri state-array change touched; the temporary reverify config is removed |
| 76 | `037095d6623feb2dc163df43dcde0a338e0b4a9d` | `merge` | #277 incidental | Merge remote-tracking branch 'origin/main' into p196c-miri-state-init-20261118 |
| 77 | `12ea1d57223fcb2c8408190f097487fb5fb85286` | `ci(workflow)` | #278 incidental | ci(workflow): run the four aarch64 cross test suites as parallel matrix jobs |
| 78 | `6d6cc273f8deaf0fe7946f6498e1a93487555026` | `refactor(core)` | #279 P2.2 | refactor(core): extract small, large and the AllocCore hub into the sefer-alloc-core crate (P2.2) |
| 79 | `97ead6ac26d698900d8dfeda847263bfef3bd51a` | `merge` | #279 P2.2 | Merge remote-tracking branch 'origin/main' into p22-alloc-core-20261118 |
| 80 | `c7785de7194b5a98db4e36e311806d85b215835d` | `build(evidence)` | #279 P2.2 | build(evidence): EV-1 request for the registry rows whose sources the P2.2 move touched |
| 81 | `caa7745913fd7330493bf10c40f6a024cc6e6162` | `merge` | #277 incidental | Merge pull request #277 from PHPCraftdream/p196c-miri-state-init-20261118 |
| 82 | `c9229eb2df556af6542c4b1c53291a39fbd1e625` | `merge` | #278 incidental | Merge pull request #278 from PHPCraftdream/p196b-aarch64-matrix-20261118 |
| 83 | `24c2acc87968905759fcb07279f8bbf779a2c618` | `fix(ci)` | #279 P2.2 | fix(ci): clear the first full CI findings of the P2.2 move |
| 84 | `0b1c78da12aebe40a12e1af894f0ea7d03fe6672` | `refactor(core)` | #279 P2.2 | refactor(core): keep the P2.2 hot-path calls inlinable across the crate boundary |
| 85 | `40cdf43ea0eef8cefacdc513fa8e189ea9bd4aef` | `merge` | #279 P2.2 | Merge remote-tracking branch 'origin/main' into p22-alloc-core-20261118 |
| 86 | `6b528cab6386646714488adf683a61a00dfef444` | `fix(ci)` | #279 P2.2 | fix(ci): scan the sefer-alloc-core tree in the source-size and stale-reference tests |
| 87 | `007a5eba8848ef040175f0ed5c0f81dbc72499c4` | `build(workspace)` | #279 P2.2 | build(workspace): regenerate the test classification after the source-scan test fixes |
| 88 | `ed4b9fc92573d0e177b1c6b8a718e68aebbc3266` | `ci(evidence)` | #279 P2.2 | ci(evidence): refresh the 25 rows whose sources P2.2 moved; the temporary reverify config is removed |
| 89 | `6b8a7ea3dd5a5db6f5cedfa89bcfa85a6ba9c243` | `fix(ci)` | #279 P2.2 | fix(ci): keep the witness tests of recorded evidence rows byte-identical |
| 90 | `7eb12eee790121c109310743c0a33851698f989d` | `merge` | #279 P2.2 | Merge pull request #279 from PHPCraftdream/p22-alloc-core-20261118 |
| 91 | `429d764b24616aeaba0b15b77aff6a8056e2c434` | `refactor(core)` | #280 P2.3 | refactor(core): extract the heap core, registry, bootstrap and xthread into the sefer-registry crate (P2.3) |
| 92 | `6f0012da439c791528d9aa22857322c7f8d18864` | `build(evidence)` | #280 P2.3 | build(evidence): EV-1 request for the registry rows whose sources the P2.3 move touched |
| 93 | `995b1da66e1afa9d9c9910169d8af979bf5a75d4` | `build(workspace)` | #280 P2.3 | build(workspace): regenerate the test classification after the P2.3 move |
| 94 | `c089abc8162b0696e2267b4d0205b449263f9e44` | `refactor(core)` | #280 P2.3 | refactor(core): keep the P2.3 registry hot-path calls inlinable across the crate boundary |
| 95 | `8cdc8a628ddba7671d25ea5c3337b2c1fe07cf4c` | `ci(evidence)` | #280 P2.3 | ci(evidence): refresh the 44 rows whose sources P2.3 moved; the temporary reverify config is removed |
| 96 | `fd7c675a409a47c87b14c696f562de235c867434` | `merge` | #280 P2.3 | Merge pull request #280 from PHPCraftdream/p23-registry-20261119 |
| 97 | `bf1344f6d5b8341687f3d2e7e1e3b74f16d325a2` | `fix(scripts)` | #281 classifier | fix(scripts): exclude Git-ignored paths from test classification |
| 98 | `cd2ce53b6de52fddfe81bd5b88973a46fc3c7246` | `bench(perf)` | #282 probe | bench(perf): stabilize the ghost-zero-cost probe |
| 99 | `99a4e474768e0397c4473c90dbd9fc953a86f175` | `build(workspace)` | #282 probe | build(workspace): regenerate the test classification for the probe report test |
| 100 | `819d5ee10bea9fd80098783007dcd9a0e3165490` | `merge` | #281 classifier | Merge pull request #281 from PHPCraftdream/p208-classify-ignored-20261121 |
| 101 | `a0134d0a90dbfe75f7b50ffe7b12af3a290bb7ac` | `merge` | #282 probe | Merge remote-tracking branch 'origin/main' into p209b-probe-20261121 |
| 102 | `041cd2e51a01a5ab84c4a19160968ce04211efd0` | `refactor(core)` | #283 P2.4 | refactor(core): move the Kani proof harnesses into the crates that own their internals (P2.4) |
| 103 | `edbabb9773516a8b7eeed2e5ac5d94cffeae1f11` | `build(evidence)` | #283 P2.4 | build(evidence): EV-1 request for the rows whose sources the P2.4 move touched |
| 104 | `8b1cd0b37da32b83ee5f40999c20d5bb5646bfff` | `merge` | #282 probe | Merge pull request #282 from PHPCraftdream/p209b-probe-20261121 |
| 105 | `8fadf8d72c51727d57636641a5db6c352cce9c38` | `merge` | #283 P2.4 | Merge remote-tracking branch 'origin/main' into p24-facade-20261121 |
| 106 | `37f5a6c8e3ef2f936370127fd549d1230ff94e5b` | `build(workspace)` | #283 P2.4 | build(workspace): regenerate the test classification after the P2.4 move |
| 107 | `668c03612a9fcc042327a3828929082c1126982f` | `ci(evidence)` | #283 P2.4 | ci(evidence): refresh the 8 rows whose sources P2.4 moved; the temporary reverify config is removed |
| 108 | `2246bfb1f147dfc4d181a5fd0ca967ad4cbbdba0` | `merge` | #283 P2.4 | Merge pull request #283 from PHPCraftdream/p24-facade-20261121 |
| 109 | `8514adfa8f6ee5c7158d5a1e3498cb3a0a02e5c8` | `test(scan)` | #284 adjacent | test(scan): cover sefer-segment/src in the source-scanner tests |
| 110 | `486fd72871a250d03282bbb86839ace7cc889cf5` | `build(workspace)` | #284 adjacent | build(workspace): regenerate the test classification after the scanner root change |
| 111 | `bf6364788445fe54ccde9781590f8c682c015303` | `merge` | #284 adjacent | Merge pull request #284 from PHPCraftdream/p210-scan-segment-20261121 |

## Production features and facade surface

Both endpoint root manifests (`git show <endpoint>:Cargo.toml`) have `default = ["std"]` and the same explicit `production` list, in order: `alloc-global`, `alloc-xthread`, `alloc-decommit`, `fastbin`, `alloc-segment-directory`, `primordial-lazy-commit`. No root feature was added to that list. **The resolved build graph changed:** at the base, those features target the in-root implementations and already forward applicable settings to `sefer-policy`, `sefer-platform` and `aligned-vmem`. At the head, `alloc-core` activates `sefer-segment` and `sefer-alloc-core`, and `alloc-global` also activates `sefer-registry`. The facade forwards `alloc-core`, `alloc-global`, `alloc-xthread`, `alloc-decommit`, `fastbin`, `alloc-segment-directory` and `primordial-lazy-commit` to the three via `?/` edges (which configure an enabled optional dependency rather than activating it). The new crates also have internal forwarding edges: registry to alloc-core, alloc-core to segment, and applicable policy/platform/VM edges. Non-bundle settings are forwarded where supported, e.g. `internals` and `bench-internals` to all three, `page-map-diag` to segment and alloc-core, and `batch-api` to alloc-core and registry. This is a manifest-level comparison, not a claim that all cfg combinations or generated code are identical.

For the public surface, the baseline is the actual parent of P2.1 (`2282acac`): `66294fa74b5bf346844cc608d3b9a66c741ee0db`, not the pre-P2.0 base. Source comparison:

```sh
git diff 66294fa74b5bf346844cc608d3b9a66c741ee0db bf6364788445fe54ccde9781590f8c682c015303 -- src/lib.rs src/alloc_core/mod.rs src/registry/mod.rs src/global/mod.rs
git diff a966aa8f bf6364788445fe54ccde9781590f8c682c015303 -- Cargo.toml ':(glob)crates/core/*/Cargo.toml'
```

`src/lib.rs` retains the literal crate-root public declarations and their gates: the region exports (including std-gated `SyncRegion`), experimental/pinning exports, `AllocCore`/`SegmentLayout`/`InvalidAlign`, decommit policy/config exports and alloc-global exports (`SeferAlloc`, `AllocStats`, `MaintenanceStartError`), plus the cfg-gated module and ghost declarations. The removed Kani module declaration is private. Below the crate root, the reviewed `alloc_core` and `registry` module-level exports now use forwarding rather than local definitions. For example, the `alloc_core` config submodules `large_cache_config`, `large_cache_mode` and `small_segment_pool_config` explicitly require `internals` in addition to `alloc-decommit` at head; the public root path into `alloc_core` already required `internals` at base. `global::dbg_exit_lock_giveups` now forwards from `sefer-platform` rather than the old fallback path. This verifies declarations and selected forwarding/gates only, not recursively reachable items, methods, trait impls, auto traits, type identity or all feature/target combinations. No blanket public-API equivalence claim.

## Performance and raw logs

**Runtime improvements this round: 0.** No `perf(runtime)` or `perf(opt-in)` commits in this range. P2.1–P2.3's cross-crate inline hints are extraction/gate work, not measured allocator speedups. The historical probe study in [GHOST_ZERO_COST_PROBE_STABILITY_20261121.md](../GHOST_ZERO_COST_PROBE_STABILITY_20261121.md) (`cd2ce53b`, `bench(perf)`) reports the chosen harness at −4,383, −2,133 and −2,543 Ir on the historical extraction pairs and 0 Ir on the self-pair. These are total probe counts, not isolated allocator results or measurements of the full manifest endpoints. No endpoint wall-clock, Ir or RSS delta was measured for this close-out.

Net new raw logs: **9 files, 1,049,566 bytes** (perf: **1 file, 2,514 bytes**; EV-1 evidence: **8 files, 1,047,052 bytes**). Including refreshed logs: **55 files, 2,866,029 head-blob bytes** (46 modified EV-1 logs account for the remaining 1,816,463 bytes). This counts distinct `_raw*.log` paths added or modified between the two Git trees, summing exact head blob lengths rather than CRLF worktree sizes or cumulative historical versions. Git reports no deleted matching paths in the range. Reproduce from the pinned trees:

```sh
node <<'NODE'
const git = (...args) => require('node:child_process').execFileSync('git', args);
const base = 'a966aa8f5e416f2f0f1902bcdb3e1cf7a9ead261';
const head = 'bf6364788445fe54ccde9781590f8c682c015303';
const tokens = git('diff', '--no-renames', '--name-status', '-z', base, head,
  '--', ':(glob)**/_raw*.log').toString().split('\0');
const groups = new Map(), total = [0, 0];
let deleted = 0;
for (let i = 0; i < tokens.length - 1; i += 2) {
  const [status, path] = tokens.slice(i, i + 2);
  if (status === 'D') {
    deleted++;
  } else {
    if (status !== 'A' && status !== 'M') throw new Error(status + ' ' + path);
    const key = status + ' ' + path.split('/').slice(0, 2).join('/') + '/';
    const size = Number(git('cat-file', '-s', head + ':' + path));
    const entry = groups.get(key) ?? [0, 0];
    entry[0]++; entry[1] += size;
    groups.set(key, entry);
    total[0]++; total[1] += size;
  }
}
console.log('status scope count bytes');
for (const [key, entry] of [...groups].sort()) console.log([key, ...entry].join(' '));
console.log(['total', ...total].join(' '));
console.log('deleted ' + deleted);
NODE
```

```text
status scope count bytes
A docs/evidence/ 8 1047052
A docs/perf/ 1 2514
M docs/evidence/ 46 1816463
total 55 2866029
deleted 0
```

## Item verdicts

`SHIPPED` below means merged into the pinned main tree, not published to crates.io. `INFRASTRUCTURE` means no runtime performance GO.

| Item | Verdict | Evidence / limit |
|---|---|---|
| P2.0a/c/d | INFRASTRUCTURE — SHIPPED | #259: move segment routing and route-pin responsibilities down; use a facade-installed fallback hook. |
| P2.0b | INFRASTRUCTURE — SHIPPED | #259: process-exit state and lock-giveup recording moved to `sefer-platform`. |
| P2.1 | INFRASTRUCTURE — SHIPPED | #274: `sefer-segment` owns segment/route, together in one crate. |
| P2.2 | INFRASTRUCTURE — SHIPPED | #279: `sefer-alloc-core` owns Small/Large, `AllocCore` and diagnostics. |
| P2.3 | INFRASTRUCTURE — SHIPPED | #280: `sefer-registry` owns heap core, registry, bootstrap and xthread. |
| P2.4 | INFRASTRUCTURE — SHIPPED | #283: root keeps global integration and facades. CI routes 12 proofs to `sefer-segment` and 8 to `sefer-registry`; existing `sefer-concurrent` proof job step remains. |
| CI permission/gating support | INFRASTRUCTURE — SHIPPED | #275 factors workflow setup into composites and gates segment jobs by crate flag. #266/#268/#272 supply adjacent P1.5 selection, crate-graph and enforcement-criterion tooling, not evidence that enforcement is enabled. |
| Classifier ignored paths | INFRASTRUCTURE — SHIPPED | #281: tracked and unignored untracked files from `git ls-files`; ignored artifacts contribute no rows. |
| Ghost probe stability | INFRASTRUCTURE — SHIPPED | #282: same workload, isolated phases; no tolerance or policy change. |
| P2.5 (#187) | INFRASTRUCTURE — CLOSE-OUT PR | The close-out ships in a PR with `CI: full`. It adds workspace-member all-features rustdoc and declared docs.rs rows, extends the weekly depth-2 feature powerset to remaining members with features (featureless members are excluded), and refreshes the README unsafe ownership inventory, CHANGELOG and manifest. Acceptance requires green full CI on the exact landing SHA (`node scripts/verify-sprint-close.mjs <sha>`) and a green scheduled or dispatched feature-powerset run. Neither result is claimed here. |

P2 numbering is from the [workspace plan](../../plans/2026-11-15-workspace-restructure-plan.md); #187 is the close-out task in this handoff, not a perf ledger ID. [OPEN_ITEMS](../OPEN_ITEMS.md) lists adjacent CI/perf items 155–157; this close-out neither renumbers nor disposes of them.

## Evidence and verification boundary

The strict workspace rustdoc row flagged a broken self intra-doc link in the first line of the module doc of
`crates/core/sefer-alloc-core/src/alloc_core/platform/sidecar.rs`; it is now plain code (`` `sidecar` ``). This one-line
doc-comment change is the only Rust edit in the PR: it does not change allocator logic, ordering sites, ghost
tokens/rules or proof harnesses. Under `CLAUDE.md` and the ghost plan, ordinary
production ranges require exact normalized `.text` and Ir identity; only qualifying extraction or layer ranges may use
the symmetric 0.1% Ir bound and head-only production ghost-token check. Probe qualification is not a substitute for the
mandatory gate; no waiver or local gate execution is claimed.

Historical EV-1 reverify request in this PR was for eight rows: R4a-03, R4b-01, R6a-A10, R6a-E1, R6a-E2, R6a-E3,
R6a-EX2 and R6a-EX4 (six rows cover the crate directory, R6a-EX2 covers the source directory and R6a-EX4 covers
`alloc_core/platform` — path scopes distinct from the documentation paths touched here). Receipts were to be reported
under `ci(evidence)`; no r20 status or outcome claims. That historical sidecar request is distinct from the current task,
which separately requests the affected manifest rows via `.github/evidence-reverify.json`. For #288, commit `35f531d3`
landed the eight (a) source-scope receipts and the distinct R0-22 witness receipt (source_sha `75786d64...`, refreshed
receipt hash; the commit body identifies it). The only EV-1 receipts requested for this change are the seven (b) manifest
rows: R4a-03, R4b-01, R6a-E1, R6a-A10, R6a-E2, R6a-E3 and R6a-EX4; close or disposition item 288 after those receipts.
All original historical evidence, table/range/raw statistics in this manifest are preserved
unchanged.

Local checks: `node scripts/ci-plan.mjs --selftest` (110 checks, PASS), `node scripts/verify-structure-limits.mjs` (21 self-test cases; 4 grandfathered directories, 28 files, none new or grown), and `node --test tests/ghost_extraction_policy.test.mjs tests/ghost_probe_stability_report.test.mjs tests/classify_ignored_paths.test.mjs tests/crate_graph.test.mjs` (35 passed, 0 failed). No results from Rust runtime tests, live Kani/Miri/Loom, performance jobs, or full CI are claimed here.
