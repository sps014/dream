//! The analyzed `@cpp` declarations: classes, value structs, and free functions, grouped by the
//! native set that compiles their shim. Built from the parsed program before semantic analysis.

use std::collections::BTreeMap;
use std::rc::Rc;

use dream_abi::attributes::{
    ALLOW_CYCLE, AttributeTarget, OwnedResult, allows_cycle, cpp_attr, cpp_header, cpp_member_name,
    cpp_target_name, has_packed_attr, owned_result, validate_attributes,
};
use dream_abi::c_abi::cpp_shim_symbol;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{AttributeNode, FunctionNode, StructDeclarationNode, Type, Visibility};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_text::text_span::TextSpan;
use indexmap::IndexSet;

use super::types::{Known, Param, Ret, dream_type};
use crate::driver::native_sets::NativeGraph;
use crate::driver::source_loader::ProgramAccumulator;
use dream_types::CScalar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemberKind {
    Constructor,
    Instance,
    Static,
    Free,
}

#[derive(Debug, Clone)]
pub(super) struct Member {
    pub kind: MemberKind,
    pub name: String,
    /// What the shim calls: a member name, a qualified function, or a template instantiation.
    pub cpp: String,
    pub symbol: String,
    pub params: Vec<Param>,
    pub ret: Ret,
    pub ret_dream: String,
    /// `@owned`: a returned `T*` transfers ownership.
    pub owned: bool,
    pub internal: bool,
    pub line: usize,
    pub at: TextSpan,
}

#[derive(Debug, Clone)]
pub(super) struct Class {
    pub allow_cycle: bool,
    pub name: String,
    pub cpp: String,
    pub public: bool,
    pub line: usize,
    pub at: TextSpan,
    pub delete_symbol: String,
    pub members: Vec<Member>,
}

#[derive(Debug, Clone)]
pub(super) struct Struct {
    pub name: String,
    pub cpp: String,
    pub line: usize,
    pub packed: bool,
    pub fields: Vec<(String, CScalar)>,
}

/// Everything one source file declares with `@cpp`; the unit the desugarer replaces.
#[derive(Debug, Clone)]
pub(super) struct FileDecls {
    pub file: Rc<str>,
    pub set: String,
    pub classes: Vec<Class>,
    pub free: Vec<Member>,
}

/// One set's shim inputs.
#[derive(Debug, Default)]
pub(super) struct SetDecls {
    pub headers: IndexSet<String>,
    pub files: Vec<FileDecls>,
    pub structs: Vec<(Rc<str>, Struct)>,
}

#[derive(Debug, Default)]
pub(super) struct Program {
    pub known: Known,
    pub sets: BTreeMap<String, SetDecls>,
}

struct Ctx<'g> {
    graph: &'g NativeGraph,
    program: Program,
}

impl Ctx<'_> {
    fn set_for(
        &mut self,
        file: Option<&Rc<str>>,
        what: &str,
        at: &SyntaxToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<String> {
        let set = file.and_then(|f| self.graph.implicit_set_for_file(f));
        if set.is_none() {
            report(
                diagnostics,
                file,
                format!(
                    "`@cpp` on '{what}' needs C/C++ sources in a `native/` directory next to the \
                     declaring package's dream.toml"
                ),
                at,
            );
        }
        set.map(str::to_string)
    }

    fn header(&mut self, set: &str, attrs: &[AttributeNode]) {
        if let Some(h) = cpp_header(attrs).map(str::to_string) {
            self.program
                .sets
                .entry(set.to_string())
                .or_default()
                .headers
                .insert(h);
        }
    }
}

fn report(diagnostics: &mut DiagnosticBag, file: Option<&Rc<str>>, msg: String, at: &SyntaxToken) {
    let saved = diagnostics.file_path.clone();
    diagnostics.file_path = file.map(|f| f.to_string());
    diagnostics.report_error(msg, Some(at.position));
    diagnostics.file_path = saved;
}

