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
    /// `@generator` functions declared in this document, by name and name span.
    pub generators: Vec<(String, usize, usize)>,
    /// Attribute types this document declares or imports from non-std files.
    pub attributes: Vec<DeclaredAttribute>,
}
impl Index {
    /// Parses `text` and builds the symbol model. Tolerates parse errors by indexing whatever
    /// AST the parser manages to produce.
    pub fn build(file_path: Option<&str>, text: &str) -> Index {
        let arena = Bump::new();
        let mut scratch = DiagnosticBag::new(None);
        let lexer = Lexer::new(text.to_string());
        let mut parser = Parser::new(lexer, &arena, &mut scratch);

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
            fn_params: HashMap::new(),
            ctor_params: HashMap::new(),
        };
        let mut generators = Vec::new();
        let mut attributes = Vec::new();
        if let Ok(ast) = parser.parse() {
            let program = ast.get_root();
            for func in &program.functions {
                if dream_abi::attributes::has_generator_attr(&func.attributes) {
                    let span = func.name.position;
                    generators.push((func.name.text.clone(), span.start, span.end));
                }
            }

            attributes.extend(declared_attrs::collect(text, &program.structs, None));

            // Pass 1: Declare all file-level symbols for the main program
            builder.walk_program_for_imports(program);

            let mut acc = dream::driver::source_loader::ProgramAccumulator::default();

            // Collect std + file imports before prelude merge so opt-in packages load.
            for import in &program.imports {
                if import.alias.is_some() {
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
                    if import.alias.is_some() {
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

            let _ = dream::driver::prelude::merge_prelude(
                &arena,
                &mut acc.all_functions,
                &mut acc.all_structs,
                &mut acc.all_interfaces,
                &mut acc.all_enums,
                &mut acc.all_extends,
                &mut acc.all_globals,
                &mut scratch,
                &mut acc.file_contents,
                &mut acc.file_modules,
                &acc.requested_std_packages,
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
        }
        Index {
            decls: builder.decls,
            refs: builder.refs,
            inlay_hints: builder.inlay_hints,
            generators,
            attributes,
        }
    }
}
