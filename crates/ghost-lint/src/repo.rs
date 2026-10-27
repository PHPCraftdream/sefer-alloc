//! Source access for the rules: repo-relative `.rs` files from disk (walked
//! once, parsed lazily) or from memory (self-test fixtures, `spans --stdin`).

use crate::facts::{collect, FileFacts};
use crate::rules::Sources;
use regex::Regex;
use serde_json::Value;
use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Directories walked for `.rs` files; everything else (target/, worktrees/, docs/) is out of scope.
const ROOTS: [&str; 5] = ["src", "crates", "tests", "examples", "benches"];

struct Slot {
    text: OnceCell<Result<String, String>>,
    facts: OnceCell<Result<FileFacts, String>>,
}

pub struct Repo {
    root: Option<PathBuf>,
    files: BTreeMap<String, Slot>,
    rules: Vec<Value>,
}

fn walk(root: &Path, rel: &str, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(root.join(rel)) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let child = format!("{rel}/{name}");
        match e.file_type() {
            Ok(t) if t.is_dir() && name != "target" => walk(root, &child, out),
            Ok(t) if t.is_file() && name.ends_with(".rs") => out.push(child),
            _ => {}
        }
    }
}

fn slot_with(text: Option<String>) -> Slot {
    let s = Slot {
        text: OnceCell::new(),
        facts: OnceCell::new(),
    };
    if let Some(t) = text {
        let _ = s.text.set(Ok(t));
    }
    s
}

/// `*` matches within one path segment, `**` across segments.
pub fn glob_regex(pattern: &str) -> Regex {
    let mut r = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                r.push_str(".*");
            }
            '*' => r.push_str("[^/]*"),
            c => r.push_str(&regex::escape(&c.to_string())),
        }
    }
    r.push('$');
    Regex::new(&r).expect("glob regex")
}

impl Repo {
    pub fn from_disk(root: &Path, overrides: BTreeMap<String, String>, rules: Vec<Value>) -> Repo {
        let mut paths = Vec::new();
        for r in ROOTS {
            walk(root, r, &mut paths);
        }
        let mut files: BTreeMap<String, Slot> =
            paths.into_iter().map(|p| (p, slot_with(None))).collect();
        for (p, t) in overrides {
            files.insert(p, slot_with(Some(t)));
        }
        Repo {
            root: Some(root.to_path_buf()),
            files,
            rules,
        }
    }

    pub fn from_memory(files: BTreeMap<String, String>, rules: Vec<Value>) -> Repo {
        Repo {
            root: None,
            files: files
                .into_iter()
                .map(|(p, t)| (p, slot_with(Some(t))))
                .collect(),
            rules,
        }
    }

    fn slot(&self, file: &str) -> Result<&Slot, String> {
        self.files
            .get(file)
            .ok_or_else(|| format!("file not found: {file}"))
    }
}

impl Sources for Repo {
    fn text(&self, file: &str) -> Result<&str, String> {
        let slot = self.slot(file)?;
        let t = slot.text.get_or_init(|| match &self.root {
            Some(root) => {
                std::fs::read_to_string(root.join(file)).map_err(|e| format!("{file}: {e}"))
            }
            None => Err(format!("{file}: not loaded")),
        });
        t.as_deref().map_err(Clone::clone)
    }

    fn facts(&self, file: &str) -> Result<&FileFacts, String> {
        let slot = self.slot(file)?;
        if let Some(done) = slot.facts.get() {
            return done.as_ref().map_err(Clone::clone);
        }
        let text = self.text(file)?;
        slot.facts
            .get_or_init(|| collect(text).map_err(|e| format!("{file}: {e}")))
            .as_ref()
            .map_err(Clone::clone)
    }

    fn glob(&self, pattern: &str) -> Vec<String> {
        let r = glob_regex(pattern);
        self.files
            .keys()
            .filter(|k| r.is_match(k))
            .cloned()
            .collect()
    }

    fn rule(&self, id: &str) -> Option<&Value> {
        self.rules
            .iter()
            .find(|r| r.get("id").and_then(Value::as_str) == Some(id))
    }
}
