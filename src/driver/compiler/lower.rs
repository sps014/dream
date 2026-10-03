use super::*;

impl Compiler {
    pub(super) fn lower_hir(
        &self,
        hir: &dream_hir::Hir,
        interner: &dream_types::TypeInterner,
        dump: &mut dream_mir::passes::MirDump,
    ) -> dream_mir::Mir {
        let mir = dream_mir::lower::lower_program(hir, interner);
        dump.module(dream_mir::passes::STAGE_LOWER, &mir, interner);
        mir
    }
}
