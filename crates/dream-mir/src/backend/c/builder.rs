use super::ast::{CTy, Expr, Func, Item, Param, Stmt, Unit};
use super::print::print_unit;

pub struct FuncBuilder {
    pub attr: Option<&'static str>,
    pub export: Option<String>,
    pub static_: bool,
    pub ret: CTy,
    pub name: String,
    pub params: Vec<Param>,
    stmts: Vec<Stmt>,
    next_temp: u32,
}

impl FuncBuilder {
    pub fn new(ret: CTy, name: impl Into<String>) -> Self {
        Self {
            attr: None,
            export: None,
            static_: false,
            ret,
            name: name.into(),
            params: Vec::new(),
            stmts: Vec::new(),
            next_temp: 0,
        }
    }

    pub fn param(&mut self, ty: CTy, name: impl Into<String>) {
        self.params.push(Param {
            ty,
            name: name.into(),
        });
    }

    pub fn stmt(&mut self, s: Stmt) {
        self.stmts.push(s);
    }

    pub fn assign(&mut self, dest: Expr, src: Expr) {
        self.stmts.push(Stmt::assign(dest, src));
    }

    pub fn expr_stmt(&mut self, e: Expr) {
        self.stmts.push(Stmt::expr(e));
    }

    pub fn call(&mut self, name: impl Into<String>, args: Vec<Expr>) {
        self.stmts.push(Stmt::call(name, args));
    }

    pub fn ret(&mut self, e: Option<Expr>) {
        self.stmts.push(Stmt::Return(e));
    }

    pub fn goto(&mut self, label: impl Into<String>) {
        self.stmts.push(Stmt::Goto(label.into()));
    }

    pub fn label(&mut self, name: impl Into<String>) {
        self.stmts.push(Stmt::Label(name.into()));
    }

    /// Run `f` on a nested builder that shares temp numbering; returns its statements untidied.
    pub fn nested(&mut self, f: impl FnOnce(&mut FuncBuilder)) -> Vec<Stmt> {
        let mut inner = FuncBuilder {
            attr: None,
            export: None,
            static_: false,
            ret: CTy::Void,
            name: String::new(),
            params: Vec::new(),
            stmts: Vec::new(),
            next_temp: self.next_temp,
        };
        f(&mut inner);
        self.next_temp = inner.next_temp;
        inner.stmts
    }

    fn fresh_name(&mut self) -> String {
        let n = self.next_temp;
        self.next_temp += 1;
        format!("t{n}")
    }

    pub fn temp(&mut self, ty: CTy, init: Option<Expr>) -> Expr {
        let name = self.fresh_name();
        self.stmts.push(Stmt::decl(ty, name.clone(), init));
        Expr::id(name)
    }

    pub fn expr_block(&mut self, f: impl FnOnce(&mut FuncBuilder) -> Expr) -> Expr {
        let mut inner = FuncBuilder {
            attr: None,
            export: None,
            static_: false,
            ret: CTy::Void,
            name: String::new(),
            params: Vec::new(),
            stmts: Vec::new(),
            next_temp: self.next_temp,
        };
        let result = f(&mut inner);
        self.next_temp = inner.next_temp;
        if inner.stmts.is_empty() {
            result
        } else {
            Expr::Gnu {
                stmts: inner.stmts,
                result: Box::new(result),
            }
        }
    }

    pub fn finish(self) -> Func {
        Func {
            attr: self.attr,
            export: self.export,
            static_: self.static_,
            ret: self.ret,
            name: self.name,
            params: self.params,
            body: tidy_body(self.stmts),
        }
    }
}

/// Conservative dead-control-flow cleanup over an emitted body:
/// - statements after a `return` are dropped up to the next label (unreachable),
/// - labels nothing jumps to are erased (their bodies remain reachable in place).
///
/// Computed jumps (`&&label` tables) disable label erasure — their targets are
/// not visible as `goto`s.
fn tidy_body(stmts: Vec<Stmt>) -> Vec<Stmt> {
    let mut refs: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let jump_table = count_label_refs(&stmts, &mut refs);
    rewrite_vec(stmts, &refs, jump_table)
}

