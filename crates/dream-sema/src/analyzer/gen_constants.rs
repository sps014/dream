use super::Analyzer;
use crate::gen_facts::GenConstant;
use dream_syntax::nodes::{ExpressionNode as E, Type};

fn value(kind: &str, value: impl ToString) -> GenConstant {
    GenConstant {
        kind: kind.to_string(),
        value: value.to_string(),
    }
}

impl Analyzer<'_> {
    /// Infers exact scalar constants conservatively without assuming overflow or machine width.
    pub(super) fn generator_constant(&self, e: &E<'_>, ty: &Type) -> Option<GenConstant> {
        let child = |e: &E<'_>| {
            let key = (
                self.current_file
                    .as_ref()
                    .map(|f| f.to_string())
                    .unwrap_or_default(),
                e.position()?.start,
            );
            self.generator_facts
                .as_ref()?
                .expressions
                .get(&key)?
                .constant
                .clone()
        };
        let constant = match e {
            E::Literal(lit) => match lit {
                Type::Integer(t) | Type::Long(t) | Type::ISize(t) => {
                    value("int", dream_syntax::number::parse_int_literal(&t.text)?)
                }
                Type::UInt(t) | Type::ULong(t) | Type::USize(t) | Type::Byte(t) => {
                    value("uint", dream_syntax::number::parse_u64_literal(&t.text)?)
                }
                Type::Boolean(t) => value("bool", &t.text),
                Type::String(t) => value("string", super::hir_emit::string_lit_value(&t.text)),
                Type::Char(t) => value("char", &t.text),
                Type::Float(t) | Type::Double(t) => {
                    let number = dream_syntax::number::parse_float_literal(&t.text)?;
                    let number = if matches!(ty, Type::Float(_)) {
                        (number as f32) as f64
                    } else {
                        number
                    };
                    if !number.is_finite() {
                        return None;
                    }
                    value("float", number)
                }
                _ => return None,
            },
            E::Parenthesized(_, e) => child(e)?,
            E::Unary(op, e) => {
                let operand = child(e)?;
                match (op.text.as_str(), operand.kind.as_str()) {
                    ("-", "int") => value("int", operand.value.parse::<i64>().ok()?.checked_neg()?),
                    ("+", _) => operand,
                    ("!", "bool") => value("bool", operand.value != "true"),
                    _ => return None,
                }
            }
            E::Binary(left, op, right) => {
                let (left, right) = (child(left)?, child(right)?);
                if left.kind != right.kind {
                    return None;
                }
                match left.kind.as_str() {
                    "int" => {
                        let (a, b) = (
                            left.value.parse::<i64>().ok()?,
                            right.value.parse::<i64>().ok()?,
                        );
                        match op.text.as_str() {
                            "+" => value("int", a.checked_add(b)?),
                            "-" => value("int", a.checked_sub(b)?),
                            "*" => value("int", a.checked_mul(b)?),
                            "/" => value("int", a.checked_div(b)?),
                            "%" => value("int", a.checked_rem(b)?),
                            "==" => value("bool", a == b),
                            "!=" => value("bool", a != b),
                            "<" => value("bool", a < b),
                            "<=" => value("bool", a <= b),
                            ">" => value("bool", a > b),
                            ">=" => value("bool", a >= b),
                            _ => return None,
                        }
                    }
                    "bool" => {
                        let (a, b) = (left.value == "true", right.value == "true");
                        value(
                            "bool",
                            match op.text.as_str() {
                                "&&" => a && b,
                                "||" => a || b,
                                "==" => a == b,
                                "!=" => a != b,
                                _ => return None,
                            },
                        )
                    }
                    "string" => match op.text.as_str() {
                        "+" => value("string", left.value + &right.value),
                        "==" => value("bool", left.value == right.value),
                        "!=" => value("bool", left.value != right.value),
                        _ => return None,
                    },
                    _ => return None,
                }
            }
            _ => return None,
        };
        // A conservative fact never guesses overflow mode or a target-dependent machine width.
        if constant.kind == "int" {
            let integer = constant.value.parse::<i64>().ok()?;
            match ty {
                Type::Integer(_) if i32::try_from(integer).is_err() => return None,
                Type::ISize(_) => return None,
                _ => {}
            }
        }
        Some(constant)
    }
}
