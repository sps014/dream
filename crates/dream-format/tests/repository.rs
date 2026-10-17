use std::{fs, path::Path, process::Command};

use dream_diagnostics::DiagnosticBag;
use dream_syntax::{lexer::Lexer, token::token_kind::TokenKind};

fn lexical_content(source: &str) -> Option<Vec<(TokenKind, String)>> {
    let mut diagnostics = DiagnosticBag::new(None);
    let tokens = Lexer::new(source.to_owned()).lex_all(&mut diagnostics);
    if diagnostics.has_errors() {
        return None;
    }
    let mut content = Vec::new();
    for token in tokens {
        for trivia in &token.leading_trivia {
            content.push((trivia.kind, normalize_comment(&trivia.text)));
        }
        content.push((token.kind, token.text));
        for trivia in &token.trailing_trivia {
            content.push((trivia.kind, normalize_comment(&trivia.text)));
        }
    }
    Some(content)
}

fn normalize_comment(text: &str) -> String {
    text.lines().map(str::trim).collect::<Vec<_>>().join("\n")
}

fn verify(source: &str, label: &str) {
    let once = dream_format::format(source);
    let twice = dream_format::format(&once);
    assert_eq!(once, twice, "formatter is not idempotent: {label}");
    if let Some(original) = lexical_content(source) {
        assert!(
            dream_format::try_format(source).is_some(),
            "rejected lexically valid source: {label}"
        );
        let rewritten = lexical_content(&once).expect("formatted source must lex");
        if original != rewritten {
            let mismatch = original
                .iter()
                .zip(&rewritten)
                .position(|(a, b)| a != b)
                .unwrap_or(original.len().min(rewritten.len()));
            panic!(
                "changed tokens or comments: {label}, item {mismatch}: {:?} -> {:?}",
                original.get(mismatch),
                rewritten.get(mismatch)
            );
        }
    } else {
        assert_eq!(once, source, "rewrote invalid lexical input: {label}");
    }
}

#[test]
fn every_tracked_dream_file_preserves_content_and_is_idempotent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "*.dream",
            "*.md",
        ])
        .current_dir(&root)
        .output()
        .expect("git must list repository fixtures");
    assert!(output.status.success());
    let mut files = 0;
    let mut fences = 0;
    for name in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let name = std::str::from_utf8(name).expect("repository paths are UTF-8");
        let path = root.join(name);
        if !path.exists() {
            continue;
        }
        let source = fs::read_to_string(path).expect("repository source must be readable");
        if name.ends_with(".dream") {
            verify(&source, name);
            files += 1;
        } else {
            let mut in_dream = false;
            let mut snippet = String::new();
            let mut indent = 0;
            for (line, text) in source.lines().enumerate() {
                if text.trim() == "```dream" {
                    in_dream = true;
                    indent = text.len() - text.trim_start().len();
                } else if in_dream && text.trim() == "```" {
                    verify(&snippet, &format!("{name}:{line}"));
                    if name.starts_with("docs/") {
                        assert_eq!(
                            snippet,
                            dream_format::format(&snippet),
                            "unformatted Dream fence: {name}:{line}"
                        );
                    }
                    fences += 1;
                    snippet.clear();
                    in_dream = false;
                } else if in_dream {
                    snippet.push_str(text.strip_prefix(&" ".repeat(indent)).unwrap_or(text));
                    snippet.push('\n');
                }
            }
        }
    }
    assert!(
        files > 500,
        "repository audit did not discover the source corpus"
    );
    assert!(
        fences > 500,
        "repository audit did not discover documentation snippets"
    );
    eprintln!("Verified {files} Dream files and {fences} documentation fences");
}
