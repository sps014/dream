//! Standard-library prelude merging. Each built-in type lives in its own embedded prelude file
//! (`dream_stdlib`); their declarations are parsed with the user's arena and merged into the
//! program so the built-in types are real, extensible classes.

use bumpalo::Bump;
use indexmap::IndexSet;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Error;
use std::rc::Rc;

use crate::driver::source_loader::collect_declarations;
use dream_diagnostics::DiagnosticBag;
use dream_stdlib::{StdPackage, resolve_packages_to_load};
use dream_syntax::lexer::Lexer;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::{
    EnumDeclarationNode, ExtendNode, FunctionNode, GlobalVariableNode, InterfaceDeclarationNode,
};
use dream_syntax::parser::Parser;

/// Declarations parsed once per embedded package. Nodes live in a process-lifetime arena; each
/// build clones the vectors (the clones still point at that arena) instead of lexing again.
#[derive(Clone)]
struct CachedPackage {
    functions: Vec<FunctionNode<'static>>,
    structs: Vec<StructDeclarationNode<'static>>,
    interfaces: Vec<InterfaceDeclarationNode<'static>>,
    enums: Vec<EnumDeclarationNode<'static>>,
    extends: Vec<ExtendNode<'static>>,
    globals: Vec<GlobalVariableNode<'static>>,
    file_contents: Vec<(String, String)>,
    file_modules: Vec<(String, Rc<str>)>,
}

/// Parses the requested embedded stdlib packages (bootstrap + `requested` + transitive deps) and
/// merges their declarations into the program. Uses the same arena as the user's files so all AST
/// nodes share a lifetime.
#[allow(clippy::too_many_arguments)]
pub fn merge_prelude<'a>(
    arena: &'a Bump,
    all_functions: &mut Vec<dream_syntax::nodes::FunctionNode<'a>>,
    all_structs: &mut Vec<dream_syntax::nodes::struct_node::StructDeclarationNode<'a>>,
    all_interfaces: &mut Vec<dream_syntax::nodes::InterfaceDeclarationNode<'a>>,
    all_enums: &mut Vec<dream_syntax::nodes::EnumDeclarationNode<'a>>,
    all_extends: &mut Vec<dream_syntax::nodes::ExtendNode<'a>>,
    all_globals: &mut Vec<dream_syntax::nodes::GlobalVariableNode<'a>>,
    diagnostics: &mut DiagnosticBag,
    file_contents: &mut HashMap<String, String>,
    file_modules: &mut HashMap<String, Rc<str>>,
    requested_packages: &IndexSet<String>,
) -> Result<(), Error> {
    let packages = resolve_packages_to_load(requested_packages);
    for pkg in packages {
        merge_package(
            pkg,
            arena,
            all_functions,
            all_structs,
            all_interfaces,
            all_enums,
            all_extends,
            all_globals,
            diagnostics,
            file_contents,
            file_modules,
        )?;
    }
    Ok(())
}

/// Merges every stdlib package (full surface). Used by host scans and callers that need the
/// complete prelude regardless of user imports.
#[allow(clippy::too_many_arguments)]
pub fn merge_full_prelude<'a>(
    arena: &'a Bump,
    all_functions: &mut Vec<dream_syntax::nodes::FunctionNode<'a>>,
    all_structs: &mut Vec<dream_syntax::nodes::struct_node::StructDeclarationNode<'a>>,
    all_interfaces: &mut Vec<dream_syntax::nodes::InterfaceDeclarationNode<'a>>,
    all_enums: &mut Vec<dream_syntax::nodes::EnumDeclarationNode<'a>>,
    all_extends: &mut Vec<dream_syntax::nodes::ExtendNode<'a>>,
    all_globals: &mut Vec<dream_syntax::nodes::GlobalVariableNode<'a>>,
    diagnostics: &mut DiagnosticBag,
    file_contents: &mut HashMap<String, String>,
    file_modules: &mut HashMap<String, Rc<str>>,
) -> Result<(), Error> {
    let all: IndexSet<String> = dream_stdlib::STD_PACKAGES
        .iter()
        .map(|p| p.name.to_string())
        .collect();
    merge_prelude(
        arena,
        all_functions,
        all_structs,
        all_interfaces,
        all_enums,
        all_extends,
        all_globals,
        diagnostics,
        file_contents,
        file_modules,
        &all,
    )
}

