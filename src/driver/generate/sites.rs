//! `name { ... }` syntax-block sites, keyed by `(file, block start)` so two identical blocks are
//! still distinct sites.

use super::walk::{Visitor, walk_program};
use crate::driver::source_loader::ProgramAccumulator;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, SyntaxBlockPart, Type};
use dream_text::text_span::TextSpan;

/// Rewrite key of a syntax site: declaring file + byte offset of the block.
pub type SiteKey = (String, usize);

#[derive(Debug, Clone)]
pub struct Site {
    pub key: SiteKey,
    pub name: String,
    pub name_span: TextSpan,
    /// Reconstructed body text; splices appear as `{expr}`.
    pub body: String,
    pub splices: Vec<String>,
}

pub fn site_key(file: Option<&str>, block: &dream_syntax::nodes::SyntaxBlockNode<'_>) -> SiteKey {
    (file.unwrap_or_default().to_string(), block.block_span.start)
}

#[derive(Default)]
struct SiteCollector {
    file: Option<String>,
    sites: Vec<Site>,
}

impl<'a> Visitor<'a> for SiteCollector {
    fn enter_file(&mut self, file: Option<&str>) {
        self.file = file.map(str::to_string);
    }

    fn expr(&mut self, e: &ExpressionNode<'a>) -> bool {
        if let ExpressionNode::SyntaxBlock(block) = e {
            let mut body = String::new();
            let mut splices = Vec::new();
            for part in &block.parts {
                match part {
                    SyntaxBlockPart::Text(t) => body.push_str(t),
                    SyntaxBlockPart::Splice(e) => {
                        let src = splice_source(e);
                        body.push('{');
                        body.push_str(&src);
                        body.push('}');
                        splices.push(src);
                    }
                }
            }
            self.sites.push(Site {
                key: site_key(self.file.as_deref(), block),
                name: block.name.text.clone(),
                name_span: block.name.position,
                body,
                splices,
            });
        }
        true
    }
}

/// Every syntax-block site in the program, in source order.
pub fn collect_sites(acc: &ProgramAccumulator<'_>) -> Vec<Site> {
    let mut c = SiteCollector::default();
    walk_program(&mut c, acc, &|_| true);
    c.sites
}

/// Reports every site no registered syntax generator claims.
pub fn report_unexpanded(
    sites: &[Site],
    claimed: &dyn Fn(&str) -> bool,
    diagnostics: &mut DiagnosticBag,
) {
    for site in sites.iter().filter(|s| !claimed(&s.name)) {
        diagnostics.file_path = Some(site.key.0.clone()).filter(|f| !f.is_empty());
        diagnostics.report_error(
            format!(
                "unexpanded syntax block '{}'; no generator registered for this introducer (declare '@generator @syntax_block fun {}(ctx: GenContext)' and import its module)",
                site.name, site.name
            ),
            Some(site.name_span),
        );
    }
}

/// Best-effort Dream source of a splice expression (identifiers and simple forms).
pub fn splice_source(expr: &ExpressionNode<'_>) -> String {
    match expr {
        ExpressionNode::Identifier(t) => t.text.clone(),
        ExpressionNode::Literal(t) => match t {
            Type::Integer(tok)
            | Type::Float(tok)
            | Type::Double(tok)
            | Type::String(tok)
            | Type::Boolean(tok)
            | Type::Char(tok)
            | Type::Long(tok)
            | Type::UInt(tok)
            | Type::ULong(tok)
            | Type::Byte(tok) => tok.text.clone(),
            _ => "/*lit*/".into(),
        },
        ExpressionNode::MemberAccess(recv, mem) => format!("{}.{}", splice_source(recv), mem.text),
        ExpressionNode::Binary(l, op, r) => {
            format!("{} {} {}", splice_source(l), op.text, splice_source(r))
        }
        ExpressionNode::Parenthesized(_, inner) => format!("({})", splice_source(inner)),
        _ => "/*expr*/".into(),
    }
}
