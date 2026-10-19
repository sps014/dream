//! Select individual host exports while preserving initializer evaluation order.

use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKind, Statement};
use oxc_span::GetSpan;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Error;

pub(super) fn select(text: &str, factory: &str, live: &BTreeSet<&str>) -> Result<String, Error> {
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, text, oxc_span::SourceType::mjs()).parse();
    if let Some(error) = parsed.diagnostics.first() {
        return Err(Error::other(format!("{factory}: {error}")));
    }
    let program = allocator.alloc(parsed.program);
    let semantic = oxc_semantic::SemanticBuilder::new()
        .with_build_nodes(true)
        .build(program)
        .semantic;
    let receivers: Vec<_> = semantic
        .nodes()
        .iter()
        .filter_map(|node| {
            if let oxc_ast::AstKind::ThisExpression(value) = node.kind() {
                Some(value.span)
            } else {
                None
            }
        })
        .collect();
    let functions: BTreeMap<_, _> = program
        .body
        .iter()
        .filter_map(|statement| {
            let Statement::FunctionDeclaration(function) = statement else {
                return None;
            };
            Some((function.id.as_ref()?.name.as_str(), function))
        })
        .collect();
    let mut visited = BTreeSet::new();
    let mut pending = vec![factory];
    let mut edits = Vec::new();
    while let Some(name) = pending.pop() {
        if !visited.insert(name) {
            continue;
        }
        let Some(function) = functions.get(name) else {
            continue;
        };
        let Some(body) = &function.body else { continue };
        for statement in &body.statements {
            let Statement::ReturnStatement(ret) = statement else {
                continue;
            };
            let Some(Expression::ObjectExpression(object)) = &ret.argument else {
                continue;
            };
            // A receiver or opaque spread can dispatch through any sibling export.
            if receivers
                .iter()
                .any(|span| span.start >= object.span.start && span.end <= object.span.end)
            {
                return Ok(text.to_string());
            }
            for property in &object.properties {
                let ObjectPropertyKind::ObjectProperty(property) = property else {
                    if let ObjectPropertyKind::SpreadProperty(spread) = property
                        && let Expression::CallExpression(call) = &spread.argument
                        && let Expression::Identifier(callee) = &call.callee
                        && functions.contains_key(callee.name.as_str())
                    {
                        pending.push(callee.name.as_str());
                        continue;
                    }
                    return Ok(text.to_string());
                };
                let Some(name) = property.key.static_name() else {
                    continue;
                };
                if property.computed
                    || property.kind != PropertyKind::Init
                    || name == "__proto__"
                    || live.contains(name.as_ref())
                {
                    continue;
                }
                // Dropping the export must not discard side effects in its initializer. The
                // minifier can remove pure values and their dependencies from the empty spread.
                let replacement = if property.method {
                    "...{}".to_string()
                } else {
                    let span = property.value.span();
                    format!(
                        "...(({}), {{}})",
                        &text[span.start as usize..span.end as usize]
                    )
                };
                edits.push((property.span, replacement));
            }
        }
    }
    edits.sort_by_key(|(span, _)| span.start);
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    for (span, replacement) in edits {
        result.push_str(&text[cursor..span.start as usize]);
        result.push_str(&replacement);
        cursor = span.end as usize;
    }
    result.push_str(&text[cursor..]);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_exports_preserves_effects_and_dynamic_properties() {
        let source = r#"
            function makeHost() {
                return {
                    unusedBefore: effect("before"),
                    used: effect("used"),
                    unusedAfter: effect("after"),
                    unusedMethod() { return "unneeded body"; },
                    get observed() { return effect("getter"); },
                    [dynamicKey]: effect("dynamic"),
                    __proto__: prototype,
                };
            }
        "#;
        let selected = select(source, "makeHost", &BTreeSet::from(["used"])).unwrap();
        assert!(!selected.contains("unusedBefore:"));
        assert!(!selected.contains("unneeded body"));
        for text in [
            "effect(\"before\")",
            "effect(\"used\")",
            "effect(\"after\")",
            "get observed()",
            "[dynamicKey]",
            "__proto__: prototype",
        ] {
            assert!(selected.contains(text), "{text}");
        }
        assert!(selected.find("effect(\"before\")") < selected.find("effect(\"used\")"));
        assert!(selected.find("effect(\"used\")") < selected.find("effect(\"after\")"));
        assert!(super::super::minify_js_source(&selected).is_ok());
    }

    #[test]
    fn opaque_dispatch_keeps_the_original_factory() {
        for source in [
            "function makeHost() { return { used() { return this.hidden(); }, hidden() { return 1; } }; }",
            "function makeHost() { return { hidden: () => 1, ...unknown }; }",
        ] {
            assert_eq!(
                select(source, "makeHost", &BTreeSet::from(["used"])).unwrap(),
                source
            );
        }
    }

    #[test]
    fn nested_factories_keep_initializer_order_when_minified() {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let source = r#"
            const effects = [];
            const effect = name => { effects.push(name); return () => name; };
            const dynamicKey = "dynamic";
            function makeChildHost() {
                return { used: effect("child"), unused: effect("child-unused") };
            }
            function makeHost() {
                return {
                    before: effect("before"), used: effect("used"), after: effect("after"),
                    ...makeChildHost(),
                    get observed() { effects.push("getter"); return 42; },
                    [dynamicKey]: 7,
                };
            }
        "#;
        let mut selected = select(source, "makeHost", &BTreeSet::from(["used"])).unwrap();
        selected.push_str(r#"
            const host = Object.assign({}, makeHost());
            if (effects.join() !== "before,used,after,child,child-unused,getter") throw Error(effects);
            if (host.used() !== "child" || host.observed !== 42 || host.dynamic !== 7) throw Error("lost exports");
            if ("before" in host || "after" in host || "unused" in host) throw Error("unused exports");
        "#);
        let minified = super::super::minify_js_source(&selected).unwrap();
        let output = std::process::Command::new("node")
            .args(["--input-type=module", "--eval", &minified])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
