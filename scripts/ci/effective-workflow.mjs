// Restricted local-composite expansion for static CI readers; never executes YAML.
import { readFileSync, realpathSync } from 'node:fs';
import { resolve, relative, sep } from 'node:path';

const header = /^name: [^\n]+\ndescription: [^\n]+\nruns:\n  using: composite\n  steps:\n/;
const call = /^      - uses: (\.\/\.github\/actions\/ci-[a-z0-9_-]+)\s*$/;
const shell = (runner) => runner.includes('windows') ? 'pwsh'
  : runner.includes('${{') ? "${{ runner.os == 'Windows' && 'pwsh' || 'bash -e {0}' }}" : 'bash -e {0}';

export function effectiveWorkflow(root, options = {}) {
  const source = options.source ?? '.github/workflows/ci.yml';
  const load = options.read ?? ((file) => {
    const base = realpathSync(root);
    const path = realpathSync(resolve(root, file));
    const rel = relative(base, path);
    if (rel.startsWith(`..${sep}`) || rel === '..' || resolve(base, rel) !== path) {
      throw Error(`CI source escapes repository: ${file}`);
    }
    return readFileSync(path, 'utf8');
  });
  const sources = new Map(), origins = [], dependencies = new Map();
  const output = [];
  let job = '', runner = '', checkedOut = false;
  function emit(text, file, line) { output.push(text); origins.push({ file, line, job }); }
  function expand(file, stack) {
    if (stack.includes(file)) throw Error(`local action cycle: ${[...stack, file].join(' -> ')}`);
    const text = load(file).replace(/\r\n/g, '\n');
    sources.set(file, text);
    if (!header.test(text)) throw Error(`unsupported composite header: ${file}`);
    const offset = text.match(header)[0].split('\n').length - 1;
    const lines = text.split('\n');
    const body = lines.slice(offset);
    if (body.some(l => l && !l.startsWith('    ') && !l.startsWith('#'))) {
      throw Error(`unsupported composite structure: ${file}`);
    }
    dependencies.set(file, new Set([...(dependencies.get(file) ?? []), job]));
    for (let i = offset; i < lines.length; i++) {
      const line = lines[i];
      if (line === '      # ci-default-shell: preserve original implicit runner shell') {
        const expected = `      shell: ${shell(runner)}`;
        if (lines[++i] !== expected) throw Error(`incorrect default shell: ${file}:${i + 1}`);
        continue;
      }
      if (/^\s*(?:- )?uses:\s*['"]?\.{1,2}\//.test(line)) {
        throw Error(`nested local actions are not supported: ${file}:${i + 1}`);
      }
      if (/\$\{\{\s*(?:needs|matrix|inputs|secrets)\./.test(line)) {
        throw Error(`unforwarded action context: ${file}:${i + 1}`);
      }
      if (line !== '' || i !== lines.length - 1) emit(line ? `  ${line}` : '', file, i + 1);
    }
  }
  const text = load(source).replace(/\r\n/g, '\n');
  sources.set(source, text);
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const match = /^  ([\w-]+):\s*$/.exec(line);
    if (match && lines.slice(0, i).some(l => l === 'jobs:')) {
      job = match[1]; runner = ''; checkedOut = false;
    }
    const run = /^    runs-on:\s*(.*)$/.exec(line);
    if (run) runner = run[1];
    if (/^      - uses: actions\/checkout@/.test(line)) checkedOut = true;
    const local = call.exec(line);
    if (local) {
      if (!job || !runner || !checkedOut) throw Error(`local action before checkout/runner: ${source}:${i + 1}`);
      if (lines[i + 1] && /^        \S/.test(lines[i + 1])) {
        throw Error(`local action call attributes unsupported: ${source}:${i + 1}`);
      }
      expand(`${local[1].slice(2)}/action.yml`, []);
    } else {
      if (/^\s*(?:- )?uses:\s*['"]?\.{1,2}\//.test(line)) throw Error(`unsupported local action call: ${source}:${i + 1}`);
      emit(line, source, i + 1);
    }
  }
  return { text: output.join('\n'), origins, sources, dependencies };
}
