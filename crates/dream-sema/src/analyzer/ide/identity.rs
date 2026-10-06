use super::{Analyzer, IdeSource, IdeTarget};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_types::{DefId, DefKind, TyKind, TypeId};

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
        if name.position.end <= name.position.start {
            return;
        }
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
        let IdeTarget::Callee { key, .. } = &target else {
            return target;
        };
        let def = key.0;
        match self.ide_sources.get(&def) {
            Some(source) => IdeTarget::Resolved {
                def,
                source: source.clone(),
                target: Box::new(target),
            },
            None => target,
        }
    }

    pub(super) fn resolve_ide_owner_target(
        &self,
        owner: TypeId,
        member: Option<&str>,
        target: IdeTarget,
    ) -> IdeTarget {
        let def = match self.type_ctx.interner.kind(owner) {
            TyKind::Struct(def, _)
            | TyKind::Union(def, _)
            | TyKind::Interface(def, _)
            | TyKind::Enum(def) => *def,
            _ => return target,
        };
        let source = match member {
            Some(member) => self.ide_member_sources.get(&(def, member.to_string())),
            None => self.ide_sources.get(&def),
        };
        match source {
            Some(source) => IdeTarget::Resolved {
                def,
                source: source.clone(),
                target: Box::new(target),
            },
            None => target,
        }
    }

    /// The un-applied nominal type of `def`; `None` for functions.
    pub(super) fn nominal_type_of(&mut self, def: DefId) -> Option<TypeId> {
        let interner = &mut self.type_ctx.interner;
        Some(match self.type_ctx.defs.get(def).kind {
            DefKind::Struct => interner.struct_ty(def, Vec::new()),
            DefKind::Interface => interner.interface_ty(def, Vec::new()),
            DefKind::Union => interner.union_ty(def, Vec::new()),
            DefKind::Enum => interner.enum_ty(def),
            DefKind::Function => return None,
        })
    }

    pub(super) fn append_ide_member_declarations(&mut self, refs: &mut Vec<super::IdeRef>) {
        let members: Vec<_> = self
            .ide_member_sources
            .iter()
            .map(|((def, name), source)| (*def, name.clone(), source.clone()))
            .collect();
        for (def, name, source) in members {
            let kind = self.type_ctx.defs.get(def).kind;
            let Some(owner) = self.nominal_type_of(def) else {
                continue;
            };
            let target = match kind {
                DefKind::Struct | DefKind::Interface => IdeTarget::Field { owner, name },
                DefKind::Enum => IdeTarget::EnumMember {
                    owner,
                    member: name,
                },
                DefKind::Union => IdeTarget::UnionVariant {
                    owner,
                    variant: name,
                },
                DefKind::Function => continue,
            };
            refs.push(super::IdeRef {
                start: source.start,
                end: source.end,
                file: source.file.clone(),
                target: IdeTarget::Resolved {
                    def,
                    source,
                    target: Box::new(target),
                },
                result: super::TypeSummary::Unknown,
            });
        }
    }

    pub(in crate::analyzer) fn record_ide_member_ref(
        &mut self,
        owner: TypeId,
        member: &SyntaxToken,
        target: IdeTarget,
        result: super::TypeSummary,
    ) {
        let resolved = self.resolve_ide_owner_target(owner, Some(&member.text), target);
        self.record_ide_ref(member.position, resolved, result);
    }
}
