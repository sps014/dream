//! The C runtime's ABI as the pinned clang lowered it.
//!
//! The backend never spells a runtime signature itself: the driver hands over the `define`,
//! `declare`, `attributes` and `target` lines of the disassembled runtime bitcode (plus an anchor
//! unit that references every function the native header declares), and every runtime call is
//! typed from this table. A missing or unsupported entry is an ICE.

use std::convert::TryFrom;

use dream_abi::target::TargetSpec;
use indexmap::IndexMap;

use super::ir::{FnAttr, FnTy, ParamAttr, Ty};

#[derive(Clone, Debug, PartialEq)]
pub struct RtFn {
    pub fty: FnTy,
    pub ret_attrs: Vec<ParamAttr>,
    pub param_attrs: Vec<Vec<ParamAttr>>,
    pub noreturn: bool,
    /// wasm32 `(module, field)` from clang's `import_module`/`import_name`.
    pub wasm_import: Option<(String, String)>,
}

impl RtFn {
    /// Attributes to put on our own `declare`. Only ABI-relevant facts clang already stated.
    pub fn decl_attrs(&self) -> Vec<FnAttr> {
        let mut out = vec![FnAttr::NoUnwind];
        if self.noreturn {
            out.push(FnAttr::NoReturn);
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RtGlobal {
    pub ty: Ty,
    pub thread_local: bool,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeSigs {
    pub triple: String,
    pub datalayout: String,
    pub fns: IndexMap<String, RtFn>,
    pub globals: IndexMap<String, RtGlobal>,
    /// `"target-cpu"`, `"target-features"`, `"tune-cpu"`, `"frame-pointer"` of the runtime's
    /// definitions. Our functions carry the same values so the inliner can merge across the
    /// runtime boundary and the platform's frame-pointer ABI holds everywhere.
    pub target_attrs: Vec<(String, String)>,
    /// wasm32 functions the runtime exports (`export_name`); the whole-program link keeps them.
    pub exports: Vec<String>,
}

const TARGET_ATTR_KEYS: &[&str] = &["target-cpu", "target-features", "tune-cpu", "frame-pointer"];

impl RuntimeSigs {
    pub fn function(&self, name: &str) -> &RtFn {
        self.fns
            .get(name)
            .unwrap_or_else(|| panic!("ICE: runtime has no function `{}`", name))
    }

    pub fn has_function(&self, name: &str) -> bool {
        self.fns.contains_key(name)
    }

    /// Rejects runtime artifacts built for a different target before their signatures shape IR.
    pub fn validate_target(&self, target: &TargetSpec) -> Result<(), String> {
        let runtime = parse_runtime_target(&self.triple)?;
        if runtime.triple != target.triple || runtime.min_os != target.min_os {
            return Err(format!(
                "target triple `{}` does not match selected target `{}`",
                self.triple, target.triple
            ));
        }
        if (runtime.ptr_size, runtime.ptr_align) != (target.ptr_size, target.ptr_align) {
            return Err(format!(
                "target triple `{}` has pointer layout ({}, {}) but selected target `{}` requires ({}, {})",
                self.triple,
                runtime.ptr_size,
                runtime.ptr_align,
                target.triple,
                target.ptr_size,
                target.ptr_align
            ));
        }
        let (size, align) = default_pointer_layout(&self.datalayout)?;
        if (size, align) != (target.ptr_size, target.ptr_align) {
            return Err(format!(
                "data layout `{}` has pointer layout ({size}, {align}) but selected target `{}` requires ({}, {})",
                self.datalayout, target.triple, target.ptr_size, target.ptr_align
            ));
        }
        Ok(())
    }

    /// Target identity alone cannot distinguish cached runtimes using the old integer handles.
    pub fn validate_reference_abi(&self, target: &TargetSpec) -> Result<(), String> {
        let reference = if target.capabilities.linear_memory {
            Ty::I32
        } else {
            Ty::Ptr
        };
        for (name, returned) in [
            ("dream_malloc", true),
            ("dream_retain", false),
            ("dream_release", false),
        ] {
            let function = self.fns.get(name).ok_or_else(|| {
                format!("missing required runtime symbol `{name}` (reference ABI)")
            })?;
            let actual = if returned {
                Some(&function.fty.ret)
            } else {
                function.fty.params.first()
            };
            if actual != Some(&reference) {
                return Err(format!(
                    "reference ABI mismatch: `{name}` {} must be {reference}, found {}",
                    if returned {
                        "return"
                    } else {
                        "first parameter"
                    },
                    actual
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "missing".into())
                ));
            }
        }
        Ok(())
    }

    /// Parses the reduced disassembly. Lines that are not external function definitions,
    /// declarations, globals, attribute groups or the target header are ignored.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut out = RuntimeSigs::default();
        let mut groups: IndexMap<String, String> = IndexMap::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("attributes #")
                && let Some((id, body)) = rest.split_once(" = { ")
            {
                groups.insert(
                    id.to_string(),
                    body.trim_end_matches('}').trim().to_string(),
                );
            }
        }
        let mut target_group: Option<String> = None;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("target triple = ") {
                out.triple = v.trim_matches('"').to_string();
            } else if let Some(v) = line.strip_prefix("target datalayout = ") {
                out.datalayout = v.trim_matches('"').to_string();
            } else if let Some(rest) = line.strip_prefix("define ") {
                if let Some((name, f, group)) = parse_fn(rest, &groups)? {
                    let body = group.as_ref().and_then(|g| groups.get(g));
                    if body.is_some_and(|b| b.contains("\"wasm-export-name\"")) {
                        out.exports.push(name.clone());
                    }
                    if target_group.is_none() {
                        target_group = group;
                    }
                    out.fns.insert(name, f);
                }
            } else if let Some(rest) = line.strip_prefix("declare ") {
                if let Some((name, f, _)) = parse_fn(rest, &groups)? {
                    out.fns.entry(name).or_insert(f);
                }
            } else if line.starts_with('@')
                && let Some((name, g)) = parse_global(line)
            {
                out.globals.insert(name, g);
            }
        }
        if let Some(body) = target_group.and_then(|g| groups.get(&g).cloned()) {
            for key in TARGET_ATTR_KEYS {
                if let Some(v) = string_attr(&body, key) {
                    out.target_attrs.push(((*key).to_string(), v));
                }
            }
        }
        if out.triple.is_empty() || out.datalayout.is_empty() {
            return Err("runtime signature table has no target triple/datalayout".into());
        }
        Ok(out)
    }
}

