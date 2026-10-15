use super::user::AttributeEnum;
use super::*;
use dream_syntax::nodes::{EnumDeclarationNode, StructDeclarationNode};
use heck::ToSnakeCase;

fn placement_targets(member: &str) -> Option<&'static [AttributeTarget]> {
    Some(match member {
        "Constructor" => &[AttributeTarget::Constructor],
        "Destructor" => &[AttributeTarget::Destructor],
        "Class" => &[AttributeTarget::Struct],
        "Struct" => &[AttributeTarget::ValueStruct],
        "Union" => &[AttributeTarget::Union],
        "Enum" => &[AttributeTarget::PlainEnum],
        "Interface" => &[AttributeTarget::Interface],
        "Type" => &[
            AttributeTarget::Struct,
            AttributeTarget::ValueStruct,
            AttributeTarget::Union,
            AttributeTarget::PlainEnum,
            AttributeTarget::Interface,
        ],
        "Field" => &[AttributeTarget::Field],
        "Function" => &[AttributeTarget::Function],
        "Method" => &[AttributeTarget::Method, AttributeTarget::StaticMethod],
        "StaticMethod" => &[AttributeTarget::StaticMethod],
        "ExternFunction" => &[AttributeTarget::ExternFunction],
        "InterfaceMethod" => &[AttributeTarget::InterfaceMethod],
        "Module" => &[AttributeTarget::Module],
        "Parameter" => &[AttributeTarget::Parameter],
        _ => return None,
    })
}

