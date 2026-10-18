//! `ctx.call_sites()` input: syntactic call sites of the functions/methods named in a
//! generator's `@on_call(...)`, with explicit type arguments and best-effort argument types.
//! Only user files are scanned.

use super::registry::CallTrigger;
use super::walk::{Visitor, is_user_file, walk_program};
use crate::driver::source_loader::ProgramAccumulator;
use dream_syntax::nodes::{ExpressionNode, StatementNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;

#[derive(Debug, Clone)]
pub struct FoundCall {
    pub trigger: usize,
    pub file: Option<String>,
    pub token: SyntaxToken,
    pub type_args: Vec<Type>,
    pub arg_types: Vec<Option<Type>>,
}

struct CallCollector<'t> {
    triggers: &'t [CallTrigger],
    index: super::decls::DeclIndex,
    file: Option<String>,
    facts: Option<&'t dream_sema::gen_facts::GenFacts>,
    found: Vec<FoundCall>,
}

impl CallCollector<'_> {
    fn record(
        &mut self,
        owner: Option<&str>,
        name: &SyntaxToken,
        type_args: &Option<Vec<Type>>,
        args: &[ExpressionNode<'_>],
    ) {
        let fact = self.facts.and_then(|facts| {
            facts
                .expressions
                .get(&(self.file.clone().unwrap_or_default(), name.position.start))
        });
        let Some(trigger) = self.triggers.iter().position(|t| {
            if let Some(f) = fact.filter(|f| !f.symbol.is_empty()) {
                f.symbol == t.id
            } else {
                let path = match owner {
                    Some(owner) => format!("{owner}.{}", name.text),
                    None => name.text.clone(),
                };
                self.index
                    .attribute_reference(&path, self.file.as_deref())
                    .as_deref()
                    == Some(t.id.as_str())
            }
        }) else {
            return;
        };
        self.found.push(FoundCall {
            trigger,
            file: self.file.clone(),
            token: name.clone(),
            type_args: fact
                .map(|f| f.type_args.clone())
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| type_args.clone().unwrap_or_default()),
            arg_types: args
                .iter()
                .map(|a| {
                    a.position()
                        .and_then(|p| {
                            self.facts.and_then(|facts| {
                                facts
                                    .expressions
                                    .get(&(self.file.clone().unwrap_or_default(), p.start))
                            })
                        })
                        .map(|f| f.ty.clone())
                })
                .collect(),
        });
    }
}

fn static_receiver<'e>(expr: &'e ExpressionNode<'_>) -> Option<&'e str> {
    match expr {
        ExpressionNode::Identifier(tok) => Some(tok.text.as_str()),
        _ => None,
    }
}

impl<'a> Visitor<'a> for CallCollector<'_> {
    fn enter_file(&mut self, file: Option<&str>) {
        self.file = file.map(str::to_string);
    }

    fn expr(&mut self, e: &ExpressionNode<'a>) -> bool {
        match e {
            ExpressionNode::MethodCall(receiver, method, type_args, args) => {
                if let Some(owner) = static_receiver(receiver) {
                    self.record(Some(owner), method, type_args, args);
                }
            }
            ExpressionNode::FunctionCall(name, type_args, args) => {
                self.record(None, name, type_args, args);
            }
            _ => {}
        }
        true
    }

    fn stmt(&mut self, s: &StatementNode<'a>) {
        match s {
            StatementNode::MethodInvocation(receiver, method, type_args, args) => {
                if let Some(owner) = static_receiver(receiver) {
                    self.record(Some(owner), method, type_args, args);
                }
            }
            StatementNode::FunctionInvocation(name, type_args, args) => {
                self.record(None, name, type_args, args);
            }
            _ => {}
        }
    }
}

/// Call sites of any trigger in user files, in source order.
pub fn collect_calls(
    acc: &ProgramAccumulator<'_>,
    triggers: &[CallTrigger],
    facts: Option<&dream_sema::gen_facts::GenFacts>,
) -> Vec<FoundCall> {
    if triggers.is_empty() {
        return Vec::new();
    }
    let mut c = CallCollector {
        triggers,
        index: super::decls::DeclIndex::build(acc),
        file: None,
        facts,
        found: Vec::new(),
    };
    walk_program(&mut c, acc, &is_user_file);
    c.found
}
