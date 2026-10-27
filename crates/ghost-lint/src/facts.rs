//! Facts the rules read from one Rust source file: every fn (owner type, cfg
//! context, line span) and, inside it, every call, method call, macro,
//! assignment and `&mut` field borrow in source order, each with its block
//! nesting and the conditions of the `if`/`while`/`match` arms around it.

use proc_macro2::Span;
use quote::ToTokens;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;

/// Memory orderings recognised as call arguments.
pub const ORDERINGS: [&str; 5] = ["SeqCst", "AcqRel", "Acquire", "Release", "Relaxed"];

/// Argument names that pass an ordering through (`fn load(&self, order: Ordering)`).
const ORDERING_PARAMS: [&str; 6] = [
    "order",
    "ordering",
    "ord",
    "success",
    "failure",
    "fetch_order",
];

/// The ghost-check macros (`src/ghost/`).
pub const GHOST_MACROS: [&str; 2] = ["ghost_assert", "ghost_event"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Call,
    Method,
    Macro,
    Assign,
    MutBorrow,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub kind: EventKind,
    /// Method / fn / macro name, or the assigned / borrowed field.
    pub name: String,
    /// Receiver (method), path prefix (call) or base expression (field), whitespace removed.
    pub receiver: String,
    /// Top-level ordering arguments in order; `["param"]` when one is passed through.
    pub orderings: Vec<String>,
    /// Top-level arguments, whitespace removed (macro arguments for ghost macros).
    pub args: Vec<String>,
    pub line: usize,
    pub column: usize,
    /// Enclosing block ids, outermost first.
    pub blocks: Vec<usize>,
    /// Enclosing branch conditions, outermost first (`!(..)` for an else branch).
    pub guards: Vec<String>,
    /// Inside the arguments of a ghost macro.
    pub in_ghost: bool,
}

#[derive(Clone, Debug)]
pub struct FnFacts {
    /// `Owner::name` for methods (impl self type, generics dropped), `name` for free fns.
    pub path: String,
    /// cfg predicates of the fn and every enclosing impl / mod, joined with ` & `.
    pub cfg: String,
    pub start: usize,
    pub end: usize,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub owner: String,
    pub field: String,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub struct GhostPathUse {
    pub path: String,
    pub cfg: String,
    pub line: usize,
}

#[derive(Clone, Debug, Default)]
pub struct FileFacts {
    pub fns: Vec<FnFacts>,
    pub fields: Vec<FieldDecl>,
    /// `ghost::...` paths used outside a ghost macro, with their cfg context.
    pub ghost_paths: Vec<GhostPathUse>,
}

pub fn norm<T: ToTokens>(t: &T) -> String {
    t.to_token_stream()
        .to_string()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn line_of(span: Span) -> (usize, usize) {
    let s = span.start();
    (s.line, s.column)
}

fn cfg_of(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("cfg"))
        .filter_map(|a| a.meta.require_list().ok().map(|l| l.tokens.to_string()))
        .collect()
}

fn owner_of(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        syn::Type::Reference(r) => owner_of(&r.elem),
        other => norm(other),
    }
}

/// Ordering arguments of a call: literal variants, or `param` for a pass-through name.
fn orderings_of<'a>(args: impl Iterator<Item = &'a syn::Expr>) -> Vec<String> {
    let mut out = Vec::new();
    let mut param = false;
    for a in args {
        if let syn::Expr::Path(p) = a {
            if let Some(seg) = p.path.segments.last() {
                let id = seg.ident.to_string();
                if ORDERINGS.contains(&id.as_str()) {
                    out.push(id);
                } else if p.path.segments.len() == 1 && ORDERING_PARAMS.contains(&id.as_str()) {
                    param = true;
                }
            }
        }
    }
    if out.is_empty() && param {
        out.push("param".to_string());
    }
    out
}

struct Collector {
    facts: FileFacts,
    cfg_stack: Vec<Vec<String>>,
    owner: Option<String>,
    current: Option<FnFacts>,
    blocks: Vec<usize>,
    next_block: usize,
    guards: Vec<String>,
    ghost_depth: usize,
}

impl Collector {
    fn cfg_now(&self) -> String {
        self.cfg_stack
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join(" & ")
    }

