//! The Missions Blueprint graph: a Lua function body as nodes on exec wires, its expressions as
//! data nodes and pins.
//!
//! [`lift`] turns a parsed script into a tree of canvases — the file's main chunk, and one canvas
//! under each `function` statement — and [`Graph::emit_body`] turns a canvas back into the AST
//! block it stands for. Every node keeps the AST piece it was lifted from as its template: its own
//! tokens (keywords, punctuation, and the whitespace and comments around them) come from the
//! template; everything that is a pin, a wire or a chain of statements comes from the graph. An
//! unedited graph therefore emits the bytes it was lifted from — the corpus gate in this module's
//! tests holds every script to that — and an edited one emits the edit with everything else as it
//! was.
//!
//! What the graph breaks down:
//! * statements — calls, local and global assignments, `if`, the loops, `do`, `return`, `break`,
//!   and `function` statements (each with a canvas of its own);
//! * expressions — literals (editable pins), locals and parameters (wires from the node that
//!   declares them; outer locals arrive as captured values on a canvas's entry), global paths,
//!   calls, tables, operators and parentheses. A table passed straight to a call is spread over
//!   the call's own pins, the way `self:CreateChild({...})` reads as an objective's fields;
//! * anonymous functions — an exec output on the node holding them, wired to their body, the
//!   way an objective's `fOnComplete` reads as where execution goes when it completes.
//!
//! Anything else is kept as the exact Lua it is: a statement as a Lua node, an expression that
//! indexes with `[...]` as a Lua pin.

use std::collections::HashMap;

use full_moon::ast::punctuated::{Pair, Punctuated};
use full_moon::ast::span::ContainedSpan;
use full_moon::ast::{
    AnonymousFunction, Assignment, BinOp, Block, Call, Do, ElseIf, Expression, Field, FunctionArgs,
    FunctionBody, FunctionCall, FunctionDeclaration, GenericFor, If, Index, LastStmt,
    LocalAssignment, LocalFunction, MethodCall, NumericFor, Parameter, Prefix, Repeat, Return,
    Stmt, Suffix, TableConstructor, UnOp, Var, VarExpression, While,
};
use full_moon::node::Node as AstNode;
use full_moon::tokenizer::{Symbol, Token, TokenReference, TokenType};
use full_moon::visitors::{VisitMut, VisitorMut};

use crate::lua_source::Script;

pub type NodeId = usize;

/// The entry node of every canvas: the function's parameters and captured values as outputs,
/// its body on the `body` exec output.
pub const ENTRY: NodeId = 0;

/// What a pin carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinTy {
    Exec,
    Str,
    Num,
    Bool,
    Obj,
    Arr,
    /// A value whose type the source does not show: a call's result, a parameter, `nil`.
    Any,
}

/// What a node is, for its colour, icon and eyebrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// A file's main chunk.
    Chunk,
    /// A function this script defines over one its base classes define.
    Lifecycle,
    /// A call to a base class's own version of a function (`MrxTaskContract.Activated(self)`).
    Base,
    /// A function this script defines, or a call to one.
    Fn,
    Vo,
    Objective,
    Event,
    Music,
    World,
    State,
    Tutorial,
    Util,
    Faction,
    Economy,
    Achievement,
    /// Any other call.
    Call,
    /// `if`, the loops, `do`, `return`, `break`.
    Flow,
    /// `local` and assignment.
    Variable,
    /// A table, an operator, parentheses.
    Data,
    /// A statement kept as its exact source.
    Lua,
}

/// The modules whose calls get a category of their own, with the category. These are the
/// script-side libraries the mission scripts drive the game through (each is a corpus script).
const MODULE_CATEGORIES: &[(&str, Category)] = &[
    ("MrxVoSequence", Category::Vo),
    ("MrxMusic", Category::Music),
    ("MrxLayerManager", Category::World),
    ("MrxState", Category::State),
    ("MrxTutorialManager", Category::Tutorial),
    ("MrxUtil", Category::Util),
    ("MrxFactionManager", Category::Faction),
    ("MrxPmc", Category::Economy),
    ("MrxSupport", Category::Economy),
    ("MrxAchievements", Category::Achievement),
];

/// One canvas.
#[derive(Clone, Debug)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub exec: Vec<ExecWire>,
    pub data: Vec<DataWire>,
}

/// From an exec output to the node it runs next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecWire {
    pub from: (NodeId, usize),
    pub to: NodeId,
}

/// From a node's output to another node's input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataWire {
    pub from: (NodeId, usize),
    pub to: (NodeId, usize),
}

#[derive(Clone, Debug)]
pub struct Node {
    pub category: Category,
    pub title: String,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    /// Statements take an exec input; data nodes and the entry do not.
    pub exec_in: bool,
    pub exec_outs: Vec<String>,
    /// Which exec output continues to the next statement, for statements that have one.
    pub next: Option<usize>,
    /// 1-based source line the node was lifted from.
    pub line: u32,
    /// The canvas of a `function` statement.
    pub sub: Option<Box<Graph>>,
    form: Form,
    closures: Vec<ClosureForm>,
    semi: Option<TokenReference>,
    /// The whitespace the statement's line starts with.
    indent: String,
    /// Made in the editor rather than lifted: emitted indented to the block it is wired into.
    fresh: bool,
    /// Outputs that are in scope after this statement: the locals it declares.
    declares: Vec<usize>,
    /// Outputs in scope only inside the block on an exec output: a loop's variables, an
    /// anonymous function's parameters.
    block_vars: Vec<(usize, Vec<usize>)>,
}

#[derive(Clone, Debug)]
pub struct Input {
    pub label: String,
    /// The argument a spread table's field came from, when a call spreads more than one.
    pub group: Option<String>,
    pub ty: PinTy,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub label: String,
    pub ty: PinTy,
    pub kind: OutKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutKind {
    /// A Lua variable this node declares: a local, a parameter, a loop variable.
    Var,
    /// A local of an enclosing function this canvas uses (only on the entry).
    Captured,
    /// A data node's result.
    Value,
}

/// What an input holds.
#[derive(Clone, Debug)]
pub enum Value {
    /// A string, number, `true`/`false`/`nil`/`...`, or a negated number.
    Literal(Expression),
    /// A global name or a dotted path rooted at one (`Event.ObjectDeath`).
    Path(Var),
    /// A list of literals (`{"loc_a", "loc_b"}`).
    Array(Box<TableConstructor>),
    /// Fed by a data wire. `site` is the variable's name token where the source used it, so the
    /// whitespace around it survives; `suffixes` index into what arrives (`self` + `.oRace`).
    Wired {
        site: Option<TokenReference>,
        suffixes: Vec<Suffix>,
    },
    /// Lua the graph keeps as its exact source.
    Lua(Lua),
}

#[derive(Clone, Debug)]
pub enum Lua {
    Expr(Expression),
    Var(Var),
}

/// Something wrong with a canvas that stops it emitting.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub node: Option<NodeId>,
    pub message: String,
}

// ───────────────────────────────────────────────────────────────────── templates

#[derive(Clone, Copy, Debug)]
enum SlotRef {
    In(usize),
    Closure(usize),
}

#[derive(Clone, Debug)]
struct ClosureForm {
    func: AnonymousFunction,
    exec: usize,
}

#[derive(Clone, Debug)]
enum Form {
    Entry,
    Call {
        binding: Binding,
        call: CallForm,
    },
    Set {
        binding: Binding,
        values: Vec<(SlotRef, Option<TokenReference>)>,
    },
    If {
        stmt: Box<If>,
        cond: SlotRef,
        then: usize,
        elseifs: Vec<(ElseIf, SlotRef, usize)>,
        otherwise: Option<usize>,
    },
    While {
        stmt: Box<While>,
        cond: SlotRef,
        body: usize,
    },
    Repeat {
        stmt: Box<Repeat>,
        until: SlotRef,
        body: usize,
    },
    NumericFor {
        stmt: Box<NumericFor>,
        start: SlotRef,
        end: SlotRef,
        step: Option<SlotRef>,
        body: usize,
    },
    GenericFor {
        stmt: Box<GenericFor>,
        exprs: Vec<(SlotRef, Option<TokenReference>)>,
        body: usize,
    },
    Do {
        stmt: Box<Do>,
        body: usize,
    },
    Function(FnDecl),
    Return {
        ret: Return,
        values: Vec<(SlotRef, Option<TokenReference>)>,
    },
    Break(TokenReference),
    Lua(Box<Stmt>),
    CallExpr(CallForm),
    Table(TableForm),
    BinOp {
        op: BinOp,
        lhs: SlotRef,
        rhs: SlotRef,
    },
    UnOp {
        op: UnOp,
        operand: SlotRef,
    },
    Paren {
        span: ContainedSpan,
        inner: SlotRef,
    },
}

#[derive(Clone, Debug)]
enum Binding {
    None,
    Local(Box<LocalAssignment>),
    Assign {
        stmt: Box<Assignment>,
        targets: Vec<(SlotRef, Option<TokenReference>)>,
    },
}

#[derive(Clone, Debug)]
enum FnDecl {
    Declaration(Box<FunctionDeclaration>),
    Local(Box<LocalFunction>),
    Assign(Box<Assignment>),
    LocalAssign(Box<LocalAssignment>),
}

#[derive(Clone, Debug)]
struct CallForm {
    head: HeadForm,
    /// The `.name` indexes between the head and the call.
    path: Vec<Suffix>,
    method: Option<Box<MethodCall>>,
    args: ArgsForm,
}

#[derive(Clone, Debug)]
enum HeadForm {
    /// A global name, part of the node's title.
    Global(TokenReference),
    Slot(SlotRef),
}

#[derive(Clone, Debug)]
enum ArgsForm {
    Parens {
        parens: ContainedSpan,
        args: Vec<(ArgForm, Option<TokenReference>)>,
    },
    String(SlotRef),
    Table(TableForm),
}

#[derive(Clone, Debug)]
enum ArgForm {
    Slot(SlotRef),
    Table(TableForm),
}

#[derive(Clone, Debug)]
struct TableForm {
    braces: ContainedSpan,
    fields: Vec<(FieldForm, Option<TokenReference>)>,
}

#[derive(Clone, Debug)]
enum FieldForm {
    Named {
        key: TokenReference,
        equal: TokenReference,
        value: SlotRef,
    },
    Keyed {
        brackets: ContainedSpan,
        key: Box<Expression>,
        equal: TokenReference,
        value: SlotRef,
    },
    Positional(SlotRef),
}

// ───────────────────────────────────────────────────────────────────── context

/// What lifting knows about the script beyond its own text: the functions of the modules it
/// calls (for argument names) and of the base classes it inherits (for overrides and `Super`).
#[derive(Clone, Debug)]
pub struct Context<'a> {
    index: &'a CorpusIndex,
    bank: String,
    stem: String,
    /// The script's file name, the chunk canvas's title.
    pub script: String,
    /// The inherit chain, nearest first, lower-case.
    pub bases: Vec<String>,
}

impl Context<'_> {
    fn module_fn(&self, module: &str, name: &str) -> Option<&Vec<String>> {
        self.index.resolve(module, &self.bank)?.functions.get(name)
    }

    fn own_fn(&self, name: &str) -> Option<&Vec<String>> {
        self.module_fn(&self.stem, name)
    }

    fn is_base(&self, module: &str) -> bool {
        let m = module.to_ascii_lowercase();
        self.bases.iter().any(|b| *b == m)
    }

    /// A function a base class defines: the definition an override replaces.
    fn base_fn(&self, name: &str) -> Option<&Vec<String>> {
        self.bases.iter().find_map(|b| self.module_fn(b, name))
    }
}

/// The function signatures and inherit lines of a whole corpus, from which a [`Context`] for
/// any one script is built.
#[derive(Clone, Debug, Default)]
pub struct CorpusIndex {
    /// Module (lower-case file stem) → one entry per bank the module ships in.
    modules: HashMap<String, Vec<ModuleInfo>>,
}

#[derive(Clone, Debug)]
struct ModuleInfo {
    bank: String,
    inherits: Option<String>,
    functions: HashMap<String, Vec<String>>,
}

impl CorpusIndex {
    /// Index `(path, source)` pairs, `path` as the corpus loader names it
    /// (`lua/vz/pmccon016.lua`). A script that does not parse is an error naming it.
    pub fn build<'a>(corpus: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<Self, String> {
        let mut modules: HashMap<String, Vec<ModuleInfo>> = HashMap::new();
        for (path, src) in corpus {
            let script = Script::parse(src).map_err(|e| format!("{path}: {e}"))?;
            let (bank, stem) = bank_and_stem(path);
            let functions = script
                .functions()
                .into_iter()
                .map(|f| {
                    let mut params = f.params;
                    if f.name.contains(':') {
                        params.insert(0, "self".into());
                    }
                    let name = f.name.rsplit(['.', ':']).next().unwrap_or(&f.name).to_string();
                    (name, params)
                })
                .collect();
            modules.entry(stem).or_default().push(ModuleInfo {
                bank,
                inherits: script.inherits(),
                functions,
            });
        }
        Ok(CorpusIndex { modules })
    }

    /// The module a script in `bank` sees under `name`: its own bank's copy, else the resident
    /// bank's, which every level mounts.
    fn resolve(&self, name: &str, bank: &str) -> Option<&ModuleInfo> {
        let all = self.modules.get(&name.to_ascii_lowercase())?;
        all.iter()
            .find(|m| m.bank == bank)
            .or_else(|| all.iter().find(|m| m.bank == "resident"))
    }

    /// The module the script at `path` inherits, as its `inherit()` call writes it.
    pub fn inherits(&self, path: &str) -> Option<&str> {
        let (bank, stem) = bank_and_stem(path);
        self.resolve(&stem, &bank)?.inherits.as_deref()
    }

    /// The context for lifting the script at `path`.
    pub fn context(&self, path: &str) -> Context<'_> {
        let (bank, stem) = bank_and_stem(path);
        let mut bases = Vec::new();
        let mut cur = self.resolve(&stem, &bank).and_then(|m| m.inherits.clone());
        while let Some(base) = cur {
            let key = base.to_ascii_lowercase();
            if bases.contains(&key) {
                break;
            }
            cur = self.resolve(&key, &bank).and_then(|m| m.inherits.clone());
            bases.push(key);
        }
        let script = path.rsplit('/').next().unwrap_or(path).to_string();
        Context {
            index: self,
            bank,
            stem,
            script,
            bases,
        }
    }
}

/// `lua/vz/pmccon016.lua` → (`vz`, `pmccon016`); `dlc/dlc01/x.lua` → (`dlc01`, `x`).
fn bank_and_stem(path: &str) -> (String, String) {
    let mut parts = path.rsplit('/');
    let file = parts.next().unwrap_or(path);
    let bank = parts.next().unwrap_or("").to_string();
    let stem = file.strip_suffix(".lua").unwrap_or(file).to_ascii_lowercase();
    (bank, stem)
}

// ───────────────────────────────────────────────────────────────────── lifting

/// Lift a script into its chunk canvas; `function` statements carry their own canvases.
pub fn lift(script: &Script, ctx: &Context<'_>) -> Graph {
    let entry = Node::new(Category::Chunk, ctx.script.clone(), 1, false, Form::Entry);
    let mut l = Lifter::new(ctx, Vec::new(), entry, true);
    l.g.nodes[ENTRY].exec_outs.push("body".into());
    l.block(script.ast().nodes(), (ENTRY, 0));
    l.g
}

