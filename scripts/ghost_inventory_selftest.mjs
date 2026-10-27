// Self-test of scripts/ghost_inventory.mjs (`--selftest`): each case is a small Rust fixture with the
// exact sites (or classification outcome) the scanner must produce. A case that passes with the
// scanner's relevant step removed would be vacuous, so every case targets one step: comment/literal
// blanking, item paths, multi-line arguments, wrappers, ordering pass-through, nesting, cfg capture,
// and the classifier's gap / double-claim / dead-scope / test-fn rules.
import assert from 'node:assert/strict';

const CASES = [
  {
    name: 'comments, strings, raw strings and chars are blanked',
    src: '// a.load(Ordering::Acquire)\n/* b.store(1, Ordering::Release) /* nested */ */\nfn f() { let s = "c.swap(1, Ordering::AcqRel)"; let r = r#"d.load(Ordering::SeqCst)"#; let ch = \'(\'; }',
    sites: [],
  },
  {
    name: 'method op: receiver, fn, impl path',
    src: 'impl HeapSlot {\n    pub(crate) fn cas(&self) -> bool {\n        self.state.load(Ordering::Acquire) == 0\n    }\n}',
    sites: [{ op: 'load', receiver: 'self.state', orderings: 'Acquire', fn: 'cas', item: 'impl HeapSlot::cas', kind: 'atomic' }],
  },
  {
    name: 'multi-line CAS keeps both orderings',
    src: 'fn g(w: &AtomicU64) {\n    let _ = w.compare_exchange(\n        1,\n        2,\n        Ordering::AcqRel,\n        Ordering::Acquire,\n    );\n}',
    sites: [{ op: 'compare_exchange', receiver: 'w', orderings: 'AcqRel/Acquire', fn: 'g' }],
  },
  {
    name: 'wrapper call carrying orderings is a site',
    src: 'fn drop_lease(slot: &HeapSlot) {\n    slot.cas_state(LIVE, FREE, Ordering::Release, Ordering::Relaxed);\n}',
    sites: [{ op: 'cas_state', receiver: 'slot', orderings: 'Release/Relaxed', kind: 'wrapper' }],
  },
  {
    name: 'ordering passed through a parameter',
    src: 'impl Head {\n    fn load(&self, order: Ordering) -> u64 {\n        self.head.load(order)\n    }\n}',
    sites: [{ op: 'load', receiver: 'self.head', orderings: 'param', fn: 'load' }],
  },
  {
    name: 'nested call: only the atomic, not the outer function',
    src: 'fn h(x: &AtomicBool) { consume(x.load(Ordering::Acquire)); }',
    sites: [{ op: 'load', receiver: 'x', orderings: 'Acquire', fn: 'h' }],
  },
  {
    name: 'array return type does not end the fn header',
    src: 'fn totals() -> [u64; 4] {\n    [A.load(Ordering::Relaxed), 0, 0, 0]\n}',
    sites: [{ op: 'load', receiver: 'A', orderings: 'Relaxed', fn: 'totals' }],
  },
  {
    name: 'cfg attribute is captured with its feature names',
    src: '#[cfg(feature = "internals")]\npub fn dbg_x() -> u64 { X.load(Ordering::Relaxed) }',
    sites: [{ op: 'load', fn: 'dbg_x', cfg: 'feature = "internals"' }],
  },
  {
    name: 'lifetimes are not char literals',
    src: "impl<'a> Scan<'a> {\n    fn next(&mut self) -> Option<&'a u8> { self.p.swap(0, Ordering::AcqRel); None }\n}",
    sites: [{ op: 'swap', receiver: 'self.p', orderings: 'AcqRel', fn: 'next', item: "impl <'a> Scan<'a>::next" }],
  },
  {
    name: 'a quote char literal does not open a string',
    src: "fn q(x: &AtomicU8) -> bool { let c = '\"'; x.load(Ordering::Acquire) == c as u8 }",
    sites: [{ op: 'load', receiver: 'x', orderings: 'Acquire', fn: 'q' }],
  },
  {
    name: 'a keyword with a parenthesised ordering comparison is not a site',
    src: 'fn p(o: Ordering) -> bool { if (o == Ordering::Acquire) { return true; } match (o) { _ => false } }',
    sites: [],
  },
  {
    name: 'fence is a site',
    src: 'fn k() { core::sync::atomic::fence(Ordering::SeqCst); }',
    sites: [{ op: 'fence', orderings: 'SeqCst', kind: 'fence' }],
  },
  {
    name: 'a non-ordering call is not a site',
    src: 'fn m(v: &Cell<u32>) { v.set(1); let _ = v.get(); store(3); }',
    sites: [],
  },
];

