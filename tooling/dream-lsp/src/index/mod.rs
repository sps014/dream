//! A span-indexed symbol model built by walking the parsed document. The compiler's analyzer
//! keys symbol tables by scope and never records an offset->symbol mapping, so navigation
//! features (hover, go-to-definition, find-references, completion) are served from this
//! lightweight index instead. It is best-effort and tolerant of partially-broken trees.
//!
//! The model is split across three submodules: [`model`] holds the plain data records,
//! [`builder`] walks the AST to populate them, and [`queries`] answers editor requests.

use bumpalo::Bump;
use dream::diagnostics::DiagnosticBag;
use dream::syntax::lexer::Lexer;
use dream::syntax::parser::Parser;
use dream_abi::target::{ExecutionEnvironment, TargetSpec};
use std::collections::HashMap;

mod attr_ide;
mod builder;
mod declared_attrs;
mod model;
mod queries;

pub use attr_ide::{
    AttrArgContext, attribute_arg_context, attribute_name_partial, attribute_signature,
};
pub use declared_attrs::{DeclaredAttribute, std_attributes};
pub use model::*;
pub(crate) use queries::import_path_partial;
pub use queries::is_member_completion_context;
pub use queries::is_switch_arm_completion_context;

use builder::Builder;

/// The complete symbol model for one document. All positions are byte offsets into the source.
#[derive(Debug)]
pub struct Index {
    pub decls: Vec<Decl>,
    pub refs: Vec<Ref>,
    pub inlay_hints: Vec<InlayHintOut>,
    /// `@generator` functions declared in this document, by qualified identity and name span.
    pub generators: Vec<(String, usize, usize)>,
    /// Attribute types this document declares or imports from non-std files.
    pub attributes: Vec<DeclaredAttribute>,
}
impl Index {
    /// Parses `text` and builds the symbol model. Tolerates parse errors by indexing whatever
    /// AST the parser manages to produce.
    pub fn build(file_path: Option<&str>, text: &str) -> Index {
        Self::build_for_target(file_path, text, &TargetSpec::host())
    }

    pub fn build_for_target(file_path: Option<&str>, text: &str, target: &TargetSpec) -> Index {
        let cfg = target.cfg_symbols(false);
        let arena = Bump::new();
        let mut scratch = DiagnosticBag::new(None);
        let lexer = Lexer::new(text.to_string());
        let mut parser = Parser::new(lexer, &arena, &mut scratch).with_cfg(cfg);

        let mut builder = Builder {
            type_ctx: std::cell::RefCell::new(dream_types::TypeCtx::new()),
            decl_types: HashMap::new(),
            inferred_types: HashMap::new(),
            callables: HashMap::new(),
            member_owners: HashMap::new(),
            decls: Vec::new(),
            refs: Vec::new(),
            inlay_hints: Vec::new(),
            next_scope: 0,
            is_main: true,
            current_file: None,
            current_namespace: None,
            file_namespaces: HashMap::new(),
            fn_params: HashMap::new(),
            ctor_params: HashMap::new(),
        };
        let mut generators = Vec::new();
        let mut attributes = Vec::new();
        let ast = parser.parse();
        let program = ast.get_root();
        for func in &program.functions {
            if func.attributes.iter().any(|a| a.name.text == "generator") {
                let span = func.name.position;
                let module = program
                    .module
                    .as_ref()
                    .map(|m| m.path.text.as_str())
                    .unwrap_or_default();
                generators.push((
                    dream_abi::attributes::decl_identity(module, &func.name.text),
                    span.start,
                    span.end,
                ));
            }
        }

        attributes.extend(declared_attrs::collect(
            text,
            &program.structs,
            &program.enums,
            None,
        ));

        if let Some(module) = &program.module {
            builder
                .file_namespaces
                .insert(String::new(), module.path.text.clone());
        }

        // Pass 1: Declare all file-level symbols for the main program
        builder.walk_program_for_imports(program);

        let mut acc = dream::driver::source_loader::ProgramAccumulator::default();
        acc.cfg = cfg;

        // Collect std + file imports before prelude merge so opt-in packages load.
        for import in &program.imports {
            if import.is_js || import.c.is_some() || import.alias.is_some() {
                continue;
            }
            let module_name = import.module_name.text.as_str();
            if let Some(pkg) = dream_stdlib::std_package_from_slash_path(module_name) {
                acc.requested_std_packages.insert(pkg.name.to_string());
            }
        }

        if let Some(path_str) = file_path {
            let parent_dir = std::path::Path::new(path_str)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));

            acc.visited.insert(path_str.to_string());

            for import in &program.imports {
                if import.is_js || import.c.is_some() || import.alias.is_some() {
                    continue;
                }
                let module_name = import.module_name.text.as_str();
                if dream_stdlib::std_package_from_slash_path(module_name).is_some() {
                    continue;
                }
                let import_path =
                    dream::driver::source_loader::resolve_import_path(parent_dir, module_name);

                if let Some(import_path_str) = import_path.to_str()
                    && import_path.exists()
                {
                    let _ = dream::driver::source_loader::parse_file_recursive(
                        &import_path_str.to_string(),
                        &mut acc,
                        &arena,
                        &mut scratch,
                    );
                }
            }
        }

        acc.parsed_files.insert(
            file_path.unwrap_or("main.dream").to_string(),
            program.clone(),
        );
        let _ = dream::driver::js_imports::prepare(
            &arena,
            &mut acc,
            file_path.unwrap_or("main.dream"),
            if target.execution_environment == ExecutionEnvironment::Node {
                "node"
            } else {
                "web"
            },
            false,
            &mut scratch,
        );
        let aliases = acc.aliased_imports.clone();
        for (_, item, alias, _) in aliases {
            if let Some(mut declaration) = acc
                .all_structs
                .iter()
                .find(|decl| decl.name.text == item)
                .cloned()
            {
                declaration.name = alias;
                acc.all_structs.push(declaration);
            }
        }

        let _ = dream::driver::prelude::merge_prelude(&mut acc);

        builder.file_namespaces.extend(
            acc.file_modules
                .iter()
                .map(|(file, namespace)| (file.clone(), namespace.to_string())),
        );
        let combined = dream::syntax::nodes::ProgramNode::new(
            vec![],
            acc.all_structs,
            acc.all_interfaces,
            acc.all_functions,
            acc.all_enums,
            acc.all_extends,
            acc.all_globals,
        );
        for s in &combined.structs {
            let Some(path) = s.file_path.as_deref() else {
                continue;
            };
            if dream_stdlib::is_std_source(path) {
                continue;
            }
            if let Some(source) = acc.file_contents.get(path) {
                attributes.extend(declared_attrs::collect(
                    source,
                    std::slice::from_ref(s),
                    &combined.enums,
                    None,
                ));
            }
        }
        // Pass 1.5: Declare all imported and prelude symbols
        builder.is_main = false;
        builder.walk_program_for_imports(&combined);
        builder.is_main = true;

        // Pass 2: Walk function/method bodies
        builder.walk_program(program);
        Index {
            decls: builder.decls,
            refs: builder.refs,
            inlay_hints: builder.inlay_hints,
            generators,
            attributes,
        }
    }
}
