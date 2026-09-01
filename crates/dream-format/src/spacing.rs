use dream_syntax::token::token_kind::TokenKind;

/// Tokens that begin a top-level declaration (used to insert the blank line separating decls).
pub(super) fn is_decl_starter(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::FunToken
            | TokenKind::ClassToken
            | TokenKind::StructToken
            | TokenKind::EnumToken
            | TokenKind::InterfaceToken
            | TokenKind::ExtendToken
            | TokenKind::ImportToken
            | TokenKind::ModuleToken
            | TokenKind::LetToken
            | TokenKind::ConstToken
            | TokenKind::PublicToken
            | TokenKind::InternalToken
            | TokenKind::ExternToken
            | TokenKind::StaticToken
            | TokenKind::AsyncToken
            | TokenKind::SealedToken
            | TokenKind::UnmanagedToken
            | TokenKind::RefToken
            | TokenKind::AtToken
            | TokenKind::TypeToken
    )
}

fn is_binary_op(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::PlusToken
            | TokenKind::MinusToken
            | TokenKind::StarToken
            | TokenKind::SlashToken
            | TokenKind::ModulusToken
            | TokenKind::EqualEqualToken
            | TokenKind::NotEqualToken
            | TokenKind::GreaterThanToken
            | TokenKind::GreaterThanEqualToken
            | TokenKind::SmallerThanToken
            | TokenKind::SmallerThanEqualToken
            | TokenKind::AmpersandAmpersandToken
            | TokenKind::PipePipeToken
            | TokenKind::BitWisePipeToken
            | TokenKind::BitWiseAmpersandToken
            | TokenKind::BitWiseXorToken
            | TokenKind::ShiftLeftToken
            | TokenKind::ShiftRightToken
            | TokenKind::QuestionQuestionToken
            | TokenKind::EqualToken
            | TokenKind::PlusEqualToken
            | TokenKind::MinusEqualToken
            | TokenKind::StarEqualToken
            | TokenKind::SlashEqualToken
            | TokenKind::ModulusEqualToken
            | TokenKind::FatArrowToken
            | TokenKind::IsToken
            | TokenKind::AsToken
            | TokenKind::InToken
            | TokenKind::DotDotToken
    )
}

fn is_prefix_unary(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::BangToken
            | TokenKind::TildeToken
            | TokenKind::PlusPlusToken
            | TokenKind::MinusMinusToken
            | TokenKind::AwaitToken
            | TokenKind::RefToken
            | TokenKind::BorrowToken
    )
}

pub(super) fn is_keyword_needing_space(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::IfToken
            | TokenKind::ElseToken
            | TokenKind::ForToken
            | TokenKind::WhileToken
            | TokenKind::DoToken
            | TokenKind::LockToken
            | TokenKind::DeferToken
            | TokenKind::ReturnToken
            | TokenKind::BreakToken
            | TokenKind::ContinueToken
            | TokenKind::LetToken
            | TokenKind::ConstToken
            | TokenKind::FunToken
            | TokenKind::AsyncToken
            | TokenKind::AwaitToken
            | TokenKind::StaticToken
            | TokenKind::ImportToken
            | TokenKind::AsToken
            | TokenKind::ModuleToken
            | TokenKind::PublicToken
            | TokenKind::InternalToken
            | TokenKind::ExternToken
            | TokenKind::ClassToken
            | TokenKind::StructToken
            | TokenKind::UnmanagedToken
            | TokenKind::SealedToken
            | TokenKind::WeakToken
            | TokenKind::UnownedToken
            | TokenKind::UniqueToken
            | TokenKind::RefToken
            | TokenKind::BorrowToken
            | TokenKind::InterfaceToken
            | TokenKind::ExtendToken
            | TokenKind::IsToken
            | TokenKind::InToken
            | TokenKind::EnumToken
            | TokenKind::TypeToken
            | TokenKind::SwitchToken
            | TokenKind::CaseToken
            | TokenKind::DefaultToken
            | TokenKind::DataTypeToken
    )
}

fn is_word_like(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::IdentifierToken
            | TokenKind::DataTypeToken
            | TokenKind::BooleanToken
            | TokenKind::NumberToken
    ) || is_keyword_needing_space(kind)
}

/// A token that can end an operand — when one precedes `-`/`+`, those are binary operators;
/// otherwise (`(`, `,`, `=`, `return`, …) they are signed-literal prefixes.
fn ends_operand(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::CloseParenthesisToken | TokenKind::CloseBracketToken
    ) || is_word_like(kind)
}