impl UserAttributes {
    pub fn collect(
        structs: &[StructDeclarationNode<'_>],
        enums: &[EnumDeclarationNode<'_>],
        module_of: &dyn Fn(Option<&str>) -> String,
        visibility: (
            std::collections::BTreeMap<String, String>,
            std::collections::BTreeMap<String, Vec<String>>,
        ),
        diagnostics: &mut DiagnosticBag,
    ) -> Self {
        let mut out = Self::default().with_visibility(visibility.0, visibility.1);
        for e in enums.iter().filter(|e| !e.is_data_enum()) {
            let module = module_of(e.file_path.as_deref());
            out.enums.insert(
                decl_identity(&module, &e.name.text),
                AttributeEnum {
                    module,
                    file: e.file_path.as_deref().map(str::to_owned),
                    visibility: e.visibility,
                    members: e.variants.iter().map(|v| v.name.text.clone()).collect(),
                },
            );
        }
        for s in structs {
            let Some(decl) = s.attributes.iter().find(|a| a.name.text == ATTRIBUTE_ATTR) else {
                continue;
            };
            diagnostics.file_path = s.file_path.as_deref().map(str::to_owned);
            let type_name = s.name.text.clone();
            let name = type_name
                .strip_suffix("Attribute")
                .filter(|name| !name.is_empty())
                .map(str::to_snake_case)
                .unwrap_or_else(|| type_name.clone());
            let module = module_of(s.file_path.as_deref());
            if s.generic_parameters.as_ref().is_some_and(|g| !g.is_empty()) {
                diagnostics.report_error(
                    format!("attribute type '{name}' cannot be generic"),
                    Some(s.name.position),
                );
                continue;
            }
            if s.methods
                .iter()
                .any(|m| m.name.text == dream_syntax::nodes::types::CONSTRUCTOR_NAME)
            {
                diagnostics.report_error(format!("attribute type '{name}' cannot declare a constructor; its fields are the argument list"), Some(s.name.position));
                continue;
            }
            let mut targets = Vec::new();
            for arg in &decl.args {
                let target = match arg {
                    AttributeArg::Enum(path) => match path.as_slice() {
                        [owner, member] if owner.text == ATTRIBUTE_TARGET_ENUM => {
                            placement_targets(&member.text)
                        }
                        _ => None,
                    },
                    _ => None,
                };
                match target {
                    Some(ts) => {
                        for t in ts {
                            if !targets.contains(t) {
                                targets.push(*t);
                            }
                        }
                    }
                    None => diagnostics.report_error(
                        format!(
                            "'{}' is not an attribute placement; use AttributeTarget members",
                            arg.display()
                        ),
                        Some(arg.position()),
                    ),
                }
            }
            let mut params = Vec::new();
            let mut optional_seen = false;
            for (i, field) in s.fields.iter().enumerate() {
                let (ty, variadic, optional) = match &field.field_type {
                    Type::Struct(tok, Some(args))
                        if args.len() == 1 && matches!(tok.text.as_str(), "List" | "Option") =>
                    {
                        (&args[0], tok.text == "List", tok.text == "Option")
                    }
                    Type::Array(inner) => (inner.as_ref(), true, false),
                    ty => (ty, false, false),
                };
                if (variadic && (i + 1 != s.fields.len() || optional_seen))
                    || (!optional && optional_seen)
                {
                    diagnostics.report_error(format!("attribute type '{name}': optional fields must be trailing and cannot be combined with a variadic List; only the last field may be a List"), Some(field.name.position));
                }
                optional_seen |= optional;
                let scalar = match ty {
                    Type::String(_) => Some((ArgKind::String, None)),
                    Type::Integer(_)
                    | Type::Byte(_)
                    | Type::Long(_)
                    | Type::UInt(_)
                    | Type::ULong(_) => Some((ArgKind::Int, None)),
                    Type::Float(_) => Some((ArgKind::Float, None)),
                    Type::Double(_) => Some((ArgKind::Double, None)),
                    Type::Boolean(_) => Some((ArgKind::Bool, None)),
                    Type::Struct(tok, None) if tok.text == "DeclarationPath" => {
                        Some((ArgKind::Path, None))
                    }
                    Type::Struct(tok, None) => {
                        let candidates: Vec<_> = out
                            .enums
                            .iter()
                            .filter(|(id, e)| {
                                let name = id.rsplit_once("::").map(|(_, n)| n).unwrap_or(id);
                                name == tok.text
                                    && out.visible(
                                        &e.module,
                                        e.file.as_deref(),
                                        e.visibility,
                                        s.file_path.as_deref(),
                                    )
                            })
                            .collect();
                        let local = candidates.iter().find(|(_, e)| e.module == module);
                        let id = local
                            .map(|(id, _)| (*id).clone())
                            .or_else(|| (candidates.len() == 1).then(|| candidates[0].0.clone()));
                        id.map(|id| (ArgKind::Enum, Some(id)))
                    }
                    _ => None,
                };
                match scalar {
                    Some((kind, enum_type)) => params.push(UserAttributeParam { name: field.name.text.clone(), kind, ty: ty.get_type(), enum_type, variadic, optional, array: matches!(field.field_type, Type::Array(_)) }),
                    None => diagnostics.report_error(format!("attribute field '{}' has type '{}', which cannot be an attribute argument", field.name.text, field.field_type.display_name()), Some(field.name.position)),
                }
            }
            let id = decl_identity(&module, &type_name);
            let attr = UserAttribute {
                id: id.clone(),
                name: name.clone(),
                type_name,
                module,
                file: s.file_path.as_deref().map(str::to_owned),
                visibility: s.visibility,
                targets,
                params,
                repeatable: s.attributes.iter().any(|a| a.name.text == REPEATABLE_ATTR),
                doc: s
                    .attributes
                    .first()
                    .map(|attribute| &attribute.name.leading_trivia)
                    .unwrap_or(&s.name.leading_trivia)
                    .iter()
                    .map(|t| t.text.trim_start_matches('/').trim())
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            if out.by_id.insert(id, attr).is_some() {
                diagnostics.report_error(
                    format!("attribute '@{name}' is declared twice in the same module"),
                    Some(s.name.position),
                );
            }
        }
        out
    }
}
