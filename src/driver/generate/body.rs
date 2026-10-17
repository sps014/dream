use super::decls::{DeclIndex, IdSpans};
use super::model::{Body, BodyNode, GenSpan};
use super::paths::ProjectPaths;
use super::walk::{Visitor, walk_body};
use dream_syntax::nodes::{
    ExpressionNode as E, FunctionNode, PatternNode, StatementNode as S, Type,
};
use dream_text::text_span::TextSpan;

struct Frame {
    node: BodyNode,
    span: TextSpan,
    has_span: bool,
}

struct Builder<'a> {
    stack: Vec<Frame>,
    spans: &'a mut IdSpans,
    file: Option<&'a str>,
    paths: &'a ProjectPaths,
    index: &'a DeclIndex,
    generics: &'a [String],
    source: Option<&'a str>,
    source_spans: &'a [dream_syntax::nodes::program::SourceNodeSpan],
    used_spans: indexmap::IndexSet<usize>,
    owner_start: usize,
    semantic: &'a dream_sema::gen_facts::GenFacts,
}

impl Builder<'_> {
    fn begin(&mut self, kind: &str, position: Option<TextSpan>) {
        let parent = self.stack.last().expect("body root");
        let span = position.unwrap_or(parent.span);
        let id = format!("{}.{}", parent.node.id, parent.node.children.len());
        let fact = position.and_then(|s| {
            self.semantic
                .expressions
                .get(&(self.file.unwrap_or_default().to_string(), s.start))
        });
        self.stack.push(Frame {
            node: BodyNode {
                id,
                kind: kind.to_string(),
                ty: fact
                    .map(|f| super::type_ref::type_ref(&f.ty, self.index, self.generics))
                    .unwrap_or_else(super::type_ref::unknown_type),
                constant: fact.and_then(|f| f.constant.as_ref()).map(|c| {
                    super::model::ConstantValue {
                        kind: c.kind.clone(),
                        value: c.value.clone(),
                    }
                }),
                symbol: fact.map(|f| f.symbol.clone()).unwrap_or_default(),
                symbol_kind: fact.map(|f| f.symbol_kind.clone()).unwrap_or_default(),
                overload: fact.map(|f| f.overload.clone()).unwrap_or_default(),
                type_args: fact
                    .map(|f| {
                        f.type_args
                            .iter()
                            .map(|t| super::type_ref::type_ref(t, self.index, self.generics))
                            .collect()
                    })
                    .unwrap_or_default(),
                ..Default::default()
            },
            span,
            has_span: position.is_some(),
        });
    }

    fn parsed_span(
        &mut self,
        expression: bool,
        kind: &str,
        anchor: Option<TextSpan>,
    ) -> Option<TextSpan> {
        let anchor = anchor.map(|s| s.start);
        let (index, source) = self.source_spans.iter().enumerate().find(|(i, s)| {
            s.expression == expression
                && s.kind == kind
                && s.anchor == anchor
                && s.span.start >= self.owner_start
                && (anchor.is_some() || !self.used_spans.contains(i))
        })?;
        self.used_spans.insert(index);
        Some(source.span)
    }

    fn finish(&mut self) {
        let mut frame = self.stack.pop().expect("body node");
        frame.node.span = GenSpan {
            file: self.paths.rel(self.file),
            line: frame.span.line_no as u32,
            column: frame.span.col_no as u32,
            length: frame.span.end.saturating_sub(frame.span.start),
        };
        self.spans.insert(&frame.node.id, self.file, frame.span);
        let parent = self.stack.last_mut().expect("body parent");
        if !parent.has_span {
            parent.span = frame.span;
            parent.has_span = true;
        }
        if frame.span.start < parent.span.start {
            parent.span.start = frame.span.start;
            parent.span.line_no = frame.span.line_no;
            parent.span.col_no = frame.span.col_no;
        }
        parent.span.end = parent.span.end.max(frame.span.end);
        parent.node.children.push(frame.node);
    }

    fn types(&mut self, types: impl IntoIterator<Item = Type>) {
        self.stack.last_mut().expect("body node").node.types.extend(
            types
                .into_iter()
                .map(|t| super::type_ref::type_ref(&t, self.index, self.generics)),
        );
    }
}

