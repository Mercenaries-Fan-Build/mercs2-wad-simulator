//! The Missions Blueprint's model of a Lua script: full_moon's lossless Lua AST.
//!
//! Every token keeps its leading and trailing trivia (whitespace and comments), so printing the
//! AST reproduces the source byte for byte. That is what lets the Blueprint write an edited
//! function back while every byte of the script it did not touch stays exactly as it was read.
//! The corpus gate in this module's tests holds the whole reference corpus to that.
//!
//! The grammar is Lua 5.2's, a superset of 5.1's syntax: the decompiled DLC scripts carry the
//! `goto`/`::label::` unluac writes for control flow it cannot structure from debug-stripped
//! bytecode, and a model that cannot hold them cannot open those scripts at all. Whether a script
//! is Lua the game runs is the compile's question (`mercs2_luac`, exact Lua 5.1.5), not the
//! parser's.

use std::ops::Range;

use full_moon::ast::lua52::{Goto, Label};
use full_moon::ast::{Ast, Call, Expression, FunctionArgs, FunctionBody, Parameter, Prefix, Stmt, Suffix};
use full_moon::node::Node;
use full_moon::tokenizer::{TokenReference, TokenType};
use full_moon::visitors::Visitor;
use full_moon::LuaVersion;

/// A parsed script.
pub struct Script {
    ast: Ast,
}

/// A function the script defines at its top level, as the Blueprint lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    /// The name as written: `Activated`, `MrxTask.Activated`, `MrxTask:Activated`.
    pub name: String,
    /// `true` for `local function Name` and `local Name = function`.
    pub local: bool,
    /// Parameter names in order; a vararg is `...`.
    pub params: Vec<String>,
    /// 1-based line of the `function` keyword (of `local` for a local definition).
    pub line: u32,
    /// Byte range of the definition in the source, from its first token to its `end`, trivia
    /// excluded.
    pub span: Range<usize>,
}

