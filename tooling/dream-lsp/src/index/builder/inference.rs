use super::*;

impl Builder {
    pub(crate) fn infer_type(&self, expr: &ExpressionNode, scope: usize) -> Option<String> {
        self.infer_type_with(expr, scope, &[])
    }

    pub(crate) fn infer_type_with(
        &self,
        expr: &ExpressionNode,
        scope: usize,
        extras: &[(String, String)],
    ) -> Option<String> {
        self.infer_type_internal(expr, scope, extras)
    }

    /// Async call sites produce `Future<T>` from a declared return `T`. Sync calls pass through.
    /// Free-function details look like `async fun f(): T`; methods like `async Gpu.try_init(): T`
    /// or `static async Owner.name(): T`.
    pub(crate) fn async_call_type(detail: &str, ret_ty: String) -> String {
        let is_async = detail.contains("async ") || detail.contains("async fun");
        if is_async && !ret_ty.starts_with("Future<") {
            format!("Future<{ret_ty}>")
        } else {
            ret_ty
        }
    }

    /// Resolves a field/method by receiver type prefix when known (mirrors Index::resolve_member).
    pub(crate) fn resolve_member_decl(
        &self,
        receiver_ty: Option<&str>,
        name: &str,
    ) -> Option<&Decl> {
        if let Some(ty) = receiver_ty {
            let base = type_base(ty);
            // Prefer detail that starts with `Owner.` / `static Owner.` / …
            return self.decls.iter().find(|d| {
                d.name == name
                    && matches!(d.kind, SymKind::Field | SymKind::Method)
                    && detail_belongs_to(&d.detail, base)
            });
        }
        self.decls
            .iter()
            .find(|d| d.name == name && matches!(d.kind, SymKind::Field | SymKind::Method))
    }

    pub(crate) fn method_param_names(
        &self,
        recv: &ExpressionNode,
        method: &str,
        scope: usize,
    ) -> Option<Vec<String>> {
        let key = self
            .receiver_type_of(recv, scope)
            .map(|ty| format!("{}.{}", type_base(&ty), method));
        if let Some(k) = &key {
            if let Some(params) = self.method_params.get(k) {
                return Some(params.clone());
            }
        }
        // Fallback: unique bare suffix match (last resort when receiver type unknown).
        let suffix = format!(".{method}");
        let mut matches = self
            .method_params
            .iter()
            .filter(|(k, _)| k.ends_with(&suffix));
        let first = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(first.1.clone())
    }

    pub(crate) fn receiver_type_of(&self, recv: &ExpressionNode, scope: usize) -> Option<String> {
        match recv {
            ExpressionNode::Identifier(id) => {
                // Bare type name used as static receiver (`ComputePass.dispatch`).
                if self.decls.iter().any(|d| {
                    d.name == id.text
                        && matches!(
                            d.kind,
                            SymKind::Class
                                | SymKind::Struct
                                | SymKind::Interface
                                | SymKind::Enum
                                | SymKind::Type
                        )
                }) {
                    Some(id.text.clone())
                } else {
                    self.infer_type(recv, scope)
                }
            }
            _ => self.infer_type(recv, scope),
        }
    }

