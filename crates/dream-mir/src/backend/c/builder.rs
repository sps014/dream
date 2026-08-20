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
            body: self.stmts,
        }
    }
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
