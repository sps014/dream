use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Error;
use std::path::Path;
use tracing::{error, info};

use crate::driver::gpu_gen::{self, GpuEmitResult};
use dream_abi::attributes::{
    c_import_target, c_marshal_charset, extern_import_target, has_c_attr, has_packed_attr,
};
use dream_syntax::nodes::function::ParameterNode;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::{AttributeNode, FunctionNode, ProgramNode, Type};

/// One live host import after MIR pruning: `(module, field)` as emitted on the WASM import.
pub type LiveImport = (String, String);

/// Emits a binary `.wasm` next to the `.wat`, and optionally an `.abi.json` describing the module's
/// **live** extern imports (for JS interop marshaling) and exported functions. When `gpu` is
/// non-empty, also writes a sibling `.wgsl` file and embeds a `"gpu"` section in the ABI (when ABI
/// is requested). Native `run` / `debug-adapter` also load `abi.gpu` for wgpu kernels/shaders.
pub(crate) fn emit_wasm_and_abi(
    wat_path: &str,
    wat_text: &str,
    program: &ProgramNode,
    gpu: &GpuEmitResult,
    live_imports: &[LiveImport],
    emit_abi: bool,
) -> Result<(), Error> {
    let base = Path::new(wat_path);

    let wasm_path = base.with_extension("wasm");
    match wat::parse_str(wat_text) {
        Ok(bytes) => {
            fs::write(&wasm_path, bytes)?;
            info!("created file: {}", wasm_path.display());
        }
        Err(e) => {
            error!("could not assemble binary wasm: {}", e);
        }
    }

    if !gpu.is_empty() {
        let wgsl_path = base.with_extension("wgsl");
        fs::write(&wgsl_path, gpu_gen::join_wgsl_module(gpu))?;
        info!("created file: {}", wgsl_path.display());
    }

    if emit_abi {
        let abi_path = base.with_extension("abi.json");
        fs::write(&abi_path, build_abi_json(program, gpu, live_imports))?;
        info!("created file: {}", abi_path.display());
    }
    Ok(())
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
fn fn_tag_from_types(params: &[Type], ret: &Type) -> String {
    fn one(t: &Type) -> &'static str {
        match t {
            Type::Integer(_) | Type::Boolean(_) | Type::Byte(_) | Type::Char(_) | Type::UInt(_) => {
                "i32"
            }
            Type::Long(_) | Type::ULong(_) => "i64",
            Type::Float(_) => "f32",
            Type::Double(_) => "f64",
            Type::Void => "void",
            _ => "ptr",
        }
    }
    let args: Vec<&str> = params.iter().map(one).collect();
    format!("fn:{}:{}", args.join(","), one(ret))
}

