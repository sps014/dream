use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Error;
use std::path::Path;

use crate::driver::cpp_bridge::{CppBridge, WrittenShim, SHIM_RUNTIME_EXPORTS};
use crate::driver::gpu_gen::{self, GpuEmitResult};
use crate::driver::native_sets::{NativeGraph, NativeSet};
use dream_abi::attributes::{c_import_target, c_marshal_charset, extern_import_target, has_c_attr};
use dream_syntax::nodes::function::ParameterNode;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::{AttributeNode, FunctionNode, ProgramNode, Type};

/// One live host import after MIR pruning: `(module, field)` as emitted on the WASM import.
pub type LiveImport = (String, String);

/// WASM custom-section name the JS loader reads with `WebAssembly.Module.customSections`.
pub const ABI_CUSTOM_SECTION: &str = "dream-abi";

fn uleb32(mut n: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
    out
}

/// A WASM custom section (`id = 0`) named `name` with `payload` bytes.
pub(crate) fn wasm_custom_section(name: &str, payload: &[u8]) -> Vec<u8> {
    let mut body = uleb32(name.len() as u32);
    body.extend(name.as_bytes());
    body.extend(payload);
    let mut sec = vec![0u8];
    sec.extend(uleb32(body.len() as u32));
    sec.extend(body);
    sec
}

/// Appends the sibling `.abi.json` into the `.wasm` as a `dream-abi` custom section so
/// `run("./mod.wasm")` does not need a second fetch. Call **after** wasm-opt (Binaryen drops
/// unknown custom sections). Missing files are a no-op — assembly may have failed earlier.
pub(crate) fn embed_abi_in_wasm(wat_path: &str) -> Result<(), Error> {
    let base = Path::new(wat_path);
    let wasm_path = base.with_extension("wasm");
    let abi_path = base.with_extension("abi.json");
    if !wasm_path.exists() || !abi_path.exists() {
        return Ok(());
    }
    let abi = fs::read(&abi_path)?;
    let mut wasm = fs::read(&wasm_path)?;
    if wasm
        .windows(ABI_CUSTOM_SECTION.len())
        .any(|w| w == ABI_CUSTOM_SECTION.as_bytes())
    {
        return Ok(());
    }
    wasm.extend(wasm_custom_section(ABI_CUSTOM_SECTION, &abi));
    fs::write(&wasm_path, wasm)?;
    Ok(())
}

/// Writes the mandatory ABI sidecar for native capability linking and JS interop. GPU programs
/// also need sibling WGSL and ABI shader metadata for native run/debug and JS hosts.
/// Returns the paths of every file written so callers can surface them as build artifacts.
pub(crate) fn emit_wasm_and_abi(
    wat_path: &str,
    program: &ProgramNode,
    gpu: &GpuEmitResult,
    live_imports: &[LiveImport],
    native: &NativeGraph,
    cpp: &CppBridge,
    layouts: &dream_hir::LayoutTable,
) -> Result<Vec<std::path::PathBuf>, Error> {
    let base = Path::new(wat_path);
    let mut written = Vec::new();

    if !gpu.is_empty() {
        let wgsl_path = base.with_extension("wgsl");
        fs::write(&wgsl_path, gpu_gen::join_wgsl_module(gpu))?;
        written.push(wgsl_path);
    }

    let abi_path = base.with_extension("abi.json");
    let native_root = base
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("native-c");
    let live = live_set_names(live_imports);
    let shims = cpp.write(&native_root, |set| live.contains(set))?;
    fs::write(
        &abi_path,
        build_abi_json(program, gpu, live_imports, native, &shims, layouts),
    )?;
    written.push(abi_path);
    Ok(written)
}

/// Escapes a string for embedding in a JSON document.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// ABI tag for a Dream `fun(...)` parameter: `fn:i64,i32,ptr:i32`.
fn fn_tag_from_types(params: &[Type], ret: &Type, ptr_size: u32) -> String {
    fn one(t: &Type, ptr_size: u32) -> &'static str {
        match t {
            Type::Integer(_) | Type::Boolean(_) | Type::Byte(_) | Type::Char(_) | Type::UInt(_) => {
                "i32"
            }
            Type::Long(_) | Type::ULong(_) => "i64",
            Type::ISize(_) | Type::USize(_) => {
                if ptr_size == 8 {
                    "i64"
                } else {
                    "i32"
                }
            }
            Type::Float(_) => "f32",
            Type::Double(_) => "f64",
            Type::Void => "void",
            _ => "ptr",
        }
    }
    let args: Vec<&str> = params.iter().map(|t| one(t, ptr_size)).collect();
    format!("fn:{}:{}", args.join(","), one(ret, ptr_size))
}

