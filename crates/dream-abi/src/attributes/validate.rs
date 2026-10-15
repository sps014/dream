//! Generic shape/placement validation for every attribute-bearing declaration.

use super::*;
use dream_syntax::nodes::function::FunctionNode;
use dream_syntax::nodes::interface_node::InterfaceDeclarationNode;
use dream_syntax::nodes::program::{EnumDeclarationNode, ExtendNode};
use dream_syntax::nodes::struct_node::{StructDeclarationNode, StructFieldNode};
use dream_syntax::nodes::types::{CONSTRUCTOR_NAME, DESTRUCTOR_NAME};
use std::rc::Rc;

pub(super) fn arg_matches_kind(arg: &AttributeArg, kind: ArgKind) -> bool {
    match (kind, arg) {
        (ArgKind::String, AttributeArg::String(_)) => true,
        (ArgKind::Int, AttributeArg::Int(_)) => true,
        (ArgKind::Float, AttributeArg::Float(_)) => true,
        // Allow unsuffixed float literal when a double param is expected (same as expression
        // expected-type retargeting for numeric literals).
        (ArgKind::Double, AttributeArg::Double(_) | AttributeArg::Float(_)) => true,
        (ArgKind::Bool, AttributeArg::Bool(_)) => true,
        (ArgKind::Enum | ArgKind::Path, AttributeArg::Enum(_)) => true,
        _ => false,
    }
}

pub(super) fn kind_name(kind: ArgKind) -> &'static str {
    match kind {
        ArgKind::String => "a string literal",
        ArgKind::Int => "an integer literal",
        ArgKind::Float => "a float literal",
        ArgKind::Double => "a double literal",
        ArgKind::Bool => "a boolean literal",
        ArgKind::Enum => "an enum member path",
        ArgKind::Path => "a declaration path",
    }
}

/// Validates placement and arguments, resolving identity before any compiler handler runs.
pub fn validate_attributes(
    attrs: &[AttributeNode],
    target: AttributeTarget,
    user_attrs: &UserAttributes,
    diagnostics: &mut DiagnosticBag,
) {
    let mut seen = Vec::new();
    for attr in attrs {
        let name = attr.name.text.as_str();
        let candidates = user_attrs.resolve(name, diagnostics.file_path.as_deref());
        if let [decl] = candidates.as_slice() {
            user_attrs.validate_use(decl, attr, target, diagnostics);
            if !decl.repeatable && seen.contains(&decl.id.as_str()) {
                diagnostics.report_error(
                    format!("duplicate '@{name}' attribute"),
                    Some(attr.name.position),
                );
            }
            seen.push(decl.id.as_str());
        } else if candidates.is_empty() {
            let hint = user_attrs
                .import_hint(name)
                .map(|pkg| format!("; it is declared in '{pkg}', add 'import {pkg};'"))
                .unwrap_or_default();
            diagnostics.report_error(
                format!("unknown attribute '@{name}'{hint}"),
                Some(attr.name.position),
            );
        } else {
            diagnostics.report_error(
                format!("ambiguous attribute '@{name}'"),
                Some(attr.name.position),
            );
        }
    }
    if has_named_attr(attrs, "inline") && has_named_attr(attrs, "noinline") {
        diagnostics.report_error(
            "'@inline' and '@noinline' cannot be combined".to_string(),
            attrs.first().map(|a| a.name.position),
        );
    }
}

fn file_path_string(file_path: &Option<Rc<str>>) -> Option<String> {
    file_path.as_ref().map(|p| p.to_string())
}

fn function_target(f: &FunctionNode<'_>) -> AttributeTarget {
    if f.is_extern {
        AttributeTarget::ExternFunction
    } else if f.name.text == CONSTRUCTOR_NAME {
        AttributeTarget::Constructor
    } else if f.name.text == DESTRUCTOR_NAME {
        AttributeTarget::Destructor
    } else if f.is_static {
        AttributeTarget::StaticMethod
    } else {
        AttributeTarget::Method
    }
}

fn validate_function_list(
    functions: &[FunctionNode<'_>],
    top_level: bool,
    user_attrs: &UserAttributes,
    diagnostics: &mut DiagnosticBag,
) {
    for f in functions {
        diagnostics.file_path = file_path_string(&f.file_path);
        {
            let target = function_target(f);
            let target = if matches!(target, AttributeTarget::Method) && top_level {
                AttributeTarget::Function
            } else {
                target
            };
            validate_attributes(&f.attributes, target, user_attrs, diagnostics);
            if matches!(target, AttributeTarget::ExternFunction) {
                validate_c_extern_attrs(&f.attributes, diagnostics);
            }
        }
        for p in &f.parameters {
            validate_attributes(
                &p.attributes,
                AttributeTarget::Parameter,
                user_attrs,
                diagnostics,
            );
            validate_marshal_context(&p.attributes, true, diagnostics);
            if has_named_attr(&p.attributes, "marshal")
                && !(f.is_extern && has_c_attr(&f.attributes))
            {
                diagnostics.report_error(
                    "'@marshal' requires a parameter of a '@c' extern".to_string(),
                    Some(p.name.position),
                );
            }
        }
    }
}

fn validate_fields(
    fields: &[StructFieldNode],
    user_attrs: &UserAttributes,
    diagnostics: &mut DiagnosticBag,
) {
    for field in fields {
        validate_attributes(
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
    user_attrs: &UserAttributes,
    diagnostics: &mut DiagnosticBag,
) {
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
        validate_attributes(&s.attributes, target, user_attrs, diagnostics);
        validate_fields(&s.fields, user_attrs, diagnostics);
        validate_function_list(&s.methods, false, user_attrs, diagnostics);
    }

    for i in interfaces {
        if i.file_path.is_none() {
            continue;
        }
        diagnostics.file_path = file_path_string(&i.file_path);
        validate_attributes(
            &i.attributes,
            AttributeTarget::Interface,
            user_attrs,
            diagnostics,
        );
        for m in &i.methods {
            validate_attributes(
                &m.attributes,
                AttributeTarget::InterfaceMethod,
                user_attrs,
                diagnostics,
            );
        }
    }

    validate_function_list(functions, true, user_attrs, diagnostics);

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
        validate_attributes(&e.attributes, target, user_attrs, diagnostics);
        for v in &e.variants {
            validate_fields(&v.fields, user_attrs, diagnostics);
        }
        validate_function_list(&e.methods, false, user_attrs, diagnostics);
    }

    for ext in extends {
        if ext.is_synthesized {
            continue;
        }
        diagnostics.file_path = file_path_string(&ext.file_path);
        validate_function_list(&ext.methods, false, user_attrs, diagnostics);
    }
}
