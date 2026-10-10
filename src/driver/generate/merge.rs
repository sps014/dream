//! Folds generator output into the program: generated files become declarations, syntax-site
//! replacements are rewritten in place.

use super::rewrite::{rewrite_expression, rewrite_function};
use super::sites::SiteKey;
use crate::driver::source_loader::{ProgramAccumulator, collect_declarations};
use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use indexmap::IndexMap;
use std::io::Error;
use std::rc::Rc;

/// Parses one generated file into `acc`. Its `extend` blocks may read private members of the
/// types they extend, as if declared beside them; imports are limited to std packages the
/// program already loads (generated code is merged after the prelude).
pub fn merge_generated_file<'a>(
    arena: &'a Bump,
    acc: &mut ProgramAccumulator<'a>,
    path: &str,
    text: String,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), Error> {
    acc.file_contents.insert(path.to_string(), text.clone());
    acc.generated_files.insert(path.to_string());
    let mut local = DiagnosticBag::new(Some(path.to_string()));
    let lexer = dream_syntax::lexer::Lexer::new(text);
    let mut parser = dream_syntax::parser::Parser::new(lexer, arena, &mut local);
    let parsed = parser.parse();
    diagnostics.extend(&local);
    let ast = parsed?;
    let program = ast.get_root();
    let loaded = dream_stdlib::resolve_packages_to_load(&acc.requested_std_packages);
    for import in &program.imports {
        let name = import.module_name.text.as_str();
        let ok = dream_stdlib::std_package_from_slash_path(name)
            .is_some_and(|pkg| loaded.iter().any(|p| p.name == pkg.name));
        if !ok {
            diagnostics.report(dream_diagnostics::Diagnostic::new(
                format!(
                    "generated file imports '{name}'; generated code may only import std packages the program already imports"
                ),
                Some(import.module_name.position),
                Some(path.to_string()),
            ));
        }
    }
    if let Some(module_decl) = &program.module {
        acc.file_modules
            .insert(path.to_string(), Rc::from(module_decl.path.text.as_str()));
    }
    let first_extend = acc.all_extends.len();
    collect_declarations(
        program,
        path,
        &mut acc.all_functions,
        &mut acc.all_structs,
        &mut acc.all_interfaces,
        &mut acc.all_enums,
        &mut acc.all_extends,
        &mut acc.all_globals,
    );
    for extend_decl in &mut acc.all_extends[first_extend..] {
        extend_decl.is_synthesized = true;
    }
    Ok(())
}

/// Rewrites every syntax site in `by_site` to its generated expression.
pub fn apply_replacements<'a>(
    arena: &'a Bump,
    acc: &mut ProgramAccumulator<'a>,
    by_site: &IndexMap<SiteKey, String>,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), Error> {
    if by_site.is_empty() {
        return Ok(());
    }
    let contents = &acc.file_contents;
    for g in acc.all_globals.iter_mut() {
        let file = g.file_path.as_deref().map(str::to_string);
        g.initializer = rewrite_expression(
            arena,
            &g.initializer,
            file.as_deref(),
            by_site,
            diagnostics,
            contents,
        )?;
    }
    for f in acc.all_functions.iter_mut() {
        let file = f.file_path.as_deref().map(str::to_string);
        rewrite_function(arena, f, file.as_deref(), by_site, diagnostics, contents)?;
    }
    for s in acc.all_structs.iter_mut() {
        let file = s.file_path.as_deref().map(str::to_string);
        for m in s.methods.iter_mut() {
            rewrite_function(arena, m, file.as_deref(), by_site, diagnostics, contents)?;
        }
    }
    for e in acc.all_enums.iter_mut() {
        let file = e.file_path.as_deref().map(str::to_string);
        for m in e.methods.iter_mut() {
            rewrite_function(arena, m, file.as_deref(), by_site, diagnostics, contents)?;
        }
    }
    for e in acc.all_extends.iter_mut().filter(|e| !e.is_synthesized) {
        let file = e.file_path.as_deref().map(str::to_string);
        for m in e.methods.iter_mut() {
            rewrite_function(arena, m, file.as_deref(), by_site, diagnostics, contents)?;
        }
    }
    Ok(())
}
