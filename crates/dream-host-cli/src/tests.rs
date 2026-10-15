use crate::parse::parse;
use serde_json::{Value, json};

fn contains(actual: &Value, expected: &Value) {
    if let Value::Object(fields) = expected {
        for (name, value) in fields {
            contains(&actual[name], value);
        }
    } else {
        assert_eq!(actual, expected);
    }
}
#[test]
fn shared_host_conformance() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../../tests/fixtures/cli/conformance.json")).unwrap();
    for case in cases {
        let actual: Value = serde_json::from_str(&parse(&case["request"].to_string())).unwrap();
        contains(&actual, &case["expected"]);
    }
}
#[test]
fn definition_combinations_never_panic() {
    for kind in ["flag", "string", "int", "positional"] {
        for required in [false, true] {
            for multiple in [false, true] {
                for default in [Value::Null, json!(""), json!("1")] {
                    for choices in [json!([]), json!(["1", "2"])] {
                        let command = json!({"name":"probe", "args":[{"name":"value", "kind":kind, "required":required,"multiple":multiple,"default_value":default,"choices":choices}]});
                        for args in [
                            json!([]),
                            json!(["--help"]),
                            json!(["--value", "1"]),
                            json!(["1"]),
                        ] {
                            let result: Value = serde_json::from_str(&parse(
                                &json!({"command":command,"args":args}).to_string(),
                            ))
                            .unwrap();
                            assert!(matches!(
                                result["kind"].as_str(),
                                Some("values" | "help" | "error")
                            ));
                        }
                    }
                }
            }
        }
    }
}
