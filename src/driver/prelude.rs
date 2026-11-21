//! Standard-library prelude merging. Each built-in type lives in its own embedded prelude file
//! (`dream_stdlib`); their declarations are parsed with the user's arena and merged into the
//! program so the built-in types are real, extensible classes.

use bumpalo_herd::Herd;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::Error;
use std::sync::{Arc, LazyLock, Mutex};

use crate::driver::source_loader::{ProgramAccumulator, collect_declarations};
use dream_diagnostics::DiagnosticBag;
use dream_stdlib::{StdPackage, resolve_packages_to_load};
use dream_syntax::conditional::CfgSymbols;
use dream_syntax::lexer::Lexer;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::{
    EnumDeclarationNode, ExtendNode, FunctionNode, GlobalVariableNode, InterfaceDeclarationNode,
    ProgramNode,
};
use dream_syntax::parser::Parser;

/// Declarations parsed once per embedded package. Nodes live in the process-lifetime herd; each
/// build clones the vectors (the clones still point at that herd) instead of lexing again.
#[derive(Default)]
struct CachedPackage {
    functions: Vec<FunctionNode<'static>>,
    structs: Vec<StructDeclarationNode<'static>>,
    interfaces: Vec<InterfaceDeclarationNode<'static>>,
    enums: Vec<EnumDeclarationNode<'static>>,
    extends: Vec<ExtendNode<'static>>,
    globals: Vec<GlobalVariableNode<'static>>,
    file_contents: Vec<(String, String)>,
    file_modules: Vec<(String, Arc<str>)>,
}

/// The embedded sources cannot change in-process, so their nodes are never freed.
static HERD: LazyLock<Herd> = LazyLock::new(Herd::new);
static PACKAGES: LazyLock<Mutex<HashMap<String, Arc<CachedPackage>>>> =
    LazyLock::new(Default::default);

/// Parses the requested embedded stdlib packages (bootstrap + `requested` + transitive deps) and
/// merges their declarations into the program.
pub fn merge_prelude(acc: &mut ProgramAccumulator<'_>) -> Result<(), Error> {
    let packages = resolve_packages_to_load(&acc.requested_std_packages);
    for cached in cached_packages(&packages, acc.cfg)? {
        acc.all_functions.extend(cached.functions.iter().cloned());
        acc.all_structs.extend(cached.structs.iter().cloned());
        acc.all_interfaces.extend(cached.interfaces.iter().cloned());
        acc.all_enums.extend(cached.enums.iter().cloned());
        acc.all_extends.extend(cached.extends.iter().cloned());
        acc.all_globals.extend(cached.globals.iter().cloned());
        for (path, source) in &cached.file_contents {
            acc.file_contents.insert(path.clone(), source.clone());
        }
        for (path, module) in &cached.file_modules {
            acc.file_modules.insert(path.clone(), Arc::clone(module));
        }
    }
    Ok(())
}

/// Identity of the embedded sources. The strings are `'static`, so the pointer and length change
/// only when the compiler binary does.
fn package_digest(pkg: &StdPackage, cfg: CfgSymbols) -> String {
    let mut key = format!("{}|{}", pkg.name, cfg.key());
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

/// The parsed packages in `packages` order. Files of packages not parsed yet are parsed in
/// parallel; the cache lock is not held meanwhile, so a concurrent build may parse the same
/// package and either identical copy is kept.
fn cached_packages(
    packages: &[&'static StdPackage],
    cfg: CfgSymbols,
) -> Result<Vec<Arc<CachedPackage>>, Error> {
    let keys: Vec<String> = packages
        .iter()
        .map(|pkg| package_digest(pkg, cfg))
        .collect();
    let missing: Vec<usize> = {
        let cache = PACKAGES.lock().unwrap_or_else(|e| e.into_inner());
        (0..packages.len())
            .filter(|&i| !cache.contains_key(&keys[i]))
            .collect()
    };
    if !missing.is_empty() {
        let files: Vec<(usize, &'static str, &'static str)> = missing
            .iter()
            .flat_map(|&i| packages[i].files.iter().map(move |&(path, src)| (i, path, src)))
            .collect();
        let parsed: Vec<Result<ProgramNode<'static>, Error>> = files
            .par_iter()
            .map_init(
                || HERD.get(),
                |member, &(i, path, source)| parse_file(member, packages[i], path, source, cfg),
            )
            .collect();
        let mut parsed = files.iter().zip(parsed).peekable();
        let mut built = Vec::with_capacity(missing.len());
        for &i in &missing {
            let mut package = CachedPackage::default();
            while let Some(((_, path, source), program)) = parsed.next_if(|((p, ..), _)| *p == i) {
                package.add(path, source, &program?);
            }
            built.push((keys[i].clone(), Arc::new(package)));
        }
        let mut cache = PACKAGES.lock().unwrap_or_else(|e| e.into_inner());
        for (key, package) in built {
            cache.entry(key).or_insert(package);
        }
    }
    let cache = PACKAGES.lock().unwrap_or_else(|e| e.into_inner());
    Ok(keys.iter().map(|key| Arc::clone(&cache[key])).collect())
}

fn parse_file(
    member: &bumpalo_herd::Member<'static>,
    pkg: &StdPackage,
    path: &str,
    source: &str,
    cfg: CfgSymbols,
) -> Result<ProgramNode<'static>, Error> {
    let mut diagnostics = DiagnosticBag::new(Some(path.to_string()));
    let lexer = Lexer::new(source.to_string());
    let ast = Parser::new(lexer, member, &mut diagnostics)
        .with_cfg(cfg)
        .parse();
    if diagnostics.has_errors() {
        let messages: Vec<&str> = diagnostics.errors().map(|d| d.message.as_str()).collect();
        return Err(Error::other(format!(
            "embedded package '{}' failed to parse: {}",
            pkg.name,
            messages.join("; ")
        )));
    }
    Ok(ast.get_root().clone())
}

impl CachedPackage {
    fn add(&mut self, path: &str, source: &str, program: &ProgramNode<'static>) {
        self.file_contents.push((path.to_string(), source.to_string()));
        if let Some(module_decl) = &program.module {
            self.file_modules
                .push((path.to_string(), Arc::from(module_decl.path.text.as_str())));
        }
        collect_declarations(
            program,
            path,
            &mut self.functions,
            &mut self.structs,
            &mut self.interfaces,
            &mut self.enums,
            &mut self.extends,
            &mut self.globals,
        );
    }
}
