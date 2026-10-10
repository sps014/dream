//! Generated native shims. Every `@c` call goes through a C shim ([`c_shim`]) that clang compiles
//! for the build target, so the platform C ABI is clang's job, never hand-written lowering.
//!
//! `@cpp` bindings build on it. Hand-written declarations in a package's thin Dream wrapper become
//! (1) ordinary Dream classes whose members call `@c` externs, spliced into the program before
//! semantic analysis, and (2) a generated C++ shim per native set ([`cpp_shim`]) that implements
//! those externs against the real headers. Both shims spell scalars through
//! [`dream_types::CScalar`], so the C prototype one declares is the one the other defines. The
//! compiler never parses C++; the C++ compiler checks the declarations.

pub mod c_shim;
pub(crate) mod cpp_shim;
mod desugar;
mod model;
#[cfg(test)]
mod tests;
mod types;

use std::collections::BTreeMap;
use std::io::Error;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bumpalo::Bump;
use dream_abi::attributes::cpp_attr;
use dream_diagnostics::DiagnosticBag;
use dream_text::text_span::TextSpan;

use crate::driver::native_sets::NativeGraph;
use crate::driver::source_loader::{ProgramAccumulator, collect_declarations};

/// The generated shims, by native set name.
#[derive(Debug, Default)]
pub struct CppBridge {
    shims: BTreeMap<String, String>,
    origins: BTreeMap<String, CppOrigin>,
}

/// The `@cpp` declaration a generated `@c` extern stands for, so diagnostics about the extern name
/// what the author wrote.
#[derive(Debug, Clone)]
pub struct CppOrigin {
    /// `Class.member`, `Class` (its destructor), or a free function name.
    pub what: String,
    pub file: Rc<str>,
    pub at: TextSpan,
}

/// Where one set's shim was written: the source to compile and the directory holding
/// `dream_bridge.hpp`, which goes on the set's include path.
pub struct WrittenShim {
    pub source: PathBuf,
    pub include: PathBuf,
}

/// The runtime entry points generated shims call; kept exported from the linked program.
pub const SHIM_RUNTIME_EXPORTS: [&str; 2] = ["dream_callback_retain", "dream_callback_release"];

impl CppBridge {
    pub fn shim(&self, set: &str) -> Option<&str> {
        self.shims.get(set).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.shims.is_empty()
    }

    /// The declaration behind the generated extern `symbol`.
    pub fn origin(&self, symbol: &str) -> Option<&CppOrigin> {
        self.origins.get(symbol)
    }

    /// Writes the shim of every set `live` accepts, plus the bridge header, under
    /// `native_root/<set>/`. Unchanged files are left untouched so the object cache stays warm.
    pub fn write(
        &self,
        native_root: &Path,
        live: impl Fn(&str) -> bool,
    ) -> std::io::Result<BTreeMap<String, WrittenShim>> {
        let mut out = BTreeMap::new();
        for (set, text) in self.shims.iter().filter(|(s, _)| live(s)) {
            let dir = native_root.join(set);
            std::fs::create_dir_all(&dir)?;
            let source = dir.join("shim.cpp");
            write_if_changed(&source, text)?;
            write_if_changed(
                &dir.join(cpp_shim::BRIDGE_HEADER_NAME),
                cpp_shim::BRIDGE_HEADER,
            )?;
            out.insert(
                set.clone(),
                WrittenShim {
                    source,
                    include: dir,
                },
            );
        }
        Ok(out)
    }
}

/// Names the desugarer invents (hidden fields, helpers, the last-error extern) rather than
/// ones the author declared.
pub fn is_generated(name: &str) -> bool {
    name.starts_with("__cpp_")
}

fn write_if_changed(path: &Path, text: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    std::fs::write(path, text)
}

fn declares_cpp(acc: &ProgramAccumulator<'_>) -> bool {
    acc.all_structs
        .iter()
        .any(|s| cpp_attr(&s.attributes).is_some())
        || acc
            .all_functions
            .iter()
            .any(|f| cpp_attr(&f.attributes).is_some())
}

