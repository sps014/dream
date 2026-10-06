//! Declared attribute types: `@attribute(AttributeTarget.Field) struct property_name { name: string; }`.
//!
//! An `@attribute` struct is an ordinary Dream type whose fields form the positional argument
//! schema of `@name(...)`. Resolution is by type name over the loaded program (std packages only
//! load when imported, so an attribute from `system.json` is visible exactly when `Json` is).
//! Each declaration has a stable identity `module::name` that generators and the analyzer compare
//! instead of bare attribute spellings.

use super::validate::{arg_matches_kind, kind_name};
use super::*;
use dream_syntax::nodes::program::EnumDeclarationNode;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use std::collections::BTreeMap;

/// Identity of the `@json` attribute declared by `system.json`.
pub const JSON_ATTRIBUTE_ID: &str = "system.json::json";

/// Path segment every `@attribute(...)` placement argument starts with.
pub const ATTRIBUTE_TARGET_ENUM: &str = "AttributeTarget";

/// `module::name` identity of a top-level declaration (`module` is empty for the root module).
pub fn decl_identity(module: &str, name: &str) -> String {
    format!("{module}::{name}")
}

#[derive(Debug, Clone)]
pub struct UserAttributeParam {
    pub name: String,
    pub kind: ArgKind,
    /// Scalar Dream type spelling (`string`, `long`, `HttpMethod`, ...); the element type for a
    /// variadic list.
    pub ty: String,
    /// Plain-enum type name for [`ArgKind::Enum`] parameters.
    pub enum_type: Option<String>,
    /// Trailing `List<T>` field: collects every remaining argument.
    pub variadic: bool,
}

#[derive(Debug, Clone)]
pub struct UserAttribute {
    pub id: String,
    pub name: String,
    pub module: String,
    /// Empty means "any declaration".
    pub targets: Vec<AttributeTarget>,
    pub params: Vec<UserAttributeParam>,
    pub repeatable: bool,
}

impl UserAttribute {
    fn required(&self) -> usize {
        self.params.iter().filter(|p| !p.variadic).count()
    }

    fn accepts_any_count(&self) -> bool {
        self.params.last().is_some_and(|p| p.variadic)
    }
}

/// Every `@attribute` declaration in a merged program, keyed by attribute name.
#[derive(Debug, Clone, Default)]
pub struct UserAttributes {
    by_name: BTreeMap<String, UserAttribute>,
    enum_members: BTreeMap<String, Vec<String>>,
    /// Maps an unloaded attribute name to the std package that declares it.
    import_hint: Option<fn(&str) -> Option<String>>,
}

fn placement_targets(member: &str) -> Option<&'static [AttributeTarget]> {
    Some(match member {
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
        "Parameter" => &[AttributeTarget::Parameter],
        _ => return None,
    })
}

fn scalar_kind(
    ty: &Type,
    enums: &BTreeMap<String, Vec<String>>,
) -> Option<(ArgKind, Option<String>)> {
    match ty {
        Type::String(_) => Some((ArgKind::String, None)),
        Type::Integer(_) | Type::Byte(_) | Type::Long(_) | Type::UInt(_) | Type::ULong(_) => {
            Some((ArgKind::Int, None))
        }
        Type::Float(_) => Some((ArgKind::Float, None)),
        Type::Double(_) => Some((ArgKind::Double, None)),
        Type::Boolean(_) => Some((ArgKind::Bool, None)),
        Type::Struct(tok, None) if enums.contains_key(&tok.text) => {
            Some((ArgKind::Enum, Some(tok.text.clone())))
        }
        _ => None,
    }
}