impl Node {
    fn new(category: Category, title: String, line: u32, exec_in: bool, form: Form) -> Node {
        Node {
            category,
            title,
            inputs: Vec::new(),
            outputs: Vec::new(),
            exec_in,
            exec_outs: Vec::new(),
            next: None,
            line,
            sub: None,
            form,
            closures: Vec::new(),
            semi: None,
            indent: String::new(),
            fresh: false,
            declares: Vec::new(),
            block_vars: Vec::new(),
        }
    }

    /// Made in the editor rather than lifted from the script.
    pub fn is_new(&self) -> bool {
        self.fresh
    }

    /// What a call node calls, as written: `MrxMusic.PlaySpecialMusic`, `self:CreateChild`.
    pub fn callee(&self) -> Option<String> {
        let call = match &self.form {
            Form::Call { call, .. } | Form::CallExpr(call) => call,
            _ => return None,
        };
        let mut s = match &call.head {
            HeadForm::Global(t) => t.token().to_string(),
            HeadForm::Slot(SlotRef::In(i)) => self.inputs[*i].label.clone(),
            HeadForm::Slot(SlotRef::Closure(_)) => "function".into(),
        };
        for p in &call.path {
            s.push_str(&bare(p));
        }
        if let Some(m) = &call.method {
            s.push(':');
            s.push_str(&m.name().token().to_string());
        }
        Some(s)
    }

    /// Whether the call is made on a global (a module or engine table) rather than a local.
    pub fn calls_global(&self) -> bool {
        matches!(&self.form, Form::Call { call, .. } | Form::CallExpr(call) if matches!(call.head, HeadForm::Global(_)))
    }

    /// The pin indices that are part of an assignment's targets (`self.oRace = …`), not the call.
    pub fn is_target(&self, pin: usize) -> bool {
        match &self.form {
            Form::Call {
                binding: Binding::Assign { targets, .. },
                ..
            }
            | Form::Set {
                binding: Binding::Assign { targets, .. },
                ..
            } => targets.iter().any(|(s, _)| matches!(s, SlotRef::In(i) if *i == pin)),
            _ => false,
        }
    }

    /// Whether this is the canvas of a `function` statement, opened rather than edited in place.
    pub fn is_function(&self) -> bool {
        matches!(self.form, Form::Function(_))
    }
}

struct Lifter<'c> {
    ctx: &'c Context<'c>,
    g: Graph,
    frames: Vec<Vec<(String, (NodeId, usize))>>,
    /// Names visible from the enclosing canvases.
    outer: Vec<String>,
    /// Lifting the chunk's own top-level statements.
    chunk: bool,
}

impl<'c> Lifter<'c> {
    fn new(ctx: &'c Context<'c>, outer: Vec<String>, entry: Node, chunk: bool) -> Self {
        Lifter {
            ctx,
            g: Graph {
                nodes: vec![entry],
                exec: Vec::new(),
                data: Vec::new(),
            },
            frames: vec![Vec::new()],
            outer,
            chunk,
        }
    }

    fn add(&mut self, n: Node) -> NodeId {
        self.g.nodes.push(n);
        self.g.nodes.len() - 1
    }

    fn declare(&mut self, name: String, at: (NodeId, usize)) {
        self.frames.last_mut().expect("a frame").push((name, at));
    }

    fn output(&mut self, id: NodeId, label: String, ty: PinTy, kind: OutKind) -> usize {
        self.g.nodes[id].outputs.push(Output { label, ty, kind });
        self.g.nodes[id].outputs.len() - 1
    }

    fn exec_out(&mut self, id: NodeId, label: &str) -> usize {
        self.g.nodes[id].exec_outs.push(label.into());
        self.g.nodes[id].exec_outs.len() - 1
    }

    fn next(&mut self, id: NodeId) {
        let n = self.exec_out(id, "next");
        self.g.nodes[id].next = Some(n);
    }

    /// The node and output a variable named `name` comes from, if it is a local here. A local
    /// of an enclosing function becomes a captured output on this canvas's entry.
    fn lookup(&mut self, name: &str) -> Option<(NodeId, usize)> {
        for frame in self.frames.iter().rev() {
            if let Some((_, at)) = frame.iter().rev().find(|(n, _)| n == name) {
                return Some(*at);
            }
        }
        if !self.outer.iter().any(|o| o == name) {
            return None;
        }
        let entry = &self.g.nodes[ENTRY];
        if let Some(i) = entry
            .outputs
            .iter()
            .position(|o| o.kind == OutKind::Captured && o.label == name)
        {
            return Some((ENTRY, i));
        }
        Some((ENTRY, self.output(ENTRY, name.into(), PinTy::Any, OutKind::Captured)))
    }

    fn visible(&self) -> Vec<String> {
        let mut v = self.outer.clone();
        for frame in &self.frames {
            v.extend(frame.iter().map(|(n, _)| n.clone()));
        }
        v
    }

    fn ty_of(&self, (id, out): (NodeId, usize)) -> PinTy {
        self.g.nodes[id].outputs[out].ty
    }

    // ── statements ──────────────────────────────────────────────────────────────────────

    /// Lift `b`'s statements as a chain from exec output `from`. The caller owns the frame.
    fn block(&mut self, b: &Block, from: (NodeId, usize)) {
        let mut cur = from;
        let chunk = std::mem::replace(&mut self.chunk, false);
        for (stmt, semi) in b.stmts_with_semicolon() {
            self.chunk = chunk;
            let id = self.stmt(stmt);
            self.chunk = false;
            self.g.nodes[id].semi = semi.clone();
            self.g.nodes[id].indent = indent_of(stmt);
            self.g.exec.push(ExecWire { from: cur, to: id });
            cur = (id, self.g.nodes[id].next.expect("a statement continues"));
        }
        if let Some((last, semi)) = b.last_stmt_with_semicolon() {
            let id = self.last(last);
            self.g.nodes[id].semi = semi.clone();
            self.g.nodes[id].indent = indent_of(last);
            self.g.exec.push(ExecWire { from: cur, to: id });
        }
    }

    fn scoped_block(&mut self, b: &Block, from: (NodeId, usize)) {
        self.frames.push(Vec::new());
        self.block(b, from);
        self.frames.pop();
    }

    fn stmt(&mut self, s: &Stmt) -> NodeId {
        let line = line_of(s);
        match s {
            Stmt::FunctionCall(fc) if chain_is_liftable(fc.prefix(), &suffixes_of_call(fc)) => {
                let id = self.add(Node::new(Category::Call, String::new(), line, true, Form::Entry));
                let call = self.call_form(id, fc.prefix(), &suffixes_of_call(fc));
                self.finish_call(id, Binding::None, call);
                id
            }
            Stmt::LocalAssignment(la) => self.local_assignment(la, line),
            Stmt::Assignment(a) => self.assignment(a, line),
            Stmt::If(i) => self.if_stmt(i, line),
            Stmt::While(w) => {
                let id = self.add(Node::new(Category::Flow, "while".into(), line, true, Form::Entry));
                let cond = self.slot(id, "while".into(), None, w.condition());
                let body = self.exec_out(id, "body");
                self.scoped_block(w.block(), (id, body));
                self.g.nodes[id].form = Form::While {
                    stmt: w.clone(),
                    cond,
                    body,
                };
                self.next(id);
                id
            }
            Stmt::Repeat(r) => {
                let id = self.add(Node::new(Category::Flow, "repeat".into(), line, true, Form::Entry));
                let body = self.exec_out(id, "body");
                self.frames.push(Vec::new());
                self.block(r.block(), (id, body));
                let until = self.slot(id, "until".into(), None, r.until());
                self.frames.pop();
                self.g.nodes[id].form = Form::Repeat {
                    stmt: r.clone(),
                    until,
                    body,
                };
                self.next(id);
                id
            }
            Stmt::NumericFor(f) => {
                let var = f.index_variable().token().to_string();
                let id = self.add(Node::new(Category::Flow, format!("for {var}"), line, true, Form::Entry));
                let start = self.slot(id, "from".into(), None, f.start());
                let end = self.slot(id, "to".into(), None, f.end());
                let step = f.step().map(|s| self.slot(id, "step".into(), None, s));
                let body = self.exec_out(id, "body");
                self.frames.push(Vec::new());
                let out = self.output(id, var.clone(), PinTy::Num, OutKind::Var);
                self.declare(var, (id, out));
                self.g.nodes[id].block_vars.push((body, vec![out]));
                self.block(f.block(), (id, body));
                self.frames.pop();
                self.g.nodes[id].form = Form::NumericFor {
                    stmt: f.clone(),
                    start,
                    end,
                    step,
                    body,
                };
                self.next(id);
                id
            }
            Stmt::GenericFor(f) => {
                let names: Vec<String> = f.names().iter().map(|t| t.token().to_string()).collect();
                let id = self.add(Node::new(
                    Category::Flow,
                    format!("for {}", names.join(", ")),
                    line,
                    true,
                    Form::Entry,
                ));
                let mut exprs = Vec::new();
                for (i, p) in f.expressions().pairs().enumerate() {
                    let label = if i == 0 { "in".to_string() } else { format!("in {}", i + 1) };
                    exprs.push((self.slot(id, label, None, p.value()), p.punctuation().cloned()));
                }
                let body = self.exec_out(id, "body");
                self.frames.push(Vec::new());
                let mut outs = Vec::new();
                for n in names {
                    let out = self.output(id, n.clone(), PinTy::Any, OutKind::Var);
                    self.declare(n, (id, out));
                    outs.push(out);
                }
                self.g.nodes[id].block_vars.push((body, outs));
                self.block(f.block(), (id, body));
                self.frames.pop();
                self.g.nodes[id].form = Form::GenericFor {
                    stmt: f.clone(),
                    exprs,
                    body,
                };
                self.next(id);
                id
            }
            Stmt::Do(d) => {
                let id = self.add(Node::new(Category::Flow, "do".into(), line, true, Form::Entry));
                let body = self.exec_out(id, "body");
                self.scoped_block(d.block(), (id, body));
                self.g.nodes[id].form = Form::Do {
                    stmt: d.clone(),
                    body,
                };
                self.next(id);
                id
            }
            Stmt::FunctionDeclaration(f) => {
                let n = f.name();
                let mut name = n
                    .names()
                    .pairs()
                    .map(|p| {
                        let mut s = p.value().token().to_string();
                        if let Some(dot) = p.punctuation() {
                            s.push_str(&dot.token().to_string());
                        }
                        s
                    })
                    .collect::<String>();
                let method = n.method_name().is_some();
                if let Some(m) = n.method_name() {
                    name.push(':');
                    name.push_str(&m.token().to_string());
                }
                let id = self.add(Node::new(Category::Fn, name.clone(), line, true, Form::Entry));
                let sub = self.canvas(&name, method, f.body(), line);
                self.function_node(id, &name, sub, FnDecl::Declaration(f.clone()));
                id
            }
            Stmt::LocalFunction(f) => {
                let name = f.name().token().to_string();
                let id = self.add(Node::new(Category::Fn, name.clone(), line, true, Form::Entry));
                let out = self.output(id, name.clone(), PinTy::Obj, OutKind::Var);
                self.declare(name.clone(), (id, out));
                self.g.nodes[id].declares.push(out);
                let sub = self.canvas(&name, false, f.body(), line);
                self.function_node(id, &name, sub, FnDecl::Local(f.clone()));
                id
            }
            other => self.lua_stmt(other.clone(), line),
        }
    }

    fn lua_stmt(&mut self, s: Stmt, line: u32) -> NodeId {
        let title = first_line(&bare(&s));
        let id = self.add(Node::new(Category::Lua, title, line, true, Form::Lua(Box::new(s))));
        self.next(id);
        id
    }

    fn last(&mut self, last: &LastStmt) -> NodeId {
        let line = last.start_position().map(|p| p.line() as u32).unwrap_or(0);
        match last {
            LastStmt::Return(r) => {
                let id = self.add(Node::new(Category::Flow, "return".into(), line, true, Form::Entry));
                let mut values = Vec::new();
                for (i, p) in r.returns().pairs().enumerate() {
                    let label = if r.returns().len() == 1 {
                        "value".to_string()
                    } else {
                        format!("value {}", i + 1)
                    };
                    values.push((self.slot(id, label, None, p.value()), p.punctuation().cloned()));
                }
                self.g.nodes[id].form = Form::Return {
                    ret: r.clone(),
                    values,
                };
                id
            }
            LastStmt::Break(t) => {
                self.add(Node::new(Category::Flow, "break".into(), line, true, Form::Break(t.clone())))
            }
            other => panic!("Lua 5.1/5.2 has no last statement of the form {other}"),
        }
    }

    fn if_stmt(&mut self, i: &If, line: u32) -> NodeId {
        let id = self.add(Node::new(Category::Flow, "if".into(), line, true, Form::Entry));
        let cond = self.slot(id, "if".into(), None, i.condition());
        let then = self.exec_out(id, "then");
        self.scoped_block(i.block(), (id, then));
        let mut elseifs = Vec::new();
        for (n, e) in i.else_if().into_iter().flatten().enumerate() {
            let label = format!("elseif {}", n + 1);
            let c = self.slot(id, label.clone(), None, e.condition());
            let out = self.exec_out(id, &label);
            self.scoped_block(e.block(), (id, out));
            elseifs.push((e.clone(), c, out));
        }
        let otherwise = i.else_block().map(|b| {
            let out = self.exec_out(id, "else");
            self.scoped_block(b, (id, out));
            out
        });
        self.g.nodes[id].form = Form::If {
            stmt: Box::new(i.clone()),
            cond,
            then,
            elseifs,
            otherwise,
        };
        self.next(id);
        id
    }

    fn local_assignment(&mut self, la: &LocalAssignment, line: u32) -> NodeId {
        let names: Vec<String> = la.names().iter().map(|t| t.token().to_string()).collect();
        let exprs: Vec<&Expression> = la.expressions().iter().collect();
        if let ([name], [Expression::Function(_)], true) = (names.as_slice(), exprs.as_slice(), self.chunk) {
            let id = self.add(Node::new(Category::Fn, name.clone(), line, true, Form::Entry));
            let out = self.output(id, name.clone(), PinTy::Obj, OutKind::Var);
            self.declare(name.clone(), (id, out));
            self.g.nodes[id].declares.push(out);
            let Expression::Function(f) = exprs[0] else { unreachable!() };
            let sub = self.canvas(name, false, f.body(), line);
            let name = name.clone();
            self.function_node(id, &name, sub, FnDecl::LocalAssign(Box::new(la.clone())));
            return id;
        }
        let single_call = match exprs.as_slice() {
            [Expression::FunctionCall(fc)] if chain_is_liftable(fc.prefix(), &suffixes_of_call(fc)) => Some(fc),
            _ => None,
        };
        let mut value_tys = Vec::new();
        let id = if let Some(fc) = single_call {
            let id = self.add(Node::new(Category::Call, String::new(), line, true, Form::Entry));
            let call = self.call_form(id, fc.prefix(), &suffixes_of_call(fc));
            self.finish_call(id, Binding::Local(Box::new(la.clone())), call);
            id
        } else {
            let id = self.add(Node::new(
                Category::Variable,
                format!("local {}", names.join(", ")),
                line,
                true,
                Form::Entry,
            ));
            let mut values = Vec::new();
            for (i, p) in la.expressions().pairs().enumerate() {
                let label = names.get(i).cloned().unwrap_or_else(|| format!("value {}", i + 1));
                let slot = self.slot(id, label, None, p.value());
                value_tys.push(match slot {
                    SlotRef::In(k) => self.g.nodes[id].inputs[k].ty,
                    SlotRef::Closure(_) => PinTy::Obj,
                });
                values.push((slot, p.punctuation().cloned()));
            }
            self.g.nodes[id].form = Form::Set {
                binding: Binding::Local(Box::new(la.clone())),
                values,
            };
            self.next(id);
            id
        };
        for (i, name) in names.into_iter().enumerate() {
            let ty = value_tys.get(i).copied().unwrap_or(PinTy::Any);
            let out = self.output(id, name.clone(), ty, OutKind::Var);
            self.declare(name, (id, out));
            self.g.nodes[id].declares.push(out);
        }
        id
    }

