use super::*;

impl Index {
    /// Completion proposals at `offset`. After a `.` we attempt member completion against the
    /// receiver's resolved struct type, falling back to all members when the type is unknown.
    pub fn completions(
        &self,
        file_path: Option<&str>,
        text: &str,
        offset: usize,
    ) -> Vec<(String, SymKind, String, Option<String>)> {
        let scope = self.enclosing_scope(offset);
        let bytes = text.as_bytes();

        // Unquoted `import system…` package paths — must run before member `.` so
        // `import system.` is not treated as a variable member access. Expression
        // `System.` still falls through to member_completions below.
        if let Some((_path_start, partial)) = import_path_partial(text, offset) {
            return import_path_completions(file_path, text, &partial);
        }

        // `@name` / `@partial` attribute-name completion (before any `.` / keyword dump).
        if let Some((_name_start, partial)) = attribute_name_partial(text, offset) {
            let mut out: Vec<_> = attribute_name_completions(&partial)
                .into_iter()
                .map(|(label, _insert, detail, doc)| (label, SymKind::Decorator, detail, doc))
                .collect();
            let partial = partial.to_lowercase();
            for attr in self.declared_attributes() {
                if attr.name.to_lowercase().starts_with(&partial)
                    && !out.iter().any(|(n, ..)| *n == attr.name)
                {
                    out.push((
                        attr.name.clone(),
                        SymKind::Decorator,
                        attr.signature.clone(),
                        Some(attr.hover()),
                    ));
                }
            }
            return out;
        }

        // Inside `@name(...)` — closed-world arg suggestions when the registry knows them.
        // Always stay in attribute-arg mode (never dump keywords / globals into the arg list).
        if let Some(ctx) = attribute_arg_context(text, offset) {
            return attribute_arg_completions(&ctx)
                .into_iter()
                .map(|(label, _insert, detail, doc)| (label, SymKind::Decorator, detail, doc))
                .collect();
        }

        // Detect `receiver.<partial>` before switch-arm so `case Color.|` uses member
        // completions (bare `Red`) instead of qualified `Color.Red` labels.
        let mut i = offset;
        while i > 0 && is_ident_byte(bytes[i - 1]) {
            i -= 1;
        }
        if i > 0 && bytes[i - 1] == b'.' {
            let mut j = i - 1;
            while j > 0 && bytes[j - 1] == b' ' {
                j -= 1;
            }
            let recv_end = j;
            let mut recv_start = recv_end;
            while recv_start > 0 && is_ident_byte(bytes[recv_start - 1]) {
                recv_start -= 1;
            }
            let receiver = &text[recv_start..recv_end];

            // `Type.staticMember.` — e.g. `js.global.` — complete against the static
            // member's return type (instance helpers on `js`), not the bare identifier.
            if recv_start > 0 && bytes[recv_start - 1] == b'.' {
                let mut k = recv_start - 1;
                while k > 0 && bytes[k - 1] == b' ' {
                    k -= 1;
                }
                let outer_end = k;
                let mut outer_start = outer_end;
                while outer_start > 0 && is_ident_byte(bytes[outer_start - 1]) {
                    outer_start -= 1;
                }
                let outer = &text[outer_start..outer_end];
                if let Some(ret_ty) = self.static_member_return_type(outer, receiver) {
                    return self.members_of_struct(&ret_ty, /*static_only*/ false);
                }
            }

            return self.member_completions(receiver, scope, recv_start);
        }

        // Pattern-matching / `case` switch arms: suggest variants of the subject enum/union.
        if let Some(subject) = switch_arm_subject(text, offset) {
            return self.switch_arm_completions(&subject, scope, offset, text, offset);
        }

        let mut out = Vec::new();
        for kw in keywords() {
            out.push((
                kw.to_string(),
                SymKind::Keyword,
                "keyword".to_string(),
                None,
            ));
        }
        for d in &self.decls {
            match d.kind {
                SymKind::Function
                | SymKind::Class
                | SymKind::Struct
                | SymKind::Interface
                | SymKind::Enum
                | SymKind::Type => {
                    out.push((
                        d.name.clone(),
                        d.kind,
                        d.detail.clone(),
                        d.doc_comment.clone(),
                    ));
                }
                // Top-level `let`/`const` globals are visible from every body.
                SymKind::Variable if d.scope == GLOBAL => {
                    out.push((
                        d.name.clone(),
                        d.kind,
                        d.detail.clone(),
                        d.doc_comment.clone(),
                    ));
                }
                SymKind::Variable | SymKind::Param if d.scope == scope && d.start <= offset => {
                    out.push((
                        d.name.clone(),
                        d.kind,
                        d.detail.clone(),
                        d.doc_comment.clone(),
                    ));
                }
                _ => {}
            }
        }
        out
    }