/// Replaces every `@cpp` class and free function with its generated Dream wrapper and builds the
/// shims. `@cpp` value structs stay as declared; their layout is checked in the shim.
pub fn expand<'a>(
    arena: &'a Bump,
    acc: &mut ProgramAccumulator<'a>,
    graph: &NativeGraph,
    diagnostics: &mut DiagnosticBag,
) -> Result<CppBridge, Error> {
    if !declares_cpp(acc) {
        return Ok(CppBridge::default());
    }
    let program = model::collect(acc, graph, diagnostics);
    acc.all_structs
        .retain(|s| s.is_value || cpp_attr(&s.attributes).is_none());
    acc.all_functions
        .retain(|f| cpp_attr(&f.attributes).is_none());
    acc.requested_std_packages.insert("system".to_string());

    let mut file_index = 0;
    for decls in program.sets.values() {
        for fd in &decls.files {
            let source = desugar::dream_source(fd, file_index);
            file_index += 1;
            parse_into(arena, acc, source, fd, diagnostics)?;
        }
    }
    let shims = program
        .sets
        .iter()
        .filter(|(_, d)| !d.files.is_empty() || !d.structs.is_empty())
        .map(|(set, d)| (set.clone(), cpp_shim::shim_source(set, d, &program.known)))
        .collect();
    Ok(CppBridge {
        shims,
        origins: origins(&program),
    })
}

fn origins(program: &model::Program) -> BTreeMap<String, CppOrigin> {
    let mut out = BTreeMap::new();
    for fd in program.sets.values().flat_map(|d| &d.files) {
        let origin = |what: String, at: TextSpan| CppOrigin {
            what,
            file: fd.file.clone(),
            at,
        };
        for c in &fd.classes {
            out.insert(c.delete_symbol.clone(), origin(c.name.clone(), c.at));
            for m in &c.members {
                out.insert(
                    m.symbol.clone(),
                    origin(format!("{}.{}", c.name, m.name), m.at),
                );
            }
        }
        for m in &fd.free {
            out.insert(m.symbol.clone(), origin(m.name.clone(), m.at));
        }
    }
    out
}

/// Parses the generated wrapper for `fd` and adds it to the program as declarations of the
/// author's own file, so file privacy and `module` membership are theirs. The names the author
/// wrote point back at their declarations for diagnostics and go-to-definition.
fn parse_into<'a>(
    arena: &'a Bump,
    acc: &mut ProgramAccumulator<'a>,
    source: String,
    fd: &model::FileDecls,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), Error> {
    use dream_syntax::lexer::Lexer;
    use dream_syntax::parser::Parser;

    let mut file_diagnostics = DiagnosticBag::new(Some(format!("{}#cpp", fd.file)));
    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer, arena, &mut file_diagnostics);
    let parsed = parser.parse();
    diagnostics.extend(&file_diagnostics);
    let ast = parsed?;
    let (first_struct, first_function) = (acc.all_structs.len(), acc.all_functions.len());
    collect_declarations(
        ast.get_root(),
        &fd.file,
        &mut acc.all_functions,
        &mut acc.all_structs,
        &mut acc.all_interfaces,
        &mut acc.all_enums,
        &mut acc.all_extends,
        &mut acc.all_globals,
    );
    for s in &mut acc.all_structs[first_struct..] {
        let Some(class) = fd.classes.iter().find(|c| c.name == s.name.text) else {
            continue;
        };
        s.name.position = class.at;
        point_at(&mut s.methods, &class.members);
    }
    point_at(&mut acc.all_functions[first_function..], &fd.free);
    Ok(())
}

/// Gives each generated function named like a declared member that member's position, pairing
/// overloads in declaration order.
fn point_at(functions: &mut [dream_syntax::nodes::FunctionNode<'_>], members: &[model::Member]) {
    let mut used = vec![false; members.len()];
    for f in functions {
        if f.parameters
            .first()
            .is_some_and(|p| is_generated(&p.name.text))
        {
            continue;
        }
        let found = members
            .iter()
            .enumerate()
            .find(|(i, m)| !used[*i] && generated_name(m) == f.name.text);
        if let Some((i, m)) = found {
            used[i] = true;
            f.name.position = m.at;
        }
    }
}

fn generated_name(m: &model::Member) -> &str {
    match m.kind {
        model::MemberKind::Constructor => dream_syntax::nodes::types::CONSTRUCTOR_NAME,
        _ => &m.name,
    }
}