impl UserAttributes {
    /// Collects every `@attribute` struct in the program, reporting malformed declarations.
    /// `module_of` maps a declaration's file to its module path.
    pub fn collect(
        structs: &[StructDeclarationNode<'_>],
        enums: &[EnumDeclarationNode<'_>],
        module_of: &dyn Fn(Option<&str>) -> String,
        diagnostics: &mut DiagnosticBag,
    ) -> Self {
        let mut out = UserAttributes::default();
        for e in enums.iter().filter(|e| !e.is_data_enum()) {
            out.enum_members.insert(
                e.name.text.clone(),
                e.variants.iter().map(|v| v.name.text.clone()).collect(),
            );
        }
        for s in structs {
            let Some(decl) = s.attributes.iter().find(|a| a.name.text == ATTRIBUTE_ATTR) else {
                continue;
            };
            diagnostics.file_path = s.file_path.as_ref().map(|p| p.to_string());
            let name = s.name.text.clone();
            if find_spec(&name).is_some() {
                diagnostics.report_error(
                    format!("attribute type '{name}' shadows the builtin attribute '@{name}'"),
                    Some(s.name.position),
                );
                continue;
            }
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
                diagnostics.report_error(
                    format!("attribute type '{name}' cannot declare a constructor; its fields are the argument list"),
                    Some(s.name.position),
                );
                continue;
            }
            let mut targets = Vec::new();
            for arg in &decl.args {
                let AttributeArg::Enum(path) = arg else {
                    continue;
                };
                let member = match path.as_slice() {
                    [owner, member] if owner.text == ATTRIBUTE_TARGET_ENUM => {
                        placement_targets(&member.text)
                    }
                    _ => None,
                };
                match member {
                    Some(ts) => {
                        for t in ts {
                            if !targets.contains(t) {
                                targets.push(*t);
                            }
                        }
                    }
                    None => diagnostics.report_error(
                        format!(
                            "'{}' is not an attribute placement; use AttributeTarget.Class, Struct, Union, Enum, Interface, Type, Field, Function, Method or Parameter",
                            arg.display()
                        ),
                        Some(arg.position()),
                    ),
                }
            }
            let mut params = Vec::new();
            let field_count = s.fields.len();
            for (i, field) in s.fields.iter().enumerate() {
                let list_elem = match &field.field_type {
                    Type::Struct(tok, Some(args)) if tok.text == "List" && args.len() == 1 => {
                        Some(&args[0])
                    }
                    _ => None,
                };
                let (ty, variadic) = match list_elem {
                    Some(elem) => (elem, true),
                    None => (&field.field_type, false),
                };
                if variadic && i + 1 != field_count {
                    diagnostics.report_error(
                        format!(
                            "attribute type '{name}': only the last field may be a List (it collects the remaining arguments)"
                        ),
                        Some(field.name.position),
                    );
                    continue;
                }
                match scalar_kind(ty, &out.enum_members) {
                    Some((kind, enum_type)) => params.push(UserAttributeParam {
                        name: field.name.text.clone(),
                        kind,
                        ty: ty.get_type(),
                        enum_type,
                        variadic,
                    }),
                    None => diagnostics.report_error(
                        format!(
                            "attribute field '{}' has type '{}', which cannot be an attribute argument (use string, int, long, float, double, bool, a plain enum, or a trailing List of those)",
                            field.name.text,
                            field.field_type.display_name()
                        ),
                        Some(field.name.position),
                    ),
                }
            }
            let module = module_of(s.file_path.as_deref());
            let attr = UserAttribute {
                id: decl_identity(&module, &name),
                name: name.clone(),
                module,
                targets,
                params,
                repeatable: s.attributes.iter().any(|a| a.name.text == REPEATABLE_ATTR),
            };
            if let Some(prev) = out.by_name.get(&name) {
                diagnostics.report_error(
                    format!(
                        "attribute '@{name}' is declared twice ('{}' and '{}')",
                        prev.id, attr.id
                    ),
                    Some(s.name.position),
                );
                continue;
            }
            out.by_name.insert(name, attr);
        }
        out
    }

    pub fn with_import_hint(mut self, hint: fn(&str) -> Option<String>) -> Self {
        self.import_hint = Some(hint);
        self
    }

    pub(super) fn import_hint(&self, name: &str) -> Option<String> {
        self.import_hint.and_then(|hint| hint(name))
    }

    /// Member names of a plain enum usable as an attribute argument type.
    pub fn enum_members(&self, enum_type: &str) -> &[String] {
        self.enum_members
            .get(enum_type)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn get(&self, name: &str) -> Option<&UserAttribute> {
        self.by_name.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &UserAttribute> {
        self.by_name.values()
    }

    /// Validates one use of a declared attribute (placement, arity, argument kinds).
    pub(super) fn validate_use(
        &self,
        decl: &UserAttribute,
        attr: &AttributeNode,
        target: AttributeTarget,
        diagnostics: &mut DiagnosticBag,
    ) {
        let name = &decl.name;
        if !decl.targets.is_empty() && !decl.targets.contains(&target) {
            diagnostics.report_error(
                format!("'@{}' cannot be applied to {}", name, target.display_name()),
                Some(attr.name.position),
            );
        }
        let required = decl.required();
        let count = attr.args.len();
        if count < required || (!decl.accepts_any_count() && count > required) {
            let expected = if decl.accepts_any_count() {
                format!("at least {required}")
            } else {
                required.to_string()
            };
            diagnostics.report_error(
                format!("'@{name}' expects {expected} argument(s), got {count}"),
                Some(attr.name.position),
            );
            return;
        }
        for (i, arg) in attr.args.iter().enumerate() {
            let Some(param) = decl.params.get(i.min(decl.params.len().saturating_sub(1))) else {
                break;
            };
            if !arg_matches_kind(arg, param.kind) {
                diagnostics.report_error(
                    format!(
                        "'@{}' argument {} must be {}, got '{}'",
                        name,
                        i + 1,
                        kind_name(param.kind),
                        arg.display()
                    ),
                    Some(arg.position()),
                );
                continue;
            }
            if let (AttributeArg::Enum(path), Some(enum_type)) = (arg, &param.enum_type) {
                let ok = match path.as_slice() {
                    [owner, member] => {
                        owner.text == *enum_type
                            && self
                                .enum_members
                                .get(enum_type)
                                .is_some_and(|ms| ms.contains(&member.text))
                    }
                    _ => false,
                };
                if !ok {
                    diagnostics.report_error(
                        format!(
                            "'@{}' argument {} must be a member of '{}', got '{}'",
                            name,
                            i + 1,
                            enum_type,
                            arg.display()
                        ),
                        Some(arg.position()),
                    );
                }
            }
        }
    }
}
