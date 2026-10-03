//! Generic shape/placement validation for every attribute-bearing declaration.

use super::*;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::interface_node::InterfaceDeclarationNode;
use dream_syntax::nodes::program::{EnumDeclarationNode, ExtendNode};
use dream_syntax::nodes::struct_node::{StructDeclarationNode, StructFieldNode};
use dream_syntax::nodes::types::is_special_member_name;
use std::collections::BTreeMap;
use std::rc::Rc;

fn arg_matches_kind(arg: &AttributeArg, kind: ArgKind) -> bool {
    match (kind, arg) {
        (ArgKind::String, AttributeArg::String(_)) => true,
        (ArgKind::Int, AttributeArg::Int(_)) => true,
        (ArgKind::Float, AttributeArg::Float(_)) => true,
        // Allow unsuffixed float literal when a double param is expected (same as expression
        // expected-type retargeting for numeric literals).
        (ArgKind::Double, AttributeArg::Double(_) | AttributeArg::Float(_)) => true,
        (ArgKind::Bool, AttributeArg::Bool(_)) => true,
        (ArgKind::Enum, AttributeArg::Enum(_)) => true,
        _ => false,
    }
}

fn kind_name(kind: ArgKind) -> &'static str {
    match kind {
        ArgKind::String => "a string literal",
        ArgKind::Int => "an integer literal",
        ArgKind::Float => "a float literal",
        ArgKind::Double => "a double literal",
        ArgKind::Bool => "a boolean literal",
        ArgKind::Enum => "an enum member path",
    }
}

fn type_to_arg_kind(ty: &Type) -> Option<ArgKind> {
    match ty {
        Type::String(_) => Some(ArgKind::String),
        Type::Integer(_) | Type::Byte(_) | Type::Long(_) | Type::UInt(_) | Type::ULong(_) => {
            Some(ArgKind::Int)
        }
        Type::Float(_) => Some(ArgKind::Float),
        Type::Double(_) => Some(ArgKind::Double),
        Type::Boolean(_) => Some(ArgKind::Bool),
        // Bare named types in params are treated as enum (e.g. `HttpMethod`).
        Type::Struct(_, None) | Type::Generic(_) => Some(ArgKind::Enum),
        _ => None,
    }
}

