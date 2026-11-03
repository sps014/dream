use super::*;
use dream_syntax::nodes::Visibility;

pub(super) fn collect(module: &mut SourceModule, files: &[SourceFile<'_>]) {
    for &index in &module.files {
        let program = &files[index].program;
        let value = |name: &str| ModuleValueExport {
            name: name.to_string(),
            file: index,
        };
        let export = |kind, name: &str| ModuleExport {
            kind,
            name: name.to_string(),
            file: index,
        };
        for global in &program.globals {
            if global.visibility == Visibility::Public {
                module.globals.push(value(&global.name.text));
            }
        }
        for extension in &program.extends {
            // An implementation clause affects conformance even without public methods.
            if !extension.implements.is_empty()
                || extension
                    .methods
                    .iter()
                    .any(|m| m.visibility == Visibility::Public)
            {
                module.extensions.push(value(&extension.target.text));
            }
        }
        for function in &program.functions {
            if function.visibility == Visibility::Public {
                module
                    .exports
                    .push(export(DefKind::Function, &function.name.text));
            }
        }
        for declaration in &program.structs {
            if declaration.visibility == Visibility::Public {
                module
                    .exports
                    .push(export(DefKind::Struct, &declaration.name.text));
            }
        }
        for declaration in &program.interfaces {
            if declaration.visibility == Visibility::Public {
                module
                    .exports
                    .push(export(DefKind::Interface, &declaration.name.text));
            }
        }
        for declaration in &program.enums {
            if declaration.visibility == Visibility::Public {
                let kind = if declaration.is_data_enum() {
                    DefKind::Union
                } else {
                    DefKind::Enum
                };
                module.exports.push(export(kind, &declaration.name.text));
            }
        }
    }
}
