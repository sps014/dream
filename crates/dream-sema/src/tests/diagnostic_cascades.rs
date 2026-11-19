//! One mistake must produce one diagnostic: these lock in that follow-on errors (undefined names,
//! false non-exhaustive switches, repeated reports from desugared copies) stay quiet.

use super::harness::*;

fn messages(code: &str) -> Vec<String> {
    analyze_code(code)
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn failed_destructuring_still_declares_its_names() {
    let errors = messages(
        "fun main(): void {
            let (first, second) = 5;
            let total = first + second;
        }",
    );
    assert_eq!(errors, ["cannot destructure non-tuple type int"]);
}

#[test]
fn compound_assignment_reports_target_errors_once() {
    let errors = messages(
        "fun main(): void {
            let values = [1];
            values[missing] += 1;
        }",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("missing"), "{errors:?}");
}

#[test]
fn wrong_pattern_arity_is_not_also_non_exhaustive() {
    let errors = messages(
        "enum Shape { Circle(int), Dot }
        fun area(shape: Shape): int {
            return switch (shape) {
                Circle() => 1,
                Dot => 2,
            };
        }
        fun main(): void {}",
    );
    assert_eq!(
        errors,
        ["Variant 'Shape.Circle' has 1 field(s), but the pattern binds 0"]
    );
}

#[test]
fn foreign_unit_variant_is_not_a_catch_all_binding() {
    let errors = messages(
        "enum Shape { Circle(int), Dot }
        enum Other { Thing(int), Nothing }
        fun area(shape: Shape): int {
            return switch (shape) {
                Nothing => 0,
                Circle(r) => r,
                Dot => 2,
            };
        }
        fun main(): void {}",
    );
    assert_eq!(
        errors,
        ["pattern 'Nothing' is a variant of 'Other', not of the matched enum 'Shape'"]
    );
}

#[test]
fn void_operand_is_reported_once() {
    let errors = messages(
        "fun nothing(): void {}
        fun main(): void {
            let joined = \"a\" + nothing();
        }",
    );
    assert_eq!(
        errors,
        ["this expression returns 'void' and has no value to use"]
    );
}

#[test]
fn failed_class_constraint_is_reported_once() {
    let errors = messages(
        "interface Ordered<T> {
            fun compare(other: T): int;
        }
        class Sorted<K: Ordered<K>> {
            public constructor() {}
            public fun put(key: K): void {
                let c = key.compare(key);
            }
        }
        class Key {
            public constructor() {}
        }
        fun main(): void {
            let sorted = Sorted<Key>();
            sorted.put(Key());
        }",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("does not satisfy"), "{errors:?}");
}

#[test]
fn mismatched_lambda_parameter_is_reported_once() {
    let errors = messages(
        "fun apply(f: fun(int): int): int {
            return f(1);
        }
        fun main(): void {
            let r = apply((x: string) => 0);
        }",
    );
    // Without the prelude, `fun` values have no backend shape; that report is unrelated.
    let errors: Vec<_> = errors
        .into_iter()
        .filter(|e| !e.contains("failed to produce code for the compiler backend"))
        .collect();
    assert_eq!(
        errors,
        ["lambda parameter type mismatch: expected int, got string"]
    );
}