/// Collects every `@cpp` declaration. Reports problems against the user's source; returns the
/// program model even when some declarations were rejected, since those are skipped.
pub(super) fn collect(
    acc: &ProgramAccumulator<'_>,
    graph: &NativeGraph,
    diagnostics: &mut DiagnosticBag,
) -> Program {
    let mut cx = Ctx {
        graph,
        program: Program::default(),
    };
    for s in &acc.all_structs {
        if cpp_attr(&s.attributes).is_none() {
            continue;
        }
        // Expansion removes the source declaration, so validate the permission before
        // replacing its attributes with the generated wrapper's bare annotation.
        let cycle_attributes: Vec<_> = s
            .attributes
            .iter()
            .filter(|attribute| attribute.name.text == ALLOW_CYCLE)
            .cloned()
            .collect();
        let saved_file = diagnostics.file_path.clone();
        diagnostics.file_path = s.file_path.as_ref().map(|file| file.to_string());
        validate_attributes(
            &cycle_attributes,
            if s.is_value {
                AttributeTarget::ValueStruct
            } else {
                AttributeTarget::Struct
            },
            diagnostics,
        );
        diagnostics.file_path = saved_file;
        let cpp =
            cpp_target_name(&s.attributes).map_or_else(|| s.name.text.clone(), str::to_string);
        if s.is_value {
            cx.program.known.structs.insert(s.name.text.clone(), cpp);
        } else {
            cx.program.known.classes.insert(s.name.text.clone(), cpp);
        }
    }
    for s in &acc.all_structs {
        if cpp_attr(&s.attributes).is_none() {
            continue;
        }
        let file = s.file_path.as_ref();
        let Some(set) = cx.set_for(file, &s.name.text, &s.name, diagnostics) else {
            continue;
        };
        cx.header(&set, &s.attributes);
        if s.is_value {
            if let Some(st) = value_struct(s, diagnostics) {
                let file = s.file_path.clone().unwrap_or_else(|| Rc::from(""));
                cx.program
                    .sets
                    .entry(set)
                    .or_default()
                    .structs
                    .push((file, st));
            }
        } else if let Some(class) = class(&cx.program.known, &set, s, diagnostics) {
            file_decls(&mut cx.program, &set, s.file_path.as_ref())
                .classes
                .push(class);
        }
    }
    let mut free_overloads: BTreeMap<(String, String), usize> = BTreeMap::new();
    for f in &acc.all_functions {
        if cpp_attr(&f.attributes).is_none() {
            continue;
        }
        let file = f.file_path.as_ref();
        let Some(set) = cx.set_for(file, &f.name.text, &f.name, diagnostics) else {
            continue;
        };
        cx.header(&set, &f.attributes);
        let n = free_overloads
            .entry((set.clone(), f.name.text.clone()))
            .or_insert(0);
        let overload = *n;
        *n += 1;
        let cpp = cpp_target_name(&f.attributes)
            .or_else(|| cpp_member_name(&f.attributes))
            .map(str::to_string)
            .unwrap_or_else(|| f.name.text.clone());
        if let Some(m) = member(
            &cx.program.known,
            &set,
            "",
            f,
            MemberKind::Free,
            cpp,
            overload,
            diagnostics,
        ) {
            file_decls(&mut cx.program, &set, f.file_path.as_ref())
                .free
                .push(m);
        }
    }
    cx.program
}

fn file_decls<'p>(
    program: &'p mut Program,
    set: &str,
    file: Option<&Rc<str>>,
) -> &'p mut FileDecls {
    let file = file.cloned().unwrap_or_else(|| Rc::from(""));
    let decls = &mut program.sets.entry(set.to_string()).or_default().files;
    let i = match decls.iter().position(|d| d.file == file) {
        Some(i) => i,
        None => {
            decls.push(FileDecls {
                file,
                set: set.to_string(),
                classes: Vec::new(),
                free: Vec::new(),
            });
            decls.len() - 1
        }
    };
    &mut decls[i]
}

fn value_struct(s: &StructDeclarationNode<'_>, diagnostics: &mut DiagnosticBag) -> Option<Struct> {
    let file = s.file_path.as_ref();
    let mut fields = Vec::with_capacity(s.fields.len());
    let mut ok = true;
    for f in &s.fields {
        match scalar_of(&f.field_type) {
            Some(sc) => fields.push((f.name.text.clone(), sc)),
            None => {
                report(
                    diagnostics,
                    file,
                    format!(
                        "`@cpp` struct field '{}' must be a number, `bool`, `char`, or `byte`",
                        f.name.text
                    ),
                    &f.name,
                );
                ok = false;
            }
        }
    }
    ok.then(|| Struct {
        name: s.name.text.clone(),
        cpp: cpp_target_name(&s.attributes).map_or_else(|| s.name.text.clone(), str::to_string),
        line: s.name.position.line_no,
        packed: has_packed_attr(&s.attributes),
        fields,
    })
}

fn scalar_of(ty: &Type) -> Option<CScalar> {
    let known = Known::default();
    match known.param(ty, false) {
        Ok(super::types::Bridge::Scalar(s)) => Some(s),
        _ => None,
    }
}