#[allow(clippy::too_many_arguments)]
fn merge_package<'a>(
    pkg: &StdPackage,
    _arena: &'a Bump,
    all_functions: &mut Vec<FunctionNode<'a>>,
    all_structs: &mut Vec<StructDeclarationNode<'a>>,
    all_interfaces: &mut Vec<InterfaceDeclarationNode<'a>>,
    all_enums: &mut Vec<EnumDeclarationNode<'a>>,
    all_extends: &mut Vec<ExtendNode<'a>>,
    all_globals: &mut Vec<GlobalVariableNode<'a>>,
    _diagnostics: &mut DiagnosticBag,
    file_contents: &mut HashMap<String, String>,
    file_modules: &mut HashMap<String, Rc<str>>,
) -> Result<(), Error> {
    let cached = cached_package(pkg)?;
    all_functions.extend(cached.functions.iter().cloned());
    all_structs.extend(cached.structs.iter().cloned());
    all_interfaces.extend(cached.interfaces.iter().cloned());
    all_enums.extend(cached.enums.iter().cloned());
    all_extends.extend(cached.extends.iter().cloned());
    all_globals.extend(cached.globals.iter().cloned());
    for (path, source) in &cached.file_contents {
        file_contents.insert(path.clone(), source.clone());
    }
    for (path, module) in &cached.file_modules {
        file_modules.insert(path.clone(), Rc::clone(module));
    }
    Ok(())
}

struct PreludeCache {
    arena: &'static Bump,
    packages: HashMap<String, CachedPackage>,
}

thread_local! {
    /// One parsed copy of each embedded package per thread. The arena is leaked for the thread's
    /// lifetime so the declaration nodes stay valid; the stdlib sources cannot change in-process.
    static PRELUDE: RefCell<PreludeCache> = RefCell::new(PreludeCache {
        arena: Box::leak(Box::new(Bump::new())),
        packages: HashMap::new(),
    });
}

/// Identity of the embedded sources. The strings are `'static`, so the pointer and length change
/// only when the compiler binary does.
fn package_digest(pkg: &StdPackage) -> String {
    let mut key = String::from(pkg.name);
    for &(path, source) in pkg.files {
        key.push('|');
        key.push_str(path);
        key.push(':');
        key.push_str(&source.len().to_string());
        key.push('@');
        key.push_str(&format!("{:x}", source.as_ptr() as usize));
    }
    key
}

fn cached_package(pkg: &StdPackage) -> Result<CachedPackage, Error> {
    let key = package_digest(pkg);
    PRELUDE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(hit) = cache.packages.get(&key) {
            return Ok(hit.clone());
        }
        let parsed = parse_package(cache.arena, pkg)?;
        cache.packages.insert(key, parsed.clone());
        Ok(parsed)
    })
}

fn parse_package(arena: &'static Bump, pkg: &StdPackage) -> Result<CachedPackage, Error> {
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut interfaces = Vec::new();
    let mut enums = Vec::new();
    let mut extends = Vec::new();
    let mut globals = Vec::new();
    let mut file_contents = Vec::new();
    let mut file_modules = Vec::new();
    for &(prelude_name, prelude_src) in pkg.files {
        let prelude_name = prelude_name.to_string();
        file_contents.push((prelude_name.clone(), prelude_src.to_string()));
        let mut prelude_diagnostics = DiagnosticBag::new(Some(prelude_name.clone()));
        let lexer = Lexer::new(prelude_src.to_string());
        let mut parser = Parser::new(lexer, arena, &mut prelude_diagnostics);
        let ast = parser.parse()?;
        if prelude_diagnostics.has_errors() {
            let messages: Vec<&str> = prelude_diagnostics
                .errors()
                .map(|d| d.message.as_str())
                .collect();
            return Err(Error::other(format!(
                "embedded package '{}' failed to parse: {}",
                pkg.name,
                messages.join("; ")
            )));
        }
        let program = ast.get_root();
        if let Some(module_decl) = &program.module {
            file_modules.push((
                prelude_name.clone(),
                Rc::from(module_decl.path.text.as_str()),
            ));
        }
        collect_declarations(
            program,
            &prelude_name,
            &mut functions,
            &mut structs,
            &mut interfaces,
            &mut enums,
            &mut extends,
            &mut globals,
        );
    }
    Ok(CachedPackage {
        functions,
        structs,
        interfaces,
        enums,
        extends,
        globals,
        file_contents,
        file_modules,
    })
}
