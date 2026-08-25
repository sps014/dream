//! Workspace-wide symbol search: scans `.dream` files under the project root (found by walking
//! up from an open document to a `dream.toml`/`.git` marker) and collects top-level
//! declarations. Results are cached briefly and invalidated by file-watch events; files already
//! open in the editor are excluded (their in-editor version is authoritative).

use bumpalo::Bump;
use dream::diagnostics::DiagnosticBag;
use dream::syntax::lexer::Lexer;
use dream::syntax::parser::Parser;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::index::SymKind;

/// Directories never descended into during the workspace scan.
const SKIPPED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    ".dream",
    "build",
    "dist",
    ".venv",
];

/// How long a scan result is reused before the next query re-walks the tree.
const SCAN_TTL: Duration = Duration::from_secs(3);

/// Safety caps so a pathological tree can't stall the server.
const MAX_FILES: usize = 2_000;
const MAX_DEPTH: usize = 12;

/// One top-level declaration found on disk.
#[derive(Debug, Clone)]
pub struct WsSymbol {
    pub name: String,
    pub kind: SymKind,
    pub path: String,
    pub start: usize,
    pub end: usize,
}

/// The cached result of one full scan.
#[derive(Debug)]
pub struct WorkspaceIndex {
    pub symbols: Vec<WsSymbol>,
    pub root: PathBuf,
    scanned_at: Instant,
}

impl WorkspaceIndex {
    pub fn new(root: PathBuf, symbols: Vec<WsSymbol>) -> Self {
        Self {
            symbols,
            root,
            scanned_at: Instant::now(),
        }
    }

    pub fn is_fresh(&self, root: &Path) -> bool {
        self.scanned_at.elapsed() < SCAN_TTL && self.root == root
    }
}

/// Finds the project root for a set of open documents: the nearest ancestor of any open file
/// that contains `dream.toml` or `.git`, falling back to the file's own directory.
pub fn project_root(open_paths: &[String]) -> Option<PathBuf> {
    let mut fallback = None;
    for p in open_paths {
        let Some(start) = Path::new(p).parent() else {
            continue;
        };
        let mut dir = start.to_path_buf();
        loop {
            if dir.join("dream.toml").is_file() || dir.join(".git").exists() {
                return Some(dir);
            }
            if fallback.is_none() {
                fallback = Some(dir.clone());
            }
            match dir.parent() {
                Some(parent) => dir = parent.to_path_buf(),
                None => break,
            }
        }
    }
    fallback
}

/// Walks `root`, parsing every `.dream` file and collecting its top-level declarations.
/// Deterministic order (sorted walk + per-file source order).
pub fn scan(root: &Path) -> Vec<WsSymbol> {
    let mut out = Vec::new();
    let mut files = Vec::new();
    collect_dream_files(root, 0, MAX_FILES, &mut files);
    files.sort();
    for path in files {
        if let Ok(text) = std::fs::read_to_string(&path) {
            out.extend(file_symbols(&text, &path.to_string_lossy()));
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
    out
}

fn collect_dream_files(dir: &Path, depth: usize, budget: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= budget {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_ref()) && !name.starts_with('.') {
                collect_dream_files(&path, depth + 1, budget, out);
            }
        } else if name.ends_with(".dream") {
            out.push(path);
            if out.len() >= budget {
                return;
            }
        }
    }
}

/// Top-level declarations of one parsed file (functions, types, enums, globals). Parse errors
/// are ignored — whatever top-level declarations did parse are still findable.
pub fn file_symbols(text: &str, path: &str) -> Vec<WsSymbol> {
    let arena = Bump::new();
    let mut scratch = DiagnosticBag::new(None);
    let lexer = Lexer::new(text.to_string());
    let mut parser = Parser::new(lexer, &arena, &mut scratch);
    let Ok(ast) = parser.parse() else {
        return Vec::new();
    };
    let program = ast.get_root();

    let mut out = Vec::new();
    let mut push = |name: &str, kind: SymKind, token: &dream::syntax::token::syntax_token::SyntaxToken| {
        if !name.is_empty() {
            out.push(WsSymbol {
                name: name.to_string(),
                kind,
                path: path.to_string(),
                start: token.position.start,
                end: token.position.end,
            });
        }
    };

    for s in &program.structs {
        push(&s.name.text, SymKind::Class, &s.name);
    }
    for i in &program.interfaces {
        push(&i.name.text, SymKind::Interface, &i.name);
    }
    for e in &program.enums {
        push(&e.name.text, SymKind::Enum, &e.name);
    }
    for f in &program.functions {
        push(&f.name.text, SymKind::Function, &f.name);
    }
    for g in &program.globals {
        push(&g.name.text, SymKind::Variable, &g.name);
    }
    out
}