    fn push(
        &mut self,
        kind: EventKind,
        name: String,
        receiver: String,
        orderings: Vec<String>,
        args: Vec<String>,
        span: Span,
    ) {
        let (line, column) = line_of(span);
        let ev = Event {
            kind,
            name,
            receiver,
            orderings,
            args,
            line,
            column,
            blocks: self.blocks.clone(),
            guards: self.guards.clone(),
            in_ghost: self.ghost_depth > 0,
        };
        if let Some(f) = self.current.as_mut() {
            f.events.push(ev);
        }
    }

    fn enter_fn(
        &mut self,
        attrs: &[syn::Attribute],
        sig: &syn::Signature,
        span: Span,
        body: &syn::Block,
    ) {
        self.cfg_stack.push(cfg_of(attrs));
        let name = sig.ident.to_string();
        let path = match &self.owner {
            Some(o) => format!("{o}::{name}"),
            None => name,
        };
        let saved = self.current.take();
        let saved_blocks = std::mem::take(&mut self.blocks);
        let saved_guards = std::mem::take(&mut self.guards);
        self.current = Some(FnFacts {
            path,
            cfg: self.cfg_now(),
            start: span.start().line,
            end: span.end().line,
            events: Vec::new(),
        });
        self.visit_block(body);
        let mut done = self.current.take().expect("fn facts");
        done.events.sort_by_key(|e| (e.line, e.column));
        self.facts.fns.push(done);
        self.current = saved;
        self.blocks = saved_blocks;
        self.guards = saved_guards;
        self.cfg_stack.pop();
    }