    fn assignment(&mut self, a: &Assignment, line: u32) -> NodeId {
        let exprs: Vec<&Expression> = a.expressions().iter().collect();
        if let (1, [Expression::Function(f)], true, Some(Var::Name(name))) =
            (a.variables().len(), exprs.as_slice(), self.chunk, a.variables().iter().next())
        {
            let name = name.token().to_string();
            let id = self.add(Node::new(Category::Fn, name.clone(), line, true, Form::Entry));
            let sub = self.canvas(&name, false, f.body(), line);
            self.function_node(id, &name, sub, FnDecl::Assign(Box::new(a.clone())));
            return id;
        }
        let id = self.add(Node::new(Category::Variable, String::new(), line, true, Form::Entry));
        let mut targets = Vec::new();
        let mut shown = Vec::new();
        for p in a.variables().pairs() {
            let text = bare(p.value());
            shown.push(text.clone());
            let slot = self.var_slot(id, format!("set {text}"), p.value());
            targets.push((slot, p.punctuation().cloned()));
        }
        let single_call = match exprs.as_slice() {
            [Expression::FunctionCall(fc)] if chain_is_liftable(fc.prefix(), &suffixes_of_call(fc)) => Some(fc),
            _ => None,
        };
        let binding = Binding::Assign {
            stmt: Box::new(a.clone()),
            targets,
        };
        if let Some(fc) = single_call {
            self.g.nodes[id].category = Category::Call;
            let call = self.call_form(id, fc.prefix(), &suffixes_of_call(fc));
            self.finish_call(id, binding, call);
        } else {
            self.g.nodes[id].title = format!("{} =", shown.join(", "));
            let mut values = Vec::new();
            for (i, p) in a.expressions().pairs().enumerate() {
                let label = shown.get(i).cloned().unwrap_or_else(|| format!("value {}", i + 1));
                values.push((self.slot(id, label, None, p.value()), p.punctuation().cloned()));
            }
            self.g.nodes[id].form = Form::Set { binding, values };
            self.next(id);
        }
        id
    }

    fn function_node(&mut self, id: NodeId, name: &str, sub: Graph, decl: FnDecl) {
        let short = name.rsplit(['.', ':']).next().unwrap_or(name);
        if self.ctx.base_fn(short).is_some() {
            self.g.nodes[id].category = Category::Lifecycle;
        }
        self.g.nodes[id].sub = Some(Box::new(sub));
        self.g.nodes[id].form = Form::Function(decl);
        self.next(id);
    }

    /// A function body as a canvas of its own.
    fn canvas(&self, name: &str, method: bool, body: &FunctionBody, line: u32) -> Graph {
        let short = name.rsplit(['.', ':']).next().unwrap_or(name);
        let (category, title) = if self.ctx.base_fn(short).is_some() {
            (Category::Lifecycle, format!("On {short}"))
        } else {
            (Category::Fn, name.to_string())
        };
        let entry = Node::new(category, title, line, false, Form::Entry);
        let mut l = Lifter::new(self.ctx, self.visible(), entry, false);
        l.g.nodes[ENTRY].exec_outs.push("body".into());
        if method {
            let out = l.output(ENTRY, "self".into(), PinTy::Obj, OutKind::Var);
            l.declare("self".into(), (ENTRY, out));
        }
        for p in body.parameters() {
            let name = param_name(p);
            let ty = if name == "self" { PinTy::Obj } else { PinTy::Any };
            let out = l.output(ENTRY, name.clone(), ty, OutKind::Var);
            l.declare(name, (ENTRY, out));
        }
        l.scoped_block(body.block(), (ENTRY, 0));
        l.g
    }

    // ── calls ───────────────────────────────────────────────────────────────────────────

    fn finish_call(&mut self, id: NodeId, binding: Binding, call: CallForm) {
        let (category, title) = self.classify(id, &call);
        let n = &mut self.g.nodes[id];
        n.category = category;
        n.title = title;
        n.form = Form::Call { binding, call };
        self.next(id);
    }

    /// Fill `owner`'s pins from the call chain `prefix suffixes` (which ends in a call).
    fn call_form(&mut self, owner: NodeId, prefix: &Prefix, suffixes: &[Suffix]) -> CallForm {
        let k = suffixes.len() - 1;
        let mut j = k;
        while j > 0 && matches!(suffixes[j - 1], Suffix::Index(Index::Dot { .. })) {
            j -= 1;
        }
        let path = suffixes[j..k].to_vec();
        let head = if j == 0 {
            match prefix {
                Prefix::Name(t) => {
                    let name = t.token().to_string();
                    match self.lookup(&name) {
                        Some(from) => {
                            let ty = self.ty_of(from);
                            HeadForm::Slot(self.wired(owner, name, ty, Some(t.clone()), Vec::new(), from))
                        }
                        None => HeadForm::Global(t.clone()),
                    }
                }
                Prefix::Expression(e) => HeadForm::Slot(self.slot(owner, "target".into(), None, e)),
                other => panic!("Lua 5.1/5.2 has no call prefix of the form {other}"),
            }
        } else {
            let inner = self.call_expr(prefix, &suffixes[..j]);
            HeadForm::Slot(self.wired(owner, "target".into(), PinTy::Any, None, Vec::new(), (inner, 0)))
        };
        let (method, args) = match &suffixes[k] {
            Suffix::Call(Call::AnonymousCall(args)) => (None, args.as_ref()),
            Suffix::Call(Call::MethodCall(m)) => (Some(m.clone()), m.args()),
            other => panic!("a call chain ends in a call, not {other}"),
        };
        let on_self = j == 0 && matches!(prefix, Prefix::Name(t) if t.token().to_string() == "self");
        let params = self.params_for(&head, on_self, &path, method.as_deref());
        let args = self.args_form(owner, args, params.as_deref());
        CallForm {
            head,
            path,
            method,
            args,
        }
    }

    /// Parameter names for a call's arguments, from the called function's definition.
    fn params_for(
        &self,
        head: &HeadForm,
        on_self: bool,
        path: &[Suffix],
        method: Option<&MethodCall>,
    ) -> Option<Vec<String>> {
        let ctx = self.ctx;
        let dot_names: Vec<String> = path
            .iter()
            .filter_map(|s| match s {
                Suffix::Index(Index::Dot { name, .. }) => Some(name.token().to_string()),
                _ => None,
            })
            .collect();
        match (head, dot_names.as_slice(), method) {
            (HeadForm::Global(m), [f], None) => ctx.module_fn(&m.token().to_string(), f).cloned(),
            (HeadForm::Global(m), [], Some(mc)) => ctx
                .module_fn(&m.token().to_string(), &mc.name().token().to_string())
                .map(|p| p.iter().skip(1).cloned().collect()),
            (HeadForm::Global(f), [], None) => {
                let name = f.token().to_string();
                ctx.own_fn(&name).or_else(|| ctx.base_fn(&name)).cloned()
            }
            (HeadForm::Slot(_), [], Some(mc)) if on_self => {
                let name = mc.name().token().to_string();
                ctx.own_fn(&name)
                    .or_else(|| ctx.base_fn(&name))
                    .map(|p| p.iter().skip(1).cloned().collect())
            }
            _ => None,
        }
    }

    fn args_form(&mut self, owner: NodeId, args: &FunctionArgs, params: Option<&[String]>) -> ArgsForm {
        let label = |i: usize| -> String {
            params
                .and_then(|p| p.get(i))
                .filter(|p| p.as_str() != "...")
                .cloned()
                .unwrap_or_else(|| format!("arg {}", i + 1))
        };
        match args {
            FunctionArgs::Parentheses {
                parentheses,
                arguments,
            } => {
                let several = arguments.len() > 1;
                let mut out = Vec::new();
                for (i, p) in arguments.pairs().enumerate() {
                    let form = match p.value() {
                        Expression::TableConstructor(tc) if !is_literal_list(tc) => {
                            let group = several.then(|| label(i));
                            ArgForm::Table(self.table_form(owner, tc, group))
                        }
                        e => ArgForm::Slot(self.slot(owner, label(i), None, e)),
                    };
                    out.push((form, p.punctuation().cloned()));
                }
                ArgsForm::Parens {
                    parens: parentheses.clone(),
                    args: out,
                }
            }
            FunctionArgs::String(t) => ArgsForm::String(self.slot(owner, label(0), None, &Expression::String(t.clone()))),
            FunctionArgs::TableConstructor(tc) => ArgsForm::Table(self.table_form(owner, tc, None)),
            other => panic!("Lua 5.1/5.2 has no call arguments of the form {other}"),
        }
    }

    fn table_form(&mut self, owner: NodeId, tc: &TableConstructor, group: Option<String>) -> TableForm {
        let mut fields = Vec::new();
        let mut pos = 0;
        for p in tc.fields().pairs() {
            let f = match p.value() {
                Field::NameKey { key, equal, value } => FieldForm::Named {
                    key: key.clone(),
                    equal: equal.clone(),
                    value: self.slot(owner, key.token().to_string(), group.clone(), value),
                },
                Field::ExpressionKey {
                    brackets,
                    key,
                    equal,
                    value,
                } => FieldForm::Keyed {
                    brackets: brackets.clone(),
                    key: key.clone(),
                    equal: equal.clone(),
                    value: self.slot(owner, format!("[{}]", bare(key.as_ref())), group.clone(), value),
                },
                Field::NoKey(value) => {
                    pos += 1;
                    let label = match value {
                        Expression::Var(v) => bare(v),
                        _ => format!("[{pos}]"),
                    };
                    FieldForm::Positional(self.slot(owner, label, group.clone(), value))
                }
                other => panic!("Lua 5.1/5.2 has no table field of the form {other}"),
            };
            fields.push((f, p.punctuation().cloned()));
        }
        TableForm {
            braces: tc.braces().clone(),
            fields,
        }
    }

    /// A data node whose result is output 0, made before its pins are lifted so the parameters
    /// of a function among them land after it.
    fn data_node(&mut self, category: Category, title: String, line: u32, out: &str, ty: PinTy) -> NodeId {
        let id = self.add(Node::new(category, title, line, false, Form::Entry));
        self.output(id, out.into(), ty, OutKind::Value);
        id
    }

    /// A call used as a value.
    fn call_expr(&mut self, prefix: &Prefix, suffixes: &[Suffix]) -> NodeId {
        let line = prefix.start_position().map(|p| p.line() as u32).unwrap_or(0);
        let id = self.data_node(Category::Call, String::new(), line, "value", PinTy::Any);
        let call = self.call_form(id, prefix, suffixes);
        let (category, title) = self.classify(id, &call);
        let n = &mut self.g.nodes[id];
        n.category = category;
        n.title = title;
        n.form = Form::CallExpr(call);
        id
    }

    /// The category and title of a call node, from what it calls.
    fn classify(&self, id: NodeId, call: &CallForm) -> (Category, String) {
        let n = &self.g.nodes[id];
        let head = match &call.head {
            HeadForm::Global(t) => t.token().to_string(),
            HeadForm::Slot(SlotRef::In(i)) => n.inputs[*i].label.clone(),
            HeadForm::Slot(SlotRef::Closure(_)) => "function".into(),
        };
        let mut callee = head.clone();
        for s in &call.path {
            callee.push_str(&bare(s));
        }
        let fname = match &call.method {
            Some(m) => {
                let name = m.name().token().to_string();
                callee.push(':');
                callee.push_str(&name);
                name
            }
            None => match call.path.last() {
                Some(Suffix::Index(Index::Dot { name, .. })) => name.token().to_string(),
                _ => head.clone(),
            },
        };
        let literal_field = |label: &str| -> Option<String> {
            n.inputs.iter().find(|i| i.label == label).and_then(|i| match &i.value {
                Value::Literal(Expression::String(t)) => string_value(t),
                _ => None,
            })
        };
        if fname == "CreateChild" {
            let title = literal_field("sModuleName").unwrap_or(callee);
            return (Category::Objective, title);
        }
        let event_kind = n.inputs.iter().find_map(|i| match &i.value {
            Value::Path(v) => bare(v).strip_prefix("Event.").map(str::to_string),
            _ => None,
        });
        if fname == "_CreateEvent" || callee == "Event.Create" {
            if let Some(kind) = event_kind {
                return (Category::Event, format!("Event · {kind}"));
            }
            return (Category::Event, callee);
        }
        if let HeadForm::Global(t) = &call.head {
            let root = t.token().to_string();
            if self.ctx.is_base(&root) && call.path.len() == 1 && call.method.is_none() {
                return (Category::Base, format!("Super.{fname}"));
            }
            if let Some((_, c)) = MODULE_CATEGORIES.iter().find(|(m, _)| *m == root) {
                return (*c, callee);
            }
        }
        let own_fn = self.ctx.own_fn(&fname).is_some();
        let simple = matches!(&call.head, HeadForm::Global(_)) && call.path.is_empty() && call.method.is_none();
        let on_self = matches!(&call.head, HeadForm::Slot(_)) && head == "self" && call.method.is_some();
        if own_fn && (simple || on_self) {
            return (Category::Fn, fname);
        }
        (Category::Call, callee)
    }

    // ── values ──────────────────────────────────────────────────────────────────────────

    fn wired(
        &mut self,
        owner: NodeId,
        label: String,
        ty: PinTy,
        site: Option<TokenReference>,
        suffixes: Vec<Suffix>,
        from: (NodeId, usize),
    ) -> SlotRef {
        let idx = self.push_input(owner, label, None, ty, Value::Wired { site, suffixes });
        self.g.data.push(DataWire { from, to: (owner, idx) });
        SlotRef::In(idx)
    }

    fn push_input(&mut self, owner: NodeId, label: String, group: Option<String>, ty: PinTy, value: Value) -> usize {
        self.g.nodes[owner].inputs.push(Input { label, group, ty, value });
        self.g.nodes[owner].inputs.len() - 1
    }

    /// Lift `e` into a new slot of `owner`.
    fn slot(&mut self, owner: NodeId, label: String, group: Option<String>, e: &Expression) -> SlotRef {
        if let Expression::Function(f) = e {
            let exec = self.exec_out(owner, &label);
            self.frames.push(Vec::new());
            let mut outs = Vec::new();
            for p in f.body().parameters() {
                let name = param_name(p);
                let out = self.output(owner, name.clone(), PinTy::Any, OutKind::Var);
                self.declare(name, (owner, out));
                outs.push(out);
            }
            self.g.nodes[owner].block_vars.push((exec, outs));
            self.g.nodes[owner].closures.push(ClosureForm {
                func: (**f).clone(),
                exec,
            });
            let c = self.g.nodes[owner].closures.len() - 1;
            self.block(f.body().block(), (owner, exec));
            self.frames.pop();
            return SlotRef::Closure(c);
        }
        let (value, ty, from) = self.value(e);
        let idx = self.push_input(owner, label, group, ty, value);
        if let Some(from) = from {
            self.g.data.push(DataWire { from, to: (owner, idx) });
        }
        SlotRef::In(idx)
    }

