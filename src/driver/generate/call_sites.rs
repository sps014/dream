//! `ctx.call_sites()` input: syntactic call sites of the functions/methods named in a
//! generator's `@on_call(...)`, with explicit type arguments and best-effort argument types.
//! Only user files are scanned.

use super::registry::CallTrigger;
use super::walk::{Visitor, is_user_file, walk_program};
use crate::driver::source_loader::ProgramAccumulator;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, StatementNode, Type};
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
    file: Option<String>,
    scopes: Vec<usize>,
    locals: Vec<(String, Type)>,
    found: Vec<FoundCall>,
}

/// Reconstructs an argument's type from locals, casts and collection constructors.
fn infer_expr_type(expr: &ExpressionNode<'_>, locals: &[(String, Type)]) -> Option<Type> {
    match expr {
        ExpressionNode::Identifier(tok) => locals
            .iter()
            .rev()
            .find(|(n, _)| n == &tok.text)
            .map(|(_, t)| t.clone()),
        ExpressionNode::Parenthesized(_, inner)
        | ExpressionNode::NamedArg(_, inner)
        | ExpressionNode::RefArgument(_, inner) => infer_expr_type(inner, locals),
        ExpressionNode::Cast(_, ty, _) => Some(ty.clone()),
        ExpressionNode::FunctionCall(name, type_args, _) if type_args.is_some() => {
            Some(Type::Struct(name.clone(), type_args.clone()))
        }
        _ => None,
    }
}

impl CallCollector<'_> {
    fn record(
        &mut self,
        owner: Option<&str>,
        name: &SyntaxToken,
        type_args: &Option<Vec<Type>>,
        args: &[ExpressionNode<'_>],
    ) {
        let Some(trigger) = self
            .triggers
            .iter()
            .position(|t| t.owner.as_deref() == owner && t.name == name.text)
        else {
            return;
        };
        self.found.push(FoundCall {
            trigger,
            file: self.file.clone(),
            token: name.clone(),
            type_args: type_args.clone().unwrap_or_default(),
            arg_types: args
                .iter()
                .map(|a| infer_expr_type(a, &self.locals))
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
        self.locals.clear();
        self.scopes.clear();
    }

    fn enter_function(&mut self, f: &FunctionNode<'a>) {
        for p in &f.parameters {
            if p.name.text != "this" {
                self.locals.push((p.name.text.clone(), p.type_.clone()));
            }
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(self.locals.len());
    }

    fn exit_scope(&mut self) {
        if let Some(mark) = self.scopes.pop() {
            self.locals.truncate(mark);
        }
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

    fn after_stmt(&mut self, s: &StatementNode<'a>) {
        if let StatementNode::Declaration(name, ty, init, _) = s {
            let ty = ty.clone().or_else(|| infer_expr_type(init, &self.locals));
            if let Some(t) = ty {
                self.locals.push((name.text.clone(), t));
            }
        }
    }
}

/// Call sites of any trigger in user files, in source order.
pub fn collect_calls(acc: &ProgramAccumulator<'_>, triggers: &[CallTrigger]) -> Vec<FoundCall> {
    if triggers.is_empty() {
        return Vec::new();
    }
    let mut c = CallCollector {
        triggers,
        file: None,
        scopes: Vec::new(),
        locals: Vec::new(),
        found: Vec::new(),
    };
    walk_program(&mut c, acc, &is_user_file);
    c.found
}
