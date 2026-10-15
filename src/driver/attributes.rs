//! Program-wide attribute preparation, shared by the compiler and the language server: collect
//! the declared `@attribute` types, validate every attribute use, and give each attribute type
//! the `__gen_*` statics `GenAttributes.id/decode/decode_all<A>()` forward to.

use crate::driver::source_loader::ProgramAccumulator;
use bumpalo::Bump;
use dream_abi::attributes::{ArgKind, UserAttribute, UserAttributes};
use dream_abi::intrinsics::{
    GEN_ATTRIBUTE_DECODE_ALL_MEMBER, GEN_ATTRIBUTE_DECODE_MEMBER, GEN_ATTRIBUTE_DECODER_PARAM,
    GEN_ATTRIBUTE_ID_MEMBER,
};
use dream_diagnostics::DiagnosticBag;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Error;

/// Std package whose `GenAttribute` model the synthesized decoders read.
pub const CODEGEN_PACKAGE: &str = "system.codegen";

/// Virtual file tag of the synthesized attribute decoders.
const DECODERS_PREFIX: &str = "<attribute-decoders/";

fn import_hint(name: &str) -> Option<String> {
    dream_stdlib::attributes::find_spec(name).map(|attribute| attribute.module.clone())
}

pub fn prepare<'a>(
    arena: &'a Bump,
    acc: &mut ProgramAccumulator<'a>,
    diagnostics: &mut DiagnosticBag,
) -> Result<UserAttributes, Error> {
    let module_of = |file: Option<&str>| crate::driver::generate::module_of(acc, file);
    let attributes = UserAttributes::collect(
        &acc.all_structs,
        &acc.all_enums,
        &module_of,
        (visibility_modules(acc), visibility_imports(acc)),
        diagnostics,
    )
    .with_import_hint(import_hint);
    dream_abi::attributes::validate_program_attributes(
        &acc.all_structs,
        &acc.all_interfaces,
        &acc.all_functions,
        &acc.all_enums,
        &acc.all_extends,
        &attributes,
        diagnostics,
    );
    for (file, program) in &acc.parsed_files {
        if let Some(module) = &program.module {
            diagnostics.file_path = Some(file.clone());
            dream_abi::attributes::validate_attributes(
                &module.attributes,
                dream_abi::attributes::AttributeTarget::Module,
                &attributes,
                diagnostics,
            );
        }
    }
    let codegen_loaded = dream_stdlib::resolve_packages_to_load(&acc.requested_std_packages)
        .iter()
        .any(|p| p.name == CODEGEN_PACKAGE);
    if codegen_loaded && attributes.iter().next().is_some() {
        acc.all_extends.retain(|e| {
            !e.file_path
                .as_deref()
                .is_some_and(|f| f.starts_with(DECODERS_PREFIX))
        });
        acc.file_contents
            .retain(|file, _| !file.starts_with(DECODERS_PREFIX));
        acc.file_modules
            .retain(|file, _| !file.starts_with(DECODERS_PREFIX));
        acc.generated_files
            .retain(|file| !file.starts_with(DECODERS_PREFIX));
        let mut modules: std::collections::BTreeMap<&str, Vec<&UserAttribute>> = Default::default();
        for attribute in attributes.iter() {
            modules
                .entry(&attribute.module)
                .or_default()
                .push(attribute);
        }
        for (module, declarations) in modules {
            let tag = format!("{DECODERS_PREFIX}{module}>");
            let source = decoder_source(&attributes, module, &declarations);
            let extends = parse_synthesized_extends(
                arena,
                source,
                &tag,
                diagnostics,
                &mut acc.file_contents,
            )?;
            acc.file_modules.insert(tag.clone(), module.into());
            acc.generated_files.insert(tag);
            acc.all_extends.extend(extends);
        }
    }
    Ok(attributes)
}

