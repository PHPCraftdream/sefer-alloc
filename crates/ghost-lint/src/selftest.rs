//! `ghost-lint selftest`: every fixture case proves its rule is not empty.
//!
//! A case is a directory under `tests/ghost_lint_fixtures/` holding `rule.json`,
//! `ok.rs` and one or more `mutant_*.rs`. `rule.json` is either a full rule or
//! `{"catalog": "<rule id>"}` (the catalog rule, retargeted at the fixture).
//! Every path in the rule that equals `$SRC` is replaced by the file under
//! test. `ok.rs` must produce no violation; every mutant must produce at least
//! one, all from this rule, and — if its first line is `// expect: <text>` —
//! one whose message contains `<text>`. Every catalog rule needs a case.

use crate::repo::Repo;
use crate::rules::check;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

const SRC: &str = "$SRC";
const FIXTURE_PATH: &str = "fixture/under_test.rs";

fn retarget(v: &Value) -> Value {
    match v {
        Value::String(s) if s == SRC => Value::String(FIXTURE_PATH.into()),
        Value::Array(a) => Value::Array(a.iter().map(retarget).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, x)| (k.clone(), retarget(x))).collect())
        }
        other => other.clone(),
    }
}

pub struct Outcome {
    pub cases: usize,
    pub mutants: usize,
    pub failures: Vec<String>,
}

fn run_one(rule: &Value, src: &str, rules: &[Value]) -> Vec<crate::rules::Violation> {
    let repo = Repo::from_memory(
        BTreeMap::from([(FIXTURE_PATH.to_string(), src.to_string())]),
        rules.to_vec(),
    );
    check(rule, &repo)
}

pub fn run(fixtures: &Path, catalog: &[Value]) -> Result<Outcome, String> {
    let mut out = Outcome {
        cases: 0,
        mutants: 0,
        failures: Vec::new(),
    };
    let mut covered = std::collections::BTreeSet::new();
    let mut dirs: Vec<_> = std::fs::read_dir(fixtures)
        .map_err(|e| format!("{}: {e}", fixtures.display()))?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let raw: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("rule.json"))
                .map_err(|e| format!("{name}/rule.json: {e}"))?,
        )
        .map_err(|e| format!("{name}/rule.json: {e}"))?;
        let rule = match raw.get("catalog").and_then(Value::as_str) {
            Some(id) => {
                covered.insert(id.to_string());
                let r = catalog
                    .iter()
                    .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
                    .ok_or(format!("{name}: catalog rule {id} not found"))?;
                let mut r = r.clone();
                for (k, x) in raw
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter(|(k, _)| k.as_str() != "catalog" && k.as_str() != "catalog_rules")
                {
                    r[k] = x.clone();
                }
                retarget(&r)
            }
            None => retarget(&raw),
        };
        let id = rule
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        out.cases += 1;
        let ok =
            std::fs::read_to_string(dir.join("ok.rs")).map_err(|e| format!("{name}/ok.rs: {e}"))?;
        // Rules R8 looks up: the catalog plus any the case brings (`catalog_rules`).
        let mut lookup: Vec<Value> = catalog.to_vec();
        lookup.extend(
            raw.get("catalog_rules")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        let vs = run_one(&rule, &ok, &lookup);
        if !vs.is_empty() {
            out.failures.push(format!(
                "{name}: ok.rs flagged: {}",
                vs.iter()
                    .map(|v| v.message.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        let mut mutants: Vec<_> = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("mutant_"))
            })
            .collect();
        mutants.sort();
        if mutants.is_empty() {
            out.failures.push(format!(
                "{name}: no mutant — the rule is not shown to reject anything"
            ));
        }
        for m in mutants {
            out.mutants += 1;
            let mname = m
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let text = std::fs::read_to_string(&m).map_err(|e| format!("{name}/{mname}: {e}"))?;
            let vs = run_one(&rule, &text, &lookup);
            if vs.is_empty() {
                out.failures.push(format!(
                    "{name}/{mname}: accepted (the rule did not reject the mutant)"
                ));
                continue;
            }
            if let Some(other) = vs.iter().find(|v| v.rule != id) {
                out.failures.push(format!(
                    "{name}/{mname}: rejected by {} instead of {id}",
                    other.rule
                ));
            }
            if let Some(exp) = text
                .lines()
                .next()
                .and_then(|l| l.strip_prefix("// expect: "))
            {
                if !vs.iter().any(|v| v.message.contains(exp.trim())) {
                    out.failures.push(format!(
                        "{name}/{mname}: no violation mentions {exp:?}; got {:?}",
                        vs.iter().map(|v| &v.message).collect::<Vec<_>>()
                    ));
                }
            }
        }
    }
    // Every catalog rule must be exercised by at least one case.
    for r in catalog {
        let id = r.get("id").and_then(Value::as_str).unwrap_or("?");
        if !covered.contains(id) {
            out.failures.push(format!(
                "catalog rule {id} has no fixture case ({{\"catalog\": \"{id}\"}})"
            ));
        }
    }
    Ok(out)
}
