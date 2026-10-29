use super::*;
use dream_sema::module_graph::ModuleGraph;
use dream_syntax::nodes::ModuleDeclNode;
use dream_syntax::token::token_kind::TokenKind;
use dream_text::{line_text::LineText, text_span::TextSpan};

impl<'a> ProgramAccumulator<'a> {
    pub fn module_graph(&self) -> ModuleGraph<'a> {
        let mut programs = indexmap::IndexMap::new();
        for (path, original) in &self.parsed_files {
            let mut program =
                ProgramNode::new(vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
            program.module = original.module.clone();
            program.imports = original.imports.clone();
            programs.insert(path.clone(), program);
        }
        let mut source_paths: Vec<_> = self.file_contents.keys().cloned().collect();
        source_paths.sort();
        for path in source_paths {
            programs.entry(path).or_insert_with(|| {
                ProgramNode::new(vec![], vec![], vec![], vec![], vec![], vec![], vec![])
            });
        }
        macro_rules! distribute {
            ($field:ident, $declarations:ident) => {
                for declaration in &self.$declarations {
                    let path = declaration.file_path.as_deref().unwrap_or("").to_string();
                    programs
                        .entry(path)
                        .or_insert_with(|| {
                            ProgramNode::new(vec![], vec![], vec![], vec![], vec![], vec![], vec![])
                        })
                        .$field
                        .push(declaration.clone());
                }
            };
        }
        distribute!(structs, all_structs);
        distribute!(interfaces, all_interfaces);
        distribute!(functions, all_functions);
        distribute!(enums, all_enums);
        distribute!(extends, all_extends);
        distribute!(globals, all_globals);
        let line = LineText::new(String::new());
        for (path, program) in &mut programs {
            if program.module.is_none()
                && let Some(module) = self.file_modules.get(path)
            {
                program.module = Some(ModuleDeclNode {
                    attributes: vec![],
                    path: SyntaxToken::new(
                        TokenKind::IdentifierToken,
                        TextSpan::new((0, 0), &line),
                        module.to_string(),
                    ),
                });
            }
        }
        let prelude_paths: Vec<_> = programs
            .keys()
            .filter(|path| {
                !self.parsed_files.contains_key(*path) && self.file_contents.contains_key(*path)
            })
            .cloned()
            .collect();
        let mut edges = self.import_edges.clone();
        let mut initialization_edges = self.import_edges.clone();
        for path in programs.keys() {
            let imports = initialization_edges.entry(path.clone()).or_default();
            let dependencies = if let Some(package) = dream_stdlib::package_for_source(path) {
                // All-to-all prelude edges create artificial import cycles: generated library
                // globals can then run before the registry globals they depend on.
                let requested = package
                    .deps
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect();
                dream_stdlib::resolve_packages_to_load(&requested)
                    .into_iter()
                    .flat_map(|dependency| dependency.files)
                    .chain(package.files)
                    .map(|(source, _)| (*source).to_string())
                    .filter(|source| programs.contains_key(source))
                    .collect::<Vec<_>>()
            } else if self.generated_files.contains(path) {
                // Generated imports are already resolved by merge_imports. Adding every loaded
                // package here would reintroduce cycles when output augments an embedded module.
                dream_stdlib::resolve_packages_to_load(&IndexSet::new())
                    .into_iter()
                    .flat_map(|package| package.files)
                    .map(|(source, _)| (*source).to_string())
                    .filter(|source| programs.contains_key(source))
                    .collect()
            } else {
                prelude_paths.clone()
            };
            for prelude in &dependencies {
                if prelude != path && !imports.contains(prelude) {
                    imports.push(prelude.clone());
                }
            }
        }
        for path in programs.keys() {
            let imports = edges.entry(path.clone()).or_default();
            for prelude in &prelude_paths {
                if prelude != path && !imports.contains(prelude) {
                    imports.push(prelude.clone());
                }
            }
        }
        let inputs = programs
            .into_iter()
            .map(|(path, program)| {
                let source = self.file_contents.get(&path).cloned().unwrap_or_default();
                (path, source, program)
            })
            .collect();
        let mut graph = ModuleGraph::new(inputs, &edges);
        graph.set_initialization_dependencies(&initialization_edges);
        graph.generated_files = self.generated_files.clone();
        graph.generator_output_files = self.generator_output_files.clone();
        graph.std_generator_output_files = self.std_generator_output_files.clone();
        graph.generator_access = self.generator_access.clone();
        graph.embedded_assets = self.embedded_assets.clone();
        graph.consumed = self.consumed.clone();
        graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_library_globals_follow_embedded_dependencies() {
        let arena = Bump::new();
        let mut acc = ProgramAccumulator::default();
        let gpu = "<std>/system/gpu/gpu_shader_library.dream";
        for (path, source, parsed) in [
            ("main.dream", "fun main(): void {}", true),
            (
                "generated/drawing.dream",
                "module system.drawing; let dependent: int = 2;",
                true,
            ),
            ("generated/gpu.dream", "module system.gpu;", true),
            (
                "<std>/system/drawing/canvas.dream",
                "module system.drawing;",
                false,
            ),
            (gpu, "module system.gpu; let registry: int = 1;", false),
        ] {
            let mut diagnostics = DiagnosticBag::new(None);
            let tree = Parser::new(Lexer::new(source.into()), &arena, &mut diagnostics)
                .parse()
                .unwrap();
            assert!(!diagnostics.has_errors());
            let program = tree.get_root();
            acc.file_contents.insert(path.into(), source.into());
            if let Some(module) = &program.module {
                acc.file_modules
                    .insert(path.into(), Rc::from(module.path.text.as_str()));
            }
            if parsed {
                acc.parsed_files.insert(path.into(), program.clone());
            }
            if path.starts_with("generated/") {
                acc.generated_files.insert(path.into());
            }
            for mut global in program.globals.clone() {
                global.file_path = Some(path.into());
                acc.all_globals.push(global);
            }
        }
        acc.import_edges
            .insert("main.dream".into(), vec![gpu.into()]);
        let graph = acc.module_graph();
        let names: Vec<_> = graph
            .view()
            .globals
            .iter()
            .map(|global| global.name.text.as_str())
            .collect();
        assert_eq!(names, ["registry", "dependent"]);
    }
}
