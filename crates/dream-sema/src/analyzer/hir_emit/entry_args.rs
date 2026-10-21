use super::*;
use crate::entry::ENTRY_NAME;

impl<'a> Analyzer<'a> {
    /// `main(args: string[])` receives `System.args`: its parameter becomes a local that the body
    /// initializes first, so every target's process entry calls `main` without arguments.
    pub(in crate::analyzer) fn hir_bind_entry_args(
        &self,
        functions: &mut [HFunction],
        diagnostics: &mut DiagnosticBag,
    ) {
        let Ok(info) = self.function_info(ENTRY_NAME) else {
            return;
        };
        let def = info.identity.0;
        let Some(main) = functions
            .iter_mut()
            .find(|f| f.def == def && f.params.len() == 1)
        else {
            return;
        };
        let getter = self.type_ctx.resolved_type("System").and_then(|owner| {
            self.unique_method_def(owner, &super::super::getter_member_name("args"))
        });
        let Some(getter) = getter else {
            diagnostics.report_error(
                "'main(args: string[])' needs the System.args getter from system.core".to_string(),
                None,
            );
            return;
        };
        let param = main.params.remove(0);
        main.locals.push(HLocal {
            id: param.local,
            name: param.name,
            ty: param.ty,
        });
        let args = HExpr::new(
            param.ty,
            HExprKind::Call {
                callee: Callee {
                    def: getter,
                    instance: vec![],
                    ret: param.ty,
                    take_params: vec![],
                },
                args: vec![],
            },
        );
        main.body.insert(
            0,
            HStmt::Let {
                local: param.local,
                ty: param.ty,
                value: args,
            },
        );
    }
}
