use super::*;

impl<'a> Analyzer<'a> {
    /// The declared module path of `file`, or `None` for a file with no `module` declaration (the
    /// implicit root module). Two `None`s are *not* automatically "the same module" by identity —
    /// callers compare `(file, module_of(file))` pairs (see [`Self::same_module`]) so unmoded files
    /// keep today's plain "same file" rule instead of becoming one giant shared "root module".
    pub(in crate::analyzer) fn module_of(&self, file: Option<&Rc<str>>) -> Option<Rc<str>> {
        file.and_then(|f| self.file_modules.get(f).cloned())
    }

    /// True when `decl_file` and `caller_file` share a *declared* module (both files wrote the same
    /// `module a.b.c;`). Deliberately `false` whenever either side has no declared module — an
    /// `internal` declaration in an unmoded file is only visible from its own file, exactly like
    /// today's file-private default, rather than being implicitly shared by every other unmoded
    /// file in the program.
    pub(in crate::analyzer) fn same_module(
        &self,
        decl_file: Option<&Rc<str>>,
        caller_file: Option<&Rc<str>>,
    ) -> bool {
        match (self.module_of(decl_file), self.module_of(caller_file)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// File/module-level visibility test (Axis 1). `Public` is visible everywhere. `Internal` is
    /// visible from the declaring file itself, or from any other file that declares the *same*
    /// `module` path. Private (the default) is only visible from the declaring file. Synthesized
    /// declarations (no declaring file) and use sites with no known file are always treated as
    /// visible.
    pub(in crate::analyzer) fn visible_across_files(
        &self,
        decl_file: &Option<Rc<str>>,
        visibility: dream_syntax::nodes::Visibility,
        caller_file: Option<&Rc<str>>,
    ) -> bool {
        use dream_syntax::nodes::Visibility;
        if visibility == Visibility::Public {
            return true;
        }
        match (decl_file, caller_file) {
            (Some(decl), Some(caller)) => {
                decl.as_ref() == caller.as_ref()
                    || (visibility == Visibility::Internal
                        && self.same_module(Some(decl), Some(caller)))
            }
            _ => true,
        }
    }

    /// Class-member visibility test (Axis 2). Always visible from the declaring type's own methods
    /// (`in_declaring_type`) or when `Public`. Otherwise `Internal` is visible from any file that
    /// declares the same `module` as the member's declaring file; private (the default) is not
    /// visible outside the declaring type at all, regardless of file/module.
    pub(in crate::analyzer) fn member_accessible(
        &self,
        visibility: dream_syntax::nodes::Visibility,
        decl_file: &Option<Rc<str>>,
        caller_file: Option<&Rc<str>>,
        in_declaring_type: bool,
    ) -> bool {
        use dream_syntax::nodes::Visibility;
        if in_declaring_type || visibility == Visibility::Public {
            return true;
        }
        visibility == Visibility::Internal && self.same_module(decl_file.as_ref(), caller_file)
    }

    /// Reports a cross-file visibility violation for a top-level declaration referenced from
    /// another file without being `public`.
    pub(in crate::analyzer) fn report_not_public(
        &self,
        kind: &str,
        name: &str,
        decl_file: &Option<Rc<str>>,
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        let where_ = decl_file
            .as_ref()
            .map(|f| format!(" (declared in '{}')", f))
            .unwrap_or_default();
        diagnostics.report_error(
            format!(
                "{} '{}' is not 'public'; it is private to its file{} and cannot be used from another file",
                kind, name, where_
            ),
            Some(position),
        );
    }

    /// Checks that a referenced enum/interface type is visible from `caller_file`, reporting an
    /// error otherwise. Types absent from `type_visibility` (structs/classes, primitives, generics,
    /// synthesized types) are handled elsewhere or always visible here.
    pub(in crate::analyzer) fn check_type_visible(
        &self,
        type_name: &str,
        caller_file: Option<&Rc<str>>,
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if let Some((decl_file, visibility)) = self.type_visibility.get(type_name) {
            if !self.visible_across_files(decl_file, *visibility, caller_file) {
                self.report_not_public("Type", type_name, decl_file, position, diagnostics);
            }
        }
    }
}
