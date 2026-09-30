//! Rewrites one file's `@cpp` declarations into ordinary Dream: each class becomes a class with
//! hidden handle/ownership/owner slots whose members call private `@c` externs bound to the
//! generated shim. Everything downstream (sema, MIR, the C marshaling glue) sees plain Dream.

use std::fmt::Write as _;

use dream_abi::c_abi::cpp_shim_symbol;

use super::model::{Class, FileDecls, Member, MemberKind};
use super::types::{Bridge, Ret};

/// Private to each generated file, so every file gets its own Dream name for the set's
/// last-error accessor.
fn last_error_fn(file_index: usize) -> String {
    format!("__cpp_last_error_{file_index}")
}

pub(super) fn last_error_symbol(set: &str) -> String {
    cpp_shim_symbol(set, "", "__last_error", 0)
}

pub(super) fn dream_source(decls: &FileDecls, file_index: usize) -> String {
    let mut out = String::new();
    let mut externs = Vec::new();
    let mut uses_result = false;
    for c in &decls.classes {
        class_source(decls, c, file_index, &mut out, &mut externs, &mut uses_result);
    }
    for m in &decls.free {
        member_source(decls, None, m, file_index, &mut out, &mut externs, &mut uses_result);
    }
    if uses_result {
        externs.push(format!(
            "@c(\"{}\", \"{}\") extern fun {}(): string;",
            decls.set,
            last_error_symbol(&decls.set),
            last_error_fn(file_index)
        ));
    }
    for e in externs {
        out.push_str(&e);
        out.push('\n');
    }
    out
}

fn class_source(
    decls: &FileDecls,
    c: &Class,
    file_index: usize,
    out: &mut String,
    externs: &mut Vec<String>,
    uses_result: &mut bool,
) {
    let n = &c.name;
    let vis = if c.public { "public " } else { "" };
    let _ = write!(
        out,
        "{vis}class {n} {{\n\
         \x20   __cpp_handle: CPtr;\n\
         \x20   __cpp_owned: bool;\n\
         \x20   __cpp_owner: Option<object>;\n\
         \n\
         \x20   constructor(__cpp_handle: CPtr, __cpp_owned: bool, __cpp_owner: Option<object>) {{\n\
         \x20       this.__cpp_handle = __cpp_handle;\n\
         \x20       this.__cpp_owned = __cpp_owned;\n\
         \x20       this.__cpp_owner = __cpp_owner;\n\
         \x20   }}\n\
         \n\
         \x20   public static fun __cpp_wrap(h: CPtr, owned: bool, owner: Option<object>): {n} {{\n\
         \x20       return {n}(h, owned, owner);\n\
         \x20   }}\n\
         \n\
         \x20   public static fun __cpp_handle_of(o: {n}): CPtr {{\n\
         \x20       return o.__cpp_handle;\n\
         \x20   }}\n\
         \n\
         \x20   del() {{\n\
         \x20       if (this.__cpp_owned) {{\n\
         \x20           {del}(this.__cpp_handle);\n\
         \x20       }}\n\
         \x20   }}\n",
        del = c.delete_symbol,
    );
    externs.push(format!(
        "@c(\"{}\", \"{}\") extern fun {}(h: CPtr): void;",
        decls.set, c.delete_symbol, c.delete_symbol
    ));
    for m in &c.members {
        out.push('\n');
        member_source(decls, Some(c), m, file_index, out, externs, uses_result);
    }
    out.push_str("}\n\n");
}

fn extern_ret(b: &Bridge) -> String {
    match b {
        Bridge::Scalar(s) => s.dream().to_string(),
        Bridge::Str { optional: false } => "string".into(),
        Bridge::Str { optional: true } => "Option<string>".into(),
        Bridge::Ptr { optional: false } | Bridge::Class { optional: false, .. } => "CPtr".into(),
        Bridge::Ptr { optional: true } | Bridge::Class { optional: true, .. } => {
            "Option<CPtr>".into()
        }
        Bridge::Struct(_) | Bridge::Array(_) | Bridge::Fun { .. } => {
            unreachable!("rejected as a @cpp result by Known::ret")
        }
    }
}

