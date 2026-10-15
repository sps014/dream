use std::process::Command;

#[test]
fn typed_attribute_decoders_keep_the_declaring_module() {
    let dir = tempfile::tempdir().unwrap();
    for (file, module, value) in [("left", "one", 7), ("right", "two", 19)] {
        std::fs::write(
            dir.path().join(format!("{file}.dream")),
            format!(
                r#"
module {module};
import system.codegen;
public enum Mode {{ First = {value}, Second }}
@attribute(AttributeTarget.Function)
public struct RouteAttribute {{ public mode: Mode; }}
@route(Mode.First)
public fun sample(): void {{}}
public fun check(): int {{
    if GenAttributes.id<RouteAttribute>() != "{module}::RouteAttribute" {{ return -1; }}
    let args = List<GenAttrArg>();
    args.push(GenAttrArg("enum", "Mode.First"));
    let attrs = List<GenAttribute>();
    attrs.push(GenAttribute(GenAttributes.id<RouteAttribute>(), "route", args));
    if GenAttributes.decode<RouteAttribute>(attrs).unwrap().mode == Mode.First {{ return {value}; }}
    return -2;
}}
"#
            ),
        )
        .unwrap();
    }
    std::fs::write(
        dir.path().join("main.dream"),
        r#"
import system.testing;
import left;
import right;
import one.check as left_check;
import two.check as right_check;

fun main(): void {
    Assert.eq(left_check(), 7);
    Assert.eq(right_check(), 19);
}

"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .arg("run")
        .arg(dir.path().join("main.dream"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