fn reader(kind: ArgKind, ty: &str, index: &str) -> String {
    match kind {
        ArgKind::String => format!("a.string_arg({index})"),
        ArgKind::Bool => format!("a.bool_arg({index})"),
        ArgKind::Float => format!("a.float_arg({index})"),
        ArgKind::Double => format!("a.double_arg({index})"),
        ArgKind::Int if ty == "int" => format!("a.int_arg({index})"),
        ArgKind::Int if ty == "long" => format!("a.long_arg({index})"),
        ArgKind::Int => format!("({ty})a.long_arg({index})"),
        ArgKind::Enum => format!("a.enum_member({index})"),
        ArgKind::Path => format!("DeclarationPath(a.args.get({index}).value)"),
    }
}

fn write_assignments(out: &mut String, attrs: &UserAttributes, attr: &UserAttribute, indent: &str) {
    for (i, p) in attr.params.iter().enumerate() {
        let target = format!("v.{}", p.name);
        if p.optional {
            let _ = writeln!(
                out,
                "{indent}{target} = Option.None;\n{indent}if a.args.length > {i} {{"
            );
        }
        let assign_one = |out: &mut String, target: &str, index: &str, push: bool| {
            let value = reader(p.kind, &p.ty, index);
            let store = |out: &mut String, expr: &str| {
                let wrapped = if p.optional {
                    format!("Option.Some({expr})")
                } else {
                    expr.to_string()
                };
                let expr = wrapped.as_str();
                if push {
                    if p.array {
                        let _ = writeln!(out, "{indent}    {target}[i - {i}] = {expr};");
                    } else {
                        let _ = writeln!(out, "{indent}    {target}.push({expr});");
                    }
                } else {
                    let _ = writeln!(out, "{indent}    {target} = {expr};");
                }
            };
            match (p.kind, &p.enum_type) {
                (ArgKind::Enum, Some(enum_type)) => {
                    if attrs.enum_members(enum_type).is_empty() {
                        return;
                    }
                    let _ = writeln!(out, "{indent}    let m = {value};");
                    for (n, member) in attrs.enum_members(enum_type).iter().enumerate() {
                        let kw = if n == 0 { "if" } else { "} else if" };
                        let _ = writeln!(out, "{indent}    {kw} m == \"{member}\" {{");
                        store(out, &format!("{}.{member}", p.ty));
                    }
                    let _ = writeln!(out, "{indent}    }}");
                }
                _ => store(out, &value),
            }
        };
        if p.variadic {
            if p.array {
                let _ = writeln!(
                    out,
                    "{indent}    {target} = Buffer.alloc<{}>(a.args.length - {i});",
                    p.ty
                );
            } else {
                let _ = writeln!(out, "{indent}    {target} = List<{}>();", p.ty);
            }
            let _ = writeln!(out, "{indent}    let i = {i};");
            let _ = writeln!(out, "{indent}    while i < a.args.length {{");
            assign_one(out, &target, "i", true);
            let _ = writeln!(out, "{indent}        i = i + 1;");
            let _ = writeln!(out, "{indent}    }}");
        } else {
            assign_one(out, &target, &i.to_string(), false);
        }
        if p.optional {
            let _ = writeln!(out, "{indent}}}");
        }
    }
}