fn count_label_refs(stmts: &[Stmt], refs: &mut std::collections::HashMap<String, u32>) -> bool {
    let mut jump_table = false;
    for s in stmts {
        match s {
            Stmt::Goto(g) => *refs.entry(g.clone()).or_insert(0) += 1,
            Stmt::GotoIndirect(_) => jump_table = true,
            Stmt::Block(bs) => jump_table |= count_label_refs(bs, refs),
            Stmt::If { then_s, else_s, .. } => {
                jump_table |= count_label_refs(std::slice::from_ref(then_s.as_ref()), refs);
                if let Some(e) = else_s {
                    jump_table |= count_label_refs(std::slice::from_ref(e.as_ref()), refs);
                }
            }
            Stmt::Switch { arms, .. } => {
                for arm in arms {
                    jump_table |= count_label_refs(&arm.body, refs);
                }
            }
            Stmt::For {
                init, step, body, ..
            } => {
                jump_table |= count_label_refs(std::slice::from_ref(init.as_ref()), refs);
                jump_table |= count_label_refs(std::slice::from_ref(step.as_ref()), refs);
                jump_table |= count_label_refs(std::slice::from_ref(body.as_ref()), refs);
            }
            _ => {}
        }
    }
    jump_table
}

fn contains_label(s: &Stmt) -> bool {
    match s {
        Stmt::Label(_) => true,
        Stmt::Block(bs) => bs.iter().any(contains_label),
        Stmt::If { then_s, else_s, .. } => {
            contains_label(then_s) || else_s.as_ref().is_some_and(|e| contains_label(e))
        }
        Stmt::Switch { arms, .. } => arms.iter().any(|arm| arm.body.iter().any(contains_label)),
        Stmt::For {
            init, step, body, ..
        } => contains_label(init) || contains_label(step) || contains_label(body),
        _ => false,
    }
}

fn rewrite_vec(
    stmts: Vec<Stmt>,
    refs: &std::collections::HashMap<String, u32>,
    jump_table: bool,
) -> Vec<Stmt> {
    let mut out: Vec<Stmt> = Vec::with_capacity(stmts.len());
    let mut terminated = false;
    for s in stmts {
        if terminated && !contains_label(&s) {
            continue;
        }
        if terminated {
            if let Stmt::Label(_) = s {
                // fall through: reachable from elsewhere
            }
            terminated = false;
        }
        match s {
            Stmt::Label(l) if !jump_table && refs.get(&l).copied().unwrap_or(0) == 0 => {}
            Stmt::Return(e) => {
                terminated = true;
                out.push(Stmt::Return(e));
            }
            Stmt::Block(inner) => out.push(Stmt::Block(rewrite_vec(inner, refs, jump_table))),
            Stmt::If {
                cond,
                then_s,
                else_s,
            } => out.push(Stmt::If {
                cond,
                then_s: Box::new(rewrite_owned(*then_s, refs, jump_table)),
                else_s: else_s.map(|e| Box::new(rewrite_owned(*e, refs, jump_table))),
            }),
            Stmt::Switch { expr, arms } => out.push(Stmt::Switch {
                expr,
                arms: arms
                    .into_iter()
                    .map(|mut arm| {
                        arm.body = rewrite_vec(std::mem::take(&mut arm.body), refs, jump_table);
                        arm
                    })
                    .collect(),
            }),
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => out.push(Stmt::For {
                init: Box::new(rewrite_owned(*init, refs, jump_table)),
                cond,
                step: Box::new(rewrite_owned(*step, refs, jump_table)),
                body: Box::new(rewrite_owned(*body, refs, jump_table)),
            }),
            other => out.push(other),
        }
    }
    out
}

fn rewrite_owned(s: Stmt, refs: &std::collections::HashMap<String, u32>, jump_table: bool) -> Stmt {
    let mut v = rewrite_vec(vec![s], refs, jump_table);
    v.pop().unwrap_or_else(|| Stmt::Block(vec![]))
}

#[derive(Default)]
pub struct ModuleBuilder {
    items: Vec<Item>,
}

impl ModuleBuilder {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn include(&mut self, header: &'static str) {
        self.items.push(Item::Include(header));
    }

    pub fn push(&mut self, item: Item) {
        self.items.push(item);
    }

    pub fn proto(&mut self, ret: CTy, name: impl Into<String>, params: Vec<Param>) {
        self.items.push(Item::Proto {
            static_: false,
            ret,
            name: name.into(),
            params,
            import: None,
            export: None,
        });
    }

    pub fn import_proto(
        &mut self,
        ret: CTy,
        name: impl Into<String>,
        params: Vec<Param>,
        module: impl Into<String>,
        import_name: impl Into<String>,
    ) {
        self.items.push(Item::Proto {
            static_: false,
            ret,
            name: name.into(),
            params,
            import: Some((module.into(), import_name.into())),
            export: None,
        });
    }

    pub fn static_proto(&mut self, ret: CTy, name: impl Into<String>, params: Vec<Param>) {
        self.items.push(Item::Proto {
            static_: true,
            ret,
            name: name.into(),
            params,
            import: None,
            export: None,
        });
    }

    pub fn push_func(&mut self, f: FuncBuilder) {
        self.items.push(Item::Func(f.finish()));
    }

    pub fn finish(self) -> String {
        print_unit(&Unit { items: self.items })
    }
}