/// Builds the `.abi.json` describing live extern imports and exported functions. Externs are
/// taken from the AST (for accurate Dream names / async flags / type strings) but filtered to
/// `(module, field)` pairs that survived MIR import pruning.
pub(crate) fn build_abi_json(
    program: &ProgramNode,
    gpu: &GpuEmitResult,
    live_imports: &[LiveImport],
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

    fn c_param_tag(param: &ParameterNode, extern_attrs: &[AttributeNode]) -> String {
        // C out-params use Dream `ref` (address passed); tagged for the native host trampoline.
        if param.is_ref {
            let ty = param.type_.get_type();
            if ty == "long" {
                return "out_long".to_string();
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
            return fn_tag_from_types(params, ret);
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
            "bool" => "bool".to_string(),
            "float" => "float".to_string(),
            "double" => "double".to_string(),
            "byte" => "byte".to_string(),
            other => format!("struct_ptr:{other}"),
        }
    }

    fn extern_entry(func: &FunctionNode) -> Option<(String, String, String, Option<String>)> {
        if !func.is_extern || dream_abi::intrinsics::has_intrinsic_attr(&func.attributes) {
            return None;
        }
        let (import_module, import_name) =
            extern_import_target(&func.attributes, &func.name.text);
        let is_c = has_c_attr(&func.attributes);
        let params: Vec<String> = if is_c {
            func.parameters
                .iter()
                .map(|p| format!("\"{}\"", json_escape(&c_param_tag(p, &func.attributes))))
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
        if let Some((module, field, entry, c_lib)) = extern_entry(func) {
            if !live.contains(&(module.as_str(), field.as_str())) {
                continue;
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
    let structs_section = build_c_structs_section(program, &externs);

    format!(
        "{{\n  \"externs\": [\n{}\n  ],\n  \"exports\": [{}]{}{}{}\n}}\n",
        externs.join(",\n"),
        exports.join(", "),
        gpu_section,
        c_libs_section,
        structs_section,
    )
}

/// Collects the set of unmanaged value-struct names referenced by any *live* `@c` extern's param
/// tags (`struct_ptr:Name` / `out_struct:Name`), then emits a `"structs"` JSON object mapping each
/// to its size/align/packed flag and field offsets. Empty when no `@c` import needs a struct.
fn build_c_structs_section(program: &ProgramNode, externs: &[String]) -> String {
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
    let mut memo: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut entries: Vec<String> = Vec::new();
    for (name, decl) in &resolved {
        let packed = has_packed_attr(&decl.attributes);
        let (size, align, field_json) = compute_c_struct_layout(decl, packed, &by_name, &mut memo);
        entries.push(format!(
            "    \"{}\": {{ \"size\": {}, \"align\": {}, \"packed\": {}, \"fields\": [{}] }}",
            json_escape(name),
            size,
            align,
            packed,
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

/// C-ABI sizes for the primitives Dream currently exposes to `@c` structs.
fn c_prim_size_align(name: &str) -> Option<(u32, u32)> {
    match name {
        "bool" | "byte" => Some((1, 1)),
        "int" | "float" => Some((4, 4)),
        "long" | "double" | "ulong" => Some((8, 8)),
        // Everything else (`string`, arrays, class refs) is a 4-byte guest pointer.
        _ => None,
    }
}

/// Field type's `(size, align, wire tag)`: a value struct recurses; a primitive uses its C size;
/// a reference is a 4-byte pointer.
fn c_field_layout(
    ty: &Type,
    by_name: &BTreeMap<&str, &StructDeclarationNode<'_>>,
    memo: &mut BTreeMap<String, (u32, u32)>,
) -> (u32, u32, String) {
    match ty {
        Type::Struct(tok, None) => {
            let name = tok.text.as_str();
            if let Some(inner) = by_name.get(name) {
                let packed = has_packed_attr(&inner.attributes);
                let (size, align) = compute_c_struct_size(inner, packed, by_name, memo);
                return (size, align, format!("struct:{name}")); 
            }
            (4, 4, "ptr".to_string())
        }
        Type::Array(_) => (4, 4, "ptr".to_string()),
        _ => {
            let key = ty.get_type();
            if let Some((s, a)) = c_prim_size_align(&key) {
                return (s, a, key);
            }
            (4, 4, "ptr".to_string())
        }
    }
}

/// Computes just `(size, align)` for a struct with memoization (used by nested-field sizing).
fn compute_c_struct_size(
    decl: &StructDeclarationNode<'_>,
    packed: bool,
    by_name: &BTreeMap<&str, &StructDeclarationNode<'_>>,
    memo: &mut BTreeMap<String, (u32, u32)>,
) -> (u32, u32) {
    let name = decl.name.text.clone();
    if let Some(&cached) = memo.get(&name) {
        return cached;
    }
    // Insert placeholder to break potential (illegal) value-type cycles rather than looping.
    memo.insert(name.clone(), (0, 1));
    let mut offset = 0u32;
    let mut max_align = 1u32;
    for field in &decl.fields {
        let (size, align, _tag) = c_field_layout(&field.field_type, by_name, memo);
        if !packed {
            let rem = offset % align;
            if rem != 0 {
                offset += align - rem;
            }
            max_align = max_align.max(align);
        }
        offset += size;
    }
    if !packed && max_align > 1 {
        let rem = offset % max_align;
        if rem != 0 {
            offset += max_align - rem;
        }
    }
    let result = (offset, if packed { 1 } else { max_align });
    memo.insert(name, result);
    result
}


/// Full layout for one struct: `(size, align, formatted field JSON)`.
fn compute_c_struct_layout(
    decl: &StructDeclarationNode<'_>,
    packed: bool,
    by_name: &BTreeMap<&str, &StructDeclarationNode<'_>>,
    memo: &mut BTreeMap<String, (u32, u32)>,
) -> (u32, u32, String) {
    let mut offset = 0u32;
    let mut max_align = 1u32;
    let mut field_entries: Vec<String> = Vec::new();
    for field in &decl.fields {
        let (size, align, tag) = c_field_layout(&field.field_type, by_name, memo);
        if !packed {
            let rem = offset % align;
            if rem != 0 {
                offset += align - rem;
            }
        }
        field_entries.push(format!(
            "{{ \"name\": \"{}\", \"offset\": {}, \"ty\": \"{}\" }}",
            json_escape(&field.name.text),
            offset,
            json_escape(&tag),
        ));
        offset += size;
        if !packed {
            max_align = max_align.max(align);
        }
    }
    let total_align = if packed { 1 } else { max_align.max(1) };
    if !packed && total_align > 1 {
        let rem = offset % total_align;
        if rem != 0 {
            offset += total_align - rem;
        }
    }
    let field_json = field_entries.join(", ");
    (offset, total_align, field_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;
    use dream_syntax::lexer::Lexer;
    use dream_syntax::parser::Parser;
    use dream_diagnostics::DiagnosticBag;

    fn abi_json_for(source: &str) -> String {
        let mut diagnostics = DiagnosticBag::new(None);
        let lexer = Lexer::new(source.to_string());
        let arena = Bump::new();
        let mut parser = Parser::new(lexer, &arena, &mut diagnostics);
        let tree = parser.parse().expect("parse should succeed");
        let program = tree.get_root();
        let gpu = crate::driver::gpu_gen::GpuEmitResult::default();
        // Consider every extern in the source "live" so the extern actually reaches the JSON.
        let live: Vec<LiveImport> = program
            .functions
            .iter()
            .filter(|f| f.is_extern)
            .map(|f| {
                let (m, fld) = dream_abi::attributes::extern_import_target(&f.attributes, &f.name.text);
                (m, fld)
            })
            .collect();
        build_abi_json(program, &gpu, &live)
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
        assert!(
            json.contains("\"size\": 8"),
            "expected size 8: {}",
            json
        );
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
        assert!(json.contains("\"packed\": true"), "expected packed: {}", json);
        // byte(1) + int(4), packed → size 5, no trailing padding.
        assert!(json.contains("\"size\": 5"), "expected packed size 5: {}", json);
        assert!(json.contains("\"align\": 1"), "expected align 1: {}", json);
    }
}
