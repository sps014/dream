use dream_syntax::nodes::{
    EnumDeclarationNode, ExtendNode, FunctionNode, GlobalVariableNode, InterfaceDeclarationNode,
    ProgramNode, StructDeclarationNode, Visibility,
};
use dream_types::{DefKind, ModuleId, TypeCtx};
use indexmap::{IndexMap, IndexSet};

pub struct SourceModule {
    pub id: ModuleId,
    pub path: String,
    pub files: Vec<usize>,
    pub imports: Vec<ModuleId>,
    pub exports: Vec<ModuleExport>,
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

impl SourceModule {
    pub fn cache_key(&self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash_text(&mut hash, &self.path);
        hash.update(&self.content_hash);
        for (path, interface) in &self.dependency_interfaces {
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
            for target in edges.get(&file.path).into_iter().flatten() {
                if let Some(&id) = paths.get(target.as_str()) {
                    if id != module.id && !module.imports.contains(&id) {
                        module.imports.push(id);
                    }
                }
            }
        }
        for module in &mut modules {
            let mut content = blake3::Hasher::new();
            let mut interface = blake3::Hasher::new();
            for &index in &module.files {
                let file = &files[index];
                content.update(&(file.source.len() as u64).to_le_bytes());
                content.update(file.source.as_bytes());
                for function in &file.program.functions {
                    if function.visibility == Visibility::Public {
                        module.exports.push(ModuleExport {
                            kind: DefKind::Function,
                            name: function.name.text.clone(),
                            file: index,
                        });
                        hash_signature(&mut interface, function);
                    }
                }
                for declaration in &file.program.structs {
                    if declaration.visibility == Visibility::Public {
                        module.exports.push(ModuleExport {
                            kind: DefKind::Struct,
                            name: declaration.name.text.clone(),
                            file: index,
                        });
                        hash_text(&mut interface, "struct");
                        hash_text(&mut interface, &declaration.name.text);
                        hash_generics(
                            &mut interface,
                            declaration.generic_parameters.as_deref(),
                            &declaration.generic_constraints,
                        );
                        hash_attributes(&mut interface, &declaration.attributes);
                        for implemented in &declaration.implements {
                            hash_text(&mut interface, &implemented.display_name());
                        }
                        interface.update(&[
                            declaration.is_value as u8,
                            declaration.is_static as u8,
                            declaration.is_shared as u8,
                            declaration.is_ref_struct as u8,
                        ]);
                        for field in &declaration.fields {
                            hash_text(&mut interface, &field.name.text);
                            hash_text(&mut interface, &field.field_type.get_type());
                            hash_text(&mut interface, &format!("{:?}", field.visibility));
                            hash_attributes(&mut interface, &field.attributes);
                            interface.update(&[field.is_weak as u8, field.is_unowned as u8]);
                        }
                        for method in &declaration.methods {
                            hash_signature(&mut interface, method);
                        }
                    }
                }
                for declaration in &file.program.interfaces {
                    if declaration.visibility != Visibility::Public {
                        continue;
                    }
                    module.exports.push(ModuleExport {
                        kind: DefKind::Interface,
                        name: declaration.name.text.clone(),
                        file: index,
                    });
                    hash_text(&mut interface, "interface");
                    hash_text(&mut interface, &declaration.name.text);
                    hash_generics(
                        &mut interface,
                        declaration.generic_parameters.as_deref(),
                        &declaration.generic_constraints,
                    );
                    hash_attributes(&mut interface, &declaration.attributes);
                    for parent in &declaration.parents {
                        hash_text(&mut interface, &parent.get_type());
                    }
                    for method in &declaration.methods {
                        hash_signature(&mut interface, method);
                    }
                }
                for declaration in &file.program.enums {
                    if declaration.visibility != Visibility::Public {
                        continue;
                    }
                    module.exports.push(ModuleExport {
                        kind: if declaration.is_data_enum() {
                            DefKind::Union
                        } else {
                            DefKind::Enum
                        },
                        name: declaration.name.text.clone(),
                        file: index,
                    });
                    hash_text(&mut interface, "enum");
                    hash_text(&mut interface, &declaration.name.text);
                    hash_generics(
                        &mut interface,
                        declaration.generic_parameters.as_deref(),
                        &declaration.generic_constraints,
                    );
                    hash_attributes(&mut interface, &declaration.attributes);
                    interface.update(&[
                        declaration.is_enum_struct as u8,
                        declaration.is_sealed as u8,
                    ]);
                    for variant in &declaration.variants {
                        hash_text(&mut interface, &variant.name.text);
                        interface.update(&variant.value.to_le_bytes());
                        for field in &variant.fields {
                            hash_text(&mut interface, &field.name.text);
                            hash_text(&mut interface, &field.field_type.get_type());
                        }
                    }
                    for method in &declaration.methods {
                        hash_signature(&mut interface, method);
                    }
                }
            }
            module.content_hash = *content.finalize().as_bytes();
            module.interface_hash = *interface.finalize().as_bytes();
        }
        let hashes: Vec<_> = modules
            .iter()
            .map(|m| (m.path.clone(), m.interface_hash))
            .collect();
        for module in &mut modules {
            module.dependency_interfaces = module
                .imports
                .iter()
                .map(|&id| hashes[id.0 as usize].clone())
                .collect();
        }
        Self { modules, files }
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

fn hash_signature(hash: &mut blake3::Hasher, function: &FunctionNode<'_>) {
    hash_text(hash, "function");
    hash_text(hash, &function.name.text);
    hash_generics(
        hash,
        function.generic_parameters.as_deref(),
        &function.generic_constraints,
    );
    hash_generics(hash, None, &function.where_constraints);
    hash_attributes(hash, &function.attributes);
    for parameter in &function.parameters {
        hash_text(hash, &parameter.name.text);
        hash_text(hash, &parameter.type_.get_type());
        hash.update(&[parameter.is_ref as u8, parameter.is_borrow as u8]);
        hash.update(&[parameter.is_variadic as u8]);
        hash_attributes(hash, &parameter.attributes);
        if let Some(default) = &parameter.default {
            hash_text(hash, &default.display_name());
            let token = match default {
                dream_syntax::nodes::Type::Integer(token)
                | dream_syntax::nodes::Type::Long(token)
                | dream_syntax::nodes::Type::String(token)
                | dream_syntax::nodes::Type::Char(token)
                | dream_syntax::nodes::Type::Boolean(token)
                | dream_syntax::nodes::Type::Float(token)
                | dream_syntax::nodes::Type::Double(token)
                | dream_syntax::nodes::Type::UInt(token)
                | dream_syntax::nodes::Type::ULong(token)
                | dream_syntax::nodes::Type::Byte(token)
                | dream_syntax::nodes::Type::ISize(token)
                | dream_syntax::nodes::Type::USize(token)
                | dream_syntax::nodes::Type::Struct(token, _) => Some(token),
                _ => None,
            };
            if let Some(token) = token {
                hash_text(hash, &token.text);
            }
        }
    }
    hash_text(
        hash,
        &function
            .return_type
            .as_ref()
            .map(|ty| ty.get_type())
            .unwrap_or_else(|| "void".into()),
    );
    hash.update(&[function.is_async as u8, function.is_static as u8]);
    hash_text(
        hash,
        &format!(
            "{:?}:{:?}:{:?}:{:?}:{:?}",
            function.visibility,
            function.receiver_mode,
            function.accessor,
            function.indexer_kind,
            function.cast_kind
        ),
    );
    hash.update(&[
        function.is_extern as u8,
        function.is_override as u8,
        function.is_default_impl as u8,
    ]);
}

fn hash_generics(
    hash: &mut blake3::Hasher,
    parameters: Option<&[dream_syntax::token::syntax_token::SyntaxToken]>,
    constraints: &[dream_syntax::nodes::GenericConstraint],
) {
    for parameter in parameters.into_iter().flatten() {
        hash_text(hash, &parameter.text);
    }
    for constraint in constraints {
        hash_text(hash, &constraint.param.text);
        for bound in &constraint.bounds {
            hash_text(hash, &bound.display_name());
        }
        for kind in &constraint.kinds {
            hash_text(hash, &format!("{kind:?}"));
        }
    }
}

fn hash_attributes(hash: &mut blake3::Hasher, attributes: &[dream_syntax::nodes::AttributeNode]) {
    for attribute in attributes {
        hash_text(hash, &attribute.name.text);
        for argument in &attribute.args {
            hash_text(hash, &argument.display());
        }
    }
}

fn hash_text(hash: &mut blake3::Hasher, text: &str) {
    hash.update(&(text.len() as u64).to_le_bytes());
    hash.update(text.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use dream_diagnostics::DiagnosticBag;
    use dream_syntax::{lexer::Lexer, parser::Parser};

    fn parse<'a>(arena: &'a Bump, source: &str) -> ProgramNode<'a> {
        let mut diagnostics = DiagnosticBag::new(None);
        let tree = Parser::new(Lexer::new(source.into()), arena, &mut diagnostics)
            .parse()
            .unwrap();
        assert!(!diagnostics.has_errors());
        tree.get_root().clone()
    }

    #[test]
    fn module_scoped_nominals_have_distinct_identities() {
        let arena = Bump::new();
        let graph = ModuleGraph::new(
            ["a", "b"]
                .iter()
                .map(|name| {
                    let source = format!("module {name}; public class User {{}}");
                    (
                        format!("{name}.dream"),
                        source.clone(),
                        parse(&arena, &source),
                    )
                })
                .collect(),
            &IndexMap::new(),
        );
        let mut types = TypeCtx::new();
        graph.configure_types(&mut types);
        let ids: Vec<_> = graph
            .files
            .iter()
            .map(|file| {
                types.set_scope(file.module);
                types.register(dream_types::DefKind::Struct, "User", vec![])
            })
            .collect();
        assert_ne!(ids[0], ids[1]);
        assert_eq!(ids[0].index, ids[1].index);
        for (file, expected) in graph.files.iter().zip(ids) {
            types.set_scope(file.module);
            assert_eq!(
                types.resolve(dream_types::DefKind::Struct, "User"),
                Some(expected)
            );
        }
    }

    #[test]
    fn interface_hash_ignores_bodies_and_source_positions() {
        let arena = Bump::new();
        let sources = [
            "module a; public fun value(): int { return 1; }",
            "\nmodule a;\npublic fun value(): int { return 2; }",
            "module a; public fun value(): long { return 2; }",
        ];
        let graphs: Vec<_> = sources
            .iter()
            .map(|source| {
                ModuleGraph::new(
                    vec![("a.dream".into(), source.to_string(), parse(&arena, source))],
                    &IndexMap::new(),
                )
            })
            .collect();
        assert_ne!(
            graphs[0].modules[1].content_hash,
            graphs[1].modules[1].content_hash
        );
        assert_eq!(
            graphs[0].modules[1].interface_hash,
            graphs[1].modules[1].interface_hash
        );
        assert_ne!(
            graphs[0].modules[1].interface_hash,
            graphs[2].modules[1].interface_hash
        );
    }
}