impl Script {
    /// Parse `src`. The error lists every problem the parser reported, each with its line and
    /// column.
    pub fn parse(src: &str) -> Result<Script, String> {
        full_moon::parse_fallible(src, LuaVersion::lua52())
            .into_result()
            .map(|ast| Script { ast })
            .map_err(|errors| {
                errors
                    .iter()
                    .map(|e| {
                        let (at, _) = e.range();
                        format!("{}:{}: {}", at.line(), at.character(), e.error_message())
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            })
    }

    pub fn ast(&self) -> &Ast {
        &self.ast
    }

    /// Whether the script has a `goto` or a `::label::` — Lua 5.2 syntax the game's Lua 5.1
    /// cannot compile.
    pub fn uses_goto(&self) -> bool {
        #[derive(Default)]
        struct Find(bool);
        impl Visitor for Find {
            fn visit_goto(&mut self, _: &Goto) {
                self.0 = true;
            }
            fn visit_label(&mut self, _: &Label) {
                self.0 = true;
            }
        }
        let mut find = Find::default();
        find.visit_ast(&self.ast);
        find.0
    }

    /// The module named by the script's top-level `inherit("Name")` call, if it makes one.
    pub fn inherits(&self) -> Option<String> {
        self.ast.nodes().stmts().find_map(|stmt| {
            let Stmt::FunctionCall(call) = stmt else { return None };
            let Prefix::Name(name) = call.prefix() else { return None };
            if name.token().to_string() != "inherit" {
                return None;
            }
            let mut suffixes = call.suffixes();
            let (Some(Suffix::Call(Call::AnonymousCall(args))), None) = (suffixes.next(), suffixes.next()) else {
                return None;
            };
            let FunctionArgs::Parentheses { arguments, .. } = args.as_ref() else { return None };
            match arguments.iter().next()?.clone() {
                Expression::String(t) => match t.token().token_type() {
                    TokenType::StringLiteral { literal, .. } => Some(literal.to_string()),
                    _ => None,
                },
                _ => None,
            }
        })
    }

    /// The functions defined by the script's top-level statements, in source order:
    /// `function Name(...)`, `local function Name(...)`, and a single name assigned a function
    /// expression (`Name = function(...)`, `local Name = function(...)`). Functions nested inside
    /// another function's body belong to that function and are not listed.
    pub fn functions(&self) -> Vec<FnDef> {
        let mut out = Vec::new();
        for stmt in self.ast.nodes().stmts() {
            match stmt {
                Stmt::FunctionDeclaration(f) => {
                    let n = f.name();
                    let mut name = joined(n.names());
                    if let (Some(colon), Some(method)) = (n.method_colon(), n.method_name()) {
                        name.push_str(&colon.token().to_string());
                        name.push_str(&method.token().to_string());
                    }
                    out.push(def(name, false, f.function_token(), f.body()));
                }
                Stmt::LocalFunction(f) => {
                    let name = f.name().token().to_string();
                    out.push(def(name, true, f.local_token(), f.body()));
                }
                Stmt::Assignment(a) => {
                    if let (1, 1) = (a.variables().len(), a.expressions().len()) {
                        if let Some(Expression::Function(func)) = a.expressions().iter().next() {
                            let var = a.variables().iter().next().expect("one variable");
                            let first = var.tokens().next().expect("a variable has tokens");
                            let name = var.tokens().map(|t| t.token().to_string()).collect();
                            out.push(def(name, false, first, func.body()));
                        }
                    }
                }
                Stmt::LocalAssignment(a) => {
                    if let (1, 1) = (a.names().len(), a.expressions().len()) {
                        if let Some(Expression::Function(func)) = a.expressions().iter().next() {
                            let name = joined(a.names());
                            out.push(def(name, true, a.local_token(), func.body()));
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }
}

/// `a.b.c` from a punctuated name list, without trivia.
fn joined(names: &full_moon::ast::punctuated::Punctuated<TokenReference>) -> String {
    names
        .pairs()
        .map(|p| match p.punctuation() {
            Some(dot) => format!("{}{}", p.value().token(), dot.token()),
            None => p.value().token().to_string(),
        })
        .collect()
}

fn def(name: String, local: bool, first: &TokenReference, body: &FunctionBody) -> FnDef {
    let start = first.token().start_position();
    let end = body.end_token().token().end_position();
    FnDef {
        name,
        local,
        params: body
            .parameters()
            .iter()
            .map(|p| match p {
                Parameter::Ellipsis(t) | Parameter::Name(t) => t.token().to_string(),
                other => panic!("Lua 5.1 has no parameter form {other}"),
            })
            .collect(),
        line: start.line() as u32,
        span: start.bytes()..end.bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// Every `.lua` under `dir`, sorted, so a failure names the same file run to run.
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

    /// The gate: every script in the reference corpus parses and prints back byte-identical. The
    /// bundle is required — a run without one fails, it does not pass on nothing — and the counts
    /// pin the corpus to the release CI downloads, so a stale or shrunken bundle cannot pass either.
    #[test]
    fn every_corpus_script_round_trips_byte_identical() {
        let home = crate::index::data_home().unwrap_or_else(|e| {
            panic!("the round-trip gate runs over the reference bundle's lua/ and lua_dlc/ corpus: {e}")
        });
        let mut failures = Vec::new();
        for (sub, expected) in crate::index::PINNED_CORPUS {
            let mut files = Vec::new();
            lua_files(&home.join(sub), &mut files);
            assert_eq!(
                files.len(),
                expected,
                "{sub}/ under {} holds {} scripts; the gates are pinned to the bundle CI downloads",
                home.display(),
                files.len()
            );
            for path in files {
                let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                let src = match String::from_utf8(bytes) {
                    Ok(s) => s,
                    Err(e) => {
                        failures.push(format!("{}: not UTF-8: {e}", path.display()));
                        continue;
                    }
                };
                match Script::parse(&src) {
                    Err(e) => failures.push(format!("{}: {e}", path.display())),
                    Ok(script) => {
                        let printed = script.ast().to_string();
                        if printed != src {
                            let at = printed
                                .bytes()
                                .zip(src.bytes())
                                .position(|(a, b)| a != b)
                                .unwrap_or(printed.len().min(src.len()));
                            failures.push(format!(
                                "{}: printed {} bytes from {}, first difference at byte {at}",
                                path.display(),
                                printed.len(),
                                src.len()
                            ));
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{} scripts failed:\n{}", failures.len(), failures.join("\n"));
    }

    #[test]
    fn a_parse_error_names_its_line_and_column() {
        let err = Script::parse("local x = 1\nlocal y = \n").err().expect("an error");
        assert_eq!(err, "2:9: expected an expression");
    }

    #[test]
    fn functions_lists_each_top_level_definition_form() {
        let src = "inherit(\"MrxTask\")\n\
                   function Activated(self)\n  local function inner() end\nend\n\
                   function MrxTask.Done(self, ...) end\n\
                   function MrxTask:Cancel() end\n\
                   local function Helper(a, b) end\n\
                   Callback = function(x) end\n\
                   local Other = function() end\n\
                   a, b = function() end, 1\n";
        let script = Script::parse(src).expect("parses");
        let fns = script.functions();
        let summary: Vec<(&str, bool, Vec<&str>, u32)> = fns
            .iter()
            .map(|f| (f.name.as_str(), f.local, f.params.iter().map(String::as_str).collect(), f.line))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Activated", false, vec!["self"], 2),
                ("MrxTask.Done", false, vec!["self", "..."], 5),
                ("MrxTask:Cancel", false, vec![], 6),
                ("Helper", true, vec!["a", "b"], 7),
                ("Callback", false, vec!["x"], 8),
                ("Other", true, vec![], 9),
            ]
        );
        let activated = &fns[0];
        assert!(src[activated.span.clone()].starts_with("function Activated(self)"));
        assert!(src[activated.span.clone()].ends_with("end\nend"));
    }

    #[test]
    fn uses_goto_finds_gotos_and_labels() {
        let jump = Script::parse("for i = 1, 2 do\n  if i then goto skip end\n  ::skip::\nend\n").expect("parses");
        assert!(jump.uses_goto());
        let plain = Script::parse("local goto_count = 1\n").expect("parses");
        assert!(!plain.uses_goto());
    }

    #[test]
    fn inherits_names_the_top_level_inherit_call() {
        let script = Script::parse("-- a contract\ninherit(\"MrxTaskContract\")\nimport(\"MrxMusic\")\n").expect("parses");
        assert_eq!(script.inherits().as_deref(), Some("MrxTaskContract"));
        let none = Script::parse("import(\"MrxMusic\")\nfunction f() inherit(\"X\") end\n").expect("parses");
        assert_eq!(none.inherits(), None);
    }
}
