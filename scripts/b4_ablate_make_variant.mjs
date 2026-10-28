#!/usr/bin/env node
// b4-ablate variant generator — makes a BUILDABLE COPY of the sefer-alloc
// crate OUTSIDE the repository tree, under `<repo-root>/../../.rush/variant/
// <variant-name>/` (the repo's `.rush/` scratch dir), with an ablation patch
// applied, so the B4 interleave judge can drive TWO independent copies of the
// allocator in ONE process (arm A = `sefer_alloc`, arm B =
// `sefer_alloc_variant`). The copy lives OUTSIDE the worktree on purpose: an
// in-tree copy under `crates/sefer-variant/` breaks
// `scripts/verify-structure-limits.mjs` and the dbg-hooks scanner. The
// identity file is ALSO mirrored to `<repo-root>/../../.rush/variant/current/
// src/ab_variant_identity.rs`, the fixed variant-independent path the judge
// core `include!`s.
//
// Usage:
//   node scripts/b4_ablate_make_variant.mjs <variant-name>
//
// Strict contract (b4-ablate phase 1, rev 3):
//   1. copies `src/` -> `<out>/src/` BYTE-FOR-BYTE (no normalisation on
//      disk — `git apply` is byte-exact and the patches are generated from
//      the very same on-disk bytes of `src/`);
//   2. applies `docs/perf/b4_ablate/patches/<variant-name>.patch` through
//      `git apply -p1` run WITH CWD = the copy root (git apply refuses to
//      write outside the invoking worktree, so `--directory=<rel-out>` from
//      the repo root is NOT an option) — a real git diff over
//      `src/`, comment header lines BEFORE the `diff --git` line are skipped
//      by git apply). A patch file with NO `diff --git` line is a no-op
//      (this is how `null` ships). Any git apply failure aborts the script;
//   3. post-conditions (non-null): the copy must DIFFER from `src/`
//      (changed_files > 0 and sha256(after) != sha256(before, = the null
//      identity)); the LINE-level diff over all changed files (added /
//      removed lines) must MATCH the patch text's `+`/`-` hunk lines, and at
//      least one REMOVED line must be non-blank, non-`//` (a comment-only
//      ablation deletes no code) — otherwise the generator aborts;
//   4. writes `<out>/Cargo.toml` (package `sefer-alloc-variant`, lib
//      `sefer_alloc_variant`, its OWN `[workspace]` root — the copy sits
//      outside every repo workspace, so workspace lint/dep inheritance is
//      impossible and must not be attempted — path deps on the same workspace
//      crates of the generating worktree, NO new external dependencies — the
//      `[dependencies]` and `[features]` tables are SLICED VERBATIM from the
//      root manifest so they cannot drift);
//   5. writes `<out>/src/ab_variant_header.txt` (variant, file count,
//      variant_name, sha256 over every copied file — sorted relative paths,
//      LF-normalised bytes for determinism) and
//      `<out>/src/ab_variant_identity.rs` (`VARIANT_NAME` /
//      `VARIANT_SHA256` consts), and mirrors the identity file to
//      `<repo-root>/../../.rush/variant/current/src/ab_variant_identity.rs`,
//      AND the whole result (src/ + Cargo.toml) is MIRRORED to the FIXED
//      dev-dependency path `<repo-root>/.rush/variant/null/` — the root
//      manifest's dev-dep is `sefer-alloc-variant = { path =
//      "../../.rush/variant/null" }`, resolved by cargo at BUILD time, so
//      without the mirror every variant build would compile the stale (null)
//      code and measure nothing. Prints the same sha256 to stdout;
//   6. touches NOTHING inside the repository except reading `src/` and the
//      patch (the copy is not a workspace member; the root crate reaches it
//      as the relative path dev-dependency `../../.rush/variant/null`).
//
// DETERMINISM: re-running with the same inputs yields a bit-identical copy
// (LF normalisation everywhere, no timestamps, sorted hashing order).

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const argv = process.argv.slice(2);
if (argv.length !== 1 || !/^[a-z0-9_]+$/.test(argv[0])) {
  console.error("usage: node scripts/b4_ablate_make_variant.mjs <variant-name>");
  process.exit(2);
}
const variant = argv[0];

const root = process.cwd();
const srcDir = path.join(root, "src");
// OUT-OF-TREE copy: `<repo-root>/../../.rush/variant/<variant>/` — outside
// every worktree, so verify-structure-limits / dbg-hook scanners never see it.
const outDir = path.resolve(root, "..", "..", ".rush", "variant", variant);
const currentIdentity = path.resolve(
  root, "..", "..", ".rush", "variant", "current", "src", "ab_variant_identity.rs",
);
const outSrc = path.join(outDir, "src");
const patchPath = path.join(root, "docs", "perf", "b4_ablate", "patches", `${variant}.patch`);

