// A mixed range may contain an already-declared runtime prefix followed by
// verification changes. Only the existing fail-closed skip policy can admit
// that prefix; the entire remaining suffix still owes exact text/Ir identity.
import { zeroCostSkip } from './ghost_diff.mjs';

// Candidates come from the PR head's first-parent history, newest first.
// The caller derives every subject and changed line from base..candidate.
export function declaredRuntimePrefix(candidates, { head, dirty = [] }) {
  if (dirty.length) return null;
  for (const candidate of candidates) {
    if (candidate.sha === head) continue;
    const reason = zeroCostSkip(candidate.subjects, candidate.changed, dirty);
    if (reason) {
      return {
        head: candidate.sha,
        reason,
        subjects: candidate.subjects,
        paths: [...candidate.changed.keys()],
      };
    }
  }
  return null;
}
