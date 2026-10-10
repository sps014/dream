//! Generator registration: `@generator fun name(ctx: GenContext)` declarations from user files,
//! from the generator packages of loaded std packages (`system.json` → `system.json.derive`),
//! and from `dream.toml` `[[generators]]` files; plus their resolved triggers.

use super::decls::DeclIndex;
use super::stage::GeneratorStage;
use crate::driver::project_manifest::GeneratorEntry;
use crate::driver::source_loader::ProgramAccumulator;
use bumpalo::Bump;
use dream_abi::attributes::{
    INCREMENTAL_ATTR, ON_ATTRIBUTE_ATTR, ON_CALL_ATTR, SYNTAX_BLOCK_ATTR, UserAttributes,
    decl_identity,
};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{AttributeArg, AttributeNode, FunctionNode, Type};
use dream_text::text_span::TextSpan;
use std::sync::OnceLock;

/// The `system.codegen` parameter type every generator takes.
pub const GEN_CONTEXT_TYPE: &str = "GenContext";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTrigger {
    pub id: String,
    /// Receiver type name for `Type.method`, `None` for a free function.
    pub owner: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RegisteredGenerator {
    pub name: String,
    pub id: String,
    pub module: String,
    /// Absolute path, or the embedded `<std>/...` path.
    pub file: String,
    /// The std package declaring the generator (its harness imports it by name).
    pub std_package: Option<&'static str>,
    pub span: Option<TextSpan>,
    pub attribute_triggers: Vec<String>,
    pub call_triggers: Vec<CallTrigger>,
    pub syntax_block: bool,
    pub incremental: bool,
    pub entry: Option<GeneratorEntry>,
}

impl RegisteredGenerator {
    pub fn is_std(&self) -> bool {
        self.std_package.is_some()
    }
}

/// The parts of a `@generator` declaration registration needs, detached from any arena.
#[derive(Debug, Clone)]
struct GeneratorDecl {
    name: String,
    span: TextSpan,
    attributes: Vec<AttributeNode>,
    takes_context: bool,
    has_body: bool,
}

fn generator_decl(f: &FunctionNode<'_>) -> Option<GeneratorDecl> {
    if !dream_abi::attributes::has_generator_attr(&f.attributes) {
        return None;
    }
    let takes_context = f.parameters.len() == 1
        && matches!(&f.parameters[0].type_, Type::Struct(tok, None) if tok.text == GEN_CONTEXT_TYPE);
    Some(GeneratorDecl {
        name: f.name.text.clone(),
        span: f.name.position,
        attributes: f.attributes.clone(),
        takes_context,
        has_body: !f.body.is_empty(),
    })
}

struct ParsedGeneratorFile {
    module: String,
    decls: Vec<GeneratorDecl>,
}

fn scan_source(path: &str, source: &str, diagnostics: &mut DiagnosticBag) -> ParsedGeneratorFile {
    let arena = Bump::new();
    let mut local = DiagnosticBag::new(Some(path.to_string()));
    let lexer = dream_syntax::lexer::Lexer::new(source.to_string());
    let mut parser = dream_syntax::parser::Parser::new(lexer, &arena, &mut local);
    let parsed = parser.parse();
    diagnostics.extend(&local);
    let Ok(ast) = parsed else {
        return ParsedGeneratorFile {
            module: String::new(),
            decls: Vec::new(),
        };
    };
    let program = ast.get_root();
    ParsedGeneratorFile {
        module: program
            .module
            .as_ref()
            .map(|m| m.path.text.clone())
            .unwrap_or_default(),
        decls: program
            .functions
            .iter()
            .filter_map(generator_decl)
            .collect(),
    }
}

/// `(package, file, module, decl)` for every generator in the embedded generator packages,
/// parsed once per process.
fn std_generator_decls() -> &'static [(&'static str, &'static str, String, GeneratorDecl)] {
    static DECLS: OnceLock<Vec<(&'static str, &'static str, String, GeneratorDecl)>> =
        OnceLock::new();
    DECLS.get_or_init(|| {
        let mut out = Vec::new();
        for pkg in dream_stdlib::STD_PACKAGES {
            for &gen_pkg in pkg.generators {
                let Some(package) = dream_stdlib::find_package(gen_pkg) else {
                    continue;
                };
                for (path, source) in package.files {
                    let mut scratch = DiagnosticBag::new(None);
                    let parsed = scan_source(path, source, &mut scratch);
                    for decl in parsed.decls {
                        out.push((package.name, *path, parsed.module.clone(), decl));
                    }
                }
            }
        }
        out
    })
}

fn path_parts(arg: &AttributeArg) -> Option<Vec<&str>> {
    match arg {
        AttributeArg::Enum(parts) => Some(parts.iter().map(|t| t.text.as_str()).collect()),
        _ => None,
    }
}

pub struct Registration<'r> {
    pub index: &'r DeclIndex,
    pub attributes: &'r UserAttributes,
    pub stage: GeneratorStage,
}