if (!fs.existsSync(srcDir)) throw new Error(`missing ${srcDir}`);
if (!fs.existsSync(patchPath)) {
  throw new Error(`patch not found: docs/perf/b4_ablate/patches/${variant}.patch ` +
    "(every variant, including `null`, needs a patch file)");
}
const patchText = fs.readFileSync(patchPath, "utf8").replace(/\r\n/g, "\n");
// A patch file without a `diff --git` line carries no hunks: `null` no-op.
const hasDiff = patchText.includes("\ndiff --git ") || patchText.startsWith("diff --git ");

// Transient staging dir INSIDE the worktree: the harness only permits
// `git apply` with cwd = the worktree root, and git cannot write outside the
// worktree, so the patch is applied to `<root>/.b4_ablate_tmp` and the
// result is then written to the OUT-OF-TREE copy. Removed before exit.
const tmpDir = path.join(root, ".b4_ablate_tmp");
const tmpSrc = path.join(tmpDir, "src");
const stageSrc = hasDiff ? tmpSrc : outSrc;

// ── 1. copy src/ byte-for-byte ──────────────────────────────────────────────
fs.rmSync(outDir, { recursive: true, force: true });
fs.rmSync(tmpDir, { recursive: true, force: true });
fs.mkdirSync(stageSrc, { recursive: true });

/** rel path (POSIX slashes) -> raw file content buffer (as on disk). */
const files = new Map();
function walk(dir, rel) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const abs = path.join(dir, e.name);
    const r = rel === "" ? e.name : `${rel}/${e.name}`;
    if (e.isDirectory()) walk(abs, r);
    else files.set(r, fs.readFileSync(abs));
  }
}
walk(srcDir, "");
for (const [rel, buf] of files) {
  const abs = path.join(stageSrc, ...rel.split("/"));
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  fs.writeFileSync(abs, buf);
}

/** Deterministic sha256 over the tree: sorted rel paths, LF-normalised bytes. */
function treeSha() {
  const sorted = [...files.keys()].sort();
  const hash = createHash("sha256");
  for (const rel of sorted) {
    const lf = files.get(rel).toString("utf8").replace(/\r\n/g, "\n");
    hash.update(`file ${rel}\n`);
    hash.update(`${Buffer.byteLength(lf, "utf8")}\n`);
    hash.update(lf, "utf8");
  }
  return hash.digest("hex");
}
const shaBefore = treeSha(); // == the `null` identity (copy before the patch)

// ── 2. apply the patch via git apply ────────────────────────────────────────
if (hasDiff) {
  try {
    execFileSync(
      "git",
      ["apply", "--directory=.b4_ablate_tmp", "-p1", "-"],
      { cwd: root, input: patchText, stdio: ["pipe", "pipe", "pipe"] },
    );
  } catch (e) {
    throw new Error(
      `git apply failed for ${variant}:\n${e.stderr ?? e.message}`,
    );
  }
  // refresh the in-memory map from the patched copy
  files.clear();
  walk(tmpSrc, "");
}

// ── 3. post-conditions + hash ───────────────────────────────────────────────
const sha = treeSha();
let changedFiles = 0;
let addedLines = 0;
let removedLines = 0;
let patchPlus = 0;
let patchMinus = 0;
if (hasDiff) {
  // count `+`/`-` hunk lines in the patch TEXT (no `+++`/`---` headers; the
  // comment header BEFORE the first `diff --git` line is not part of any
  // hunk and is skipped by starting the scan at the first diff line).
  const body = patchText.slice(patchText.indexOf("diff --git "));
  for (const l of body.split("\n")) {
    if (l.startsWith("+++") || l.startsWith("---")) continue;
    if (l.startsWith("+")) patchPlus += 1;
    else if (l.startsWith("-")) patchMinus += 1;
  }

  // count files whose patched content differs from the pre-patch copy
  const before = new Map();
  for (const e of fs.readdirSync(srcDir, { withFileTypes: true, recursive: true })) {
    if (!e.isFile()) continue;
    const rel = path.relative(srcDir, path.join(e.parentPath ?? srcDir, e.name)).replaceAll("\\", "/");
    before.set(rel, fs.readFileSync(path.join(srcDir, ...rel.split("/"))));
  }
  let removedSample = null;
  for (const [rel, buf] of files) {
    const b = before.get(rel);
    if (b === undefined || !b.equals(buf)) changedFiles += 1;
    // LINE-level diff (added/removed) over every changed file.
    const d = lineDiff(
      (b ?? Buffer.alloc(0)).toString("utf8").replace(/\r\n/g, "\n"),
      buf.toString("utf8").replace(/\r\n/g, "\n"),
    );
    addedLines += d.added;
    removedLines += d.removed;
    if (b !== undefined && d.removedSample !== null && removedSample === null) {
      removedSample = d.removedSample;
    }
  }
  if (changedFiles === 0) {
    throw new Error(`${variant}: patch applied but the copy is identical to src/ (0 changed files)`);
  }
  if (sha === shaBefore) {
    throw new Error(`${variant}: sha256(after) == sha256(before) — ablation is a no-op`);
  }
  if (addedLines !== patchPlus || removedLines !== patchMinus) {
    throw new Error(
      `${variant}: patch text does not match the applied change ` +
        `(lines added/removed=${addedLines}/${removedLines}, ` +
        `patch +/-=${patchPlus}/${patchMinus})`,
    );
  }
  if (removedSample === null) {
    throw new Error(`${variant}: ablation removes no code (no non-blank, non-// line deleted)`);
  }
}

