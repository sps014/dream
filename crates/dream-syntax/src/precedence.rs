use crate::token::token_kind::TokenKind;

/// Binding power of prefix operators and of a cast's operand: above every binary operator.
pub const UNARY_PRECEDENCE: i32 = 100;

impl TokenKind {
    pub fn get_binary_precedence(&self) -> i32 {
        match self {
            TokenKind::BitWiseAmpersandToken => 90,
            TokenKind::BitWiseXorToken => 85,
            TokenKind::BitWisePipeToken => 80,

            TokenKind::ModulusToken => 55,

            TokenKind::SlashToken => 50,
            TokenKind::StarToken => 50,

            TokenKind::PlusToken => 40,
            TokenKind::MinusToken => 40,

            TokenKind::ShiftLeftToken => 35,
            TokenKind::ShiftRightToken => 35,

            TokenKind::BangToken => 30,

            TokenKind::GreaterThanEqualToken => 25,
            TokenKind::GreaterThanToken => 25,
            TokenKind::SmallerThanEqualToken => 25,
            TokenKind::SmallerThanToken => 25,
            TokenKind::EqualEqualToken => 25,
            TokenKind::NotEqualToken => 25,
            TokenKind::IsToken => 25,
            // Comparisons/`is` must bind *tighter* than `&&`, which in turn must bind tighter than
            // `||`, so `a > 0 && b > 0` parses as `(a > 0) && (b > 0)` rather than `a > (0 && b) > 0`
            // (the previous ordering here — comparisons at 15, `&&` at 20 — inverted this and made
            // any bare `cond && comparison` a compile error unless parenthesized).
            TokenKind::AmpersandAmpersandToken => 20,
            TokenKind::PipePipeToken => 10,
            TokenKind::QuestionQuestionToken => 8,
            TokenKind::DotDotToken => 5,

            _ => 0,
        }
    }
    /// Unary ops must outrank every binary op so Pratt accepts a leading `-`/`!`/`~`/`+`/`++`/`--`
    /// on the RHS of a comparison or arithmetic op (`t > -2`, `a + -b`). With a lower value the loop
    /// refuses unary when `unary_precedence < parent_precedence` (e.g. after `>` at 25).
    pub fn get_unary_precedence(&self) -> i32 {
        match self {
            TokenKind::PlusToken => UNARY_PRECEDENCE,
            TokenKind::MinusToken => UNARY_PRECEDENCE,
            TokenKind::BangToken => UNARY_PRECEDENCE,
            TokenKind::TildeToken => UNARY_PRECEDENCE,
            TokenKind::PlusPlusToken => UNARY_PRECEDENCE,
            TokenKind::MinusMinusToken => UNARY_PRECEDENCE,
            _ => 0,
        }
    }
}
