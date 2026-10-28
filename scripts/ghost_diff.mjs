// Scope of scripts/ghost_zero_cost.mjs (`--allow-declared-runtime`), pure; its
// self-test lives there. Plan §6 binds the zero-cost proof to ghost and
// verification changes; a range that declares a runtime change by its R30-12
// prefix changes production .text by design. The exception is fail-closed: any
// doubt keeps the proof required.
//
// Skip only when ALL hold over the PR's own commits (merges excluded):
//   1. some subject declares a runtime change: perf(runtime) / perf(opt-in) / fix(perf);
//   2. every subject has a conventional prefix and none is scoped to
//      verification / ghost / loom / miri / kani;
//   3. no changed path is a toolchain or build-graph input (Cargo.toml,
//      Cargo.lock, build.rs, .cargo/, rust-toolchain*) or lies under src/ghost/;
//   4. no changed line of a build-input file (src/, crates/) contains ghost,
//      shadow, sefer_ghost, loom, miri or kani in any case — this also catches a
//      cfg written across lines, since the token sits on its own line.
// A ghost macro called through an alias changes nothing in production: the
// macros compile to nothing outside cfg(any(loom, miri, kani, sefer_ghost)) and
// their definitions are under src/ghost/ (rule 3).

const RUNTIME = /^(?:perf\((?:runtime|opt-in)\)|fix\(perf\))!?:/;
const CONVENTIONAL = /^[a-z]+(?:\([^()]*\))?!?: \S/;
const VERIFICATION_SCOPE = /^[a-z]+\((?:[^()]*[,/ ])?(?:verification|ghost[^()]*|loom|miri|kani)(?:[,/ ][^()]*)?\)/;
const BUILD_GRAPH = /^(?:(?:[^/]+\/)*Cargo\.(?:toml|lock)|(?:[^/]+\/)*build\.rs|\.cargo\/.*|rust-toolchain(?:\.toml)?)$/;
const GHOST_TOKEN = /ghost|shadow|\b(?:loom|miri|kani)\b/i;
export const isBuildInput = (path) => path.startsWith('src/') || path.startsWith('crates/');

// `changed`: Map path -> array of changed line texts (without the +/- sign).
export function zeroCostSkip(subjects, changed) {
  if (!subjects.some((s) => RUNTIME.test(s))) return null;
  for (const s of subjects) {
    if (!CONVENTIONAL.test(s)) return null;
    if (VERIFICATION_SCOPE.test(s)) return null;
  }
  for (const [path, lines] of changed) {
    if (BUILD_GRAPH.test(path) || path.startsWith('src/ghost/')) return null;
    if (isBuildInput(path) && lines.some((l) => GHOST_TOKEN.test(l))) return null;
  }
  return 'the range declares a runtime change (perf(runtime) / perf(opt-in) / fix(perf)), has no verification-scoped commit and changes no ghost code or build-graph input';
}

// Changed lines of a `git diff -U0` for ONE path (headers before the first hunk skipped).
export function changedLines(diff) {
  const out = [];
  let inHunk = false;
  for (const line of diff.split('\n')) {
    if (line.startsWith('@@')) { inHunk = true; continue; }
    if (!inHunk) continue;
    if (line.startsWith('+') || line.startsWith('-')) out.push(line.slice(1));
  }
  return out;
}

// The exception is decided on BASE..HEAD commits, so the checkout must be that
// state: the classified head itself, or the PR merge ref whose parents are
// exactly [base, head]; and no uncommitted change to any tracked or untracked
// path. -> null when consistent, else why the exception cannot apply.
export function endpointsMismatch({ base, head, checkout, parents, dirty }) {
  if (dirty.length) return `uncommitted changes in the checkout: ${dirty.slice(0, 5).join(', ')}`;
  if (checkout === head) return null;
  if (parents.length === 2 && parents[0] === base && parents[1] === head) return null;
  return `checkout ${checkout} is neither the classified head ${head} nor its merge onto ${base}`;
}
