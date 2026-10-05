/// Origin of one generated replacement: the `html {}` / `quote {}` block it was
/// produced from, in the user's own source file.
pub struct GenOrigin {
    pub real_file: String,
    /// Byte offset of the block's opening `{` in `real_file`.
    pub block_start: usize,
}

pub(super) const WRAP_PREFIX: &str = "fun __gen(): void { return ";

pub(super) struct Rewriter<'a, 'ctx> {
    pub(super) arena: &'a bumpalo::Bump,
    pub(super) by_site: &'ctx indexmap::IndexMap<(String, String), String>,
    pub(super) diagnostics: &'ctx mut dream_diagnostics::DiagnosticBag,
    pub(super) changed: &'ctx mut bool,
    pub(super) file: Option<&'ctx str>,
    pub(super) file_contents: &'ctx std::collections::HashMap<String, String>,
}
