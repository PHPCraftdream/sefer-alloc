//! ghost-lint — per-site semantic lint rules for sefer-alloc (docs/GHOST_CHECKS_PLAN.md §4).
//!
//! ```text
//! ghost-lint check    [--root DIR] [--catalog FILE]
//! ghost-lint selftest [--root DIR] [--catalog FILE] [--fixtures DIR]
//! ghost-lint spans    [--root DIR] [--catalog FILE] --file REL [--stdin]
//! ```
//! `check` evaluates every catalog rule against the tree (exit 1 on any
//! violation); `selftest` runs the fixture cases (exit 1 unless every `ok.rs`
//! passes and every mutant is rejected by exactly its rule); `spans` prints
//! `<rule-id> <first-line> <last-line>` for each place in `REL` a rule protects,
//! reading the file from stdin with `--stdin` (any commit, no checkout).

mod facts;
mod repo;
mod rules;
mod selftest;

use repo::Repo;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

const CATALOG: &str = "docs/verification/ghost-rules.json";
const FIXTURES: &str = "tests/ghost_lint_fixtures";

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn load_rules(path: &PathBuf) -> Result<Vec<Value>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let rules = v
        .get("rules")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut ids = std::collections::BTreeSet::new();
    for r in &rules {
        let id = r
            .get("id")
            .and_then(Value::as_str)
            .ok_or("catalog rule without id")?;
        if !ids.insert(id.to_string()) {
            return Err(format!("duplicate rule id {id}"));
        }
    }
    Ok(rules)
}

fn real_main() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().cloned().unwrap_or_default();
    let root = PathBuf::from(arg(&args, "--root").unwrap_or_else(|| ".".into()));
    let catalog_path = root.join(arg(&args, "--catalog").unwrap_or_else(|| CATALOG.into()));
    let rules = load_rules(&catalog_path)?;
    match cmd.as_str() {
        "check" => {
            let repo = Repo::from_disk(&root, BTreeMap::new(), rules.clone());
            let mut n = 0;
            for r in &rules {
                for v in rules::check(r, &repo) {
                    println!("{}:{}: [{}] {}", v.file, v.line, v.rule, v.message);
                    n += 1;
                }
            }
            println!(
                "[ghost-lint] check: {} rule(s), {n} violation(s)",
                rules.len()
            );
            Ok(n == 0)
        }
        "selftest" => {
            let fx = root.join(arg(&args, "--fixtures").unwrap_or_else(|| FIXTURES.into()));
            let o = selftest::run(&fx, &rules)?;
            for f in &o.failures {
                println!("FAIL {f}");
            }
            println!(
                "[ghost-lint] selftest: {} case(s), {} mutant(s), {} failure(s)",
                o.cases,
                o.mutants,
                o.failures.len()
            );
            Ok(o.failures.is_empty() && o.cases > 0)
        }
        "spans" => {
            let file = arg(&args, "--file").ok_or("spans needs --file")?;
            let mut overrides = BTreeMap::new();
            if args.iter().any(|a| a == "--stdin") {
                let mut t = String::new();
                std::io::stdin()
                    .read_to_string(&mut t)
                    .map_err(|e| e.to_string())?;
                overrides.insert(file.clone(), t);
            }
            let repo = Repo::from_disk(&root, overrides, rules.clone());
            for r in &rules {
                let id = r.get("id").and_then(Value::as_str).unwrap_or("?");
                for (a, b) in rules::spans(r, &file, &repo) {
                    println!("{id} {a} {b}");
                }
            }
            Ok(true)
        }
        other => Err(format!(
            "unknown command {other:?} (check | selftest | spans)"
        )),
    }
}

fn main() -> ExitCode {
    match real_main() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("[ghost-lint] error: {e}");
            ExitCode::from(2)
        }
    }
}
