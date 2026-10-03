use super::*;
use dream_syntax::nodes::{FunctionNode, Visibility};

pub(super) fn prepare_module(module: &mut SourceModule, files: &[SourceFile<'_>]) {
    let mut content = blake3::Hasher::new();
    let mut interface = blake3::Hasher::new();
    hash_text(&mut content, "dream-module-content-v2");
    hash_text(&mut interface, "dream-module-interface-v2");
    hash_count(&mut content, module.files.len());
    for &index in &module.files {
        let file = &files[index];
        hash_text(&mut content, &file.path);
        content.update(&(file.source.len() as u64).to_le_bytes());
        content.update(file.source.as_bytes());
        // Prepared ASTs include generated declarations absent from the original source.
        hash_text(&mut content, &format!("{:?}", file.program));
        if let Some(declaration) = &file.program.module {
            hash_text(&mut interface, "module");
            hash_attributes(&mut interface, &declaration.attributes);
        }
        for global in &file.program.globals {
            if global.visibility != Visibility::Public {
                continue;
            }
            module.globals.push(ModuleValueExport {
                name: global.name.text.clone(),
                file: index,
            });
            hash_text(&mut interface, "global");
            hash_text(&mut interface, &global.name.text);
            interface.update(&[global.is_const as u8]);
            match &global.declared_type {
                Some(ty) => {
                    interface.update(&[1]);
                    hash_type(&mut interface, ty);
                }
                None => {
                    interface.update(&[0]);
                }
            }
        }
        for extension in &file.program.extends {
            // An implementation clause affects conformance even without public methods.
            if extension.implements.is_empty()
                && !extension
                    .methods
                    .iter()
                    .any(|m| m.visibility == Visibility::Public)
            {
                continue;
            }
            module.extensions.push(ModuleValueExport {
                name: extension.target.text.clone(),
                file: index,
            });
            hash_text(&mut interface, "extension");
            hash_text(&mut interface, &extension.target.text);
            hash_generics(
                &mut interface,
                extension.generic_parameters.as_deref(),
                &extension.generic_constraints,
            );
            interface.update(&[extension.is_synthesized as u8]);
            hash_count(&mut interface, extension.implements.len());
            for implemented in &extension.implements {
                hash_type(&mut interface, implemented);
            }
            hash_count(&mut interface, extension.methods.len());
            for method in &extension.methods {
                hash_signature(&mut interface, method);
            }
        }
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
                hash_count(&mut interface, declaration.implements.len());
                for implemented in &declaration.implements {
                    hash_type(&mut interface, implemented);
                }
                interface.update(&[
                    declaration.is_value as u8,
                    declaration.is_static as u8,
                    declaration.is_shared as u8,
                    declaration.is_ref_struct as u8,
                    declaration.is_sealed as u8,
                ]);
                hash_count(&mut interface, declaration.fields.len());
                for field in &declaration.fields {
                    hash_text(&mut interface, &field.name.text);
                    hash_type(&mut interface, &field.field_type);
                    hash_text(&mut interface, &format!("{:?}", field.visibility));
                    hash_attributes(&mut interface, &field.attributes);
                    interface.update(&[field.is_weak as u8, field.is_unowned as u8]);
                }
                hash_count(&mut interface, declaration.methods.len());
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
            hash_count(&mut interface, declaration.parents.len());
            for parent in &declaration.parents {
                hash_type(&mut interface, parent);
            }
            hash_count(&mut interface, declaration.methods.len());
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
            hash_count(&mut interface, declaration.variants.len());
            for variant in &declaration.variants {
                hash_text(&mut interface, &variant.name.text);
                interface.update(&variant.value.to_le_bytes());
                hash_count(&mut interface, variant.fields.len());
                for field in &variant.fields {
                    hash_text(&mut interface, &field.name.text);
                    hash_type(&mut interface, &field.field_type);
                    hash_attributes(&mut interface, &field.attributes);
                    hash_text(&mut interface, &format!("{:?}", field.visibility));
                    interface.update(&[field.is_weak as u8, field.is_unowned as u8]);
                }
            }
            hash_count(&mut interface, declaration.methods.len());
            for method in &declaration.methods {
                hash_signature(&mut interface, method);
            }
        }
    }
    if module
        .files
        .iter()
        .any(|&index| interface_needs_implementation(&files[index].program))
    {
        for &index in &module.files {
            hash_implementation(&mut interface, &files[index]);
        }
    }
    module.content_hash = *content.finalize().as_bytes();
    module.interface_hash = *interface.finalize().as_bytes();
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
    hash_count(hash, function.parameters.len());
    for parameter in &function.parameters {
        hash_text(hash, &parameter.name.text);
        hash_type(hash, &parameter.type_);
        hash.update(&[parameter.is_ref as u8, parameter.is_borrow as u8]);
        hash.update(&[parameter.is_variadic as u8]);
        hash_attributes(hash, &parameter.attributes);
        hash.update(&[parameter.default.is_some() as u8]);
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
    match &function.return_type {
        Some(ty) => hash_type(hash, ty),
        None => hash_type(hash, &dream_syntax::nodes::Type::Void),
    }
    hash.update(&[function.is_async as u8, function.is_static as u8]);
    hash_text(hash, function.operator_symbol.as_deref().unwrap_or(""));
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
    hash_count(hash, parameters.map_or(0, |params| params.len()));
    for parameter in parameters.into_iter().flatten() {
        hash_text(hash, &parameter.text);
    }
    hash_count(hash, constraints.len());
    for constraint in constraints {
        hash_text(hash, &constraint.param.text);
        hash_count(hash, constraint.bounds.len());
        for bound in &constraint.bounds {
            hash_type(hash, bound);
        }
        hash_count(hash, constraint.kinds.len());
        for kind in &constraint.kinds {
            hash_text(hash, &format!("{kind:?}"));
        }
    }
}

fn hash_attributes(hash: &mut blake3::Hasher, attributes: &[dream_syntax::nodes::AttributeNode]) {
    hash_count(hash, attributes.len());
    for attribute in attributes {
        hash_text(hash, &attribute.name.text);
        hash_count(hash, attribute.args.len());
        for argument in &attribute.args {
            hash_text(hash, &argument.display());
        }
    }
}

pub(super) fn hash_count(hash: &mut blake3::Hasher, count: usize) {
    hash.update(&(count as u64).to_le_bytes());
}

fn hash_type(hash: &mut blake3::Hasher, ty: &dream_syntax::nodes::Type) {
    use dream_syntax::nodes::Type;
    match ty {
        Type::Array(inner) => {
            hash_text(hash, "array");
            hash_type(hash, inner);
        }
        Type::Tuple(elements) => {
            hash_text(hash, "tuple");
            hash_count(hash, elements.len());
            for element in elements {
                hash_type(hash, element);
            }
        }
        Type::Struct(name, args) => {
            hash_text(hash, "nominal");
            hash_text(hash, &name.text);
            hash_count(hash, args.as_ref().map_or(0, Vec::len));
            for arg in args.iter().flatten() {
                hash_type(hash, arg);
            }
        }
        Type::Function(params, result) => {
            hash_text(hash, "function-type");
            hash_count(hash, params.len());
            for param in params {
                hash_type(hash, param);
            }
            hash_type(hash, result);
        }
        Type::Generic(name) => {
            hash_text(hash, "generic");
            hash_text(hash, name);
        }
        Type::GenericFunctionItem(name) => {
            hash_text(hash, "generic-function");
            hash_text(hash, name);
        }
        other => hash_text(hash, &other.display_name()),
    }
}

fn method_needs_implementation(method: &FunctionNode<'_>) -> bool {
    method
        .generic_parameters
        .as_ref()
        .is_some_and(|params| !params.is_empty())
        || method.is_default_impl
        || (!method.is_static && method.receiver_mode.is_none())
}

fn interface_needs_implementation(program: &ProgramNode<'_>) -> bool {
    program
        .globals
        .iter()
        .any(|g| g.visibility == Visibility::Public && (g.is_const || g.declared_type.is_none()))
        || program.functions.iter().any(|f| {
            f.visibility == Visibility::Public
                && f.generic_parameters
                    .as_ref()
                    .is_some_and(|params| !params.is_empty())
        })
        || program.structs.iter().any(|s| {
            s.visibility == Visibility::Public
                && (s
                    .generic_parameters
                    .as_ref()
                    .is_some_and(|params| !params.is_empty())
                    || s.methods.iter().any(method_needs_implementation))
        })
        || program.enums.iter().any(|e| {
            e.visibility == Visibility::Public
                && (e
                    .generic_parameters
                    .as_ref()
                    .is_some_and(|params| !params.is_empty())
                    || e.methods.iter().any(method_needs_implementation))
        })
        || program.interfaces.iter().any(|i| {
            i.visibility == Visibility::Public && i.methods.iter().any(|m| m.is_default_impl)
        })
        || program.extends.iter().any(|e| {
            (!e.implements.is_empty()
                || e.methods.iter().any(|m| m.visibility == Visibility::Public))
                && (e
                    .generic_parameters
                    .as_ref()
                    .is_some_and(|params| !params.is_empty())
                    || e.methods.iter().any(method_needs_implementation))
        })
        // Private nominal layouts may occur in public signatures. Without resolved export
        // reachability, retaining their implementation is safer than reusing stale layout facts.
        || program.structs.iter().any(|s| s.visibility != Visibility::Public)
        || program.interfaces.iter().any(|i| i.visibility != Visibility::Public)
        || program.enums.iter().any(|e| e.visibility != Visibility::Public)
}

fn hash_implementation(hash: &mut blake3::Hasher, file: &SourceFile<'_>) {
    hash_text(hash, "implementation-dependent-interface");
    for extension in file.program.extends.iter().filter(|e| e.is_synthesized) {
        hash_text(hash, &format!("{extension:?}"));
    }
    // Until analyzed export summaries exist, hash the complete token stream conservatively:
    // inferred types/receivers and generic/default bodies can depend on private helpers.
    if file.source.is_empty() {
        hash_text(hash, &format!("{:?}", file.program));
    } else {
        let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
        let tokens = dream_syntax::lexer::Lexer::new(file.source.clone()).lex_all(&mut diagnostics);
        hash_count(hash, tokens.len());
        for token in tokens {
            hash_text(hash, &format!("{:?}", token.kind));
            hash_text(hash, &token.text);
        }
    }
}

pub(super) fn hash_text(hash: &mut blake3::Hasher, text: &str) {
    hash.update(&(text.len() as u64).to_le_bytes());
    hash.update(text.as_bytes());
}