fn decoder_source(attrs: &UserAttributes, module: &str, declarations: &[&UserAttribute]) -> String {
    let mut out = String::new();
    if !module.is_empty() {
        let _ = writeln!(out, "module {module};");
    }
    for attr in declarations {
        let name = &attr.type_name;
        let id = &attr.id;
        let _ = writeln!(out, "extend {name} {{");
        let _ = writeln!(
            out,
            "    public static fun {GEN_ATTRIBUTE_ID_MEMBER}(): string {{\n        return \"{id}\";\n    }}"
        );
        let _ = writeln!(
            out,
            "    public static fun {GEN_ATTRIBUTE_DECODE_ALL_MEMBER}<{GEN_ATTRIBUTE_DECODER_PARAM}>(borrow attrs: List<GenAttribute>): List<{name}> {{"
        );
        let _ = writeln!(out, "        let out = List<{name}>();");
        let _ = writeln!(out, "        for (let a in attrs) {{");
        let _ = writeln!(out, "            if a.id == \"{id}\" {{");
        let _ = writeln!(out, "                let v = {name}();");
        write_assignments(&mut out, attrs, attr, "            ");
        let _ = writeln!(out, "                out.push(v);");
        let _ = writeln!(
            out,
            "            }}\n        }}\n        return out;\n    }}"
        );
        let _ = writeln!(
            out,
            "    public static fun {GEN_ATTRIBUTE_DECODE_MEMBER}<{GEN_ATTRIBUTE_DECODER_PARAM}>(borrow attrs: List<GenAttribute>): Option<{name}> {{"
        );
        let _ = writeln!(out, "        for (let a in attrs) {{");
        let _ = writeln!(out, "            if a.id == \"{id}\" {{");
        let _ = writeln!(out, "                let v = {name}();");
        write_assignments(&mut out, attrs, attr, "            ");
        let _ = writeln!(out, "                return Option.Some(v);");
        let _ = writeln!(
            out,
            "            }}\n        }}\n        return Option.None;\n    }}"
        );
        let _ = writeln!(out, "}}\n");
    }
    out
}

/// Parses compiler-made Dream source holding only `extend` blocks, marked synthesized (skipped by
/// attribute validation). Callers register `synthetic_path` as a generated file so its code may
/// use the file-private declarations it was made for.
pub(crate) fn parse_synthesized_extends<'a>(
    arena: &'a Bump,
    source: String,
    synthetic_path: &str,
    diagnostics: &mut DiagnosticBag,
    file_contents: &mut HashMap<String, String>,
) -> Result<Vec<dream_syntax::nodes::ExtendNode<'a>>, Error> {
    use dream_syntax::lexer::Lexer;
    use dream_syntax::parser::Parser;

    file_contents.insert(synthetic_path.to_string(), source.clone());
    let mut local = DiagnosticBag::new(Some(synthetic_path.to_string()));
    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer, arena, &mut local);
    let parsed = parser.parse();
    diagnostics.extend(&local);
    let ast = parsed?;
    let program = ast.get_root();
    let file_tag: std::rc::Rc<str> = std::rc::Rc::from(synthetic_path);
    Ok(program
        .extends
        .iter()
        .cloned()
        .map(|mut extend_decl| {
            extend_decl.file_path = Some(file_tag.clone());
            extend_decl.is_synthesized = true;
            for method in extend_decl.methods.iter_mut() {
                method.file_path = Some(file_tag.clone());
            }
            extend_decl
        })
        .collect())
}

fn visibility_modules(acc: &ProgramAccumulator<'_>) -> std::collections::BTreeMap<String, String> {
    acc.file_contents
        .keys()
        .map(|file| {
            (
                file.clone(),
                acc.file_modules
                    .get(file)
                    .map(|m| m.to_string())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn visibility_imports(
    acc: &ProgramAccumulator<'_>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut imports = std::collections::BTreeMap::new();
    for file in acc.file_contents.keys() {
        let mut modules = Vec::new();
        if let Some(program) = acc.parsed_files.get(file) {
            for import in &program.imports {
                modules.push(import.module_name.text.replace('/', "."));
            }
            for path in acc.import_edges.get(file).into_iter().flatten() {
                modules.push(
                    acc.file_modules
                        .get(path)
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                );
            }
        } else if let Some(package) = dream_stdlib::package_for_source(file) {
            modules.extend(package.deps.iter().map(|d| d.to_string()));
        } else {
            modules.extend(
                dream_stdlib::resolve_packages_to_load(&acc.requested_std_packages)
                    .iter()
                    .map(|p| p.name.to_string()),
            );
        }
        imports.insert(file.clone(), modules);
    }
    imports
}
