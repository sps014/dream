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
            if program.module.is_none() {
                if let Some(module) = self.file_modules.get(path) {
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
        }
        let prelude_paths: Vec<_> = programs
            .keys()
            .filter(|path| {
                !self.parsed_files.contains_key(*path) && self.file_contents.contains_key(*path)
            })
            .cloned()
            .collect();
        let mut edges = self.import_edges.clone();
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
        ModuleGraph::new(inputs, &edges)
    }
}