    fn visit_macro_tokens(&mut self, mac: &syn::Macro) {
        let name = mac
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        let parsed = mac
            .parse_body_with(Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated)
            .ok();
        let ghost = GHOST_MACROS.contains(&name.as_str());
        let args: Vec<String> = parsed
            .as_ref()
            .map(|p| p.iter().map(norm).collect())
            .unwrap_or_default();
        self.push(
            EventKind::Macro,
            name,
            String::new(),
            Vec::new(),
            args,
            mac.path.span(),
        );
        if let Some(p) = parsed {
            if ghost {
                self.ghost_depth += 1;
            }
            for e in p.iter() {
                self.visit_expr(e);
            }
            if ghost {
                self.ghost_depth -= 1;
            }
        }
    }
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        self.cfg_stack.push(cfg_of(&i.attrs));
        syn::visit::visit_item_mod(self, i);
        self.cfg_stack.pop();
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        self.cfg_stack.push(cfg_of(&i.attrs));
        let saved = self.owner.replace(owner_of(&i.self_ty));
        syn::visit::visit_item_impl(self, i);
        self.owner = saved;
        self.cfg_stack.pop();
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        self.cfg_stack.push(cfg_of(&i.attrs));
        let saved = self.owner.replace(i.ident.to_string());
        syn::visit::visit_item_trait(self, i);
        self.owner = saved;
        self.cfg_stack.pop();
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        let saved = self.owner.take();
        self.enter_fn(&i.attrs, &i.sig, i.span(), &i.block);
        self.owner = saved;
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        self.enter_fn(&i.attrs, &i.sig, i.span(), &i.block);
    }

    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        if let Some(b) = &i.default {
            self.enter_fn(&i.attrs, &i.sig, i.span(), b);
        }
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        for f in i.fields.iter() {
            if let Some(id) = &f.ident {
                self.facts.fields.push(FieldDecl {
                    owner: i.ident.to_string(),
                    field: id.to_string(),
                    line: id.span().start().line,
                });
            }
        }
        syn::visit::visit_item_struct(self, i);
    }

    fn visit_block(&mut self, b: &'ast syn::Block) {
        self.next_block += 1;
        self.blocks.push(self.next_block);
        syn::visit::visit_block(self, b);
        self.blocks.pop();
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        let ords = orderings_of(m.args.iter());
        self.push(
            EventKind::Method,
            m.method.to_string(),
            norm(&m.receiver),
            ords,
            m.args.iter().map(norm).collect(),
            m.method.span(),
        );
        syn::visit::visit_expr_method_call(self, m);
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*c.func {
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let name = segs.last().cloned().unwrap_or_default();
            let prefix = segs[..segs.len().saturating_sub(1)].join("::");
            self.push(
                EventKind::Call,
                name,
                prefix,
                orderings_of(c.args.iter()),
                c.args.iter().map(norm).collect(),
                p.span(),
            );
        }
        syn::visit::visit_expr_call(self, c);
    }

    fn visit_expr_macro(&mut self, m: &'ast syn::ExprMacro) {
        self.visit_macro_tokens(&m.mac);
    }

    fn visit_stmt_macro(&mut self, m: &'ast syn::StmtMacro) {
        self.visit_macro_tokens(&m.mac);
    }

    fn visit_expr_assign(&mut self, a: &'ast syn::ExprAssign) {
        if let syn::Expr::Field(f) = &*a.left {
            self.push(
                EventKind::Assign,
                norm(&f.member),
                norm(&f.base),
                Vec::new(),
                vec![norm(&a.right)],
                f.span(),
            );
        }
        syn::visit::visit_expr_assign(self, a);
    }

    fn visit_expr_binary(&mut self, b: &'ast syn::ExprBinary) {
        let compound = matches!(
            b.op,
            syn::BinOp::AddAssign(_)
                | syn::BinOp::SubAssign(_)
                | syn::BinOp::MulAssign(_)
                | syn::BinOp::DivAssign(_)
                | syn::BinOp::RemAssign(_)
                | syn::BinOp::BitXorAssign(_)
                | syn::BinOp::BitAndAssign(_)
                | syn::BinOp::BitOrAssign(_)
                | syn::BinOp::ShlAssign(_)
                | syn::BinOp::ShrAssign(_)
        );
        if compound {
            if let syn::Expr::Field(f) = &*b.left {
                self.push(
                    EventKind::Assign,
                    norm(&f.member),
                    norm(&f.base),
                    Vec::new(),
                    vec![norm(&b.right)],
                    f.span(),
                );
            }
        }
        syn::visit::visit_expr_binary(self, b);
    }

    fn visit_expr_reference(&mut self, r: &'ast syn::ExprReference) {
        if r.mutability.is_some() {
            if let syn::Expr::Field(f) = &*r.expr {
                self.push(
                    EventKind::MutBorrow,
                    norm(&f.member),
                    norm(&f.base),
                    Vec::new(),
                    Vec::new(),
                    f.span(),
                );
            }
        }
        syn::visit::visit_expr_reference(self, r);
    }

    fn visit_expr_if(&mut self, i: &'ast syn::ExprIf) {
        self.visit_expr(&i.cond);
        let cond = norm(&i.cond);
        self.guards.push(cond.clone());
        self.visit_block(&i.then_branch);
        self.guards.pop();
        if let Some((_, e)) = &i.else_branch {
            self.guards.push(format!("!({cond})"));
            self.visit_expr(e);
            self.guards.pop();
        }
    }

    fn visit_expr_while(&mut self, w: &'ast syn::ExprWhile) {
        self.visit_expr(&w.cond);
        self.guards.push(norm(&w.cond));
        self.visit_block(&w.body);
        self.guards.pop();
    }

    fn visit_expr_match(&mut self, m: &'ast syn::ExprMatch) {
        self.visit_expr(&m.expr);
        let scrut = norm(&m.expr);
        for arm in &m.arms {
            let mut g = format!("match{scrut}=>{}", norm(&arm.pat));
            if let Some((_, e)) = &arm.guard {
                self.visit_expr(e);
                g = format!("{g}if{}", norm(e));
            }
            self.guards.push(g);
            self.visit_expr(&arm.body);
            self.guards.pop();
        }
    }

    fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
        let segs: Vec<String> = p
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let is_ghost = (segs.len() > 1 && segs[0] == "ghost")
            || (segs.len() > 2 && segs[0] == "crate" && segs[1] == "ghost");
        if is_ghost && self.ghost_depth == 0 {
            let cfg = self.cfg_now();
            self.facts.ghost_paths.push(GhostPathUse {
                path: segs.join("::"),
                cfg,
                line: p.span().start().line,
            });
        }
        syn::visit::visit_expr_path(self, p);
    }
}

/// Parses `src` and collects its facts.
pub fn collect(src: &str) -> Result<FileFacts, String> {
    let file = syn::parse_file(src).map_err(|e| {
        format!(
            "parse error at {}:{}: {e}",
            e.span().start().line,
            e.span().start().column
        )
    })?;
    let mut c = Collector {
        facts: FileFacts::default(),
        cfg_stack: vec![cfg_of(&file.attrs)],
        owner: None,
        current: None,
        blocks: Vec::new(),
        next_block: 0,
        guards: Vec::new(),
        ghost_depth: 0,
    };
    c.visit_file(&file);
    Ok(c.facts)
}
