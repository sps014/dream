//! Coarse lexical token categories shared by `semantic_tokens`' symbol-aware classifier, so
//! "which `TokenKind`s are keywords / types / operators / literals" is defined exactly once.

use dream::syntax::token::token_kind::TokenKind;

/// Coarse lexical category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexCategory {
    Keyword,
    Type,
    Number,
    String,
    Operator,
}

/// Maps a lexical token kind to its coarse category, or `None` for a token with no fixed category
/// (identifiers - classified contextually by each caller - punctuation, end-of-file, bad tokens).
pub fn lex_category(kind: TokenKind) -> Option<LexCategory> {
    use TokenKind::*;
    Some(match kind {
        DataTypeToken => LexCategory::Type,
        NumberToken => LexCategory::Number,
        StringToken | CharToken => LexCategory::String,
        BooleanToken => LexCategory::Keyword,
        IfToken | ElseToken | ForToken | WhileToken | DoToken | ReturnToken | BreakToken
        | ContinueToken | LetToken | ConstToken | FunToken | StaticToken | ImportToken
        | PublicToken | ExternToken | ClassToken | StructToken | UnmanagedToken | ExtendToken
        | IsToken | InToken | EnumToken | TypeToken | SwitchToken | CaseToken | DefaultToken
        | SealedToken | InterfaceToken | AsyncToken | AwaitToken | InternalToken | ModuleToken
        | AsToken | RefToken | BorrowToken | WeakToken | UnownedToken | LockToken => {
            LexCategory::Keyword
        }
        PlusToken
        | MinusToken
        | SlashToken
        | StarToken
        | BangToken
        | ModulusToken
        | PlusEqualToken
        | MinusEqualToken
        | StarEqualToken
        | SlashEqualToken
        | ModulusEqualToken
        | PlusPlusToken
        | MinusMinusToken
        | EqualEqualToken
        | NotEqualToken
        | AmpersandAmpersandToken
        | PipePipeToken
        | BitWisePipeToken
        | BitWiseAmpersandToken
        | BitWiseXorToken
        | ShiftLeftToken
        | ShiftRightToken
        | QuestionQuestionToken
        | EqualToken
        | GreaterThanEqualToken
        | GreaterThanToken
        | SmallerThanToken
        | SmallerThanEqualToken => LexCategory::Operator,
        _ => return None,
    })
}