/// Builds the `.abi.json` describing live extern imports and exported functions. Externs are
/// taken from the AST (for accurate Dream names / async flags / type strings) but filtered to
/// `(module, field)` pairs that survived MIR import pruning.
pub(crate) fn build_abi_json(
    program: &ProgramNode,
    gpu: &GpuEmitResult,
    live_imports: &[LiveImport],
    native: &NativeGraph,
    shims: &BTreeMap<String, WrittenShim>,
    layouts: &dream_hir::LayoutTable,
) -> String {
    let live: BTreeSet<(&str, &str)> = live_imports
        .iter()
        .map(|(m, f)| (m.as_str(), f.as_str()))
        .collect();

    fn type_name(t: Option<&Type>) -> String {
        match t {
            Some(t) => t.get_type(),
            None => "void".to_string(),
        }
    }

    fn c_param_tag(param: &ParameterNode, extern_attrs: &[AttributeNode], ptr_size: u32) -> String {
        // C out-params use Dream `ref` (address passed); tagged for the native host trampoline.
        if param.is_ref {
            let ty = param.type_.get_type();
            if ty == "long" {
                return "out_long".to_string();
            }
            if ty == "isize" || ty == "usize" {
                return if ptr_size == 8 { "out_long" } else { "out_int" }.to_string();
            }
            if ty == "int" {
                return "out_int".to_string();
            }
            if ty.starts_with("fun(") || matches!(param.type_, Type::Function(..)) {
                return "fn".to_string();
            }
            return format!("out_struct:{ty}");
        }
        let ty = &param.type_;
        if let Type::Function(params, ret) = ty {
            return fn_tag_from_types(params, ret, ptr_size);
        }
        if let Type::Array(inner) = ty {
            if matches!(**inner, Type::Byte(_)) {
                return "bytes".to_string();
            }
        }
        let type_str = ty.get_type();
        match type_str.as_str() {
            "string" => {
                if c_marshal_charset(extern_attrs) == Some("lpwstr") {
                    "string_utf16".to_string()
                } else {
                    "string".to_string()
                }
            }
            "int" => "int".to_string(),
            "long" => "long".to_string(),
            "isize" | "usize" => if ptr_size == 8 { "long" } else { "int" }.to_string(),
            "bool" => "bool".to_string(),
            "float" => "float".to_string(),
            "double" => "double".to_string(),
            "byte" => "byte".to_string(),
            other => format!("struct_ptr:{other}"),
        }
    }

    fn extern_entry(
        func: &FunctionNode,
        ptr_size: u32,
    ) -> Option<(String, String, String, Option<String>)> {
        if !func.is_extern || dream_abi::intrinsics::has_intrinsic_attr(&func.attributes) {
            return None;
        }
        let (import_module, import_name) = extern_import_target(&func.attributes, &func.name.text);
        let is_c = has_c_attr(&func.attributes);
        let params: Vec<String> = if is_c {
            func.parameters
                .iter()
                .map(|p| {
                    format!(
                        "\"{}\"",
                        json_escape(&c_param_tag(p, &func.attributes, ptr_size))
                    )
                })
                .collect()
        } else {
            func.parameters
                .iter()
                .map(|p| format!("\"{}\"", json_escape(&p.type_.get_type())))
                .collect()
        };
        let mut c_libs = None;
        let c_fields = if is_c {
            let (lib, symbol) = c_import_target(&func.attributes)?;
            c_libs = Some(lib.clone());
            Some(format!(
                ", \"kind\": \"c\", \"lib\": \"{}\", \"symbol\": \"{}\"",
                json_escape(&lib),
                json_escape(&symbol)
            ))
        } else {
            None
        };
        let entry = format!(
            "    {{ \"name\": \"{}\", \"module\": \"{}\", \"field\": \"{}\", \"params\": [{}], \"result\": \"{}\", \"async\": {}{} }}",
            json_escape(&func.name.text),
            json_escape(&import_module),
            json_escape(&import_name),
            params.join(", "),
            json_escape(&type_name(func.return_type.as_ref())),
            func.is_async,
            c_fields.unwrap_or_default(),
        );
        Some((import_module, import_name, entry, c_libs))
    }

    let mut externs = Vec::new();
    let mut host_capabilities = vec![dream_abi::host_capability::HostCapability::Core];
    let mut c_lib_set: BTreeSet<String> = BTreeSet::new();
    let mut seen_fields: BTreeSet<(String, String)> = BTreeSet::new();
    let class_methods = program.structs.iter().flat_map(|s| s.methods.iter());
    let extend_methods = program.extends.iter().flat_map(|e| e.methods.iter());
    for func in program
        .functions
        .iter()
        .chain(class_methods)
        .chain(extend_methods)
    {
        if let Some((module, field, entry, c_lib)) = extern_entry(func, layouts.target.ptr_size) {
            if !live.contains(&(module.as_str(), field.as_str())) {
                continue;
            }
            if let Some(package) = func
                .file_path
                .as_deref()
                .and_then(dream_stdlib::package_for_source)
            {
                host_capabilities.extend_from_slice(package.host_capabilities);
            }
            if let Some(lib) = c_lib {
                c_lib_set.insert(lib);
            }
            if !seen_fields.insert((module, field)) {
                continue;
            }
            externs.push(entry);
        }
    }

    let mut exports = Vec::new();
    for func in program.functions.iter() {
        if func.is_extern || func.generic_parameters.is_some() {
            continue;
        }
        if dream_abi::attributes::is_gpu_shader_attr(&func.attributes) {
            continue;
        }
        if func.visibility.is_public() || func.name.text == dream_mir::abi::ENTRY_FN {
            exports.push(format!("\"{}\"", json_escape(&func.name.text)));
        }
    }

    let gpu_section = if gpu.is_empty() {
        String::new()
    } else {
        format!(",\n  \"gpu\": {{ {} }}", gpu_gen::gpu_abi_json(gpu))
    };

    let live_sets: Vec<&NativeSet> = live_set_names(live_imports)
        .into_iter()
        .filter_map(|lib| native.sets.get(lib))
        .collect();
    c_lib_set.retain(|lib| !native.sets.contains_key(lib));
    let c_sources_section = c_sources_json(&live_sets, shims);

    let c_libs_section = if c_lib_set.is_empty() {
        String::new()
    } else {
        let libs: Vec<String> = c_lib_set
            .iter()
            .map(|l| format!("\"{}\"", json_escape(l)))
            .collect();
        format!(",\n  \"c_libs\": [{}]", libs.join(", "))
    };

    // Struct map for `@c`-referenced unmanaged value types (native host consults it to marshal
    // struct-pointer params and to size out-struct writebacks).
    let structs_section = build_c_structs_section(program, &externs, layouts);
    let host_capabilities = dream_abi::host_capability::HostCapability::ALL
        .iter()
        .filter(|capability| host_capabilities.contains(capability))
        .map(|capability| format!("\"{}\"", capability.name()))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "{{\n  \"native_abi_version\": 2,\n  \"externs\": [\n{}\n  ],\n  \"exports\": [{}],\n  \"host_capabilities\": [{}]{}{}{}{}\n}}\n",
        externs.join(",\n"),
        exports.join(", "),
        host_capabilities,
        gpu_section,
        c_libs_section,
        c_sources_section,
        structs_section,
    )
}

