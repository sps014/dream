use super::types::Type;
use crate::token::syntax_token::SyntaxToken;
use dream_text::text_span::TextSpan;

/// A pattern matched by a `match` arm. Patterns may nest (a variant's sub-patterns are themselves
/// patterns), enabling forms like `Pair(Some(x), None)`.
#[derive(Debug, Clone)]
pub enum PatternNode {
    /// `_` - matches anything and binds nothing.
    Wildcard(SyntaxToken),
    /// A bare identifier - matches anything and binds the matched value to the name.
    Binding(SyntaxToken),
    /// A constant literal (`0`, `"s"`, `true`) - matches when the subject equals the literal.
    Literal(Type),
    /// A (discriminated-union) variant pattern: an optional `EnumName.` qualifier, the variant
    /// name, and the sub-patterns for its payload fields (positional, in declaration order).
    /// A unit variant has no sub-patterns.
    Variant(Option<SyntaxToken>, SyntaxToken, Vec<PatternNode>),
    /// An inclusive range literal pattern (`1..5`) - matches when the subject is between the two
    /// bounds (inclusive), for an ordered scalar subject type (integers, `char`, floats).
    Range(Type, Type),
    /// An or-pattern (`A | B | C`) - matches when the subject matches any alternative. Every
    /// alternative must be binding-free (a literal, range, wildcard, or payload-free variant);
    /// this is validated during analysis, not here.
    Or(Vec<PatternNode>),
    /// `(p0, p1, …)` — positional tuple pattern, arity ≥ 2. Sub-patterns may nest.
    Tuple(Vec<PatternNode>),
}

impl PatternNode {
    /// A representative source span for diagnostics.
    pub fn position(&self) -> Option<TextSpan> {
        match self {
            PatternNode::Wildcard(t) | PatternNode::Binding(t) => Some(t.position),
            PatternNode::Literal(ty) => ty.get_span(),
            PatternNode::Variant(_, name, _) => Some(name.position),
            PatternNode::Range(lo, _) => lo.get_span(),
            PatternNode::Or(alts) => alts.first().and_then(|p| p.position()),
            PatternNode::Tuple(elems) => elems.first().and_then(|p| p.position()),
        }
    }

    /// Identifier tokens bound by this pattern (`_` excluded).
    pub fn binding_names(&self) -> Vec<&SyntaxToken> {
        let mut out = Vec::new();
        self.collect_binding_names(&mut out);
        out
    }

    fn collect_binding_names<'a>(&'a self, out: &mut Vec<&'a SyntaxToken>) {
        match self {
            PatternNode::Binding(t) if t.text != "_" => out.push(t),
            PatternNode::Tuple(elems) | PatternNode::Or(elems) => {
                for e in elems {
                    e.collect_binding_names(out);
                }
            }
            PatternNode::Variant(_, _, subs) => {
                for s in subs {
                    s.collect_binding_names(out);
                }
            }
            PatternNode::Wildcard(_)
            | PatternNode::Literal(_)
            | PatternNode::Range(..)
            | PatternNode::Binding(_) => {}
        }
    }

    /// Bindings, `_`, and nested tuples only — legal on the left of `let`/`const`.
    pub fn is_irrefutable_let_pattern(&self) -> bool {
        match self {
            PatternNode::Wildcard(_) | PatternNode::Binding(_) => true,
            PatternNode::Tuple(elems) => {
                elems.len() >= 2 && elems.iter().all(|e| e.is_irrefutable_let_pattern())
            }
            _ => false,
        }
    }

    /// This pattern's match alternatives: an or-pattern's elements, otherwise just the pattern
    /// itself.
    pub fn or_alternatives(&self) -> &[PatternNode] {
        match self {
            PatternNode::Or(alts) => alts,
            other => std::slice::from_ref(other),
        }
    }
}