impl<'a> Visitor<'a> for Builder<'_> {
    fn expr(&mut self, e: &E<'a>) -> bool {
        let kind = e.syntax_kind();
        let parsed_span = self.parsed_span(true, kind, e.position());
        self.begin(kind, e.position());
        if let Some(span) = parsed_span {
            let node = self.stack.last_mut().expect("expression");
            node.span = span;
            node.has_span = true;
        }
        self.stack.last_mut().expect("expression").node.role = "expr".to_string();
        let node = &mut self.stack.last_mut().expect("expression").node;
        match e {
            E::Literal(t) => {
                node.literal_kind = t.get_type();
                node.literal = e
                    .position()
                    .and_then(|s| self.source.and_then(|source| source.get(s.start..s.end)))
                    .map(str::to_string)
                    .unwrap_or_else(|| super::sites::splice_source(e));
            }
            E::Identifier(t)
            | E::FunctionCall(t, ..)
            | E::MemberAccess(_, t)
            | E::MethodCall(_, t, ..)
            | E::NamedArg(t, _) => node.text = t.text.clone(),
            E::Binary(_, t, _) | E::Unary(t, _) | E::IncDec { op: t, .. } => {
                node.operator = t.text.clone()
            }
            E::SyntaxBlock(b) => node.text = super::sites::splice_source(&E::SyntaxBlock(b)),
            E::NameOf(_, path) | E::DeclOf(_, path) => {
                node.text = path
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            }
            _ => {}
        }
        match e {
            E::Cast(_, t, _) | E::SizeOf(_, t) | E::AlignOf(_, t) | E::IsExpression(_, t, _) => {
                self.types([t.clone()])
            }
            E::FunctionCall(_, ts, _) | E::Call(_, ts, _) | E::MethodCall(_, _, ts, _) => {
                self.types(ts.iter().flatten().cloned())
            }
            E::Lambda(l) => self.types(l.parameters.iter().map(|p| p.type_.clone())),
            _ => {}
        }
        true
    }

    fn pattern(&mut self, pattern: &PatternNode) {
        self.begin("Pattern", pattern.position());
        let node = &mut self.stack.last_mut().expect("pattern").node;
        node.role = "expr".to_string();
        match pattern {
            PatternNode::Wildcard(t) | PatternNode::Binding(t) => {
                node.text = t.text.clone();
                node.operator = "binding".to_string();
            }
            PatternNode::Literal(ty) => {
                node.literal_kind = ty.get_type();
                node.literal = super::sites::splice_source(&E::Literal(ty.clone()));
            }
            PatternNode::Variant(owner, name, _) => {
                node.text = owner
                    .as_ref()
                    .map(|o| format!("{}.{}", o.text, name.text))
                    .unwrap_or_else(|| name.text.clone());
                node.operator = "variant".to_string();
            }
            PatternNode::Range(lo, hi) => {
                node.operator = "range".to_string();
                node.text = format!(
                    "{}..{}",
                    super::sites::splice_source(&E::Literal(lo.clone())),
                    super::sites::splice_source(&E::Literal(hi.clone()))
                );
            }
            PatternNode::Tuple(_) => node.operator = "tuple".to_string(),
            PatternNode::Or(_) => node.operator = "or".to_string(),
        }
    }

    fn after_pattern(&mut self, _: &PatternNode) {
        self.finish();
    }

    fn after_expr(&mut self, _: &E<'a>) {
        self.finish();
    }

    fn enter_scope(&mut self) {
        self.begin("Block", None);
    }
    fn exit_scope(&mut self) {
        self.finish();
    }

    fn stmt(&mut self, s: &S<'a>) {
        let (kind, token) = match s {
            S::Assignment(t, _) => ("Assignment", Some(t)),
            S::IndexAssignment(..) => ("IndexAssignment", None),
            S::MemberAssignment(_, t, _) => ("MemberAssignment", Some(t)),
            S::Declaration(t, _, _, _, _) => ("Declaration", Some(t)),
            S::TupleDeclaration { .. } => ("TupleDeclaration", None),
            S::FunctionInvocation(t, ..) => ("FunctionInvocation", Some(t)),
            S::MethodInvocation(_, t, ..) => ("MethodInvocation", Some(t)),
            S::Return(_) => ("Return", None),
            S::Yield(t, _) => ("Yield", Some(t)),
            S::AwaitForEach(t, _) => ("AwaitForEach", Some(t)),
            S::IfElse(..) => ("If", None),
            S::While(..) => ("While", None),
            S::DoWhile(..) => ("DoWhile", None),
            S::For(..) => ("For", None),
            S::ForEach(t, ..) => ("ForEach", Some(t)),
            S::Labeled(..) => ("Labeled", None),
            S::Break(_) => ("Break", None),
            S::Continue(_) => ("Continue", None),
            S::ExpressionStatement(_) => ("Expression", None),
            S::AwaitStmt(_) => ("Await", None),
            S::Switch(..) => ("Switch", None),
            S::Lock(..) => ("Lock", None),
            S::Defer(..) => ("Defer", None),
            S::Overflow(_, t, _) => ("Overflow", Some(t)),
        };
        let parsed_span = self.parsed_span(false, kind, s.position());
        self.begin(kind, token.map(|t| t.position));
        if let Some(span) = parsed_span {
            let node = self.stack.last_mut().expect("statement");
            node.span = span;
            node.has_span = true;
        }
        self.stack.last_mut().expect("statement").node.role = "stmt".to_string();
        match s {
            S::Break(label) | S::Continue(label) => {
                self.stack.last_mut().expect("statement").node.text =
                    label.clone().unwrap_or_default()
            }
            S::Labeled(label, _) => {
                self.stack.last_mut().expect("statement").node.text = label.clone()
            }
            S::Declaration(_, _, _, constant, weak) => {
                self.stack.last_mut().expect("statement").node.operator =
                    match (*constant, *weak) {
                        (false, false) => "let",
                        (true, false) => "const",
                        (false, true) => "let weak",
                        (true, true) => "const weak",
                    }
                    .to_string();
            }
            S::TupleDeclaration {
                is_const: constant, ..
            } => {
                self.stack.last_mut().expect("statement").node.operator =
                    if *constant { "const" } else { "let" }.to_string()
            }
            _ => {}
        }
        if let Some(t) = token {
            self.stack.last_mut().expect("statement").node.text = t.text.clone();
        }
        match s {
            S::Declaration(_, ty, _, _, _) | S::TupleDeclaration { ty, .. } => {
                self.types(ty.iter().cloned())
            }
            S::FunctionInvocation(_, ts, _) | S::MethodInvocation(_, _, ts, _) => {
                self.types(ts.iter().flatten().cloned())
            }
            _ => {}
        }
    }

    fn after_stmt(&mut self, _: &S<'a>) {
        self.finish();
    }
}

