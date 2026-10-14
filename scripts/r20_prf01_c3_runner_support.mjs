export function measureToInput(runs, producerRuns, {
  ctx, C3, shaFile, REPO, sourceOf, ORACLE_CFG, PRODUCER_SPECS, local, putJson, putText,
  assert, fs, join,
}) {
  const hashes = C3.FROZEN_FILES.map(path => ({
    path,
    sha256: shaFile(join(REPO, path)),
  }));
  const environmentSha256 = ctx.builds.environmentSha256;
  const identities = {};

  for (const arm of ['A', 'B']) {
    identities[arm] = {
      label: arm,
      measurementCommit: ctx.manifest.arches[arm].head,
      source: sourceOf(ctx.manifest.arches[arm]),
      environmentSha256,
      binaries: Object.fromEntries(
        Object.entries(ctx.builds.arms[arm]).map(([key, value]) => [
          key,
          value.binarySha256,
        ]),
      ),
      buildStartedAt: ctx.builds.buildStartedAt,
      buildReceipt: 'builds.json',
      features: C3.FEATURES,
      primaryCfg: [],
      oracleCfg: [ORACLE_CFG],
      toolchain: {
        rustc: 'environment.log',
        target: 'environment.log',
        profile: 'release',
        flags: 'role cfg only',
      },
    };
  }

  identities.C0 = {
    source: identities.A.source,
    binarySha256: ctx.builds.arms.C.iai.binarySha256,
    controlsBinarySha256: ctx.builds.arms.C.controls.binarySha256,
    c3BinarySha256: ctx.builds.arms.C.c3.binarySha256,
    trimBinarySha256: ctx.builds.arms.C.trim.binarySha256,
    c3ControlBinarySha256: ctx.builds.arms.C.c3Control.binarySha256,
    controlBinarySha256: ctx.builds.arms.C.control.binarySha256,
    buildStartedAt: ctx.builds.buildStartedAt,
    buildReceipt: 'builds.json',
  };
  for (const [key, value] of Object.entries({
    iai: ctx.builds.arms.C.iai.binarySha256,
    controls: ctx.builds.arms.C.controls.binarySha256,
    c3: ctx.builds.arms.C.c3.binarySha256,
    trim: ctx.builds.arms.C.trim.binarySha256,
    c3Control: ctx.builds.arms.C.c3Control.binarySha256,
    control: ctx.builds.arms.C.control.binarySha256,
  })) {
    identities.C0[`${key}BinarySha256`] = value;
  }
  identities.C0.source = sourceOf(ctx.manifest.arches.A);

  const runnerVersionMatch = /(\d+[.]\d+[.]\d+)/.exec(
    fs.readFileSync(local('environment.log'), 'utf8')
      .split('\n')
      .find(line => line.includes('iai-callgrind-runner')) ?? '',
  );
  assert.ok(runnerVersionMatch, 'runner version absent from environment');
  const runnerVersion = runnerVersionMatch[1];
  const input = {
    version: 1,
    hashes,
    environmentSha256,
    ...(ctx.rehearsal ? { rehearsal: true } : {}),
    ...(ctx.rehearsal ? { rehearsal: true } : {}),
    inventoryReceipt: 'manifest.json',
    identityReviewReceipt: 'manifest.json; per-stage identity revalidation',
    environmentReceipt: 'environment.log',
    oracleReceipt: 'oracle.json',
    entry: 'SeferAlloc::GlobalAlloc->HeapCore routed',
    nativeDeferred: {
      deferred: true,
      method: 'fixed-batch native paired A/B on a quiet bare-metal host; '
        + 'per-process batch times divided by batch size (deferred; not launched)',
    },
    inventory: [...C3.INVENTORY],
    identities,
    iai: [],
    sizeOf: { A: null, B: null, receipt: 'oracle.json' },
    ancestry: {
      result: 'A ancestor B',
      chain: [ctx.manifest.arches.A.head, ctx.manifest.arches.B.head],
      receipt: 'manifest.json',
    },
  };
  let runId = 0;

  for (const run of runs) {
    const source = sourceOf(ctx.manifest.arches[run.arm]);
    for (const batch of run.cells) {
      for (const name of batch.cells) {
        const oracleRec = ctx.oracle.records[run.label]?.[name];
        const raw = oracleRec?.row?.oracle ?? Object.fromEntries(
          C3.ALL_COUNTERS.map(key => [key, null]),
        );
        const binarySha256 = ctx.builds.arms[run.arm][batch.key].binarySha256;
        const config = oracleRec?.row?.config ?? {
          requested: null,
          resolved: null,
          configConflictsDelta: null,
          processBoundary: 'isolated-process',
          runId: `${run.label}-${name}-${runId}`,
          pid: batch.pids[name],
          notApplicableReason: 'control cell has no SeferAlloc config oracle',
        };
        input.iai.push({
          name,
          arm: run.label,
          Ir: batch.ir[name],
          environmentSha256,
          binarySha256,
          source,
          startedAt: batch.startedAt,
          features: C3.FEATURES,
          receipt: batch.log.path,
          config: {
            ...config,
            pid: batch.pids[name],
            runId: `${run.label}-${name}-${++runId}`,
          },
          oracle: {
            ...raw,
            unobservedReasons: Object.fromEntries(
              Object.keys(raw)
                .filter(key => raw[key] === null)
                .map(key => [key, `not observed for ${run.label}/${name}`]),
            ),
            linkage: oracleRec ? {
              binarySha256: oracleRec.binarySha256,
              primaryBinarySha256: binarySha256,
              source: oracleRec.source,
              features: C3.FEATURES,
              cfg: [ORACLE_CFG],
              environmentSha256,
              receipt: `oracle/${run.label}-${name}.json`,
              featureExactReceipt: 'oracle.json',
              startedAt: oracleRec.row.startedAt,
              config: oracleRec.row.config,
            } : null,
          },
        });
      }
    }
  }

  for (const producer of producerRuns) {
    const spec = PRODUCER_SPECS[producer.name];
    const row = input.iai.find(
      item => item.name === producer.name && item.arm === producer.label,
    );
    assert.ok(row);
    const primary = producer.primary;
    const control = producer.control;
    assert.equal(primary.totalIr, row.Ir, `producer rerun Ir mismatch @${producer.label}`);
    const workload = primary.childIr - control.childIr;
    assert.ok(workload > 0, `empty producer workload @${producer.label}`);
    row.producerIrReceipt = {
      binarySha256: row.binarySha256,
      source: row.source,
      environmentSha256,
      features: C3.FEATURES,
      cfg: [],
      receipt: primary.log.path,
      symbolReceipt: `boundary/${producer.label}-${producer.name}-primary-nm.log`,
      profileReceipt: `profiles/${producer.label}-${producer.name}-primary-inventory.json`,
      symbol: spec.symbol,
      runnerVersion,
      valgrindVersion: 'environment.log',
      collectAtStart: false,
      defaultToggle: false,
      entryPoint: 'Custom',
      separateThreads: true,
      toggleCollect: spec.symbol,
      generatedArguments: primary.generatedArguments,
      aggregation: 'sum of all thread profile Ir',
      childFreeCount: spec.freeCount,
      childIr: primary.childIr,
      boundaryOverheadIr: control.childIr,
      workloadIr: workload,
      boundaryExecutions: 1,
      totalIr: row.Ir,
      startedAt: primary.startedAt,
      absoluteTolerance: producer.name === C3.LARGE_REMOTE_FREE ? 'T_s' : null,
      control: {
        receipt: control.log.path,
        workload: 'child remote frees removed',
        source: row.source,
        binarySha256: control.binarySha256,
        environmentSha256,
        childIr: control.childIr,
        workloadIr: 0,
        childFreeCount: 0,
        boundaryExecutions: 1,
        symbolReceipt: `boundary/${producer.label}-${producer.name}-control-nm.log`,
        profileReceipt: `profiles/${producer.label}-${producer.name}-control-inventory.json`,
        symbol: spec.symbol,
        toggleCollect: spec.symbol,
        cfg: [spec.cfg],
        features: C3.FEATURES,
        generatedArguments: control.generatedArguments,
        runnerVersion,
        valgrindVersion: 'environment.log',
        collectAtStart: false,
        defaultToggle: false,
        entryPoint: 'Custom',
        separateThreads: true,
        aggregation: 'sum of all thread profile Ir',
        totalIr: control.totalIr,
        deltaIr: primary.totalIr - control.totalIr,
        parentSetupUnchanged: true,
        collectionUnchanged: true,
      },
    };
  }

  const observedSize = arm => {
    const record = ctx.oracle.records[arm]?.[C3.V2_BENEFIT[0]]?.row;
    const value = record?.oracle?.sizeof_small_sidecar ?? record?.sizeof_small_sidecar;
    assert.ok(Number.isSafeInteger(value) && value > 0,
      `oracle must observe positive sidecar size for ${arm}`);
    return value;
  };
  input.sizeOf = { A: observedSize('A1'), B: observedSize('B1'), receipt: 'oracle.json' };
  const receipts = {};
  const scan = value => {
    if (typeof value === 'string'
      && /^(?:environment[.]log|manifest[.]json|builds[.]json|oracle[.]json|(?:logs|boundary|profiles|oracle)\/)/
        .test(value)
      && !(value in receipts)) {
      receipts[value] = shaFile(local(value));
    } else if (Array.isArray(value)) {
      value.forEach(scan);
    } else if (value && typeof value === 'object') {
      Object.values(value).forEach(scan);
    }
  };
  scan(input);
  input.receiptHashes = receipts;
  if (ctx.rehearsal) input.rehearsal = true;
  putJson('input.json', input);
  putText('summary.csv', [
    ...(ctx.rehearsal ? ['# rehearsal: harness validation only; never gate data'] : []),
    'target,arm,name,ir,binarySha256',
    ...input.iai.map(row => `${C3.targetFor(row.name)},${row.arm},${row.name},${row.Ir},${row.binarySha256}`),
  ].join('\n') + '\n');
  return input;
}