const CLASSIFY = {
  catalog: {
    test_fn: '^dbg_', test_fn_scope: 'hook',
    scopes: [
      { id: 'proto', class: 'protocol', files: ['src/a.rs'], receiver: '^self\\.state$' },
      { id: 'counter', class: 'counter', files: ['src/**'], receiver: '^[A-Z_]+$', orderings: ['Relaxed'] },
      { id: 'both', class: 'counter', files: ['src/b.rs'], receiver: '^B$' },
      { id: 'hook', class: 'test-hook', files: ['src/**'], fn: '^dbg_' },
      { id: 'dead', class: 'counter', files: ['src/none.rs'] },
    ],
  },
  sites: [
    { file: 'src/a.rs', fn: 'claim', item: 'claim', receiver: 'self.state', op: 'load', orderings: 'Acquire', kind: 'atomic', cfg: '' },
    { file: 'src/a.rs', fn: 'claim', item: 'claim', receiver: 'other', op: 'load', orderings: 'Acquire', kind: 'atomic', cfg: '' },
    { file: 'src/b.rs', fn: 'bump', item: 'bump', receiver: 'B', op: 'fetch_add', orderings: 'Relaxed', kind: 'atomic', cfg: '' },
    { file: 'src/a.rs', fn: 'dbg_peek', item: 'dbg_peek', receiver: 'self.state', op: 'load', orderings: 'Acquire', kind: 'atomic', cfg: '' },
    { file: 'src/a.rs', fn: 'bump', item: 'bump', receiver: 'A', op: 'fetch_add', orderings: 'Relaxed', kind: 'atomic', cfg: '' },
    { file: 'src/a.rs', fn: 'claim', item: 'claim', receiver: 'C', op: 'load', orderings: 'Acquire', kind: 'atomic', cfg: '' },
  ],
  expect: { gaps: ['other', 'C'], doubles: ['B'], dead: ['both', 'dead'], hook: 1, proto: 1 },
};

export function selftest({ scanFile, classify }) {
  let n = 0;
  for (const c of CASES) {
    const got = scanFile('src/fixture.rs', c.src);
    assert.equal(got.length, c.sites.length, `${c.name}: ${got.length} sites, want ${c.sites.length}: ${JSON.stringify(got)}`);
    c.sites.forEach((want, i) => {
      for (const [k, v] of Object.entries(want)) assert.equal(got[i][k], v, `${c.name}: site ${i} ${k}`);
    });
    n++;
  }
  const r = classify(CLASSIFY.catalog, CLASSIFY.sites.map((s) => ({ ...s })));
  assert.deepEqual(r.gaps.map((g) => g.receiver), CLASSIFY.expect.gaps, 'classify: gaps');
  assert.deepEqual(r.doubles.map((d) => d.site.receiver), CLASSIFY.expect.doubles, 'classify: doubles');
  assert.deepEqual(r.dead, CLASSIFY.expect.dead, 'classify: dead scopes');
  assert.equal(r.perScope.get('hook'), CLASSIFY.expect.hook, 'classify: a test fn is claimed by the hook scope only');
  assert.equal(r.perScope.get('proto'), CLASSIFY.expect.proto, 'classify: protocol scope excludes the test fn');
  n++;
  return n;
}
