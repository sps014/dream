//! DWARF through the printer's metadata: one compile unit, a subprogram for every function whose
//! MIR carries source lines, a location per `DebugLine`, and a variable per named local slot so
//! lldb shows Dream names and values (typed through `debug_views.rs`).

use super::fx::Fx;
use super::ir::{MdRef, Metadata};
use super::lcx::Lcx;
use crate::{MirFunction, Statement};
use indexmap::IndexSet;
use std::path::Path;

/// A string field of a specialized DI node: quoted like an MDString, without the `!`.
fn di_str(s: &str) -> String {
    Metadata::string(s)[1..].to_string()
}

fn first_line(f: &MirFunction) -> Option<u32> {
    f.blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .find_map(|s| match s {
            Statement::DebugLine(l) => Some(*l),
            _ => None,
        })
}

impl<'a> Lcx<'a> {
    fn di_file(&mut self, path: &str) -> MdRef {
        let on_disk = self.std_sources.as_ref().and_then(|dir| {
            path.strip_prefix(dream_stdlib::STD_PATH_PREFIX)
                .map(|rel| dir.join(rel))
        });
        let p = on_disk.as_deref().unwrap_or(Path::new(path));
        let name = p.file_name().map_or_else(
            || p.to_string_lossy().into_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let dir = p
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.m.md.node(format!(
            "!DIFile(filename: {}, directory: {})",
            di_str(&name),
            di_str(&dir)
        ))
    }

    fn di_unit(&mut self, file: MdRef) -> MdRef {
        if let Some(cu) = self.dbg_cu {
            return cu;
        }
        let md = &mut self.m.md;
        let cu = md.distinct(format!(
            "!DICompileUnit(language: DW_LANG_C99, file: {file}, producer: \"dream\", \
             isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug)"
        ));
        md.add_named("llvm.dbg.cu", cu);
        md.module_flag(7, "Dwarf Version", "i32 4");
        md.module_flag(2, "Debug Info Version", "i32 3");
        self.dbg_cu = Some(cu);
        cu
    }
}

impl<'l, 'a> Fx<'l, 'a> {
    /// Opens a subprogram named after `named` (the stub, for an async poll body) when the body
    /// carries source lines; the prologue is attributed to the first one.
    pub fn debug_begin(&mut self, named: &MirFunction) {
        let f = self.f;
        let Some(path) = f.file.as_deref().or(named.file.as_deref()) else {
            return;
        };
        let Some(line) = first_line(f) else {
            return;
        };
        let file = self.l.di_file(path);
        let cu = self.l.di_unit(file);
        let md = &mut self.l.m.md;
        let null = md.node("!{null}");
        let sty = md.node(format!("!DISubroutineType(types: {null})"));
        let none = md.node("!{}");
        let sp = md.distinct(format!(
            "!DISubprogram(name: {}, linkageName: {}, scope: {file}, file: {file}, line: {line}, \
             type: {sty}, scopeLine: {line}, spFlags: DISPFlagDefinition, unit: {cu}, \
             retainedNodes: {none})",
            di_str(&named.name),
            di_str(self.w.name()),
        ));
        self.w.subprogram = Some(sp);
        self.dbg = Some((file, line));
        self.set_line(line);
    }

    pub fn set_line(&mut self, line: u32) {
        let Some(sp) = self.w.subprogram else {
            return;
        };
        let loc = self
            .l
            .m
            .md
            .node(format!("!DILocation(line: {line}, column: 1, scope: {sp})"));
        self.w.set_loc(Some(loc));
    }

    /// `#dbg_declare` for every named local slot. `args` numbers the params as arguments, which
    /// only holds when the function's own parameters are the MIR params (not a poll's frame).
    pub fn debug_locals(&mut self, args: bool) {
        let (Some(sp), Some((file, line)), Some(loc)) = (self.w.subprogram, self.dbg, self.w.loc())
        else {
            return;
        };
        let f = self.f;
        let mut used: IndexSet<String> = IndexSet::new();
        for (i, decl) in f.locals.iter().enumerate() {
            let (Some(name), Some(slot)) = (decl.name.as_deref(), self.slots[i].as_ref()) else {
                continue;
            };
            let mut shown = name.to_string();
            let mut k = 2;
            while used.contains(&shown) {
                shown = format!("{name}_{k}");
                k += 1;
            }
            used.insert(shown.clone());
            let ptr = slot.ptr.typed();
            let ty = self.l.di_local_ty(decl.ty);
            let arg = match f.params.iter().position(|p| p.0 as usize == i) {
                Some(k) if args => format!(", arg: {}", k + 1),
                _ => String::new(),
            };
            let var = self.l.m.md.node(format!(
                "!DILocalVariable(name: {}{arg}, scope: {sp}, file: {file}, line: {line}, type: {ty})",
                di_str(&shown)
            ));
            self.w.debug_record(format!(
                "#dbg_declare({ptr}, {var}, !DIExpression(), {loc})"
            ));
        }
    }
}
