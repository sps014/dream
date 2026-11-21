//! Reads and parses every file reachable from the entry before the sequential import walk, one
//! import level at a time and each level in parallel. The walk then takes each file from here
//! instead of parsing it, so file order, diagnostics and recorded inputs are exactly those of a
//! sequential load. A file this pass did not reach is still parsed by the walk itself.

use super::{ProgramAccumulator, find_dream_packages_dir, resolve_import};
use bumpalo_herd::Herd;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::conditional::CfgSymbols;
use dream_syntax::lexer::Lexer;
use dream_syntax::nodes::ProgramNode;
use dream_syntax::parser::{AstArena, Parser};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::Path;

/// One file read and parsed ahead of the walk.
pub(crate) struct Prefetched<'a> {
    pub(super) text: String,
    pub(super) program: ProgramNode<'a>,
    pub(super) diagnostics: DiagnosticBag,
}

/// Parses `entry` (its canonical path and source) and everything it imports into `acc`.
pub(super) fn prefetch<'a>(
    acc: &mut ProgramAccumulator<'a>,
    herd: &'a Herd,
    entry: String,
    text: String,
) {
    let packages_dir = acc.package_base.as_deref().and_then(find_dream_packages_dir);
    let cfg = acc.cfg;
    let mut seen = HashSet::from([entry.clone()]);
    let mut frontier = vec![(entry, Some(text))];
    while !frontier.is_empty() {
        let level: Vec<(String, Option<Prefetched<'a>>, Vec<String>)> = frontier
            .into_par_iter()
            .map_init(
                || herd.get(),
                |member, (path, text)| {
                    let Some(text) = text.or_else(|| std::fs::read_to_string(&path).ok()) else {
                        return (path, None, Vec::new());
                    };
                    let file = parse(AstArena::Member(member), &path, text, cfg);
                    let imports = file_imports(&path, &file.program, packages_dir.as_deref());
                    (path, Some(file), imports)
                },
            )
            .collect();
        frontier = Vec::new();
        for (path, file, imports) in level {
            if let Some(file) = file {
                acc.prefetched.insert(path, file);
            }
            for import in imports {
                if seen.insert(import.clone()) {
                    frontier.push((import, None));
                }
            }
        }
    }
}

pub(super) fn parse<'a>(
    arena: AstArena<'a, '_>,
    path: &str,
    text: String,
    cfg: CfgSymbols,
) -> Prefetched<'a> {
    let mut diagnostics = DiagnosticBag::new(Some(path.to_string()));
    let program = Parser::new(Lexer::new(text.clone()), arena, &mut diagnostics)
        .with_cfg(cfg)
        .with_c_import_scope(crate::driver::c_imports::source_scope(Path::new(path)))
        .parse()
        .get_root()
        .clone();
    Prefetched {
        text,
        program,
        diagnostics,
    }
}

/// The canonical paths of the source files `program` imports, resolved as the walk resolves them.
fn file_imports(path: &str, program: &ProgramNode<'_>, packages_dir: Option<&Path>) -> Vec<String> {
    let parent_dir = Path::new(path).parent().unwrap_or_else(|| Path::new(""));
    program
        .imports
        .iter()
        .filter(|import| import.c.is_none() && !import.is_js && import.alias.is_none())
        .map(|import| import.module_name.text.as_str())
        .filter(|name| dream_stdlib::std_package_from_slash_path(name).is_none())
        .filter_map(|name| dunce::canonicalize(resolve_import(parent_dir, name, packages_dir)).ok())
        .filter_map(|path| path.to_str().map(str::to_owned))
        .collect()
}