impl Registration<'_> {
    fn register(
        &self,
        decl: &GeneratorDecl,
        file: &str,
        module: &str,
        std_package: Option<&'static str>,
        entry: Option<&GeneratorEntry>,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<RegisteredGenerator> {
        diagnostics.file_path = Some(file.to_string());
        if !decl.takes_context || !decl.has_body {
            diagnostics.report_error(
                format!(
                    "generator '{}': @generator must be 'fun {}(ctx: GenContext)' with a non-empty body",
                    decl.name, decl.name
                ),
                Some(decl.span),
            );
            return None;
        }
        let mut registered = RegisteredGenerator {
            name: decl.name.clone(),
            id: decl_identity(module, &decl.name),
            module: module.to_string(),
            file: file.to_string(),
            std_package,
            span: Some(decl.span),
            attribute_triggers: Vec::new(),
            call_triggers: Vec::new(),
            syntax_block: false,
            incremental: false,
            entry: entry.cloned(),
        };
        for attr in &decl.attributes {
            match attr.name.text.as_str() {
                SYNTAX_BLOCK_ATTR => registered.syntax_block = true,
                INCREMENTAL_ATTR => registered.incremental = true,
                ON_ATTRIBUTE_ATTR => {
                    for arg in &attr.args {
                        let Some(parts) = path_parts(arg) else {
                            continue;
                        };
                        let resolved = parts
                            .last()
                            .and_then(|name| self.attributes.get(name))
                            .filter(|a| {
                                parts.len() == 1 || a.module == parts[..parts.len() - 1].join(".")
                            });
                        match resolved {
                            Some(a) => {
                                if !registered.attribute_triggers.contains(&a.id) {
                                    registered.attribute_triggers.push(a.id.clone());
                                }
                            }
                            // An attribute from a package the program never loads cannot be
                            // used, so the trigger can never fire; only user generators that
                            // name a missing attribute are an error.
                            None if std_package.is_some() => {}
                            None => diagnostics.report_error(
                                format!(
                                    "generator '{}': '{}' is not a declared @attribute type",
                                    decl.name,
                                    arg.display()
                                ),
                                Some(arg.position()),
                            ),
                        }
                    }
                }
                ON_CALL_ATTR => {
                    for arg in &attr.args {
                        let Some(parts) = path_parts(arg) else {
                            continue;
                        };
                        match self.index.resolve_path(&parts) {
                            Some(id) => {
                                let (owner, name) = match parts.as_slice() {
                                    [.., owner, name] if id.ends_with(&format!(".{name}")) => {
                                        (Some(owner.to_string()), name.to_string())
                                    }
                                    [.., name] => (None, name.to_string()),
                                    [] => continue,
                                };
                                let trigger = CallTrigger { id, owner, name };
                                if !registered.call_triggers.contains(&trigger) {
                                    registered.call_triggers.push(trigger);
                                }
                            }
                            None if std_package.is_some() => {}
                            None => diagnostics.report_error(
                                format!(
                                    "generator '{}': '{}' does not name a function or method",
                                    decl.name,
                                    arg.display()
                                ),
                                Some(arg.position()),
                            ),
                        }
                    }
                }
                _ => {}
            }
        }
        Some(registered)
    }

    /// Every generator this compile may run, sorted by name for a deterministic merge order.
    pub fn discover(
        &self,
        acc: &ProgramAccumulator<'_>,
        manifest: &[(String, GeneratorEntry)],
        diagnostics: &mut DiagnosticBag,
    ) -> Vec<RegisteredGenerator> {
        let saved_file = diagnostics.file_path.clone();
        let out = self.discover_all(acc, manifest, diagnostics);
        diagnostics.file_path = saved_file;
        out
    }

    fn discover_all(
        &self,
        acc: &ProgramAccumulator<'_>,
        manifest: &[(String, GeneratorEntry)],
        diagnostics: &mut DiagnosticBag,
    ) -> Vec<RegisteredGenerator> {
        let mut out: Vec<RegisteredGenerator> = Vec::new();
        if self.stage == GeneratorStage::None {
            return out;
        }
        let entry_for = |file: &str| {
            manifest
                .iter()
                .find(|(path, _)| path == file)
                .map(|(_, e)| e)
        };
        if self.stage == GeneratorStage::All {
            for f in &acc.all_functions {
                let Some(file) = f.file_path.as_deref() else {
                    continue;
                };
                if dream_stdlib::is_std_source(file) {
                    continue;
                }
                if let Some(decl) = generator_decl(f) {
                    let module = super::decls::module_of(acc, Some(file));
                    out.extend(self.register(
                        &decl,
                        file,
                        &module,
                        None,
                        entry_for(file),
                        diagnostics,
                    ));
                }
            }
            for (path, entry) in manifest {
                if acc.file_contents.contains_key(path) {
                    continue;
                }
                match std::fs::read_to_string(path) {
                    Ok(source) => {
                        let parsed = scan_source(path, &source, diagnostics);
                        for decl in &parsed.decls {
                            out.extend(self.register(
                                decl,
                                path,
                                &parsed.module,
                                None,
                                Some(entry),
                                diagnostics,
                            ));
                        }
                    }
                    Err(e) => diagnostics.report_error(
                        format!("cannot read generator file '{path}' (from dream.toml): {e}"),
                        None,
                    ),
                }
            }
        }
        let loaded = dream_stdlib::resolve_packages_to_load(&acc.requested_std_packages);
        for (package, file, module, decl) in std_generator_decls() {
            let owner_loaded = loaded.iter().any(|p| p.generators.contains(package));
            if owner_loaded {
                out.extend(self.register(decl, file, module, Some(package), None, diagnostics));
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name).then(a.file.cmp(&b.file)));
        for pair in out.windows(2) {
            if pair[0].name == pair[1].name {
                diagnostics.file_path = Some(pair[1].file.clone());
                diagnostics.report_error(
                    format!(
                        "generator '{}' is declared twice ('{}' and '{}'); generator names must be unique",
                        pair[0].name, pair[0].file, pair[1].file
                    ),
                    pair[1].span,
                );
            }
        }
        out
    }
}