    /// Return type of a static method `Type.member` when both are indexed (e.g. `js.global` → `js`).
    pub(crate) fn static_member_return_type(
        &self,
        type_name: &str,
        member: &str,
    ) -> Option<String> {
        let is_type = self.decls.iter().any(|d| {
            d.name == type_name
                && matches!(
                    d.kind,
                    SymKind::Class
                        | SymKind::Struct
                        | SymKind::Interface
                        | SymKind::Enum
                        | SymKind::Type
                )
        });
        if !is_type {
            return None;
        }
        let detail = self
            .decls
            .iter()
            .find(|d| {
                d.kind == SymKind::Method
                    && d.name == member
                    && detail_is_static_method(&d.detail)
                    && detail_belongs_to(&d.detail, type_name)
            })
            .map(|d| d.detail.as_str())?;
        detail
            .rfind(':')
            .map(|i| detail[i + 1..].trim().to_string())
    }

    /// Members available on `receiver`, resolved by type. If `receiver` is a variable/parameter
    /// (including `this`) whose type is a known struct, only that struct's fields and methods are
    /// offered. If `receiver` names an enum type (and is not a shadowed local), its variants and
    /// static methods are offered. A bare class/struct name only offers **static** methods.
    pub(crate) fn member_completions(
        &self,
        receiver: &str,
        scope: usize,
        before: usize,
    ) -> Vec<(String, SymKind, String, Option<String>)> {
        // Locals / params win over a same-named enum type (`let Color = …; Color.`).
        if let Some(decl) = self.resolve(receiver, scope, before)
            && matches!(decl.kind, SymKind::Variable | SymKind::Param) {
                return match &decl.ty {
                    Some(ty) => {
                        let base = ty.trim_end_matches('?').trim_end_matches("[]");
                        self.members_of_struct(base, /*static_only*/ false)
                    }
                    // In-scope local with unknown type: never fall through to the enum type.
                    None => Vec::new(),
                };
            }

        if self
            .decls
            .iter()
            .any(|d| d.kind == SymKind::Enum && d.name == receiver)
        {
            return self.members_of_enum_type(receiver);
        }

        // A bare class/struct/interface/type name used as a receiver (e.g. static `Point.` / `js.`).
        if self.decls.iter().any(|d| {
            matches!(
                d.kind,
                SymKind::Class | SymKind::Struct | SymKind::Interface | SymKind::Type
            ) && d.name == receiver
        }) {
            return self.members_of_struct(receiver, /*static_only*/ true);
        }

        Vec::new()
    }

    pub(crate) fn members_of_struct(
        &self,
        ty: &str,
        static_only: bool,
    ) -> Vec<(String, SymKind, String, Option<String>)> {
        // `ty` may carry generic arguments (`Box<int>`); members are registered under the bare
        // struct name (`Box.value`), so match on that while keeping the full type for argument
        // substitution in member signatures.
        let base = type_base(ty);
        self.decls
            .iter()
            .filter(|d| {
                matches!(d.kind, SymKind::Field | SymKind::Method)
                    && d.scope == GLOBAL
                    && detail_belongs_to(&d.detail, base)
                    && d.name != CONSTRUCTOR_NAME
                    && if static_only {
                        // Type-name access: only static methods (no fields / instance methods).
                        d.kind == SymKind::Method && detail_is_static_method(&d.detail)
                    } else {
                        // Value receiver: fields + instance methods (not static).
                        d.kind == SymKind::Field
                            || (d.kind == SymKind::Method && !detail_is_static_method(&d.detail))
                    }
            })
            .map(|d| {
                let detail = Self::substitute_generic(&d.detail, ty);
                (d.name.clone(), d.kind, detail, d.doc_comment.clone())
            })
            .collect()
    }

