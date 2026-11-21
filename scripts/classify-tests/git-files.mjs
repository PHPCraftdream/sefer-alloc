import { spawnSync } from 'node:child_process';
import { lstatSync } from 'node:fs';
import { join } from 'node:path';

// Git supplies repo-relative POSIX paths; the worktree decides which still exist.
export function listGitFiles(rootDir, roots) {
  const result = spawnSync('git', [
    '--literal-pathspecs', 'ls-files', '-z', '--cached', '--others',
    '--exclude-standard', '--', ...roots,
  ], {
    cwd: rootDir,
    encoding: 'utf8',
    shell: false,
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) {
    throw new Error(`git ls-files failed in ${rootDir}: ${result.error.message}`, {
      cause: result.error,
    });
  }
  if (result.status !== 0) {
    const detail = result.stderr.trim() || result.signal || `exit ${result.status}`;
    throw new Error(`git ls-files failed in ${rootDir}: ${detail}`);
  }

  const files = new Set(result.stdout.split('\0'));
  files.delete('');
  return [...files].filter((file) => {
    try {
      return lstatSync(join(rootDir, file), { throwIfNoEntry: false })?.isFile();
    } catch (error) {
      if (error.code === 'ENOTDIR') return false;
      throw error;
    }
  }).sort();
}
