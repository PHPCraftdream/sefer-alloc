//! Rule kinds R1–R10 (docs/GHOST_CHECKS_PLAN.md §4.2) evaluated over [`FileFacts`].
//! A rule names its place by file and fn path (`Owner::name`); a path that
//! matches no fn, or more than one without a `cfg` selector, is itself a
//! violation — an ambiguous catalog entry must not match silently.

use crate::facts::{Event, EventKind, FileFacts, FnFacts, GHOST_MACROS, ORDERINGS};
use regex::Regex;
use serde_json::Value;

/// One finding: the rule that produced it, where, and why.
#[derive(Debug, Clone)]
pub struct Violation {
    pub rule: String,
    pub file: String,
    pub line: usize,
    pub message: String,
}

/// Source access for rule evaluation: parsed facts and raw text by repo-relative path.
pub trait Sources {
    fn facts(&self, file: &str) -> Result<&FileFacts, String>;
    fn text(&self, file: &str) -> Result<&str, String>;
    /// Repo-relative files matching a glob (`*` within a segment, `**` across segments).
    fn glob(&self, pattern: &str) -> Vec<String>;
    /// A catalog rule by id (R8 binds a model to the orderings of an R1 rule).
    fn rule(&self, id: &str) -> Option<&Value>;
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn strs(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn re(pat: &str) -> Result<Regex, String> {
    Regex::new(pat).map_err(|e| format!("bad regex {pat:?}: {e}"))
}

/// An event selector: `name` (exact), optional `receiver` regex, `kind`, `orderings`,
/// `args` regex, `guard` regex (some enclosing branch condition matches) and
/// `unguarded` (no enclosing branch at all).
struct Pattern {
    name: Option<String>,
    receiver: Option<Regex>,
    kind: Option<EventKind>,
    orderings: Option<Vec<String>>,
    args: Option<Regex>,
    guard: Option<Regex>,
    unguarded: bool,
}

impl Pattern {
    fn from(v: &Value) -> Result<Pattern, String> {
        let kind = match s(v, "kind") {
            None => None,
            Some("call") => Some(EventKind::Call),
            Some("method") => Some(EventKind::Method),
            Some("macro") => Some(EventKind::Macro),
            Some("assign") => Some(EventKind::Assign),
            Some("mut_borrow") => Some(EventKind::MutBorrow),
            Some("return_if") => Some(EventKind::ReturnIf),
            Some(o) => return Err(format!("unknown event kind {o}")),
        };
        Ok(Pattern {
            name: s(v, "name").map(String::from),
            receiver: s(v, "receiver").map(re).transpose()?,
            kind,
            orderings: v.get("orderings").map(|_| strs(v, "orderings")),
            args: s(v, "args").map(re).transpose()?,
            guard: s(v, "guard").map(re).transpose()?,
            unguarded: v.get("unguarded").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    fn matches(&self, e: &Event) -> bool {
        self.name.as_ref().is_none_or(|n| *n == e.name)
            && self
                .receiver
                .as_ref()
                .is_none_or(|r| r.is_match(&e.receiver))
            && self.kind.is_none_or(|k| k == e.kind)
            && self.orderings.as_ref().is_none_or(|o| *o == e.orderings)
            && self
                .args
                .as_ref()
                .is_none_or(|r| r.is_match(&e.args.join(",")))
            && self
                .guard
                .as_ref()
                .is_none_or(|r| e.guards.iter().any(|g| r.is_match(g)))
            && (!self.unguarded || e.guards.is_empty())
    }
}

/// Files a scoped rule (R5, R9) covers: `files` globs minus `exclude` globs.
fn scoped_files(rule: &Value, src: &dyn Sources) -> Vec<String> {
    let exclude: Vec<Regex> = strs(rule, "exclude")
        .iter()
        .map(|g| crate::repo::glob_regex(g))
        .collect();
    let mut files: Vec<String> = strs(rule, "files")
        .iter()
        .flat_map(|g| src.glob(g))
        .filter(|f| !exclude.iter().any(|r| r.is_match(f)))
        .collect();
    files.sort();
    files.dedup();
    files
}

/// True for an event that carries a memory ordering (atomic op, fence or wrapper).
fn is_ordered(e: &Event) -> bool {
    !e.orderings.is_empty() && matches!(e.kind, EventKind::Method | EventKind::Call)
}

/// The single fn a rule targets: `fn` path, plus an optional `cfg` regex over its cfg context.
fn target<'a>(src: &'a dyn Sources, file: &str, place: &Value) -> Result<&'a FnFacts, String> {
    let path = s(place, "fn").ok_or("rule has no fn")?;
    let cfg = s(place, "cfg").map(re).transpose()?;
    let facts = src.facts(file)?;
    let hits: Vec<&FnFacts> = facts
        .fns
        .iter()
        .filter(|f| f.path == path && cfg.as_ref().is_none_or(|r| r.is_match(&f.cfg)))
        .collect();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err(format!("no fn {path} in {file}")),
        n => Err(format!(
            "{n} fns match {path} in {file} — add a cfg selector"
        )),
    }
}

/// Evaluates one rule; an Err is reported as a violation of that rule.
pub fn check(rule: &Value, src: &dyn Sources) -> Vec<Violation> {
    let id = s(rule, "id").unwrap_or("?").to_string();
    let file = s(rule, "file").unwrap_or("").to_string();
    match check_inner(rule, src) {
        Ok(v) => v,
        Err(message) => vec![Violation {
            rule: id,
            file,
            line: 0,
            message,
        }],
    }
}

fn v(rule: &Value, file: &str, line: usize, message: String) -> Violation {
    Violation {
        rule: s(rule, "id").unwrap_or("?").to_string(),
        file: file.to_string(),
        line,
        message,
    }
}

fn check_inner(rule: &Value, src: &dyn Sources) -> Result<Vec<Violation>, String> {
    let kind = s(rule, "kind").ok_or("rule has no kind")?;
    let file = s(rule, "file").unwrap_or("");
    let mut out = Vec::new();
    match kind {
        // R1: the ordering of a specific atomic operation in a specific fn.
        "R1" => {
            let f = target(src, file, rule)?;
            let site = Pattern::from(rule.get("site").ok_or("R1 needs site")?)?;
            let want = strs(rule, "orderings");
            let hits: Vec<&Event> = f
                .events
                .iter()
                .filter(|e| !e.in_ghost && site.matches(e))
                .collect();
            let count = rule.get("count").and_then(Value::as_u64).unwrap_or(1) as usize;
            if hits.len() != count {
                out.push(v(
                    rule,
                    file,
                    f.start,
                    format!(
                        "{}: expected {count} matching site(s), found {}",
                        f.path,
                        hits.len()
                    ),
                ));
            }
            for e in hits {
                if e.orderings != want {
                    out.push(v(
                        rule,
                        file,
                        e.line,
                        format!(
                            "{}: {}.{} ordering {:?}, rule requires {:?}",
                            f.path, e.receiver, e.name, e.orderings, want
                        ),
                    ));
                }
            }
        }
        // R2: the multiset of ordered operations in a fn equals the catalog table.
        "R2" => {
            let f = target(src, file, rule)?;
            let table = rule
                .get("sites")
                .and_then(Value::as_array)
                .ok_or("R2 needs sites")?;
            let mut pats: Vec<(Pattern, String)> = Vec::new();
            for t in table {
                pats.push((Pattern::from(t)?, t.to_string()));
            }
            let mut used = vec![false; pats.len()];
            for e in f.events.iter().filter(|e| !e.in_ghost && is_ordered(e)) {
                match pats
                    .iter()
                    .enumerate()
                    .position(|(i, (p, _))| !used[i] && p.matches(e))
                {
                    Some(i) => used[i] = true,
                    None => out.push(v(
                        rule,
                        file,
                        e.line,
                        format!(
                            "{}: ordered op {}.{}({:?}) not in the catalog table",
                            f.path, e.receiver, e.name, e.orderings
                        ),
                    )),
                }
            }
            for (i, (_, txt)) in pats.iter().enumerate() {
                if !used[i] {
                    out.push(v(
                        rule,
                        file,
                        f.start,
                        format!("{}: catalog site {txt} not found", f.path),
                    ));
                }
            }
        }
        // R3: every `after` event is preceded by a `before` event in the same or an enclosing block.
        "R3" => {
            let f = target(src, file, rule)?;
            let before = Pattern::from(rule.get("before").ok_or("R3 needs before")?)?;
            let after = Pattern::from(rule.get("after").ok_or("R3 needs after")?)?;
            let afters: Vec<&Event> = f
                .events
                .iter()
                .filter(|e| !e.in_ghost && after.matches(e))
                .collect();
            if afters.is_empty() {
                out.push(v(
                    rule,
                    file,
                    f.start,
                    format!("{}: anchor (after) not found", f.path),
                ));
            }
            for a in afters {
                let ok = f.events.iter().any(|b| {
                    !b.in_ghost
                        && before.matches(b)
                        && (b.line, b.column) < (a.line, a.column)
                        && a.blocks.starts_with(&b.blocks)
                });
                if !ok {
                    out.push(v(rule, file, a.line, format!("{}: {}.{} is not preceded by the required operation in a dominating position", f.path, a.receiver, a.name)));
                }
            }
        }
        // R4: every `action` event sits under a branch whose condition matches
        // `guard`, or after an early exit `if <c> { return }` (same or enclosing
        // block) whose condition matches `guard_return`.
        "R4" => {
            let f = target(src, file, rule)?;
            let action = Pattern::from(rule.get("action").ok_or("R4 needs action")?)?;
            let guard = re(s(rule, "guard").unwrap_or("$^"))?;
            let guard_return = s(rule, "guard_return").map(re).transpose()?;
            if s(rule, "guard").is_none() && guard_return.is_none() {
                return Err("R4 needs guard or guard_return".into());
            }
            let acts: Vec<&Event> = f
                .events
                .iter()
                .filter(|e| !e.in_ghost && action.matches(e))
                .collect();
            if acts.is_empty() {
                out.push(v(
                    rule,
                    file,
                    f.start,
                    format!("{}: anchor (action) not found", f.path),
                ));
            }
            for a in acts {
                let early = guard_return.as_ref().is_some_and(|gr| {
                    f.events.iter().any(|r| {
                        r.kind == EventKind::ReturnIf
                            && !r.in_ghost
                            && (r.line, r.column) < (a.line, a.column)
                            && a.blocks.starts_with(&r.blocks)
                            && r.args.first().is_some_and(|c| gr.is_match(c))
                    })
                });
                if !early && !a.guards.iter().any(|g| guard.is_match(g)) {
                    out.push(v(
                        rule,
                        file,
                        a.line,
                        format!(
                            "{}: {}.{} outside a branch matching {}",
                            f.path,
                            a.receiver,
                            a.name,
                            guard.as_str()
                        ),
                    ));
                }
            }
        }
        // R5: the target occurs only inside the allowed fns, across the scoped files.
        "R5" => {
            let target_pat = Pattern::from(rule.get("target").ok_or("R5 needs target")?)?;
            let allowed = strs(rule, "allowed");
            let files = scoped_files(rule, src);
            if files.is_empty() {
                return Err("R5 scope matches no file".into());
            }
            let mut seen = 0usize;
            for fl in &files {
                for f in &src.facts(fl)?.fns {
                    for e in f
                        .events
                        .iter()
                        .filter(|e| !e.in_ghost && target_pat.matches(e))
                    {
                        seen += 1;
                        if !allowed.contains(&f.path) {
                            out.push(v(
                                rule,
                                fl,
                                e.line,
                                format!(
                                    "{}.{} in {} — only {:?} may do this",
                                    e.receiver, e.name, f.path, allowed
                                ),
                            ));
                        }
                    }
                }
            }
            if seen == 0 {
                out.push(v(
                    rule,
                    file,
                    0,
                    "anchor (target) not found in scope".into(),
                ));
            }
        }
        // R6: the named fn carries a ghost macro with this rule's ghost id.
        "R6" => {
            let f = target(src, file, rule)?;
            let gid = s(rule, "ghost").ok_or("R6 needs ghost")?;
            let lit = format!("\"{gid}\"");
            let found = f.events.iter().any(|e| {
                e.kind == EventKind::Macro
                    && GHOST_MACROS.contains(&e.name.as_str())
                    && e.args.first() == Some(&lit)
            });
            if !found {
                out.push(v(
                    rule,
                    file,
                    f.start,
                    format!("{}: no ghost_assert!/ghost_event! with id {gid}", f.path),
                ));
            }
        }
        // R7: two twins have the same normalized sequence of ordered operations.
        "R7" => {
            let a = rule.get("a").ok_or("R7 needs a")?;
            let b = rule.get("b").ok_or("R7 needs b")?;
            let fa = target(src, s(a, "file").ok_or("R7 a.file")?, a)?;
            let fb = target(src, s(b, "file").ok_or("R7 b.file")?, b)?;
            let seq = |f: &FnFacts| -> Vec<String> {
                f.events
                    .iter()
                    .filter(|e| !e.in_ghost && is_ordered(e))
                    .map(|e| format!("{}({})", e.name, e.orderings.join("/")))
                    .collect()
            };
            let (sa, sb) = (seq(fa), seq(fb));
            let allowed: Vec<(usize, String)> = rule
                .get("allowed")
                .and_then(Value::as_array)
                .map(|xs| {
                    xs.iter()
                        .filter_map(|x| {
                            Some((
                                x.get("index")?.as_u64()? as usize,
                                s(x, "reason")?.to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if sa.len() != sb.len() {
                out.push(v(
                    rule,
                    s(a, "file").unwrap_or(""),
                    fa.start,
                    format!("twins differ in length: {sa:?} vs {sb:?}"),
                ));
            } else {
                for (i, (x, y)) in sa.iter().zip(&sb).enumerate() {
                    if x != y && !allowed.iter().any(|(j, r)| *j == i && !r.is_empty()) {
                        out.push(v(
                            rule,
                            s(a, "file").unwrap_or(""),
                            fa.start,
                            format!("twins differ at op {i}: {x} vs {y}"),
                        ));
                    }
                }
            }
        }
        // R8: every `// ghost-bind: <rule>` line in a model names the orderings of that R1 rule.
        "R8" => {
            let model = s(rule, "model").ok_or("R8 needs model")?;
            let text = src.text(model)?;
            let binds = rule
                .get("binds")
                .and_then(Value::as_array)
                .ok_or("R8 needs binds")?;
            let lines: Vec<&str> = text.lines().collect();
            let ord_re =
                Regex::new(&format!(r"\b({})\b", ORDERINGS.join("|"))).expect("static regex");
            for bnd in binds {
                let rid = s(bnd, "rule").ok_or("bind needs rule")?;
                let want = match bnd.get("orderings") {
                    Some(_) => strs(bnd, "orderings"),
                    None => {
                        let r1 = src
                            .rule(rid)
                            .ok_or(format!("bind names unknown rule {rid}"))?;
                        if s(r1, "kind") != Some("R1") {
                            return Err(format!("bind {rid} is not an R1 rule"));
                        }
                        strs(r1, "orderings")
                    }
                };
                let tag = format!("ghost-bind: {rid}");
                let at: Vec<usize> = lines
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.contains(&tag))
                    .map(|(i, _)| i)
                    .collect();
                if at.is_empty() {
                    out.push(v(rule, model, 0, format!("no `// {tag}` in {model}")));
                }
                for i in at {
                    // The bound code is the tag line itself or the next non-comment line.
                    let code_line = (i..lines.len().min(i + 4))
                        .find(|&j| ord_re.is_match(lines[j].split("//").next().unwrap_or("")))
                        .unwrap_or(i);
                    let code = lines[code_line].split("//").next().unwrap_or("");
                    let got: Vec<String> = ord_re
                        .find_iter(code)
                        .map(|m| m.as_str().to_string())
                        .collect();
                    if got != want {
                        out.push(v(
                            rule,
                            model,
                            code_line + 1,
                            format!("model binds {rid} with {got:?}, prod rule requires {want:?}"),
                        ));
                    }
                }
            }
        }
        // R10: a declaration keeps its shape — a struct field (`owner`, `field`) or a
        // static (`static`, also inside `thread_local!`) has a type matching `ty`,
        // an initializer matching `init`, and a type not matching `not_ty`.
        "R10" => {
            let d = rule.get("decl").ok_or("R10 needs decl")?;
            let facts = src.facts(file)?;
            let (what, line, ty, init) = if let Some(name) = s(d, "static") {
                let hits: Vec<_> = facts.statics.iter().filter(|x| x.name == name).collect();
                if hits.len() != 1 {
                    return Err(format!("{} statics named {name} in {file}", hits.len()));
                }
                let x = hits[0];
                if let Some(tl) = d.get("thread_local").and_then(Value::as_bool) {
                    if tl != x.thread_local {
                        out.push(v(
                            rule,
                            file,
                            x.line,
                            format!(
                                "static {name}: thread_local is {}, rule requires {tl}",
                                x.thread_local
                            ),
                        ));
                    }
                }
                (
                    format!("static {name}"),
                    x.line,
                    x.ty.clone(),
                    Some(x.init.clone()),
                )
            } else {
                let owner = s(d, "owner").ok_or("R10 decl needs static or owner")?;
                let field = s(d, "field").ok_or("R10 decl needs field")?;
                let hits: Vec<_> = facts
                    .fields
                    .iter()
                    .filter(|x| x.owner == owner && x.field == field)
                    .collect();
                if hits.len() != 1 {
                    return Err(format!("{} fields {owner}.{field} in {file}", hits.len()));
                }
                (
                    format!("{owner}.{field}"),
                    hits[0].line,
                    hits[0].ty.clone(),
                    None,
                )
            };
            if let Some(r) = s(rule, "ty").map(re).transpose()? {
                if !r.is_match(&ty) {
                    out.push(v(
                        rule,
                        file,
                        line,
                        format!("{what}: type {ty} does not match {}", r.as_str()),
                    ));
                }
            }
            if let Some(r) = s(rule, "not_ty").map(re).transpose()? {
                if r.is_match(&ty) {
                    out.push(v(
                        rule,
                        file,
                        line,
                        format!("{what}: type {ty} matches forbidden {}", r.as_str()),
                    ));
                }
            }
            if let Some(r) = s(rule, "init").map(re).transpose()? {
                match &init {
                    Some(i) if r.is_match(i) => {}
                    Some(i) => out.push(v(
                        rule,
                        file,
                        line,
                        format!("{what}: initializer {i} does not match {}", r.as_str()),
                    )),
                    None => return Err("R10 init applies to statics only".into()),
                }
            }
        }
        // R9: ghost hygiene — no ghost fields, ghost macro arguments do not write,
        // `ghost::` outside macros only under a ghost cfg.
        "R9" => {
            let writes = re(s(rule, "write_methods").unwrap_or(r"^(store|swap|fetch_\w+|compare_exchange\w*|write\w*|set_\w+|push\w*|insert|remove|take|replace|clear)$"))?;
            let ghost_cfg = re(s(rule, "ghost_cfg").unwrap_or(r"loom|miri|kani|sefer_ghost"))?;
            let files = scoped_files(rule, src);
            for fl in &files {
                let facts = src.facts(fl)?;
                for fd in facts
                    .fields
                    .iter()
                    .filter(|fd| fd.field.starts_with("ghost"))
                {
                    out.push(v(
                        rule,
                        fl,
                        fd.line,
                        format!("ghost field {}.{} in a prod struct", fd.owner, fd.field),
                    ));
                }
                for f in &facts.fns {
                    for e in f.events.iter().filter(|e| e.in_ghost) {
                        let bad = match e.kind {
                            EventKind::Assign | EventKind::MutBorrow => true,
                            EventKind::Method => writes.is_match(&e.name),
                            _ => false,
                        };
                        if bad {
                            out.push(v(
                                rule,
                                fl,
                                e.line,
                                format!(
                                    "{}: ghost macro argument writes ({}.{})",
                                    f.path, e.receiver, e.name
                                ),
                            ));
                        }
                    }
                }
                if !fl.contains("/ghost/") {
                    for g in facts
                        .ghost_paths
                        .iter()
                        .filter(|g| !ghost_cfg.is_match(&g.cfg))
                    {
                        out.push(v(
                            rule,
                            fl,
                            g.line,
                            format!("{} used outside a ghost macro without a ghost cfg", g.path),
                        ));
                    }
                }
            }
        }
        other => return Err(format!("unknown rule kind {other}")),
    }
    Ok(out)
}

/// Line spans of the places a rule protects in `file` (for the commit-trailer linter).
pub fn spans(rule: &Value, file: &str, src: &dyn Sources) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let kind = s(rule, "kind").unwrap_or("");
    let fn_span = |place: &Value, f: &str| target(src, f, place).ok().map(|x| (x.start, x.end));
    match kind {
        "R1" | "R2" | "R3" | "R4" | "R6" if s(rule, "file") == Some(file) => {
            out.extend(fn_span(rule, file))
        }
        "R7" => {
            for k in ["a", "b"] {
                if let Some(p) = rule.get(k) {
                    if s(p, "file") == Some(file) {
                        out.extend(fn_span(p, file));
                    }
                }
            }
        }
        "R5" => {
            if strs(rule, "files")
                .iter()
                .any(|g| src.glob(g).iter().any(|x| x == file))
            {
                if let (Ok(facts), Ok(p)) = (
                    src.facts(file),
                    Pattern::from(rule.get("target").unwrap_or(&Value::Null)),
                ) {
                    for f in &facts.fns {
                        if f.events.iter().any(|e| p.matches(e)) {
                            out.push((f.start, f.end));
                        }
                    }
                }
            }
        }
        "R10" if s(rule, "file") == Some(file) => {
            if let (Ok(facts), Some(d)) = (src.facts(file), rule.get("decl")) {
                let line = match s(d, "static") {
                    Some(n) => facts.statics.iter().find(|x| x.name == n).map(|x| x.line),
                    None => facts
                        .fields
                        .iter()
                        .find(|x| {
                            Some(x.owner.as_str()) == s(d, "owner")
                                && Some(x.field.as_str()) == s(d, "field")
                        })
                        .map(|x| x.line),
                };
                out.extend(line.map(|l| (l, l)));
            }
        }
        "R8" if s(rule, "model") == Some(file) => {
            if let Ok(text) = src.text(file) {
                for (i, l) in text.lines().enumerate() {
                    if l.contains("ghost-bind:") {
                        out.push((i + 1, i + 4));
                    }
                }
            }
        }
        _ => {}
    }
    out
}
