const PHASES = ['setup', 'toolchain', 'cacheRestore', 'cacheSave', 'build', 'execution', 'other', 'unknown'];
const VERSION = 3;
const ANSI = /\u001b\[[0-?]*[ -/]*[@-~]/g;
const PREFIX = /^(?:\uFEFF)?(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?Z)\s?(.*)$/;
const GROUP = /^##\[group\](.*)$/;
const END_GROUP = /^##\[endgroup\]/;
const ACTION_START = /^##\[start-action\s+display=(.*?);id=([^\]]+)\]/;
const ACTION_END = /^##\[end-action\s+id=([^;\]]+)/;
const CARGO = /^cargo(?:\s+\+[^\s]+)?\s+(miri|test|check|build|clippy|doc|run|install|bench|kani)\b[^\r\n]*/i;
// Cargo emits package/version status lines; verifier output also uses “Checking”, but is not a build.
const CARGO_STATUS = /^(?:Compiling|Checking)\s+\S+\s+v/;
const KANI_EXECUTION = /^(?:Checking harness\b|VERIFICATION:|Verification Time:|Running propositional reduction\b|Running with \d+ object bits\b)/i;
const timestamp = value => {
  if (value === undefined || value === null || value === '') return null;
  const ms = Date.parse(value);
  return Number.isFinite(ms) ? ms : null;
};
const sec = value => Math.round(value * 1000) / 1000;
function compact(value, max = 160) {
  return String(value ?? '').replace(ANSI, '').replaceAll('\r', '').replace(/\s+/g, ' ').trim()
    .replace(/(?:[A-Z]:\\|\\\\|\/home\/|\/Users\/)[^\s"']+/g, '<path>')
    .replace(/(token|password|secret)(\s*[=:]\s*)\S+/ig, '$1$2<redacted>').slice(0, max);
}
function labelPhase(label) {
  const value = String(label ?? '').toLowerCase();
  if (/cache.*restore|restore.*cache|rust-cache.*restore/.test(value)) return 'cacheRestore';
  if (/cache.*save|save.*cache|rust-cache.*save/.test(value)) return 'cacheSave';
  if (/set up job|setup job|checkout|runner image|provision|prepare workflow|complete job/.test(value)) return 'setup';
  if (/rust-toolchain|toolchain|rustup|install rust|miri sysroot/.test(value)) return 'toolchain';
  return null;
}
function commandFrom(value) {
  const text = compact(value, 260);
  const match = CARGO.exec(text);
  if (!match) return null;
  const verb = match[1].toLowerCase();
  const targets = [...text.matchAll(/--test\s+([\w.-]+)/g)].map(item => item[1]).slice(0, 20);
  const packages = [...text.matchAll(/(?:-p|--package)\s+([\w.-]+)/g)].map(item => item[1]).slice(0, 10);
  const features = [...text.matchAll(/--features?\s+(?:"([^"]*)"|'([^']*)'|(\S+))/g)].map(item => item[1] ?? item[2] ?? item[3]).join(' ');
  return { command: text, verb, kind: verb === 'install' ? 'install' : verb, targets, packages, features: compact(features, 180) };
}

/** Pure parser for GitHub Actions job logs. Log timestamps are bounded by the supplied API interval. */
export function parseJobLog(text, { runId, jobId, jobName, startedAt, completedAt } = {}) {
  const startMs = timestamp(startedAt), endMs = timestamp(completedAt);
  const duration = startMs === null || endMs === null || endMs < startMs ? null : (endMs - startMs) / 1000;
  const meta = { schema: 'ci-long-jobs-log-v1', parserVersion: VERSION, runId: String(runId ?? ''), jobId: String(jobId ?? ''), jobName: compact(jobName), startedAt: startedAt ?? null, completedAt: completedAt ?? null };
  const diagnostics = { invalidEndpoints: duration === null, invalidTimestamps: 0, outOfOrderTimestamps: 0, outsideJobTimestamps: 0, unmatchedEndGroups: 0, unclosedGroups: 0, unclosedActions: 0 };
  const groups = [], actions = [], commands = [], cargoSpans = [], segments = [];
  const groupStack = [], actionStack = [];
  const lines = String(text ?? '').split(/\r?\n/);
  if (duration === null) return { ...meta, durationSeconds: null, phases: Object.fromEntries(PHASES.map(key => [key, null])), segments, groups, actions, commands, cargoSpans, diagnostics };

  let cursor = startMs, cursorLine = 0, previousInput = -Infinity;
  let activeStep = '', activeCommand = null, activeSpan = null, activeSpanIndex = -1;
  let compileStart = null, executionStart = null, cachePhase = null, toolchain = false;
  let echoCommand = null, echoGroup = null;
  const env = {};
  const offset = ms => sec((ms - startMs) / 1000);
  const currentAction = () => actionStack.at(-1)?.label ?? null;
  const phaseNow = () => {
    if (cachePhase) return cachePhase;
    if (activeSpan) return activeSpan.phase;
    const explicit = labelPhase(currentAction()) ?? labelPhase(activeStep);
    if (explicit) return explicit;
    return toolchain ? 'toolchain' : activeStep ? 'other' : 'unknown';
  };
  function addSegment(end, line) {
    if (end <= cursor) return;
    const phase = phaseNow(), command = activeCommand;
    const segment = { start: offset(cursor), end: offset(end), seconds: sec((end - cursor) / 1000), phase, sourceLines: [cursorLine, Math.max(cursorLine, line - 1)] };
    if (command) {
      segment.command = command.command;
      segment.targets = command.targets;
      segment.features = command.features;
      segment.env = { ...env };
    }
    if (currentAction()) segment.action = compact(currentAction(), 100);
    const last = segments.at(-1);
    if (last && last.phase === segment.phase && last.command === segment.command && last.action === segment.action && last.end === segment.start && JSON.stringify(last.env ?? {}) === JSON.stringify(segment.env ?? {})) {
      last.end = segment.end; last.seconds = sec(last.seconds + segment.seconds); last.sourceLines[1] = segment.sourceLines[1];
    } else segments.push(segment);
  }
  function openCargo(command, ms, line) {
    if (!command || activeSpan) return;
    activeCommand = command;
    const span = { command: command.command, kind: command.kind, targets: command.targets, packages: command.packages, features: command.features, commandGroup: command.group ?? null, commandProven: command.kind !== 'unknown' && command.group !== null, startedAt: offset(ms), endedAt: null, sourceLines: [line, line], compile: [], execution: [], action: compact(currentAction(), 100) || null };
    activeSpanIndex = cargoSpans.push(span) - 1;
    activeSpan = { phase: command.kind === 'install' ? 'setup' : 'unknown' };
    compileStart = null; executionStart = null; toolchain = false;
  }
  function closeCargo(ms, line) {
    if (!activeSpan) return;
    const span = cargoSpans[activeSpanIndex], end = offset(ms);
    span.endedAt = end; span.sourceLines[1] = line;
    if (compileStart) span.compile.push({ start: compileStart.offset, end, sourceLines: [compileStart.line, line], reportedSeconds: null, bounded: false });
    if (executionStart) span.execution.push({ start: executionStart.offset, end, sourceLines: [executionStart.line, line] });
    activeCommand = null; activeSpan = null; activeSpanIndex = -1; compileStart = null; executionStart = null;
  }
  function addCommand(command, line, group) {
    if (!command) return null;
    let existing = null;
    for (let i = commands.length - 1; i >= 0; i--) {
      if (commands[i].group === group) { if (commands[i].command === command.command) existing = commands[i]; break; }
    }
    if (existing) { existing.sourceLines[1] = line; return existing; }
    const row = { ...command, group, sourceLines: [line, line], action: compact(currentAction(), 100) || null };
    commands.push(row);
    return row;
  }

  for (let i = 0; i < lines.length; i++) {
    const lineNo = i + 1, raw = lines[i], prefix = PREFIX.exec(raw);
    if (!prefix) continue;
    const ms = timestamp(prefix[1]);
    if (ms === null) { diagnostics.invalidTimestamps++; continue; }
    if (ms < previousInput) { diagnostics.outOfOrderTimestamps++; continue; }
    previousInput = ms;
    if (ms < startMs || ms > endMs) { diagnostics.outsideJobTimestamps++; continue; }
    if (ms < cursor) { diagnostics.outOfOrderTimestamps++; continue; }
    addSegment(ms, lineNo);
    cursor = ms; cursorLine = lineNo;
    const message = prefix[2].replace(ANSI, '').trim();

    const groupMatch = GROUP.exec(message);
    if (groupMatch) {
      const label = compact(groupMatch[1]);
      if (/^Run\s/i.test(label)) {
        // A peer Run starts a new runner step; prior command's log interval ends here.
        closeCargo(ms, lineNo);
        cachePhase = null; toolchain = false; echoCommand = null; echoGroup = null;
        const peerAt = groupStack.findLastIndex(group => group.isRun);
        if (peerAt >= 0) {
          const old = groupStack.splice(peerAt);
          for (const group of old) { group.autoClosedAt = offset(ms); group.autoClosedLine = lineNo; }
        }
        activeStep = label;
      }
      const group = { index: groups.length, label, parent: groupStack.at(-1)?.index ?? null, start: offset(ms), startLine: lineNo, endMarkerAt: null, endMarkerLine: null, autoClosedAt: null, autoClosedLine: null, scopeEnd: null, scopeEndLine: null, isRun: /^Run\s/i.test(label) };
      groups.push(group); groupStack.push(group);
      if (group.isRun) {
        const direct = commandFrom(label.replace(/^Run\s+/i, ''));
        if (direct) { echoCommand = addCommand(direct, lineNo, group.index); echoGroup = group.index; }
      }
      const phase = labelPhase(label);
      if (phase === 'cacheRestore' || phase === 'cacheSave') cachePhase = phase;
      if (phase === 'toolchain') toolchain = true;
      continue;
    }
    if (END_GROUP.test(message)) {
      const group = groupStack.pop();
      if (!group) diagnostics.unmatchedEndGroups++;
      else {
        group.endMarkerAt = offset(ms); group.endMarkerLine = lineNo;
        if (group.isRun) {
          const command = echoGroup === group.index ? echoCommand : commandFrom(group.label.replace(/^Run\s+/i, ''));
          if (command) openCargo(command, ms, lineNo);
        }
      }
      continue;
    }

    const startAction = ACTION_START.exec(message);
    if (startAction) {
      const label = compact(startAction[1]);
      const action = { label, id: startAction[2], parent: actionStack.at(-1)?.id ?? null, start: offset(ms), startLine: lineNo, end: null, endLine: null };
      const sibling = actionStack.findLastIndex(item => item.parent === action.parent);
      if (sibling >= 0) {
        const closed = actionStack.splice(sibling);
        for (const item of closed) { item.end = offset(ms); item.endLine = lineNo; }
        closeCargo(ms, lineNo);
      }
      actionStack.push(action); actions.push(action);
      const phase = labelPhase(label);
      if (phase === 'cacheRestore' || phase === 'cacheSave') cachePhase = phase;
      if (phase === 'toolchain') toolchain = true;
      continue;
    }
    const endAction = ACTION_END.exec(message);
    if (endAction) {
      const index = actionStack.findLastIndex(item => item.id === endAction[1]);
      if (index >= 0) {
        const closed = actionStack.splice(index);
        for (const item of closed) { item.end = offset(ms); item.endLine = lineNo; }
      }
      closeCargo(ms, lineNo); cachePhase = null; toolchain = false; echoCommand = null; echoGroup = null;
      continue;
    }

    if (/^env:$/.test(message)) continue;
    const envMatch = /^\s*([A-Z_][A-Z0-9_]*)\s*:\s*(.*)$/.exec(prefix[2]);
    if (envMatch) { env[envMatch[1]] = compact(envMatch[2], 100); continue; }
    const cleanMessage = message.replace(/^\u001b\[[0-9;]*m/, '').replace(/\u001b\[[0-9;]*m/g, '').trim();
    const isEcho = /^\u001b\[[0-9;]*m/.test(prefix[2]) || /^cargo(?:\s|$)/.test(cleanMessage);
    if (isEcho) {
      const info = commandFrom(cleanMessage);
      if (info) { echoCommand = addCommand(info, lineNo, groupStack.at(-1)?.index ?? null); echoGroup = groupStack.at(-1)?.index ?? null; }
    }

    const lower = cleanMessage.toLowerCase();
    if (/cache.*restore|restore.*cache/.test(lower)) cachePhase = 'cacheRestore';
    else if (/cache.*save|save.*cache/.test(lower)) cachePhase = 'cacheSave';
    if (/cache restored|cache saved|cache not found|cache is not found|cache hit|cache miss/.test(lower)) cachePhase = null;
    if (/running .*rustup.*component.*add|downloading component rust-src|preparing a sysroot for miri|syncing channel updates for/.test(lower)) {
      if (activeSpan && activeCommand?.kind !== 'install') activeSpan.phase = 'toolchain';
      else if (!activeSpan) toolchain = true;
    }
    if (/downloading crates/.test(lower) && activeSpan && activeCommand?.kind !== 'install') activeSpan.phase = 'build';

    // Only canonical Cargo package/version status lines start or reset build timing. Kani’s
    // “Checking harness …” and similar verifier output belongs to execution, not compilation.
    const compileLine = CARGO_STATUS.test(cleanMessage);
    const verificationLine = KANI_EXECUTION.test(cleanMessage);
    const finishedMatch = /\bFinished\b.*\bin\s+((?:\d+m)?\d+(?:\.\d+)?s)/i.exec(cleanMessage);
    const runningLine = /^(?:running \d+ tests?|.*\bRunning\b.*(?:\.rs|\/target\/|\btests?\b))/.test(cleanMessage);
    const resultLine = /^(?:test result:|error: test failed)/.test(cleanMessage);
    if ((compileLine || runningLine || (verificationLine && (activeCommand?.verb === 'kani' || /\bkani\b/i.test(currentAction() ?? '')))) && !activeSpan) openCargo(echoCommand ?? { command: 'unknown cargo command', verb: 'unknown', kind: 'unknown', targets: [], packages: [], features: '', group: null }, ms, lineNo);
    if (activeSpan) {
      const span = cargoSpans[activeSpanIndex], at = offset(ms);
      span.sourceLines[1] = lineNo;
      if (compileLine && activeCommand.kind !== 'install') {
        activeSpan.phase = 'build'; toolchain = false;
        if (!compileStart) compileStart = { offset: at, line: lineNo };
      }
      if (finishedMatch && activeCommand.kind !== 'install') {
        if (compileStart) {
          const rawElapsed = finishedMatch[1], reported = rawElapsed.includes('m')
            ? Number(rawElapsed.split('m')[0]) * 60 + Number(rawElapsed.split('m')[1].slice(0, -1))
            : Number(rawElapsed.slice(0, -1));
          span.compile.push({ start: compileStart.offset, end: at, sourceLines: [compileStart.line, lineNo], reportedSeconds: Number.isFinite(reported) ? reported : null, bounded: true });
          compileStart = null;
        }
        const runsTests = ['miri', 'test', 'bench', 'run', 'kani'].includes(activeCommand.verb);
        // For Kani, Cargo finishing its dev-profile build does not mean verification started.
        activeSpan.phase = activeCommand.verb === 'kani' ? 'other' : runsTests ? 'execution' : 'build';
        if (runsTests && activeCommand.verb !== 'kani' && !executionStart) executionStart = { offset: at, line: lineNo };
      }
      if (runningLine && activeCommand.kind !== 'install') {
        activeSpan.phase = 'execution';
        if (!executionStart) executionStart = { offset: at, line: lineNo };
      }
      if ((activeCommand.verb === 'kani' || /\bkani\b/i.test(currentAction() ?? '')) && verificationLine) {
        activeSpan.phase = 'execution';
        if (!executionStart) executionStart = { offset: at, line: lineNo };
      }
      if (resultLine && activeCommand.kind !== 'install') {
        if (executionStart) { span.execution.push({ start: executionStart.offset, end: at, sourceLines: [executionStart.line, lineNo] }); executionStart = null; }
        activeSpan.phase = 'other';
      }
    }
  }
  if (cursor < endMs) addSegment(endMs, lines.length);
  if (activeSpan) closeCargo(endMs, lines.length);
  const commandSignaturesByGroup = new Map();
  for (const command of commands) {
    if (command.group === null) continue;
    const signatures = commandSignaturesByGroup.get(command.group) ?? new Set();
    signatures.add(command.command);
    commandSignaturesByGroup.set(command.group, signatures);
  }
  for (const span of cargoSpans) {
    const groupCommands = commandSignaturesByGroup.get(span.commandGroup)?.size ?? 0;
    const knownCommand = span.commandProven && groupCommands === 1;
    span.commandAttribution = knownCommand ? 'known' : 'ambiguous';
    span.commandAttributionReason = knownCommand ? null
      : !span.commandProven ? 'command-not-proven-in-run-or-action'
        : groupCommands > 1 ? 'multiple-distinct-commands-in-group' : 'command-group-not-found';
    const targetCount = span.targets?.length ?? 0;
    span.targetAttribution = !knownCommand ? 'ambiguous' : targetCount === 1 ? 'known' : targetCount > 1 ? 'ambiguous' : 'unavailable';
    span.targetAttributionReason = !knownCommand ? span.commandAttributionReason
      : targetCount > 1 ? 'multiple-targets' : targetCount === 0 ? 'no-single-target' : null;
  }
  diagnostics.unclosedGroups = groupStack.length;
  diagnostics.unclosedActions = actionStack.length;
  for (const group of groups) {
    const sibling = groups.find(item => item.index > group.index && item.parent === group.parent);
    if (sibling) { group.scopeEnd = sibling.start; group.scopeEndLine = sibling.startLine; }
    else { group.scopeEnd = sec(duration); group.scopeEndLine = null; }
  }
  // Log silent gaps are unknown; timestamps still form a complete, disjoint API-bounded partition.
  const partition = [];
  let next = 0;
  for (const segment of segments) {
    if (segment.start > next) partition.push({ start: next, end: segment.start, seconds: sec(segment.start - next), phase: 'unknown', sourceLines: [0, segment.sourceLines[0]] });
    if (segment.start < next) throw new Error('overlapping log intervals');
    partition.push(segment); next = segment.end;
  }
  if (next < duration) partition.push({ start: next, end: sec(duration), seconds: sec(duration - next), phase: 'unknown', sourceLines: [lines.length, 0] });
  const phases = Object.fromEntries(PHASES.map(phase => [phase, sec(partition.filter(item => item.phase === phase).reduce((sum, item) => sum + item.seconds, 0))]));
  const sum = Object.values(phases).reduce((total, value) => total + value, 0);
  if (partition.some(item => item.seconds < 0 || item.end < item.start) || Math.abs(sum - duration) > 0.002) throw new Error('log phase partition invariant failed');
  return { ...meta, durationSeconds: sec(duration), phases, segments: partition, groups, actions, commands, cargoSpans, diagnostics };
}

export const logParserVersion = VERSION;
