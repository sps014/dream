use dream_syntax::nodes::{
    EnumDeclarationNode, ExtendNode, FunctionNode, GlobalVariableNode, InterfaceDeclarationNode,
    ProgramNode, StructDeclarationNode,
};
use dream_types::{DefKind, ModuleId, TypeCtx};
use indexmap::{IndexMap, IndexSet};

mod hash;
use hash::{hash_count, hash_text};

pub struct SourceModule {
    pub id: ModuleId,
    pub path: String,
    pub files: Vec<usize>,
    pub imports: Vec<ModuleId>,
    pub exports: Vec<ModuleExport>,
    pub globals: Vec<ModuleValueExport>,
    pub extensions: Vec<ModuleValueExport>,
    pub content_hash: [u8; 32],
    pub interface_hash: [u8; 32],
    pub dependency_interfaces: Vec<(String, [u8; 32])>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleExport {
    pub kind: DefKind,
    pub name: String,
    pub file: usize,
}

/// Globals and extensions are exports, but do not allocate nominal DefIds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleValueExport {
    pub name: String,
    pub file: usize,
}

impl SourceModule {
    pub fn cache_key(&self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash_text(&mut hash, "dream-module-cache-v2");
        hash_text(&mut hash, &self.path);
        hash.update(&self.content_hash);
        hash.update(&self.interface_hash);
        let mut dependencies: Vec<_> = self.dependency_interfaces.iter().collect();
        dependencies.sort_by(|a, b| a.0.cmp(&b.0));
        hash_count(&mut hash, dependencies.len());
        for (path, interface) in dependencies {
            hash_text(&mut hash, path);
            hash.update(interface);
        }
        *hash.finalize().as_bytes()
    }
}

pub struct SourceFile<'a> {
    pub path: String,
    pub source: String,
    pub module: ModuleId,
    pub program: ProgramNode<'a>,
}

pub struct ModuleGraph<'a> {
    pub modules: Vec<SourceModule>,
    pub files: Vec<SourceFile<'a>>,
}

/// The analyzer borrows declarations from their owning source files; no merged AST is produced.
pub struct ProgramView<'a> {
    pub structs: Vec<&'a StructDeclarationNode<'a>>,
    pub interfaces: Vec<&'a InterfaceDeclarationNode<'a>>,
    pub functions: Vec<&'a FunctionNode<'a>>,
    pub enums: Vec<&'a EnumDeclarationNode<'a>>,
    pub extends: Vec<&'a ExtendNode<'a>>,
    pub globals: Vec<&'a GlobalVariableNode<'a>>,
}

impl<'a> ModuleGraph<'a> {
    pub fn single(program: ProgramNode<'a>) -> Self {
        Self::new(vec![("".into(), "".into(), program)], &IndexMap::new())
    }

    pub fn new(
        inputs: Vec<(String, String, ProgramNode<'a>)>,
        edges: &IndexMap<String, Vec<String>>,
    ) -> Self {
        let mut names = IndexSet::new();
        names.insert(String::new());
        for (_, _, program) in &inputs {
            names.insert(
                program
                    .module
                    .as_ref()
                    .map(|m| m.path.text.clone())
                    .unwrap_or_default(),
            );
        }
        let ids: IndexMap<_, _> = names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), ModuleId(i as u32)))
            .collect();
        let mut modules: Vec<_> = names
            .into_iter()
            .map(|path| SourceModule {
                id: ids[&path],
                path,
                files: vec![],
                imports: vec![],
                exports: vec![],
                globals: vec![],
                extensions: vec![],
                content_hash: [0; 32],
                interface_hash: [0; 32],
                dependency_interfaces: vec![],
            })
            .collect();
        let files: Vec<_> = inputs
            .into_iter()
            .map(|(path, source, program)| {
                let name = program
                    .module
                    .as_ref()
                    .map(|m| m.path.text.clone())
                    .unwrap_or_default();
                SourceFile {
                    path,
                    source,
                    module: ids[&name],
                    program,
                }
            })
            .collect();
        let paths: IndexMap<_, _> = files.iter().map(|f| (f.path.as_str(), f.module)).collect();
        for (index, file) in files.iter().enumerate() {
            let module = &mut modules[file.module.0 as usize];
            module.files.push(index);
            for import in file
                .program
                .imports
                .iter()
                .filter(|import| import.alias.is_some())
            {
                if let Some((path, _)) = import.module_name.text.rsplit_once('.') {
                    if let Some(&id) = ids.get(path) {
                        if id != module.id && !module.imports.contains(&id) {
                            module.imports.push(id);
                        }
                    }
                }
            }
            for target in edges.get(&file.path).into_iter().flatten() {
                if let Some(&id) = paths.get(target.as_str()) {
                    if id != module.id && !module.imports.contains(&id) {
                        module.imports.push(id);
                    }
                }
            }
        }
        for module in &mut modules {
            hash::prepare_module(module, &files);
        }
        let mut graph = Self { modules, files };
        graph.prepare_dependency_interfaces();
        graph
    }

    /// Include the reachable interface closure: a signature can expose a type from an indirect
    /// dependency. Iterative traversal also terminates for import cycles without recursive hashes.
    pub fn prepare_dependency_interfaces(&mut self) {
        let dependencies: Vec<_> = self
            .modules
            .iter()
            .map(|module| {
                let mut seen = IndexSet::new();
                seen.insert(module.id);
                let mut pending = module.imports.clone();
                let mut interfaces = Vec::new();
                while let Some(id) = pending.pop() {
                    if !seen.insert(id) {
                        continue;
                    }
                    let dependency = &self.modules[id.0 as usize];
                    interfaces.push((dependency.path.clone(), dependency.interface_hash));
                    pending.extend(dependency.imports.iter().copied());
                }
                interfaces.sort_by(|a, b| a.0.cmp(&b.0));
                interfaces
            })
            .collect();
        for (module, interfaces) in self.modules.iter_mut().zip(dependencies) {
            module.dependency_interfaces = interfaces;
        }
    }

    pub fn view(&'a self) -> ProgramView<'a> {
        ProgramView {
            structs: self
                .files
                .iter()
                .flat_map(|f| f.program.structs.iter())
                .collect(),
            interfaces: self
                .files
                .iter()
                .flat_map(|f| f.program.interfaces.iter())
                .collect(),
            functions: self
                .files
                .iter()
                .flat_map(|f| f.program.functions.iter())
                .collect(),
            enums: self
                .files
                .iter()
                .flat_map(|f| f.program.enums.iter())
                .collect(),
            extends: self
                .files
                .iter()
                .flat_map(|f| f.program.extends.iter())
                .collect(),
            globals: self
                .files
                .iter()
                .flat_map(|f| f.program.globals.iter())
                .collect(),
        }
    }

    pub fn configure_types(&self, types: &mut TypeCtx) {
        for module in &self.modules {
            types.define_module(module.id, module.path.clone(), module.imports.clone());
        }
    }

    pub fn module_for_file(&self, path: Option<&str>) -> ModuleId {
        path.and_then(|path| self.files.iter().find(|f| f.path == path))
            .or_else(|| (self.files.len() == 1).then(|| &self.files[0]))
            .map(|f| f.module)
            .unwrap_or(ModuleId::ROOT)
    }
}

#[cfg(test)]
#[path = "tests/module_graph_cache.rs"]
mod tests;
