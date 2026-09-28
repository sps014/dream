//! Textual LLVM IR printer. No LLVM library dependency: every other LLVM writer builds on
//! these types, and the pinned `llvm-as`/`opt` toolchain parses the result.

pub mod attrs;
pub mod fmt;
pub mod function;
pub mod metadata;
pub mod module;
pub mod ty;
pub mod value;

pub use attrs::{CallConv, FnAttr, Linkage, ParamAttr};
pub use function::{BlockRef, CallArg, FunctionWriter, Tail};
pub use metadata::{MdRef, Metadata};
pub use module::{Decl, GlobalDef, ModuleWriter};
pub use ty::{FnTy, Ty};
pub use value::{Repr, Value};

/// `llvm-as` from `DREAM_LLVM` or `~/.dream/toolchains/llvm-*`, for tests that check printed IR
/// really parses. `None` when no toolchain is installed; those tests then only check text.
#[cfg(test)]
pub(crate) fn test_llvm_tool(name: &str) -> Option<std::path::PathBuf> {
    let mut bins = Vec::new();
    if let Ok(v) = std::env::var("DREAM_LLVM") {
        bins.push(std::path::PathBuf::from(&v));
        bins.push(std::path::PathBuf::from(v).join("bin"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let tc = std::path::PathBuf::from(home).join(".dream/toolchains");
        if let Ok(rd) = std::fs::read_dir(&tc) {
            let mut dirs: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            dirs.sort();
            for d in dirs.into_iter().rev() {
                if d.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("llvm-"))
                {
                    bins.push(d.join("bin"));
                }
            }
        }
    }
    bins.into_iter().map(|b| b.join(name)).find(|p| p.is_file())
}

/// Runs `llvm-as` over `ir` and panics with the assembler's message when it rejects it.
#[cfg(test)]
pub(crate) fn assert_assembles(ir: &str) {
    use std::io::Write;
    let Some(as_) = test_llvm_tool("llvm-as") else {
        return;
    };
    let mut child = std::process::Command::new(as_)
        .args(["-", "-o", "/dev/null"])
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn llvm-as");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(ir.as_bytes())
        .expect("write IR");
    let out = child.wait_with_output().expect("llvm-as");
    assert!(
        out.status.success(),
        "llvm-as rejected IR:\n{}\n---\n{ir}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_module_assembles() {
        let mut m = ModuleWriter::new("t.dream", "", "");
        let fty = FnTy::new(Ty::I32, vec![Ty::I32]);
        m.declare(
            "ext",
            Decl {
                fty: fty.clone(),
                ret_attrs: vec![],
                param_attrs: vec![vec![ParamAttr::SignExt]],
                attrs: vec![FnAttr::NoUnwind],
                cc: CallConv::C,
            },
        );
        m.global(
            "g",
            GlobalDef {
                linkage: Linkage::Internal,
                thread_local: true,
                constant: false,
                unnamed_addr: false,
                ty: Ty::I64,
                init: Some("0".into()),
                align: 8,
            },
        );
        let mut f = FunctionWriter::new("f", Ty::I32, vec![(Ty::I32, vec![])]);
        let loop_ = f.new_block("loop");
        let done = f.new_block("done");
        let slot = f.alloca(Ty::I32, 4);
        f.store(&f.param(0), &slot, 4, &[]);
        f.br(loop_);
        f.switch_to(loop_);
        let v = f.load(Ty::I32, &slot, 4, &[]);
        let r = f
            .call(
                Tail::None,
                CallConv::C,
                &fty,
                &Value::global("ext"),
                vec![CallArg {
                    value: v.clone(),
                    attrs: vec![ParamAttr::SignExt],
                }],
                &[],
            )
            .expect("value");
        let c = f.icmp("slt", &r, &Value::i32(10));
        let ptr = f.gep_const(&Value::global("g"), 4);
        let _ = f.load(Ty::I32, &ptr, 4, &[]);
        f.store(&r, &slot, 4, &[]);
        f.cond_br(&c, loop_, done);
        f.switch_to(done);
        let sw = f.new_block("sw");
        f.switch(&r, sw, &[(1, loop_), (2, sw)]);
        f.switch_to(sw);
        let d = f.cast("sitofp", &r, Ty::F64);
        let e = f.bin("fadd", &d, &Value::f64(0.5));
        let back = f.cast("fptosi", &e, Ty::I32);
        f.ret(Some(&back));
        m.define(f);
        assert_assembles(&m.finish());
    }
}