/// Preserves structural identities and parsed ranges independently of HIR emission.
pub(super) fn lower(
    f: &FunctionNode<'_>,
    id: &str,
    file: Option<&str>,
    facts: &super::snapshot::ProgramFacts<'_>,
    generics: &[String],
    spans: &mut IdSpans,
) -> Body {
    let root = Frame {
        node: BodyNode {
            id: format!("{id}#b"),
            ..Default::default()
        },
        span: f.name.position,
        has_span: true,
    };
    let mut b = Builder {
        stack: vec![root],
        spans,
        file,
        paths: facts.paths,
        index: facts.index,
        generics,
        semantic: facts.semantic,
        source: file.and_then(|f| facts.sources.get(f)).map(String::as_str),
        source_spans: file
            .and_then(|file| facts.source_spans.get(file))
            .map(|s| s.as_slice())
            .unwrap_or_default(),
        used_spans: indexmap::IndexSet::new(),
        owner_start: f.name.position.start,
    };
    walk_body(&mut b, f.body);
    Body {
        statements: b
            .stack
            .pop()
            .expect("body root")
            .node
            .children
            .pop()
            .map(|n| n.children)
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::generate::snapshot::ProgramFacts;
    use bumpalo::Bump;
    use dream_diagnostics::DiagnosticBag;
    use dream_syntax::{lexer::Lexer, parser::Parser};
    use indexmap::IndexMap;
    use std::sync::Arc;

    fn sample_body(source: &str) -> (Body, IdSpans) {
        let arena = Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        let mut parser = Parser::new(Lexer::new(source.into()), &arena, &mut diagnostics);
        let program = parser.parse().unwrap().get_root().clone();
        assert!(!diagnostics.has_errors(), "{:?}", diagnostics.diagnostics);
        let sources = IndexMap::from([("test.dream".to_string(), program.node_spans.clone())]);
        let paths = ProjectPaths {
            root: std::path::PathBuf::new(),
            has_manifest: false,
        };
        let index = DeclIndex::default();
        let semantic = dream_sema::gen_facts::GenFacts::default();
        let attributes = dream_abi::attributes::UserAttributes::default();
        let config = Arc::new(crate::driver::toolchain::ToolchainConfig::default());
        let facts = ProgramFacts {
            index: &index,
            attributes: &attributes,
            sites: &[],
            call_triggers: &[],
            calls: &[],
            paths: &paths,
            target: "native",
            #[cfg(feature = "native")]
            config: &config,
            source_spans: &sources,
            sources: &std::collections::HashMap::from([(
                "test.dream".to_string(),
                source.to_string(),
            )]),
            semantic: &semantic,
        };
        let mut spans = IdSpans::default();
        let body = lower(
            program
                .functions
                .iter()
                .find(|f| f.name.text == "sample")
                .unwrap(),
            "::sample",
            Some("test.dream"),
            &facts,
            &[],
            &mut spans,
        );
        (body, spans)
    }

    #[test]
    fn parsed_ranges_include_keywords_and_punctuation_and_ids_are_local() {
        let source = "fun sample(x: int): int { let y = x + 2; if true { return y; } return 0; }";
        let (body, spans) = sample_body(source);
        assert_eq!(body.statements.len(), 3);
        let declaration = &body.statements[0];
        assert_eq!(declaration.id, "::sample#b.0.0");
        let (_, span) = spans.get(&declaration.id).unwrap();
        assert_eq!(&source[span.start..span.end], "let y = x + 2;");
        let nested = &body.statements[1].children[1].children[0];
        let (_, span) = spans.get(&nested.id).unwrap();
        assert_eq!(&source[span.start..span.end], "return y;");
    }
    fn nodes(body: &Body) -> Vec<&BodyNode> {
        fn visit<'a>(node: &'a BodyNode, out: &mut Vec<&'a BodyNode>) {
            out.push(node);
            for child in &node.children {
                visit(child, out);
            }
        }
        let mut out = Vec::new();
        for statement in &body.statements {
            visit(statement, &mut out);
        }
        out
    }

    #[test]
    fn unrelated_declarations_do_not_change_body_ids() {
        let source = "fun sample(): int { return 42; }";
        let (first, _) = sample_body(source);
        let (second, _) = sample_body(&format!("fun unrelated(): void {{ return; }}\n{source}"));
        assert_eq!(
            nodes(&first).iter().map(|n| &n.id).collect::<Vec<_>>(),
            nodes(&second).iter().map(|n| &n.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_expression_and_statement_shape_is_retained() {
        let source = r#"async fun sample(): void {
            let a = 1; let b = true; let str = "x"; let character = 'A';
            let ar = [1, 2]; let repeat = [0; 3]; let tuple = (1, 2);
            let set = {1, 2}; let map = {"a": 1}; let (first, second) = tuple;
            let parenthesized = (a); let binary = a + 1; let unary = -a;
            let inc = a++; let index = ar[0]; let cast = (int)a;
            let size = sizeof(int); let align = alignof(int); let name = nameof(a);
            let decl = declof(sample); let type_name = typeof(a);
            let member = a.field; let method = a.method(); let call = f(1);
            let indirect = (f)(2); let ternary = true ? 1 : 2;
            let lambda = (x: int) => { return x; }; let future = f().await;
            let is_type = a is int; let option = a?; let syntax = html { <p>{a}</p> };
            let matched = switch (a) { 0 => 1, _ => 2 };
            f(ref a, value: 1); a = 2; ar[0] = 1; a.field = 3; a.method(2); a + 1;
            if true { a = 1; } else if false { a = 2; } else { a = 3; }
            while true { break; } do { continue; } while false;
            for (let i = 0; i < 1; i = i + 1) { a = i; }
            for (let item in ar) { a = item; }
            switch (a) { case 0: break; default: break; }
            lock(a) {} defer {} checked {} unchecked {}
            label: while true { break label; }
            f().await; yield 1; yield break;
            await for (let item in ar) { a = item; }
            return;
        }"#;
        let (body, _) = sample_body(source);
        let all = nodes(&body);
        let kinds: std::collections::BTreeSet<_> = all
            .iter()
            .map(|n| (n.role.as_str(), n.kind.as_str()))
            .collect();
        for kind in [
            "Literal",
            "Identifier",
            "Array",
            "ArrayRepeat",
            "Tuple",
            "Set",
            "Map",
            "Binary",
            "Unary",
            "IncDec",
            "Parenthesized",
            "FunctionCall",
            "Call",
            "Index",
            "Cast",
            "SizeOf",
            "AlignOf",
            "NameOf",
            "DeclOf",
            "TypeOf",
            "Member",
            "Is",
            "MethodCall",
            "Ternary",
            "Await",
            "Switch",
            "Try",
            "Lambda",
            "NamedArg",
            "Ref",
            "SyntaxBlock",
            "Pattern",
        ] {
            assert!(kinds.contains(&("expr", kind)), "missing {kind}: {kinds:?}");
        }
        for kind in [
            "Assignment",
            "IndexAssignment",
            "MemberAssignment",
            "Declaration",
            "TupleDeclaration",
            "FunctionInvocation",
            "MethodInvocation",
            "Return",
            "Yield",
            "AwaitForEach",
            "If",
            "While",
            "DoWhile",
            "For",
            "ForEach",
            "Labeled",
            "Break",
            "Continue",
            "Expression",
            "Await",
            "Switch",
            "Lock",
            "Defer",
            "Overflow",
        ] {
            assert!(kinds.contains(&("stmt", kind)), "missing {kind}: {kinds:?}");
        }
        assert!(all.iter().any(|n| n.literal == "'A'"));
    }
}
