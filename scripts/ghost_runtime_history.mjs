// Lift source-history discovery across a tree-identical two-parent landing only.
// This chooses no measured baseline and grants no exemption: the unchanged policy
// still admits the prefix, and the actual landing checkout owes the suffix proof.
export function runtimeHistory({ base, head }, git) {
  const original = { base, head };
  const parents = git(['rev-list', '--parents', '-n', '1', head]).trim().split(/\s+/);
  if (parents.length !== 3 || parents[0] !== head || parents[1] !== base) return original;
  const side = parents[2];
  const bases = git(['merge-base', '--all', base, side]).trim().split(/\s+/).filter(Boolean);
  if (bases.length !== 1) return original;
  const common = bases[0];
  const tree = ref => git(['rev-parse', '--verify', `${ref}^{tree}`]).trim();
  const headTree = tree(head), sideTree = tree(side);
  if (!headTree || headTree !== sideTree) return original;
  const baseTree = tree(base), commonTree = tree(common);
  if (!baseTree || baseTree !== commonTree) return original;
  return { base: common, head: side, bridge: {
    landing: head, first_parent: base, side_parent: side, common_base: common,
    landing_and_side_tree: headTree, base_and_common_tree: baseTree,
    scope: 'source-history bridge only; actual landing remains the measured proof head',
  } };
}