    pub(crate) fn infer_type_internal(
        &self,
        expr: &ExpressionNode,
        scope: usize,
        extras: &[(String, String)],
    ) -> Option<String> {
        match expr {
            ExpressionNode::Literal(t) => Some(t.display_name()),
            ExpressionNode::SizeOf(_, _) => Some("int".to_string()),
            ExpressionNode::NameOf(_, _) | ExpressionNode::TypeOf(_, _) => {
                Some("string".to_string())
            }
            ExpressionNode::Cast(_, ty, _) => Some(ty.display_name()),
            ExpressionNode::IsExpression(_, _, _) => Some("bool".to_string()),
            ExpressionNode::Binary(left, op, right) => match op.kind {
                dream::syntax::token::token_kind::TokenKind::EqualEqualToken
                | dream::syntax::token::token_kind::TokenKind::NotEqualToken
                | dream::syntax::token::token_kind::TokenKind::GreaterThanToken
                | dream::syntax::token::token_kind::TokenKind::GreaterThanEqualToken
                | dream::syntax::token::token_kind::TokenKind::SmallerThanToken
                | dream::syntax::token::token_kind::TokenKind::SmallerThanEqualToken
                | dream::syntax::token::token_kind::TokenKind::AmpersandAmpersandToken
                | dream::syntax::token::token_kind::TokenKind::PipePipeToken => {
                    Some("bool".to_string())
                }
                // Arithmetic operators (`+ - * /`) yield the type of their left operand, mirroring
                // the compiler's `analyze_binary_expression` (result type = left operand type). This
                // is what makes `let a = c * 5` infer `int` for hover/inlay hints. Fall back to the
                // right operand when the left is unresolvable.
                dream::syntax::token::token_kind::TokenKind::PlusToken
                | dream::syntax::token::token_kind::TokenKind::MinusToken
                | dream::syntax::token::token_kind::TokenKind::StarToken
                | dream::syntax::token::token_kind::TokenKind::SlashToken => self
                    .infer_type_with(left, scope, extras)
                    .or_else(|| self.infer_type_with(right, scope, extras)),
                _ => None,
            },
            ExpressionNode::Identifier(token) => extras
                .iter()
                .find(|(n, _)| n == &token.text)
                .map(|(_, t)| t.clone())
                .or_else(|| {
                    self.resolve(&token.text, scope, token.position.start)
                        .and_then(|d| d.ty.clone())
                }),
            ExpressionNode::MemberAccess(recv, member) => {
                let receiver_ty = self.receiver_type_of(recv, scope);
                self.resolve_member_decl(receiver_ty.as_deref(), &member.text)
                    .and_then(|d| {
                        d.ty.clone().or_else(|| {
                            // Methods often only store the signature in `detail`
                            // (`static js.global(…): js`); recover the return type from there.
                            if d.kind == SymKind::Method {
                                d.detail
                                    .rfind(':')
                                    .map(|i| d.detail[i + 1..].trim().to_string())
                            } else {
                                None
                            }
                        })
                    })
            }
            ExpressionNode::FunctionCall(name, generic_args, _) => {
                self.resolve(&name.text, scope, name.position.start)
                    .and_then(|d| {
                        if matches!(d.kind, SymKind::Class | SymKind::Struct) {
                            // It's a constructor call (e.g. `Test("John", 20)`), so the type is the
                            // class/struct name itself, rendered with angle brackets when generic
                            // (`Box<int>`).
                            match generic_args {
                                Some(args) => {
                                    let args_str = args
                                        .iter()
                                        .map(|a| a.display_name())
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    Some(format!("{}<{}>", name.text, args_str))
                                }
                                None => Some(name.text.clone()),
                            }
                        } else {
                            // detail string usually looks like: fun(int, int): string
                            // or `async fun foo(): T` — async calls yield `Future<T>` until awaited.
                            if let Some(colon_idx) = d.detail.rfind(':') {
                                let mut ret_ty = d.detail[colon_idx + 1..].trim().to_string();
                                if let Some(args) = generic_args {
                                    if args.len() == 1 {
                                        let arg_type = args[0].display_name();
                                        ret_ty = ret_ty
                                            .replace("<T>", &format!("<{}>", arg_type))
                                            .replace(" T", &format!(" {}", arg_type))
                                            .replace("T ", &format!("{} ", arg_type));
                                        if ret_ty == "T" {
                                            ret_ty = arg_type.to_string();
                                        }
                                    }
                                }
                                Some(Self::async_call_type(&d.detail, ret_ty))
                            } else {
                                None
                            }
                        }
                    })
            }
            ExpressionNode::Call(callee, _, _) => {
                // Best-effort: if the callee is a fun-typed expression, we don't recover the return
                // type from the index heuristic; fall through to walking the callee alone.
                self.infer_type_with(callee, scope, extras)
            }
            ExpressionNode::MethodCall(recv, method, generic_args, args) => {
                self.infer_method_call_type(recv, method, generic_args, args, scope)
            }
            ExpressionNode::Parenthesized(_, inner) => self.infer_type_with(inner, scope, extras),
            ExpressionNode::Await(_, inner) => {
                // `.await` unwraps `Future<T>` → `T`. Async call inference wraps declared returns
                // as `Future<T>`, so bare `f()` and `f().await` stay distinct for member completion.
                let inner_ty = self.infer_type_with(inner, scope, extras)?;
                let unwrapped = inner_ty
                    .strip_prefix("Future<")
                    .and_then(|rest| rest.strip_suffix('>'))
                    .map(|t| t.to_string())
                    .unwrap_or(inner_ty);
                Some(unwrapped)
            }
            _ => None,
        }
    }
}
