//! One parsed copy of each dependency-package file per thread, like the prelude cache. Generator
//! executables, test runners and the language server compile the same installed packages many
//! times in one process; their sources only change when `dreamer` installs a new revision, which
//! changes the text this cache is keyed by.

use super::*;
use std::cell::RefCell;

struct ParseCache {
    arena: &'static Bump,
    /// `(path, cfg key, text)` → the parsed file. Files with any parse diagnostic are not stored,
    /// so a hit never has diagnostics to replay.
    files: HashMap<(String, String, String), ProgramNode<'static>>,
}

thread_local! {
    /// The arena is leaked for the thread's lifetime so cached nodes stay valid across compiles.
    static CACHE: RefCell<ParseCache> = RefCell::new(ParseCache {
        arena: Box::leak(Box::new(Bump::new())),
        files: HashMap::new(),
    });
}

pub(super) fn parse_library_file<'a>(
    path: &str,
    text: &str,
    cfg: dream_syntax::conditional::CfgSymbols,
    diagnostics: &mut DiagnosticBag,
) -> Result<ProgramNode<'a>, Error> {
    let key = (path.to_string(), cfg.key(), text.to_string());
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(hit) = cache.files.get(&key) {
            return Ok(hit.clone());
        }
        let mut file_diagnostics = DiagnosticBag::new(Some(path.to_string()));
        let parsed = Parser::new(
            Lexer::new(text.to_string()),
            cache.arena,
            &mut file_diagnostics,
        )
        .with_cfg(cfg)
        .parse();
        diagnostics.extend(&file_diagnostics);
        let program = parsed?.get_root().clone();
        if file_diagnostics.diagnostics.is_empty() {
            cache.files.insert(key, program.clone());
        }
        Ok(program)
    })
}