    /// An assignment target as a slot of `owner`.
    fn var_slot(&mut self, owner: NodeId, label: String, v: &Var) -> SlotRef {
        let (value, ty, from) = self.var_value(v);
        let idx = self.push_input(owner, label, None, ty, value);
        if let Some(from) = from {
            self.g.data.push(DataWire { from, to: (owner, idx) });
        }
        SlotRef::In(idx)
    }

    fn value(&mut self, e: &Expression) -> (Value, PinTy, Option<(NodeId, usize)>) {
        match e {
            Expression::Number(_) => (Value::Literal(e.clone()), PinTy::Num, None),
            Expression::String(_) => (Value::Literal(e.clone()), PinTy::Str, None),
            Expression::Symbol(t) => {
                let ty = match t.token().token_type() {
                    TokenType::Symbol {
                        symbol: Symbol::True | Symbol::False,
                    } => PinTy::Bool,
                    _ => PinTy::Any,
                };
                (Value::Literal(e.clone()), ty, None)
            }
            Expression::UnaryOperator {
                unop: UnOp::Minus(_),
                expression,
            } if matches!(expression.as_ref(), Expression::Number(_)) => (Value::Literal(e.clone()), PinTy::Num, None),
            Expression::Var(v) => self.var_value(v),
            Expression::FunctionCall(fc) => {
                let suffixes = suffixes_of_call(fc);
                if !chain_is_liftable(fc.prefix(), &suffixes) {
                    return (Value::Lua(Lua::Expr(e.clone())), PinTy::Any, None);
                }
                let id = self.call_expr(fc.prefix(), &suffixes);
                (
                    Value::Wired {
                        site: None,
                        suffixes: Vec::new(),
                    },
                    PinTy::Any,
                    Some((id, 0)),
                )
            }
            Expression::TableConstructor(tc) => {
                if is_literal_list(tc) {
                    return (Value::Array(tc.clone()), PinTy::Arr, None);
                }
                let line = line_of(tc.as_ref());
                let all_positional = tc.fields().iter().all(|f| matches!(f, Field::NoKey(_)));
                let ty = if all_positional && !tc.fields().is_empty() { PinTy::Arr } else { PinTy::Obj };
                let id = self.data_node(Category::Data, "table".into(), line, "table", ty);
                let form = self.table_form(id, tc, None);
                self.g.nodes[id].form = Form::Table(form);
                (
                    Value::Wired {
                        site: None,
                        suffixes: Vec::new(),
                    },
                    ty,
                    Some((id, 0)),
                )
            }
            Expression::BinaryOperator { lhs, binop, rhs } => {
                let line = line_of(e);
                let op = binop.token().token().to_string();
                let ty = match op.as_str() {
                    "==" | "~=" | "<" | "<=" | ">" | ">=" => PinTy::Bool,
                    ".." => PinTy::Str,
                    "+" | "-" | "*" | "/" | "%" | "^" => PinTy::Num,
                    _ => PinTy::Any,
                };
                let id = self.data_node(Category::Data, op, line, "value", ty);
                let l = self.slot(id, "a".into(), None, lhs);
                let r = self.slot(id, "b".into(), None, rhs);
                self.g.nodes[id].form = Form::BinOp {
                    op: binop.clone(),
                    lhs: l,
                    rhs: r,
                };
                (
                    Value::Wired {
                        site: None,
                        suffixes: Vec::new(),
                    },
                    ty,
                    Some((id, 0)),
                )
            }
            Expression::UnaryOperator { unop, expression } => {
                let line = line_of(e);
                let op = unop.token().token().to_string();
                let ty = match op.as_str() {
                    "not" => PinTy::Bool,
                    _ => PinTy::Num,
                };
                let id = self.data_node(Category::Data, op, line, "value", ty);
                let operand = self.slot(id, "value".into(), None, expression);
                self.g.nodes[id].form = Form::UnOp {
                    op: unop.clone(),
                    operand,
                };
                (
                    Value::Wired {
                        site: None,
                        suffixes: Vec::new(),
                    },
                    ty,
                    Some((id, 0)),
                )
            }
            Expression::Parentheses { contained, expression } => {
                let line = line_of(e);
                let id = self.data_node(Category::Data, "( )".into(), line, "value", PinTy::Any);
                let inner = self.slot(id, "value".into(), None, expression);
                let ty = match inner {
                    SlotRef::In(i) => self.g.nodes[id].inputs[i].ty,
                    SlotRef::Closure(_) => PinTy::Obj,
                };
                self.g.nodes[id].outputs[0].ty = ty;
                self.g.nodes[id].form = Form::Paren {
                    span: contained.clone(),
                    inner,
                };
                (
                    Value::Wired {
                        site: None,
                        suffixes: Vec::new(),
                    },
                    ty,
                    Some((id, 0)),
                )
            }
            other => (Value::Lua(Lua::Expr(other.clone())), PinTy::Any, None),
        }
    }

    fn var_value(&mut self, v: &Var) -> (Value, PinTy, Option<(NodeId, usize)>) {
        match v {
            Var::Name(t) => match self.lookup(&t.token().to_string()) {
                Some(from) => (
                    Value::Wired {
                        site: Some(t.clone()),
                        suffixes: Vec::new(),
                    },
                    self.ty_of(from),
                    Some(from),
                ),
                None => (Value::Path(v.clone()), PinTy::Obj, None),
            },
            Var::Expression(ve) => {
                let suffixes: Vec<Suffix> = ve.suffixes().cloned().collect();
                if !chain_is_liftable(ve.prefix(), &suffixes) {
                    return (Value::Lua(Lua::Var(v.clone())), PinTy::Any, None);
                }
                match suffixes.iter().rposition(|s| matches!(s, Suffix::Call(_))) {
                    Some(k) => {
                        let id = self.call_expr(ve.prefix(), &suffixes[..=k]);
                        (
                            Value::Wired {
                                site: None,
                                suffixes: suffixes[k + 1..].to_vec(),
                            },
                            PinTy::Any,
                            Some((id, 0)),
                        )
                    }
                    None => match ve.prefix() {
                        Prefix::Name(t) => match self.lookup(&t.token().to_string()) {
                            Some(from) => (
                                Value::Wired {
                                    site: Some(t.clone()),
                                    suffixes,
                                },
                                PinTy::Any,
                                Some(from),
                            ),
                            None => (Value::Path(v.clone()), PinTy::Obj, None),
                        },
                        Prefix::Expression(e) => match self.value(e) {
                            (
                                Value::Wired {
                                    site: None,
                                    suffixes: inner,
                                },
                                _,
                                Some(from),
                            ) if inner.is_empty() => (
                                Value::Wired {
                                    site: None,
                                    suffixes,
                                },
                                PinTy::Any,
                                Some(from),
                            ),
                            _ => (Value::Lua(Lua::Var(v.clone())), PinTy::Any, None),
                        },
                        other => panic!("Lua 5.1/5.2 has no prefix of the form {other}"),
                    },
                }
            }
            other => panic!("Lua 5.1/5.2 has no variable of the form {other}"),
        }
    }
}

// ───────────────────────────────────────────────────────────────────── emitting

/// One level of indentation for statements the editor writes: the reference corpus's (unluac's).
const INDENT_UNIT: &str = "  ";

impl Graph {
    /// The block this canvas stands for, rebuilt from its nodes and wires.
    pub fn emit_body(&self) -> Result<Block, Vec<Problem>> {
        self.emit_body_at(String::new())
    }

    /// As [`Graph::emit_body`], for a body whose new statements indent to `indent` when no
    /// statement from the source shows the block's indentation.
    fn emit_body_at(&self, indent: String) -> Result<Block, Vec<Problem>> {
        let mut em = Emitter::new(self);
        let block = em.chain((ENTRY, 0), indent).map_err(|p| vec![p])?;
        let mut problems = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if n.exec_in && !em.reached[i] {
                problems.push(Problem {
                    node: Some(i),
                    message: format!("`{}` is not on the function's exec chain, so it would never run", n.title),
                });
            }
            let is_data = !n.exec_in && i != ENTRY;
            if is_data && em.used[i] != 1 {
                problems.push(Problem {
                    node: Some(i),
                    message: format!(
                        "`{}` feeds {} pins; an expression is evaluated where it is used, so it must feed exactly one",
                        n.title, em.used[i]
                    ),
                });
            }
        }
        if problems.is_empty() {
            Ok(block)
        } else {
            Err(problems)
        }
    }
}

/// The script with its main chunk replaced by `chunk`'s emitted body.
pub fn emit(script: &Script, chunk: &Graph) -> Result<String, Vec<Problem>> {
    let block = chunk.emit_body()?;
    Ok(script.ast().clone().with_nodes(block).to_string())
}

struct Emitter<'g> {
    g: &'g Graph,
    reached: Vec<bool>,
    used: Vec<u32>,
    /// The indentation of the statement being emitted; its blocks indent one level further.
    indent: String,
    /// The variables (node, output) in scope where emission stands: those declared by statements
    /// already emitted on this chain and the chains enclosing it, and those of the blocks being
    /// emitted (a loop's variables, a function's parameters). The entry's are always in scope.
    scope: Vec<(NodeId, usize)>,
}

impl<'g> Emitter<'g> {
    fn new(g: &'g Graph) -> Self {
        Emitter {
            g,
            reached: vec![false; g.nodes.len()],
            used: vec![0; g.nodes.len()],
            indent: String::new(),
            scope: Vec::new(),
        }
    }
}

fn problem(node: NodeId, message: impl Into<String>) -> Problem {
    Problem {
        node: Some(node),
        message: message.into(),
    }
}

fn punct<T>(items: Vec<(T, Option<TokenReference>)>) -> Punctuated<T> {
    items.into_iter().map(|(v, p)| Pair::new(v, p)).collect()
}

