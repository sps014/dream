use super::model::{Function, Record, Type, dream_identifier};
use dream_abi::{attributes::C_ATTR, core_items::C_INTEROP_PACKAGE};
use dream_types::CScalar;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

fn integer_literal(scalar: CScalar, value: &str) -> Option<String> {
    use CScalar::*;
    let signed = value.parse::<i64>().ok();
    let unsigned = value.parse::<u64>().ok();
    let numerals = match scalar {
        Bool => return Some(if value == "0" { "false" } else { "true" }.into()),
        I8 | I16 | I32 => format!("0x{:X}", i32::try_from(signed?).ok()? as u32),
        I64 | ISize => format!("0x{:X}L", signed? as u64),
        Char => format!("{}u", value.parse::<i16>().ok()? as u8),
        U8 | U16 | U32 => format!("{}u", u32::try_from(unsigned?).ok()?),
        U64 | USize => format!("{}ul", unsigned?),
        _ => return None,
    };
    Some(format!("({}){numerals}", scalar.prim().name()))
}

pub(super) struct Api {
    pub(super) records: BTreeMap<String, Record>,
    pub(super) functions: BTreeMap<String, Function>,
    pub(super) constants: BTreeMap<String, (CScalar, String)>,
    pub(super) omissions: BTreeMap<String, String>,
}

pub(super) fn record_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Record(name) => Some(name),
        _ => None,
    }
}

impl Api {
    pub(super) fn add_constant(&mut self, name: &str, scalar: Option<CScalar>, value: &str) {
        if let Some(scalar) = scalar
            && dream_identifier(name)
            && let Some(literal) = integer_literal(scalar, value)
        {
            self.constants.insert(name.into(), (scalar, literal));
        } else {
            self.omissions.insert(
                name.into(),
                "integer constant has no Dream representation".into(),
            );
        }
    }

    fn record_order(&self) -> Vec<&str> {
        let mut order = Vec::new();
        let mut seen = BTreeSet::new();
        for name in self.records.keys() {
            let mut pending = vec![(name.as_str(), false)];
            while let Some((name, ready)) = pending.pop() {
                if ready {
                    order.push(name);
                } else if seen.insert(name) {
                    pending.push((name, true));
                    pending.extend(
                        self.records[name]
                            .fields
                            .iter()
                            .rev()
                            .filter_map(|(_, ty)| record_name(ty))
                            .map(|child| (child, false)),
                    );
                }
            }
        }
        order
    }

    pub(super) fn probes(&self) -> String {
        let mut c =
            String::from("#include <stddef.h>\n#include <stdint.h>\n#include <stdbool.h>\n");
        for name in self.record_order() {
            let record = &self.records[name];
            let packed = if record.packed {
                "__attribute__((packed)) "
            } else {
                ""
            };
            let mirror = format!("struct {packed}__dream_mirror_{name}");
            let ty = format!("struct __dream_mirror_{name}");
            let original = &record.c_name;
            let _ = writeln!(c, "{mirror} {{");
            for (field, t) in &record.fields {
                let _ = writeln!(c, "    {} {field};", t.c());
            }
            let _ = writeln!(c, "}};");
            let _ = write!(
                c,
                "__attribute__((used)) const unsigned char __dream_layout_{name} = sizeof({ty}) == sizeof({original}) && _Alignof({ty}) == _Alignof({original})"
            );
            for (index, (field, _)) in record.fields.iter().enumerate() {
                if record.inline_array {
                    let _ = write!(
                        c,
                        " && offsetof({ty}, {field}) == {index} * sizeof((*({original} *)0)[0])"
                    );
                } else {
                    let _ = write!(
                        c,
                        " && offsetof({ty}, {field}) == offsetof({original}, {field})"
                    );
                }
            }
            c.push_str(";\n");
        }
        for function in self.functions.values() {
            let name = &function.name;
            let mut params = function.params.iter().map(Type::c).collect::<Vec<_>>();
            if params.is_empty() {
                params.push("void".into());
            }
            let _ = writeln!(c, "__typeof__({name}) __dream_actual_{name};");
            let _ = writeln!(
                c,
                "{} __dream_binding_{name}({});",
                function.ret.c(),
                params.join(", ")
            );
            let _ = writeln!(
                c,
                "__attribute__((used)) static __typeof__(&__dream_actual_{name}) a_{name} = &__dream_actual_{name};"
            );
            let _ = writeln!(
                c,
                "__attribute__((used)) static __typeof__(&__dream_binding_{name}) b_{name} = &__dream_binding_{name};"
            );
        }
        c
    }

