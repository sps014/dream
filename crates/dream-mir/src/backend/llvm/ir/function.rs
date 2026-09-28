//! One function body under construction.
//!
//! Every `alloca` is hoisted into the synthetic `entry` block (LLVM's mem2reg/SROA only promote
//! entry-block allocas), and `entry` can never be a branch target, so lowering code is free to
//! jump back to its own first block.

use super::attrs::{join, CallConv, FnAttr, Linkage, ParamAttr};
use super::metadata::MdRef;
use super::ty::{FnTy, Ty};
use super::value::{Repr, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockRef(u32);

impl BlockRef {
    pub const ENTRY: BlockRef = BlockRef(0);
}

struct Block {
    label: String,
    body: Vec<String>,
    terminated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tail {
    None,
    Tail,
    MustTail,
}

/// Metadata attachments for a single memory instruction (`!tbaa`, `!range`, ...).
pub type MdAttach<'a> = &'a [(&'static str, MdRef)];

pub struct FunctionWriter {
    name: String,
    pub linkage: Linkage,
    pub cc: CallConv,
    pub ret: Ty,
    pub ret_attrs: Vec<ParamAttr>,
    params: Vec<(Ty, Vec<ParamAttr>)>,
    pub attrs: Vec<FnAttr>,
    pub subprogram: Option<MdRef>,
    loc: Option<MdRef>,
    allocas: Vec<String>,
    blocks: Vec<Block>,
    cur: usize,
    next_reg: u32,
}

pub struct CallArg {
    pub value: Value,
    pub attrs: Vec<ParamAttr>,
}

impl From<Value> for CallArg {
    fn from(value: Value) -> Self {
        CallArg {
            value,
            attrs: Vec::new(),
        }
    }
}

impl FunctionWriter {
    pub fn new(name: impl Into<String>, ret: Ty, params: Vec<(Ty, Vec<ParamAttr>)>) -> Self {
        Self {
            name: name.into(),
            linkage: Linkage::Internal,
            cc: CallConv::C,
            ret,
            ret_attrs: Vec::new(),
            params,
            attrs: vec![FnAttr::NoUnwind],
            subprogram: None,
            loc: None,
            allocas: Vec::new(),
            blocks: vec![Block {
                label: "entry".into(),
                body: Vec::new(),
                terminated: false,
            }],
            cur: 0,
            next_reg: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fn_ty(&self) -> FnTy {
        FnTy::new(
            self.ret.clone(),
            self.params.iter().map(|(t, _)| t.clone()).collect(),
        )
    }

    pub fn param(&self, i: usize) -> Value {
        Value::new(self.params[i].0.clone(), Repr::Arg(i as u32))
    }

    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    pub fn set_loc(&mut self, loc: Option<MdRef>) {
        self.loc = loc;
    }

    pub fn loc(&self) -> Option<MdRef> {
        self.loc
    }

    /// A debug record (`#dbg_declare(...)`); it carries its own location, so no `!dbg` suffix.
    pub fn debug_record(&mut self, text: impl Into<String>) {
        self.ensure_open();
        self.blocks[self.cur].body.push(text.into());
    }

    // ---- blocks -------------------------------------------------------------------------------

    pub fn new_block(&mut self, hint: &str) -> BlockRef {
        let n = self.blocks.len();
        self.blocks.push(Block {
            label: format!("{hint}{n}"),
            body: Vec::new(),
            terminated: false,
        });
        BlockRef(n as u32)
    }

    pub fn switch_to(&mut self, b: BlockRef) {
        self.cur = b.0 as usize;
    }

    pub fn current(&self) -> BlockRef {
        BlockRef(self.cur as u32)
    }

    pub fn is_terminated(&self) -> bool {
        self.blocks[self.cur].terminated
    }

    fn label(&self, b: BlockRef) -> String {
        assert!(b != BlockRef::ENTRY, "ICE: branch to the entry block");
        format!("%{}", super::fmt::ident(&self.blocks[b.0 as usize].label))
    }

    /// Code after a terminator (e.g. following a `noreturn` call) lands in a fresh block with no
    /// predecessors instead of corrupting the terminated one.
    fn ensure_open(&mut self) {
        if self.blocks[self.cur].terminated {
            let b = self.new_block("dead");
            self.switch_to(b);
        }
    }

    fn line(&mut self, mut text: String) {
        self.ensure_open();
        if let Some(loc) = self.loc {
            text.push_str(&format!(", !dbg {loc}"));
        }
        self.blocks[self.cur].body.push(text);
    }

    fn fresh(&mut self, ty: Ty) -> Value {
        let n = self.next_reg;
        self.next_reg += 1;
        Value::new(ty, Repr::Reg(n))
    }

    /// `%vN = <rhs>` for any instruction the typed helpers below do not cover.
    pub fn assign(&mut self, ty: Ty, rhs: impl AsRef<str>) -> Value {
        let v = self.fresh(ty);
        self.line(format!("{} = {}", v.operand(), rhs.as_ref()));
        v
    }

    /// A result-less instruction line.
    pub fn emit(&mut self, text: impl Into<String>) {
        self.line(text.into());
    }

    fn terminate(&mut self, text: String) {
        self.line(text);
        self.blocks[self.cur].terminated = true;
    }

    // ---- memory -------------------------------------------------------------------------------

    pub fn alloca(&mut self, ty: Ty, align: u32) -> Value {
        let v = self.fresh(Ty::Ptr);
        self.allocas
            .push(format!("{} = alloca {ty}, align {align}", v.operand()));
        v
    }

    pub fn load(&mut self, ty: Ty, ptr: &Value, align: u32, md: MdAttach<'_>) -> Value {
        let text = format!("load {ty}, {}, align {align}{}", ptr.typed(), md_suffix(md));
        self.assign(ty, text)
    }

    pub fn store(&mut self, v: &Value, ptr: &Value, align: u32, md: MdAttach<'_>) {
        self.emit(format!(
            "store {}, {}, align {align}{}",
            v.typed(),
            ptr.typed(),
            md_suffix(md)
        ));
    }

    /// Byte-offset address arithmetic (`(char *)p + off`). Deliberately not `inbounds`: MIR can
    /// form addresses off a null or one-past-the-end base that are never dereferenced.
    pub fn gep_i8(&mut self, base: &Value, off: &Value) -> Value {
        if off.const_int() == Some(0) {
            return base.clone();
        }
        self.assign(
            Ty::Ptr,
            format!("getelementptr i8, {}, {}", base.typed(), off.typed()),
        )
    }

    pub fn gep_const(&mut self, base: &Value, off: i64) -> Value {
        self.gep_i8(base, &Value::i64(off))
    }

    // ---- arithmetic ---------------------------------------------------------------------------

    /// `op` includes any flags (`add nsw`, `lshr exact`, `fadd`).
    pub fn bin(&mut self, op: &str, a: &Value, b: &Value) -> Value {
        let text = format!("{op} {}, {}", a.typed(), b.operand());
        self.assign(a.ty.clone(), text)
    }

    pub fn icmp(&mut self, pred: &str, a: &Value, b: &Value) -> Value {
        let text = format!("icmp {pred} {}, {}", a.typed(), b.operand());
        self.assign(Ty::I1, text)
    }

    pub fn fcmp(&mut self, pred: &str, a: &Value, b: &Value) -> Value {
        let text = format!("fcmp {pred} {}, {}", a.typed(), b.operand());
        self.assign(Ty::I1, text)
    }

    pub fn cast(&mut self, op: &str, v: &Value, to: Ty) -> Value {
        let text = format!("{op} {} to {to}", v.typed());
        self.assign(to, text)
    }

    pub fn select(&mut self, c: &Value, a: &Value, b: &Value) -> Value {
        let text = format!("select {}, {}, {}", c.typed(), a.typed(), b.typed());
        self.assign(a.ty.clone(), text)
    }

    /// Must be the first instruction of the current block.
    pub fn phi(&mut self, ty: Ty, incoming: &[(Value, BlockRef)]) -> Value {
        let arms = incoming
            .iter()
            .map(|(v, b)| format!("[ {}, {} ]", v.operand(), self.label(*b)))
            .collect::<Vec<_>>()
            .join(", ");
        self.assign(ty.clone(), format!("phi {ty} {arms}"))
    }

    pub fn extract(&mut self, agg: &Value, idx: u32, ty: Ty) -> Value {
        let text = format!("extractvalue {}, {idx}", agg.typed());
        self.assign(ty, text)
    }

    // ---- calls --------------------------------------------------------------------------------

    /// Calls `callee` (a symbol name or a pointer value) with type `fty`. Returns `None` for a
    /// `void` call.
    pub fn call(
        &mut self,
        tail: Tail,
        cc: CallConv,
        fty: &FnTy,
        callee: &Value,
        args: Vec<CallArg>,
        fn_attrs: &[FnAttr],
    ) -> Option<Value> {
        let tail = match tail {
            Tail::None => "",
            Tail::Tail => "tail ",
            Tail::MustTail => "musttail ",
        };
        let args = args
            .iter()
            .map(|a| format!("{}{} {}", a.value.ty, join(&a.attrs), a.value.operand()))
            .collect::<Vec<_>>()
            .join(", ");
        let callee_ty = if fty.varargs {
            fty.to_string()
        } else {
            fty.ret.to_string()
        };
        let text = format!(
            "{tail}call {}{callee_ty} {}({args}){}",
            cc.prefix(),
            callee.operand(),
            join(fn_attrs)
        );
        if fty.ret.is_void() {
            self.emit(text);
            None
        } else {
            Some(self.assign(fty.ret.clone(), text))
        }
    }

    // ---- terminators --------------------------------------------------------------------------

    pub fn br(&mut self, b: BlockRef) {
        let t = format!("br label {}", self.label(b));
        self.terminate(t);
    }

    pub fn cond_br(&mut self, c: &Value, t: BlockRef, e: BlockRef) {
        let text = format!(
            "br {}, label {}, label {}",
            c.typed(),
            self.label(t),
            self.label(e)
        );
        self.terminate(text);
    }

    pub fn switch(&mut self, v: &Value, default: BlockRef, arms: &[(i128, BlockRef)]) {
        let mut text = format!("switch {}, label {} [", v.typed(), self.label(default));
        for (k, b) in arms {
            text.push_str(&format!(" {} {k}, label {}", v.ty, self.label(*b)));
        }
        text.push_str(" ]");
        self.terminate(text);
    }

    pub fn ret(&mut self, v: Option<&Value>) {
        let text = match v {
            Some(v) => format!("ret {}", v.typed()),
            None => "ret void".into(),
        };
        self.terminate(text);
    }

    pub fn unreachable(&mut self) {
        self.terminate("unreachable".into());
    }

    // ---- output -------------------------------------------------------------------------------

    pub fn write(&self, out: &mut String) {
        let params = self
            .params
            .iter()
            .enumerate()
            .map(|(i, (t, a))| format!("{t}{} %a{i}", join(a)))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = if self.ret_attrs.is_empty() {
            self.ret.to_string()
        } else {
            format!("{} {}", join(&self.ret_attrs).trim_start(), self.ret)
        };
        out.push_str(&format!(
            "define {}{}{ret} {}({params}){}",
            self.linkage.prefix(),
            self.cc.prefix(),
            super::fmt::global(&self.name),
            join(&self.attrs)
        ));
        if let Some(sp) = self.subprogram {
            out.push_str(&format!(" !dbg {sp}"));
        }
        out.push_str(" {\n");
        for (i, b) in self.blocks.iter().enumerate() {
            assert!(
                b.terminated,
                "ICE: unterminated LLVM block {} in {}",
                b.label, self.name
            );
            out.push_str(&super::fmt::ident(&b.label));
            out.push_str(":\n");
            if i == 0 {
                for a in &self.allocas {
                    out.push_str("  ");
                    out.push_str(a);
                    out.push('\n');
                }
            }
            for l in &b.body {
                out.push_str("  ");
                out.push_str(l);
                out.push('\n');
            }
        }
        out.push_str("}\n\n");
    }
}

fn md_suffix(md: MdAttach<'_>) -> String {
    let mut s = String::new();
    for (k, r) in md {
        s.push_str(&format!(", !{k} {r}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocas_hoist_and_entry_is_never_a_target() {
        let mut f = FunctionWriter::new("f", Ty::I32, vec![(Ty::I32, vec![ParamAttr::NoUndef])]);
        let body = f.new_block("bb");
        f.br(body);
        f.switch_to(body);
        let slot = f.alloca(Ty::I32, 4);
        f.store(&f.param(0), &slot, 4, &[]);
        let v = f.load(Ty::I32, &slot, 4, &[]);
        let one = Value::i32(1);
        let s = f.bin("add", &v, &one);
        f.ret(Some(&s));
        let mut out = String::new();
        f.write(&mut out);
        assert_eq!(
            out,
            "define internal i32 @f(i32 noundef %a0) nounwind {\n\
             entry:\n  %v0 = alloca i32, align 4\n  br label %bb1\n\
             bb1:\n  store i32 %a0, ptr %v0, align 4\n  %v1 = load i32, ptr %v0, align 4\n  \
             %v2 = add i32 %v1, 1\n  ret i32 %v2\n}\n\n"
        );
    }

    #[test]
    fn code_after_a_terminator_opens_a_dead_block() {
        let mut f = FunctionWriter::new("g", Ty::Void, vec![]);
        f.unreachable();
        f.ret(None);
        let mut out = String::new();
        f.write(&mut out);
        assert!(out.contains("dead1:\n  ret void"));
    }
}