fn parse_runtime_target(triple: &str) -> Result<TargetSpec, String> {
    TargetSpec::parse(triple).or_else(|original| {
        let Some(msvc) = triple.find("-msvc") else {
            return Err(format!(
                "runtime target triple `{triple}` is invalid: {original}"
            ));
        };
        let suffix = &triple[msvc + "-msvc".len()..];
        if suffix.is_empty()
            || !suffix
                .chars()
                .all(|character| character.is_ascii_digit() || character == '.')
        {
            return Err(format!(
                "runtime target triple `{triple}` is invalid: {original}"
            ));
        }
        TargetSpec::parse(&triple[..msvc + "-msvc".len()])
            .map_err(|e| format!("runtime target triple `{triple}` is invalid: {e}"))
    })
}

fn default_pointer_layout(datalayout: &str) -> Result<(u32, u32), String> {
    let layout = datalayout
        .split('-')
        .find_map(|part| part.strip_prefix("p:").or_else(|| part.strip_prefix("p0:")));
    let Some(layout) = layout else {
        // LLVM's default address-space-zero pointer layout when `p0` is omitted.
        return Ok((8, 8));
    };
    let mut parts = layout.split(':');
    let bits = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or_else(|| format!("data layout `{datalayout}` has an invalid pointer size"))?;
    let align_bits = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or_else(|| format!("data layout `{datalayout}` has an invalid pointer alignment"))?;
    if bits % 8 != 0 || align_bits % 8 != 0 {
        return Err(format!(
            "data layout `{datalayout}` has a non-byte pointer layout"
        ));
    }
    Ok((bits / 8, align_bits / 8))
}