impl Emitter<'_> {
    fn target(&self, from: (NodeId, usize)) -> Result<Option<NodeId>, Problem> {
        let mut to = self.g.exec.iter().filter(|w| w.from == from).map(|w| w.to);
        let first = to.next();
        if to.next().is_some() {
            let n = &self.g.nodes[from.0];
            return Err(problem(
                from.0,
                format!("`{}` exec output `{}` is wired to more than one node", n.title, n.exec_outs[from.1]),
            ));
        }
        Ok(first)
    }

    /// The statements on the chain from exec output `from`, in order.
    fn walk(&mut self, from: (NodeId, usize)) -> Result<Vec<NodeId>, Problem> {
        let g = self.g;
        let mut ids = Vec::new();
        let mut cur = self.target(from)?;
        while let Some(id) = cur {
            let n = &g.nodes[id];
            if self.reached[id] {
                return Err(problem(
                    id,
                    format!("`{}` is reached twice; an exec chain cannot loop back or merge", n.title),
                ));
            }
            self.reached[id] = true;
            if !n.exec_in {
                return Err(problem(id, format!("`{}` has no exec input", n.title)));
            }
            ids.push(id);
            let next = match n.next {
                Some(next) => self.target((id, next))?,
                None => None,
            };
            if matches!(n.form, Form::Return { .. } | Form::Break(_)) {
                if let Some(after) = next {
                    return Err(problem(
                        after,
                        format!("`{}` follows a `{}`, which ends its block", g.nodes[after].title, n.title),
                    ));
                }
                break;
            }
            cur = next;
        }
        Ok(ids)
    }

    /// The block the chain from `from` stands for. Statements made in the editor indent to the
    /// block's indentation: a source statement's on the chain, else `fallback`.
    fn chain(&mut self, from: (NodeId, usize), fallback: String) -> Result<Block, Problem> {
        let mark = self.scope.len();
        let block = self.chain_open(from, fallback);
        self.scope.truncate(mark);
        block
    }

    /// As [`Emitter::chain`], leaving the chain's statements in scope for the caller.
    fn chain_open(&mut self, from: (NodeId, usize), fallback: String) -> Result<Block, Problem> {
        let g = self.g;
        let ids = self.walk(from)?;
        let block_indent = ids
            .iter()
            .find(|&&id| !g.nodes[id].fresh)
            .map(|&id| g.nodes[id].indent.clone())
            .unwrap_or(fallback);
        let mut stmts = Vec::new();
        let mut last = None;
        for id in ids {
            let n = &g.nodes[id];
            let own = if n.fresh { block_indent.clone() } else { n.indent.clone() };
            let saved = std::mem::replace(&mut self.indent, own);
            match &n.form {
                Form::Return { ret, values } => {
                    let returns = self.slots(id, values)?;
                    let mut s = LastStmt::Return(ret.clone().with_returns(returns));
                    if n.fresh {
                        s = reindent(s, &n.indent, &self.indent);
                    }
                    last = Some((s, n.semi.clone()));
                }
                Form::Break(t) => {
                    let mut s = LastStmt::Break(t.clone());
                    if n.fresh {
                        s = reindent(s, &n.indent, &self.indent);
                    }
                    last = Some((s, n.semi.clone()));
                }
                _ => {
                    let mut s = self.stmt(id)?;
                    if n.fresh {
                        s = reindent(s, &n.indent, &self.indent);
                    }
                    stmts.push((s, n.semi.clone()));
                }
            }
            self.indent = saved;
            self.scope.extend(n.declares.iter().map(|&o| (id, o)));
        }
        Ok(Block::new().with_stmts(stmts).with_last_stmt(last))
    }

    /// Bring the variables of the block on exec output `out` of `id` into scope.
    fn open_block(&mut self, id: NodeId, out: usize) {
        let vars = self.g.nodes[id].block_vars.iter().filter(|(o, _)| *o == out).flat_map(|(_, v)| v);
        self.scope.extend(vars.map(|&v| (id, v)));
    }

    /// The block hanging off exec output `out` of the statement being emitted, with the
    /// statement's own variables (a loop's, a function's parameters) in scope.
    fn inner(&mut self, id: NodeId, out: usize) -> Result<Block, Problem> {
        let deeper = format!("{}{INDENT_UNIT}", self.indent);
        let mark = self.scope.len();
        self.open_block(id, out);
        let block = self.chain((id, out), deeper);
        self.scope.truncate(mark);
        block
    }

    fn slots(&mut self, id: NodeId, slots: &[(SlotRef, Option<TokenReference>)]) -> Result<Punctuated<Expression>, Problem> {
        let mut out = Vec::new();
        for (s, p) in slots {
            out.push((self.slot(id, *s)?, p.clone()));
        }
        Ok(punct(out))
    }

    fn stmt(&mut self, id: NodeId) -> Result<Stmt, Problem> {
        let g = self.g;
        let n = &g.nodes[id];
        Ok(match &n.form {
            Form::Call { binding, call } => {
                let fc = self.call(id, call)?;
                let value = vec![(Expression::FunctionCall(fc.clone()), None)];
                match binding {
                    Binding::None => Stmt::FunctionCall(fc),
                    Binding::Local(la) => Stmt::LocalAssignment(la.as_ref().clone().with_expressions(punct(value))),
                    Binding::Assign { stmt, targets } => {
                        let vars = self.targets(id, targets)?;
                        Stmt::Assignment(stmt.as_ref().clone().with_variables(vars).with_expressions(punct(value)))
                    }
                }
            }
            Form::Set { binding, values } => {
                let exprs = self.slots(id, values)?;
                match binding {
                    Binding::Local(la) => Stmt::LocalAssignment(la.as_ref().clone().with_expressions(exprs)),
                    Binding::Assign { stmt, targets } => {
                        let vars = self.targets(id, targets)?;
                        Stmt::Assignment(stmt.as_ref().clone().with_variables(vars).with_expressions(exprs))
                    }
                    Binding::None => unreachable!("a Set node always binds"),
                }
            }
            Form::If {
                stmt,
                cond,
                then,
                elseifs,
                otherwise,
            } => {
                let c = self.slot(id, *cond)?;
                let b = self.inner(id, *then)?;
                let mut eifs = Vec::new();
                for (e, ec, out) in elseifs {
                    let c = self.slot(id, *ec)?;
                    let b = self.inner(id, *out)?;
                    eifs.push(e.clone().with_condition(c).with_block(b));
                }
                let else_block = match otherwise {
                    Some(o) => Some(self.inner(id, *o)?),
                    None => None,
                };
                Stmt::If(Box::new(
                    stmt.as_ref()
                        .clone()
                        .with_condition(c)
                        .with_block(b)
                        .with_else_if(if eifs.is_empty() { None } else { Some(eifs) })
                        .with_else(else_block),
                ))
            }
            Form::While { stmt, cond, body } => {
                let c = self.slot(id, *cond)?;
                let b = self.inner(id, *body)?;
                Stmt::While(Box::new(stmt.as_ref().clone().with_condition(c).with_block(b)))
            }
            Form::Repeat { stmt, until, body } => {
                // `until` sees the body's locals, so the body's scope stays open for it.
                let deeper = format!("{}{INDENT_UNIT}", self.indent);
                let mark = self.scope.len();
                self.open_block(id, *body);
                let b = self.chain_open((id, *body), deeper);
                let u = b.as_ref().ok().map(|_| self.slot(id, *until));
                self.scope.truncate(mark);
                let (b, u) = (b?, u.expect("the body emitted")?);
                Stmt::Repeat(Box::new(stmt.as_ref().clone().with_block(b).with_until(u)))
            }
            Form::NumericFor {
                stmt,
                start,
                end,
                step,
                body,
            } => {
                let s = self.slot(id, *start)?;
                let e = self.slot(id, *end)?;
                let st = match step {
                    Some(st) => Some(self.slot(id, *st)?),
                    None => None,
                };
                let b = self.inner(id, *body)?;
                Stmt::NumericFor(Box::new(
                    stmt.as_ref().clone().with_start(s).with_end(e).with_step(st).with_block(b),
                ))
            }
            Form::GenericFor { stmt, exprs, body } => {
                let es = self.slots(id, exprs)?;
                let b = self.inner(id, *body)?;
                Stmt::GenericFor(Box::new(stmt.as_ref().clone().with_expressions(es).with_block(b)))
            }
            Form::Do { stmt, body } => {
                let b = self.inner(id, *body)?;
                Stmt::Do(Box::new(stmt.as_ref().clone().with_block(b)))
            }
            Form::Function(decl) => {
                let sub = n.sub.as_ref().expect("a function node has a canvas");
                let block = sub.emit_body_at(format!("{}{INDENT_UNIT}", self.indent)).map_err(|ps| {
                    let first = &ps[0];
                    problem(id, format!("in `{}`: {}", n.title, first.message))
                })?;
                let with_block = |body: &FunctionBody| body.clone().with_block(block.clone());
                match decl {
                    FnDecl::Declaration(f) => {
                        Stmt::FunctionDeclaration(Box::new(f.as_ref().clone().with_body(with_block(f.body()))))
                    }
                    FnDecl::Local(f) => Stmt::LocalFunction(Box::new(f.as_ref().clone().with_body(with_block(f.body())))),
                    FnDecl::Assign(a) => {
                        let e = closure_with_body(a.expressions(), &block);
                        Stmt::Assignment(a.as_ref().clone().with_expressions(e))
                    }
                    FnDecl::LocalAssign(la) => {
                        let e = closure_with_body(la.expressions(), &block);
                        Stmt::LocalAssignment(la.as_ref().clone().with_expressions(e))
                    }
                }
            }
            Form::Lua(s) => s.as_ref().clone(),
            _ => return Err(problem(id, format!("`{}` is not a statement", n.title))),
        })
    }

    fn targets(&mut self, id: NodeId, targets: &[(SlotRef, Option<TokenReference>)]) -> Result<Punctuated<Var>, Problem> {
        let mut out = Vec::new();
        for (s, p) in targets {
            let v = match self.slot(id, *s)? {
                Expression::Var(v) => v,
                other => {
                    return Err(problem(
                        id,
                        format!("`{}` cannot be assigned to", bare(&other)),
                    ))
                }
            };
            out.push((v, p.clone()));
        }
        Ok(punct(out))
    }

    fn slot(&mut self, id: NodeId, s: SlotRef) -> Result<Expression, Problem> {
        match s {
            SlotRef::In(i) => self.input(id, i),
            SlotRef::Closure(c) => {
                let g = self.g;
                let cf = &g.nodes[id].closures[c];
                let block = self.inner(id, cf.exec)?;
                let f = cf.func.clone();
                let body = f.body().clone().with_block(block);
                Ok(Expression::Function(Box::new(f.with_body(body))))
            }
        }
    }

    fn input(&mut self, id: NodeId, i: usize) -> Result<Expression, Problem> {
        let g = self.g;
        let n = &g.nodes[id];
        let input = &n.inputs[i];
        Ok(match &input.value {
            Value::Literal(e) => e.clone(),
            Value::Path(v) => Expression::Var(v.clone()),
            Value::Array(t) => Expression::TableConstructor(t.clone()),
            Value::Lua(Lua::Expr(e)) => e.clone(),
            Value::Lua(Lua::Var(v)) => Expression::Var(v.clone()),
            Value::Wired { site, suffixes } => {
                let mut wires = g.data.iter().filter(|w| w.to == (id, i));
                let Some(w) = wires.next() else {
                    return Err(problem(id, format!("`{}` pin `{}` has nothing wired to it", n.title, input.label)));
                };
                if wires.next().is_some() {
                    return Err(problem(id, format!("`{}` pin `{}` has more than one wire", n.title, input.label)));
                }
                let (from, out) = w.from;
                let producer = &g.nodes[from].outputs[out];
                let base = match producer.kind {
                    OutKind::Var | OutKind::Captured => {
                        if from != ENTRY && !self.scope.contains(&(from, out)) {
                            return Err(problem(
                                id,
                                format!(
                                    "`{}` pin `{}` reads `{}`, which is not in scope there: `{}` declares it later or inside another block",
                                    n.title, input.label, producer.label, g.nodes[from].title
                                ),
                            ));
                        }
                        Expression::Var(Var::Name(name_token(&producer.label, site.as_ref())))
                    }
                    OutKind::Value => {
                        let e = self.expr(from)?;
                        match site {
                            Some(s) => retrivia(e, s.leading_trivia().cloned().collect(), s.trailing_trivia().cloned().collect()),
                            None => e,
                        }
                    }
                };
                with_suffixes(base, suffixes).map_err(|m| problem(id, m))?
            }
        })
    }

    /// A data node's expression.
    fn expr(&mut self, id: NodeId) -> Result<Expression, Problem> {
        self.used[id] += 1;
        let g = self.g;
        let n = &g.nodes[id];
        Ok(match &n.form {
            Form::CallExpr(call) => Expression::FunctionCall(self.call(id, call)?),
            Form::Table(t) => Expression::TableConstructor(Box::new(self.table(id, t)?)),
            Form::BinOp { op, lhs, rhs } => Expression::BinaryOperator {
                lhs: Box::new(self.slot(id, *lhs)?),
                binop: op.clone(),
                rhs: Box::new(self.slot(id, *rhs)?),
            },
            Form::UnOp { op, operand } => Expression::UnaryOperator {
                unop: op.clone(),
                expression: Box::new(self.slot(id, *operand)?),
            },
            Form::Paren { span, inner } => Expression::Parentheses {
                contained: span.clone(),
                expression: Box::new(self.slot(id, *inner)?),
            },
            _ => return Err(problem(id, format!("`{}` has no value to wire", n.title))),
        })
    }

    fn call(&mut self, id: NodeId, call: &CallForm) -> Result<FunctionCall, Problem> {
        let (prefix, mut suffixes) = match &call.head {
            HeadForm::Global(t) => (Prefix::Name(t.clone()), Vec::new()),
            HeadForm::Slot(s) => {
                let e = self.slot(id, *s)?;
                split_chain(e).map_err(|m| problem(id, m))?
            }
        };
        suffixes.extend(call.path.iter().cloned());
        let args = self.args(id, &call.args)?;
        let c = match &call.method {
            Some(m) => Call::MethodCall(Box::new(m.as_ref().clone().with_args(args))),
            None => Call::AnonymousCall(Box::new(args)),
        };
        suffixes.push(Suffix::Call(c));
        Ok(FunctionCall::new(prefix).with_suffixes(suffixes))
    }

    fn args(&mut self, id: NodeId, args: &ArgsForm) -> Result<FunctionArgs, Problem> {
        Ok(match args {
            ArgsForm::Parens { parens, args } => {
                let mut out = Vec::new();
                for (a, p) in args {
                    let e = match a {
                        ArgForm::Slot(s) => self.slot(id, *s)?,
                        ArgForm::Table(t) => Expression::TableConstructor(Box::new(self.table(id, t)?)),
                    };
                    out.push((e, p.clone()));
                }
                FunctionArgs::Parentheses {
                    parentheses: parens.clone(),
                    arguments: punct(out),
                }
            }
            ArgsForm::String(s) => match self.slot(id, *s)? {
                Expression::String(t) => FunctionArgs::String(t),
                other => {
                    return Err(problem(
                        id,
                        format!("`{}` is called with a string literal, not `{}`", self.g.nodes[id].title, bare(&other)),
                    ))
                }
            },
            ArgsForm::Table(t) => FunctionArgs::TableConstructor(Box::new(self.table(id, t)?)),
        })
    }

    fn table(&mut self, id: NodeId, t: &TableForm) -> Result<TableConstructor, Problem> {
        let mut fields = Vec::new();
        for (f, p) in &t.fields {
            let field = match f {
                FieldForm::Named { key, equal, value } => Field::NameKey {
                    key: key.clone(),
                    equal: equal.clone(),
                    value: Box::new(self.slot(id, *value)?),
                },
                FieldForm::Keyed {
                    brackets,
                    key,
                    equal,
                    value,
                } => Field::ExpressionKey {
                    brackets: brackets.clone(),
                    key: key.clone(),
                    equal: equal.clone(),
                    value: Box::new(self.slot(id, *value)?),
                },
                FieldForm::Positional(value) => Field::NoKey(self.slot(id, *value)?),
            };
            fields.push((field, p.clone()));
        }
        Ok(TableConstructor::new().with_braces(t.braces.clone()).with_fields(punct(fields)))
    }
}

/// The single function expression of a `X = function` statement with its body replaced.
fn closure_with_body(exprs: &Punctuated<Expression>, block: &Block) -> Punctuated<Expression> {
    exprs
        .pairs()
        .map(|p| {
            let e = match p.value() {
                Expression::Function(f) => {
                    let body = f.body().clone().with_block(block.clone());
                    Expression::Function(Box::new(f.as_ref().clone().with_body(body)))
                }
                other => other.clone(),
            };
            Pair::new(e, p.punctuation().cloned())
        })
        .collect()
}

/// A variable's name token, with the whitespace and comments of the place it was used.
fn name_token(name: &str, site: Option<&TokenReference>) -> TokenReference {
    let token = Token::new(TokenType::Identifier {
        identifier: name.into(),
    });
    match site {
        Some(s) => TokenReference::new(s.leading_trivia().cloned().collect(), token, s.trailing_trivia().cloned().collect()),
        None => TokenReference::new(Vec::new(), token, Vec::new()),
    }
}

/// An expression as a call/var chain head and its suffixes.
fn split_chain(e: Expression) -> Result<(Prefix, Vec<Suffix>), String> {
    Ok(match e {
        Expression::Var(Var::Name(t)) => (Prefix::Name(t), Vec::new()),
        Expression::Var(Var::Expression(ve)) => (ve.prefix().clone(), ve.suffixes().cloned().collect()),
        Expression::FunctionCall(fc) => (fc.prefix().clone(), fc.suffixes().cloned().collect()),
        e @ Expression::Parentheses { .. } => (Prefix::Expression(Box::new(e)), Vec::new()),
        other => return Err(format!("`{}` cannot be indexed or called; wrap it in parentheses", bare(&other))),
    })
}

fn with_suffixes(e: Expression, suffixes: &[Suffix]) -> Result<Expression, String> {
    if suffixes.is_empty() {
        return Ok(e);
    }
    let (prefix, mut all) = split_chain(e)?;
    all.extend(suffixes.iter().cloned());
    Ok(if matches!(all.last(), Some(Suffix::Call(_))) {
        Expression::FunctionCall(FunctionCall::new(prefix).with_suffixes(all))
    } else {
        Expression::Var(Var::Expression(Box::new(VarExpression::new(prefix).with_suffixes(all))))
    })
}

// ───────────────────────────────────────────────────────────────────── editing

/// Whether a value of type `from` may feed a pin of type `to`.
pub fn compatible(from: PinTy, to: PinTy) -> bool {
    from == to || from == PinTy::Any || to == PinTy::Any || (to == PinTy::Obj && from == PinTy::Arr)
}

impl Graph {
    /// The Lua a pin holds now, or why it holds none.
    pub fn pin_source(&self, node: NodeId, pin: usize) -> Result<String, String> {
        let mut em = Emitter::new(self);
        em.scope = all_vars(self);
        em.input(node, pin).map(|e| bare(&e)).map_err(|p| p.message)
    }