fn class(
    known: &Known,
    set: &str,
    s: &StructDeclarationNode<'_>,
    diagnostics: &mut DiagnosticBag,
) -> Option<Class> {
    let file = s.file_path.as_ref();
    let name = s.name.text.clone();
    if s.generic_parameters.is_some() {
        report(
            diagnostics,
            file,
            format!(
                "`@cpp` class '{name}' cannot be generic; bind each instantiation with its own class and `@cpp(\"h\", \"ns::T<int>\")`"
            ),
            &s.name,
        );
        return None;
    }
    let mut ok = true;
    for f in &s.fields {
        report(
            diagnostics,
            file,
            format!(
                "`@cpp` class '{name}' holds only `extern` members; keep Dream state in a \
                 wrapping class"
            ),
            &f.name,
        );
        ok = false;
    }
    let mut overloads: BTreeMap<String, usize> = BTreeMap::new();
    let mut members = Vec::new();
    for m in &s.methods {
        if !m.is_extern {
            report(
                diagnostics,
                file,
                format!(
                    "`@cpp` class '{name}' holds only `extern` members; put helpers such as \
                     '{}' in an `extend {name} {{ ... }}` block",
                    m.name.text
                ),
                &m.name,
            );
            ok = false;
            continue;
        }
        let kind = if m.name.text == dream_syntax::nodes::types::CONSTRUCTOR_NAME {
            MemberKind::Constructor
        } else if m.is_static {
            MemberKind::Static
        } else {
            MemberKind::Instance
        };
        let n = overloads.entry(m.name.text.clone()).or_insert(0);
        let overload = *n;
        *n += 1;
        let cpp =
            cpp_member_name(&m.attributes).map_or_else(|| m.name.text.clone(), str::to_string);
        match member(known, set, &name, m, kind, cpp, overload, diagnostics) {
            Some(mem) => members.push(mem),
            None => ok = false,
        }
    }
    ok.then(|| Class {
        allow_cycle: allows_cycle(&s.attributes),
        cpp: known
            .classes
            .get(&name)
            .cloned()
            .unwrap_or_else(|| name.clone()),
        public: s.visibility.is_public(),
        line: s.name.position.line_no,
        at: s.name.position,
        delete_symbol: cpp_shim_symbol(set, &name, "__delete", 0),
        name,
        members,
    })
}

#[allow(clippy::too_many_arguments)]
fn member(
    known: &Known,
    set: &str,
    class: &str,
    f: &FunctionNode<'_>,
    kind: MemberKind,
    cpp: String,
    overload: usize,
    diagnostics: &mut DiagnosticBag,
) -> Option<Member> {
    let file = f.file_path.as_ref();
    let name = f.name.text.clone();
    if !f.is_extern {
        report(
            diagnostics,
            file,
            format!("`@cpp` function '{name}' must be `extern`; the compiler writes its body"),
            &f.name,
        );
        return None;
    }
    let unsupported = if f.is_async {
        Some("`async`")
    } else if f.generic_parameters.is_some() {
        Some("generic; bind each instantiation with `@cpp_name(\"f<int>\")`")
    } else if f.operator_symbol.is_some() || f.indexer_kind.is_some() || f.accessor.is_some() {
        Some("an operator, indexer, or accessor; bind a named C++ function instead")
    } else {
        None
    };
    if let Some(why) = unsupported {
        report(
            diagnostics,
            file,
            format!("`@cpp` member '{name}' cannot be {why}"),
            &f.name,
        );
        return None;
    }
    if let OwnedResult::FreedBy(free) = owned_result(&f.attributes) {
        report(
            diagnostics,
            file,
            format!(
                "`@owned(\"{free}\")` names a C free function, which only applies to `@c` externs \
                 returning `OwnedCPtr`; a `@cpp` member takes bare `@owned`"
            ),
            &f.name,
        );
        return None;
    }
    let mut ok = true;
    let mut params = Vec::with_capacity(f.parameters.len());
    for p in &f.parameters {
        if p.default.is_some() || p.is_variadic {
            report(
                diagnostics,
                file,
                format!(
                    "`@cpp` parameter '{}' cannot have a default or be variadic; declare an \
                     overload with fewer parameters and C++ fills in its defaults",
                    p.name.text
                ),
                &p.name,
            );
            ok = false;
            continue;
        }
        match known.param(&p.type_, p.is_ref) {
            Ok(bridge) => params.push(Param {
                name: p.name.text.clone(),
                bridge,
                is_ref: p.is_ref,
                dream: dream_type(&p.type_),
            }),
            Err(msg) => {
                report(
                    diagnostics,
                    file,
                    format!("`@cpp` member '{name}' parameter '{}': {msg}", p.name.text),
                    &p.name,
                );
                ok = false;
            }
        }
    }
    let ret = if kind == MemberKind::Constructor {
        Ret::Void
    } else {
        match known.ret(f.return_type.as_ref()) {
            Ok(r) => r,
            Err(msg) => {
                report(
                    diagnostics,
                    file,
                    format!("`@cpp` member '{name}' result: {msg}"),
                    &f.name,
                );
                return None;
            }
        }
    };
    let symbol_member = if kind == MemberKind::Constructor {
        "__new"
    } else {
        name.as_str()
    };
    let symbol = cpp_shim_symbol(set, class, symbol_member, overload);
    ok.then(|| Member {
        kind,
        symbol,
        ret_dream: f
            .return_type
            .as_ref()
            .map(dream_type)
            .unwrap_or_else(|| "void".to_string()),
        owned: owned_result(&f.attributes).is_owned(),
        internal: f.visibility == Visibility::Internal,
        line: f.name.position.line_no,
        at: f.name.position,
        name,
        cpp,
        params,
        ret,
    })
}