fn string_attr(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"=\"");
    let start = body.find(&pat)? + pat.len();
    let len = body[start..].find('"')?;
    Some(body[start..start + len].to_string())
}

const SKIP_LINKAGE: &[&str] = &[
    "internal",
    "private",
    "available_externally",
    "linkonce_odr",
];

type ParsedFn = (String, RtFn, Option<String>);

fn parse_fn(rest: &str, groups: &IndexMap<String, String>) -> Result<Option<ParsedFn>, String> {
    let Some(at) = rest.find(" @") else {
        return Ok(None);
    };
    let head = &rest[..at];
    let tail = &rest[at + 2..];
    let Some(open) = tail.find('(') else {
        return Ok(None);
    };
    let name = unquote(&tail[..open]);
    let head_toks = split_top(head, ' ');
    if head_toks.iter().any(|t| SKIP_LINKAGE.contains(&t.as_str())) {
        return Ok(None);
    }
    let Some(close) = matching_paren(tail, open) else {
        return Err(format!("unbalanced parameter list for @{name}"));
    };
    let Some((ret, ret_rest)) = parse_ret(&head_toks) else {
        return Ok(None);
    };
    let ret_attrs = ext_attrs(&ret_rest);
    let mut params = Vec::new();
    let mut param_attrs = Vec::new();
    let mut varargs = false;
    let mut unsupported = false;
    for p in split_top(&tail[open + 1..close], ',') {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if p == "..." {
            varargs = true;
            continue;
        }
        let toks = split_top(p, ' ');
        let Some((ty, _)) = toks.first().and_then(|t| parse_ty(t)) else {
            return Ok(None);
        };
        if toks
            .iter()
            .any(|t| t.starts_with("byval") || t.starts_with("sret") || t.starts_with("inalloca"))
        {
            unsupported = true;
        }
        param_attrs.push(ext_attrs(&toks[1..]));
        params.push(ty);
    }
    if unsupported {
        return Ok(None);
    }
    let after = &tail[close + 1..];
    let group = after
        .split_whitespace()
        .find_map(|t| t.strip_prefix('#'))
        .map(str::to_string);
    let body = group.as_ref().and_then(|g| groups.get(g));
    let noreturn = body.is_some_and(|b| b.split_whitespace().any(|w| w == "noreturn"));
    let wasm_import = body.and_then(|b| {
        Some((
            string_attr(b, "wasm-import-module")?,
            string_attr(b, "wasm-import-name")?,
        ))
    });
    let fty = FnTy {
        ret,
        params,
        varargs,
    };
    Ok(Some((
        name,
        RtFn {
            fty,
            ret_attrs,
            param_attrs,
            noreturn,
            wasm_import,
        },
        group,
    )))
}

/// The return type is the last type-shaped token of the head; everything after linkage and
/// before it is return attributes.
fn parse_ret(toks: &[String]) -> Option<(Ty, Vec<String>)> {
    for i in (0..toks.len()).rev() {
        if let Some((ty, "")) = parse_ty(&toks[i]) {
            return Some((ty, toks[..i].to_vec()));
        }
    }
    None
}

fn ext_attrs(toks: &[String]) -> Vec<ParamAttr> {
    toks.iter()
        .filter_map(|t| match t.as_str() {
            "zeroext" => Some(ParamAttr::ZeroExt),
            "signext" => Some(ParamAttr::SignExt),
            _ => None,
        })
        .collect()
}

fn parse_global(line: &str) -> Option<(String, RtGlobal)> {
    let (lhs, rhs) = line.split_once(" = ")?;
    let name = unquote(lhs.strip_prefix('@')?);
    let toks = split_top(rhs, ' ');
    if toks.iter().any(|t| SKIP_LINKAGE.contains(&t.as_str())) {
        return None;
    }
    let kw = toks.iter().position(|t| t == "global" || t == "constant")?;
    let thread_local = toks[..kw].iter().any(|t| t.starts_with("thread_local"));
    let ty_tok = toks.get(kw + 1)?;
    let (ty, _) = parse_ty(ty_tok.trim_end_matches(','))?;
    Some((name, RtGlobal { ty, thread_local }))
}

