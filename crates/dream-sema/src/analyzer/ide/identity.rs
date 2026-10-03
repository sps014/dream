use super::{Analyzer, IdeSource, IdeTarget};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_types::{DefId, DefKind, TyKind};

impl Analyzer<'_> {
    pub(in crate::analyzer) fn record_ide_definition(
        &mut self,
        def: DefId,
        name: &SyntaxToken,
        file: Option<&str>,
    ) {
        if name.position.end > name.position.start {
            self.ide_sources.insert(
                def,
                IdeSource {
                    file: file.map(str::to_string),
                    start: name.position.start,
                    end: name.position.end,
                },
            );
        }
    }

    pub(in crate::analyzer) fn record_ide_member_definition(
        &mut self,
        def: DefId,
        name: &SyntaxToken,
        file: Option<&str>,
    ) {
        self.ide_member_sources.insert(
            (def, name.text.clone()),
            IdeSource {
                file: file.map(str::to_string),
                start: name.position.start,
                end: name.position.end,
            },
        );
    }

    pub(super) fn resolve_ide_target(&self, target: IdeTarget) -> IdeTarget {
        let nominal = |name: &str| {
            let ty = self.type_ctx.resolved_type(name)?;
            match self.type_ctx.interner.kind(ty) {
                TyKind::Struct(def, _)
                | TyKind::Interface(def, _)
                | TyKind::Union(def, _)
                | TyKind::Enum(def) => Some(*def),
                _ => None,
            }
        };
        let (def, member) = match &target {
            IdeTarget::Callee { key, .. } => (self.type_ctx.resolve(DefKind::Function, key), None),
            IdeTarget::Constructor { type_key } => (nominal(type_key), None),
            IdeTarget::Field { type_key, name } => (nominal(type_key), Some(name)),
            IdeTarget::EnumMember { enum_name, member } => (nominal(enum_name), Some(member)),
            IdeTarget::UnionVariant { union_key, variant } => (nominal(union_key), Some(variant)),
            _ => return target,
        };
        let Some(def) = def else {
            return target;
        };
        let source = member
            .and_then(|member| self.ide_member_sources.get(&(def, member.clone())))
            .or_else(|| self.ide_sources.get(&def));
        match source {
            Some(source) => IdeTarget::Resolved {
                def,
                source: source.clone(),
                target: Box::new(target),
            },
            None => target,
        }
    }
}
