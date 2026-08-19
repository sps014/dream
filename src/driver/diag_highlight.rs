//! Syntax-highlight a single source line for rustc-style diagnostic excerpts.
//! Uses the same logos [`TokenKind`] grammar as the compiler lexer.

use dream_syntax::token::token_kind::TokenKind;
use logos::Logos;

const RESET: &str = "\x1b[0m";
const KEYWORD: &str = "\x1b[1;35m";
const TYPE: &str = "\x1b[33m";
const STRING: &str = "\x1b[32m";
const NUMBER: &str = "\x1b[36m";
const COMMENT: &str = "\x1b[90m";
const ATTR: &str = "\x1b[34m";

/// Colorize `line` when `color` is set; otherwise return it unchanged.
pub fn highlight_dream_line(line: &str, color: bool) -> String {
    if !color {
        return line.to_string();
    }
    let mut out = String::new();
    let mut lexer = TokenKind::lexer(line);
    let mut last = 0usize;
    while let Some(kind) = lexer.next() {
        let span = lexer.span();
        if span.start > last {
            out.push_str(&line[last..span.start]);
        }
        let piece = &line[span.start..span.end];
        let kind = kind.unwrap_or(TokenKind::BadToken);
        match token_ansi(kind) {
            Some(code) => {
                out.push_str(code);
                out.push_str(piece);
                out.push_str(RESET);
            }
            None => out.push_str(piece),
        }
        last = span.end;
    }
    if last < line.len() {
        out.push_str(&line[last..]);
    }
    out
}

fn token_ansi(kind: TokenKind) -> Option<&'static str> {
    use TokenKind::*;
    match kind {
        IfToken | ElseToken | ForToken | WhileToken | DoToken | LockToken | ReturnToken
        | BreakToken | ContinueToken | LetToken | ConstToken | FunToken | AsyncToken
        | AwaitToken | StaticToken | ImportToken | AsToken | ModuleToken | PublicToken
        | InternalToken | ExternToken | ClassToken | StructToken | UnmanagedToken | SealedToken
        | WeakToken | UnownedToken | RefToken | BorrowToken | InterfaceToken | ExtendToken
        | IsToken | InToken | EnumToken | TypeToken | SwitchToken | CaseToken | DefaultToken => {
            Some(KEYWORD)
        }
        DataTypeToken => Some(TYPE),
        StringToken | InterpolatedStringToken | CharToken => Some(STRING),
        NumberToken | BooleanToken => Some(NUMBER),
        LineCommentToken | BlockCommentToken => Some(COMMENT),
        AtToken => Some(ATTR),
        _ => None,
    }
}