/// Decides whether a single space belongs between `prev` and `cur`.
///
/// `before_prev` is the token before `prev`, needed only for the signed-literal rule.
pub(super) fn needs_space(before_prev: Option<TokenKind>, prev: TokenKind, cur: TokenKind) -> bool {
    // Never space before closers / separators / postfix.
    if matches!(
        cur,
        TokenKind::CommaToken
            | TokenKind::SemicolonToken
            | TokenKind::CloseParenthesisToken
            | TokenKind::CloseBracketToken
            | TokenKind::CurlyCloseBracketToken
            | TokenKind::DotToken
            | TokenKind::PlusPlusToken
            | TokenKind::MinusMinusToken
    ) {
        return false;
    }

    // Never space after openers / `@` / `.` / `...`.
    if matches!(
        prev,
        TokenKind::OpenParenthesisToken
            | TokenKind::OpenBracketToken
            | TokenKind::DotToken
            | TokenKind::DotDotDotToken
            | TokenKind::AtToken
    ) {
        return false;
    }

    // `foo(` / `foo[` — no space between a callee/subscriptee and its opening delimiter.
    if matches!(
        cur,
        TokenKind::OpenParenthesisToken | TokenKind::OpenBracketToken
    ) && matches!(
        prev,
        TokenKind::IdentifierToken
            | TokenKind::CloseParenthesisToken
            | TokenKind::CloseBracketToken
            | TokenKind::GreaterThanToken
            | TokenKind::ShiftRightToken
            | TokenKind::DataTypeToken
            | TokenKind::BooleanToken
            | TokenKind::StringToken
            | TokenKind::InterpolatedStringToken
            | TokenKind::CharToken
            | TokenKind::NumberToken
            | TokenKind::CurlyCloseBracketToken
    ) {
        return false;
    }

    // Signed literals: `-1`, `+x` right after an operand position are unary and bind tight.
    if matches!(prev, TokenKind::MinusToken | TokenKind::PlusToken)
        && !before_prev.map(ends_operand).unwrap_or(false)
    {
        return false;
    }

    // Colons hug what precedes them (annotations `x: int`, case labels `case 1:`, struct
    // fields); only a ternary's colon gets surrounding space.
    if cur == TokenKind::ColonToken {
        return prev == TokenKind::QuestionMarkToken;
    }
    if prev == TokenKind::ColonToken {
        return true;
    }

    // Prefix unary: `!x` `~x` `++x` `--x`; space only after word-like unaries (`await x`).
    if is_prefix_unary(prev) {
        return matches!(
            prev,
            TokenKind::AwaitToken | TokenKind::RefToken | TokenKind::BorrowToken
        );
    }

    // Space after `,` `:` `;` (`;` usually already newline'd).
    if matches!(prev, TokenKind::CommaToken | TokenKind::SemicolonToken) {
        return true;
    }

    // Ternary `cond ? a : b`.
    if cur == TokenKind::QuestionMarkToken && ends_operand(prev) {
        return true;
    }

    // Attribute groups separate from what follows: `@inline public fun ...`,
    // `@a @b static ...` — but `(T)x` casts stay tight.
    if prev == TokenKind::CloseParenthesisToken
        && (cur == TokenKind::AtToken || is_decl_starter(cur))
    {
        return true;
    }

    // Consecutive attributes: `@a @b`.
    if cur == TokenKind::AtToken
        && (is_word_like(prev) || prev == TokenKind::CurlyCloseBracketToken)
    {
        return true;
    }

    // Space around binary operators.
    if is_binary_op(prev) || is_binary_op(cur) {
        return true;
    }

    // `}` followed by anything but separators stays attached: `} else`, `} while (`,
    // `};`-less decl starts, `})`, etc.
    if prev == TokenKind::CurlyCloseBracketToken {
        return !matches!(
            cur,
            TokenKind::CommaToken
                | TokenKind::SemicolonToken
                | TokenKind::CloseParenthesisToken
                | TokenKind::CloseBracketToken
                | TokenKind::DotToken
        );
    }

    // Keywords that introduce a following expression / type / name.
    if is_keyword_needing_space(prev) {
        return true;
    }

    // Adjacent identifiers / literals / keywords → space (`return x`, `pub fun`).
    if is_word_like(prev) && is_word_like(cur) {
        return true;
    }

    // `) {` / `] {` / ident `{`
    if cur == TokenKind::CurlyOpenBracketToken {
        return true;
    }

    false
}