/// Top-level functions marked `@attribute`: name (exact casing) → parameter kinds.
fn collect_user_attributes(
    functions: &[FunctionNode<'_>],
    diagnostics: &mut DiagnosticBag,
) -> BTreeMap<String, Vec<ArgKind>> {
    let mut out = BTreeMap::new();
    for f in functions {
        if !f.attributes.iter().any(|a| a.name.text == "attribute") {
            continue;
        }
        diagnostics.file_path = file_path_string(&f.file_path);
        let mut kinds = Vec::new();
        let mut ok = true;
        for p in &f.parameters {
            match type_to_arg_kind(&p.type_) {
                Some(k) => kinds.push(k),
                None => {
                    diagnostics.report_error(
                        format!(
                            "attribute function '{}': parameter '{}' has a type that cannot be used as an attribute argument",
                            f.name.text, p.name.text
                        ),
                        Some(p.name.position),
                    );
                    ok = false;
                }
            }
        }
        if ok {
            if out.contains_key(&f.name.text) {
                diagnostics.report_error(
                    format!("duplicate attribute function '{}'", f.name.text),
                    Some(f.name.position),
                );
            } else {
                out.insert(f.name.text.clone(), kinds);
            }
        }
    }
    out
}

fn validate_arg_list(
    attr_name: &str,
    attr: &AttributeNode,
    kinds: &[ArgKind],
    min: usize,
    max: usize,
    diagnostics: &mut DiagnosticBag,
) {
    if attr.args.len() < min || attr.args.len() > max {
        let expected = if min == max {
            format!("{}", min)
        } else {
            format!("{}-{}", min, max)
        };
        diagnostics.report_error(
            format!(
                "'@{}' expects {} argument(s), got {}",
                attr_name,
                expected,
                attr.args.len()
            ),
            Some(attr.name.position),
        );
    }
    if kinds.is_empty() {
        return;
    }
    for (i, arg) in attr.args.iter().enumerate() {
        let kind = kinds[i.min(kinds.len() - 1)];
        if !arg_matches_kind(arg, kind) {
            diagnostics.report_error(
                format!(
                    "'@{}' argument {} must be {}, got '{}'",
                    attr_name,
                    i + 1,
                    kind_name(kind),
                    arg.display()
                ),
                Some(arg.position()),
            );
        }
    }
}

/// Validates one declaration's attribute list against `target`: every attribute must be a known
/// builtin name or a user `@attribute` function, allowed on `target`, carry the right argument shape,
/// and (unless `repeatable`) appear at most once.
pub fn validate_attributes(
    attrs: &[AttributeNode],
    target: AttributeTarget,
    diagnostics: &mut DiagnosticBag,
) {
    validate_attributes_with(attrs, target, &BTreeMap::new(), diagnostics);
}

fn validate_attributes_with(
    attrs: &[AttributeNode],
    target: AttributeTarget,
    user_attrs: &BTreeMap<String, Vec<ArgKind>>,
    diagnostics: &mut DiagnosticBag,
) {
    let mut seen: Vec<&str> = Vec::new();
    for attr in attrs {
        let name = attr.name.text.as_str();
        if let Some(spec) = find_spec(name) {
            if !spec.targets.contains(&target) {
                diagnostics.report_error(
                    format!("'@{}' cannot be applied to {}", name, target.display_name()),
                    Some(attr.name.position),
                );
            }
            match spec.args {
                ArgShape::None => {
                    if !attr.args.is_empty() {
                        diagnostics.report_error(
                            format!("'@{}' does not take any arguments", name),
                            Some(attr.name.position),
                        );
                    }
                }
                ArgShape::Args { kinds, min, max } => {
                    validate_arg_list(name, attr, kinds, min, max, diagnostics);
                }
            }
            if !spec.repeatable && seen.contains(&name) {
                diagnostics.report_error(
                    format!("duplicate '@{}' attribute", name),
                    Some(attr.name.position),
                );
            }
            if (name == "inline" && seen.contains(&"noinline"))
                || (name == "noinline" && seen.contains(&"inline"))
            {
                diagnostics.report_error(
                    "'@inline' and '@noinline' cannot be combined".to_string(),
                    Some(attr.name.position),
                );
            }
            seen.push(name);
            continue;
        }

        if let Some(kinds) = user_attrs.get(name) {
            let n = kinds.len();
            validate_arg_list(name, attr, kinds, n, n, diagnostics);
            if seen.contains(&name) {
                diagnostics.report_error(
                    format!("duplicate '@{}' attribute", name),
                    Some(attr.name.position),
                );
            }
            seen.push(name);
            continue;
        }

        diagnostics.report_error(
            format!("unknown attribute '@{}'", name),
            Some(attr.name.position),
        );
    }
}

fn file_path_string(file_path: &Option<Rc<str>>) -> Option<String> {
    file_path.as_ref().map(|p| p.to_string())
}

/// The target kind for a function/method declaration, derived from its own modifiers. `None` for
/// constructors/destructors, which cannot carry attributes today.
fn function_target(f: &FunctionNode<'_>) -> Option<AttributeTarget> {
    if is_special_member_name(&f.name.text) {
        return None;
    }
    Some(if f.is_extern {
        AttributeTarget::ExternFunction
    } else if f.is_static {
        AttributeTarget::StaticMethod
    } else {
        AttributeTarget::Method
    })
}

fn validate_function_list(
    functions: &[FunctionNode<'_>],
    top_level: bool,
    user_attrs: &BTreeMap<String, Vec<ArgKind>>,
    diagnostics: &mut DiagnosticBag,
) {
    for f in functions {
        diagnostics.file_path = file_path_string(&f.file_path);
        if let Some(target) = function_target(f) {
            let target = if matches!(target, AttributeTarget::Method) && top_level {
                AttributeTarget::Function
            } else {
                target
            };
            validate_attributes_with(&f.attributes, target, user_attrs, diagnostics);
            if matches!(target, AttributeTarget::ExternFunction) {
                validate_c_extern_attrs(&f.attributes, diagnostics);
            }
        }
        for p in &f.parameters {
            validate_attributes_with(
                &p.attributes,
                AttributeTarget::Parameter,
                user_attrs,
                diagnostics,
            );
        }
    }
}

fn validate_fields(
    fields: &[StructFieldNode],
    user_attrs: &BTreeMap<String, Vec<ArgKind>>,
    diagnostics: &mut DiagnosticBag,
) {
    for field in fields {
        validate_attributes_with(
            &field.attributes,
            AttributeTarget::Field,
            user_attrs,
            diagnostics,
        );
    }
}

/// Walks every attribute-bearing declaration in the (fully merged, pre-derive) program once,
/// reporting unknown/misapplied/malformed attributes. Run from the driver right after source
/// loading and prelude merge, before `@json` derivation and semantic analysis, so both of those
/// later stages can assume every attribute they see already has valid shape and placement.
/// Synthesized declarations (`file_path: None` for structs/enums/functions, or
/// `is_synthesized` for `extend` blocks) are compiler-generated and always skipped.
pub fn validate_program_attributes(
    structs: &[StructDeclarationNode<'_>],
    interfaces: &[InterfaceDeclarationNode<'_>],
    functions: &[FunctionNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
    extends: &[ExtendNode<'_>],
    diagnostics: &mut DiagnosticBag,
) {
    let user_attrs = collect_user_attributes(functions, diagnostics);

    for s in structs {
        if s.file_path.is_none() {
            continue;
        }
        diagnostics.file_path = file_path_string(&s.file_path);
        let target = if s.is_value {
            AttributeTarget::ValueStruct
        } else {
            AttributeTarget::Struct
        };
        validate_attributes_with(&s.attributes, target, &user_attrs, diagnostics);
        validate_fields(&s.fields, &user_attrs, diagnostics);
        validate_function_list(&s.methods, false, &user_attrs, diagnostics);
    }

    for i in interfaces {
        if i.file_path.is_none() {
            continue;
        }
        diagnostics.file_path = file_path_string(&i.file_path);
        validate_attributes_with(
            &i.attributes,
            AttributeTarget::Interface,
            &user_attrs,
            diagnostics,
        );
        for m in &i.methods {
            validate_attributes_with(
                &m.attributes,
                AttributeTarget::InterfaceMethod,
                &user_attrs,
                diagnostics,
            );
        }
    }

    validate_function_list(functions, true, &user_attrs, diagnostics);

    for e in enums {
        if e.file_path.is_none() {
            continue;
        }
        diagnostics.file_path = file_path_string(&e.file_path);
        let target = if e.is_data_enum() {
            AttributeTarget::Union
        } else {
            AttributeTarget::PlainEnum
        };
        validate_attributes_with(&e.attributes, target, &user_attrs, diagnostics);
        for v in &e.variants {
            validate_fields(&v.fields, &user_attrs, diagnostics);
        }
        validate_function_list(&e.methods, false, &user_attrs, diagnostics);
    }

    for ext in extends {
        if ext.is_synthesized {
            continue;
        }
        diagnostics.file_path = file_path_string(&ext.file_path);
        validate_function_list(&ext.methods, false, &user_attrs, diagnostics);
    }
}
