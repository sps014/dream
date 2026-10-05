use super::super::context::GeneratorContext;
#[cfg(feature = "native")]
use super::collection_discovery::collect_all_collections;
#[cfg(feature = "native")]
use super::diagnostics::json_error_file_path;
#[cfg(feature = "native")]
use super::diagnostics::lookup_json_error_span;
#[cfg(feature = "native")]
use super::harness::run_dream_json_generator;
#[cfg(feature = "native")]
use super::snapshot::build_snapshot;
use crate::driver::source_loader::ProgramAccumulator;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::EnumDeclarationNode;
use std::collections::HashSet;

/// Expands every `@json` type into synthesized `extend` source through `emit_file`.
pub fn expand_from_acc(
    ctx: &mut GeneratorContext,
    acc: &ProgramAccumulator<'_>,
    structs: &[StructDeclarationNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
    diagnostics: &mut DiagnosticBag,
) {
    let mut json_names: HashSet<String> = structs
        .iter()
        .filter(|s| s.attributes.iter().any(|a| a.name.text == "json"))
        .map(|s| s.name.text.clone())
        .collect();
    json_names.extend(
        enums
            .iter()
            .filter(|e| e.attributes.iter().any(|a| a.name.text == "json"))
            .map(|e| e.name.text.clone()),
    );
    if json_names.is_empty() {
        #[cfg(feature = "native")]
        {
            let mut jsonable: HashSet<String> =
                structs.iter().map(|s| s.name.text.clone()).collect();
            jsonable.extend(
                enums
                    .iter()
                    .filter(|e| e.is_data_enum())
                    .map(|e| e.name.text.clone()),
            );
            let collections = collect_all_collections(acc, &jsonable);
            if collections.is_empty() {
                return;
            }
        }
        #[cfg(not(feature = "native"))]
        {
            let _ = acc;
            return;
        }
    }

    #[cfg(not(feature = "native"))]
    {
        let _ = (ctx, structs, enums);
        diagnostics.report_error(
            "@json derive requires the native compiler feature".to_string(),
            None,
        );
    }

    #[cfg(feature = "native")]
    {
        let mut jsonable: HashSet<String> = structs.iter().map(|s| s.name.text.clone()).collect();
        jsonable.extend(
            enums
                .iter()
                .filter(|e| e.is_data_enum())
                .map(|e| e.name.text.clone()),
        );

        let snapshot = build_snapshot(acc, structs, enums, &json_names, &jsonable);
        match run_dream_json_generator(&ctx.toolchain_config, &snapshot) {
            Ok(source) => {
                if !source.is_empty() {
                    ctx.emit_file("<json-derive>", source);
                }
            }
            Err(err) => {
                let span = lookup_json_error_span(&err, structs, enums);
                if let Some(path) = json_error_file_path(&err, structs, enums) {
                    diagnostics.file_path = Some(path);
                }
                diagnostics.report_error(err.message, span);
            }
        }
    }
}
