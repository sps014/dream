use crate::MirFunction;

/// The emitted symbol for a function (or generic instance): the source name, suffixed with the
/// instance's interned type-arg ids so each monomorphization stays distinct.
pub(crate) fn func_symbol(func: &MirFunction) -> String {
    if func.instance.is_empty() {
        func.name.clone()
    } else {
        let args: Vec<String> = func.instance.iter().map(|t| t.0.to_string()).collect();
        format!("{}__{}", func.name, args.join("_"))
    }
}