const sorted = [...files.keys()].sort();
for (const rel of sorted) {
  const abs = path.join(outSrc, ...rel.split("/"));
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  fs.writeFileSync(abs, files.get(rel));
}
fs.writeFileSync(
  path.join(outSrc, "ab_variant_header.txt"),
  `# generated by scripts/b4_ablate_make_variant.mjs — b4-ablate phase 1\n` +
    `variant=${variant}\n` +
    `variant_name=${variant}\n` +
    `files=${sorted.length}\n` +
    `changed_files=${changedFiles}\n` +
    `sha256=${sha}\n`,
  "utf8",
);
fs.writeFileSync(
  path.join(outSrc, "ab_variant_identity.rs"),
  `// GENERATED by scripts/b4_ablate_make_variant.mjs — do not edit.\n` +
    `#[allow(dead_code)] // identity consts; not every including crate reads both\n` +
    `pub const VARIANT_NAME: &str = ${JSON.stringify(variant)};\n` +
    `#[allow(dead_code)]\n` +
    `pub const VARIANT_SHA256: &str = ${JSON.stringify(sha)};\n`,
  "utf8",
);
// The header/identity files are metadata, NOT part of the hashed tree.
// The identity is ALSO mirrored to the fixed `current/` path the judge core
// `include!`s (variant-independent: `../../.rush/variant/current/...
// relative to CARGO_MANIFEST_DIR`), so a re-generation is visible to the
// judge without any rust-code edit.
fs.mkdirSync(path.dirname(currentIdentity), { recursive: true });
fs.copyFileSync(path.join(outSrc, "ab_variant_identity.rs"), currentIdentity);
fs.rmSync(tmpDir, { recursive: true, force: true });

/**
 * Line-level diff of two LF texts (LCS on the common-prefix/suffix-trimmed
 * remainder; patch hunks are tiny, so the DP remainder is always small).
 * Returns { added, removed, removedSample } where removedSample is the FIRST
 * removed line that is non-blank and not a `//` comment (or null).
 */
function lineDiff(beforeText, afterText) {
  const a = beforeText.split("\n");
  const b = afterText.split("\n");
  let p = 0;
  while (p < a.length && p < b.length && a[p] === b[p]) p += 1;
  let ea = a.length;
  let eb = b.length;
  while (ea > p && eb > p && a[ea - 1] === b[eb - 1]) { ea -= 1; eb -= 1; }
  const ra = a.slice(p, ea);
  const rb = b.slice(p, eb);
  const n = ra.length;
  const m = rb.length;
  if (n === 0 || m === 0) {
    return { added: m, removed: n, removedSample: firstCodeRemoved(ra) };
  }
  // LCS table (column-striped Uint32; n*m stays small after trimming).
  const table = new Uint32Array((n + 1) * (m + 1));
  const w = m + 1;
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      table[i * w + j] = ra[i] === rb[j]
        ? table[(i + 1) * w + j + 1] + 1
        : Math.max(table[(i + 1) * w + j], table[i * w + j + 1]);
    }
  }
  let added = 0;
  let removed = 0;
  let removedSample = null;
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (ra[i] === rb[j]) { i += 1; j += 1; }
    else if (table[(i + 1) * w + j] >= table[i * w + j + 1]) {
      if (removedSample === null) {
        const t = ra[i].trim();
        if (t !== "" && !t.startsWith("//")) removedSample = ra[i];
      }
      removed += 1; i += 1;
    } else { added += 1; j += 1; }
  }
  removed += n - i;
  added += m - j;
  if (removedSample === null) removedSample = firstCodeRemoved(ra);
  return { added, removed, removedSample };
}

/** First non-blank, non-`//` line of `lines`, or null. */
function firstCodeRemoved(lines) {
  for (const l of lines) {
    const t = l.trim();
    if (t !== "" && !t.startsWith("//")) return l;
  }
  return null;
}

