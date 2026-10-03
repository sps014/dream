//! Builds the span-indexed symbol model by walking the parsed AST: records declarations and
//! references, infers variable types, and emits inlay hints. Best-effort and tolerant of
//! partially-broken trees.

use dream_types::{DefKind, TyKind, TypeCtx, TypeId};
use indexmap::IndexMap;
use std::cell::RefCell;
use std::collections::HashMap;

use dream::syntax::nodes::struct_node::StructDeclarationNode;
use dream::syntax::nodes::types::CONSTRUCTOR_NAME;
use dream::syntax::nodes::{
    ExpressionNode, FunctionNode, LambdaBody, LambdaNode, PatternNode, ProgramNode, StatementNode,
    SwitchArmBody, SyntaxBlockPart, Type,
};
use dream::syntax::token::syntax_token::SyntaxToken;

use super::{
    base_struct, method_detail, param_names, parse_angle_type_args, signature,
    substitute_named_type_params, type_base, Decl, InlayHintOut, InlayKind, Ref, SymKind, GLOBAL,
};

pub(crate) struct Builder {
    pub(crate) type_ctx: RefCell<TypeCtx>,
    pub(crate) decl_types: HashMap<usize, Type>,
    pub(crate) inferred_types: HashMap<usize, TypeId>,
    pub(crate) callables: HashMap<usize, Callable>,
    pub(crate) member_owners: HashMap<usize, String>,
    pub(crate) decls: Vec<Decl>,
    pub(crate) refs: Vec<Ref>,
    pub(crate) inlay_hints: Vec<InlayHintOut>,
    pub(crate) next_scope: usize,
    pub(crate) is_main: bool,
    /// File owning decls currently being recorded (`None` = the open document).
    pub(crate) current_file: Option<String>,
    /// Parameter names per free function name, used to render parameter-name inlay hints at calls.
    pub(crate) fn_params: HashMap<String, Vec<String>>,
    /// Constructor parameter names per struct name (only when a custom `constructor` is declared).
    pub(crate) ctor_params: HashMap<String, Vec<String>>,
}

pub(crate) struct Callable {
    params: Vec<dream::syntax::nodes::function::ParameterNode>,
    ret: Type,
    generics: Vec<String>,
    owner: Option<String>,
    is_async: bool,
}

mod declarations;
mod expressions;
mod generics;
mod inference;
use inference::{call_result, param_names_from_tokens};
mod statements;
impl Builder {
    fn fresh_scope(&mut self) -> usize {
        let scope = self.next_scope;
        self.next_scope += 1;
        scope
    }

    fn push_decl(
        &mut self,
        token: &SyntaxToken,
        kind: SymKind,
        detail: String,
        scope: usize,
        ty: Option<String>,
    ) {
        if token.text.is_empty() {
            return;
        }

        let doc_comment = Self::doc_comment_from_trivia(token);
        self.decls.push(Decl {
            name: token.text.clone(),
            kind,
            detail,
            doc_comment,
            start: token.position.start,
            end: token.position.end,
            scope,
            ty,
            is_main: self.is_main,
            file_path: self.current_file.clone(),
        });
    }

    /// Extracts the doc comment attached to `token`, i.e. the trailing run of leading comment
    /// trivia that is *contiguous* — each comment immediately followed by the next, with no blank
    /// line in between. All the comments in `leading_trivia` sit directly before the declaration
    /// with no other real token between them (otherwise the lexer would have attached them
    /// elsewhere), so a blank line is the only thing that can separate two trivia comments; when it
    /// does, everything before that gap belongs to an earlier, disconnected block (e.g. a
    /// file-level header) and must not be glued onto the declaration's doc comment.
    fn doc_comment_from_trivia(token: &SyntaxToken) -> Option<String> {
        use dream::syntax::token::token_kind::TokenKind;

        let comments: Vec<_> = token
            .leading_trivia
            .iter()
            .filter(|t| {
                t.kind == TokenKind::LineCommentToken || t.kind == TokenKind::BlockCommentToken
            })
            .collect();
        if comments.is_empty() {
            return None;
        }

        // Walk backwards from the comment closest to the declaration, stopping at the first blank
        // line — i.e. where a comment's end line isn't immediately followed by the next one.
        let mut start = comments.len() - 1;
        while start > 0 {
            let prev = comments[start - 1];
            let cur = comments[start];
            let prev_end_line = prev.position.line_no + prev.text.matches('\n').count();
            if prev_end_line + 1 != cur.position.line_no {
                break;
            }
            start -= 1;
        }

        let mut doc_comment = String::new();
        for c in &comments[start..] {
            let mut text = c.text.trim();
            if text.starts_with("//") {
                text = text.trim_start_matches('/').trim_start();
            } else if text.starts_with("/*") {
                text = text.trim_start_matches("/*").trim_end_matches("*/").trim();
            }
            if !doc_comment.is_empty() {
                doc_comment.push_str("\n\n");
            }
            doc_comment.push_str(text);
        }
        if doc_comment.is_empty() {
            None
        } else {
            Some(doc_comment)
        }
    }

    fn add_ref(&mut self, token: &SyntaxToken, kind: SymKind, scope: usize) {
        self.add_ref_with_receiver(token, kind, scope, None);
    }

    /// Like [`Self::add_ref`], but records the receiver of a field/method/enum-member access
    /// (`recv.token`) when the receiver is a plain identifier, so queries can resolve it directly
    /// instead of re-parsing source text around the reference.
    fn add_ref_with_receiver(
        &mut self,
        token: &SyntaxToken,
        kind: SymKind,
        scope: usize,
        receiver: Option<String>,
    ) {
        if token.text.is_empty() {
            return;
        }
        self.refs.push(Ref {
            name: token.text.clone(),
            kind,
            start: token.position.start,
            end: token.position.end,
            scope,
            is_main: self.is_main,
            receiver,
        });
    }
}

/// The identifier text of `expr`, when it is a plain identifier (e.g. `obj` in `obj.field`, or an
/// enum/struct name in `Color.Red`/`Point.origin`). `None` for any other receiver shape (a call, an
/// index, another member access, etc.), matching what a single-identifier receiver scan could ever
/// recover.
fn receiver_ident(expr: &ExpressionNode) -> Option<String> {
    match expr {
        ExpressionNode::Identifier(token) => Some(token.text.clone()),
        _ => None,
    }
}

fn as_lambda(expr: &ExpressionNode) -> Option<bool> {
    match expr {
        ExpressionNode::Lambda(l) => Some(l.is_async),
        ExpressionNode::Parenthesized(_, inner) => as_lambda(inner),
        ExpressionNode::NamedArg(_, inner) => as_lambda(inner),
        _ => None,
    }
}

fn lambda_node<'a>(expr: &'a ExpressionNode<'a>) -> Option<&'a LambdaNode<'a>> {
    match expr {
        ExpressionNode::Lambda(l) => Some(l),
        ExpressionNode::Parenthesized(_, inner) => lambda_node(inner),
        ExpressionNode::NamedArg(_, inner) => lambda_node(inner),
        _ => None,
    }
}
