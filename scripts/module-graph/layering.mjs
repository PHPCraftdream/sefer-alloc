// Layering lint core for the A1 ratchet (scripts/module-graph.mjs is the thin
// CLI wiring around these functions). Pure functions only — importing this
// module prints nothing.
//
// The ratchet model: a committed baseline CSV (kind,path,count,reason) pins
// every known upward back-edge with its reference count. Counts may only
// shrink, and only via an explicit edit of the baseline file:
//   a) a NEW back-edge not in the baseline is RED;
//   b) a count GROWTH beyond the baseline count is RED;
//   c) a count drop is RED too — the baseline must be lowered by an explicit
//      edit (otherwise the base silently loosens);
//   d) a VANISHED edge with a stale baseline row is RED under the same rule (c).
//
// Conduits (see layers.json) are already exempt upstream: graph.backEdges
// carries no conduit-owned edges, and refs through a conduit resolve to the
// defining module, so they are counted against the defining module's layer.

import { readFileSync } from "node:fs";

const KIND_ORDER = { unmapped: 0, "new-edge": 1, growth: 2, ratchet: 3 };

/** Parse the baseline CSV tolerantly (CRLF, blanks, `#` comments, quoted
 * fields with `""` escapes). Throws with the offending line number. */
export function loadBaseline(path) {
  const text = readFileSync(path, "utf8").replace(/\r/g, "");
  const lines = text.split("\n");
  const rows = [];
  let seenHeader = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line === "" || line.startsWith("#")) continue;
    const fields = splitCsvLine(line);
    if (!seenHeader) {
      if (fields.join(",") !== "kind,path,count,reason") {
        throw new Error(`${path}: line ${i + 1}: expected header "kind,path,count,reason", got "${line}"`);
      }
      seenHeader = true;
      continue;
    }
    if (fields.length !== 4) {
      throw new Error(`${path}: line ${i + 1}: expected 4 fields, got ${fields.length}`);
    }
    const [kind, edge, count, reason] = fields;
    if (kind !== "backedge") {
      throw new Error(`${path}: line ${i + 1}: kind must be "backedge", got "${kind}"`);
    }
    const arrow = edge.indexOf("->");
    const src = arrow === -1 ? "" : edge.slice(0, arrow);
    const tgt = arrow === -1 ? "" : edge.slice(arrow + 2);
    if (src === "" || tgt === "") {
      throw new Error(`${path}: line ${i + 1}: edge must be "<source>-><target>" with non-empty sides, got "${edge}"`);
    }
    if (!/^\d+$/.test(count.trim())) {
      throw new Error(`${path}: line ${i + 1}: count must be a non-negative integer, got "${count}"`);
    }
    rows.push({ kind, edge, count: Number(count.trim()), reason });
  }
  if (!seenHeader) throw new Error(`${path}: missing header "kind,path,count,reason"`);
  return rows;
}

/** One CSV line -> fields; double-quoted fields may contain commas and
 * escaped (`""`) quotes; a quote may only open at a field start. */
function splitCsvLine(line) {
  const fields = [];
  let cur = "";
  let i = 0;
  while (i < line.length) {
    if (line[i] === '"' && cur === "") {
      i += 1;
      while (i < line.length) {
        if (line[i] === '"') {
          if (line[i + 1] === '"') { cur += '"'; i += 2; continue; }
          i += 1;
          break;
        }
        cur += line[i];
        i += 1;
      }
    } else if (line[i] === ",") {
      fields.push(cur);
      cur = "";
      i += 1;
    } else {
      cur += line[i];
      i += 1;
    }
  }
  fields.push(cur);
  return fields;
}

/** Compare the current back-edge set against the baseline. Pure function of
 * its inputs; returns findings sorted by kind then message. */
export function checkBackedges(graph, baselineRows, { maxSites = 5 } = {}) {
  const current = new Map(); // edgeId -> edge (from graph.backEdges, conduit-exempt)
  for (const e of graph.backEdges) current.set(e.source + "->" + e.target, e);
  const baseline = new Map(); // edgeId -> row
  for (const r of baselineRows) baseline.set(r.edge, r);

  const conduits = new Set((graph.config?.conduits ?? []).flatMap((c) => c.modules ?? []));

  const findings = [];
  const fmtSites = (edge) =>
    edge.sites.slice(0, maxSites).map((s) => `${s.file}:${s.line}`).join(", ");

  for (const [id, e] of current) {
    if (!baseline.has(id)) {
      findings.push({
        kind: "new-edge",
        edge: id,
        message: `[new-edge] ${e.source} -> ${e.target} ` +
          `(${e.sourceLayer.name}@${e.sourceLayer.order} -> ${e.targetLayer.name}@${e.targetLayer.order}): ` +
          `${e.count} refs, not in the baseline; sites: ${fmtSites(e)}`,
      });
      continue;
    }
    const b = baseline.get(id);
    if (e.count > b.count) {
      findings.push({
        kind: "growth",
        edge: id,
        message: `[growth] ${id}: baseline ${b.count} -> current ${e.count}; sites: ${fmtSites(e)}`,
      });
    } else if (e.count < b.count) {
      const sites = e.count > 0 ? `; sites: ${fmtSites(e)}` : "";
      findings.push({
        kind: "ratchet",
        edge: id,
        message: `[ratchet] ${id}: baseline ${b.count} -> current ${e.count}; ` +
          `lower the baseline (delete or reduce the row) — the base may only shrink via an explicit baseline edit${sites}`,
      });
    }
  }
  for (const [id, b] of baseline) {
    if (current.has(id)) continue;
    findings.push({
      kind: "ratchet",
      edge: id,
      message: `[ratchet] ${id}: baseline ${b.count} -> current 0; ` +
        `lower the baseline (delete or reduce the row) — the base may only shrink via an explicit baseline edit`,
    });
  }
  for (const m of graph.modules) {
    if (m.layer !== null || conduits.has(m.path)) continue;
    findings.push({
      kind: "unmapped",
      edge: m.path,
      message: `[unmapped] module \`${m.path}\` (${m.file}) is in no layer and not a declared conduit; ` +
        `add it to layers.json or declare it a conduit in layers.json`,
    });
  }
  return findings.sort((a, b) =>
    (KIND_ORDER[a.kind] ?? 99) - (KIND_ORDER[b.kind] ?? 99) || a.message.localeCompare(b.message));
}

/** The derived-by-script baseline text (committed rows come from this,
 * with the reason column filled in by hand). LF, trailing newline. */
export function emitBaseline(graph) {
  const rows = graph.backEdges
    .map((e) => ({ id: e.source + "->" + e.target, count: e.count }))
    .sort((a, b) => a.id.localeCompare(b.id))
    .map((r) => `backedge,${r.id},${r.count},`);
  return ["kind,path,count,reason", ...rows].join("\n") + "\n";
}