/// Libraries named by live `@c` imports (`c/<lib>` modules); native set names among them.
fn live_set_names(live_imports: &[LiveImport]) -> BTreeSet<&str> {
    live_imports
        .iter()
        .filter_map(|(m, _)| m.strip_prefix("c/"))
        .collect()
}

/// `"c_sources"`: every live native source set with absolute paths, compiled and linked by the
/// native build (`execution::native::native_c`). A set with `@cpp` declarations also compiles its
/// generated shim and keeps the runtime entry points the shim calls exported.
fn c_sources_json(sets: &[&NativeSet], shims: &BTreeMap<String, WrittenShim>) -> String {
    if sets.is_empty() {
        return String::new();
    }
    fn list<I: IntoIterator<Item = String>>(items: I) -> String {
        let quoted: Vec<String> = items
            .into_iter()
            .map(|s| format!("\"{}\"", json_escape(&s)))
            .collect();
        format!("[{}]", quoted.join(", "))
    }
    let path_list =
        |ps: &[std::path::PathBuf]| list(ps.iter().map(|p| p.to_string_lossy().into_owned()));
    let entries: Vec<String> = sets
        .iter()
        .map(|s| {
            let mut sources = s.sources.clone();
            let mut include = s.include.clone();
            let mut exports: Vec<String> = Vec::new();
            if let Some(shim) = shims.get(&s.name) {
                sources.push(shim.source.clone());
                include.push(shim.include.clone());
                exports.extend(SHIM_RUNTIME_EXPORTS.iter().map(|e| e.to_string()));
            }
            format!(
                "    {{ \"name\": \"{}\", \"sources\": {}, \"include\": {}, \"defines\": {}, \"cflags\": {}, \"frameworks\": {}, \"libs\": {}, \"runtime_exports\": {} }}",
                json_escape(&s.name),
                path_list(&sources),
                path_list(&include),
                list(s.defines.iter().cloned()),
                list(s.cflags.iter().cloned()),
                list(s.frameworks.iter().cloned()),
                list(s.libs.iter().cloned()),
                list(exports),
            )
        })
        .collect();
    format!(",\n  \"c_sources\": [\n{}\n  ]", entries.join(",\n"))
}