fn member_source(
    decls: &FileDecls,
    class: Option<&Class>,
    m: &Member,
    file_index: usize,
    out: &mut String,
    externs: &mut Vec<String>,
    uses_result: &mut bool,
) {
    let indent = if class.is_some() { "    " } else { "" };
    let mut ext_params: Vec<String> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    let mut prelude = String::new();
    if m.kind == MemberKind::Instance {
        ext_params.push("__cpp_h: CPtr".into());
        args.push("this.__cpp_handle".into());
    }
    for (i, p) in m.params.iter().enumerate() {
        let r = if p.is_ref { "ref " } else { "" };
        match &p.bridge {
            Bridge::Class {
                name,
                optional: false,
            } => {
                ext_params.push(format!("p{i}: CPtr"));
                args.push(format!("{name}.__cpp_handle_of({})", p.name));
            }
            Bridge::Class {
                name,
                optional: true,
            } => {
                ext_params.push(format!("p{i}: Option<CPtr>"));
                let _ = write!(
                    prelude,
                    "{indent}    let __cpp_a{i}: Option<CPtr> = Option.None;\n\
                     {indent}    switch ({p}) {{\n\
                     {indent}        Some(__cpp_x) => {{ __cpp_a{i} = Option.Some({name}.__cpp_handle_of(__cpp_x)); }},\n\
                     {indent}        None => {{}},\n\
                     {indent}    }}\n",
                    p = p.name,
                );
                args.push(format!("__cpp_a{i}"));
            }
            Bridge::Array(_) => {
                ext_params.push(format!("p{i}: {}", p.dream));
                ext_params.push(format!("p{i}_len: int"));
                args.push(p.name.clone());
                args.push(format!("{}.length", p.name));
            }
            Bridge::Fun { dream, .. } => {
                ext_params.push(format!("p{i}: NativeCallback<{dream}>"));
                args.push(format!("NativeCallback<{dream}>({})", p.name));
            }
            // The shim takes `const T*` and copies; `@c` has no struct-by-value, so pass a copy by ref.
            Bridge::Struct(_) if !p.is_ref => {
                ext_params.push(format!("ref p{i}: {}", p.dream));
                let _ = writeln!(prelude, "{indent}    let __cpp_a{i} = {};", p.name);
                args.push(format!("ref __cpp_a{i}"));
            }
            _ => {
                ext_params.push(format!("{r}p{i}: {}", p.dream));
                args.push(format!("{r}{}", p.name));
            }
        }
    }
    let (value, is_result) = match &m.ret {
        Ret::Void => (None, false),
        Ret::Value(b) => (Some(b), false),
        Ret::Result(b) => (Some(b), true),
    };
    let returns_class = matches!(value, Some(Bridge::Class { .. }));
    if returns_class {
        ext_params.push("ref __cpp_owned: int".into());
        args.push("ref __cpp_owned".into());
    }
    if is_result {
        *uses_result = true;
        ext_params.push("ref __cpp_failed: int".into());
        args.push("ref __cpp_failed".into());
    }
    let ext_ret = match (m.kind, value) {
        (MemberKind::Constructor, _) => "CPtr".to_string(),
        (_, None) => "void".to_string(),
        (_, Some(b)) => extern_ret(b),
    };
    externs.push(format!(
        "@c(\"{}\", \"{}\") extern fun {}({}): {};",
        decls.set,
        m.symbol,
        m.symbol,
        ext_params.join(", "),
        ext_ret
    ));

    let vis = if m.internal { "internal" } else { "public" };
    let params: Vec<String> = m
        .params
        .iter()
        .map(|p| {
            let r = if p.is_ref { "ref " } else { "" };
            format!("{r}{}: {}", p.name, p.dream)
        })
        .collect();
    let params = params.join(", ");
    let _ = match m.kind {
        MemberKind::Constructor => writeln!(out, "{indent}{vis} constructor({params}) {{"),
        MemberKind::Static => writeln!(
            out,
            "{indent}{vis} static fun {}({params}): {} {{",
            m.name, m.ret_dream
        ),
        MemberKind::Instance | MemberKind::Free => writeln!(
            out,
            "{indent}{vis} fun {}({params}): {} {{",
            m.name, m.ret_dream
        ),
    };
    out.push_str(&prelude);
    let b = format!("{indent}    ");
    if returns_class {
        let _ = writeln!(out, "{b}let __cpp_owned: int = 0;");
    }
    if is_result {
        let _ = writeln!(out, "{b}let __cpp_failed: int = 0;");
    }
    let call = format!("{}({})", m.symbol, args.join(", "));
    match (m.kind, value) {
        (MemberKind::Constructor, _) => {
            let _ = write!(
                out,
                "{b}this.__cpp_handle = {call};\n\
                 {b}this.__cpp_owned = true;\n\
                 {b}this.__cpp_owner = Option.None;\n"
            );
        }
        (_, None) => {
            let _ = writeln!(out, "{b}{call};");
        }
        (_, Some(v)) => {
            let _ = writeln!(out, "{b}let __cpp_r = {call};");
            if is_result {
                let _ = writeln!(
                    out,
                    "{b}if (__cpp_failed != 0) {{\n{b}    return Result.Err({}());\n{b}}}",
                    last_error_fn(file_index)
                );
            }
            let result = match v {
                Bridge::Class { name, optional } => {
                    wrap_object(out, &b, name, *optional, m.kind == MemberKind::Instance);
                    "__cpp_v"
                }
                _ => "__cpp_r",
            };
            if is_result {
                let _ = writeln!(out, "{b}return Result.Ok({result});");
            } else {
                let _ = writeln!(out, "{b}return {result};");
            }
        }
    }
    let _ = writeln!(out, "{indent}}}");
    if class.is_none() {
        out.push('\n');
    }
}

/// Builds `__cpp_v` from the returned handle `__cpp_r`. A borrowed object keeps the object it
/// came from (`this`) alive, so a C++ reference cannot outlive its owner.
fn wrap_object(out: &mut String, b: &str, class: &str, optional: bool, from_instance: bool) {
    let _ = writeln!(out, "{b}let __cpp_o: Option<object> = Option.None;");
    if from_instance {
        let _ = writeln!(
            out,
            "{b}if (__cpp_owned == 0) {{\n{b}    __cpp_o = Option.Some(this);\n{b}}}"
        );
    }
    if optional {
        let _ = write!(
            out,
            "{b}let __cpp_v: Option<{class}> = Option.None;\n\
             {b}switch (__cpp_r) {{\n\
             {b}    Some(__cpp_p) => {{ __cpp_v = Option.Some({class}.__cpp_wrap(__cpp_p, __cpp_owned != 0, __cpp_o)); }},\n\
             {b}    None => {{}},\n\
             {b}}}\n"
        );
    } else {
        let _ = writeln!(
            out,
            "{b}let __cpp_v = {class}.__cpp_wrap(__cpp_r, __cpp_owned != 0, __cpp_o);"
        );
    }
}