    pub(super) fn verify(&mut self, ir: &str) {
        let record_names = self
            .record_order()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut rejected = BTreeSet::new();
        for name in record_names {
            let expected = format!("@__dream_layout_{name} = ");
            let valid = ir
                .lines()
                .find(|l| l.starts_with(&expected))
                .is_some_and(|l| l.contains("constant i8 1,"))
                && self.records[&name]
                    .fields
                    .iter()
                    .filter_map(|(_, ty)| record_name(ty))
                    .all(|nested| !rejected.contains(nested));
            if !valid {
                self.records.remove(&name);
                self.omissions.insert(
                    name.clone(),
                    "C record layout differs from Dream's ABI layout".into(),
                );
                rejected.insert(name);
            }
        }
        let signatures = ir
            .lines()
            .filter(|l| l.starts_with("declare "))
            .filter_map(signature)
            .collect::<BTreeMap<_, _>>();
        self.functions.retain(|name, function| {
            let actual = signatures.get(format!("__dream_actual_{name}").as_str());
            let binding = signatures.get(format!("__dream_binding_{name}").as_str());
            let valid = actual.is_some()
                && actual == binding
                && function
                    .params
                    .iter()
                    .chain([&function.ret])
                    .filter_map(record_name)
                    .all(|ty| !rejected.contains(ty));
            if !valid {
                self.omissions.insert(
                    name.clone(),
                    "the lowered C signature differs from Dream's ABI".into(),
                );
            }
            valid
        });
    }

    pub(super) fn render(&self, prefix: &str, library: &str) -> String {
        let mut out = format!("// Generated from a C header.\nimport {C_INTEROP_PACKAGE};\n\n");
        for name in self.record_order() {
            let record = &self.records[name];
            if record.packed {
                out.push_str("@packed\n");
            }
            let _ = writeln!(out, "public struct {prefix}{name} {{");
            for (field, ty) in &record.fields {
                let _ = writeln!(out, "    public {field}: {};", ty.dream(prefix));
            }
            out.push_str("}\n\n");
        }
        for (name, (scalar, literal)) in &self.constants {
            let _ = writeln!(
                out,
                "public const {prefix}{name}: {} = {literal};",
                scalar.prim().name()
            );
        }
        for function in self.functions.values() {
            let name = &function.name;
            let params = function
                .params
                .iter()
                .enumerate()
                .map(|(i, t)| format!("arg{i}: {}", t.dream(prefix)))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "\n@{C_ATTR}(\"{library}\", \"{name}\")\npublic extern fun {prefix}{name}({params}): {};",
                function.ret.dream(prefix)
            );
        }
        out
    }
}

// Clang's LLVM declarations carry byval/sret attributes with record names. Layout checks
// establish record equivalence separately, so those names are immaterial to this comparison.
fn signature(line: &str) -> Option<(&str, String)> {
    let (prefix, rest) = line.split_once('@')?;
    let (name, signature) = rest.split_once('(')?;
    let signature = signature.split(" #").next()?;
    Some((
        name.strip_prefix(char::from(1)).unwrap_or(name),
        format!(
            "{}({}",
            normalize_nominal_names(prefix),
            normalize_nominal_names(signature)
        ),
    ))
}

fn normalize_nominal_names(mut tail: &str) -> String {
    let mut normalized = String::new();
    while let Some((before, after)) = tail.split_once('%') {
        normalized.push_str(before);
        normalized.push('%');
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || "_.$-".contains(c)))
            .unwrap_or(after.len());
        tail = &after[end..];
    }
    normalized.push_str(tail);
    normalized
}
