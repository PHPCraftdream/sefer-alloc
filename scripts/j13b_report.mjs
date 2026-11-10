#!/usr/bin/env node
import { closeSync, fsyncSync, openSync, readFileSync, statSync, unlinkSync, writeSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseAndDerive, reportSyntheticJsonl } from './j13b_stats.mjs';

const MAX_INPUT_BYTES = 32 * 1024 * 1024;
const USAGE = 'usage: node scripts/j13b_report.mjs --fixtures <synthetic.jsonl> <report.md> <summary.csv>';

function writeExclusive(path, text) {
  const fd = openSync(resolve(path), 'wx');
  try {
    writeSync(fd, text);
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
}

export const nodeIo = {
  readInput(path) {
    const resolved = resolve(path);
    if (statSync(resolved).size > MAX_INPUT_BYTES) throw Error('fixture input exceeds 32 MiB');
    return readFileSync(resolved);
  },
  writePair(markdownPath, markdown, csvPath, csv) {
    writeExclusive(markdownPath, markdown);
    try {
      writeExclusive(csvPath, csv);
    } catch (error) {
      unlinkSync(resolve(markdownPath));
      throw error;
    }
  },
};

export function runReportCli(args, io = nodeIo) {
  if (args.length === 0) {
    try {
      parseAndDerive('');
    } catch (error) {
      return { code: 1, message: `[j13b_report] no report emitted: ${error.message}` };
    }
  }
  if (args[0] !== '--fixtures' || args.length !== 4) return { code: 2, message: USAGE };
  try {
    const bytes = io.readInput(args[1]);
    if (bytes.length > MAX_INPUT_BYTES) throw Error('fixture input exceeds 32 MiB');
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    const report = reportSyntheticJsonl(text);
    io.writePair(args[2], report.markdown, args[3], report.csv);
    return { code: 0, message: '[j13b_report] wrote SYNTHETIC fixture artifacts only; not measurements' };
  } catch (error) {
    return { code: 1, message: `[j13b_report] no report emitted: ${error.message}` };
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = runReportCli(process.argv.slice(2));
  (result.code === 0 ? console.log : console.error)(result.message);
  process.exitCode = result.code;
}
