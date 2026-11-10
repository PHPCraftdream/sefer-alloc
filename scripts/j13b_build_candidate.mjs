// CI-only entry point. Never invoke this builder as a local static check.
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildCandidate } from './j13b/build-candidate.mjs';

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [directory, ...extra] = process.argv.slice(2);
  if (!directory || extra.length) {
    console.error('usage: node scripts/j13b_build_candidate.mjs <new-in-tree-output-directory>');
    process.exitCode = 2;
  } else {
    try {
      const result = buildCandidate(directory);
      console.log(`[j13b-candidate-build] wrote scoped candidate/provenance record: ${result.recordPath}`);
      console.log('[j13b-candidate-build] no payload, historical/static capability or runtime admission');
    } catch (error) {
      console.error(`[j13b-candidate-build] refused: ${error.message}`);
      process.exitCode = 1;
    }
  }
}