    /// A statement's own Lua (its blocks included), without the whitespace and comments around
    /// it and with its lines dedented to start at column 0 — what the inspector shows and what
    /// the palette re-adds.
    pub fn node_source(&self, node: NodeId) -> Result<String, String> {
        let n = self.nodes.get(node).ok_or("no such node")?;
        let mut em = Emitter::new(self);
        em.scope = all_vars(self);
        em.indent = n.indent.clone();
        let text = match &n.form {
            Form::Return { ret, values } => {
                let returns = em.slots(node, values).map_err(|p| p.message)?;
                strip_outer(LastStmt::Return(ret.clone().with_returns(returns))).to_string()
            }
            Form::Break(t) => strip_outer(LastStmt::Break(t.clone())).to_string(),
            _ if n.exec_in => strip_outer(em.stmt(node).map_err(|p| p.message)?).to_string(),
            _ => strip_outer(em.expr(node).map_err(|p| p.message)?).to_string(),
        };
        Ok(text
            .lines()
            .map(|l| l.strip_prefix(n.indent.as_str()).unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    /// The data wire feeding a pin, if one does.
    pub fn feeder(&self, node: NodeId, pin: usize) -> Option<(NodeId, usize)> {
        self.data.iter().find(|w| w.to == (node, pin)).map(|w| w.from)
    }

    /// Where an exec output leads, if it is wired.
    pub fn exec_target(&self, node: NodeId, out: usize) -> Option<NodeId> {
        self.exec.iter().find(|w| w.from == (node, out)).map(|w| w.to)
    }

    /// The whitespace and comments around what a pin holds now, to carry over to what replaces it.
    fn pin_trivia(&self, node: NodeId, pin: usize) -> (Vec<Token>, Vec<Token>) {
        let mut em = Emitter::new(self);
        em.scope = all_vars(self);
        match em.input(node, pin) {
            Ok(e) => outer_trivia(&e),
            Err(_) => match &self.nodes[node].inputs[pin].value {
                Value::Wired { site: Some(s), .. } => {
                    (s.leading_trivia().cloned().collect(), s.trailing_trivia().cloned().collect())
                }
                _ => (Vec::new(), Vec::new()),
            },
        }
    }

    /// Replace what a pin holds with the Lua expression `lua`: a literal, a global path, or any
    /// other expression (kept as Lua). A wired pin changes by its wire.
    pub fn set_value(&mut self, node: NodeId, pin: usize, lua: &str) -> Result<(), String> {
        let input = self.nodes.get(node).and_then(|n| n.inputs.get(pin)).ok_or("no such pin")?;
        if self.feeder(node, pin).is_some() {
            return Err(format!("`{}` is wired; break the wire before typing a value", input.label));
        }
        let e = parse_expression(lua)?;
        let (lead, trail) = self.pin_trivia(node, pin);
        let (value, ty) = typed_value(retrivia(e, lead, trail));
        let input = &mut self.nodes[node].inputs[pin];
        input.value = value;
        input.ty = ty;
        Ok(())
    }

    /// Wire output `from` into input `to`, replacing what fed it.
    pub fn connect_data(&mut self, from: (NodeId, usize), to: (NodeId, usize)) -> Result<(), String> {
        let out = self.nodes.get(from.0).and_then(|n| n.outputs.get(from.1)).ok_or("no such output")?;
        let input = self.nodes.get(to.0).and_then(|n| n.inputs.get(to.1)).ok_or("no such pin")?;
        if !compatible(out.ty, input.ty) {
            return Err(format!(
                "`{}` carries {}, and `{}` takes {}",
                out.label,
                out.ty.name(),
                input.label,
                input.ty.name()
            ));
        }
        if from.0 == to.0 || self.feeds(to.0, from.0) {
            return Err("that wire would make an expression contain itself".into());
        }
        let ty = if out.ty == PinTy::Any { input.ty } else { out.ty };
        let (lead, trail) = self.pin_trivia(to.0, to.1);
        self.data.retain(|w| w.to != to);
        self.data.push(DataWire { from, to });
        let input = &mut self.nodes[to.0].inputs[to.1];
        input.value = Value::Wired {
            site: Some(TokenReference::new(lead, ident("_"), trail)),
            suffixes: Vec::new(),
        };
        input.ty = ty;
        Ok(())
    }

    /// Remove the wire feeding a pin. The pin is empty until something is wired or typed in.
    pub fn break_data(&mut self, to: (NodeId, usize)) {
        if self.feeder(to.0, to.1).is_none() {
            return;
        }
        let (lead, trail) = self.pin_trivia(to.0, to.1);
        self.data.retain(|w| w.to != to);
        self.nodes[to.0].inputs[to.1].value = Value::Wired {
            site: Some(TokenReference::new(lead, ident("_"), trail)),
            suffixes: Vec::new(),
        };
    }

    /// Whether `a`'s value reaches `b` through data wires.
    fn feeds(&self, a: NodeId, b: NodeId) -> bool {
        let mut stack = vec![a];
        let mut seen = vec![false; self.nodes.len()];
        while let Some(n) = stack.pop() {
            if n == b {
                return true;
            }
            if std::mem::replace(&mut seen[n], true) {
                continue;
            }
            stack.extend(self.data.iter().filter(|w| w.from.0 == n).map(|w| w.to.0));
        }
        false
    }

    /// Wire exec output `from` to statement `to`. `to` leaves whatever chain it was on.
    pub fn connect_exec(&mut self, from: (NodeId, usize), to: NodeId) -> Result<(), String> {
        let n = self.nodes.get(from.0).ok_or("no such node")?;
        if from.1 >= n.exec_outs.len() {
            return Err("no such exec output".into());
        }
        let target = self.nodes.get(to).ok_or("no such node")?;
        if !target.exec_in {
            return Err(format!("`{}` is a value, not a statement; wire it into a pin", target.title));
        }
        if to == from.0 || self.runs_into(to, from.0) {
            return Err(format!("`{}` already leads back here; an exec chain cannot loop", target.title));
        }
        self.exec.retain(|w| w.from != from && w.to != to);
        self.exec.push(ExecWire { from, to });
        Ok(())
    }

    /// Remove the exec wire leaving `from`.
    pub fn break_exec(&mut self, from: (NodeId, usize)) {
        self.exec.retain(|w| w.from != from);
    }

    /// Whether execution from `a` (any of its exec outputs) reaches `b`.
    fn runs_into(&self, a: NodeId, b: NodeId) -> bool {
        let mut stack = vec![a];
        let mut seen = vec![false; self.nodes.len()];
        while let Some(n) = stack.pop() {
            if n == b {
                return true;
            }
            if std::mem::replace(&mut seen[n], true) {
                continue;
            }
            stack.extend(self.exec.iter().filter(|w| w.from.0 == n).map(|w| w.to));
        }
        false
    }

    /// `ids` with everything that belongs to them: the data nodes feeding them only, and the
    /// statements in their blocks and anonymous functions.
    fn subtree(&self, ids: &[NodeId]) -> Vec<bool> {
        let mut inside = vec![false; self.nodes.len()];
        let mut stack: Vec<NodeId> = ids.iter().copied().filter(|&i| i != ENTRY).collect();
        while let Some(n) = stack.pop() {
            if std::mem::replace(&mut inside[n], true) {
                continue;
            }
            let node = &self.nodes[n];
            for w in self.exec.iter().filter(|w| w.from.0 == n && Some(w.from.1) != node.next) {
                stack.push(w.to);
            }
            for w in self.exec.iter().filter(|w| w.from.0 == n && Some(w.from.1) == node.next) {
                if !ids.contains(&n) {
                    stack.push(w.to);
                }
            }
            for w in self.data.iter().filter(|w| w.to.0 == n) {
                let p = &self.nodes[w.from.0];
                if !p.exec_in && w.from.0 != ENTRY {
                    stack.push(w.from.0);
                }
            }
        }
        inside
    }

    /// Delete nodes with everything that belongs to them. A deleted statement's chain closes up
    /// over it. Returns where each old node id went (`None` for deleted ones).
    pub fn delete(&mut self, ids: &[NodeId]) -> Vec<Option<NodeId>> {
        let gone = self.subtree(ids);
        for &id in ids {
            if id == ENTRY || !self.nodes[id].exec_in {
                continue;
            }
            let Some(into) = self.exec.iter().find(|w| w.to == id).map(|w| w.from) else { continue };
            if gone[into.0] {
                continue;
            }
            let mut cur = id;
            let next = loop {
                match self.nodes[cur].next.and_then(|o| self.exec_target(cur, o)) {
                    Some(t) if gone[t] => cur = t,
                    other => break other,
                }
            };
            self.exec.retain(|w| w.from != into);
            if let Some(next) = next {
                self.exec.push(ExecWire { from: into, to: next });
            }
        }
        let mut map = Vec::with_capacity(self.nodes.len());
        let mut kept = 0;
        for &g in &gone {
            map.push(if g { None } else { kept += 1; Some(kept - 1) });
        }
        let old = std::mem::take(&mut self.nodes);
        self.nodes = old.into_iter().zip(&gone).filter(|(_, g)| !**g).map(|(n, _)| n).collect();
        self.exec = self
            .exec
            .iter()
            .filter_map(|w| Some(ExecWire { from: (map[w.from.0]?, w.from.1), to: map[w.to]? }))
            .collect();
        self.data = self
            .data
            .iter()
            .filter_map(|w| Some(DataWire { from: (map[w.from.0]?, w.from.1), to: (map[w.to.0]?, w.to.1) }))
            .collect();
        map
    }

    /// Copy statements with everything that belongs to them. The copies are not on any chain
    /// yet; wires from outside (the locals they read) are copied too. Returns the copies of
    /// `ids`, in order.
    pub fn clone_nodes(&mut self, ids: &[NodeId]) -> Vec<NodeId> {
        let inside = self.subtree(ids);
        let base = self.nodes.len();
        let mut map = vec![None; base];
        for (i, &inn) in inside.iter().enumerate() {
            if inn {
                map[i] = Some(self.nodes.len());
                let mut copy = self.nodes[i].clone();
                copy.fresh = copy.exec_in;
                self.nodes.push(copy);
            }
        }
        let exec: Vec<ExecWire> = self
            .exec
            .iter()
            .filter_map(|w| {
                let from = map[w.from.0]?;
                let n = &self.nodes[w.from.0];
                if ids.contains(&w.from.0) && Some(w.from.1) == n.next {
                    return None;
                }
                Some(ExecWire { from: (from, w.from.1), to: map[w.to]? })
            })
            .collect();
        let data: Vec<DataWire> = self
            .data
            .iter()
            .filter_map(|w| {
                let to = map[w.to.0]?;
                let from = map[w.from.0].unwrap_or(w.from.0);
                Some(DataWire { from: (from, w.from.1), to: (to, w.to.1) })
            })
            .collect();
        self.exec.extend(exec);
        self.data.extend(data);
        ids.iter().filter_map(|&i| map.get(i).copied().flatten()).collect()
    }

    /// Add a statement written as Lua (`MrxMusic.PlaySpecialMusic("mu_x")`) to this canvas,
    /// lifted the way the source's own statements are; names of this canvas's parameters wire to
    /// them. It is on no chain until wired.
    pub fn add_statement(&mut self, lua: &str, ctx: &Context<'_>) -> Result<NodeId, String> {
        let ast = full_moon::parse_fallible(lua, full_moon::LuaVersion::lua52())
            .into_result()
            .map_err(|errs| errs.iter().map(|e| e.error_message().to_string()).collect::<Vec<_>>().join("; "))?;
        let block = ast.nodes();
        let stmts: Vec<&Stmt> = block.stmts().collect();
        let before = self.nodes.len();
        let mut l = Lifter::new(ctx, Vec::new(), Node::new(Category::Chunk, String::new(), 0, false, Form::Entry), false);
        l.g = std::mem::replace(
            self,
            Graph {
                nodes: Vec::new(),
                exec: Vec::new(),
                data: Vec::new(),
            },
        );
        for (i, o) in l.g.nodes[ENTRY].outputs.iter().enumerate() {
            l.frames[0].push((o.label.clone(), (ENTRY, i)));
        }
        let id = match (stmts.as_slice(), block.last_stmt()) {
            ([s], None) => Ok(l.stmt(s)),
            ([], Some(last)) => Ok(l.last(last)),
            _ => Err("write one statement".to_string()),
        };
        *self = l.g;
        let id = id?;
        for n in &mut self.nodes[before..] {
            n.fresh = n.exec_in;
        }
        Ok(id)
    }
}

/// Every variable output of a canvas, for reading one pin outside of an emission.
fn all_vars(g: &Graph) -> Vec<(NodeId, usize)> {
    g.nodes
        .iter()
        .enumerate()
        .flat_map(|(i, n)| {
            n.outputs
                .iter()
                .enumerate()
                .filter(|(_, o)| o.kind != OutKind::Value)
                .map(move |(k, _)| (i, k))
        })
        .collect()
}

/// `lua` as one expression.
fn parse_expression(lua: &str) -> Result<Expression, String> {
    let src = format!("return {lua}");
    let ast = full_moon::parse_fallible(&src, full_moon::LuaVersion::lua52())
        .into_result()
        .map_err(|errs| errs.iter().map(|e| e.error_message().to_string()).collect::<Vec<_>>().join("; "))?;
    let block = ast.nodes();
    let value = match (block.stmts().next(), block.last_stmt()) {
        (None, Some(LastStmt::Return(r))) if r.returns().len() == 1 => r.returns().iter().next().cloned(),
        _ => None,
    };
    match value {
        Some(e) => Ok(retrivia(e, Vec::new(), Vec::new())),
        None => Err(format!("`{lua}` is not one Lua expression")),
    }
}

/// The value and type a typed-in expression gives a pin.
fn typed_value(e: Expression) -> (Value, PinTy) {
    match &e {
        Expression::Number(_) => (Value::Literal(e), PinTy::Num),
        Expression::String(_) => (Value::Literal(e), PinTy::Str),
        Expression::Symbol(t) => {
            let ty = match t.token().token_type() {
                TokenType::Symbol {
                    symbol: Symbol::True | Symbol::False,
                } => PinTy::Bool,
                _ => PinTy::Any,
            };
            (Value::Literal(e), ty)
        }
        _ if is_literal(&e) => (Value::Literal(e), PinTy::Num),
        Expression::TableConstructor(tc) if is_literal_list(tc) => (Value::Array(tc.clone()), PinTy::Arr),
        Expression::Var(v @ Var::Name(_)) => (Value::Path(v.clone()), PinTy::Obj),
        Expression::Var(v @ Var::Expression(ve))
            if matches!(ve.prefix(), Prefix::Name(_))
                && ve.suffixes().all(|s| matches!(s, Suffix::Index(Index::Dot { .. }))) =>
        {
            (Value::Path(v.clone()), PinTy::Obj)
        }
        _ => (Value::Lua(Lua::Expr(e)), PinTy::Any),
    }
}

impl PinTy {
    pub fn name(self) -> &'static str {
        match self {
            PinTy::Exec => "exec",
            PinTy::Str => "str",
            PinTy::Num => "num",
            PinTy::Bool => "bool",
            PinTy::Obj => "obj",
            PinTy::Arr => "arr",
            PinTy::Any => "any",
        }
    }
}

fn ident(name: &str) -> Token {
    Token::new(TokenType::Identifier {
        identifier: name.into(),
    })
}

fn whitespace(s: &str) -> Token {
    Token::new(TokenType::Whitespace { characters: s.into() })
}

fn ends_line(trivia: &[Token]) -> bool {
    trivia.last().is_some_and(|t| t.to_string().ends_with('\n'))
}

/// The leading trivia of a node's first token and the trailing trivia of its last.
fn outer_trivia(n: &impl AstNode) -> (Vec<Token>, Vec<Token>) {
    let first = n.tokens().next();
    let last = n.tokens().last();
    (
        first.map(|t| t.leading_trivia().cloned().collect()).unwrap_or_default(),
        last.map(|t| t.trailing_trivia().cloned().collect()).unwrap_or_default(),
    )
}

/// The whitespace a statement's line starts with.
fn indent_of(n: &impl AstNode) -> String {
    let Some(first) = n.tokens().next() else { return String::new() };
    let lead: String = first.leading_trivia().map(|t| t.to_string()).collect();
    let line = lead.rsplit('\n').next().unwrap_or("");
    if line.chars().all(char::is_whitespace) {
        line.to_string()
    } else {
        String::new()
    }
}

/// A node without the whitespace and comments before its first token and after its last.
fn strip_outer<T: AstNode + VisitMut>(n: T) -> T {
    retrivia(n, Vec::new(), Vec::new())
}

/// `e` with `lead` before its first token and `trail` after its last.
fn retrivia<T: AstNode + VisitMut>(e: T, lead: Vec<Token>, trail: Vec<Token>) -> T {
    struct Swap {
        i: usize,
        last: usize,
        lead: Option<Vec<Token>>,
        trail: Option<Vec<Token>>,
    }
    impl VisitorMut for Swap {
        fn visit_token_reference(&mut self, t: TokenReference) -> TokenReference {
            let i = self.i;
            self.i += 1;
            let mut leading: Vec<Token> = t.leading_trivia().cloned().collect();
            let mut trailing: Vec<Token> = t.trailing_trivia().cloned().collect();
            if i == 0 {
                leading = self.lead.take().unwrap_or_default();
            }
            if i == self.last {
                trailing = self.trail.take().unwrap_or_default();
            }
            TokenReference::new(leading, t.token().clone(), trailing)
        }
    }
    let last = e.tokens().count().saturating_sub(1);
    e.visit_mut(&mut Swap {
        i: 0,
        last,
        lead: Some(lead),
        trail: Some(trail),
    })
}

/// A statement made in the editor, its lines re-based from indentation `old` to `new` and its
/// last token ending the line. Only the statement's own tokens move: the blocks inside it are
/// other statements, already emitted at their own indentation.
fn reindent<T: AstNode + VisitMut>(s: T, old: &str, new: &str) -> T {
    struct Reindent<'a> {
        old: &'a str,
        new: &'a str,
        depth: usize,
        line_start: bool,
        i: usize,
        last: usize,
    }
    impl VisitorMut for Reindent<'_> {
        fn visit_block(&mut self, b: Block) -> Block {
            self.depth += 1;
            b
        }
        fn visit_block_end(&mut self, b: Block) -> Block {
            self.depth -= 1;
            b
        }
        fn visit_token_reference(&mut self, t: TokenReference) -> TokenReference {
            let i = self.i;
            self.i += 1;
            let mut leading: Vec<Token> = t.leading_trivia().cloned().collect();
            let mut trailing: Vec<Token> = t.trailing_trivia().cloned().collect();
            if self.depth == 0 {
                if self.line_start {
                    let split = leading
                        .iter()
                        .rposition(|t| !matches!(t.token_type(), TokenType::Whitespace { .. }))
                        .map_or(0, |p| p + 1);
                    let tail: String = leading[split..].iter().map(|t| t.to_string()).collect();
                    let (head, line) = match tail.rfind('\n') {
                        Some(p) => tail.split_at(p + 1),
                        None => ("", tail.as_str()),
                    };
                    let rest = line.strip_prefix(self.old).unwrap_or("");
                    leading.truncate(split);
                    let ws = format!("{head}{}{rest}", self.new);
                    if !ws.is_empty() {
                        leading.push(whitespace(&ws));
                    }
                }
                if i == self.last && !ends_line(&trailing) {
                    trailing.push(whitespace("\n"));
                }
            }
            self.line_start = ends_line(&trailing);
            TokenReference::new(leading, t.token().clone(), trailing)
        }
    }
    let last = s.tokens().count().saturating_sub(1);
    s.visit_mut(&mut Reindent {
        old,
        new,
        depth: 0,
        line_start: true,
        i: 0,
        last,
    })
}

