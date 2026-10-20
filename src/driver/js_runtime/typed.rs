use std::collections::BTreeSet;
use std::fmt::Write;
use std::io::Error;

use crate::driver::abi::LiveImport;

pub(super) fn supports(ty: &str) -> bool {
    !ty.contains("js") && !ty.contains("fun(") && !ty.contains("[][]")
}

fn array(ty: &str) -> Option<(&'static str, &str)> {
    let element = ty.strip_suffix("[]")?;
    Some((
        match element {
            "byte" | "bool" => "Uint8Array",
            "uint" | "usize" | "string" => "Uint32Array",
            "long" => "BigInt64Array",
            "ulong" => "BigUint64Array",
            "float" => "Float32Array",
            "double" => "Float64Array",
            _ => "Int32Array",
        },
        element,
    ))
}

fn argument(ty: &str, name: &str) -> String {
    let ty = ty.trim_end_matches('?');
    if ty == "string" {
        return format!("readGuestString(inst, {name})");
    }
    if ty == "bool" {
        return format!("{name} !== 0");
    }
    if let Some((view, element)) = array(ty) {
        let value = format!("readGuestArray(inst, {name}, {view}, snapshot)");
        return match element {
            "bool" => format!("Array.from({value}, value => value !== 0)"),
            "string" => format!("Array.from({value}, value => readGuestString(inst, value))"),
            _ => value,
        };
    }
    name.to_string()
}

fn result(ty: &str) -> String {
    match ty.trim_end_matches('?') {
        "string" => "writeGuestString(inst, value)".into(),
        "bool" => "value ? 1 : 0".into(),
        "void" => "value ?? 0".into(),
        "long" | "ulong" => "BigInt(value ?? 0)".into(),
        ty => {
            if let Some((view, element)) = array(ty) {
                let convert = match element {
                    "string" => ", value => writeGuestString(inst, value)",
                    "long" | "ulong" => ", value => BigInt(value)",
                    "bool" => ", value => value ? 1 : 0",
                    _ => "",
                };
                format!("writeGuestArray(inst, value, {view}{convert})")
            } else {
                "value ?? 0".into()
            }
        }
    }
}

pub(super) fn generate(abi: &serde_json::Value, imports: &[LiveImport]) -> Result<String, Error> {
    let live: BTreeSet<_> = imports
        .iter()
        .map(|(module, field)| (module.as_str(), field.as_str()))
        .collect();
    let entries = abi["externs"]
        .as_array()
        .ok_or_else(|| Error::other("missing extern inventory"))?;
    let mut out = String::from(
        "function bindTypedImport(getInstance, fn, signature, user) {\nif (!signature) return null;\nswitch (signature.module + '\\0' + signature.field) {\n",
    );
    let mut seen = BTreeSet::new();
    for entry in entries {
        let module = entry["module"]
            .as_str()
            .ok_or_else(|| Error::other("extern has no module"))?;
        let field = entry["field"]
            .as_str()
            .ok_or_else(|| Error::other("extern has no field"))?;
        if !live.contains(&(module, field)) || !seen.insert((module, field)) {
            continue;
        }
        let params = entry["params"]
            .as_array()
            .ok_or_else(|| Error::other("extern has no parameter types"))?;
        let types: Vec<_> = params
            .iter()
            .map(|ty| {
                ty.as_str()
                    .ok_or_else(|| Error::other("invalid extern type"))
            })
            .collect::<Result<_, _>>()?;
        let ret = entry["result"]
            .as_str()
            .ok_or_else(|| Error::other("invalid extern result"))?;
        if !types
            .iter()
            .chain(std::iter::once(&ret))
            .all(|ty| supports(ty))
        {
            continue;
        }
        let key = serde_json::to_string(&format!("{module}\0{field}")).map_err(Error::other)?;
        let args = types
            .iter()
            .enumerate()
            .map(|(i, ty)| argument(ty, &format!("a{i}")))
            .collect::<Vec<_>>()
            .join(", ");
        let names = (0..types.len())
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let is_async = entry["async"] == true;
        let needs_inst = types
            .iter()
            .any(|ty| ty.contains("string") || ty.contains("[]"))
            || ret.contains("string")
            || ret.contains("[]");
        writeln!(out, "case {key}: {{ const snapshot = user || {is_async};").unwrap();
        if is_async {
            let convert = if ret == "long" || ret == "ulong" {
                format!("boxLong64(inst, value, TAGS.{})", ret.to_uppercase())
            } else {
                result(ret)
            };
            writeln!(out, "return startAsyncImport(getInstance, fn, (inst, [{names}]) => [{args}], (inst, value) => {convert});").unwrap();
        } else {
            let instance = if needs_inst {
                "const inst = getInstance();"
            } else {
                ""
            };
            writeln!(
                out,
                "return ({names}) => {{ {instance} const value = fn({args}); return {}; }};",
                result(ret)
            )
            .unwrap();
        }
        out.push_str("}\n");
    }
    out.push_str("default: return null;\n}\n}\n");
    Ok(out)
}
