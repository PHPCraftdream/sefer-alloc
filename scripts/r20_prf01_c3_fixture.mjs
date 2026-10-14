export function fixture(frozen, context) {
  const {
    inventory, counters, hotDecl, primary, benefit, producer, mi, PRODUCERS,
    targetFor, arms, DISTINCT_WORD_PRODUCER, LARGE_REMOTE_FREE, TRIM_CELLS,
    SUMMARY_ONLY_COUNTERS,
  } = context;
  const h = c => c.repeat(64);
  const src = c => ({
    baseHead: c.repeat(40),
    patchSha256: h(c),
    untrackedSha256: h(c),
    identitySha256: h(c),
    capturedAt: '2026-10-11T00:00:00Z',
  });
  const p = (c, bs) => ({
    label: c === 'a' ? 'A' : 'B',
    source: src(c),
    measurementCommit: c.repeat(40),
    environmentSha256: h('e'),
    binaries: {
      iai: h(bs[0]), controls: h(bs[3]), oracle: h(bs[1]),
      control: h(bs[2]), c3: h(bs[0]), trim: h(bs[3]),
    },
    buildStartedAt: '2026-10-11T01:00:00Z',
    features: primary,
    primaryCfg: [],
    oracleCfg: ['r19_sidecar_summary_oracle', ...(c === 'b' ? ['r20_prf01_c3_oracle'] : [])],
    buildReceipt: 'synthetic/build',
    toolchain: {rustc: 'synthetic', target: 'synthetic', profile: 'bench', flags: 'none'},
  });
  let id = 0;
  const cfg = (workCount = 1, blockBytes = 16) => {
    const requested = {
      maintenance: false, workCount, segmentCount: 1, blockBytes,
      cachePolicy: 'default',
    };
    return {
      requested, resolved: {...requested}, configConflictsDelta: 0,
      processBoundary: 'isolated-process', runId: `synthetic-${++id}`, pid: id,
    };
  };
  const hotCfg = (name, a) => {
    const [w, b, al] = hotDecl[name];
    const requested = {
      maintenance: false, workCount: w, segmentCount: null,
      blockBytes: b, cachePolicy: 'default',
    };
    return {
      requested, resolved: {...requested}, configConflictsDelta: 0,
      processBoundary: 'isolated-process', runId: `synthetic-${++id}`, pid: id,
      unobservedReasons: {segmentCount: 'measured body has no paired geometry observer'},
      declaredProvenance: {
        receipt: 'synthetic/inventory', cell: name,
        fields: ['workCount', 'blockBytes', 'alignment'], alignment: al,
      },
      resolvedProvenance: {
        runId: 'probe-' + id, pid: 999, arm: a,
        binarySha256: a === 'C0' ? d.identities.C0.oracleBinarySha256 : a === 'B' ? h('6') : h('2'),
        receipt: 'synthetic/probe', claimPath: 'global',
        copiedFields: ['cachePolicy', 'configConflictsDelta', 'maintenance'],
      },
    };
  };
  const d = {
    version: 1,
    hashes: frozen,
    entry: 'SeferAlloc::GlobalAlloc->HeapCore routed',
    environmentSha256: h('e'),
    inventoryReceipt: 'synthetic/inventory',
    identityReviewReceipt: 'synthetic/review',
    environmentReceipt: 'synthetic/env',
    oracleReceipt: 'synthetic/oracle',
    nativeDeferred: {
      deferred: true,
      method: 'batch-timer native paired A/B on a quiet bare-metal host (deferred; not launched in v2)',
    },
    identities: {
      A: p('a', '1234'),
      B: p('b', '5678'),
      C0: {
        source: src('a'), binarySha256: h('7'), controlsBinarySha256: h('9'), oracleBinarySha256: h('d'),
        c3BinarySha256: h('a'), trimBinarySha256: h('b'),
        buildStartedAt: '2026-10-11T01:00:00Z', buildReceipt: 'synthetic/c0',
      },
    },
    ancestry: {
      result: 'A ancestor B', chain: ['a'.repeat(40), 'b'.repeat(40)],
      receipt: 'synthetic/ancestry',
    },
    sizeOf: {A: 41984, B: 41984, receipt: 'synthetic/size'},
    inventory,
    iai: [],
  };
  function row(arm, binary, name, configuration) {
    return {
      name, arm, environmentSha256: d.environmentSha256, binarySha256: binary,
      source: d.identities[arm.startsWith('B') ? 'B' : 'A'].source,
      startedAt: '2026-10-11T02:00:00Z', features: primary,
      receipt: 'synthetic/log', config: configuration,
    };
  }
  const linkage = (r, pin) => ({
    binarySha256: r.arm === 'C0' ? d.identities.C0.oracleBinarySha256 : pin.binaries.oracle,
    primaryBinarySha256: r.binarySha256,
    source: r.source,
    features: primary,
    cfg: ['r19_sidecar_summary_oracle', ...(r.arm.startsWith('B') ? ['r20_prf01_c3_oracle'] : [])],
    environmentSha256: d.environmentSha256,
    receipt: 'synthetic/oracle-log',
    featureExactReceipt: 'synthetic/review',
    startedAt: r.startedAt,
    config: {...r.config, runId: `link-${++id}`},
  });
  for (const name of inventory) {
    for (const arm of arms) {
      const a = arm.startsWith('B') ? 'B' : 'A';
      const isB = a === 'B';
      const pin = d.identities[a];
      const isHot = hotDecl[name] !== undefined;
      const work = isHot ? hotDecl[name][0]
        : name === producer ? 4096
        : name.startsWith('growth16_')
          ? [32768, 65536, 131072][benefit.indexOf(name) - 2]
          : 1;
      const role = targetFor(name);
      const binary = role === 'c3' ? pin.binaries.c3
        : role === 'trim' ? pin.binaries.trim
        : isHot ? pin.binaries.controls : pin.binaries.iai;
      const c0Binary = role === 'c3' ? d.identities.C0.c3BinarySha256
        : role === 'trim' ? d.identities.C0.trimBinarySha256
        : isHot ? d.identities.C0.controlsBinarySha256
        : d.identities.C0.binarySha256;
      const blockBytes = name.startsWith('growth16_')
        || name === DISTINCT_WORD_PRODUCER || name === LARGE_REMOTE_FREE ? 16 : 258752;
      const r = {
        ...row(
          arm, arm === 'C0' ? c0Binary : binary, name,
          isHot ? hotCfg(name, a) : cfg(work, blockBytes),
        ),
      };
      if (isHot && arm === 'C0') r.config.resolvedProvenance.binarySha256 = d.identities.C0.oracleBinarySha256;
      r.Ir = 100000;
      if (mi.includes(name)) {
        r.config = {
          requested: null, resolved: null, configConflictsDelta: null,
          notApplicableReason: 'mimalloc does not resolve SeferAlloc configuration',
          processBoundary: 'isolated-process', runId: r.config.runId, pid: r.config.pid,
        };
      }
      const needed = benefit.includes(name) || name === context.publication
        || Object.hasOwn(PRODUCERS, name) || TRIM_CELLS.includes(name);
      r.oracle = Object.fromEntries([...counters, 'unique_pending_words']
        .map(k => [k, needed ? 0 : null]));
      const reasonKeys = [...counters.filter(k => r.oracle[k] === null), ...(r.oracle.unique_pending_words === null ? ['unique_pending_words'] : [])];
      r.oracle.unobservedReasons = Object.fromEntries(reasonKeys.map(k => [k, needed ? 'unobserved in synthetic fixture' : 'compiled out in primary; no paired observer for this control']));
      if (needed && arm.startsWith('A')) {
        for (const k of [...SUMMARY_ONLY_COUNTERS, 'unique_pending_words']) {
          r.oracle[k] = 0;
          delete r.oracle.unobservedReasons[k];
        }
      }
      if (!needed) {
        r.oracle.unique_pending_words = null;
        r.oracle.unobservedReasons.unique_pending_words = 'no producer counter observed in this control';
      }
      r.oracle.linkage = null;
      if (needed) {
        r.oracle.linkage = linkage(r, pin);
        if (benefit.includes(name)) {
          r.oracle.routed_scans = 99;
          if (isB) r.oracle.summary_words_visited = 64;
          else {
            const v = name === benefit[0] ? 319063 : name === benefit[1] ? 1004503 : 100;
            r.oracle.pending_words_visited = v;
            r.oracle.pending_words_exchanged = v;
          }
        }
        if (name === context.publication || TRIM_CELLS.includes(name)) {
          r.oracle.routed_scans = 1;
          r.oracle.discovery_records_reclaimed = name === context.publication ? 34 : 1;
          r.oracle.strict_trim_remaining = name === TRIM_CELLS[0] || name === context.publication ? 0 : null;
          if (r.name === context.publication || TRIM_CELLS.includes(name)) {
            r.oracle.quiescent_summary_missing_bits = 0;
            r.oracle.sizeof_small_sidecar = 41984;
          }
          if (r.oracle.strict_trim_remaining === null) {
            r.oracle.unobservedReasons.strict_trim_remaining = 'strict trim counter unavailable for this cell';
          } else delete r.oracle.unobservedReasons.strict_trim_remaining;
        }
        if (Object.hasOwn(PRODUCERS, name) || TRIM_CELLS.includes(name)) {
          r.oracle.sizeof_small_sidecar = 41984;
          r.oracle.quiescent_summary_missing_bits = 0;
        }
        if (Object.hasOwn(PRODUCERS, name)) {
          r.oracle.sizeof_small_sidecar = 41984;
          r.oracle.quiescent_summary_missing_bits = 0;
          r.oracle.summary_hint_rmw = isB
            ? (name === LARGE_REMOTE_FREE ? 0 : name === DISTINCT_WORD_PRODUCER ? 4096 : 64) : 0;
          r.oracle.summary_hint_loads = isB ? (name === LARGE_REMOTE_FREE ? 0 : 4096) : 0;
          r.oracle.unique_pending_words = isB
            ? (name === LARGE_REMOTE_FREE ? 0 : name === DISTINCT_WORD_PRODUCER ? 4096 : 64) : 0;
          if (isB) {
            for (const k of [
              'unique_pending_words', 'summary_hint_rmw', 'summary_hint_loads',
              'quiescent_summary_missing_bits', 'sizeof_small_sidecar',
            ]) delete r.oracle.unobservedReasons[k];
          }
        }
        if (name === producer) {
          r.scenario = 'joined remote Small frees; repeated frees in same pending word; ' +
            'no owner discovery during producer window';
        }
        if (!context.strictTrimObserved(name)) r.oracle.strict_trim_remaining = null;
        for (const k of counters) {
          if (k === 'strict_trim_remaining') {
            if (r.oracle[k] === null) {
              r.oracle.unobservedReasons[k] = 'strict trim counter unavailable for this cell';
            } else delete r.oracle.unobservedReasons[k];
          }
        }
        for (const k of [...counters, 'unique_pending_words']) {
          if (r.oracle[k] !== null) delete r.oracle.unobservedReasons[k];
        }
      }
      if (Object.hasOwn(PRODUCERS, name)) {
        const spec = PRODUCERS[name];
        const large = name === LARGE_REMOTE_FREE;
        const free = spec.freeCount;
        const block = spec.blockBytes ?? 1024;
        const symbol = spec.symbol;
        const primaryIr = 100000;
        const controlIr = 99990;
        const overhead = 8;
        const control = {
          receipt: 'synthetic/c', workload: 'child remote frees removed',
          source: r.source, binarySha256: h('8'), environmentSha256: r.environmentSha256,
          childIr: overhead, workloadIr: 0, childFreeCount: 0, boundaryExecutions: 1,
          symbolReceipt: 'synthetic/cnm', profileReceipt: 'synthetic/cprof', symbol,
          toggleCollect: symbol, cfg: [spec.cfg], features: primary,
          generatedArguments: 'synthetic', runnerVersion: '0.14.2',
          valgrindVersion: 'synthetic', collectAtStart: false, defaultToggle: false,
          entryPoint: 'Custom', separateThreads: true,
          aggregation: 'sum of all thread profile Ir', totalIr: controlIr,
          deltaIr: primaryIr - controlIr, parentSetupUnchanged: true,
          collectionUnchanged: true, minimumWorkPath: 'synthetic/minimum-work.txt',
          minimumWorkSha256: h('c'), minimumWorkRemovedPassObserved: true,
        };
        r.Ir = primaryIr;
        r.producerIrReceipt = {
          binarySha256: r.binarySha256, source: r.source,
          environmentSha256: r.environmentSha256, features: primary, cfg: [],
          receipt: 'synthetic/p', symbolReceipt: 'synthetic/nm',
          profileReceipt: 'synthetic/prof', symbol, runnerVersion: '0.14.2',
          valgrindVersion: 'synthetic', collectAtStart: false, defaultToggle: false,
          entryPoint: 'Custom', separateThreads: true, toggleCollect: symbol,
          generatedArguments: 'synthetic', aggregation: 'sum of all thread profile Ir',
          childFreeCount: free, childIr: 18, boundaryOverheadIr: 8, workloadIr: 10,
          boundaryExecutions: 1, totalIr: primaryIr, startedAt: r.startedAt,
          minimumWorkPath: 'synthetic/minimum-work.txt', minimumWorkSha256: h('d'),
          minimumWorkRemovedPassObserved: true, control,
        };
        r.producerIrReceipt.control.totalIr = controlIr;
        r.producerIrReceipt.control.deltaIr = primaryIr - controlIr;
        r.oracle.summary_hint_rmw = isB
          ? (large ? 0 : name === DISTINCT_WORD_PRODUCER ? 4096 : 64) : 0;
        r.oracle.summary_hint_loads = isB ? (large ? 0 : 4096) : 0;
        r.oracle.unique_pending_words = isB
          ? (large ? 0 : name === DISTINCT_WORD_PRODUCER ? 4096 : 64) : 0;
        r.oracle.quiescent_summary_missing_bits = 0;
        r.oracle.sizeof_small_sidecar = 41984;
        if (large) {
          r.Ir = 100000;
          r.producerIrReceipt.workloadIr = 10;
          r.producerIrReceipt.childIr = 18;
          r.producerIrReceipt.totalIr = r.Ir;
        }
        r.config.requested.workCount = free;
        r.config.resolved.workCount = free;
        r.config.requested.blockBytes = block;
        r.config.resolved.blockBytes = block;
      }
      if (Object.hasOwn(PRODUCERS, name)) {
        r.oracle.unique_pending_words = name === LARGE_REMOTE_FREE ? 0
          : name === DISTINCT_WORD_PRODUCER ? 4096 : 64;
        delete r.oracle.unobservedReasons.unique_pending_words;
      }
      if (arm === 'A1' || arm === 'A2' || arm === 'C0') {
        if (Object.hasOwn(PRODUCERS, name)) {
          r.oracle.summary_hint_rmw = null;
          r.oracle.summary_hint_loads = null;
          r.oracle.unobservedReasons.summary_hint_rmw = 'not observed in fixture';
          r.oracle.unobservedReasons.summary_hint_loads = 'not observed in fixture';
        }
      }
      if (name === context.construction && arm.startsWith('B')) {
        r.constructionGainReceipt = {
          path: 'docs/perf/r20_prf01_construction_gain_decomposition.txt',
          sha256: h('9'), removedPassObserved: true,
          decomposition: 'retained Callgrind A/B functional decomposition: the 4096-word pending pass is gone',
        };
      }
      d.iai.push(r);
    }
  }
  const setIr = (name, arm, ir) => {
    d.iai.find(r => r.name === name && r.arm === arm).Ir = ir;
  };
  for (const arm of arms) {
    setIr(benefit[0], arm, arm.startsWith('B') ? 1000000 : 4000000);
    setIr(benefit[1], arm, arm.startsWith('B') ? 1000000 : 12000000);
    for (const name of benefit.slice(2)) {
      setIr(name, arm, 100000 + benefit.indexOf(name) * 1000);
    }
    for (const f of context.hotFamilies) {
      for (const [i, s] of ['n', '2n', '4n'].entries()) {
        setIr(`r20_prf01_${f}_${s}`, arm, 100000 + 6400 * (1 << i));
      }
    }
    setIr(context.construction, arm, 100000);
    for (const name of mi) setIr(name, arm, 50000);
    setIr(context.publication, arm, arm.startsWith('B') ? 1000 : 100000);
    setIr(producer, arm, arm.startsWith('B') ? 100000 : 100000);
  }
  for (const r of d.iai.filter(r => r.name === producer)) {
    r.producerIrReceipt.totalIr = r.Ir;
    r.producerIrReceipt.control.totalIr = r.Ir - 10;
    r.producerIrReceipt.control.deltaIr = 10;
  }
  return d;
}