fn unquote(s: &str) -> String {
    s.trim_matches('"').to_string()
}

fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits on `sep` outside `()`, `[]`, `{}`, `<>` and quotes.
fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quoted = false;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            '(' | '[' | '{' | '<' if !quoted => depth += 1,
            ')' | ']' | '}' | '>' if !quoted => depth -= 1,
            _ => {}
        }
        if c == sep && depth == 0 && !quoted {
            if !cur.trim().is_empty() {
                out.push(cur.trim().to_string());
            }
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Parses one LLVM type from the start of `s`, returning the rest.
pub(crate) fn parse_ty(s: &str) -> Option<(Ty, &str)> {
    let s = s.trim_start();
    for (kw, ty) in [
        ("void", Ty::Void),
        ("float", Ty::F32),
        ("double", Ty::F64),
        ("ptr", Ty::Ptr),
    ] {
        if let Some(rest) = s.strip_prefix(kw)
            && !rest.starts_with(|c: char| c.is_ascii_alphanumeric())
        {
            return Some((ty, rest));
        }
    }
    if let Some(rest) = s.strip_prefix('i') {
        let n: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !n.is_empty() {
            return Some((Ty::Int(n.parse().ok()?), &rest[n.len()..]));
        }
        return None;
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (n, elem, rest) = sized(rest)?;
        return Some((Ty::Array(n, Box::new(elem)), rest.strip_prefix(']')?));
    }
    if let Some(rest) = s.strip_prefix("<{") {
        let (fields, rest) = fields(rest)?;
        return Some((
            Ty::Struct {
                packed: true,
                fields,
            },
            rest.trim_start().strip_prefix('>')?,
        ));
    }
    if let Some(rest) = s.strip_prefix('<') {
        let (n, elem, rest) = sized(rest)?;
        return Some((
            Ty::Vector(u32::try_from(n).ok()?, Box::new(elem)),
            rest.strip_prefix('>')?,
        ));
    }
    if let Some(rest) = s.strip_prefix('{') {
        let (fields, rest) = fields(rest)?;
        return Some((
            Ty::Struct {
                packed: false,
                fields,
            },
            rest,
        ));
    }
    None
}

fn sized(s: &str) -> Option<(u64, Ty, &str)> {
    let s = s.trim_start();
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    let n = digits.parse().ok()?;
    let rest = s[digits.len()..].trim_start().strip_prefix('x')?;
    let (elem, rest) = parse_ty(rest)?;
    Some((n, elem, rest.trim_start()))
}

fn fields(mut s: &str) -> Option<(Vec<Ty>, &str)> {
    let mut out = Vec::new();
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix('}') {
            return Some((out, rest));
        }
        let (t, rest) = parse_ty(s)?;
        out.push(t);
        s = rest.trim_start();
        if let Some(rest) = s.strip_prefix(',') {
            s = rest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"target datalayout = "e-m:o-i64:64"
target triple = "arm64-apple-macosx26.0.0"
@g0 = external thread_local global i64, align 8
@dream_rt_mt = global i32 0, align 4
@local = internal global i32 0
define void @dream_retain(i64 noundef %0) local_unnamed_addr #0 {
define noundef zeroext i1 @dream_rc_last(i64 noundef %0) #0 {
define internal void @helper() #0 {
declare void @dream_panic(ptr noundef) #1
declare i32 @printf(ptr noundef, ...) #2
declare { i64, i64 } @pair([2 x i64] %0, <4 x float> signext %1) #2
declare void @big(ptr byval(%struct.S) align 8) #2
attributes #0 = { nounwind "frame-pointer"="non-leaf" "target-cpu"="apple-m1" "target-features"="+neon" }
attributes #1 = { cold noreturn nounwind }
attributes #2 = { nounwind }
"#;

    #[test]
    fn parses_runtime_table() {
        let s = RuntimeSigs::parse(SAMPLE).expect("parse");
        assert_eq!(s.triple, "arm64-apple-macosx26.0.0");
        assert_eq!(s.fns.len(), 5, "{:?}", s.fns.keys().collect::<Vec<_>>());
        assert!(!s.has_function("helper"));
        assert!(!s.has_function("big"));
        let last = s.function("dream_rc_last");
        assert_eq!(last.fty.ret, Ty::I1);
        assert_eq!(last.ret_attrs, vec![ParamAttr::ZeroExt]);
        assert!(s.function("dream_panic").noreturn);
        assert!(s.function("printf").fty.varargs);
        let pair = s.function("pair");
        assert_eq!(
            pair.fty.to_string(),
            "{ i64, i64 } ([2 x i64], <4 x float>)"
        );
        assert_eq!(pair.param_attrs[1], vec![ParamAttr::SignExt]);
        assert!(s.globals["g0"].thread_local);
        assert_eq!(s.globals["dream_rt_mt"].ty, Ty::I32);
        assert!(!s.globals.contains_key("local"));
        assert_eq!(
            s.target_attrs,
            vec![
                ("target-cpu".to_string(), "apple-m1".to_string()),
                ("target-features".to_string(), "+neon".to_string()),
                ("frame-pointer".to_string(), "non-leaf".to_string()),
            ]
        );
    }

    #[test]
    fn validates_target_identity_and_pointer_layout() {
        let target = TargetSpec::parse("aarch64-apple-macosx26.0.0").unwrap();
        let sigs = RuntimeSigs::parse(SAMPLE).unwrap();
        sigs.validate_target(&target).unwrap();

        let wrong_target = TargetSpec::parse("x86_64-unknown-linux-gnu").unwrap();
        assert!(
            sigs.validate_target(&wrong_target)
                .unwrap_err()
                .contains("target triple")
        );

        let mut wrong_layout = sigs.clone();
        wrong_layout.datalayout = "e-p:32:32".into();
        assert!(
            wrong_layout
                .validate_target(&target)
                .unwrap_err()
                .contains("pointer layout")
        );
    }

    #[test]
    fn accepts_clang_msvc_version_suffix() {
        let target = TargetSpec::parse("x86_64-pc-windows-msvc").unwrap();
        let sigs = RuntimeSigs {
            triple: "x86_64-pc-windows-msvc19.44.35211".into(),
            datalayout: "e-m:w-i64:64-f80:128-n8:16:32:64-S128".into(),
            ..Default::default()
        };
        sigs.validate_target(&target).unwrap();
    }

    #[test]
    fn reference_abi_accepts_native_pointers_and_wasm_offsets() {
        for (triple, reference) in [
            ("x86_64-unknown-linux-gnu", "ptr"),
            ("wasm32-unknown-wasip1", "i32"),
        ] {
            let text = format!(
                "target triple = \"{triple}\"\ntarget datalayout = \"e\"\ndeclare {reference} @dream_malloc(i64, i32)\ndeclare void @dream_retain({reference})\ndeclare void @dream_release({reference})\n"
            );
            RuntimeSigs::parse(&text)
                .unwrap()
                .validate_reference_abi(&TargetSpec::parse(triple).unwrap())
                .unwrap();
        }
    }

    #[test]
    fn reference_abi_rejects_stale_integer_native_handles() {
        let target = TargetSpec::parse("x86_64-unknown-linux-gnu").unwrap();
        for name in ["dream_malloc", "dream_retain", "dream_release"] {
            let text = "target triple = \"x86_64-unknown-linux-gnu\"\ntarget datalayout = \"e\"\ndeclare ptr @dream_malloc(i64, i32)\ndeclare void @dream_retain(ptr)\ndeclare void @dream_release(ptr)\n";
            let old = if name == "dream_malloc" {
                text.replace("ptr @dream_malloc", "i64 @dream_malloc")
            } else {
                text.replace(&format!("@{name}(ptr)"), &format!("@{name}(i64)"))
            };
            let error = RuntimeSigs::parse(&old)
                .unwrap()
                .validate_reference_abi(&target)
                .unwrap_err();
            assert!(error.contains(name));
            assert!(error.contains("reference ABI mismatch"));
        }
    }
}
