use super::*;

impl Index {
    pub(crate) fn span_at(start: usize, end: usize, offset: usize) -> bool {
        offset >= start && offset <= end
    }

    /// Returns the declaration whose name token is under `offset`, if any.
    pub(crate) fn decl_at(&self, offset: usize) -> Option<&Decl> {
        self.decls
            .iter()
            .find(|d| d.is_main && Self::span_at(d.start, d.end, offset))
    }

    /// Returns the reference whose name token is under `offset`, if any.
    pub(crate) fn ref_at(&self, offset: usize) -> Option<&Ref> {
        self.refs
            .iter()
            .find(|r| r.is_main && Self::span_at(r.start, r.end, offset))
    }

    /// Resolves a name used at `offset` within `scope` to its declaration. Locals (variables and
    /// parameters declared at or before the use site, in the same function) take precedence over
    /// file-scope declarations, approximating lexical scoping without block-level precision.
    pub(crate) fn resolve(&self, name: &str, scope: usize, before: usize) -> Option<&Decl> {
        let local = self
            .decls
            .iter()
            .filter(|d| {
                d.name == name
                    && d.scope == scope
                    && matches!(d.kind, SymKind::Variable | SymKind::Param)
                    && d.start <= before
            })
            .max_by_key(|d| d.start);
        if local.is_some() {
            return local;
        }
        // File-scope fallback: free functions, types, and top-level `let`/`const` globals (which
        // carry `scope == GLOBAL` and `SymKind::Variable`).
        self.decls.iter().find(|d| {
            d.name == name
                && d.scope == GLOBAL
                && matches!(
                    d.kind,
                    SymKind::Function
                        | SymKind::Class
                        | SymKind::Struct
                        | SymKind::Interface
                        | SymKind::Enum
                        | SymKind::Variable
                )
        })
    }

    /// Resolves a field or method named `name`. When `receiver_ty` is known, prefer the member
    /// whose `detail` is qualified by that type (`Owner.` / `static Owner.` / …) so same-named
    /// members on different types (e.g. `js.global` vs `Regex.global`) disambiguate. A known
    /// receiver that has no matching member returns `None` — never a random same-named field.
    pub(crate) fn resolve_member(&self, receiver_ty: Option<&str>, name: &str) -> Option<&Decl> {
        if let Some(ty) = receiver_ty {
            let base = type_base(ty);
            return self.decls.iter().find(|d| {
                d.name == name
                    && matches!(d.kind, SymKind::Field | SymKind::Method)
                    && detail_belongs_to(&d.detail, base)
            });
        }
        self.decls.iter().find(|d| {
            d.name == name
                && matches!(
                    d.kind,
                    SymKind::Field | SymKind::Method | SymKind::EnumMember
                )
        })
    }

    /// Type of a receiver identifier: variable/param type, or the bare type name itself when it
    /// names a class/struct/interface/enum/extend-target (static access `JobGroup.dispatch` /
    /// `js.global`).
    pub(crate) fn receiver_type_name(
        &self,
        receiver: &str,
        scope: usize,
        before: usize,
    ) -> Option<String> {
        if let Some(ty) = self.variable_type(receiver, scope, before) {
            return Some(ty);
        }
        if self.decls.iter().any(|d| {
            d.name == receiver
                && matches!(
                    d.kind,
                    SymKind::Class
                        | SymKind::Struct
                        | SymKind::Interface
                        | SymKind::Enum
                        | SymKind::Type
                )
        }) {
            return Some(receiver.to_string());
        }
        None
    }

    /// Resolves an enum variant reference. When the receiver (the `Enum` in `Enum.Variant`) is
    /// known, prefer the variant whose `detail` is qualified by that enum so look-alike variant
    /// names across different enums (e.g. `Some`/`None`) disambiguate; otherwise fall back to the
    /// first variant matching by name.
    pub(crate) fn resolve_enum_member(&self, receiver: Option<&str>, name: &str) -> Option<&Decl> {
        if let Some(recv) = receiver {
            let prefix = format!("{}.", recv);
            if let Some(d) = self.decls.iter().find(|d| {
                d.kind == SymKind::EnumMember && d.name == name && d.detail.starts_with(&prefix)
            }) {
                return Some(d);
            }
        }
        self.decls
            .iter()
            .find(|d| d.kind == SymKind::EnumMember && d.name == name)
    }

    pub(crate) fn substitute_generic(detail: &str, receiver_ty: &str) -> String {
        // `receiver_ty` is the human-readable type (e.g. `List<int>` / `List<float>`);
        // pull the generic argument out of the angle brackets.
        let mut generic_arg = None;
        if let Some(start) = receiver_ty.find('<')
            && let Some(end) = receiver_ty.rfind('>') {
                generic_arg = Some(&receiver_ty[start + 1..end]);
            }

        let Some(generic_arg) = generic_arg else {
            return detail.to_string();
        };

        substitute_type_param_t(detail, generic_arg)
    }

    /// Applies call-site / receiver type arguments to a method detail that still mentions `T`
    /// (e.g. `List.create<float>` → `List<float>`, `read_at(): T[]` → `float[]`).
    /// Method-level params (`dispatch<TIn, TOut>`) are handled only by
    /// [`substitute_method_type_args`] — never via class-`T` synthesis, which would turn
    /// `TIn` into `stringIn` when args are `int, string`.
    pub(crate) fn apply_type_args_to_detail(
        detail: &str,
        receiver_ty: Option<&str>,
        call_type_args: &[String],
    ) -> String {
        let mut out = detail.to_string();
        let method_has_type_params = method_detail_has_type_params(detail);
        // Prefer an already-concrete receiver (`List<float>`).
        if let Some(recv) = receiver_ty {
            if recv.contains('<') {
                out = Self::substitute_generic(&out, recv);
            } else if !call_type_args.is_empty() && !method_has_type_params {
                // Bare `List.create<float>(…)`: synthesize `List<float>`.
                let synthetic = format!("{}<{}>", type_base(recv), call_type_args.join(", "));
                out = Self::substitute_generic(&out, &synthetic);
            }
        } else if call_type_args.len() == 1 && !method_has_type_params {
            out = substitute_type_param_t(&out, &call_type_args[0]);
        }
        if !call_type_args.is_empty() {
            out = substitute_method_type_args(&out, call_type_args);
        }
        out
    }

    /// Type name of a variable/parameter named `name` visible at `before` within `scope`.
    pub(crate) fn variable_type(&self, name: &str, scope: usize, before: usize) -> Option<String> {
        self.resolve(name, scope, before).and_then(|d| d.ty.clone())
    }

    /// The function scope whose body span contains `offset`, or [`GLOBAL`].
    pub(crate) fn enclosing_scope(&self, offset: usize) -> usize {
        // Parameters/locals of a function share its scope id and are appended in source order,
        // so the latest local/param declared before `offset` identifies the enclosing function.
        let mut best: Option<(usize, usize)> = None; // (scope, name_start)
        for d in &self.decls {
            if matches!(d.kind, SymKind::Param | SymKind::Variable)
                && d.scope != GLOBAL
                && d.start <= offset
            {
                match best {
                    Some((_, s)) if s >= d.start => {}
                    _ => best = Some((d.scope, d.start)),
                }
            }
        }
        best.map(|(scope, _)| scope).unwrap_or(GLOBAL)
    }
}