// ───────────────────────────────────────────────────────────────────── palette

/// Something the editor can add, with the Lua it adds.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteEntry {
    pub label: String,
    pub category: Category,
    /// The statement it adds: the shortest real call site in the corpus, or one of Lua's forms.
    pub lua: String,
    /// Corpus call sites (overriding scripts, for a lifecycle hook; 0 for Lua's own forms).
    pub uses: usize,
    /// A function definition, added at the top level of the script rather than in a body.
    pub top_level: bool,
}

/// What the editor can add, from the corpus.
#[derive(Clone, Debug, Default)]
pub struct Palette {
    /// The base task's functions, by how many mission scripts override them.
    pub lifecycle: Vec<PaletteEntry>,
    /// `Event.*` kinds the scripts create events for.
    pub events: Vec<PaletteEntry>,
    /// The objective classes `CreateChild` names.
    pub objectives: Vec<PaletteEntry>,
    /// Calls on globals, by the global (`MrxMusic`, `Pg`), most-used group first.
    pub calls: Vec<(String, Vec<PaletteEntry>)>,
    /// Lua's own statements.
    pub flow: Vec<PaletteEntry>,
}

/// The module every mission task inherits from, whose functions are the lifecycle hooks.
const TASK_BASE: &str = "mrxtask";

impl Palette {
    /// Lift every script in `corpus` and collect what it calls. A script that does not parse,
    /// or a node whose source cannot be read back, is an error naming it.
    pub fn build<'a>(
        index: &CorpusIndex,
        corpus: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Palette, String> {
        type Sites = HashMap<String, (usize, String, Category)>;
        let mut events: Sites = HashMap::new();
        let mut objectives: Sites = HashMap::new();
        let mut calls: HashMap<String, Sites> = HashMap::new();
        let mut overrides: HashMap<String, usize> = HashMap::new();

        fn note(sites: &mut Sites, key: String, lua: String, category: Category) {
            let e = sites.entry(key).or_insert((0, lua.clone(), category));
            e.0 += 1;
            if lua.len() < e.1.len() {
                e.1 = lua;
            }
        }
        fn walk(
            g: &Graph,
            path: &str,
            events: &mut Sites,
            objectives: &mut Sites,
            calls: &mut HashMap<String, Sites>,
        ) -> Result<(), String> {
            for (id, n) in g.nodes.iter().enumerate() {
                if let Some(sub) = &n.sub {
                    walk(sub, path, events, objectives, calls)?;
                }
                if !(n.exec_in && matches!(n.form, Form::Call { .. })) {
                    continue;
                }
                let Some(callee) = n.callee() else { continue };
                let lua = g.node_source(id).map_err(|e| format!("{path}: {e}"))?;
                match n.category {
                    Category::Event => {
                        let kind = n.title.strip_prefix("Event · ").unwrap_or(&n.title).to_string();
                        note(events, kind, lua, Category::Event);
                    }
                    Category::Objective => note(objectives, n.title.clone(), lua, Category::Objective),
                    c if n.calls_global() || callee.starts_with("self:") => {
                        let head = callee.split(['.', ':']).next().unwrap_or(&callee).to_string();
                        note(calls.entry(head).or_default(), callee, lua, c);
                    }
                    _ => {}
                }
            }
            Ok(())
        }

        for (path, src) in corpus {
            let script = Script::parse(src).map_err(|e| format!("{path}: {e}"))?;
            let ctx = index.context(path);
            let chunk = lift(&script, &ctx);
            walk(&chunk, path, &mut events, &mut objectives, &mut calls)?;
            if ctx.bases.iter().any(|b| b == TASK_BASE) {
                for n in &chunk.nodes {
                    if n.is_function() && n.category == Category::Lifecycle {
                        *overrides.entry(n.title.clone()).or_default() += 1;
                    }
                }
            }
        }

        fn ranked(sites: Sites) -> Vec<PaletteEntry> {
            let mut v: Vec<PaletteEntry> = sites
                .into_iter()
                .map(|(label, (uses, lua, category))| PaletteEntry {
                    label,
                    category,
                    lua,
                    uses,
                    top_level: false,
                })
                .collect();
            v.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.label.cmp(&b.label)));
            v
        }

        let mut lifecycle: Vec<PaletteEntry> = index
            .resolve(TASK_BASE, "resident")
            .map(|m| {
                m.functions
                    .iter()
                    .map(|(name, params)| PaletteEntry {
                        label: name.clone(),
                        category: Category::Lifecycle,
                        lua: format!("function {name}({})\nend", params.join(", ")),
                        uses: overrides.get(name).copied().unwrap_or(0),
                        top_level: true,
                    })
                    .collect()
            })
            .unwrap_or_default();
        lifecycle.retain(|e| e.uses > 0);
        lifecycle.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.label.cmp(&b.label)));

        let mut calls: Vec<(String, Vec<PaletteEntry>)> = calls.into_iter().map(|(h, s)| (h, ranked(s))).collect();
        calls.sort_by(|a, b| {
            let total = |v: &[PaletteEntry]| v.iter().map(|e| e.uses).sum::<usize>();
            total(&b.1).cmp(&total(&a.1)).then_with(|| a.0.cmp(&b.0))
        });

        Ok(Palette {
            lifecycle,
            events: ranked(events),
            objectives: ranked(objectives),
            calls,
            flow: lua_forms(),
        })
    }

    /// Every entry, for searching.
    pub fn all(&self) -> impl Iterator<Item = &PaletteEntry> {
        self.flow
            .iter()
            .chain(&self.lifecycle)
            .chain(&self.events)
            .chain(&self.objectives)
            .chain(self.calls.iter().flat_map(|(_, v)| v))
    }
}

/// Lua's own statements, for the palette.
fn lua_forms() -> Vec<PaletteEntry> {
    [
        ("if", Category::Flow, "if true then\nend"),
        ("if … else", Category::Flow, "if true then\nelse\nend"),
        ("while", Category::Flow, "while true do\nend"),
        ("for i = a, b", Category::Flow, "for i = 1, 10 do\nend"),
        ("for k, v in pairs", Category::Flow, "for k, v in pairs({}) do\nend"),
        ("repeat … until", Category::Flow, "repeat\nuntil true"),
        ("do", Category::Flow, "do\nend"),
        ("return", Category::Flow, "return"),
        ("break", Category::Flow, "break"),
        ("local", Category::Variable, "local value = nil"),
        ("set", Category::Variable, "value = nil"),
    ]
    .into_iter()
    .map(|(label, category, lua)| PaletteEntry {
        label: label.into(),
        category,
        lua: lua.into(),
        uses: 0,
        top_level: false,
    })
    .collect()
}

// ───────────────────────────────────────────────────────────────────── compiling

/// Why [`compile`] produced no bytecode.
#[derive(Clone, Debug, PartialEq)]
pub enum CompileFailure {
    /// The graph does not stand for a script yet.
    Graph(Vec<Problem>),
    /// The game's Lua refused the emitted script; the message is Lua's own, with the line.
    Lua(String),
}

/// Emit the script `chunk` stands for and compile it with the game's Lua 5.1.
/// Returns the emitted source and its bytecode.
pub fn compile(script: &Script, chunk: &Graph, chunk_name: &str) -> Result<(String, Vec<u8>), CompileFailure> {
    let source = emit(script, chunk).map_err(CompileFailure::Graph)?;
    let bytecode = mercs2_luac::compile(&source, chunk_name).map_err(|e| CompileFailure::Lua(e.to_string()))?;
    Ok((source, bytecode))
}

// ───────────────────────────────────────────────────────────────────── helpers

fn suffixes_of_call(fc: &FunctionCall) -> Vec<Suffix> {
    fc.suffixes().cloned().collect()
}

/// Chains the graph breaks into nodes: no `[...]` index anywhere.
fn chain_is_liftable(prefix: &Prefix, suffixes: &[Suffix]) -> bool {
    matches!(prefix, Prefix::Name(_) | Prefix::Expression(_))
        && suffixes.iter().all(|s| {
            matches!(
                s,
                Suffix::Index(Index::Dot { .. }) | Suffix::Call(Call::AnonymousCall(_) | Call::MethodCall(_))
            )
        })
}

fn is_literal(e: &Expression) -> bool {
    match e {
        Expression::Number(_) | Expression::String(_) | Expression::Symbol(_) => true,
        Expression::UnaryOperator {
            unop: UnOp::Minus(_),
            expression,
        } => matches!(expression.as_ref(), Expression::Number(_)),
        _ => false,
    }
}

/// A non-empty table of positional literals only: `{"a", "b"}`.
fn is_literal_list(tc: &TableConstructor) -> bool {
    !tc.fields().is_empty()
        && tc.fields().iter().all(|f| matches!(f, Field::NoKey(e) if is_literal(e)))
}

fn param_name(p: &Parameter) -> String {
    match p {
        Parameter::Name(t) | Parameter::Ellipsis(t) => t.token().to_string(),
        other => panic!("Lua 5.1/5.2 has no parameter of the form {other}"),
    }
}

fn line_of(n: &impl AstNode) -> u32 {
    n.start_position().map(|p| p.line() as u32).unwrap_or(0)
}

/// A string literal's contents, when it is a plain quoted one.
pub fn string_value(t: &TokenReference) -> Option<String> {
    match t.token().token_type() {
        TokenType::StringLiteral { literal, .. } => Some(literal.to_string()),
        _ => None,
    }
}

/// A node's source without comments, whitespace collapsed to single spaces.
pub fn bare(n: &impl AstNode) -> String {
    let mut out = String::new();
    let mut gap = false;
    for t in n.tokens() {
        if !out.is_empty() && (gap || t.leading_trivia().next().is_some()) {
            out.push(' ');
        }
        out.push_str(&t.token().to_string());
        gap = t.trailing_trivia().next().is_some();
    }
    out
}