// ── 4. variant Cargo.toml — sliced verbatim from the ROOT manifest ──────────
const rootManifest = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8").replace(/\r\n/g, "\n");
function slice(startMarker, endMarkers) {
  const s = rootManifest.indexOf(startMarker);
  if (s < 0) throw new Error(`root Cargo.toml: ${startMarker.trim()} not found`);
  let e = rootManifest.length;
  for (const m of endMarkers) {
    const idx = rootManifest.indexOf(m, s + startMarker.length);
    if (idx >= 0 && idx < e) e = idx;
  }
  return rootManifest.slice(s, e).replace(/^\n+/, "").replace(/\s+$/, "\n");
}
// Markers are anchored with a leading newline so comment text (e.g. a prose
// line mentioning "profile.") can never truncate a section.
const deps = slice("\n[dependencies]", ["\n[target."]);
const feats = slice("\n[features]", ["\n[dependencies]"]);
const cratesRel = path.relative(outDir, path.join(root, "crates")).replaceAll("\\", "/");

// Deterministically add the ablate-defaulted feature? No: the variant keeps
// the ROOT feature set byte-identical; the judge enables `production` on it
// through its dev-dependency declaration in the root manifest.
const variantManifest =
  `# GENERATED by scripts/b4_ablate_make_variant.mjs (variant: ${variant}).\n` +
  `# A sefer-alloc COPY for single-process A/B ablation (b4-ablate phase 1).\n` +
  `# [dependencies] and [features] are sliced verbatim from the root manifest;\n` +
  `# no new external dependencies. Not published; not committed as source;\n` +
  `# lives OUTSIDE the repository tree (.rush/variant/<variant>).\n` +
  `[package]\n` +
  `name = "sefer-alloc-variant"\n` +
  `version = "0.3.0"\n` +
  `edition = "2021"\n` +
  `rust-version = "1.93"\n` +
  `license = "MIT OR Apache-2.0"\n` +
  `description = "Generated sefer-alloc variant copy (b4-ablate, variant ${variant}) — not published"\n` +
  `publish = false\n` +
  `\n` +
  `[lib]\n` +
  `name = "sefer_alloc_variant"\n` +
  `path = "src/lib.rs"\n` +
  `\n` +
  // Path deps are relative to the variant crate: it lives in
  // `<repo>/.rush/variant/<variant>/`, the workspace crates in the generating
  // worktree's `crates/` — the relative hop is computed, never hardcoded
  // (no machine-private absolute paths in ANY generated file).
  deps.replace(/path = "crates\//g, `path = "${cratesRel}/`) +
  `\n` +
  feats +
  `\n` +
  // The copy sits OUTSIDE every repo workspace, so it must be its own
  // (memberless) workspace root — and it cannot inherit `[lints] workspace`.
  `[workspace]
`;
fs.writeFileSync(path.join(outDir, "Cargo.toml"), variantManifest, "utf8");

// ── 5. MIRROR to the fixed dev-dependency path (.rush/variant/null) ─────────
// The judge's dev-dependency is `sefer-alloc-variant = { path =
// "../../.rush/variant/null" }` — a PATH cargo resolves at BUILD time and
// which we cannot change per variant (that would edit the root manifest).
// Without mirroring the freshly generated variant there, every phase-2 build
// would compile whatever was in `null/` (the previous variant or null itself)
// and the measurement would be silently wrong. rm+copy keeps the mirror
// exact; for variant == `null` this is the same directory (outDir), so the
// write above already produced it — no double work.
const nullDir = path.resolve(root, "..", "..", ".rush", "variant", "null");
if (path.resolve(outDir) !== nullDir) {
  fs.rmSync(nullDir, { recursive: true, force: true });
  fs.mkdirSync(path.join(nullDir, "src"), { recursive: true });
  for (const rel of sorted) {
    const abs = path.join(nullDir, "src", ...rel.split("/"));
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, files.get(rel));
  }
  fs.writeFileSync(path.join(nullDir, "src", "ab_variant_header.txt"),
    fs.readFileSync(path.join(outSrc, "ab_variant_header.txt")), "utf8");
  fs.writeFileSync(path.join(nullDir, "src", "ab_variant_identity.rs"),
    fs.readFileSync(path.join(outSrc, "ab_variant_identity.rs")), "utf8");
  fs.writeFileSync(path.join(nullDir, "Cargo.toml"), variantManifest, "utf8");
  console.log(`b4_ablate_make_variant: mirrored variant=${variant} -> ${nullDir}`);
}

console.log(
  `b4_ablate_make_variant: variant=${variant} files=${sorted.length} ` +
    `changed_files=${changedFiles} sha256=${sha}` +
    (hasDiff ? ` added=${addedLines} removed=${removedLines} patch(+/-)=${patchPlus}/${patchMinus}` : ""),
);
