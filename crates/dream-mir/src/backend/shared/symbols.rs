use crate::MirFunction;

/// Symbols are resolved while the definition table is available, then carried through lowering.
pub(crate) fn func_symbol(func: &MirFunction) -> String {
    func.symbol.clone()
}