    pub(crate) fn members_of_enum(
        &self,
        name: &str,
    ) -> Vec<(String, SymKind, String, Option<String>)> {
        let prefix = format!("{}.", name);
        self.decls
            .iter()
            .filter(|d| d.kind == SymKind::EnumMember && d.detail.starts_with(&prefix))
            .map(|d| {
                (
                    d.name.clone(),
                    d.kind,
                    d.detail.clone(),
                    d.doc_comment.clone(),
                )
            })
            .collect()
    }

    /// Variants plus static methods on a bare enum type name (`Color.` / `Option.`).
    pub(crate) fn members_of_enum_type(
        &self,
        name: &str,
    ) -> Vec<(String, SymKind, String, Option<String>)> {
        let mut out = self.members_of_enum(name);
        out.extend(
            self.decls
                .iter()
                .filter(|d| {
                    d.kind == SymKind::Method
                        && d.scope == GLOBAL
                        && detail_belongs_to(&d.detail, name)
                        && detail_is_static_method(&d.detail)
                })
                .map(|d| {
                    (
                        d.name.clone(),
                        d.kind,
                        d.detail.clone(),
                        d.doc_comment.clone(),
                    )
                }),
        );
        out
    }

    /// Variants for a switch arm, filtered by any partial identifier already typed.
    pub(crate) fn switch_arm_completions(
        &self,
        subject: &str,
        scope: usize,
        before: usize,
        text: &str,
        offset: usize,
    ) -> Vec<(String, SymKind, String, Option<String>)> {
        let Some(enum_name) = self.switch_subject_enum_name(subject, scope, before) else {
            return Vec::new();
        };
        let mut out = self.members_of_enum(&enum_name);
        // C-style `case Color.|` is handled by member completion; after bare `case ` offer
        // qualified `Enum.Variant` labels so integer enums match documented syntax.
        if switch_arm_is_c_style_case(text, offset)
            && self
                .decls
                .iter()
                .any(|d| d.kind == SymKind::Enum && d.name == enum_name)
        {
            // Prefer qualified labels when the enum looks like a plain int enum (no payload
            // variants in detail). Payload unions keep bare `Ok` / `Circle` names.
            let has_payload = out.iter().any(|(_, _, detail, _)| detail.contains('('));
            if !has_payload {
                out = out
                    .into_iter()
                    .map(|(name, kind, detail, doc)| {
                        (format!("{enum_name}.{name}"), kind, detail, doc)
                    })
                    .collect();
            }
        }
        let partial = partial_ident_before(text, offset);
        if !partial.is_empty() {
            out.retain(|(name, ..)| {
                name.starts_with(&partial) || name.contains(&format!(".{partial}"))
            });
        }
        out
    }

    /// Resolve `switch (subject)` to the bare enum/union type name (`Result`, `Shape`, …).
    pub(crate) fn switch_subject_enum_name(
        &self,
        subject: &str,
        scope: usize,
        before: usize,
    ) -> Option<String> {
        let subject = subject.trim();
        // Prefer the variable/parameter type when the subject is an identifier.
        if subject
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            if let Some(ty) = self.variable_type(subject, scope, before) {
                let base = type_base(&ty).to_string();
                if self
                    .decls
                    .iter()
                    .any(|d| d.kind == SymKind::Enum && d.name == base)
                {
                    return Some(base);
                }
            }
            // Bare type name used as subject (unusual but valid).
            if self
                .decls
                .iter()
                .any(|d| d.kind == SymKind::Enum && d.name == subject)
            {
                return Some(subject.to_string());
            }
        }
        None
    }
}