/// Collects the set of unmanaged value-struct names referenced by any *live* `@c` extern's param
/// tags (`struct_ptr:Name` / `out_struct:Name`), then emits a `"structs"` JSON object mapping each
/// to its size/align/packed flag and field offsets. Empty when no `@c` import needs a struct.
fn build_c_structs_section(
    program: &ProgramNode,
    externs: &[String],
    layouts: &dream_hir::LayoutTable,
) -> String {
    // Names mentioned as `"struct_ptr:X"` / `"out_struct:X"` in the already-rendered externs. We
    // parse them back out rather than re-walking the AST so this stays in perfect lockstep with
    // whatever `c_param_tag` actually emitted (including future tag variants).
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for entry in externs {
        for tag in tag_names_from_entry(entry) {
            wanted.insert(tag);
        }
    }
    if wanted.is_empty() {
        return String::new();
    }
    let by_name: BTreeMap<&str, &StructDeclarationNode<'_>> = program
        .structs
        .iter()
        .filter(|s| s.is_value)
        .map(|s| (s.name.text.as_str(), s))
        .collect();
    // Reachability closure: a wanted struct's value-struct field is itself an unmanaged type the
    // FFI must know the size of, so include it too (recursively).
    let mut resolved: BTreeMap<String, &StructDeclarationNode<'_>> = BTreeMap::new();
    let mut work: Vec<String> = wanted.into_iter().collect();
    while let Some(name) = work.pop() {
        if resolved.contains_key(&name) {
            continue;
        }
        let Some(decl) = by_name.get(name.as_str()) else {
            continue;
        };
        resolved.insert(name.clone(), *decl);
        for field in &decl.fields {
            if let Some(inner) = value_struct_name(&field.field_type) {
                if by_name.contains_key(inner) && !resolved.contains_key(inner) {
                    work.push(inner.to_string());
                }
            }
        }
    }
    if resolved.is_empty() {
        return String::new();
    }
    let mut entries: Vec<String> = Vec::new();
    for (name, decl) in &resolved {
        let Some(layout) = layouts.structs.values().find(|layout| layout.name == *name) else {
            continue;
        };
        let field_json = decl
            .fields
            .iter()
            .zip(&layout.fields)
            .map(|(field, field_layout)| {
                let tag = c_field_tag(&field.field_type, &by_name);
                format!(
                    "{{ \"name\": \"{}\", \"offset\": {}, \"ty\": \"{}\" }}",
                    json_escape(&field.name.text),
                    field_layout.offset,
                    json_escape(&tag),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        entries.push(format!(
            "    \"{}\": {{ \"size\": {}, \"align\": {}, \"packed\": {}, \"fields\": [{}] }}",
            json_escape(name),
            layout.size,
            layout.align,
            layout.packed,
            field_json,
        ));
    }
    format!(",\n  \"structs\": {{\n{}\n  }}", entries.join(",\n"))
}

/// Parses the `struct_ptr:X` / `out_struct:X` tags out of one already-formatted extern entry.
fn tag_names_from_entry(entry: &str) -> Vec<String> {
    let mut out = Vec::new();
    for prefix in ["\"struct_ptr:", "\"out_struct:"] {
        let mut rest = entry;
        while let Some(pos) = rest.find(prefix) {
            let after = &rest[pos + prefix.len()..];
            if let Some(end) = after.find('"') {
                out.push(after[..end].to_string());
                rest = &after[end..];
            } else {
                break;
            }
        }
    }
    out
}

/// Extracts a value-struct name from a field type (`Type::Struct("Name", None)`); `None` for
/// primitives, arrays, and reference types.
fn value_struct_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Struct(tok, None) => Some(tok.text.as_str()),
        _ => None,
    }
}

fn c_field_tag(ty: &Type, by_name: &BTreeMap<&str, &StructDeclarationNode<'_>>) -> String {
    match ty {
        Type::Struct(tok, None) => {
            let name = tok.text.as_str();
            if by_name.contains_key(name) {
                return format!("struct:{name}");
            }
            "ptr".to_string()
        }
        Type::Array(_) => "ptr".to_string(),
        _ => {
            let key = ty.get_type();
            if matches!(
                key.as_str(),
                "bool" | "byte" | "int" | "float" | "long" | "double" | "ulong" | "isize" | "usize"
            ) {
                return key;
            }
            "ptr".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use dream_diagnostics::DiagnosticBag;
    use dream_syntax::lexer::Lexer;
    use dream_syntax::parser::Parser;

    fn test_layouts(
        program: &ProgramNode<'_>,
        target: dream_hir::TargetLayout,
    ) -> dream_hir::LayoutTable {
        let mut types = dream_types::TypeCtx::new();
        for decl in &program.structs {
            let def = types.register(dream_types::DefKind::Struct, &decl.name.text, vec![]);
            if decl.is_value {
                types.defs.mark_value(def);
                types.interner.mark_value_def(def);
            }
        }
        let defs = program
            .structs
            .iter()
            .filter(|decl| decl.is_value)
            .map(|decl| {
                let ty = types.lower_str(&decl.name.text);
                dream_hir::StructLayoutDef {
                    ty,
                    name: decl.name.text.clone(),
                    fields: decl
                        .fields
                        .iter()
                        .map(|field| dream_hir::LayoutFieldDef {
                            name: field.name.text.clone(),
                            ty: types.lower(&field.field_type),
                            is_weak: field.is_weak,
                            is_unowned: field.is_unowned,
                        })
                        .collect(),
                    packed: dream_abi::attributes::has_packed_attr(&decl.attributes),
                    destructor: None,
                }
            })
            .collect();
        dream_hir::LayoutTable::build(target, &types.interner, defs, vec![])
    }

    fn abi_json_for(source: &str) -> String {
        abi_json_for_target(source, dream_hir::TargetLayout::default())
    }

    fn abi_json_for_target(source: &str, target: dream_hir::TargetLayout) -> String {
        let mut diagnostics = DiagnosticBag::new(None);
        let lexer = Lexer::new(source.to_string());
        let arena = Bump::new();
        let mut parser = Parser::new(lexer, &arena, &mut diagnostics);
        let tree = parser.parse().expect("parse should succeed");
        let program = tree.get_root();
        let layouts = test_layouts(program, target);
        let gpu = crate::driver::gpu_gen::GpuEmitResult::default();
        // Consider every extern in the source "live" so the extern actually reaches the JSON.
        let live: Vec<LiveImport> = program
            .functions
            .iter()
            .filter(|f| f.is_extern)
            .map(|f| {
                let (m, fld) =
                    dream_abi::attributes::extern_import_target(&f.attributes, &f.name.text);
                (m, fld)
            })
            .collect();
        build_abi_json(
            program,
            &gpu,
            &live,
            &NativeGraph::default(),
            &BTreeMap::new(),
            &layouts,
        )
    }

    #[test]
    fn pointer_integer_c_metadata_uses_target_width() {
        let source = r#"@c("c", "sizes") extern fun sizes(bytes: usize, offset: isize, callback: fun(usize): isize, ref out: usize): usize;"#;
        for (size, scalar, callback, output) in [
            (4, "int", "fn:i32:i32", "out_int"),
            (8, "long", "fn:i64:i64", "out_long"),
        ] {
            let json = abi_json_for_target(
                source,
                dream_hir::TargetLayout {
                    ptr_size: size,
                    ptr_align: size,
                },
            );
            assert!(
                json.contains(&format!(
                    "\"params\": [\"{scalar}\", \"{scalar}\", \"{callback}\", \"{output}\"]"
                )),
                "{}",
                json
            );
        }
    }

    #[test]
    fn live_native_sets_become_c_sources_not_c_libs() {
        let mut diagnostics = DiagnosticBag::new(None);
        let source = "@c(\"kv\", \"kv_open\") extern fun kv_open(): int;\n@c(\"z\", \"inflate\") extern fun inflate(): int;\n";
        let arena = Bump::new();
        let mut parser = Parser::new(Lexer::new(source.to_string()), &arena, &mut diagnostics);
        let tree = parser.parse().expect("parse should succeed");
        let live: Vec<LiveImport> = vec![
            ("c/kv".into(), "kv_open".into()),
            ("c/z".into(), "inflate".into()),
        ];
        let mut graph = NativeGraph::default();
        graph.sets.insert(
            "kv".into(),
            NativeSet {
                name: "kv".into(),
                root: "/p".into(),
                sources: vec!["/p/native/kv.c".into()],
                include: vec!["/p/native".into()],
                defines: vec!["A=1".into()],
                cflags: vec![],
                frameworks: vec![],
                libs: vec!["m".into()],
            },
        );
        let mut shims = BTreeMap::new();
        shims.insert(
            "kv".to_string(),
            WrittenShim {
                source: "/out/native-c/kv/shim.cpp".into(),
                include: "/out/native-c/kv".into(),
            },
        );
        let json = build_abi_json(
            tree.get_root(),
            &crate::driver::gpu_gen::GpuEmitResult::default(),
            &live,
            &graph,
            &shims,
            &dream_hir::LayoutTable::default(),
        );
        assert!(json.contains("\"c_libs\": [\"z\"]"), "{}", json);
        assert!(json.contains("\"c_sources\""), "{}", json);
        assert!(
            json.contains("\"/p/native/kv.c\", \"/out/native-c/kv/shim.cpp\""),
            "{}",
            json
        );
        assert!(json.contains("\"libs\": [\"m\"]"), "{}", json);
        assert!(
            json.contains(
                "\"runtime_exports\": [\"dream_callback_retain\", \"dream_callback_release\"]"
            ),
            "{}",
            json
        );
    }

    #[test]
    fn abi_emits_kind_c_and_c_libs_for_c_extern() {
        let source = r#"
            @c("sqlite3", "sqlite3_close")
            extern fun sqlite3_close(db: long): int;
        "#;
        let json = abi_json_for(source);
        assert!(json.contains("\"kind\": \"c\""), "missing kind:c: {}", json);
        assert!(
            json.contains("\"lib\": \"sqlite3\""),
            "missing lib field: {}",
            json
        );
        assert!(
            json.contains("\"symbol\": \"sqlite3_close\""),
            "missing symbol field: {}",
            json
        );
        assert!(
            json.contains("\"c_libs\": [\"sqlite3\"]"),
            "missing c_libs section: {}",
            json
        );
    }

    #[test]
    fn abi_ref_long_param_is_out_long() {
        let source = r#"
            @c("sqlite3", "sqlite3_open")
            extern fun sqlite3_open(path: string, ref db: long): int;
        "#;
        let json = abi_json_for(source);
        // The `ref db: long` param becomes `out_long`, the `string` stays `string`.
        assert!(
            json.contains("\"params\": [\"string\", \"out_long\"]"),
            "expected params [string, out_long]: {}",
            json
        );
    }

    #[test]
    fn abi_emits_structs_section_for_c_struct_ptr() {
        let source = r#"
            struct Point {
                x: int;
                y: int;
            }
            @c("mylib", "point_new")
            extern fun point_new(p: Point): int;
        "#;
        let json = abi_json_for(source);
        assert!(
            json.contains("\"structs\":"),
            "expected structs section: {}",
            json
        );
        assert!(
            json.contains("\"Point\":"),
            "expected Point entry: {}",
            json
        );
        assert!(json.contains("\"size\": 8"), "expected size 8: {}", json);
    }

    #[test]
    fn abi_packed_struct_is_size_1_packed() {
        let source = r#"
            @packed
            struct Header {
                kind: byte;
                length: int;
            }
            @c("mylib", "header_read")
            extern fun header_read(h: Header): int;
        "#;
        let json = abi_json_for(source);
        assert!(
            json.contains("\"packed\": true"),
            "expected packed: {}",
            json
        );
        // byte(1) + int(4), packed → size 5, no trailing padding.
        assert!(
            json.contains("\"size\": 5"),
            "expected packed size 5: {}",
            json
        );
        assert!(json.contains("\"align\": 1"), "expected align 1: {}", json);
    }

    #[test]
    fn nested_struct_abi_uses_the_selected_target_layout() {
        let source = r#"
            struct Inner {
                text: string;
                count: int;
            }
            struct Outer {
                tag: byte;
                inner: Inner;
                total: double;
            }
            @c("mylib", "consume")
            extern fun consume(value: Outer): int;
        "#;
        let mut diagnostics = DiagnosticBag::new(None);
        let arena = Bump::new();
        let mut parser = Parser::new(Lexer::new(source.to_string()), &arena, &mut diagnostics);
        let tree = parser.parse().expect("parse should succeed");
        let program = tree.get_root();
        let live = vec![("c/mylib".to_string(), "consume".to_string())];
        for (target, expected) in [
            (dream_hir::TargetLayout::default(), 24),
            (
                dream_hir::TargetLayout {
                    ptr_size: 8,
                    ptr_align: 8,
                },
                32,
            ),
        ] {
            let layouts = test_layouts(program, target);
            let outer = layouts
                .structs
                .values()
                .find(|layout| layout.name == "Outer")
                .expect("Outer layout");
            assert_eq!(outer.size, expected);
            let json = build_abi_json(
                program,
                &crate::driver::gpu_gen::GpuEmitResult::default(),
                &live,
                &NativeGraph::default(),
                &BTreeMap::new(),
                &layouts,
            );
            assert!(
                json.contains(&format!("\"Outer\": {{ \"size\": {expected}")),
                "{}",
                json
            );
        }
    }

    #[test]
    fn sizeof_lowering_and_abi_agree_for_nested_value_structs() {
        let source = r#"
            struct Inner { public text: string; public count: int; }
            struct Outer { public tag: byte; public inner: Inner; public total: double; }
            public fun footprint(): int { return sizeof(Outer); }
        "#;
        let mut diagnostics = DiagnosticBag::new(None);
        let arena = Bump::new();
        let mut parser = Parser::new(Lexer::new(source.to_string()), &arena, &mut diagnostics);
        let tree = parser.parse().expect("parse should succeed");
        for (ptr_size, expected) in [(4, 24), (8, 32)] {
            let mut analyzer = dream_sema::analyzer::Analyzer::new(&tree, &arena)
                .with_crate_type(dream_sema::analyzer::CrateType::Lib, None)
                .with_target_layout(dream_hir::TargetLayout {
                    ptr_size,
                    ptr_align: ptr_size,
                });
            let hir = analyzer
                .analyze(&mut diagnostics)
                .expect("analysis should succeed")
                .hir;
            assert!(!diagnostics.has_errors(), "{:?}", diagnostics.diagnostics);
            let mir = dream_mir::lower::lower_program(&hir, analyzer.interner());
            let function = mir
                .functions
                .iter()
                .find(|f| f.name == "footprint")
                .unwrap();
            assert!(function.blocks.iter().any(|block| matches!(
                block.terminator,
                dream_mir::Terminator::Return(Some(dream_mir::Operand::Const(dream_mir::Const::Int(n))))
                    if n == expected
            )));
            let section = build_c_structs_section(
                tree.get_root(),
                &["\"struct_ptr:Outer\"".into()],
                &hir.layouts,
            );
            assert!(
                section.contains(&format!("\"Outer\": {{ \"size\": {expected}")),
                "{}",
                section
            );
        }
    }

    #[test]
    fn wasm_custom_section_carries_name_and_payload() {
        let sec = wasm_custom_section(ABI_CUSTOM_SECTION, br#"{"externs":[]}"#);
        assert_eq!(sec[0], 0, "custom section id");
        let joined = String::from_utf8_lossy(&sec);
        assert!(joined.contains(ABI_CUSTOM_SECTION));
        assert!(joined.contains("externs"));
    }
}
