//! The embedded generator packages (`system.json.derive`, `system.gpu.shaders`, …). Each is
//! parsed at most once per process, and only once a loaded package registers it: a program that
//! loads none of their owners never parses them.

use super::attribute_refs::Scope;
use super::registry::GeneratorDecl;
use crate::driver::source_loader::ProgramAccumulator;
use indexmap::IndexMap;
use std::sync::OnceLock;

pub(super) struct StdGeneratorFile {
    pub file: &'static str,
    pub module: String,
    pub decls: Vec<GeneratorDecl>,
    pub scope: Scope,
}

pub(super) struct StdGeneratorPackage {
    pub name: &'static str,
    pub files: Vec<StdGeneratorFile>,
}

fn parse(package: &'static dream_stdlib::StdPackage) -> StdGeneratorPackage {
    let files = package
        .files
        .iter()
        .filter_map(|&(file, source)| {
            let arena = bumpalo::Bump::new();
            let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
            let lexer = dream_syntax::lexer::Lexer::new(source.to_string());
            let mut parser = dream_syntax::parser::Parser::new(lexer, &arena, &mut diagnostics);
            let ast = parser.parse().ok()?;
            let program = ast.get_root();
            dream_stdlib::attributes::resolve_source_attribute_ids(program);
            Some(StdGeneratorFile {
                file,
                module: program
                    .module
                    .as_ref()
                    .map(|m| m.path.text.clone())
                    .unwrap_or_default(),
                decls: program
                    .functions
                    .iter()
                    .filter_map(super::registry::generator_decl)
                    .collect(),
                scope: Scope::of_program(program),
            })
        })
        .collect();
    StdGeneratorPackage {
        name: package.name,
        files,
    }
}

fn package(name: &str) -> Option<&'static StdGeneratorPackage> {
    static PACKAGES: OnceLock<IndexMap<&'static str, OnceLock<StdGeneratorPackage>>> =
        OnceLock::new();
    let packages = PACKAGES.get_or_init(|| {
        dream_stdlib::STD_PACKAGES
            .iter()
            .flat_map(|owner| owner.generators.iter().copied())
            .map(|name| (name, OnceLock::new()))
            .collect()
    });
    let slot = packages.get(name)?;
    let package = dream_stdlib::find_package(name)?;
    Some(slot.get_or_init(|| parse(package)))
}

/// The generator packages registered by the packages this program loads, in load order.
pub(super) fn loaded(acc: &ProgramAccumulator<'_>) -> Vec<&'static StdGeneratorPackage> {
    let mut out: Vec<&'static StdGeneratorPackage> = Vec::new();
    for owner in dream_stdlib::resolve_packages_to_load(&acc.requested_std_packages) {
        for &name in owner.generators {
            if out.iter().any(|p| p.name == name) {
                continue;
            }
            if let Some(package) = package(name) {
                out.push(package);
            }
        }
    }
    out
}
