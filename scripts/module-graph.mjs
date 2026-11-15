#!/usr/bin/env node
// Thin CLI entry for the module-dependency-graph tool (scripts/module-graph/:
// lexer.mjs → parse.mjs → graph.mjs → report.mjs; THIS file adds no business
// logic — it parses argv, builds the graph, prints or writes).
//
//   node scripts/module-graph.mjs                       # summary: identity + stats + one line per claim status
//   node scripts/module-graph.mjs --level top|nested    # print that rollup's edge list (source target count cfg)
//   node scripts/module-graph.mjs --json                # full graph JSON to stdout
//   node scripts/module-graph.mjs --csv <path>          # write the nested-edge CSV
//   node scripts/module-graph.mjs --report [dir]        # write <dir>/MODULE_GRAPH_20261115.md + .csv (default docs/perf)
//   node scripts/module-graph.mjs --check-backedges     # exit 1 if any back-edge exists (future A1 ratchet;
//                                                       #   today prints the baseline count and exits 0)
//
// Shared flags: --src <dir> (default src) --config <path>
//               (default scripts/module-graph/layers.json) --max-sites <N> (default 5)
//
// Exit codes: 0 ok; 1 runtime error or (--check-backedges, once the ratchet is
// armed) check failure; 2 usage error.

import { writeFileSync } from "node:fs";
import { buildModuleGraph } from "./module-graph/graph.mjs";
import { renderCsv, renderMarkdown, writeReport } from "./module-graph/report.mjs";

const USAGE = `usage: node scripts/module-graph.mjs [--level top|nested] [--json] [--csv <path>]
                    [--report [dir]] [--check-backedges]
                    [--src <dir>] [--config <path>] [--max-sites <N>]`;

// A1 flips this to true: --check-backedges then exits 1 on any back-edge.
const RATCHET_ENFORCED = false;

function parseArgs(argv) {
  const opts = { src: "src", config: "scripts/module-graph/layers.json", maxSites: 5, modes: [] };
  const wantValue = new Set(["--src", "--config", "--max-sites", "--csv"]);
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const val = () => {
      if (i + 1 >= argv.length) throw new Error(`missing value for ${a}`);
      return argv[++i];
    };
    switch (a) {
      case "--src": opts.src = val(); break;
      case "--config": opts.config = val(); break;
      case "--max-sites": {
        const n = Number(val());
        if (!Number.isInteger(n) || n < 1) throw new Error("--max-sites needs a positive integer");
        opts.maxSites = n;
        break;
      }
      case "--csv": opts.csvPath = val(); opts.modes.push("csv"); break;
      case "--level": {
        const level = val();
        if (level !== "top" && level !== "nested") throw new Error("--level must be top or nested");
        opts.level = level;
        opts.modes.push("level");
        break;
      }
      case "--report": {
        const next = argv[i + 1];
        opts.reportDir = next !== undefined && !next.startsWith("--") ? (++i, next) : "docs/perf";
        opts.modes.push("report");
        break;
      }
      case "--json": opts.json = true; opts.modes.push("json"); break;
      case "--check-backedges": opts.checkBackedges = true; opts.modes.push("check"); break;
      default: throw new Error(`unknown flag: ${a}`);
    }
  }
  return opts;
}

/** Default mode: identity + stats + one line per claim status. */
function printSummary(g) {
  const id = g.identity;
  console.log(`head ${id.headSha}`);
  console.log(`tree HEAD:src ${id.srcTreeSha}`);
  console.log(`content sha256 ${id.contentSha256} (${id.fileCount} files)`);
  console.log(`src ${id.srcDir} generated ${id.generatedAtISO}`);
  const s = g.stats;
  console.log(
    `files ${s.files} | modules ${s.modules} | edges(full) ${s.edges} | edges(nested) ${s.nestedEdges} |` +
    ` edges(top) ${s.topEdges} | back-edges ${s.backEdges} | cycle-directions ${s.cycleEdges} |` +
    ` docRefs ${s.docRefs} | externUses ${g.externUses} | unresolved ${s.unresolvedRefs} |` +
    ` orphans ${s.orphans} | phantom ${s.phantom} | unmappedLayers ${s.unmappedLayers}`,
  );
  for (const c of g.claims) console.log(`${c.id}: ${c.status}`);
}

/** --level mode: one "source target count cfg" line per rollup edge. */
function printLevel(g, level) {
  const list = level === "top" ? g.topEdges : g.nestedEdges;
  for (const e of list) {
    const cfg = e.cfgOnly ? "¶" : (e.cfgs.filter(Boolean).join(";") || "-");
    console.log(`${e.source} ${e.target} ${e.count} ${cfg}`);
  }
}

function run() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (err) {
    console.error(`error: ${err.message}\n${USAGE}`);
    process.exit(2);
  }
  if (opts.modes.length > 1) {
    console.error(`error: pick at most one output mode (got: ${opts.modes.join(", ")})\n${USAGE}`);
    process.exit(2);
  }
  const g = buildModuleGraph({ srcDir: opts.src, configPath: opts.config, maxSites: opts.maxSites });
  if (opts.checkBackedges) {
    const n = g.backEdges.length;
    console.log(`baseline: ${n} back-edges (ratchet target: shrink)`);
    if (RATCHET_ENFORCED && n > 0) process.exit(1);
    return;
  }
  if (opts.json) {
    console.log(JSON.stringify(g, null, 1));
    return;
  }
  if (opts.level) {
    printLevel(g, opts.level);
    return;
  }
  if (opts.csvPath) {
    writeFileSync(opts.csvPath, renderCsv(g, { maxSites: opts.maxSites }));
    return;
  }
  if (opts.reportDir) {
    const { mdPath, csvPath } = writeReport(g, { outDir: opts.reportDir, maxSites: opts.maxSites });
    console.log(`wrote ${mdPath}`);
    console.log(`wrote ${csvPath}`);
    return;
  }
  printSummary(g);
}

try {
  run();
} catch (err) {
  console.error(`error: ${err?.stack ?? err}`);
  process.exit(1);
}
