use super::model::GenOrigin;
use super::model::WRAP_PREFIX;
use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::lexer::Lexer;
use dream_syntax::nodes::ExpressionNode;
use dream_syntax::nodes::StatementNode;
use dream_syntax::parser::Parser;
use dream_text::line_text::LineText;
use dream_text::text_span::TextSpan;
use std::io::Error;

/// Parses a Dream expression from `source` and returns it arena-allocated. Diagnostics
/// are remapped onto the origin block in the user's file (offset within the generated
/// source is preserved, clamped to the block).
pub fn parse_expression_source<'a>(
    arena: &'a Bump,
    source: &str,
    diagnostics: &mut DiagnosticBag,
    origin: Option<&GenOrigin>,
    real_source: Option<&str>,
) -> Result<ExpressionNode<'a>, Error> {
    // Wrap as `fun __gen(): void { return <expr>; }` and pull the return expression.
    let wrapped = format!("{}{}; }}\n", WRAP_PREFIX, source);
    let mut local = DiagnosticBag::new(Some("<syntax-replace>".into()));
    let lexer = Lexer::new(wrapped);
    let mut parser = Parser::new(lexer, arena, &mut local);
    let ast = match parser.parse() {
        Ok(a) => a,
        Err(e) => {
            remap_local(&mut local, source, origin, real_source);
            diagnostics.extend(&local);
            return Err(e);
        }
    };
    remap_local(&mut local, source, origin, real_source);
    diagnostics.extend(&local);
    let program = ast.get_root();
    let f = program.functions.first().ok_or_else(|| {
        Error::new(
            std::io::ErrorKind::InvalidData,
            "replace parse produced no function",
        )
    })?;
    for stmt in f.body {
        if let StatementNode::Return(Some(e)) = stmt {
            return Ok(e.clone());
        }
    }
    Err(Error::new(
        std::io::ErrorKind::InvalidData,
        "replace parse: expected return expression",
    ))
}

/// Rewrites `<syntax-replace>`-labeled diagnostics onto the origin block. Offsets inside
/// the generated source are preserved and clamped to the block; line/col are recomputed
/// against the user's real file so the rendered squiggle lands in their `html {}` block.
pub(super) fn remap_local(
    local: &mut DiagnosticBag,
    source: &str,
    origin: Option<&GenOrigin>,
    real_source: Option<&str>,
) {
    let Some(origin) = origin else { return };
    let line_text = real_source.map(|src| LineText::new(src.to_string()));
    for d in local.diagnostics.iter_mut() {
        let Some(span) = d.span.take() else { continue };
        let off = span
            .start
            .saturating_sub(WRAP_PREFIX.len())
            .min(source.len());
        let abs = origin.block_start.saturating_add(off);
        d.file_path = Some(origin.real_file.clone());
        d.span = Some(match &line_text {
            Some(lt) => TextSpan::new((abs, abs + 1), lt),
            None => TextSpan {
                start: abs,
                end: abs + 1,
                line_no: 0,
                col_no: 0,
            },
        });
    }
}