fn first_line(s: &str) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.chars().count() > 48 {
        format!("{}…", line.chars().take(47).collect::<String>())
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn lua_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        for e in entries {
            let p = e.unwrap_or_else(|e| panic!("{}: {e}", dir.display())).path();
            if p.is_dir() {
                lua_files(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("lua") {
                out.push(p);
            }
        }
        out.sort();
    }

    /// The reference corpus as `(corpus path, source)`, named the way the Workshop's loader
    /// names it.
    fn corpus() -> Vec<(String, String)> {
        let home = crate::index::data_home().expect(
            "no workshop_data reference bundle found; the Blueprint gates run over its lua/ and \
             lua_dlc/ corpus",
        );
        let mut out = Vec::new();
        for ((sub, expected), tag) in crate::index::PINNED_CORPUS.into_iter().zip(["lua", "dlc"]) {
            let root = home.join(sub);
            let mut files = Vec::new();
            lua_files(&root, &mut files);
            assert_eq!(
                files.len(),
                expected,
                "{sub}/ under {} holds {} scripts; the gates are pinned to the bundle CI downloads",
                home.display(),
                files.len()
            );
            for f in files {
                let rel = f.strip_prefix(&root).expect("under the root").to_string_lossy().replace('\\', "/");
                let src = std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
                out.push((format!("{tag}/{rel}"), src));
            }
        }
        out
    }

    /// Every canvas, the chunk's and each function's, emits clean.
    fn all_clean(g: &Graph, path: &str, problems: &mut Vec<String>) {
        if let Err(ps) = g.emit_body() {
            for p in ps {
                problems.push(format!("{path}: {}", p.message));
            }
        }
        for n in &g.nodes {
            if let Some(sub) = &n.sub {
                all_clean(sub, path, problems);
            }
        }
    }

    /// The gate: every script lifts into a graph that emits back byte-identical, and no canvas
    /// of an unedited script reports a problem.
    #[test]
    fn every_corpus_script_lifts_and_emits_byte_identical() {
        let corpus = corpus();
        let index = CorpusIndex::build(corpus.iter().map(|(p, s)| (p.as_str(), s.as_str()))).expect("the corpus indexes");
        let mut failures = Vec::new();
        for (path, src) in &corpus {
            let script = Script::parse(src).unwrap_or_else(|e| panic!("{path}: {e}"));
            let graph = lift(&script, &index.context(path));
            all_clean(&graph, path, &mut failures);
            match emit(&script, &graph) {
                Err(ps) => failures.push(format!("{path}: {}", ps[0].message)),
                Ok(out) if out != *src => {
                    let at = out.bytes().zip(src.bytes()).position(|(a, b)| a != b).unwrap_or(out.len().min(src.len()));
                    failures.push(format!(
                        "{path}: emitted {} bytes from {}, first difference at byte {at}: {:?}",
                        out.len(),
                        src.len(),
                        &out[at.saturating_sub(20)..(at + 40).min(out.len())]
                    ));
                }
                Ok(_) => {}
            }
        }
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }

    const RACE: &str = r#"inherit("MrxTaskContract")
function Activated(self)
  MrxTaskContract.Activated(self)
  self:GetVehicles()
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 1 then
    nTimeLimit = 60
  end
  self.oRace = self:CreateChild({
    sName = "PmcCon016Race",
    sModuleName = "MrxTaskRace",
    tCourseLocs = {"loc_1", "loc_2"},
    fOnComplete = function()
      MrxMusic.PlaySpecialMusic("mu_PmcCon016_01")
    end
  })
  self:_CreateEvent(Event.ObjectDeath, {tVehicle}, self.VehicleDeath, {self})
end
function GetVehicles(self)
end
"#;

    const RACE_PATH: &str = "lua/vz/pmccon016.lua";

    fn race_index() -> CorpusIndex {
        CorpusIndex::build([
            (RACE_PATH, RACE),
            ("lua/resident/mrxtaskcontract.lua", "inherit(\"MrxTask\")\nfunction Activated(self) end\n"),
            (
                "lua/resident/mrxtask.lua",
                "function Activated(self) end\nfunction _CreateEvent(self, eEvent, tArgs, fCallback, tCallbackArgs) end\n",
            ),
            ("lua/resident/mrxmusic.lua", "function PlaySpecialMusic(sMusicCue) end\n"),
        ])
        .expect("indexes")
    }

    fn canvas<'g>(chunk: &'g Graph, name: &str) -> &'g Graph {
        chunk
            .nodes
            .iter()
            .find(|n| n.title == name && n.sub.is_some())
            .and_then(|n| n.sub.as_deref())
            .unwrap_or_else(|| panic!("no canvas {name}"))
    }

    #[test]
    fn context_follows_the_inherit_chain() {
        let index = race_index();
        let ctx = index.context(RACE_PATH);
        assert_eq!(ctx.bases, vec!["mrxtaskcontract".to_string(), "mrxtask".to_string()]);
    }

    #[test]
    fn a_mission_function_lifts_to_the_nodes_it_reads_as() {
        let script = Script::parse(RACE).expect("parses");
        let chunk = lift(&script, &race_index().context(RACE_PATH));
        let act = canvas(&chunk, "Activated");
        let titles: Vec<(&str, Category)> = act.nodes.iter().map(|n| (n.title.as_str(), n.category)).collect();
        assert_eq!(titles[0], ("On Activated", Category::Lifecycle));
        assert!(titles.contains(&("Super.Activated", Category::Base)), "{titles:?}");
        assert!(titles.contains(&("GetVehicles", Category::Fn)), "{titles:?}");
        assert!(titles.contains(&("MrxTaskRace", Category::Objective)), "{titles:?}");
        assert!(titles.contains(&("Event · ObjectDeath", Category::Event)), "{titles:?}");
        assert!(titles.contains(&("MrxMusic.PlaySpecialMusic", Category::Music)), "{titles:?}");

        let race = act.nodes.iter().find(|n| n.title == "MrxTaskRace").expect("objective");
        let labels: Vec<&str> = race.inputs.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["set self.oRace", "self", "sName", "sModuleName", "tCourseLocs"]);
        assert_eq!(race.inputs[4].ty, PinTy::Arr);
        assert_eq!(race.exec_outs, vec!["fOnComplete".to_string(), "next".to_string()]);

        let music = act.nodes.iter().position(|n| n.title == "MrxMusic.PlaySpecialMusic").expect("music");
        let race_id = act.nodes.iter().position(|n| n.title == "MrxTaskRace").expect("race");
        assert!(act.exec.contains(&ExecWire { from: (race_id, 0), to: music }));
        assert_eq!(act.nodes[music].inputs[0].label, "sMusicCue");

        let local = act.nodes.iter().position(|n| n.title == "self:GetNumCompletions").expect("local");
        assert_eq!(act.nodes[local].outputs[0].label, "nCompletions");
        let eq = act.nodes.iter().position(|n| n.title == "==").expect("comparison");
        assert!(act.data.contains(&DataWire { from: (local, 0), to: (eq, 0) }));

        let event = act.nodes.iter().find(|n| n.title == "Event · ObjectDeath").expect("event");
        let labels: Vec<&str> = event.inputs.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["self", "eEvent", "tVehicle", "fCallback", "self"]);
        assert_eq!(event.inputs[2].group.as_deref(), Some("tArgs"));
    }

    /// The compile gate: every script without `goto` compiles with the game's Lua 5.1 from its
    /// lifted graph; every one with it (unluac's output for debug-stripped DLC chunks) is refused
    /// by that Lua, not by the graph.
    #[test]
    fn every_corpus_script_compiles_with_the_games_lua() {
        let corpus = corpus();
        let index = CorpusIndex::build(corpus.iter().map(|(p, s)| (p.as_str(), s.as_str()))).expect("the corpus indexes");
        let mut failures = Vec::new();
        let mut refused = 0;
        for (path, src) in &corpus {
            let script = Script::parse(src).unwrap_or_else(|e| panic!("{path}: {e}"));
            let graph = lift(&script, &index.context(path));
            match (compile(&script, &graph, path), script.uses_goto()) {
                (Ok(_), false) => {}
                (Err(CompileFailure::Lua(_)), true) => refused += 1,
                (Ok(_), true) => failures.push(format!("{path}: has goto but compiled")),
                (Err(e), _) => failures.push(format!("{path}: {e:?}")),
            }
        }
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
        assert_eq!(refused, 15, "scripts with goto");
    }

    fn canvas_mut<'g>(chunk: &'g mut Graph, name: &str) -> &'g mut Graph {
        chunk
            .nodes
            .iter_mut()
            .find(|n| n.title == name && n.sub.is_some())
            .and_then(|n| n.sub.as_deref_mut())
            .unwrap_or_else(|| panic!("no canvas {name}"))
    }

    fn node(g: &Graph, title: &str) -> NodeId {
        g.nodes.iter().position(|n| n.title == title).unwrap_or_else(|| panic!("no node {title}"))
    }

    fn race() -> (Script, CorpusIndex) {
        (Script::parse(RACE).expect("parses"), race_index())
    }

    #[test]
    fn typing_a_literal_changes_that_literal_only() {
        let (script, index) = race();
        let ctx = index.context(RACE_PATH);
        let mut chunk = lift(&script, &ctx);
        let act = canvas_mut(&mut chunk, "Activated");
        let music = node(act, "MrxMusic.PlaySpecialMusic");
        act.set_value(music, 0, "\"mu_new\"").expect("sets");
        let (out, _) = compile(&script, &chunk, "pmccon016").expect("compiles");
        assert_eq!(out, RACE.replace("\"mu_PmcCon016_01\"", "\"mu_new\""));
    }

    #[test]
    fn a_new_statement_emits_at_its_blocks_indentation() {
        let (script, index) = race();
        let ctx = index.context(RACE_PATH);
        let mut chunk = lift(&script, &ctx);
        let act = canvas_mut(&mut chunk, "Activated");
        let sup = node(act, "Super.Activated");
        let next = act.nodes[sup].next.expect("continues");
        let after = act.exec_target(sup, next).expect("wired");
        let id = act.add_statement("MrxMusic.StopSpecialMusic()", &ctx).expect("adds");
        assert_eq!(act.nodes[id].category, Category::Music);
        act.connect_exec((sup, next), id).expect("wires in");
        act.connect_exec((id, act.nodes[id].next.expect("continues")), after).expect("wires out");
        let (out, _) = compile(&script, &chunk, "pmccon016").expect("compiles");
        assert_eq!(
            out,
            RACE.replace(
                "  MrxTaskContract.Activated(self)\n",
                "  MrxTaskContract.Activated(self)\n  MrxMusic.StopSpecialMusic()\n"
            )
        );
    }

    #[test]
    fn deleting_a_statement_closes_the_chain_over_it() {
        let (script, index) = race();
        let mut chunk = lift(&script, &index.context(RACE_PATH));
        let act = canvas_mut(&mut chunk, "Activated");
        let get = node(act, "GetVehicles");
        let local = node(act, "self:GetNumCompletions");
        act.delete(&[get, local]);
        assert!(act.nodes.iter().all(|n| n.title != "GetVehicles"));
        let problems = act.emit_body().expect_err("the comparison reads the deleted local");
        assert!(problems.iter().any(|p| p.message.contains("nothing wired")), "{problems:?}");
        let mut chunk = lift(&script, &index.context(RACE_PATH));
        let act = canvas_mut(&mut chunk, "Activated");
        let get = node(act, "GetVehicles");
        act.delete(&[get]);
        let (out, _) = compile(&script, &chunk, "pmccon016").expect("compiles");
        assert_eq!(out, RACE.replace("  self:GetVehicles()\n", ""));
    }

    #[test]
    fn deleting_consecutive_statements_closes_the_chain_over_all_of_them() {
        let (script, index) = race();
        let mut chunk = lift(&script, &index.context(RACE_PATH));
        let act = canvas_mut(&mut chunk, "Activated");
        let sup = node(act, "Super.Activated");
        let get = node(act, "GetVehicles");
        act.delete(&[sup, get]);
        let (out, _) = compile(&script, &chunk, "pmccon016").expect("compiles");
        assert_eq!(out, RACE.replace("  MrxTaskContract.Activated(self)\n  self:GetVehicles()\n", ""));
    }

    #[test]
    fn a_clone_wired_in_emits_a_copy_of_the_line() {
        let (script, index) = race();
        let mut chunk = lift(&script, &index.context(RACE_PATH));
        let act = canvas_mut(&mut chunk, "Activated");
        let music = node(act, "MrxMusic.PlaySpecialMusic");
        let copy = act.clone_nodes(&[music])[0];
        act.connect_exec((music, act.nodes[music].next.expect("continues")), copy).expect("wires");
        let (out, _) = compile(&script, &chunk, "pmccon016").expect("compiles");
        let line = "      MrxMusic.PlaySpecialMusic(\"mu_PmcCon016_01\")\n";
        assert_eq!(out, RACE.replace(line, &format!("{line}{line}")));
    }

    #[test]
    fn a_wire_to_a_local_out_of_scope_is_a_problem() {
        let (script, index) = race();
        let mut chunk = lift(&script, &index.context(RACE_PATH));
        let act = canvas_mut(&mut chunk, "Activated");
        let sup = node(act, "Super.Activated");
        let local = node(act, "self:GetNumCompletions");
        act.connect_data((local, 0), (sup, 0)).expect("types fit");
        let problems = act.emit_body().expect_err("out of scope");
        assert!(problems[0].message.contains("not in scope"), "{problems:?}");
    }

    #[test]
    fn an_exec_wire_back_up_the_chain_is_refused() {
        let (script, index) = race();
        let mut chunk = lift(&script, &index.context(RACE_PATH));
        let act = canvas_mut(&mut chunk, "Activated");
        let sup = node(act, "Super.Activated");
        let get = node(act, "GetVehicles");
        let err = act.connect_exec((get, act.nodes[get].next.expect("continues")), sup).expect_err("a loop");
        assert!(err.contains("cannot loop"), "{err}");
    }

    fn race_corpus() -> Vec<(&'static str, &'static str)> {
        vec![
            (RACE_PATH, RACE),
            ("lua/resident/mrxtaskcontract.lua", "inherit(\"MrxTask\")\nfunction Activated(self) end\n"),
            (
                "lua/resident/mrxtask.lua",
                "function Activated(self) end\nfunction _CreateEvent(self, eEvent, tArgs, fCallback, tCallbackArgs) end\n",
            ),
            ("lua/resident/mrxmusic.lua", "function PlaySpecialMusic(sMusicCue) end\n"),
        ]
    }

    #[test]
    fn the_palette_collects_hooks_events_objectives_and_calls() {
        let corpus = race_corpus();
        let index = CorpusIndex::build(corpus.clone()).expect("indexes");
        let palette = Palette::build(&index, corpus).expect("builds");
        let entry = |v: &[PaletteEntry], label: &str| v.iter().find(|e| e.label == label).cloned();

        // pmccon016 and mrxtaskcontract both inherit from MrxTask and define Activated.
        let hook = entry(&palette.lifecycle, "Activated").expect("hook");
        assert_eq!((hook.uses, hook.lua.as_str(), hook.top_level), (2, "function Activated(self)\nend", true));
        let event = entry(&palette.events, "ObjectDeath").expect("event");
        assert_eq!(event.lua, "self:_CreateEvent(Event.ObjectDeath, {tVehicle}, self.VehicleDeath, {self})");
        let race = entry(&palette.objectives, "MrxTaskRace").expect("objective");
        assert!(race.lua.starts_with("self.oRace = self:CreateChild({\n  sName = \"PmcCon016Race\","), "{}", race.lua);
        let music = palette.calls.iter().find(|(h, _)| h == "MrxMusic").expect("group");
        assert_eq!(music.1[0].lua, "MrxMusic.PlaySpecialMusic(\"mu_PmcCon016_01\")");
        assert_eq!(music.1[0].category, Category::Music);
    }

    /// Every palette entry the corpus yields adds back as a statement, in a function canvas and,
    /// for a hook, at the top of a script.
    #[test]
    fn every_corpus_palette_entry_adds_as_a_statement() {
        let corpus = corpus();
        let pairs = || corpus.iter().map(|(p, s)| (p.as_str(), s.as_str()));
        let index = CorpusIndex::build(pairs()).expect("indexes");
        let palette = Palette::build(&index, pairs()).expect("builds");
        assert!(!palette.lifecycle.is_empty() && !palette.events.is_empty() && !palette.objectives.is_empty());
        let (script, race) = race();
        let ctx = race.context(RACE_PATH);
        let mut failures = Vec::new();
        for e in palette.all() {
            let mut chunk = lift(&script, &ctx);
            let target = if e.top_level { &mut chunk } else { canvas_mut(&mut chunk, "Activated") };
            if let Err(err) = target.add_statement(&e.lua, &ctx) {
                failures.push(format!("{}: {err}\n{}", e.label, e.lua));
            }
        }
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    }

    #[test]
    fn an_unwired_statement_is_a_problem_not_dropped_code() {
        let script = Script::parse(RACE).expect("parses");
        let mut chunk = lift(&script, &race_index().context(RACE_PATH));
        let act = chunk.nodes.iter_mut().find(|n| n.title == "Activated").and_then(|n| n.sub.as_deref_mut()).expect("canvas");
        let super_call = act.nodes.iter().position(|n| n.title == "Super.Activated").expect("super");
        act.exec.retain(|w| w.from.0 != super_call);
        let problems = act.emit_body().expect_err("problems");
        assert!(problems.iter().any(|p| p.message.contains("never run")), "{problems:?}");
    }
}
